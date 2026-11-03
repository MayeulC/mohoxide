//! XACT sound banks (`.xsb`, magic `SDBK`): cue names -> sounds -> tracks
//! -> play-wave events over the wave banks.
//!
//! Version 43 (SupCom's), reverse-engineered against the shipped banks (all
//! 163 parse to the byte; layouts below). Header: `SDBK`, content version
//! (43), tool version, CRC, FILETIME, platform, simple / complex cue counts,
//! wave bank count (u8), sound count, cue name table length, then offsets:
//! simple cues, complex cues, cue names, (unknown), variations,
//! transitions, wave bank names (64 bytes each), cue hash, cue name index
//! (u32 name offset + u16 hash link per cue: simple cues first), sounds;
//! then the bank's own name (64 bytes).
//!
//! - Simple cue (5 bytes): flags, sound offset.
//! - Complex cue (15): flags (`4` = plays a sound, else a variation table:
//!   none in SupCom), sound offset, transition offset, instance limit, fade
//!   in / out (ms), instance behaviour (`>> 3`, as categories).
//! - Sound: flags (`1` complex, `2` sound RPCs, `4` track RPCs, `0x10` DSP),
//!   category (u16), volume byte, pitch (i16 cents), priority, entry length
//!   (u16); complex: track count; RPC block (u16 length incl. itself: sound
//!   RPC codes, then per track); DSP block (u16 length); then per track a
//!   volume byte + event list offset (complex) or the wave (u16 track, u8
//!   wave bank) for a simple sound.
//! - Event list: count (always 1 in SupCom), then per event: info (type in
//!   the low 5 bits, timestamp ms in the next 16), random offset (u16 ms),
//!   an unknown byte, flags, then by type: 1 play wave (u16 track, u8 bank,
//!   loop count (255 = forever), u16 position, u16 angle); 3 play wave with
//!   track variation (loop, position, angle, u16 entry count, u16 variation
//!   type, 4 unknown, entries of u16 track + u8 bank + weight min / max);
//!   4 play wave with effect variation (as 1, then pitch min / max i16
//!   cents, volume byte min / max, variation flags: `0x80` pitch, `0x40`
//!   volume); 6 both (loop, position, angle, the effect variation, then the
//!   track variation).

use std::collections::HashMap;

