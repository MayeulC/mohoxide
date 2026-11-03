//! XACT wave banks (`.xwb`, magic `WBND`): the audio data the sound banks'
//! cues play.
//!
//! SupCom's banks are XACT tool version 43 / header version 42 (2007).
//! Every wave in the shipped banks (3043 of them) is 16-bit PCM, mono or
//! stereo, at 24 / 32 / 44.1 kHz, so PCM (8 and 16 bit) is all that's
//! decoded; ADPCM / XMA / WMA entries parse but play as silence (logged once).
//!
//! Layout: `WBND`, version, header version, then five (offset, length)
//! segments: bank data, entry metadata, seek tables, entry names, wave data.
//! The bank data holds flags (`1` = streaming: the big music / voice banks,
//! read from disk as they play), the entry count, the name and the entry
//! layout. Each 24-byte entry: flags + duration (28 bits, frames), a packed
//! format (tag 2 bits, channels 3, rate 18, block align 8, 16-bit flag 1),
//! the play region (offset into the wave data segment, length in bytes) and
//! the loop region (frames).

use std::sync::Arc;

/// Bytes of a bank file: memory-mapped from disk (folder mounts: the
/// game's `/sounds`), or read whole (a bank inside an archive).
pub type BankBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveFormat {
    Pcm,
    Xma,
    Adpcm,
    Wma,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveEntry {
    pub format: WaveFormat,
    pub channels: u16,
    pub sample_rate: u32,
    /// 16-bit samples (PCM); 8-bit unsigned otherwise.
    pub bits16: bool,
    pub block_align: u16,
    /// Absolute byte offset of the play region in the file.
    pub offset: u32,
    pub length: u32,
    /// Loop region in frames (0 length = the whole wave).
    pub loop_start: u32,
    pub loop_length: u32,
    /// Duration in frames, as the bank records it.
    pub duration: u32,
}

impl WaveEntry {
    pub fn frame_bytes(&self) -> u32 {
        u32::from(self.channels) * if self.bits16 { 2 } else { 1 }
    }

    /// Frames in the play region.
    pub fn frames(&self) -> u32 {
        match self.format {
            WaveFormat::Pcm => self.length / self.frame_bytes().max(1),
            _ => self.duration,
        }
    }
}

pub struct WaveBank {
    pub name: String,
    pub streaming: bool,
    pub entries: Vec<WaveEntry>,
    data: BankBytes,
}

impl std::fmt::Debug for WaveBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaveBank").field("name", &self.name).field("streaming", &self.streaming).field("entries", &self.entries.len()).finish()
    }
}

const FLAG_STREAMING: u32 = 0x1;
const FLAG_COMPACT: u32 = 0x2_0000;

use super::u32_at;

fn unpack_format(v: u32) -> (WaveFormat, u16, u32, u16, bool) {
    let format = match v & 3 {
        0 => WaveFormat::Pcm,
        1 => WaveFormat::Xma,
        2 => WaveFormat::Adpcm,
        _ => WaveFormat::Wma,
    };
    let channels = ((v >> 2) & 7) as u16;
    let rate = (v >> 5) & 0x3_FFFF;
    let align = ((v >> 23) & 0xFF) as u16;
    let bits16 = v >> 31 == 1;
    (format, channels, rate, align, bits16)
}

