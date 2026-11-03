//! UI fonts: FA's TrueType fonts from the VFS (`/fonts/*.ttf`), by the
//! family names FAF's UI asks for. Metrics feed the text controls' layout
//! LazyVars (`TextAdvance`, `FontAscent`, `FontDescent`); the renderer
//! rasterises glyphs from the same fonts.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};

use crate::engine::vfs::Vfs;

/// The file for a family name (unknown families fall back to Arial).
pub fn file_for(family: &str) -> &'static str {
    match family.to_ascii_lowercase().as_str() {
        "arial bold" | "arial gras" | "arialbd" => "/fonts/arialbd.ttf",
        "arial black" => "/fonts/ariblk.ttf",
        "arial italic" => "/fonts/ariali.ttf",
        "arial bold italic" => "/fonts/arialbi.ttf",
        "arial narrow" => "/fonts/arialn.ttf",
        "arial narrow bold" => "/fonts/arialnb.ttf",
        "arial rounded mt bold" | "arial rounded" => "/fonts/arlrdbd.ttf",
        "butterbelly" => "/fonts/butterbe.ttf",
        "vdub" => "/fonts/vdub.ttf",
        "wintermute" => "/fonts/wintermu.ttf",
        "zeroes three" | "zeroes 3" => "/fonts/zeroes_3.ttf",
        _ => "/fonts/arial.ttf",
    }
}

/// Pixel height of a point size (FA draws a size-n font about n pixels high).
pub fn px(pointsize: f32) -> PxScale {
    PxScale::from(pointsize.max(1.0) * 1.25)
}

/// Loaded fonts by file (lookups only).
pub struct Fonts {
    vfs: Arc<RwLock<Vfs>>,
    loaded: HashMap<&'static str, Option<FontArc>>,
}

/// One line's metrics at a size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub ascent: f32,
    pub descent: f32,
    pub advance: f32,
    /// Extra space between lines (the font's line gap).
    pub leading: f32,
}

impl Fonts {
    pub fn new(vfs: Arc<RwLock<Vfs>>) -> Self {
        Self { vfs, loaded: HashMap::new() }
    }

    pub fn font(&mut self, family: &str) -> Option<FontArc> {
        self.font_file(file_for(family))
    }

    /// The font in VFS file `file` (one of [`file_for`]'s).
    pub fn font_file(&mut self, file: &'static str) -> Option<FontArc> {
        self.loaded
            .entry(file)
            .or_insert_with(|| {
                let bytes = self.vfs.read().ok()?.read(file).ok()?;
                match FontArc::try_from_vec(bytes) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        tracing::warn!(file, "font failed to load: {e}");
                        None
                    }
                }
            })
            .clone()
    }

    /// Ascent, descent (positive) and the advance of `text` in pixels.
    pub fn measure(&mut self, family: &str, pointsize: f32, text: &str) -> Metrics {
        let Some(font) = self.font(family) else { return Metrics { ascent: pointsize, descent: pointsize * 0.25, advance: text.chars().count() as f32 * pointsize * 0.5, leading: 0.0 } };
        let f = font.as_scaled(px(pointsize));
        let mut advance = 0.0;
        let mut prev = None;
        for c in text.chars() {
            let g = f.glyph_id(c);
            if let Some(p) = prev {
                advance += f.kern(p, g);
            }
            advance += f.h_advance(g);
            prev = Some(g);
        }
        Metrics { ascent: f.ascent(), descent: -f.descent(), advance, leading: f.line_gap() }
    }
}
