//! `.dds` texture decoder for the renderer (unit albedo / specteam maps).
//!
//! FA's textures are DXT1 / DXT3 / DXT5 with mip chains, plus some 32- and
//! 24-bit uncompressed ones (some stratum normal maps). [`read`] returns the mip levels either still
//! block-compressed (for GPUs that sample BC directly) or decoded to RGBA8.
//!
//! Layout: `"DDS "`, 124-byte header (height @12, width @16, mip count @28,
//! pixel format @76: flags @80, fourCC @84, bit count @88, masks @92..108),
//! then the levels largest first, BC levels in 4x4 blocks.

use std::io;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// DXT1, 8 bytes per 4x4 block.
    Bc1,
    /// DXT3, 16 bytes per block.
    Bc2,
    /// DXT5, 16 bytes per block.
    Bc3,
    /// 4 bytes per pixel, RGBA order.
    Rgba8,
    /// 4 bytes per pixel, BGRA order: 32-bit `.dds` as stored, which
    /// [`layout`] streams as is ([`read`] swizzles them to [`Format::Rgba8`]).
    Bgra8,
}

impl Format {
    pub fn is_compressed(self) -> bool {
        matches!(self, Format::Bc1 | Format::Bc2 | Format::Bc3)
    }

    /// Bytes for one level of `w` x `h` pixels.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let blocks = (w.div_ceil(4) * h.div_ceil(4)) as usize;
        match self {
            Format::Bc1 => blocks * 8,
            Format::Bc2 | Format::Bc3 => blocks * 16,
            Format::Rgba8 | Format::Bgra8 => (w * h * 4) as usize,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub format: Format,
    /// Mip levels, largest first. BC levels keep only those at least one
    /// block (4x4) in both dimensions.
    pub levels: Vec<Vec<u8>>,
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn u32_at(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| invalid("truncated dds header"))
}

/// Decodes a `.dds`. `keep_compressed`: return BC levels as stored (the GPU
/// samples them); else every level is decoded to RGBA8.
pub fn read(b: &[u8], keep_compressed: bool) -> io::Result<Texture> {
    if b.get(0..4) != Some(b"DDS ") {
        return Err(invalid("not a dds (bad magic)"));
    }
    let height = u32_at(b, 12)?;
    let width = u32_at(b, 16)?;
    let mips = u32_at(b, 28)?.max(1);
    let pf_flags = u32_at(b, 80)?;
    let fourcc = b.get(84..88).ok_or_else(|| invalid("truncated dds header"))?;
    const FOURCC: u32 = 0x4;
    const RGB: u32 = 0x40;
    const ALPHA: u32 = 0x2;
    // 8-bit alpha only (A8: the sky's `horizonLookup.dds`), sampled as
    // black with that alpha, as D3D does.
    let alpha_only = pf_flags & (FOURCC | RGB) == 0 && pf_flags & ALPHA != 0 && u32_at(b, 88)? == 8;
    // 16-bit packed pixels (A4R4G4B4, A1R5G5B5, R5G6B5: FAF's
    // hydrocarbon marker): each channel's mask, decoded to RGBA8.
    let mut packed: Option<[u32; 4]> = None;
    let (format, swizzle) = if pf_flags & FOURCC != 0 {
        match fourcc {
            b"DXT1" => (Format::Bc1, None),
            b"DXT2" | b"DXT3" => (Format::Bc2, None),
            b"DXT4" | b"DXT5" => (Format::Bc3, None),
            _ => return Err(invalid("unsupported dds fourCC")),
        }
    } else if alpha_only {
        (Format::Rgba8, None)
    } else if pf_flags & RGB != 0 && u32_at(b, 88)? == 16 {
        packed = Some([u32_at(b, 92)?, u32_at(b, 96)?, u32_at(b, 100)?, u32_at(b, 104)?]);
        (Format::Rgba8, None)
    } else if pf_flags & RGB != 0 && matches!(u32_at(b, 88)?, 24 | 32) {
        // Byte position of each RGBA channel from its mask (alpha may be absent).
        let shift = |m: u32| if m == 0 { None } else { Some((m.trailing_zeros() / 8) as usize) };
        let (r, g, bl, a) = (u32_at(b, 92)?, u32_at(b, 96)?, u32_at(b, 100)?, u32_at(b, 104)?);
        match (shift(r), shift(g), shift(bl)) {
            (Some(r), Some(g), Some(bl)) => (Format::Rgba8, Some([r, g, bl, shift(a).unwrap_or(usize::MAX)])),
            _ => return Err(invalid("unsupported dds pixel masks")),
        }
    } else {
        return Err(invalid("unsupported dds pixel format"));
    };
    if width == 0 || height == 0 {
        return Err(invalid("empty dds"));
    }
    // Source bytes per pixel of uncompressed data.
    let bpp = if format == Format::Rgba8 { (u32_at(b, 88)? / 8) as usize } else { 4 };
    // A packed channel scaled to 0..255 (absent: opaque / 0).
    let channel = |v: u32, m: u32, absent: u8| -> u8 {
        if m == 0 {
            return absent;
        }
        let max = m >> m.trailing_zeros();
        (((v & m) >> m.trailing_zeros()) * 255 / max) as u8
    };
    let mut at = 128;
    let mut levels = Vec::with_capacity(mips as usize);
    for l in 0..mips {
        let (w, h) = ((width >> l).max(1), (height >> l).max(1));
        if format.is_compressed() && (w < 4 || h < 4) && l > 0 {
            break;
        }
        let size = if format == Format::Rgba8 { (w * h) as usize * bpp } else { format.level_size(w, h) };
        let Some(src) = b.get(at..at + size) else { break };
        at += size;
        levels.push(match (format, swizzle) {
            (Format::Rgba8, None) if alpha_only => src.iter().flat_map(|&a| [0, 0, 0, a]).collect(),
            (Format::Rgba8, None) if packed.is_some() => {
                let [r, g, bl, a] = packed.unwrap_or_default();
                src.chunks_exact(2)
                    .flat_map(|p| {
                        let v = u16::from_le_bytes([p[0], p[1]]) as u32;
                        [channel(v, r, 0), channel(v, g, 0), channel(v, bl, 0), channel(v, a, 255)]
                    })
                    .collect()
            }
            (Format::Rgba8, Some([r, g, bl, a])) => src
                .chunks_exact(bpp)
                .flat_map(|p| [p[r], p[g], p[bl], if a < bpp { p[a] } else { 255 }])
                .collect(),
            (_, _) if keep_compressed => src.to_vec(),
            (f, _) => {
                let tf = match f {
                    Format::Bc1 => texpresso::Format::Bc1,
                    Format::Bc2 => texpresso::Format::Bc2,
                    _ => texpresso::Format::Bc3,
                };
                let mut out = vec![0u8; (w * h * 4) as usize];
                tf.decompress(src, w as usize, h as usize, &mut out);
                out
            }
        });
    }
    if levels.is_empty() {
        return Err(invalid("truncated dds data"));
    }
    let format = if keep_compressed { format } else { Format::Rgba8 };
    Ok(Texture { width, height, format, levels })
}

/// `t` as a `.dds` file, all its levels: BC as DXT1/3/5, RGBA and BGRA as
/// 32-bit BGRA (FAF's mask order, so [`layout`] streams it). [`read`] of the
/// result gives `t` back.
pub fn write(t: &Texture) -> Vec<u8> {
    let mut h = vec![0u8; 128];
    let mut put = |at: usize, v: u32| h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT.
    put(4, 124);
    put(8, 0x2_1007);
    put(12, t.height);
    put(16, t.width);
    put(28, t.levels.len() as u32);
    put(76, 32);
    let fourcc = match t.format {
        Format::Bc1 => Some(b"DXT1"),
        Format::Bc2 => Some(b"DXT3"),
        Format::Bc3 => Some(b"DXT5"),
        Format::Rgba8 | Format::Bgra8 => None,
    };
    match fourcc {
        Some(f) => {
            put(80, 0x4);
            put(84, u32::from_le_bytes(*f));
        }
        None => {
            // RGB | ALPHAPIXELS, 32 bits, BGRA in memory.
            put(80, 0x41);
            put(88, 32);
            put(92, 0xFF_0000);
            put(96, 0xFF00);
            put(100, 0xFF);
            put(104, 0xFF00_0000);
        }
    }
    // COMPLEX | TEXTURE | MIPMAP.
    put(108, 0x40_1008);
    h[..4].copy_from_slice(b"DDS ");
    for level in &t.levels {
        if t.format == Format::Rgba8 {
            h.extend(level.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]));
        } else {
            h.extend_from_slice(level);
        }
    }
    h
}

