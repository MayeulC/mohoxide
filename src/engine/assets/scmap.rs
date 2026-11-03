//! `.scmap` decoder (little-endian): the heightmap, water, terrain types and
//! props for the sim ([`read_terrain`]), plus lighting, strata, decals and
//! the texture blobs for the renderer ([`read_render`]). Versions 56 (FAF /
//! Ozonex editor) and 60 (FA) differ only in an extra minimap float and the
//! skybox section.
//!
//! Layout (verified against stock FA maps, e.g. SCMP_009):
//! ```text
//! "Map\x1a"  i32 major(=2)  u32 0xBEEFFEED  i32 2  f32 width  f32 height  i32 0  i16 0
//! i32 previewLen  [previewLen] DDS
//! i32 version (56 vanilla / 60 FA)
//! i32 hmWidth  i32 hmHeight  f32 heightScale  u16[(hmWidth+1)*(hmHeight+1)]
//! u8 0  cstr terrainShader  cstr background  cstr skyCube
//! i32 n  n x (cstr name, cstr path)            -- environment cube maps
//! f32 lightingMult  3f sunDir  3f ambience  3f sunColor  3f shadow  4f specular
//! f32 bloom  3f fogColor  f32 fogStart  f32 fogEnd
//! u8 hasWater  f32 surface  f32 deep  f32 abyss  ...rest of water settings
//! i32 n  n x wave generator
//! 6 x i32 minimap colours  [f32 if version > 56]
//! 10 x (cstr albedo, f32 scale)  9 x (cstr normal, f32 scale)  i32  i32
//! i32 n  n x decal   i32 n  n x decal group   i32 w  i32 h
//! i32 n  n x [i32 len] DDS normal map (several on 81 km maps)  [i32 len] DDS mask low  [i32 len] DDS mask high
//! i32 n  n x [i32 len] DDS water map   3 x u8[(w/2)*(h/2)] foam/flatness/depth bias
//! u8[w*h] terrain type codes (see /lua/TerrainTypes.lua)
//! [skybox if version >= 60]
//! i32 n  n x (cstr blueprint, 3f position, 3f rotX, 3f rotY, 3f rotZ, 3f scale)
//! ```
//! Section layout cross-checked against FAF's Neroxis map generator
//! (`SCMapImporter`) and verified to consume stock maps exactly to EOF.
//! World height of a sample = `sample * heightScale` (scale is usually 1/128).

use std::io;

#[derive(Debug, Clone)]
pub struct ScMapHeader {
    pub version: i32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WaterLevels {
    pub surface: f32,
    pub deep: f32,
    pub abyss: f32,
}

#[derive(Debug, Clone)]
pub struct Heightmap {
    /// Cells; there are `width + 1` samples per row.
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub samples: Vec<u16>,
}

struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self.o.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, format!("scmap truncated at {}", self.o))
        })?;
        let s = &self.b[self.o..end];
        self.o = end;
        Ok(s)
    }
    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn cstr(&mut self) -> io::Result<&'a str> {
        let rest = &self.b[self.o..];
        let len = rest.iter().position(|&c| c == 0).ok_or_else(|| invalid("unterminated string"))?;
        let s = std::str::from_utf8(&rest[..len]).map_err(|_| invalid("non-utf8 string"))?;
        self.o += len + 1;
        Ok(s)
    }
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapProp {
    pub blueprint: String,
    pub position: [f32; 3],
    /// Rotation matrix rows (x, y, z axes).
    pub rotation: [[f32; 3]; 3],
    pub scale: [f32; 3],
}

/// Everything the sim needs from an `.scmap`.
pub struct TerrainData {
    pub header: ScMapHeader,
    pub heightmap: Heightmap,
    pub water: Option<WaterLevels>,
    /// Terrain type code per cell, `width * height` (row-major, z then x).
    pub terrain_types: Vec<u8>,
    pub props: Vec<MapProp>,
}

fn f32x<const N: usize>(r: &mut Reader<'_>) -> io::Result<[f32; N]> {
    let mut out = [0.0; N];
    for v in &mut out {
        *v = r.f32()?;
    }
    Ok(out)
}


/// Parses everything the sim needs from an `.scmap`.
pub fn read_terrain(bytes: &[u8]) -> io::Result<TerrainData> {
    parse(bytes, false).map(|(t, _)| t)
}

