//! Bone poses for the renderer: a mesh's skeleton plus a snapshot's pose
//! ops (`snapshot::PoseOp`) -> one skinning matrix per bone.
//!
//! Each bone starts at its rest pose (relative to its parent); an animation
//! clip replaces the poses of the bones it names, then rotators / aims /
//! sliders compose on top in manipulator order, and hidden bones collapse.
//! Bones are composed parent-first (`.scm` lists parents before children);
//! the skinning matrix is `animated world * inverse rest world`, so a rest
//! pose gives identity. Floats throughout: render-only, never sim state.

use glam::{Mat4, Quat, Vec3};

use crate::engine::assets::sca::Clip;
use crate::engine::assets::scm::RenderBone;
use crate::engine::snapshot::{Pose, PoseOp};

pub struct Skeleton {
    parents: Vec<Option<u32>>,
    rest_pos: Vec<Vec3>,
    rest_rot: Vec<Quat>,
    inv_rest: Vec<Mat4>,
    rest: Vec<Mat4>,
    names: Vec<String>,
}

fn quat(wxyz: [f32; 4]) -> Quat {
    let q = Quat::from_xyzw(wxyz[1], wxyz[2], wxyz[3], wxyz[0]);
    if q.length_squared() > 1e-12 { q.normalize() } else { Quat::IDENTITY }
}

impl Skeleton {
    pub fn new(bones: &[RenderBone]) -> Self {
        let parents: Vec<Option<u32>> = bones.iter().map(|b| b.parent).collect();
        let rest_pos: Vec<Vec3> = bones.iter().map(|b| Vec3::from_array(b.position)).collect();
        let rest_rot: Vec<Quat> = bones.iter().map(|b| quat(b.rotation)).collect();
        let mut world: Vec<Mat4> = Vec::with_capacity(bones.len());
        for i in 0..bones.len() {
            let local = Mat4::from_rotation_translation(rest_rot[i], rest_pos[i]);
            let parent = parents[i].and_then(|p| world.get(p as usize).copied());
            world.push(parent.map_or(local, |p| p * local));
        }
        Self { parents, rest_pos, rest_rot, inv_rest: world.iter().map(|m| m.inverse()).collect(), rest: world, names: bones.iter().map(|b| b.name.clone()).collect() }
    }

    /// Bone `b`'s transform in mesh space: posed by its skinning matrix
    /// (`skin`, from [`evaluate`]) or at rest.
    pub fn bone(&self, b: usize, skin: Option<&[[f32; 4]; 4]>) -> Option<Mat4> {
        let rest = *self.rest.get(b)?;
        Some(match skin {
            Some(m) => Mat4::from_cols_array_2d(m) * rest,
            None => rest,
        })
    }

    pub fn len(&self) -> usize {
        self.parents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }

    /// For each of this skeleton's bones, the clip bone of the same name.
    pub fn bind(&self, clip: &Clip) -> Vec<Option<u32>> {
        self.names.iter().map(|n| clip.bones.iter().position(|c| c == n).map(|i| i as u32)).collect()
    }
}

/// Reused per-frame buffers.
#[derive(Default)]
pub struct Scratch {
    pos: Vec<Vec3>,
    rot: Vec<Quat>,
    hidden: Vec<bool>,
    world: Vec<Mat4>,
    placed: Vec<Option<Mat4>>,
}

