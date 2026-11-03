//! Effect blueprints (`/effects/emitters/*.bp`): `EmitterBlueprint { ... }`,
//! `BeamBlueprint { ... }` and `TrailEmitterBlueprint { ... }` Lua table
//! literals, parsed without a Lua VM so the renderer can read them on its
//! own threads (and the sim only needs the lifetime).
//!
//! Semantics (FAF `engine/Core/Blueprints/*Blueprint.lua`, wiki "Modding
//! Emitters"): times are in ticks (`Lifetime` -1 = forever); curves are keyed
//! over the emitter's repeat cycle (`x` in `0..XRange`), each key a value `y`
//! and a random range `z` (a particle gets `y +- z/2`), sampled once when a
//! particle is born; velocities are ogrids per tick, accelerations ogrids per
//! tick squared; `EmitRate` is particles per tick.

use std::io;

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// A parsed Lua literal value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Str(String),
    Bool(bool),
    /// Fields in order; `None` key for list entries.
    Table(Vec<(Option<String>, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Table(t) => t.iter().find(|(k, _)| k.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(key))).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn num(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Value::Number(n) => Some(*n),
            Value::Bool(b) => Some(*b as u8 as f64),
            _ => None,
        }
    }
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Value::Bool(true))) || self.num(key).is_some_and(|n| n != 0.0)
    }
    pub fn string(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    o: usize,
}

impl<'a> Parser<'a> {
    fn skip(&mut self) {
        loop {
            while self.o < self.s.len() && self.s[self.o].is_ascii_whitespace() {
                self.o += 1;
            }
            if self.s[self.o..].starts_with(b"--") {
                while self.o < self.s.len() && self.s[self.o] != b'\n' {
                    self.o += 1;
                }
                continue;
            }
            break;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.s.get(self.o).copied()
    }
    fn ident(&mut self) -> Option<&'a str> {
        self.skip();
        let start = self.o;
        while self.o < self.s.len() && (self.s[self.o].is_ascii_alphanumeric() || self.s[self.o] == b'_') {
            self.o += 1;
        }
        if self.o == start || self.s[start].is_ascii_digit() {
            self.o = start;
            return None;
        }
        Some(std::str::from_utf8(&self.s[start..self.o]).unwrap_or(""))
    }
    fn expect(&mut self, c: u8) -> io::Result<()> {
        if self.peek() == Some(c) {
            self.o += 1;
            Ok(())
        } else {
            Err(invalid(format!("expected '{}' at byte {}", c as char, self.o)))
        }
    }
    fn value(&mut self) -> io::Result<Value> {
        match self.peek().ok_or_else(|| invalid("unexpected end"))? {
            b'{' => self.table(),
            b'[' if self.s[self.o..].starts_with(b"[[") => {
                let start = self.o + 2;
                let len = self.s[start..].windows(2).position(|w| w == b"]]").ok_or_else(|| invalid("unterminated [[string]]"))?;
                self.o = start + len + 2;
                Ok(Value::Str(String::from_utf8_lossy(&self.s[start..start + len]).into_owned()))
            }
            q @ (b'\'' | b'"') => {
                self.o += 1;
                let start = self.o;
                while self.o < self.s.len() && self.s[self.o] != q {
                    self.o += 1;
                }
                let v = String::from_utf8_lossy(&self.s[start..self.o]).into_owned();
                self.o += 1;
                Ok(Value::Str(v))
            }
            _ => {
                let save = self.o;
                if let Some(id) = self.ident() {
                    return match id {
                        "true" => Ok(Value::Bool(true)),
                        "false" => Ok(Value::Bool(false)),
                        "nil" => Ok(Value::Bool(false)),
                        _ => Err(invalid(format!("unexpected identifier {id} at byte {save}"))),
                    };
                }
                let start = self.o;
                while self.o < self.s.len() && matches!(self.s[self.o], b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E') {
                    self.o += 1;
                }
                std::str::from_utf8(&self.s[start..self.o])
                    .ok()
                    .and_then(|t| t.parse::<f64>().ok())
                    .map(Value::Number)
                    .ok_or_else(|| invalid(format!("bad value at byte {start}")))
            }
        }
    }
    fn table(&mut self) -> io::Result<Value> {
        self.expect(b'{')?;
        let mut fields = Vec::new();
        loop {
            match self.peek() {
                Some(b'}') => {
                    self.o += 1;
                    return Ok(Value::Table(fields));
                }
                Some(b',' | b';') => {
                    self.o += 1;
                }
                None => return Err(invalid("unterminated table")),
                _ => {
                    let save = self.o;
                    let key = self.ident().map(str::to_owned);
                    if key.is_some() && self.peek() == Some(b'=') {
                        self.o += 1;
                        let v = self.value()?;
                        fields.push((key, v));
                    } else {
                        self.o = save;
                        fields.push((None, self.value()?));
                    }
                }
            }
        }
    }
}

