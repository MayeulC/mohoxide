//! Audio: SupCom's XACT banks played through the OS output (`cpal`).
//!
//! Presentation only. The sim never reads anything from here: world sounds
//! leave the sim as snapshot data (`Snapshot::sounds` / `ambient`), the
//! user layer's sound globals (`PlaySound`, `PlayVoice`, ...) only send
//! commands, and nothing comes back into the sim. See docs/architecture/audio.md.
//!
//! [`Audio`] is a cheap, cloneable handle; the audio thread
//! ([`output::run`]) owns the XACT runtime ([`engine::AudioEngine`]) and
//! the device.

pub mod engine;
pub mod output;
pub mod xact;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub use engine::{Ambient, Command, CueStatus, Listener, Source, WorldSound};

#[derive(Clone, Debug, Default)]
pub struct AudioOptions {
    /// Open the OS output device (else mix silently against the clock).
    pub device: bool,
    /// Write the first seconds of the mix to this WAV (`--audio-dump`).
    pub dump: Option<(std::path::PathBuf, f32)>,
}

struct Inner {
    /// False for [`Audio::disabled`]: nothing listens, cues finish at once.
    enabled: bool,
    /// The bank source was sent ([`Audio::use_vfs`] sends it once).
    has_source: std::sync::atomic::AtomicBool,
    tx: Mutex<Sender<Command>>,
    status: Arc<engine::StatusBoard>,
    next_id: AtomicU64,
    volumes: Mutex<BTreeMap<String, f32>>,
    rate: u32,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Command::Shutdown);
        }
        if let Some(t) = self.thread.lock().ok().and_then(|mut t| t.take()) {
            let _ = t.join();
        }
    }
}

static INSTALLED: std::sync::OnceLock<Audio> = std::sync::OnceLock::new();

/// The game's audio (one per process; clones share it).
#[derive(Clone)]
pub struct Audio(Arc<Inner>);

impl std::fmt::Debug for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Audio")
    }
}

impl Audio {
    /// Starts the audio thread (and the device, if asked and present).
    pub fn start(options: AudioOptions) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let (rate_tx, rate_rx) = std::sync::mpsc::channel();
        let status = Arc::new(engine::StatusBoard::default());
        let thread_status = status.clone();
        let dump = options.dump.map(|(path, seconds)| output::Dump { path, seconds });
        let thread = std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || {
                crate::engine::tracy::name_current();
                output::run(rx, thread_status, options.device, dump, rate_tx)
            })
            .expect("spawn the audio thread");
        let rate = rate_rx.recv().unwrap_or(48000);
        Self(Arc::new(Inner {
            enabled: true,
            has_source: Default::default(),
            tx: Mutex::new(tx),
            status,
            next_id: AtomicU64::new(1),
            volumes: Mutex::new(BTreeMap::new()),
            rate,
            thread: Mutex::new(Some(thread)),
        }))
    }

    /// No audio at all (headless runs, tests): no thread; cues report done
    /// at once, so scripts waiting on them (movies, `WaitFor`) carry on.
    pub fn disabled() -> Self {
        let (tx, _) = std::sync::mpsc::channel();
        Self(Arc::new(Inner {
            enabled: false,
            has_source: Default::default(),
            tx: Mutex::new(tx),
            status: Arc::default(),
            next_id: AtomicU64::new(1),
            volumes: Mutex::new(BTreeMap::new()),
            rate: 48000,
            thread: Mutex::new(None),
        }))
    }

    pub fn is_enabled(&self) -> bool {
        self.0.enabled
    }

    /// Makes this the process's audio ([`installed`](Self::installed)); the
    /// first call wins.
    pub fn install(self) {
        let _ = INSTALLED.set(self);
    }

    /// The process's audio, if one was installed (windowed runs).
    pub fn installed() -> Option<Self> {
        INSTALLED.get().cloned()
    }

    /// Banks come from this file system (`/sounds`); only the first call
    /// counts (the front end's and the game's interfaces share the banks).
    pub fn use_vfs(&self, vfs: Arc<std::sync::RwLock<crate::engine::vfs::Vfs>>) {
        if !self.0.has_source.swap(true, Ordering::Relaxed) {
            self.send(Command::SetSource(Source::Vfs(vfs)));
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.0.rate
    }

    pub fn send(&self, cmd: Command) {
        if let Ok(tx) = self.0.tx.lock() {
            let _ = tx.send(cmd);
        }
    }

    /// Starts (or prepares) a cue for a script; its id is the Lua handle.
    pub fn play(&self, bank: &str, cue: &str, start: bool, voice: bool, duck: bool) -> u64 {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        if !self.0.enabled {
            self.0.status.insert(id, CueStatus::Done);
            return id;
        }
        self.0.status.insert(id, CueStatus::Pending);
        self.send(Command::Play { id, bank: bank.into(), cue: cue.into(), start, voice, duck });
        id
    }

    /// `None` once forgotten (its handle was collected).
    pub fn status(&self, id: u64) -> Option<CueStatus> {
        self.0.status.get(id)
    }

    /// The script let go of a handle: its cue plays on, untracked.
    pub fn forget(&self, id: u64) {
        self.0.status.forget(id);
    }

    /// Handles still tracked (tests: handles are forgotten when collected).
    pub fn tracked(&self) -> usize {
        self.0.status.len()
    }

    pub fn set_volume(&self, category: &str, volume: f32) {
        if let Ok(mut v) = self.0.volumes.lock() {
            v.insert(category.to_ascii_lowercase(), volume);
        }
        self.send(Command::SetVolume { category: category.into(), volume });
    }

    /// The volume a script set for `category` (1 by default).
    pub fn volume(&self, category: &str) -> f32 {
        self.0.volumes.lock().ok().and_then(|v| v.get(&category.to_ascii_lowercase()).copied()).unwrap_or(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_without_a_device_finishes_cues_and_dumps_a_wav() {
        let Some(dir) = xact::tests::sounds_dir() else { return };
        let out = std::env::temp_dir().join(format!("rts-audio-test-{}.wav", std::process::id()));
        let audio = Audio::start(AudioOptions { device: false, dump: Some((out.clone(), 0.5)) });
        audio.send(Command::SetSource(Source::Dir(dir)));
        let id = audio.play("Interface", "UI_Menu_MouseDown_Sml", true, false, false);
        let t = std::time::Instant::now();
        while audio.status(id) != Some(CueStatus::Done) && t.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(audio.status(id), Some(CueStatus::Done));
        while !out.exists() && t.elapsed().as_secs() < 5 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        drop(audio);
        let wav = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(&wav[..4], b"RIFF");
        let samples: Vec<f32> = wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(rms > 0.001, "dump rms {rms}");
    }

    #[test]
    fn volumes_read_back() {
        let audio = Audio::start(AudioOptions::default());
        assert_eq!(audio.volume("Music"), 1.0);
        audio.set_volume("Music", 0.25);
        assert_eq!(audio.volume("music"), 0.25);
    }
}