use super::{cstr, u16_at, u32_at, volume_db};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveRef {
    /// Index into the sound bank's wave bank names.
    pub bank: u8,
    pub index: u16,
    pub weight: (u8, u8),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayEvent {
    pub timestamp_ms: u16,
    pub random_offset_ms: u16,
    /// 255 = loops until stopped.
    pub loop_count: u8,
    pub flags: u8,
    /// One wave, or the variation's choices.
    pub waves: Vec<WaveRef>,
    /// Low nibble: 0 ordered, 1 ordered from random, 2 random, 3 random
    /// without an immediate repeat, 4 shuffle; `0x40`: a new choice on
    /// every loop.
    pub variation: u16,
    /// Cents.
    pub pitch_variation: Option<(i16, i16)>,
    /// dB.
    pub volume_variation: Option<(f32, f32)>,
}

impl PlayEvent {
    pub fn new_wave_per_loop(&self) -> bool {
        self.waves.len() > 1 && self.variation & 0x40 != 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub volume_db: f32,
    /// RPC offsets into the global settings.
    pub rpcs: Vec<u32>,
    pub event: Option<PlayEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub category: u16,
    pub volume_db: f32,
    pub pitch_cents: i16,
    pub priority: u8,
    pub rpcs: Vec<u32>,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub name: String,
    /// Index into [`SoundBank::sounds`].
    pub sound: Option<usize>,
    /// 255 = no limit.
    pub instance_limit: u8,
    pub fade_in_ms: u16,
    pub fade_out_ms: u16,
    pub behavior: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundBank {
    pub name: String,
    pub wave_banks: Vec<String>,
    pub sounds: Vec<Sound>,
    pub cues: Vec<Cue>,
    by_name: HashMap<String, usize>,
}

fn read_event(d: &[u8], o: usize) -> Result<Option<PlayEvent>, String> {
    let count = *d.get(o).ok_or("truncated event list")?;
    if count == 0 {
        return Ok(None);
    }
    let info = u32_at(d, o + 1)?;
    let kind = info & 0x1F;
    let mut e = PlayEvent {
        timestamp_ms: ((info >> 5) & 0xFFFF) as u16,
        random_offset_ms: u16_at(d, o + 5)?,
        loop_count: 0,
        flags: *d.get(o + 8).ok_or("truncated event")?,
        waves: Vec::new(),
        variation: 0,
        pitch_variation: None,
        volume_variation: None,
    };
    let mut p = o + 9;
    let byte = |p: usize| d.get(p).copied().ok_or_else(|| format!("truncated event at {p:#x}"));
    let effect = |e: &mut PlayEvent, p: &mut usize| -> Result<(), String> {
        let (pmin, pmax) = (u16_at(d, *p)? as i16, u16_at(d, *p + 2)? as i16);
        let (vmin, vmax, flags) = (byte(*p + 4)?, byte(*p + 5)?, byte(*p + 6)?);
        if flags & 0x80 != 0 {
            e.pitch_variation = Some((pmin.min(pmax), pmin.max(pmax)));
        }
        if flags & 0x40 != 0 {
            let (a, b) = (volume_db(vmin), volume_db(vmax));
            e.volume_variation = Some((a.min(b), a.max(b)));
        }
        *p += 7;
        Ok(())
    };
    let variation = |e: &mut PlayEvent, p: &mut usize| -> Result<(), String> {
        let n = u16_at(d, *p)? as usize;
        e.variation = u16_at(d, *p + 2)?;
        *p += 8;
        for _ in 0..n {
            e.waves.push(WaveRef { index: u16_at(d, *p)?, bank: byte(*p + 2)?, weight: (byte(*p + 3)?, byte(*p + 4)?) });
            *p += 5;
        }
        Ok(())
    };
    match kind {
        1 | 4 => {
            e.waves.push(WaveRef { index: u16_at(d, p)?, bank: byte(p + 2)?, weight: (0, 255) });
            e.loop_count = byte(p + 3)?;
            p += 8;
            if kind == 4 {
                effect(&mut e, &mut p)?;
            }
        }
        3 | 6 => {
            e.loop_count = byte(p)?;
            p += 5;
            if kind == 6 {
                effect(&mut e, &mut p)?;
            }
            variation(&mut e, &mut p)?;
        }
        // Other events (stop, pitch / volume ramps, markers) don't occur in
        // SupCom's banks.
        _ => return Ok(None),
    }
    Ok(Some(e))
}

fn read_sound(d: &[u8], o: usize) -> Result<Sound, String> {
    let flags = *d.get(o).ok_or("truncated sound")?;
    let mut s = Sound {
        category: u16_at(d, o + 1)?,
        volume_db: volume_db(*d.get(o + 3).ok_or("truncated sound")?),
        pitch_cents: u16_at(d, o + 4)? as i16,
        priority: *d.get(o + 6).ok_or("truncated sound")?,
        rpcs: Vec::new(),
        tracks: Vec::new(),
    };
    let mut p = o + 9;
    let complex = flags & 1 != 0;
    let track_count = if complex {
        p += 1;
        d[p - 1] as usize
    } else {
        1
    };
    let mut track_rpcs = vec![Vec::new(); track_count];
    if flags & 0x0E != 0 {
        let len = u16_at(d, p)? as usize;
        let end = p + len;
        let mut q = p + 2;
        let codes = |q: &mut usize| -> Result<Vec<u32>, String> {
            let n = *d.get(*q).ok_or("truncated rpc list")? as usize;
            let v = (0..n).map(|k| u32_at(d, *q + 1 + 4 * k)).collect::<Result<Vec<_>, _>>()?;
            *q += 1 + 4 * n;
            Ok(v)
        };
        if flags & 0x02 != 0 && q < end {
            s.rpcs = codes(&mut q)?;
        }
        if flags & 0x04 != 0 {
            for t in track_rpcs.iter_mut() {
                if q < end {
                    *t = codes(&mut q)?;
                }
            }
        }
        p = end;
    }
    if flags & 0x10 != 0 {
        p += u16_at(d, p)? as usize;
    }
    if complex {
        for rpcs in track_rpcs {
            let volume = volume_db(*d.get(p).ok_or("truncated track")?);
            let events = u32_at(d, p + 1)? as usize;
            p += 5;
            s.tracks.push(Track { volume_db: volume, rpcs, event: read_event(d, events)? });
        }
    } else {
        let wave = WaveRef { index: u16_at(d, p)?, bank: *d.get(p + 2).ok_or("truncated sound")?, weight: (0, 255) };
        let event = PlayEvent { timestamp_ms: 0, random_offset_ms: 0, loop_count: 0, flags: 0, waves: vec![wave], variation: 0, pitch_variation: None, volume_variation: None };
        s.tracks.push(Track { volume_db: 0.0, rpcs: track_rpcs.pop().unwrap_or_default(), event: Some(event) });
    }
    Ok(s)
}

impl SoundBank {
    pub fn parse(d: &[u8]) -> Result<Self, String> {
        if d.get(0..4) != Some(b"SDBK") {
            return Err("not a sound bank (no SDBK)".into());
        }
        let simple = u16_at(d, 0x13)? as usize;
        let complex = u16_at(d, 0x15)? as usize;
        let wave_bank_count = *d.get(0x1B).ok_or("truncated header")? as usize;
        let sound_count = u16_at(d, 0x1C)? as usize;
        let off = |o: usize| u32_at(d, o).map(|v| v as usize);
        let (simple_off, complex_off, wave_names, name_index, sound_off) = (off(0x22)?, off(0x26)?, off(0x3A)?, off(0x42)?, off(0x46)?);
        let mut bank = SoundBank { name: cstr(d, 0x4A)?, ..Default::default() };
        for i in 0..wave_bank_count {
            bank.wave_banks.push(cstr(d, wave_names + 64 * i)?);
        }
        // Sounds, keyed by offset for the cues.
        let mut at = HashMap::with_capacity(sound_count);
        let mut o = sound_off;
        for i in 0..sound_count {
            let s = read_sound(d, o).map_err(|e| format!("sound {i} at {o:#x}: {e}"))?;
            at.insert(o, i);
            bank.sounds.push(s);
            o += u16_at(d, o + 7)? as usize;
        }
        let name = |i: usize| -> Result<String, String> {
            if name_index == 0xFFFF_FFFF {
                return Ok(String::new());
            }
            cstr(d, u32_at(d, name_index + 6 * i)? as usize)
        };
        for i in 0..simple {
            let o = simple_off + 5 * i;
            let sound = off(o + 1)?;
            bank.cues.push(Cue { name: name(i)?, sound: at.get(&sound).copied(), instance_limit: 255, fade_in_ms: 0, fade_out_ms: 0, behavior: 0 });
        }
        for i in 0..complex {
            let o = complex_off + 15 * i;
            let flags = *d.get(o).ok_or("truncated cue")?;
            let sound = off(o + 1)?;
            bank.cues.push(Cue {
                name: name(simple + i)?,
                sound: if flags & 4 != 0 { at.get(&sound).copied() } else { None },
                instance_limit: *d.get(o + 9).ok_or("truncated cue")?,
                fade_in_ms: u16_at(d, o + 10)?,
                fade_out_ms: u16_at(d, o + 12)?,
                behavior: *d.get(o + 14).ok_or("truncated cue")? >> 3,
            });
        }
        bank.by_name = bank.cues.iter().enumerate().map(|(i, c)| (c.name.to_ascii_lowercase(), i)).collect();
        Ok(bank)
    }

    /// A cue by name (case-insensitive, as FAF's scripts aren't always
    /// consistent).
    pub fn cue(&self, name: &str) -> Option<usize> {
        if let Some(i) = self.cues.iter().position(|c| c.name == name) {
            return Some(i);
        }
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank(name: &str) -> Option<SoundBank> {
        let dir = super::super::tests::sounds_dir()?;
        Some(SoundBank::parse(&std::fs::read(dir.join(name)).ok()?).unwrap())
    }

    #[test]
    fn parses_the_music_bank() {
        let Some(b) = bank("Music.xsb") else { return };
        assert_eq!(b.name, "Music");
        assert_eq!(b.wave_banks, ["Music"]);
        let names: Vec<&str> = b.cues.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Main_Menu", "Base_Building", "Battle"]);
        let menu = &b.sounds[b.cues[0].sound.unwrap()];
        assert_eq!(menu.category, 2);
        assert_eq!(menu.tracks[0].rpcs, [0x47e]);
        let e = menu.tracks[0].event.as_ref().unwrap();
        assert_eq!((e.loop_count, e.waves.len()), (255, 1));
        // Base building: a new random track each loop.
        let building = b.sounds[b.cues[1].sound.unwrap()].tracks[0].event.as_ref().unwrap();
        assert_eq!(building.waves.len(), 5);
        assert!(building.new_wave_per_loop());
        assert_eq!((b.cues[1].instance_limit, b.cues[1].fade_out_ms), (1, 200));
    }

    #[test]
    fn parses_every_shipped_sound_bank() {
        let Some(dir) = super::super::tests::sounds_dir() else { return };
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&dir).into_iter().flatten() {
            if entry.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("xsb")) {
                files.push(entry.path().to_path_buf());
            }
        }
        assert!(files.len() > 50, "{}", files.len());
        let mut cues = 0;
        for f in files {
            let b = SoundBank::parse(&std::fs::read(&f).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            for c in &b.cues {
                let s = &b.sounds[c.sound.unwrap_or_else(|| panic!("{}: cue {} has no sound", f.display(), c.name))];
                assert!(s.tracks.iter().all(|t| t.event.as_ref().is_some_and(|e| !e.waves.is_empty())), "{}: {}", f.display(), c.name);
            }
            cues += b.cues.len();
        }
        assert!(cues > 3000, "{cues}");
    }
}
