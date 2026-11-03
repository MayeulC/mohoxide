//! XACT global settings (`SupCom.xgs`, magic `XGSF`): sound categories
//! (volume, parent, instance limits), variables (`Distance`, `Duck`,
//! `Weapon_LodCutoff`, ...) and RPC curves (variable -> volume / pitch),
//! which the sound banks reference by byte offset.
//!
//! Version 43 layout (verified against SupCom.xgs): `XGSF`, content version,
//! tool version, CRC, FILETIME, platform byte, then counts (categories,
//! variables, two unknown blobs, RPCs, DSP presets, DSP parameters) and
//! offsets (categories, variables, blob 1, category name index, blob 2,
//! variable name index, category names, variable names, RPCs, DSP presets,
//! DSP parameters). A category is 10 bytes: instance limit, fade in / out
//! (ms), instance behaviour (`>> 3`: 0 fail, 1 queue, 2 replace oldest, 3
//! replace quietest, 4 replace lowest priority), parent (`0xffff` none),
//! volume byte, visibility. A variable is 13 bytes: flags, initial, min,
//! max. An RPC: variable, point count, parameter (0 volume in 1/100 dB, 1
//! pitch in cents), then (x, y, curve type) points.

use std::collections::BTreeMap;

use super::{cstr, f32_at, u16_at, u32_at, volume_db};