/// Parses `Name { ... }`: the blueprint kind name and its table.
pub fn parse_literal(src: &str) -> io::Result<(String, Value)> {
    let mut p = Parser { s: src.as_bytes(), o: 0 };
    let name = p.ident().ok_or_else(|| invalid("expected a blueprint name"))?.to_owned();
    Ok((name, p.table()?))
}

/// A curve over the emitter cycle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    pub x_range: f32,
    /// (x, value, random range), sorted by x.
    pub keys: Vec<[f32; 3]>,
}

impl Curve {
    fn from(v: Option<&Value>) -> Self {
        let Some(v) = v else { return Self::default() };
        let mut keys: Vec<[f32; 3]> = match v.get("Keys") {
            Some(Value::Table(list)) => list
                .iter()
                .filter_map(|(_, k)| Some([k.num("x")? as f32, k.num("y").unwrap_or(0.0) as f32, k.num("z").unwrap_or(0.0) as f32]))
                .collect(),
            _ => Vec::new(),
        };
        keys.sort_by(|a, b| a[0].total_cmp(&b[0]));
        Self { x_range: v.num("XRange").unwrap_or(1.0) as f32, keys }
    }

    /// Value and random range at cycle fraction `t` (0..1): flat before the
    /// first and after the last key, linear between.
    pub fn sample(&self, t: f32) -> (f32, f32) {
        let x = t * self.x_range;
        match self.keys.as_slice() {
            [] => (0.0, 0.0),
            [k] => (k[1], k[2]),
            keys => {
                let i = keys.partition_point(|k| k[0] <= x);
                if i == 0 {
                    return (keys[0][1], keys[0][2]);
                }
                if i == keys.len() {
                    let k = keys[keys.len() - 1];
                    return (k[1], k[2]);
                }
                let (a, b) = (keys[i - 1], keys[i]);
                let f = if b[0] > a[0] { (x - a[0]) / (b[0] - a[0]) } else { 0.0 };
                (a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f)
            }
        }
    }
}

/// Emitter curves, in `IEffect` curve parameter order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CurveId {
    XDir,
    YDir,
    ZDir,
    EmitRate,
    Lifetime,
    Velocity,
    XAccel,
    YAccel,
    ZAccel,
    Resistance,
    Size,
    XPos,
    YPos,
    ZPos,
    StartSize,
    EndSize,
    InitialRotation,
    RotationRate,
    FrameRate,
    TextureSelection,
    RampSelection,
}

impl CurveId {
    pub const ALL: [CurveId; 21] = [
        CurveId::XDir,
        CurveId::YDir,
        CurveId::ZDir,
        CurveId::EmitRate,
        CurveId::Lifetime,
        CurveId::Velocity,
        CurveId::XAccel,
        CurveId::YAccel,
        CurveId::ZAccel,
        CurveId::Resistance,
        CurveId::Size,
        CurveId::XPos,
        CurveId::YPos,
        CurveId::ZPos,
        CurveId::StartSize,
        CurveId::EndSize,
        CurveId::InitialRotation,
        CurveId::RotationRate,
        CurveId::FrameRate,
        CurveId::TextureSelection,
        CurveId::RampSelection,
    ];

