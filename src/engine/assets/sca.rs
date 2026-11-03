//! `.sca` animation clips: the duration for the sim ([`read_duration`]) and
//! the keyframes for the renderer ([`read_clip`]).
//!
//! Layout (version 5, little endian, verified on stock clips):
//! ```text
//! "ANIM" i32 version  i32 frameCount  f32 duration  i32 boneCount
//! i32 namesOffset  i32 linksOffset  i32 firstFrameOffset  i32 frameSize
//! names: boneCount NUL-terminated strings   links: i32 parent x boneCount
//! at firstFrameOffset: f32[3] root position delta, f32[4] root rotation
//! then frameCount frames of frameSize (= 8 + 28 x boneCount) bytes:
//!   f32 time  u32 flags  { f32[3] position  f32[4] rotation (w,x,y,z) } x boneCount
//! ```
//! Bone poses are relative to the parent, like `.scm` rest poses.

use std::io;

use crate::engine::sim::math::Fx;

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

/// Clip length in seconds. The f32 -> fixed conversion is exact and
/// platform-independent.
pub fn read_duration(b: &[u8]) -> io::Result<Fx> {
    if b.get(0..4) != Some(b"ANIM") {
        return Err(invalid("not an sca (bad magic)"));
    }
    let word = |o: usize| -> io::Result<[u8; 4]> {
        b.get(o..o + 4).and_then(|s| s.try_into().ok()).ok_or_else(|| invalid("truncated sca header"))
    };
    let version = i32::from_le_bytes(word(4)?);
    if version != 5 {
        return Err(invalid(format!("unsupported sca version {version}")));
    }
    let duration = f32::from_le_bytes(word(12)?);
    if !duration.is_finite() || duration < 0.0 {
        return Err(invalid(format!("bad sca duration {duration}")));
    }
    Ok(Fx::saturating_from_num(duration))
}

/// A clip's keyframes (`f32`, render-only).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    pub duration: f32,
    /// Lowercased bone names (bound to a mesh's bones by name).
    pub bones: Vec<String>,
    pub times: Vec<f32>,
    /// `times.len() x bones.len()` poses, frame-major: position and
    /// rotation `[w, x, y, z]`.
    pub poses: Vec<([f32; 3], [f32; 4])>,
}

pub fn read_clip(b: &[u8]) -> io::Result<Clip> {
    let duration = read_duration(b)?.to_num::<f32>();
    let i32_at = |o: usize| -> io::Result<i32> {
        b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| invalid("truncated sca"))
    };
    let f32_at = |o: usize| -> io::Result<f32> {
        b.get(o..o + 4).map(|s| f32::from_le_bytes(s.try_into().unwrap())).ok_or_else(|| invalid("truncated sca"))
    };
    let frames = i32_at(8)?.max(0) as usize;
    let bones = i32_at(16)?.max(0) as usize;
    let names_at = i32_at(20)?.max(0) as usize;
    let first = i32_at(28)?.max(0) as usize + 28;
    let frame_size = i32_at(32)?.max(0) as usize;
    if frame_size < 8 + 28 * bones {
        return Err(invalid("sca frame size too small"));
    }
    let mut clip = Clip { duration, bones: Vec::with_capacity(bones), times: Vec::with_capacity(frames), poses: Vec::with_capacity(frames * bones) };
    let mut at = names_at;
    for _ in 0..bones {
        let tail = b.get(at..).ok_or_else(|| invalid("bad sca names offset"))?;
        let len = tail.iter().position(|&c| c == 0).ok_or_else(|| invalid("unterminated sca bone name"))?;
        clip.bones.push(String::from_utf8_lossy(&tail[..len]).to_lowercase());
        at += len + 1;
    }
    for f in 0..frames {
        let o = first + f * frame_size;
        clip.times.push(f32_at(o)?);
        for k in 0..bones {
            let p = o + 8 + k * 28;
            clip.poses.push(([f32_at(p)?, f32_at(p + 4)?, f32_at(p + 8)?], [f32_at(p + 12)?, f32_at(p + 16)?, f32_at(p + 20)?, f32_at(p + 24)?]));
        }
    }
    even_times_if_unstamped(&mut clip);
    Ok(clip)
}

/// Some clips carry no usable frame times (FAF's re-exported UEF T1 land
/// factory upgrade, `UEB0101_Aupgrade.sca`: every frame stamped 0, which
/// put it at its last frame from the start): their frames are spread evenly
/// over the clip's duration instead. Inferred from the game, which plays
/// that clip (docs/research/verifying-against-the-binary.md).
fn even_times_if_unstamped(clip: &mut Clip) {
    let n = clip.times.len();
    if n < 2 || clip.times.windows(2).all(|w| w[1] > w[0]) {
        return;
    }
    let step = clip.duration / (n - 1) as f32;
    for (i, t) in clip.times.iter_mut().enumerate() {
        *t = step * i as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_header_duration() {
        let mut b = b"ANIM".to_vec();
        for v in [5i32, 101] {
            b.extend(v.to_le_bytes());
        }
        b.extend(3.3333333f32.to_le_bytes());
        assert_eq!(read_duration(&b).unwrap(), Fx::saturating_from_num(3.3333333f32));
        assert!(read_duration(b"MODL").is_err());
    }

    #[test]
    fn reads_keyframes() {
        // 2 frames, 1 bone "Root".
        let mut b = b"ANIM".to_vec();
        let names = 36;
        let links = names + 5;
        let first = links + 4;
        for v in [5i32, 2] {
            b.extend(v.to_le_bytes());
        }
        b.extend(0.1f32.to_le_bytes());
        for v in [1i32, names, links, first, 8 + 28] {
            b.extend(v.to_le_bytes());
        }
        b.extend(b"Root\0");
        b.extend((-1i32).to_le_bytes());
        b.extend([0u8; 28]);
        for (t, x) in [(0.0f32, 1.0f32), (0.1, 2.0)] {
            b.extend(t.to_le_bytes());
            b.extend(0u32.to_le_bytes());
            for v in [x, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
                b.extend(v.to_le_bytes());
            }
        }
        let c = read_clip(&b).unwrap();
        assert_eq!((c.bones.clone(), c.times.clone()), (vec!["root".to_string()], vec![0.0, 0.1]));
        assert_eq!(c.poses[1], ([2.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]));
        // Frames all stamped 0: spread over the duration.
        let mut unstamped = Clip { duration: 1.5, times: vec![0.0; 4], ..Default::default() };
        even_times_if_unstamped(&mut unstamped);
        assert_eq!(unstamped.times, vec![0.0, 0.5, 1.0, 1.5]);
    }
}
