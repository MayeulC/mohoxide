//! Filling in a texture's missing mip levels.
//!
//! The real game loads every texture with `D3DXCreateTextureFromFileInMemoryEx`
//! and `MipLevels = D3DX_DEFAULT` (gpggal.dll), so D3DX builds whatever part
//! of the chain the file lacks. Hundreds of FA's own textures ship without
//! one (unit normal maps, decals, effects, the map's strata masks), and
//! without mips a texture drawn small shimmers.
//!
//! Each new level is a 2x2 box filter of the one above, kept in `f32` from
//! the last stored level down (so rounding doesn't build up), in linear
//! light for sRGB textures (else in the stored values, as D3DX does). BC
//! textures are decoded, filtered and compressed again.

use super::dds::{Format, Texture};

/// Levels in a full chain, as [`super::dds::read`] keeps them: down to 1x1,
/// or for BC to the last level at least one block (4x4) in both dimensions.
pub fn full_chain(format: Format, width: u32, height: u32) -> usize {
    let below = |l: u32| ((width >> l).max(1), (height >> l).max(1));
    1 + (1..32).take_while(|&l| width.max(height) >> l > 0 && (!format.is_compressed() || { let (w, h) = below(l); w >= 4 && h >= 4 })).count()
}

/// Adds the levels `t` lacks below its last one. Returns how many it added.
pub fn complete(t: &mut Texture, srgb: bool) -> usize {
    let (have, want) = (t.levels.len(), full_chain(t.format, t.width, t.height));
    if have == 0 || have >= want {
        return 0;
    }
    let last = have as u32 - 1;
    let (mut w, mut h) = ((t.width >> last).max(1), (t.height >> last).max(1));
    let to_linear = srgb_to_linear();
    // Channels 0..3 are colour in both RGBA and BGRA; 3 is alpha.
    let mut px: Vec<[f32; 4]> = decode(t.format, &t.levels[have - 1], w, h)
        .chunks_exact(4)
        .map(|p| {
            let c = |v: u8| if srgb { to_linear[v as usize] } else { v as f32 / 255.0 };
            [c(p[0]), c(p[1]), c(p[2]), p[3] as f32 / 255.0]
        })
        .collect();
    for _ in have..want {
        let (nw, nh) = ((w >> 1).max(1), (h >> 1).max(1));
        px = halve(&px, w, h, nw, nh);
        (w, h) = (nw, nh);
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let c = |v: f32| if srgb { q(linear_to_srgb(v)) } else { q(v) };
        let bytes: Vec<u8> = px.iter().flat_map(|p| [c(p[0]), c(p[1]), c(p[2]), q(p[3])]).collect();
        t.levels.push(encode(t.format, &bytes, w, h));
    }
    want - have
}

/// One level down: each texel the mean of the (up to) 2x2 above it.
fn halve(src: &[[f32; 4]], w: u32, h: u32, nw: u32, nh: u32) -> Vec<[f32; 4]> {
    let (w, h) = (w as usize, h as usize);
    let mut out = Vec::with_capacity((nw * nh) as usize);
    for y in 0..nh as usize {
        let ys = [(2 * y).min(h - 1), (2 * y + 1).min(h - 1)];
        for x in 0..nw as usize {
            let xs = [(2 * x).min(w - 1), (2 * x + 1).min(w - 1)];
            let mut sum = [0.0f32; 4];
            for &sy in &ys {
                for &sx in &xs {
                    let p = src[sy * w + sx];
                    for c in 0..4 {
                        sum[c] += p[c];
                    }
                }
            }
            out.push(sum.map(|v| v * 0.25));
        }
    }
    out
}

fn texpresso_format(f: Format) -> Option<texpresso::Format> {
    match f {
        Format::Bc1 => Some(texpresso::Format::Bc1),
        Format::Bc2 => Some(texpresso::Format::Bc2),
        Format::Bc3 => Some(texpresso::Format::Bc3),
        Format::Rgba8 | Format::Bgra8 => None,
    }
}

fn decode(f: Format, level: &[u8], w: u32, h: u32) -> Vec<u8> {
    match texpresso_format(f) {
        Some(tf) => {
            let mut out = vec![0u8; (w * h * 4) as usize];
            tf.decompress(level, w as usize, h as usize, &mut out);
            out
        }
        None => level.to_vec(),
    }
}

fn encode(f: Format, rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    match texpresso_format(f) {
        Some(tf) => {
            let mut out = vec![0u8; tf.compressed_size(w as usize, h as usize)];
            let params = texpresso::Params { algorithm: texpresso::Algorithm::ClusterFit, ..Default::default() };
            tf.compress(rgba, w as usize, h as usize, params, &mut out);
            out
        }
        None => rgba.to_vec(),
    }
}

fn srgb_to_linear() -> &'static [f32; 256] {
    static LUT: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let v = i as f32 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        })
    })
}

fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_chain_stops_at_one_texel_or_one_block() {
        assert_eq!(full_chain(Format::Rgba8, 256, 256), 9);
        assert_eq!(full_chain(Format::Rgba8, 256, 64), 9);
        assert_eq!(full_chain(Format::Rgba8, 1, 1), 1);
        assert_eq!(full_chain(Format::Bc1, 256, 256), 7);
        assert_eq!(full_chain(Format::Bc3, 256, 512), 7);
        assert_eq!(full_chain(Format::Bc3, 4, 4), 1);
    }

    #[test]
    fn rgba_chain_is_box_filtered_to_one_texel() {
        // 2x2: black, white, black, white (alpha 255, 0, 255, 0).
        let level = [0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255, 0].to_vec();
        let mut t = Texture { width: 2, height: 2, format: Format::Rgba8, levels: vec![level] };
        assert_eq!(complete(&mut t, false), 1);
        assert_eq!(t.levels[1], vec![128, 128, 128, 128]);
        // In linear light, half white is brighter than value 128.
        let mut t = Texture { levels: vec![t.levels[0].clone()], ..t };
        complete(&mut t, true);
        assert_eq!(t.levels[1], vec![188, 188, 188, 128]);
    }

    #[test]
    fn complete_chains_are_left_alone_and_partial_ones_extended() {
        let mut t = Texture { width: 2, height: 1, format: Format::Rgba8, levels: vec![vec![0; 8], vec![0; 4]] };
        assert_eq!(complete(&mut t, false), 0);
        let mut t = Texture { width: 4, height: 4, format: Format::Rgba8, levels: vec![vec![200; 64], vec![100; 16]] };
        assert_eq!(complete(&mut t, false), 1);
        assert_eq!(t.levels[2], vec![100; 4]);
    }

    #[test]
    fn bc_levels_are_recompressed_at_the_right_size() {
        let mut t = Texture { width: 16, height: 8, format: Format::Bc3, levels: vec![vec![0; Format::Bc3.level_size(16, 8)]] };
        assert_eq!(complete(&mut t, false), 1);
        assert_eq!(t.levels[1].len(), Format::Bc3.level_size(8, 4));
    }
}