/// Everything the renderer draws from an `.scmap` (render-only: floats and
/// texture blobs, never sim state).
#[derive(Debug, Clone, Default)]
pub struct MapRender {
    /// `TTerrain`, `TTerrainXP`, `Terrain200`, ...
    pub shader: String,
    pub background: String,
    pub sky_cube: String,
    /// Environment cube maps (name, path): units' reflections and ambient
    /// light (`<default>`, else moho's `/textures/environment/defaultenvcube.dds`).
    pub environment_cubes: Vec<(String, String)>,
    pub lighting: Lighting,
    pub water: Option<WaterRender>,
    /// Lower, strata 0-7, upper: texture path and scale.
    pub albedo: [Stratum; 10],
    /// Lower, strata 0-7.
    pub normals: [Stratum; 9],
    pub decals: Vec<Decal>,
    /// Shoreline wave generators (moho's `WaveSystem`), in file order.
    pub wave_generators: Vec<WaveGenerator>,
    /// DDS blobs (empty when absent).
    pub normal_map: Vec<u8>,
    /// Masks of strata 0-3 (rgba) and 4-7.
    pub mask_low: Vec<u8>,
    pub mask_high: Vec<u8>,
    /// Water map: g = water depth (`UtilityTextureC`).
    pub water_map: Vec<u8>,
    /// The sky dome (v60 maps; `None` on v56 ones).
    pub sky: Option<SkyBox>,
}

/// Moho's `SkyDome` as the map stores it (field order as `SkyDome::Load`
/// reads it; MohoEngine.dll 0x103dfec0).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkyBox {
    /// The dome's centre on the ground.
    pub position: [f32; 3],
    /// Raises the dome (and is where the atmosphere's horizon colour ends).
    pub horizon_height: f32,
    /// The dome's radius at its base.
    pub scale: f32,
    /// The base's elevation angle on the dome's sphere (radians).
    pub sub_height: f32,
    /// Vertices around and rows up.
    pub subdivisions: (i32, i32),
    /// Height over the horizon where the zenith colour is reached.
    pub zenith_height: f32,
    pub horizon_color: [f32; 3],
    pub zenith_color: [f32; 3],
    /// Planets' glow (`decalGlowMultiplier`).
    pub decal_glow: f32,
    /// The planets' albedo and glow textures.
    pub albedo: String,
    pub glow: String,
    pub planets: Vec<SkyPlanet>,
    pub mid_color: [u8; 3],
    pub cirrus_multiplier: f32,
    pub cirrus_color: [f32; 3],
    pub cirrus_texture: String,
    pub cirrus_layers: Vec<CirrusLayer>,
    /// The section's last float (unused by the shaders).
    pub clouds: f32,
}

/// A planet (or moon) on the sky: a billboard of part of the sky's albedo.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkyPlanet {
    pub position: [f32; 3],
    pub rotation: f32,
    pub scale: [f32; 2],
    /// The texture's region: u, v, width, height.
    pub uv: [f32; 4],
}

