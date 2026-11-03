//! MPEG-1 video: SupCom's movies (`/movies/*.sfd`).
//!
//! An `.sfd` is CRI Sofdec's container, which is a plain MPEG-1 program
//! stream: one MPEG-1 video stream (0xE0) plus Sofdec's private streams
//! (0xBE / 0xBF, skipped). There is no audio in them: the game plays the
//! soundtrack and voice from sound banks (FAF's `Movie(parent, file, sound,
//! voice)`).
//!
//! Decoded by pl_mpeg (`vendor/pl_mpeg`, C, built by `build.rs`), which
//! reads through callbacks into a Rust reader, so a movie streams from its
//! file (pl_mpeg buffers 128 KB) instead of being loaded whole.
//!
//! Render-side only: nothing here may feed the sim (decoding is floating
//! point and paced by wall time).

use std::ffi::{c_int, c_uint, c_void};
use std::io::{Read, Seek, SeekFrom};

#[repr(C)]
struct PlmPlane {
    width: c_uint,
    height: c_uint,
    data: *mut u8,
}

#[repr(C)]
struct PlmFrame {
    time: f64,
    width: c_uint,
    height: c_uint,
    y: PlmPlane,
    cr: PlmPlane,
    cb: PlmPlane,
}

/// `plm_rs_source` in `vendor/pl_mpeg/plm_rs.c`.
#[repr(C)]
struct RsSource {
    read: extern "C" fn(*mut c_void, *mut u8, usize) -> usize,
    seek: extern "C" fn(*mut c_void, usize),
    tell: extern "C" fn(*mut c_void) -> usize,
    user: *mut c_void,
}

#[repr(C)]
struct Plm {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    fn plm_rs_create(src: *mut RsSource, length: usize) -> *mut Plm;
    fn plm_destroy(plm: *mut Plm);
    fn plm_get_width(plm: *mut Plm) -> c_int;
    fn plm_get_height(plm: *mut Plm) -> c_int;
    fn plm_get_framerate(plm: *mut Plm) -> f64;
    fn plm_get_duration(plm: *mut Plm) -> f64;
    fn plm_rewind(plm: *mut Plm);
    fn plm_decode_video(plm: *mut Plm) -> *mut PlmFrame;
}

/// What a movie reads from: a file, or bytes in memory.
pub trait Source: Read + Seek + Send {}
impl<T: Read + Seek + Send> Source for T {}

type Reader = Box<dyn Source>;

extern "C" fn source_read(user: *mut c_void, dst: *mut u8, len: usize) -> usize {
    // SAFETY: `user` is the `Reader` the decoder owns (`Mpeg::reader`), and
    // pl_mpeg passes `len` writable bytes at `dst`.
    let reader = unsafe { &mut *(user as *mut Reader) };
    let buf = unsafe { std::slice::from_raw_parts_mut(dst, len) };
    loop {
        match reader.read(buf) {
            Ok(n) => return n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return 0,
        }
    }
}

extern "C" fn source_seek(user: *mut c_void, offset: usize) {
    let reader = unsafe { &mut *(user as *mut Reader) };
    let _ = reader.seek(SeekFrom::Start(offset as u64));
}

extern "C" fn source_tell(user: *mut c_void) -> usize {
    let reader = unsafe { &mut *(user as *mut Reader) };
    reader.stream_position().map_or(0, |p| p as usize)
}

/// A decoded picture: the three planes of YCbCr 4:2:0 (BT.601, studio
/// range), each `stride` wide (rounded up to whole macroblocks, so wider
/// and taller than the picture: `width` x `height` is what's shown).
#[derive(Clone, Debug, Default)]
pub struct Planes {
    pub width: u32,
    pub height: u32,
    /// Luma plane size; the chroma planes are half each way.
    pub plane_width: u32,
    pub plane_height: u32,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
}

impl Planes {
    /// Chroma plane size.
    pub fn chroma_size(&self) -> (u32, u32) {
        (self.plane_width / 2, self.plane_height / 2)
    }

    /// RGB (8-bit, as pl_mpeg's `plm_frame_to_rgb`: BT.601 studio range)
    /// of the shown pixel at (x, y). For tests and tools; the renderer
    /// converts on the GPU.
    pub fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let (cw, _) = self.chroma_size();
        let luma = self.y[(y * self.plane_width + x) as usize] as f32;
        let ci = ((y / 2) * cw + x / 2) as usize;
        let (cb, cr) = (self.cb[ci] as f32 - 128.0, self.cr[ci] as f32 - 128.0);
        let l = (luma - 16.0) * 1.164;
        let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        [c(l + 1.596 * cr), c(l - 0.391 * cb - 0.813 * cr), c(l + 2.018 * cb)]
    }
}

/// An open movie. Decodes frames in order; [`Mpeg::rewind`] starts over.
pub struct Mpeg {
    plm: *mut Plm,
    /// Kept alive (and at a fixed address) for pl_mpeg's callbacks.
    _source: Box<RsSource>,
    _reader: Box<Reader>,
    width: u32,
    height: u32,
    frame_rate: f64,
    duration: f64,
}

// SAFETY: the decoder state is owned by this value alone (no globals in
// pl_mpeg), and the reader is `Send`.
unsafe impl Send for Mpeg {}

impl Drop for Mpeg {
    fn drop(&mut self) {
        unsafe { plm_destroy(self.plm) };
    }
}