/// The levels of a `.dds` the GPU samples as stored, from its header alone,
/// so a loader can stream them straight into GPU memory instead of through
/// [`read`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    /// Block-compressed, or [`Format::Bgra8`].
    pub format: Format,
    /// Mip levels, largest first, as [`read`] keeps them: (width, height,
    /// bytes), following the 128-byte header back to back.
    pub levels: Vec<(u32, u32, usize)>,
}

/// The layout of a `.dds` from its first 128 bytes: DXT1/3/5, or 32-bit
/// with alpha in BGRA order (FAF's uncompressed unit textures); `None` for
/// any other pixel format (those are decoded by [`read`]).
pub fn layout(header: &[u8]) -> io::Result<Option<Layout>> {
    if header.get(0..4) != Some(b"DDS ") {
        return Err(invalid("not a dds (bad magic)"));
    }
    const FOURCC: u32 = 0x4;
    const RGB: u32 = 0x40;
    let flags = u32_at(header, 80)?;
    let format = if flags & FOURCC != 0 {
        match header.get(84..88).ok_or_else(|| invalid("truncated dds header"))? {
            b"DXT1" => Format::Bc1,
            b"DXT2" | b"DXT3" => Format::Bc2,
            b"DXT4" | b"DXT5" => Format::Bc3,
            _ => return Ok(None),
        }
    } else if flags & RGB != 0 && u32_at(header, 88)? == 32 && (92..108).step_by(4).map(|at| u32_at(header, at)).collect::<io::Result<Vec<_>>>()? == [0xFF_0000, 0xFF00, 0xFF, 0xFF00_0000] {
        Format::Bgra8
    } else {
        return Ok(None);
    };
    let (height, width, mips) = (u32_at(header, 12)?, u32_at(header, 16)?, u32_at(header, 28)?.max(1));
    if width == 0 || height == 0 {
        return Err(invalid("empty dds"));
    }
    let levels = (0..mips)
        .map(|l| ((width >> l).max(1), (height >> l).max(1)))
        .enumerate()
        .take_while(|&(l, (w, h))| l == 0 || !format.is_compressed() || (w >= 4 && h >= 4))
        .map(|(_, (w, h))| (w, h, format.level_size(w, h)))
        .collect();
    Ok(Some(Layout { width, height, format, levels }))
}