/// A scrolling cirrus layer (`sky.fx` `Cirrus`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CirrusLayer {
    pub frequency: [f32; 2],
    pub speed: f32,
    pub direction: [f32; 2],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stratum {
    pub path: String,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Lighting {
    pub multiplier: f32,
    /// Toward the sun.
    pub sun_direction: [f32; 3],
    pub ambience: [f32; 3],
    pub sun_color: [f32; 3],
    pub shadow_fill: [f32; 3],
    pub specular: [f32; 4],
    pub bloom: f32,
    pub fog_color: [f32; 3],
    pub fog_start: f32,
    pub fog_end: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaterRender {
    pub levels: WaterLevels,
    pub surface_color: [f32; 3],
    /// Colour lerp (min, max).
    pub color_lerp: [f32; 2],
    pub refraction_scale: f32,
    pub fresnel_bias: f32,
    pub fresnel_power: f32,
    pub unit_reflection: f32,
    pub sky_reflection: f32,
    pub sun_shininess: f32,
    pub sun_strength: f32,
    pub sun_direction: [f32; 3],
    pub sun_color: [f32; 3],
    pub sun_reflection: f32,
    pub sun_glow: f32,
    pub cube_map: String,
    pub ramp: String,
    /// Per wave texture: repeat, movement (x, y) and path.
    pub waves: [(f32, [f32; 2], String); 4],
}

/// A shoreline wave generator (`Moho::WaveGenerator`, field order as its
/// `Load`, MohoEngine.dll 0x1044b890): every `period` seconds or so it lays
/// one flat particle (`TRampAnimateFlat`) at `position`, turned by
/// `rotation`, drifting at `velocity` (per tick), living `lifetime` ticks
/// and growing from `scale.0` to `scale.1` (half-size). Pairs are (min,
/// max) ranges each wave picks from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaveGenerator {
    pub texture: String,
    pub ramp: String,
    pub position: [f32; 3],
    pub rotation: f32,
    pub velocity: [f32; 3],
    pub lifetime: (f32, f32),
    pub period: (f32, f32),
    pub scale: (f32, f32),
    pub frame_count: f32,
    pub frame_rate: (f32, f32),
    pub strip_count: f32,
}

/// `Decal::kind` values (GPG editor numbering).
pub mod decal_kind {
    pub const ALBEDO: i32 = 1;
    pub const NORMALS: i32 = 2;
    pub const WATER_MASK: i32 = 3;
    pub const WATER_ALBEDO: i32 = 4;
    pub const WATER_NORMALS: i32 = 5;
    pub const GLOW: i32 = 6;
    pub const NORMALS_ALPHA: i32 = 7;
    pub const GLOW_MASK: i32 = 8;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decal {
    pub kind: i32,
    /// Texture paths (first: albedo / normal map; second: optional mask or
    /// specular).
    pub textures: Vec<String>,
    pub scale: [f32; 3],
    pub position: [f32; 3],
    /// Euler angles, radians.
    pub rotation: [f32; 3],
    /// Not drawn beyond this camera distance (0 = always).
    pub cutoff: f32,
    pub cutoff_min: f32,
    /// Army that owns it (-1 = none).
    pub army: i32,
}

/// Parses the render data of an `.scmap` as well as the sim's.
pub fn read_render(bytes: &[u8]) -> io::Result<(TerrainData, MapRender)> {
    parse(bytes, true).map(|(t, r)| (t, r.expect("render data requested")))
}

fn sky_box(r: &mut Reader<'_>) -> io::Result<SkyBox> {
    let mut s = SkyBox { position: f32x::<3>(r)?, ..Default::default() };
    [s.horizon_height, s.scale, s.sub_height] = f32x::<3>(r)?;
    s.subdivisions = (r.i32()?, r.i32()?);
    [s.zenith_height] = f32x::<1>(r)?;
    s.horizon_color = f32x::<3>(r)?;
    s.zenith_color = f32x::<3>(r)?;
    [s.decal_glow] = f32x::<1>(r)?;
    s.albedo = string(r)?;
    s.glow = string(r)?;
    let planets = r.i32()?;
    for _ in 0..planets.max(0) {
        let position = f32x::<3>(r)?;
        let [rotation] = f32x::<1>(r)?;
        s.planets.push(SkyPlanet { position, rotation, scale: f32x::<2>(r)?, uv: f32x::<4>(r)? });
    }
    s.mid_color = [r.u8()?, r.u8()?, r.u8()?];
    [s.cirrus_multiplier] = f32x::<1>(r)?;
    s.cirrus_color = f32x::<3>(r)?;
    s.cirrus_texture = string(r)?;
    let layers = r.i32()?;
    for _ in 0..layers.max(0) {
        let frequency = f32x::<2>(r)?;
        let [speed] = f32x::<1>(r)?;
        s.cirrus_layers.push(CirrusLayer { frequency, speed, direction: f32x::<2>(r)? });
    }
    [s.clouds] = f32x::<1>(r)?;
    Ok(s)
}

fn string(r: &mut Reader<'_>) -> io::Result<String> {
    Ok(r.cstr()?.to_owned())
}

/// A length-prefixed blob; copied only when `keep`.
fn blob(r: &mut Reader<'_>, keep: bool) -> io::Result<Vec<u8>> {
    let len = r.i32()?;
    let b = r.take(usize::try_from(len).map_err(|_| invalid("negative blob length"))?)?;
    Ok(if keep { b.to_vec() } else { Vec::new() })
}

fn parse(bytes: &[u8], render: bool) -> io::Result<(TerrainData, Option<MapRender>)> {
    let (header, heightmap, mut r) = read_heightmap_inner(bytes)?;
    if header.version < 56 {
        return Err(invalid(format!("scmap version {} not supported (FA maps are 56 or 60)", header.version)));
    }
    let mut m = MapRender::default();
    r.u8()?;
    m.shader = string(&mut r)?;
    m.background = string(&mut r)?;
    m.sky_cube = string(&mut r)?;
    let cubemaps = r.i32()?;
    for _ in 0..cubemaps.max(0) {
        let name = r.cstr()?.to_string();
        let path = r.cstr()?.to_string();
        m.environment_cubes.push((name, path));
    }
    let l = &mut m.lighting;
    l.multiplier = r.f32()?;
    l.sun_direction = f32x::<3>(&mut r)?;
    l.ambience = f32x::<3>(&mut r)?;
    l.sun_color = f32x::<3>(&mut r)?;
    l.shadow_fill = f32x::<3>(&mut r)?;
    l.specular = f32x::<4>(&mut r)?;
    l.bloom = r.f32()?;
    l.fog_color = f32x::<3>(&mut r)?;
    l.fog_start = r.f32()?;
    l.fog_end = r.f32()?;
    let has_water = r.u8()? != 0;
    let levels = WaterLevels { surface: r.f32()?, deep: r.f32()?, abyss: r.f32()? };
    let water = has_water.then_some(levels);
    let mut w = WaterRender { levels, ..Default::default() };
    w.surface_color = f32x::<3>(&mut r)?;
    w.color_lerp = f32x::<2>(&mut r)?;
    [w.refraction_scale, w.fresnel_bias, w.fresnel_power, w.unit_reflection, w.sky_reflection, w.sun_shininess, w.sun_strength] = f32x::<7>(&mut r)?;
    w.sun_direction = f32x::<3>(&mut r)?;
    w.sun_color = f32x::<3>(&mut r)?;
    [w.sun_reflection, w.sun_glow] = f32x::<2>(&mut r)?;
    w.cube_map = string(&mut r)?;
    w.ramp = string(&mut r)?;
    let repeats = f32x::<4>(&mut r)?;
    for (i, wave) in w.waves.iter_mut().enumerate() {
        let movement = f32x::<2>(&mut r)?;
        *wave = (repeats[i], movement, string(&mut r)?);
    }
    m.water = has_water.then_some(w);
    let wave_generators = r.i32()?;
    for _ in 0..wave_generators.max(0) {
        let (texture, ramp) = (string(&mut r)?, string(&mut r)?);
        let position = f32x::<3>(&mut r)?;
        let rotation = r.f32()?;
        let velocity = f32x::<3>(&mut r)?;
        // (Maps from version 52 on carry the frame fields; FA's are 56+.)
        let [l0, l1, p0, p1, s0, s1, frames, f0, f1, strips] = f32x::<10>(&mut r)?;
        if render {
            m.wave_generators.push(WaveGenerator {
                texture,
                ramp,
                position,
                rotation,
                velocity,
                lifetime: (l0, l1),
                period: (p0, p1),
                scale: (s0, s1),
                frame_count: frames,
                frame_rate: (f0, f1),
                strip_count: strips,
            });
        }
    }
    r.take(4 * 6)?; // minimap colours
    if header.version > 56 {
        r.take(4)?;
    }
    for s in m.albedo.iter_mut() {
        *s = Stratum { path: string(&mut r)?, scale: r.f32()? };
    }
    for s in m.normals.iter_mut() {
        *s = Stratum { path: string(&mut r)?, scale: r.f32()? };
    }
    r.take(8)?;
    let decals = r.i32()?;
    for _ in 0..decals.max(0) {
        let _id = r.i32()?;
        let kind = r.i32()?;
        let n = r.i32()?;
        let mut textures = Vec::with_capacity(n.max(0) as usize);
        for _ in 0..n.max(0) {
            let b = blob(&mut r, render)?;
            if render {
                textures.push(String::from_utf8_lossy(&b).trim_end_matches('\0').to_lowercase());
            }
        }
        let scale = f32x::<3>(&mut r)?;
        let position = f32x::<3>(&mut r)?;
        let rotation = f32x::<3>(&mut r)?;
        let [cutoff, cutoff_min] = f32x::<2>(&mut r)?;
        let army = r.i32()?;
        if render {
            m.decals.push(Decal { kind, textures, scale, position, rotation, cutoff, cutoff_min, army });
        }
    }
    let groups = r.i32()?;
    for _ in 0..groups.max(0) {
        r.take(4)?;
        r.cstr()?;
        let n = r.i32()?;
        r.take(4 * n.max(0) as usize)?;
    }
    r.take(8)?; // width, height
    // Big maps (81 km) split the normal map into several blobs.
    let normal_maps = r.i32()?;
    for i in 0..normal_maps.max(0) {
        let b = blob(&mut r, render && i == 0)?;
        if i == 0 {
            m.normal_map = b;
        }
    }
    m.mask_low = blob(&mut r, render)?;
    m.mask_high = blob(&mut r, render)?;
    let water_maps = r.i32()?;
    for i in 0..water_maps.max(0) {
        let b = blob(&mut r, render && i == 0)?;
        if i == 0 {
            m.water_map = b;
        }
    }
    let (w, h) = (heightmap.width as usize, heightmap.height as usize);
    r.take(3 * (w / 2) * (h / 2))?; // foam, flatness, depth bias
    let terrain_types = r.take(w * h)?.to_vec();
    if header.version >= 60 {
        m.sky = Some(sky_box(&mut r)?);
    }
    let prop_count = r.i32()?;
    let mut props = Vec::with_capacity(prop_count.max(0) as usize);
    for _ in 0..prop_count.max(0) {
        let blueprint = r.cstr()?.to_owned();
        let position = f32x::<3>(&mut r)?;
        let rotation = [f32x::<3>(&mut r)?, f32x::<3>(&mut r)?, f32x::<3>(&mut r)?];
        let scale = f32x::<3>(&mut r)?;
        props.push(MapProp { blueprint, position, rotation, scale });
    }
    if r.o != r.b.len() {
        return Err(invalid(format!("scmap: {} trailing bytes", r.b.len() - r.o)));
    }
    Ok((TerrainData { header, heightmap, water, terrain_types, props }, render.then_some(m)))
}

/// Parses the header and heightmap of an `.scmap`.
/// The map's preview image (the DDS the editor stored after the header):
/// what the minimap shows.
pub fn read_preview(bytes: &[u8]) -> io::Result<&[u8]> {
    let mut r = Reader { b: bytes, o: 0 };
    if r.take(4)? != b"Map\x1a" {
        return Err(invalid("not an scmap (bad magic)"));
    }
    r.take(4 + 8 + 8 + 6)?; // major, 0xBEEFFEED + 2, width / height, i32 0 + i16 0
    let len = r.i32()?;
    r.take(usize::try_from(len).map_err(|_| invalid("negative preview length"))?)
}

pub fn read_heightmap(bytes: &[u8]) -> io::Result<(ScMapHeader, Heightmap)> {
    let (h, hm, _) = read_heightmap_inner(bytes)?;
    Ok((h, hm))
}

fn read_heightmap_inner(bytes: &[u8]) -> io::Result<(ScMapHeader, Heightmap, Reader<'_>)> {
    let mut r = Reader { b: bytes, o: 0 };
    if r.take(4)? != b"Map\x1a" {
        return Err(invalid("not an scmap (bad magic)"));
    }
    let major = r.i32()?;
    if major != 2 {
        return Err(invalid(format!("unsupported scmap major version {major}")));
    }
    r.take(8)?; // 0xBEEFFEED, 2
    let width = r.f32()?;
    let height = r.f32()?;
    r.take(6)?; // i32 0, i16 0
    let preview_len = r.i32()?;
    r.take(usize::try_from(preview_len).map_err(|_| invalid("negative preview length"))?)?;
    let version = r.i32()?;
    let hw = r.i32()?;
    let hh = r.i32()?;
    let scale = r.f32()?;
    if hw <= 0 || hh <= 0 {
        return Err(invalid(format!("bad heightmap size {hw}x{hh}")));
    }
    let count = (hw as usize + 1) * (hh as usize + 1);
    let raw = r.take(count * 2)?;
    let samples = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    Ok((
        ScMapHeader { version, width, height },
        Heightmap { width: hw as u32, height: hh as u32, scale, samples },
        r,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every stock map parses to exactly its end. Fails without the files
    /// (`RTS_SKIP_DATA_TESTS=1` skips).
    #[test]
    fn parses_every_stock_map_to_the_end() {
        const DIR: &str = "../original supreme commander unpacked/maps";
        if !crate::test_data::require("stock maps", &[DIR]) {
            return;
        }
        let dirs = std::fs::read_dir(DIR).expect("read maps");
        let mut n = 0;
        for d in dirs.flatten() {
            let Ok(files) = std::fs::read_dir(d.path()) else { continue };
            for f in files.flatten().filter(|f| f.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("scmap"))) {
                let b = std::fs::read(f.path()).unwrap();
                read_render(&b).unwrap_or_else(|e| panic!("{}: {e}", f.path().display()));
                n += 1;
            }
        }
        assert!(n > 10, "{n} maps");
    }

    /// Seton's Clutch's shoreline wave generators, as the editor wrote them:
    /// turbulence sprites on the waterline, 30-70 tick lives, one every
    /// 4.5-5.5 s. Fails without the file (`RTS_SKIP_DATA_TESTS=1` skips).
    /// Seton's Clutch's sky dome as the file holds it (printed by the test
    /// on failure). Fails without the file (`RTS_SKIP_DATA_TESTS=1` skips).
    #[test]
    fn reads_the_sky_box() {
        const MAP: &str = "../original supreme commander unpacked/maps/SCMP_009/SCMP_009.scmap";
        if !crate::test_data::require("SCMP_009", &[MAP]) {
            return;
        }
        let (_, m) = read_render(&std::fs::read(MAP).unwrap()).unwrap();
        let s = m.sky.expect("a v60 map's sky");
        println!("{s:#?}");
        assert!(s.scale > 0.0 && s.subdivisions.0 > 2 && s.subdivisions.1 > 0, "{s:?}");
        assert!(!s.cirrus_layers.is_empty() && s.cirrus_texture.ends_with(".dds"), "{s:?}");
    }

    #[test]
    fn reads_wave_generators() {
        const MAP: &str = "../original supreme commander unpacked/maps/SCMP_009/SCMP_009.scmap";
        if !crate::test_data::require("SCMP_009", &[MAP]) {
            return;
        }
        let (_, m) = read_render(&std::fs::read(MAP).unwrap()).unwrap();
        assert_eq!(m.wave_generators.len(), 6939);
        let w = &m.wave_generators[0];
        assert_eq!((w.texture.as_str(), w.ramp.as_str()), ("/env/common/decals/shoreline/turbulance02_albedo.dds", "/env/common/decals/shoreline/waveramptest.dds"));
        assert_eq!((w.position[0], w.position[1], w.position[2]), (0.5, 17.5, 232.5));
        assert_eq!((w.lifetime, w.period, w.frame_count, w.strip_count), ((30.0, 70.0), (4.5, 5.5), 1.0, 1.0));
        assert!((w.rotation + 2.582).abs() < 1e-3 && w.scale.0 < w.scale.1);
    }

    /// A stock FA map (v60, TTerrain) and two FAF vault maps (v56, TTerrainXP
    /// and Terrain200) parse to exactly their end. Fails without the files
    /// (`RTS_SKIP_DATA_TESTS=1` skips).
    #[test]
    fn parses_render_data_of_stock_and_faf_maps() {
        let maps = [
            ("../original supreme commander unpacked/maps/SCMP_009/SCMP_009.scmap", "TTerrain"),
            ("../faf-vault/maps/adaptive_monument_valley.v0005/adaptive_monument_valley.scmap", "TTerrainXP"),
            ("../faf-vault/maps/texture_test_map.v0003/texture_test_map.scmap", "Terrain200"),
        ];
        if !crate::test_data::require("stock and vault maps", &maps.map(|(path, _)| path)) {
            return;
        }
        for (path, shader) in maps {
            let b = std::fs::read(path).expect("read map");
            let (t, m) = read_render(&b).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(m.shader, shader);
            assert!(!m.mask_low.is_empty() && !m.normal_map.is_empty());
            eprintln!("{path}: v{} {:?} lighting {:?}", t.header.version, m.shader, m.lighting);
            eprintln!("  albedo {:?}", m.albedo.iter().map(|s| (s.path.as_str(), s.scale)).collect::<Vec<_>>());
            eprintln!("  normals {:?}", m.normals.iter().map(|s| (s.path.as_str(), s.scale)).collect::<Vec<_>>());
            eprintln!("  water {:?}", m.water.as_ref().map(|w| (&w.ramp, w.surface_color, &w.waves)));
            eprintln!("  decals {} e.g. {:?}", m.decals.len(), m.decals.first());
            eprintln!("  blobs normal {} masks {} {} water {}", m.normal_map.len(), m.mask_low.len(), m.mask_high.len(), m.water_map.len());
        }
    }
}