    /// Blueprint field name.
    pub fn field(self) -> &'static str {
        match self {
            CurveId::XDir => "XDirectionCurve",
            CurveId::YDir => "YDirectionCurve",
            CurveId::ZDir => "ZDirectionCurve",
            CurveId::EmitRate => "EmitRateCurve",
            CurveId::Lifetime => "LifetimeCurve",
            CurveId::Velocity => "VelocityCurve",
            CurveId::XAccel => "XAccelCurve",
            CurveId::YAccel => "YAccelCurve",
            CurveId::ZAccel => "ZAccelCurve",
            CurveId::Resistance => "ResistanceCurve",
            CurveId::Size => "SizeCurve",
            CurveId::XPos => "XPosCurve",
            CurveId::YPos => "YPosCurve",
            CurveId::ZPos => "ZPosCurve",
            CurveId::StartSize => "StartSizeCurve",
            CurveId::EndSize => "EndSizeCurve",
            CurveId::InitialRotation => "InitialRotationCurve",
            CurveId::RotationRate => "RotationRateCurve",
            CurveId::FrameRate => "FrameRateCurve",
            CurveId::TextureSelection => "TextureSelectionCurve",
            CurveId::RampSelection => "RampSelectionCurve",
        }
    }

    /// `IEffect:SetEmitterCurveParam` name.
    pub fn param(self) -> &'static str {
        match self {
            CurveId::XDir => "XDIR_CURVE",
            CurveId::YDir => "YDIR_CURVE",
            CurveId::ZDir => "ZDIR_CURVE",
            CurveId::EmitRate => "EMITRATE_CURVE",
            CurveId::Lifetime => "LIFETIME_CURVE",
            CurveId::Velocity => "VELOCITY_CURVE",
            CurveId::XAccel => "X_ACCEL_CURVE",
            CurveId::YAccel => "Y_ACCEL_CURVE",
            CurveId::ZAccel => "Z_ACCEL_CURVE",
            CurveId::Resistance => "RESISTANCE_CURVE",
            CurveId::Size => "SIZE_CURVE",
            CurveId::XPos => "X_POSITION_CURVE",
            CurveId::YPos => "Y_POSITION_CURVE",
            CurveId::ZPos => "Z_POSITION_CURVE",
            CurveId::StartSize => "BEGINSIZE_CURVE",
            CurveId::EndSize => "ENDSIZE_CURVE",
            CurveId::InitialRotation => "ROTATION_CURVE",
            CurveId::RotationRate => "ROTATION_RATE_CURVE",
            CurveId::FrameRate => "FRAMERATE_CURVE",
            CurveId::TextureSelection => "TEXTURESELECTION_CURVE",
            CurveId::RampSelection => "RAMPSELECTION_CURVE",
        }
    }
}

/// Blend modes: 0 alpha, 1 modulate inverse, 2 modulate 2x inverse, 3 add,
/// 4 premultiplied alpha, 5 refract.
#[derive(Clone, Debug, PartialEq)]
pub struct EmitterBp {
    pub lifetime: f32,
    pub repeat_time: f32,
    pub blend_mode: u32,
    pub frame_count: u32,
    pub strip_count: u32,
    pub texture: String,
    pub ramp: String,
    pub flat: bool,
    pub gravity: bool,
    pub align_rotation: bool,
    pub align_to_bone: bool,
    pub local_velocity: bool,
    pub local_acceleration: bool,
    pub resistance: bool,
    pub interpolate_emission: bool,
    pub snap_to_waterline: bool,
    pub only_emit_on_water: bool,
    /// Emit only while on screen (else count the missed ticks).
    pub emit_if_visible: bool,
    /// Back on screen, emit for the ticks missed.
    pub catchup_emit: bool,
    /// Off screen when created: never emits.
    pub create_if_visible: bool,
    /// Shown at each graphics fidelity (low, medium, high).
    pub fidelity: [bool; 3],
    /// Distance from the camera plane past which emission stops; `<= 0`
    /// never.
    pub lod_cutoff: f32,
    pub sort_order: f32,
    /// By [`CurveId`] index.
    pub curves: Vec<Curve>,
}

impl EmitterBp {
    pub fn curve(&self, id: CurveId) -> &Curve {
        &self.curves[id as usize]
    }