impl Mpeg {
    /// Opens a movie from `reader`. `None` unless it's an MPEG-1 program
    /// stream with a video stream.
    pub fn open(reader: impl Source + 'static) -> Option<Self> {
        let mut reader: Box<Reader> = Box::new(Box::new(reader));
        let length = reader.seek(SeekFrom::End(0)).ok()?;
        reader.seek(SeekFrom::Start(0)).ok()?;
        let mut source = Box::new(RsSource { read: source_read, seek: source_seek, tell: source_tell, user: &mut *reader as *mut Reader as *mut c_void });
        let plm = unsafe { plm_rs_create(&mut *source, length as usize) };
        if plm.is_null() {
            return None;
        }
        let (width, height, frame_rate) = unsafe { (plm_get_width(plm), plm_get_height(plm), plm_get_framerate(plm)) };
        // Reads the last timestamp near the end of the file, then rewinds.
        let duration = unsafe { plm_get_duration(plm) };
        let m = Self { plm, _source: source, _reader: reader, width: width.max(0) as u32, height: height.max(0) as u32, frame_rate, duration };
        (m.width > 0 && m.height > 0 && m.frame_rate > 0.0).then_some(m)
    }

    /// Opens the movie file at `path` (streamed, not read whole).
    pub fn open_file(path: &std::path::Path) -> Option<Self> {
        Self::open(std::fs::File::open(path).ok()?)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Frames per second (SupCom's: 30, or 29.97 for the talking heads).
    pub fn frame_rate(&self) -> f64 {
        self.frame_rate
    }

    /// Seconds from the first frame's start to the last frame's end.
    pub fn duration(&self) -> f64 {
        self.duration + 1.0 / self.frame_rate
    }

    /// Frames in the movie (from its timestamps; the last frame starts at
    /// the last timestamp).
    pub fn frame_count(&self) -> u32 {
        (self.duration * self.frame_rate).round() as u32 + 1
    }

    /// Decodes the next frame into `out` (its buffers reused). `false` at
    /// the end (or on data it can't decode).
    pub fn decode(&mut self, out: &mut Planes) -> bool {
        let frame = unsafe { plm_decode_video(self.plm) };
        if frame.is_null() {
            return false;
        }
        // SAFETY: valid until the next decode; each plane is width x height
        // bytes.
        let f = unsafe { &*frame };
        let copy = |p: &PlmPlane, dst: &mut Vec<u8>| {
            let n = (p.width * p.height) as usize;
            dst.clear();
            if !p.data.is_null() {
                dst.extend_from_slice(unsafe { std::slice::from_raw_parts(p.data, n) });
            }
        };
        out.width = f.width;
        out.height = f.height;
        out.plane_width = f.y.width;
        out.plane_height = f.y.height;
        copy(&f.y, &mut out.y);
        copy(&f.cb, &mut out.cb);
        copy(&f.cr, &mut out.cr);
        true
    }

    /// Back to the first frame.
    pub fn rewind(&mut self) {
        unsafe { plm_rewind(self.plm) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The FA install's movies, next to the repo (`docs/architecture/vfs-assets.md`).
    /// Fails the test without them; `None` only when skipping
    /// (`RTS_SKIP_DATA_TESTS=1`).
    pub(crate) fn movie(name: &str) -> Option<std::path::PathBuf> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Supreme Commander Forged Alliance/movies").join(name);
        // Worktrees sit three levels further down.
        let q = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../Supreme Commander Forged Alliance/movies").join(name);
        if q.exists() && !p.exists() {
            return Some(q);
        }
        crate::test_data::require("FA movies", &[&p]).then_some(p)
    }

    #[test]
    fn not_a_movie_does_not_open() {
        assert!(Mpeg::open(std::io::Cursor::new(vec![0u8; 4096])).is_none());
        assert!(Mpeg::open(std::io::Cursor::new(Vec::new())).is_none());
    }

    #[test]
    fn talking_head_decodes_every_frame_and_rewinds() {
        let Some(path) = movie("Abasi.sfd") else { return };
        let mut m = Mpeg::open_file(&path).expect("open");
        assert_eq!((m.width(), m.height()), (192, 192));
        assert!((m.frame_rate() - 29.97).abs() < 1e-6);
        let mut p = Planes::default();
        let mut n = 0;
        while m.decode(&mut p) {
            n += 1;
        }
        assert_eq!(n, m.frame_count(), "frames decoded vs counted from the timestamps");
        assert_eq!((p.width, p.height, p.plane_width, p.plane_height), (192, 192, 192, 192));
        assert_eq!((p.y.len(), p.cb.len(), p.cr.len()), (192 * 192, 96 * 96, 96 * 96));
        m.rewind();
        let mut first = Planes::default();
        assert!(m.decode(&mut first), "decodes again after a rewind");
    }

    #[test]
    fn menu_background_is_a_moving_picture() {
        let Some(path) = movie("main_menu.sfd") else { return };
        let mut m = Mpeg::open_file(&path).expect("open");
        assert_eq!((m.width(), m.height(), m.frame_rate()), (1824, 1024, 30.0));
        assert_eq!(m.frame_count(), 2335);
        let (mut a, mut b) = (Planes::default(), Planes::default());
        assert!(m.decode(&mut a));
        for _ in 0..30 {
            assert!(m.decode(&mut b));
        }
        assert_ne!(a.y, b.y, "a second in, the picture has changed");
        // Not a blank frame: the luma varies across the picture.
        let (lo, hi) = b.y.iter().fold((255u8, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(hi - lo > 64, "luma range {lo}..{hi}");
    }
}
