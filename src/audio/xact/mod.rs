//! XACT (Xbox Audio Cross-platform Tool) data, as SupCom ships it: sound
//! banks (`.xsb`), wave banks (`.xwb`) and the global settings (`.xgs`).
//! Parsers only; playback is `audio::engine`.

pub mod xgs;
pub mod xsb;
pub mod xwb;

pub(crate) fn u16_at(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| format!("truncated at {o:#x}"))
}

pub(crate) fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("truncated at {o:#x}"))
}

pub(crate) fn f32_at(d: &[u8], o: usize) -> Result<f32, String> {
    u32_at(d, o).map(f32::from_bits)
}

/// The NUL-terminated string at `o`.
pub(crate) fn cstr(d: &[u8], o: usize) -> Result<String, String> {
    let rest = d.get(o..).ok_or_else(|| format!("string offset {o:#x} out of range"))?;
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
}

/// XACT's volume byte in dB: 0 is -96 dB (silence), 180 is 0 dB, 255 is
/// +6 dB. The curve is the one Mednafen's XACT work fitted (FAudio uses it).
pub fn volume_db(v: u8) -> f32 {
    ((-96.0 - 67.738_521_233_404_7) / (1.0 + (f64::from(v) / 80.174_860_029_796_3).powf(0.432_254_984_608_615)) + 67.738_521_233_404_7) as f32
}

/// dB to a linear gain (-96 dB and below is silence).
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -96.0 { 0.0 } else { 10f32.powf(db / 20.0) }
}

/// Builders for small synthetic banks (tests without the game's files).
#[cfg(test)]
pub(crate) mod test_banks {
    /// A version-43 wave bank named "Test" with 16-bit PCM entries
    /// (channels, rate, interleaved samples).
    pub fn wave_bank(entries: &[(u16, u32, Vec<i16>)]) -> Vec<u8> {
        let bank_at = 0x34usize;
        let meta_at = bank_at + 96;
        let data_at = meta_at + 24 * entries.len();
        let mut wave_data = Vec::new();
        let mut meta = Vec::new();
        for (ch, rate, samples) in entries {
            let offset = wave_data.len() as u32;
            for s in samples {
                wave_data.extend_from_slice(&s.to_le_bytes());
            }
            let frames = (samples.len() / *ch as usize) as u32;
            let format = (u32::from(*ch) << 2) | (rate << 5) | ((u32::from(*ch) * 2) << 23) | (1 << 31);
            for v in [frames << 4, format, offset, samples.len() as u32 * 2, 0, 0] {
                meta.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut d = Vec::new();
        d.extend_from_slice(b"WBND");
        d.extend_from_slice(&43u32.to_le_bytes());
        d.extend_from_slice(&42u32.to_le_bytes());
        for (o, l) in [(bank_at, 96), (meta_at, meta.len()), (0, 0), (0, 0), (data_at, wave_data.len())] {
            d.extend_from_slice(&(o as u32).to_le_bytes());
            d.extend_from_slice(&(l as u32).to_le_bytes());
        }
        d.resize(bank_at, 0);
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        let mut name = [0u8; 64];
        name[..4].copy_from_slice(b"Test");
        d.extend_from_slice(&name);
        for v in [24u32, 64, 4, 0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.resize(meta_at, 0);
        d.extend_from_slice(&meta);
        d.extend_from_slice(&wave_data);
        d
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The game's `/sounds` folder (the sibling install). Fails the test
    /// without it; `None` only when skipping (`RTS_SKIP_DATA_TESTS=1`).
    pub fn sounds_dir() -> Option<std::path::PathBuf> {
        const DIR: &str = "../Supreme Commander Forged Alliance/sounds";
        crate::test_data::require("the game's sounds", &[DIR]).then(|| std::path::PathBuf::from(DIR))
    }

    #[test]
    fn volume_bytes_map_to_decibels() {
        assert!((volume_db(0) + 96.0).abs() < 0.01);
        assert!(volume_db(180).abs() < 0.1);
        assert!((volume_db(255) - 6.0).abs() < 0.2);
        assert!(volume_db(60) < -15.0);
        assert_eq!(db_to_gain(-96.0), 0.0);
        assert!((db_to_gain(-6.0206) - 0.5).abs() < 1e-3);
    }

    /// Every wave in the shipped banks is PCM (what the decoder supports).
    #[test]
    fn shipped_wave_banks_are_pcm() {
        let Some(dir) = sounds_dir() else { return };
        let mut waves = 0;
        for entry in walkdir::WalkDir::new(&dir).into_iter().flatten() {
            if !entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("xwb")) {
                continue;
            }
            let file = std::fs::File::open(entry.path()).unwrap();
            let map = unsafe { memmap2::Mmap::map(&file) }.unwrap();
            let bank = xwb::WaveBank::parse(std::sync::Arc::new(map)).unwrap_or_else(|e| panic!("{}: {e}", entry.path().display()));
            for e in &bank.entries {
                assert_eq!(e.format, xwb::WaveFormat::Pcm, "{}", entry.path().display());
                assert!(e.bits16 && (1..=2).contains(&e.channels) && [24000, 32000, 44100].contains(&e.sample_rate));
            }
            waves += bank.entries.len();
        }
        assert!(waves > 3000, "{waves}");
    }
}