impl WaveBank {
    pub fn parse(data: BankBytes) -> Result<Self, String> {
        let d: &[u8] = (*data).as_ref();
        if d.get(0..4) != Some(b"WBND") {
            return Err("not a wave bank (no WBND)".into());
        }
        let segment = |i: usize| -> Result<(usize, usize), String> { Ok((u32_at(d, 12 + 8 * i)? as usize, u32_at(d, 16 + 8 * i)? as usize)) };
        let (bank, _) = segment(0)?;
        let (meta, meta_len) = segment(1)?;
        let (waves, waves_len) = segment(4)?;
        let flags = u32_at(d, bank)?;
        let count = u32_at(d, bank + 4)? as usize;
        let name_bytes = d.get(bank + 8..bank + 72).ok_or("truncated bank name")?;
        let name = String::from_utf8_lossy(&name_bytes[..name_bytes.iter().position(|&b| b == 0).unwrap_or(64)]).into_owned();
        let element = u32_at(d, bank + 72)? as usize;
        let alignment = u32_at(d, bank + 80)?;
        let compact_format = u32_at(d, bank + 84)?;
        let mut entries = Vec::with_capacity(count);
        if flags & FLAG_COMPACT != 0 {
            // Compact: one u32 per entry (offset in alignment units: 21
            // bits), every entry in the bank's format; a length runs to the
            // next entry's offset.
            let (format, channels, sample_rate, block_align, bits16) = unpack_format(compact_format);
            let offsets: Vec<u32> = (0..count).map(|i| u32_at(d, meta + 4 * i).map(|v| (v & 0x1F_FFFF) * alignment)).collect::<Result<_, _>>()?;
            for (i, &off) in offsets.iter().enumerate() {
                let end = offsets.get(i + 1).copied().unwrap_or(waves_len as u32);
                entries.push(WaveEntry { format, channels, sample_rate, bits16, block_align, offset: waves as u32 + off, length: end.saturating_sub(off), loop_start: 0, loop_length: 0, duration: 0 });
            }
        } else {
            if element < 24 && count > 0 {
                return Err(format!("unsupported entry size {element}"));
            }
            if count * element > meta_len {
                return Err("entry table overruns its segment".into());
            }
            for i in 0..count {
                let o = meta + i * element;
                let flags_duration = u32_at(d, o)?;
                let (format, channels, sample_rate, block_align, bits16) = unpack_format(u32_at(d, o + 4)?);
                let offset = waves as u32 + u32_at(d, o + 8)?;
                let length = u32_at(d, o + 12)?;
                if offset as usize + length as usize > d.len() {
                    return Err(format!("entry {i} runs past the end of the file"));
                }
                entries.push(WaveEntry {
                    format,
                    channels: channels.max(1),
                    sample_rate,
                    bits16,
                    block_align,
                    offset,
                    length,
                    loop_start: u32_at(d, o + 16)?,
                    loop_length: u32_at(d, o + 20)?,
                    duration: flags_duration >> 4,
                });
            }
        }
        Ok(Self { name, streaming: flags & FLAG_STREAMING != 0, entries, data })
    }

    /// The raw play region of `entry`.
    pub fn bytes(&self, entry: &WaveEntry) -> &[u8] {
        let d: &[u8] = (*self.data).as_ref();
        &d[entry.offset as usize..(entry.offset + entry.length) as usize]
    }

    /// Decodes a whole entry to interleaved f32 (tests and tools; playback
    /// reads samples in place, see [`sample`]).
    pub fn decode(&self, index: usize) -> Option<Vec<f32>> {
        let e = self.entries.get(index)?;
        if e.format != WaveFormat::Pcm {
            return None;
        }
        let b = self.bytes(e);
        Some(if e.bits16 { b.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0).collect() } else { b.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect() })
    }
}

/// Sample `channel` of `frame` from a PCM play region, as -1..1. Out of
/// range reads are silence.
#[inline]
pub fn sample(bytes: &[u8], entry: &WaveEntry, frame: u32, channel: u16) -> f32 {
    let ch = channel.min(entry.channels - 1) as usize;
    if entry.bits16 {
        let i = (frame as usize * entry.channels as usize + ch) * 2;
        match bytes.get(i..i + 2) {
            Some(s) => i16::from_le_bytes([s[0], s[1]]) as f32 * (1.0 / 32768.0),
            None => 0.0,
        }
    } else {
        let i = frame as usize * entry.channels as usize + ch;
        bytes.get(i).map_or(0.0, |&s| (s as f32 - 128.0) * (1.0 / 128.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_the_packed_format() {
        // Music.xwb's first entry: 16-bit stereo PCM at 44.1 kHz.
        let (f, ch, rate, align, bits16) = unpack_format(0x8215_8888);
        assert_eq!((f, ch, rate, align, bits16), (WaveFormat::Pcm, 2, 44100, 4, true));
    }

    #[test]
    fn parses_a_built_bank_and_reads_samples() {
        let bank = super::super::test_banks::wave_bank(&[(1, 32000, vec![0, 16384, -16384, 32767])]);
        let wb = WaveBank::parse(Arc::new(bank)).unwrap();
        assert_eq!(wb.name, "Test");
        let e = wb.entries[0];
        assert_eq!((e.channels, e.sample_rate, e.frames()), (1, 32000, 4));
        assert_eq!(sample(wb.bytes(&e), &e, 1, 0), 0.5);
        assert_eq!(wb.decode(0).unwrap()[2], -0.5);
        assert_eq!(sample(wb.bytes(&e), &e, 9, 0), 0.0);
    }
}