/// A UI image: a `.dds` (decoded to RGBA), or a `.png` (FAF's interface
/// uses a few, e.g. the hotbuild key label background) as one RGBA level.
pub fn read_image(b: &[u8]) -> io::Result<Texture> {
    if b.starts_with(b"\x89PNG") {
        return read_png(b);
    }
    read(b, false)
}

/// Width and height from an image's header, without decoding it.
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    let word = |o: usize, be: bool| b.get(o..o + 4).map(|s| if be { u32::from_be_bytes(s.try_into().unwrap()) } else { u32::from_le_bytes(s.try_into().unwrap()) });
    if b.starts_with(b"DDS ") {
        return Some((word(16, false)?, word(12, false)?));
    }
    // PNG: the IHDR chunk comes first, width then height, big-endian.
    if b.starts_with(b"\x89PNG") {
        return Some((word(16, true)?, word(20, true)?));
    }
    None
}

fn read_png(b: &[u8]) -> io::Result<Texture> {
    let mut decoder = png::Decoder::new(io::Cursor::new(b));
    // Palette, grey and 16-bit images come out as 8-bit RGB(A).
    decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or_else(|| invalid("png too large"))?];
    let info = reader.next_frame(&mut buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    buf.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        other => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unexpected png colour type {other:?}"))),
    };
    Ok(Texture { width: info.width, height: info.height, format: Format::Rgba8, levels: vec![rgba] })
}

