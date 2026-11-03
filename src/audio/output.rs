//! The audio thread: owns the [`AudioEngine`] and the output device.
//!
//! It drains commands, then renders ahead into a lock-free ring buffer
//! (`rtrb`) that the device callback only copies out of: no locks, no
//! allocation and no file access in the callback (streamed banks are read
//! by this thread, when it mixes). With no output device (headless
//! machines, CI) it renders against the wall clock instead and throws the
//! samples away, so cues still finish and `WaitFor` on them returns.
//! `--audio-dump` writes the first seconds of the mix to a WAV either way.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::engine::{AudioEngine, BLOCK, Command, StatusBoard};

/// Mixed audio kept ahead of the device: the latency between a command and
/// its sound (on top of the device's own buffer).
const AHEAD: Duration = Duration::from_millis(30);

pub struct Dump {
    pub path: std::path::PathBuf,
    pub seconds: f32,
}

struct DumpWriter {
    path: std::path::PathBuf,
    rate: u32,
    frames_left: usize,
    samples: Vec<i16>,
}

impl DumpWriter {
    fn push(&mut self, stereo: &[f32]) {
        let frames = (stereo.len() / 2).min(self.frames_left);
        self.samples.extend(stereo[..frames * 2].iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
        self.frames_left -= frames;
        if self.frames_left == 0 {
            self.finish();
        }
    }

    fn finish(&mut self) {
        if self.samples.is_empty() && self.frames_left == 0 {
            return;
        }
        match write_wav(&self.path, self.rate, &self.samples) {
            Ok(()) => tracing::info!(path = %self.path.display(), seconds = self.samples.len() as f32 / 2.0 / self.rate as f32, rms = rms_i16(&self.samples), "audio dump written"),
            Err(e) => tracing::error!("audio dump {}: {e}", self.path.display()),
        }
        self.samples = Vec::new();
        self.frames_left = 0;
    }
}

fn rms_i16(s: &[i16]) -> f32 {
    (s.iter().map(|&x| (x as f32 / 32768.0).powi(2)).sum::<f32>() / s.len().max(1) as f32).sqrt()
}

/// A 16-bit stereo PCM WAV.
pub fn write_wav(path: &std::path::Path, rate: u32, interleaved: &[i16]) -> std::io::Result<()> {
    let data_len = (interleaved.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in interleaved {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b)
}

/// An open output device: the stream (kept alive on this thread) and the
/// ring it reads.
struct Device {
    _stream: cpal::Stream,
    producer: rtrb::Producer<f32>,
    rate: u32,
}

fn open_device() -> Option<Device> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let supported = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("audio: no output config: {e}");
            return None;
        }
    };
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let rate = config.sample_rate;
    let channels = usize::from(config.channels).max(1);
    // A second of stereo: far more than `AHEAD` needs.
    let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(rate as usize * 2);
    let err = |e| tracing::warn!("audio device error: {e}");
    macro_rules! build {
        ($t:ty, $conv:expr) => {
            device.build_output_stream::<$t, _, _>(
                config.clone(),
                move |data: &mut [$t], _| {
                    for frame in data.chunks_mut(channels) {
                        let l = consumer.pop().unwrap_or(0.0);
                        let r = consumer.pop().unwrap_or(0.0);
                        if channels == 1 {
                            frame[0] = $conv((l + r) * 0.5);
                        } else {
                            frame[0] = $conv(l);
                            frame[1] = $conv(r);
                            for s in &mut frame[2..] {
                                *s = $conv(0.0);
                            }
                        }
                    }
                },
                err,
                None,
            )
        };
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => build!(i16, |s: f32| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
        cpal::SampleFormat::I32 => build!(i32, |s: f32| (s.clamp(-1.0, 1.0) as f64 * 2_147_483_647.0) as i32),
        cpal::SampleFormat::U16 => build!(u16, |s: f32| ((s.clamp(-1.0, 1.0) + 1.0) * 32767.5) as u16),
        other => {
            tracing::warn!("audio: unsupported device sample format {other:?}");
            return None;
        }
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("audio: can't open the output stream: {e}");
            return None;
        }
    };
    if let Err(e) = stream.play() {
        tracing::warn!("audio: can't start the output stream: {e}");
        return None;
    }
    tracing::info!(rate, channels, format = ?format, "audio output opened");
    Some(Device { _stream: stream, producer, rate })
}

/// Runs the audio thread until the command channel closes.
pub fn run(rx: Receiver<Command>, status: Arc<StatusBoard>, use_device: bool, dump: Option<Dump>, rate_tx: std::sync::mpsc::Sender<u32>) {
    let mut device = if use_device { open_device() } else { None };
    if device.is_none() {
        tracing::info!("audio: no output device; mixing without one");
    }
    let rate = device.as_ref().map_or(48000, |d| d.rate);
    let _ = rate_tx.send(rate);
    let mut engine = AudioEngine::new(rate, status);
    let mut dump = dump.map(|d| DumpWriter { path: d.path, rate, frames_left: (d.seconds.max(0.0) * rate as f32) as usize, samples: Vec::new() });
    let mut block = vec![0.0f32; BLOCK * 2];
    let ahead = (AHEAD.as_secs_f32() * rate as f32) as usize * 2;
    let started = Instant::now();
    let mut rendered: u64 = 0;
    'run: loop {
        match rx.recv_timeout(Duration::from_millis(2)) {
            Ok(cmd) => {
                if matches!(cmd, Command::Shutdown) {
                    break 'run;
                }
                engine.handle(cmd);
                while let Ok(cmd) = rx.try_recv() {
                    if matches!(cmd, Command::Shutdown) {
                        break 'run;
                    }
                    engine.handle(cmd);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let _s = crate::prof_span!("audio mix");
        match device.as_mut() {
            Some(d) => {
                let capacity = d.producer.buffer().capacity();
                while capacity - d.producer.slots() < ahead && d.producer.slots() >= block.len() {
                    engine.render(&mut block);
                    if let Ok(mut chunk) = d.producer.write_chunk_uninit(block.len()) {
                        let (a, b) = chunk.as_mut_slices();
                        for (dst, src) in a.iter_mut().chain(b.iter_mut()).zip(block.iter()) {
                            dst.write(*src);
                        }
                        // Safety: every slot of the chunk was written above.
                        unsafe { chunk.commit_all() };
                    }
                    if let Some(w) = dump.as_mut().filter(|w| w.frames_left > 0) {
                        w.push(&block);
                    }
                }
            }
            None => {
                // No device: keep time with the wall clock.
                let due = (started.elapsed().as_secs_f64() * f64::from(rate)) as u64;
                while rendered + BLOCK as u64 <= due {
                    engine.render(&mut block);
                    rendered += BLOCK as u64;
                    if let Some(w) = dump.as_mut().filter(|w| w.frames_left > 0) {
                        w.push(&block);
                    }
                }
            }
        }
    }
    if let Some(w) = dump.as_mut() {
        w.finish();
    }
}