#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub name: String,
    /// 255 = no limit.
    pub instance_limit: u8,
    pub fade_in_ms: u16,
    pub fade_out_ms: u16,
    pub behavior: u8,
    pub parent: Option<u16>,
    pub volume_db: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    pub name: String,
    pub flags: u8,
    pub initial: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RpcParameter {
    /// Hundredths of a dB.
    Volume,
    /// Cents.
    Pitch,
    Other(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rpc {
    pub variable: u16,
    pub parameter: RpcParameter,
    /// (x, y), x ascending.
    pub points: Vec<(f32, f32)>,
}

impl Rpc {
    /// The curve at `x` (linear between points, held past the ends).
    pub fn eval(&self, x: f32) -> f32 {
        let p = &self.points;
        let Some(first) = p.first() else { return 0.0 };
        if x <= first.0 {
            return first.1;
        }
        for w in p.windows(2) {
            let (a, b) = (w[0], w[1]);
            if x <= b.0 {
                let t = if b.0 > a.0 { (x - a.0) / (b.0 - a.0) } else { 1.0 };
                return a.1 + (b.1 - a.1) * t;
            }
        }
        p[p.len() - 1].1
    }

    /// The x past which the curve stays at or below -96 dB (volume curves
    /// that fade out), if it does.
    pub fn silent_from(&self) -> Option<f32> {
        if self.parameter != RpcParameter::Volume {
            return None;
        }
        let last = self.points.last()?;
        (last.1 <= -9600.0).then(|| self.points.iter().find(|p| p.1 <= -9600.0).map_or(last.0, |p| p.0))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlobalSettings {
    pub categories: Vec<Category>,
    pub variables: Vec<Variable>,
    /// By byte offset in the file (how sound banks name them).
    pub rpcs: BTreeMap<u32, Rpc>,
}

impl GlobalSettings {
    pub fn parse(d: &[u8]) -> Result<Self, String> {
        if d.get(0..4) != Some(b"XGSF") {
            return Err("not an XACT global settings file (no XGSF)".into());
        }
        let n = |o: usize| u16_at(d, o).map(usize::from);
        let (categories, variables, rpcs) = (n(0x13)?, n(0x15)?, n(0x1b)?);
        let off = |i: usize| u32_at(d, 0x21 + 4 * i).map(|v| v as usize);
        let (cat_off, var_off, cat_names, var_names, rpc_off) = (off(0)?, off(1)?, off(3)?, off(5)?, off(8)?);
        let mut s = GlobalSettings::default();
        for i in 0..categories {
            let o = cat_off + 10 * i;
            let parent = u16_at(d, o + 6)?;
            s.categories.push(Category {
                name: cstr(d, u32_at(d, cat_names + 6 * i)? as usize)?,
                instance_limit: *d.get(o).ok_or("truncated category")?,
                fade_in_ms: u16_at(d, o + 1)?,
                fade_out_ms: u16_at(d, o + 3)?,
                behavior: d[o + 5] >> 3,
                parent: (parent != 0xFFFF).then_some(parent),
                volume_db: volume_db(*d.get(o + 8).ok_or("truncated category")?),
            });
        }
        for i in 0..variables {
            let o = var_off + 13 * i;
            s.variables.push(Variable {
                name: cstr(d, u32_at(d, var_names + 6 * i)? as usize)?,
                flags: *d.get(o).ok_or("truncated variable")?,
                initial: f32_at(d, o + 1)?,
                min: f32_at(d, o + 5)?,
                max: f32_at(d, o + 9)?,
            });
        }
        let mut o = rpc_off;
        for _ in 0..rpcs {
            let start = o as u32;
            let variable = u16_at(d, o)?;
            let count = *d.get(o + 2).ok_or("truncated rpc")? as usize;
            let parameter = match u16_at(d, o + 3)? {
                0 => RpcParameter::Volume,
                1 => RpcParameter::Pitch,
                p => RpcParameter::Other(p),
            };
            o += 5;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                points.push((f32_at(d, o)?, f32_at(d, o + 4)?));
                o += 9;
            }
            s.rpcs.insert(start, Rpc { variable, parameter, points });
        }
        Ok(s)
    }

    pub fn category(&self, name: &str) -> Option<u16> {
        self.categories.iter().position(|c| c.name.eq_ignore_ascii_case(name)).map(|i| i as u16)
    }

    pub fn variable(&self, name: &str) -> Option<u16> {
        self.variables.iter().position(|v| v.name.eq_ignore_ascii_case(name)).map(|i| i as u16)
    }

    /// Whether `category` is `ancestor` or below it.
    pub fn is_within(&self, category: u16, ancestor: u16) -> bool {
        let mut c = Some(category);
        let mut guard = 0;
        while let Some(i) = c {
            if i == ancestor {
                return true;
            }
            c = self.categories.get(i as usize).and_then(|c| c.parent);
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_curves_interpolate_and_hold() {
        let r = Rpc { variable: 0, parameter: RpcParameter::Volume, points: vec![(0.0, 0.0), (100.0, -1000.0), (200.0, -9600.0)] };
        assert_eq!(r.eval(-5.0), 0.0);
        assert_eq!(r.eval(50.0), -500.0);
        assert_eq!(r.eval(500.0), -9600.0);
        assert_eq!(r.silent_from(), Some(200.0));
    }

    #[test]
    fn parses_supcom_xgs() {
        let Some(path) = super::super::tests::sounds_dir().map(|d| d.join("SupCom.xgs")) else { return };
        let s = GlobalSettings::parse(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(s.categories.len(), 39);
        let music = s.category("Music").unwrap();
        let c = &s.categories[music as usize];
        assert_eq!((c.instance_limit, c.fade_out_ms, c.behavior, c.parent), (1, 200, 2, Some(0)));
        let units_uef = s.category("UnitsUEF").unwrap();
        assert!(s.is_within(units_uef, s.category("World").unwrap()));
        assert!(!s.is_within(units_uef, s.category("Music").unwrap()));
        assert_eq!(s.variables[s.variable("Distance").unwrap() as usize].max, 10000.0);
        assert_eq!(s.rpcs.len(), 20);
        // The music cues' release fade (`ReleaseTime` -> volume).
        let release = &s.rpcs[&0x47e];
        assert_eq!(s.variables[release.variable as usize].name, "ReleaseTime");
        assert!(release.silent_from().unwrap() > 6000.0);
    }
}