/// A cube map's six faces (+x, -x, +y, -y, +z, -z), each with its mips;
/// `None` if `b` is not a cube map.
pub fn read_cube(b: &[u8], keep_compressed: bool) -> io::Result<Option<[Texture; 6]>> {
    const CUBEMAP: u32 = 0x200;
    if b.get(0..4) != Some(b"DDS ") || u32_at(b, 112)? & CUBEMAP == 0 {
        return Ok(None);
    }
    // Faces follow one another, each a full mip chain: read the first as a
    // 2D texture to learn the layout, then step through the rest.
    let first = read(b, true)?;
    let mips = u32_at(b, 28)?.max(1);
    let face_size: usize = (0..mips).map(|l| first.format.level_size((first.width >> l).max(1), (first.height >> l).max(1))).sum();
    let mut faces = Vec::with_capacity(6);
    for f in 0..6 {
        let start = 128 + f * face_size;
        let face = b.get(start..start + face_size).ok_or_else(|| invalid("truncated dds cube map"))?;
        let mut one = b[..128].to_vec();
        one[112..116].copy_from_slice(&0u32.to_le_bytes());
        one.extend_from_slice(face);
        faces.push(read(&one, keep_compressed)?);
    }
    Ok(faces.try_into().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: u32, h: u32, mips: u32, fourcc: &[u8; 4]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"DDS ");
        b[12..16].copy_from_slice(&h.to_le_bytes());
        b[16..20].copy_from_slice(&w.to_le_bytes());
        b[28..32].copy_from_slice(&mips.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(fourcc);
        b
    }

    /// A4R4G4B4 (FAF's hydrocarbon marker) decodes to RGBA8.
    #[test]
    fn packed_16_bit_decodes_by_mask() {
        let mut b = header(2, 1, 1, b"\0\0\0\0");
        b[80..84].copy_from_slice(&0x41u32.to_le_bytes());
        b[88..92].copy_from_slice(&16u32.to_le_bytes());
        for (i, m) in [0x0f00u32, 0x00f0, 0x000f, 0xf000].iter().enumerate() {
            b[92 + i * 4..96 + i * 4].copy_from_slice(&m.to_le_bytes());
        }
        // ARGB 0xF0F8 (opaque, r 0, g 255, b 136); 0x0000 (all clear).
        b.extend_from_slice(&[0xF8, 0xF0, 0x00, 0x00]);
        let t = read(&b, true).unwrap();
        assert_eq!(t.format, Format::Rgba8);
        assert_eq!(t.levels[0], vec![0, 255, 136, 255, 0, 0, 0, 0]);
    }

    /// A8 (the sky's horizon lookup) decodes to black with that alpha.
    #[test]
    fn alpha_only_decodes_to_black_with_alpha() {
        let mut b = header(2, 1, 1, b"\0\0\0\0");
        b[80..84].copy_from_slice(&0x2u32.to_le_bytes());
        b[88..92].copy_from_slice(&8u32.to_le_bytes());
        b[104..108].copy_from_slice(&0xffu32.to_le_bytes());
        b.extend_from_slice(&[0x40, 0xff]);
        let t = read(&b, true).unwrap();
        assert_eq!(t.format, Format::Rgba8);
        assert_eq!(t.levels[0], vec![0, 0, 0, 0x40, 0, 0, 0, 0xff]);
    }

    #[test]
    fn dxt1_levels_stop_below_a_block_and_decode_to_rgba() {
        let mut b = header(8, 8, 4, b"DXT1");
        // 8x8: 4 blocks; 4x4: 1 block (2x2 and 1x1 dropped). Solid red:
        // colour0 = colour1 = 0xF800, indices 0.
        for _ in 0..5 {
            b.extend_from_slice(&[0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0]);
        }
        let t = read(&b, true).unwrap();
        assert_eq!((t.format, t.levels.len(), t.levels[0].len(), t.levels[1].len()), (Format::Bc1, 2, 32, 8));
        let t = read(&b, false).unwrap();
        assert_eq!((t.format, t.levels.len(), t.levels[0].len()), (Format::Rgba8, 2, 256));
        assert_eq!(&t.levels[1][0..4], &[255, 0, 0, 255]);
    }

    /// The streamed layout keeps exactly the levels `read` keeps, at the
    /// same sizes, and declines what `read` decodes.
    #[test]
    fn layout_matches_read() {
        for (w, h, mips, fourcc) in [(8, 8, 4, b"DXT1"), (16, 8, 5, b"DXT5"), (64, 64, 7, b"DXT3"), (4, 4, 1, b"DXT1"), (32, 16, 1, b"DXT5")] {
            let mut b = header(w, h, mips, fourcc);
            let data: usize = (0..mips).map(|l| Format::Bc3.level_size((w >> l).max(1), (h >> l).max(1))).sum();
            b.extend(std::iter::repeat_n(0u8, data));
            let layout = layout(&b[..128]).unwrap().expect("a BC layout");
            let t = read(&b, true).unwrap();
            assert_eq!((layout.width, layout.height, layout.format), (t.width, t.height, t.format));
            assert_eq!(layout.levels.iter().map(|l| l.2).collect::<Vec<_>>(), t.levels.iter().map(Vec::len).collect::<Vec<_>>(), "{w}x{h} {mips} {fourcc:?}");
        }
        // 32-bit BGRA: every level, as stored; other masks are decoded.
        let mut bgra = bgra_header(8, 2, 4);
        bgra.extend(std::iter::repeat_n(0u8, (8 * 2 + 4 + 2 + 1) * 4));
        let l = layout(&bgra[..128]).unwrap().expect("a BGRA layout");
        assert_eq!((l.format, l.levels.clone()), (Format::Bgra8, vec![(8, 2, 64), (4, 1, 16), (2, 1, 8), (1, 1, 4)]));
        assert_eq!(l.levels.iter().map(|l| l.2).collect::<Vec<_>>(), read(&bgra, true).unwrap().levels.iter().map(Vec::len).collect::<Vec<_>>());
        bgra[104..108].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(layout(&bgra[..128]).unwrap(), None, "no alpha: decoded with alpha 255");
        let mut packed = header(1, 1, 1, b"\0\0\0\0");
        packed[80..84].copy_from_slice(&(0x40u32 | 0x1).to_le_bytes());
        packed[88..92].copy_from_slice(&16u32.to_le_bytes());
        assert_eq!(layout(&packed[..128]).unwrap(), None);
    }

    #[test]
    fn write_round_trips_through_read_and_layout() {
        let rgba = Texture { width: 2, height: 2, format: Format::Rgba8, levels: vec![(0..16).collect(), vec![9, 8, 7, 6]] };
        let b = write(&rgba);
        let back = read(&b, true).unwrap();
        assert_eq!((back.width, back.height, back.format, back.levels), (2, 2, Format::Rgba8, rgba.levels.clone()));
        assert_eq!(layout(&b[..128]).unwrap().unwrap().format, Format::Bgra8);
        let bc = Texture { width: 8, height: 4, format: Format::Bc3, levels: vec![vec![7; Format::Bc3.level_size(8, 4)]] };
        let back = read(&write(&bc), true).unwrap();
        assert_eq!((back.format, back.levels), (Format::Bc3, bc.levels));
    }

    fn bgra_header(w: u32, h: u32, mips: u32) -> Vec<u8> {
        let mut b = header(w, h, mips, b"\0\0\0\0");
        b[80..84].copy_from_slice(&(0x40u32 | 0x1).to_le_bytes());
        b[88..92].copy_from_slice(&32u32.to_le_bytes());
        for (at, m) in [(92, 0x00FF_0000u32), (96, 0xFF00), (100, 0xFF), (104, 0xFF00_0000)] {
            b[at..at + 4].copy_from_slice(&m.to_le_bytes());
        }
        b
    }

    #[test]
    fn cube_maps_split_into_six_faces() {
        let mut b = header(4, 4, 1, b"DXT1");
        b[112..116].copy_from_slice(&0xFE00u32.to_le_bytes());
        for f in 0..6u8 {
            b.extend_from_slice(&[f, 0, f, 0, 0, 0, 0, 0]);
        }
        let faces = read_cube(&b, true).unwrap().unwrap();
        assert_eq!(faces[5].levels[0][0], 5);
        assert!(read_cube(&header(4, 4, 1, b"DXT1"), true).unwrap().is_none());
    }

    #[test]
    fn uncompressed_bgra_is_swizzled_to_rgba() {
        let mut b = bgra_header(1, 1, 1);
        b.extend_from_slice(&[10, 20, 30, 40]); // B G R A
        let t = read(&b, true).unwrap();
        assert_eq!(t.levels[0], vec![30, 20, 10, 40]);
    }

    /// A strategic icon decodes with its transparent texels at alpha 0 (the
    /// textures carry grey colour under them: the selected frame's
    /// footprint, which must not show through).
    #[test]
    fn strategic_icon_decodes_with_a_transparent_border() {
        let candidates = ["../fa", "../../../../fa"];
        let Some(bytes) = candidates.iter().find_map(|d| std::fs::read(format!("{d}/textures/ui/common/game/strategicicons/icon_land1_directfire_rest.dds")).ok()) else {
            eprintln!("no FAF checkout beside the repo: skipped");
            return;
        };
        let t = read_image(&bytes).unwrap();
        assert_eq!((t.width, t.height), (16, 16));
        let px = &t.levels[0];
        let alpha = |x: u32, y: u32| px[((y * t.width + x) * 4 + 3) as usize];
        let border: Vec<u8> = (0..16).flat_map(|i| [alpha(i, 0), alpha(i, 15), alpha(0, i), alpha(15, i)]).collect();
        // The diamond's tip touches the top edge and its stem the bottom:
        // four opaque border texels, every other one transparent.
        assert_eq!(border.iter().filter(|&&a| a > 0).count(), 4, "border alphas: {border:?}");
        assert!(border.iter().all(|&a| a == 0 || a == 255), "border alphas: {border:?}");
        // Inside the border, the grey-under-alpha texels beside the diamond.
        for (x, y) in [(2, 1), (12, 1), (2, 11), (12, 11), (3, 3)] {
            assert_eq!(alpha(x, y), 0, "texel ({x}, {y})");
        }
    }
}