    /// The longest a particle of this emitter can live (ticks): the
    /// lifetime curve's highest value plus half its spread.
    pub fn longest_particle_life(&self) -> f32 {
        let c = self.curve(CurveId::Lifetime);
        c.keys.iter().map(|k| k[1] + k[2].abs() * 0.5).fold(0.0, f32::max)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeamBp {
    pub length: f32,
    pub lifetime: f32,
    pub thickness: f32,
    pub texture: String,
    /// RGBA (blueprint `x, y, z, w` = R, G, B, A... stored ARGB as w, x, y, z).
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub u_shift: f32,
    pub v_shift: f32,
    /// Texture repeats per ogrid (0 = stretched over the beam).
    pub repeat_rate: f32,
    pub blend_mode: u32,
    /// `<= 0` never cut.
    pub lod_cutoff: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrailBp {
    pub lifetime: f32,
    /// Ogrids.
    pub length: f32,
    pub size: f32,
    pub blend_mode: u32,
    pub texture: String,
    pub ramp: String,
    pub texture_repeat_rate: f32,
    /// `<= 0` never cut.
    pub lod_cutoff: f32,
    pub sort_order: f32,
    pub u_shift: f32,
    pub v_shift: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EffectBp {
    Emitter(EmitterBp),
    Beam(BeamBp),
    Trail(TrailBp),
}

fn lower(s: Option<&str>) -> String {
    s.unwrap_or("").to_lowercase()
}

/// A boolean field, `default` when absent.
fn flag_or(v: &Value, key: &str, default: bool) -> bool {
    if v.get(key).is_some() { v.flag(key) } else { default }
}

/// `ARGB` colour stored as a named quaternion (`w, x, y, z` = A, R, G, B).
fn color(v: Option<&Value>) -> [f32; 4] {
    match v {
        Some(c) => [c.num("x").unwrap_or(1.0) as f32, c.num("y").unwrap_or(1.0) as f32, c.num("z").unwrap_or(1.0) as f32, c.num("w").unwrap_or(1.0) as f32],
        None => [1.0; 4],
    }
}

impl EffectBp {
    pub fn parse(src: &str) -> io::Result<Self> {
        let (name, v) = parse_literal(src)?;
        let num = |k: &str, d: f64| v.num(k).unwrap_or(d) as f32;
        Ok(match name.as_str() {
            "EmitterBlueprint" => EffectBp::Emitter(EmitterBp {
                lifetime: num("Lifetime", -1.0),
                repeat_time: num("Repeattime", 1.0),
                blend_mode: num("Blendmode", 0.0) as u32,
                frame_count: num("TextureFramecount", 1.0).max(1.0) as u32,
                strip_count: num("TextureStripcount", 1.0).max(1.0) as u32,
                texture: lower(v.string("Texture").or(v.string("TextureName"))),
                ramp: lower(v.string("RampTexture")),
                flat: v.flag("Flat"),
                gravity: v.flag("Gravity"),
                align_rotation: v.flag("AlignRotation"),
                align_to_bone: v.flag("AlignToBone"),
                local_velocity: v.flag("LocalVelocity"),
                local_acceleration: v.flag("LocalAcceleration"),
                resistance: v.flag("ParticleResistance"),
                interpolate_emission: v.flag("InterpolateEmission"),
                snap_to_waterline: v.flag("SnapToWaterline"),
                only_emit_on_water: v.flag("OnlyEmitOnWater"),
                // Moho's defaults: emit only when visible, catch up, create
                // regardless, every fidelity.
                emit_if_visible: flag_or(&v, "EmitIfVisible", true),
                catchup_emit: flag_or(&v, "CatchupEmit", true),
                create_if_visible: v.flag("CreateIfVisible"),
                fidelity: [flag_or(&v, "LowFidelity", true), flag_or(&v, "MedFidelity", true), flag_or(&v, "HighFidelity", true)],
                lod_cutoff: num("LODCutoff", 300.0),
                sort_order: num("SortOrder", 0.0),
                curves: CurveId::ALL.iter().map(|c| Curve::from(v.get(c.field()))).collect(),
            }),
            "BeamBlueprint" => EffectBp::Beam(BeamBp {
                length: num("Length", 1.0),
                lifetime: num("Lifetime", -1.0),
                thickness: num("Thickness", 0.1),
                texture: lower(v.string("TextureName")),
                start_color: color(v.get("StartColor")),
                end_color: color(v.get("EndColor")),
                u_shift: num("UShift", 0.0),
                v_shift: num("VShift", 0.0),
                repeat_rate: num("RepeatRate", 0.0),
                blend_mode: num("Blendmode", 3.0) as u32,
                lod_cutoff: num("LODCutoff", 0.0),
            }),
            "TrailEmitterBlueprint" | "TrailBlueprint" => EffectBp::Trail(TrailBp {
                lifetime: num("Lifetime", -1.0),
                length: num("TrailLength", 1.0),
                size: num("Size", 0.1),
                blend_mode: v.num("BlendMode").or(v.num("Blendmode")).unwrap_or(3.0) as u32,
                texture: lower(v.string("RepeatTexture")),
                ramp: lower(v.string("RampTexture")),
                texture_repeat_rate: num("TextureRepeatRate", 1.0),
                lod_cutoff: num("LODCutoff", 300.0),
                sort_order: num("SortOrder", 0.0),
                u_shift: num("UShift", 0.0),
                v_shift: num("VShift", 0.0),
            }),
            other => return Err(invalid(format!("unknown effect blueprint {other}"))),
        })
    }

    /// Emitter lifetime in ticks (negative = forever).
    pub fn lifetime(&self) -> f32 {
        match self {
            EffectBp::Emitter(e) => e.lifetime,
            EffectBp::Beam(b) => b.lifetime,
            EffectBp::Trail(t) => t.lifetime,
        }
    }
}

/// Just the `Lifetime` of an effect blueprint, as the decimal in the file
/// (deterministic: the sim expires effects by it).
pub fn lifetime_ticks(src: &str) -> Option<f64> {
    let (_, v) = parse_literal(src).ok()?;
    v.num("Lifetime")
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMITTER: &str = "-- not used by faf
EmitterBlueprint {
	BlueprintId = 'build_laser_03',
	Lifetime = -1.00,
	Repeattime = 10.00,
	Blendmode = 3.00,
	Flat = false,
	Texture = '/textures/particles/build_laser_add_01.dds',
	RampTexture = '/textures/particles/ramp_blue_01.dds',
	YPosCurve = {
		XRange = 10.00,
		Keys = {
			{ x=2.00,y=-0.10,z=0.00 },
			{ x=6.00,y=0.30,z=0.20 },
		},
	},
}";

    #[test]
    fn parses_emitters_and_samples_curves() {
        let EffectBp::Emitter(e) = EffectBp::parse(EMITTER).unwrap() else { panic!() };
        assert_eq!((e.lifetime, e.repeat_time, e.blend_mode, e.flat), (-1.0, 10.0, 3, false));
        assert_eq!(e.texture, "/textures/particles/build_laser_add_01.dds");
        let c = e.curve(CurveId::YPos);
        assert_eq!(c.sample(0.0), (-0.1, 0.0)); // flat before the first key
        let (v, r) = c.sample(0.4); // x = 4: halfway between the keys
        assert!((v - 0.1).abs() < 1e-6 && (r - 0.1).abs() < 1e-6);
        assert_eq!(c.sample(1.0), (0.3, 0.2));
        assert_eq!(e.curve(CurveId::EmitRate).keys.len(), 0);
    }

    #[test]
    fn parses_beams_trails_and_lifetimes() {
        let beam = "BeamBlueprint{\n    EndColor = {\n        w = 0,\n        x = 0,\n        y = 0.5,\n        z = 1,\n    },\n    Length = 1.5,\n    Lifetime = 3.0,\n    TextureName = \"/textures/particles/beam_white_03.dds\",\n    Thickness = 0.0175,\n}";
        let EffectBp::Beam(b) = EffectBp::parse(beam).unwrap() else { panic!() };
        assert_eq!((b.length, b.lifetime, b.thickness, b.end_color), (1.5, 3.0, 0.0175, [0.0, 0.5, 1.0, 0.0]));
        let trail = "TrailEmitterBlueprint {\n    BlendMode = 0,\n    Lifetime = -1.0,\n    RepeatTexture = \"/textures/particles/trail_white_01.dds\",\n    Size = 0.12,\n    TrailLength = 10,\n}";
        let EffectBp::Trail(t) = EffectBp::parse(trail).unwrap() else { panic!() };
        assert_eq!((t.blend_mode, t.length, t.size), (0, 10.0, 0.12));
        assert_eq!(lifetime_ticks(beam), Some(3.0));
    }

    /// Every FAF effect blueprint parses. Fails without the FAF checkout
    /// (`RTS_SKIP_DATA_TESTS=1` skips).
    #[test]
    fn parses_every_faf_effect_blueprint() {
        const DIR: &str = "../fa/effects/emitters";
        if !crate::test_data::require("FAF effect blueprints", &[DIR]) {
            return;
        }
        let dir = std::fs::read_dir(DIR).expect("read emitters");
        let mut n = 0;
        for f in dir.flatten() {
            let src = String::from_utf8_lossy(&std::fs::read(f.path()).unwrap()).into_owned();
            EffectBp::parse(&src).unwrap_or_else(|e| panic!("{}: {e}", f.path().display()));
            n += 1;
        }
        assert!(n > 2000, "{n}");
    }
}