/// `a` + the shortest turn to `b`, `alpha` of the way (angles wrap).
fn lerp_angle(a: f32, b: f32, alpha: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let d = (b - a + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    a + d * alpha
}

/// `op` as of `alpha` of the way from its counterpart in `prev` (same
/// manipulator, same bone and kind) to it; unpaired ops are taken as is.
pub fn interpolate(pose: &Pose, prev: &[Pose], alpha: f32) -> PoseOp {
    let same = |p: &&Pose| {
        p.source == pose.source
            && match (p.op, pose.op) {
                (PoseOp::Clip { clip: a, .. }, PoseOp::Clip { clip: b, .. }) => a == b,
                (PoseOp::Rotate { bone: a, axis: x, .. }, PoseOp::Rotate { bone: b, axis: y, .. }) => a == b && x == y,
                (PoseOp::Translate { bone: a, .. }, PoseOp::Translate { bone: b, .. }) => a == b,
                (PoseOp::Place { bone: a, .. }, PoseOp::Place { bone: b, .. }) => a == b,
                _ => false,
            }
    };
    let Some(p) = prev.iter().find(same) else { return pose.op };
    match (p.op, pose.op) {
        (PoseOp::Clip { time: t0, .. }, PoseOp::Clip { clip, time: t1, duration, looping }) => {
            let time = if looping && duration > 0.0 {
                // Wrapped past the end since the last tick.
                let t1 = if t1 < t0 { t1 + duration } else { t1 };
                (t0 + (t1 - t0) * alpha).rem_euclid(duration)
            } else {
                t0 + (t1 - t0) * alpha
            };
            PoseOp::Clip { clip, time, duration, looping }
        }
        (PoseOp::Rotate { radians: a, .. }, PoseOp::Rotate { bone, axis, radians: b }) => PoseOp::Rotate { bone, axis, radians: lerp_angle(a, b, alpha) },
        (PoseOp::Translate { offset: a, .. }, PoseOp::Translate { bone, offset: b }) => {
            PoseOp::Translate { bone, offset: Vec3::from_array(a).lerp(Vec3::from_array(b), alpha).to_array() }
        }
        (PoseOp::Place { position: p0, rotation: r0, .. }, PoseOp::Place { bone, position: p1, rotation: r1 }) => PoseOp::Place {
            bone,
            position: Vec3::from_array(p0).lerp(Vec3::from_array(p1), alpha).to_array(),
            rotation: Quat::from_array(r0).slerp(Quat::from_array(r1), alpha).to_array(),
        },
        _ => pose.op,
    }
}

/// Bone `k`'s pose in `clip` at `time` (keyframes blended).
fn sample(clip: &Clip, k: usize, time: f32) -> (Vec3, Quat) {
    let n = clip.bones.len();
    let frames = clip.times.len();
    let at = |f: usize| {
        let (p, r) = clip.poses[f * n + k];
        (Vec3::from_array(p), quat(r))
    };
    let next = clip.times.partition_point(|&t| t <= time).min(frames - 1);
    let prev = next.saturating_sub(1);
    let (t0, t1) = (clip.times[prev], clip.times[next]);
    let w = if t1 > t0 { ((time - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
    let (p0, r0) = at(prev);
    let (p1, r1) = at(next);
    (p0.lerp(p1, w), r0.slerp(r1, w))
}

/// Appends the skinning matrices of `sk` posed by `ops` to `out`. `clip`
/// resolves a clip index to the clip and its binding to this skeleton
/// (`None` while it loads: those bones stay at rest).
pub fn evaluate<'a>(
    sk: &Skeleton,
    ops: impl Iterator<Item = PoseOp> + Clone,
    clip: impl Fn(u32) -> Option<(&'a Clip, &'a [Option<u32>])>,
    s: &mut Scratch,
    out: &mut Vec<[[f32; 4]; 4]>,
) {
    let n = sk.len();
    s.pos.clear();
    s.pos.extend_from_slice(&sk.rest_pos);
    s.rot.clear();
    s.rot.extend_from_slice(&sk.rest_rot);
    s.hidden.clear();
    s.hidden.resize(n, false);
    s.placed.clear();
    s.placed.resize(n, None);
    // Clips set the base pose first; aims, rotators and sliders compose on
    // top whatever order they were created in (an animator made after a
    // turret's aim controller must not undo its aim).
    for op in ops.clone().filter(|op| matches!(op, PoseOp::Clip { .. })).chain(ops.filter(|op| !matches!(op, PoseOp::Clip { .. }))) {
        match op {
            PoseOp::Clip { clip: c, time, .. } => {
                let Some((c, binding)) = clip(c) else { continue };
                if c.times.is_empty() {
                    continue;
                }
                for (b, k) in binding.iter().enumerate() {
                    if let Some(k) = k {
                        let (p, r) = sample(c, *k as usize, time);
                        s.pos[b] = p;
                        s.rot[b] = r;
                    }
                }
            }
            PoseOp::Rotate { bone, axis, radians } => {
                if let Some(r) = s.rot.get_mut(bone as usize) {
                    *r = (*r * Quat::from_axis_angle(Vec3::from_array(axis), radians)).normalize();
                }
            }
            PoseOp::Translate { bone, offset } => {
                // Along the bone's own axes, as `sim::pose` does.
                let r = s.rot.get(bone as usize).copied().unwrap_or(Quat::IDENTITY);
                if let Some(p) = s.pos.get_mut(bone as usize) {
                    *p += r * Vec3::from_array(offset);
                }
            }
            PoseOp::Hide { bone } => {
                if let Some(h) = s.hidden.get_mut(bone as usize) {
                    *h = true;
                }
            }
            PoseOp::Place { bone, position, rotation } => {
                if let Some(p) = s.placed.get_mut(bone as usize) {
                    *p = Some(Mat4::from_rotation_translation(Quat::from_array(rotation).normalize(), Vec3::from_array(position)));
                }
            }
        }
    }
    s.world.clear();
    for b in 0..n {
        if let Some(m) = s.placed[b] {
            s.world.push(m);
            continue;
        }
        let local = Mat4::from_rotation_translation(s.rot[b], s.pos[b]);
        let parent = sk.parents[b].and_then(|p| s.world.get(p as usize).copied());
        s.world.push(parent.map_or(local, |p| p * local));
    }
    // A hidden bone's vertices shrink onto the bone, which keeps its place
    // and facing: effects attached to it still follow it (the energy being
    // hides its whole mesh and is only the emitters on its bones). A zero
    // matrix put them at the world origin with a NaN facing.
    let hidden = Mat4::from_scale(Vec3::splat(HIDDEN_SCALE));
    out.extend((0..n).map(|b| if s.hidden[b] { (s.world[b] * hidden * sk.inv_rest[b]).to_cols_array_2d() } else { (s.world[b] * sk.inv_rest[b]).to_cols_array_2d() }));
}

/// How far a hidden bone shrinks its vertices: far below a pixel (a 50-unit
/// part becomes 5e-5) yet a scale `Mat4::to_scale_rotation_translation` can
/// still take the facing from.
const HIDDEN_SCALE: f32 = 1e-6;

#[cfg(test)]
mod tests {
    use super::*;

    fn bones() -> Vec<RenderBone> {
        vec![
            RenderBone { name: "root".into(), parent: None, position: [0.0, 0.0, 0.0], rotation: [1.0, 0.0, 0.0, 0.0] },
            RenderBone { name: "turret".into(), parent: Some(0), position: [0.0, 1.0, 0.0], rotation: [1.0, 0.0, 0.0, 0.0] },
            RenderBone { name: "barrel".into(), parent: Some(1), position: [0.0, 0.0, 1.0], rotation: [1.0, 0.0, 0.0, 0.0] },
        ]
    }

    fn apply(m: [[f32; 4]; 4], p: Vec3) -> Vec3 {
        Mat4::from_cols_array_2d(&m).transform_point3(p)
    }

    #[test]
    fn a_placed_bone_ignores_its_parent_and_carries_its_children() {
        let sk = Skeleton::new(&bones());
        let mut out = Vec::new();
        let ops = [
            PoseOp::Rotate { bone: 0, axis: [0.0, 1.0, 0.0], radians: 1.0 },
            PoseOp::Place { bone: 1, position: [5.0, 0.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0] },
        ];
        evaluate(&sk, ops.iter().copied(), |_| None, &mut Scratch::default(), &mut out);
        // The turret (rest at y 1) now sits at (5, 0, 0); the barrel (rest
        // at (0, 1, 1)) moves with it.
        let turret = apply(out[1], Vec3::new(0.0, 1.0, 0.0));
        let barrel = apply(out[2], Vec3::new(0.0, 1.0, 1.0));
        assert!((turret - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-5, "{turret}");
        assert!((barrel - Vec3::new(5.0, 0.0, 1.0)).length() < 1e-5, "{barrel}");
    }

    // XSL0402 (the Ythotha's energy being) hides bone 0 and its children,
    // then attaches its emitters to a child: they must stay on the bone.
    #[test]
    fn a_hidden_bone_shrinks_its_mesh_but_keeps_its_place() {
        let sk = Skeleton::new(&bones());
        let mut out = Vec::new();
        let ops = [
            PoseOp::Rotate { bone: 1, axis: [0.0, 1.0, 0.0], radians: std::f32::consts::FRAC_PI_2 },
            PoseOp::Hide { bone: 1 },
            PoseOp::Hide { bone: 2 },
        ];
        evaluate(&sk, ops.into_iter(), |_| None, &mut Scratch::default(), &mut out);
        // Drawn: a barrel vertex 2 ahead of the turret collapses onto the
        // barrel bone (rest (0, 1, 1), swung to (1, 1, 0)).
        let vertex = apply(out[2], Vec3::new(0.0, 1.0, 3.0));
        assert!(vertex.abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-4), "{vertex}");
        // Anchored (what effects attach to): the posed bone, facing intact.
        let (_, r, t) = sk.bone(2, Some(&out[2])).unwrap().to_scale_rotation_translation();
        assert!(t.abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-4), "{t}");
        assert!((r * Vec3::Z).abs_diff_eq(Vec3::X, 1e-3), "{r}");
    }

    #[test]
    fn rest_pose_is_identity_and_yaw_swings_children() {
        let sk = Skeleton::new(&bones());
        let mut out = Vec::new();
        evaluate(&sk, std::iter::empty(), |_| None, &mut Scratch::default(), &mut out);
        assert!(out.iter().all(|m| Mat4::from_cols_array_2d(m).abs_diff_eq(Mat4::IDENTITY, 1e-6)));
        // Turret (pivot (0,1,0)) yawed 90 degrees: a barrel point 2 ahead swings to +x.
        out.clear();
        let ops = [PoseOp::Rotate { bone: 1, axis: [0.0, 1.0, 0.0], radians: std::f32::consts::FRAC_PI_2 }];
        evaluate(&sk, ops.into_iter(), |_| None, &mut Scratch::default(), &mut out);
        assert!(apply(out[2], Vec3::new(0.0, 1.0, 2.0)).abs_diff_eq(Vec3::new(2.0, 1.0, 0.0), 1e-5));
        assert!(apply(out[0], Vec3::new(0.0, 0.0, 2.0)).abs_diff_eq(Vec3::new(0.0, 0.0, 2.0), 1e-6));
    }

    #[test]
    fn manipulators_compose_on_top_of_clips_in_any_order() {
        let sk = Skeleton::new(&bones());
        let clip = Clip { duration: 1.0, bones: vec!["turret".into()], times: vec![0.0], poses: vec![([0.0, 1.0, 0.0], [1.0, 0.0, 0.0, 0.0])] };
        let binding = sk.bind(&clip);
        // The aim comes first, the clip (an animator made later) second.
        let ops = [
            PoseOp::Rotate { bone: 1, axis: [0.0, 1.0, 0.0], radians: std::f32::consts::FRAC_PI_2 },
            PoseOp::Clip { clip: 0, time: 0.0, duration: 1.0, looping: true },
        ];
        let mut out = Vec::new();
        evaluate(&sk, ops.into_iter(), |_| Some((&clip, binding.as_slice())), &mut Scratch::default(), &mut out);
        assert!(apply(out[2], Vec3::new(0.0, 1.0, 2.0)).abs_diff_eq(Vec3::new(2.0, 1.0, 0.0), 1e-5));
    }

    #[test]
    fn clips_set_named_bones_and_loop_interpolation_wraps() {
        let sk = Skeleton::new(&bones());
        let clip = Clip {
            duration: 1.0,
            bones: vec!["turret".into()],
            times: vec![0.0, 1.0],
            poses: vec![([0.0, 1.0, 0.0], [1.0, 0.0, 0.0, 0.0]), ([0.0, 3.0, 0.0], [1.0, 0.0, 0.0, 0.0])],
        };
        let binding = sk.bind(&clip);
        assert_eq!(binding, vec![None, Some(0), None]);
        let mut out = Vec::new();
        let ops = [PoseOp::Clip { clip: 0, time: 0.5, duration: 1.0, looping: true }];
        evaluate(&sk, ops.into_iter(), |_| Some((&clip, binding.as_slice())), &mut Scratch::default(), &mut out);
        assert!(apply(out[1], Vec3::new(0.0, 1.0, 0.0)).abs_diff_eq(Vec3::new(0.0, 2.0, 0.0), 1e-5));
        // 0.9 -> 0.1 across the loop end, halfway = 0.0 (not 0.5).
        let prev = [Pose { source: 7, op: PoseOp::Clip { clip: 0, time: 0.9, duration: 1.0, looping: true } }];
        let now = Pose { source: 7, op: PoseOp::Clip { clip: 0, time: 0.1, duration: 1.0, looping: true } };
        let PoseOp::Clip { time, .. } = interpolate(&now, &prev, 0.5) else { panic!() };
        assert!(time.abs() < 1e-5 || (time - 1.0).abs() < 1e-5, "{time}");
    }

    /// A stock walker and its walk clip: every bone binds, and posed bones
    /// stay within a stride of their rest positions (the model is ~40 units
    /// tall; no clip-space mixups). Fails without the game files
    /// (`RTS_SKIP_DATA_TESTS=1` skips).
    #[test]
    fn stock_walk_clip_poses_near_rest() {
        let dir = std::path::Path::new("../original supreme commander unpacked/gamedata/units/URL0106");
        let (mesh, clip) = (dir.join("URL0106_LOD0.scm"), dir.join("URL0106_Awalk01.sca"));
        if !crate::test_data::require("the stock walker", &[&mesh, &clip]) {
            return;
        }
        let (mesh, clip) = (std::fs::read(mesh).expect("read mesh"), std::fs::read(clip).expect("read clip"));
        let mesh = crate::engine::assets::scm::read_mesh(&mesh).unwrap();
        let clip = crate::engine::assets::sca::read_clip(&clip).unwrap();
        let sk = Skeleton::new(&mesh.bones);
        let binding = sk.bind(&clip);
        assert!(binding.iter().all(|b| b.is_some()), "{binding:?}");
        for t in [0.0, 1.0, 2.9, 3.0] {
            let mut out = Vec::new();
            let ops = [PoseOp::Clip { clip: 0, time: t, duration: clip.duration, looping: true }];
            evaluate(&sk, ops.into_iter(), |_| Some((&clip, binding.as_slice())), &mut Scratch::default(), &mut out);
            for (b, m) in out.iter().enumerate() {
                let rest = Skeleton::new(&mesh.bones).inv_rest[b].inverse().w_axis.truncate();
                let moved = Mat4::from_cols_array_2d(m).transform_point3(rest);
                assert!(moved.is_finite() && moved.distance(rest) < 15.0, "t={t} bone {b}: {rest} -> {moved}");
            }
        }
    }
}
