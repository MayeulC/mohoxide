//! Bone poses in the sim, in fixed point: where a bone is once the unit's
//! manipulators (animation clips, aims, rotators, sliders) have moved it.
//!
//! Weapons fire from posed muzzles, and `GetPosition(bone)` reports the
//! posed bone, so a recoiling barrel or a bot's swinging arm shoots from where it
//! is drawn. The renderer poses bones from the same op list ([`ops_of`]),
//! converted to floats in the snapshot, so the two agree on what each
//! manipulator does: clips set their bones' local poses first, then aims,
//! rotators and sliders compose on top in manipulator order
//! (`render_pose::evaluate`).
//!
//! Only the chain from the bone to the root is evaluated (a few bones), on
//! demand: when a projectile spawns or a script asks, not per tick. Clips
//! blend keyframes with a normalised lerp (the renderer slerps; a hair of
//! difference between frames, none at keyframes).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::engine::sim::entity::Entity;
use crate::engine::sim::manipulator::{ManipulatorKind, Manipulators};
use crate::engine::sim::math::{sin_cos, sqrt, Fx, FxQuat, FxVec3};
use crate::engine::sim::weapon::Weapons;

/// What one manipulator does to its bones.
#[derive(Clone, Copy, Debug)]
pub enum Op<'a> {
    /// Plays `clip` at `time` seconds (clip bones named in it, except the
    /// animator's disabled ones).
    Clip { clip: &'a str, time: Fx, duration: Fx, looping: bool, disabled: &'a BTreeSet<u32> },
    /// Turns `bone` about its local unit `axis` by `radians`.
    Rotate { bone: u32, axis: FxVec3, radians: Fx },
    /// Moves `bone` by `offset` (mesh units, parent frame).
    Translate { bone: u32, offset: FxVec3 },
}

/// `"x"` / `"-y"` / ... -> a unit axis (`"z"` for anything else).
pub fn axis_of(name: &str) -> FxVec3 {
    let (sign, rest) = match name.trim().strip_prefix('-') {
        Some(r) => (-Fx::ONE, r),
        None => (Fx::ONE, name.trim()),
    };
    let (x, y, z) = match rest.as_bytes() {
        [b'x' | b'X'] => (sign, Fx::ZERO, Fx::ZERO),
        [b'y' | b'Y'] => (Fx::ZERO, sign, Fx::ZERO),
        _ => (Fx::ZERO, Fx::ZERO, sign),
    };
    FxVec3 { x, y, z }
}

fn degrees_to_radians(d: Fx) -> Fx {
    d.saturating_mul(Fx::PI) / 180
}

/// The ops of manipulator kind `kind` owned by entity `owner` (scale:
/// its mesh scale, for world-unit sliders). Nothing for presentation-only
/// manipulators the renderer doesn't pose either. `manips` (the owner's
/// other manipulators) is what a slave copies its source bone's turn from.
pub fn ops_of<'a>(kind: &'a ManipulatorKind, owner: u32, owner_scale: Fx, weapons: Option<&Weapons>, manips: Option<&'a Manipulators>, mut push: impl FnMut(Op<'a>)) {
    match kind {
        // `CreateSlaver(unit, dest, src)`: `dest` turns as `src` does (FAF
        // slaves a weapon's outer rack bones to its pitch bone with
        // `RackSlavedToTurret`, weapon.lua; the UEF battleship's barrels).
        // The turns the owner's other manipulators give `src`, applied to
        // `dest` in its own frame; its position is its own.
        ManipulatorKind::Slave { dest: Some(dest), src: Some(src) } => {
            let Some(m) = manips else { return };
            // (Clips aren't copied: only turns. Whether an aim shows is
            // asked only of one that turns `src`.)
            let turners = m.of(owner).filter(|o| crate::engine::sim::manipulator::Manipulators::poses(o) && matches!(o.kind, ManipulatorKind::Rotator(_) | ManipulatorKind::Aim(_) | ManipulatorKind::BuilderArm(_) | ManipulatorKind::Thrust(_)));
            for o in turners {
                let mut turns: [Option<(FxVec3, Fx)>; 2] = [None; 2];
                let mut n = 0;
                // (Through `dyn`: a closure per nesting level would never
                // stop instantiating.)
                let mut find = |op: Op<'a>| {
                    if let Op::Rotate { bone, axis, radians } = op
                        && bone == *src && n < turns.len() {
                        turns[n] = Some((axis, radians));
                        n += 1;
                    }
                };
                ops_of(&o.kind, owner, owner_scale, weapons, None, &mut find as &mut dyn FnMut(Op<'a>));
                if n > 0 && !m.aim_hidden(o) {
                    for (axis, radians) in turns.into_iter().flatten() {
                        push(Op::Rotate { bone: *dest, axis, radians });
                    }
                }
            }
        }
        ManipulatorKind::Animator(a) => {
            if let Some(path) = &a.clip {
                push(Op::Clip { clip: path, time: a.time, duration: a.length(), looping: a.looping, disabled: &a.disabled_bones });
            }
        }
        ManipulatorKind::Rotator(r) => {
            if let Some(bone) = r.bone {
                push(Op::Rotate { bone, axis: axis_of(&r.axis), radians: degrees_to_radians(r.angle) });
            }
        }
        ManipulatorKind::Slider(s) => {
            if let Some(bone) = s.bone {
                // World-unit goals are in world units: back to mesh units.
                let offset = if s.world_units && owner_scale > Fx::ZERO {
                    FxVec3 { x: s.current.x / owner_scale, y: s.current.y / owner_scale, z: s.current.z / owner_scale }
                } else {
                    s.current
                };
                push(Op::Translate { bone, offset });
            }
        }
        ManipulatorKind::Aim(a) => {
            // Weapons aim through their state (radians from the unit's
            // facing); script-driven aims through `SetHeadingPitch` (the
            // same convention: heading positive toward +x).
            let (h, p) = match weapons.and_then(|w| w.get(owner, a.weapon_index)) {
                Some(w) => (w.aim_heading, w.aim_pitch),
                None => (a.heading, a.pitch),
            };
            // An arm under its weapon's torso aim: the torso took the heading.
            let h = if a.under_yaw_aim { Fx::ZERO } else { h };
            let (y_axis, x_axis) = (FxVec3 { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO }, FxVec3 { x: Fx::ONE, y: Fx::ZERO, z: Fx::ZERO });
            if let Some(bone) = a.yaw_bone {
                push(Op::Rotate { bone, axis: y_axis, radians: h });
            }
            if let Some(bone) = a.pitch_bone {
                push(Op::Rotate { bone, axis: x_axis, radians: -p });
            }
        }
        ManipulatorKind::Thrust(t) => {
            if let (Some(bone), Some((axis, radians))) = (t.bone, t.axis_angle()) {
                push(Op::Rotate { bone, axis, radians });
            }
        }
        ManipulatorKind::BuilderArm(b) => {
            let (y_axis, x_axis) = (FxVec3 { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO }, FxVec3 { x: Fx::ONE, y: Fx::ZERO, z: Fx::ZERO });
            if let Some(bone) = b.yaw_bone {
                push(Op::Rotate { bone, axis: y_axis, radians: b.heading });
            }
            if let Some(bone) = b.pitch_bone {
                push(Op::Rotate { bone, axis: x_axis, radians: -b.pitch });
            }
        }
        _ => {}
    }
}

/// A clip's keyframes in fixed point (the `.sca` f32s converted exactly).
#[derive(Debug, Default)]
pub struct SimClip {
    pub duration: Fx,
    /// Bone names (matched case-insensitively; clips have few bones, so a
    /// scan beats hashing).
    bones: Vec<String>,
    bone_count: usize,
    times: Vec<Fx>,
    /// `times.len() x bone_count`, frame-major.
    poses: Vec<(FxVec3, FxQuat)>,
    /// Per skeleton it played on (by address; lookups only): each skeleton
    /// bone's clip bone. Usually one entry.
    bindings: std::sync::Mutex<Vec<(usize, Arc<[Option<u16>]>)>>,
}

impl SimClip {
    pub fn from_clip(c: &crate::engine::assets::sca::Clip) -> Self {
        let fx = |v: f32| Fx::saturating_from_num(v);
        Self {
            duration: fx(c.duration),
            bones: c.bones.clone(),
            bone_count: c.bones.len(),
            times: c.times.iter().map(|&t| fx(t)).collect(),
            poses: c
                .poses
                .iter()
                .map(|(p, r)| (FxVec3 { x: fx(p[0]), y: fx(p[1]), z: fx(p[2]) }, normalize(FxQuat { w: fx(r[0]), x: fx(r[1]), y: fx(r[2]), z: fx(r[3]) })))
                .collect(),
            bindings: Default::default(),
        }
    }

    /// Each bone of `sk`: the clip bone of the same name (cached per skeleton).
    pub fn binding(&self, sk: &Arc<crate::engine::assets::scm::Skeleton>) -> Arc<[Option<u16>]> {
        let key = Arc::as_ptr(sk) as usize;
        let mut cache = self.bindings.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, b)) = cache.iter().find(|(k, _)| *k == key) {
            return b.clone();
        }
        let b: Arc<[Option<u16>]> = sk.bones.iter().map(|bone| self.bones.iter().position(|c| c.eq_ignore_ascii_case(&bone.name)).map(|i| i as u16)).collect();
        cache.push((key, b.clone()));
        b
    }

    /// Bone `name`'s local pose at `time` (keyframes blended), if the clip
    /// moves it.
    pub fn sample(&self, name: &str, time: Fx) -> Option<(FxVec3, FxQuat)> {
        let k = self.bones.iter().position(|b| b.eq_ignore_ascii_case(name))?;
        self.sample_index(k, time)
    }

    /// Clip bone `k`'s local pose at `time`.
    pub fn sample_index(&self, k: usize, time: Fx) -> Option<(FxVec3, FxQuat)> {
        let frames = self.times.len();
        if frames == 0 {
            return None;
        }
        let at = |f: usize| self.poses[f * self.bone_count + k];
        let next = self.times.partition_point(|&t| t <= time).min(frames - 1);
        let prev = next.saturating_sub(1);
        let (t0, t1) = (self.times[prev], self.times[next]);
        let w = if t1 > t0 { ((time - t0) / (t1 - t0)).clamp(Fx::ZERO, Fx::ONE) } else { Fx::ZERO };
        let ((p0, r0), (p1, r1)) = (at(prev), at(next));
        let lerp = |a: Fx, b: Fx| a + (b - a) * w;
        let pos = FxVec3 { x: lerp(p0.x, p1.x), y: lerp(p0.y, p1.y), z: lerp(p0.z, p1.z) };
        // Shortest way round: flip the second if the two disagree in sign.
        let dot = r0.x * r1.x + r0.y * r1.y + r0.z * r1.z + r0.w * r1.w;
        let r1 = if dot < Fx::ZERO { FxQuat { x: -r1.x, y: -r1.y, z: -r1.z, w: -r1.w } } else { r1 };
        let rot = normalize(FxQuat { x: lerp(r0.x, r1.x), y: lerp(r0.y, r1.y), z: lerp(r0.z, r1.z), w: lerp(r0.w, r1.w) });
        Some((pos, rot))
    }
}

fn normalize(q: FxQuat) -> FxQuat {
    let n = sqrt(q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w);
    if n == Fx::ZERO {
        return FxQuat::IDENTITY;
    }
    FxQuat { x: q.x / n, y: q.y / n, z: q.z / n, w: q.w / n }
}

fn axis_angle(axis: FxVec3, radians: Fx) -> FxQuat {
    // Wrap to [-pi, pi] so half angles stay in CORDIC's comfortable range.
    let pi = Fx::PI;
    let a = (radians + pi).rem_euclid(pi * 2) - pi;
    let (s, c) = sin_cos(a / 2);
    FxQuat { x: axis.x * s, y: axis.y * s, z: axis.z * s, w: c }
}

/// Clips loaded for the sim, by lowercased VFS path (`None`: failed to
/// load). Lookups only; never iterated.
// Iterated only by `paths`, which sorts.
#[allow(clippy::disallowed_types)]
#[derive(Default)]
pub struct Clips(pub HashMap<String, Option<Arc<SimClip>>>);

impl Clips {
    /// The clips' paths, sorted (a keyframe reloads them: posing reads
    /// only clips already here).
    pub fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.0.keys().cloned().collect();
        paths.sort();
        paths
    }
}

/// Deepest bone chain evaluated (skeletons are far shallower).
const MAX_DEPTH: usize = 64;

/// World position and orientation of `bone` of `e` as posed by its
/// manipulators (`None` bone: the entity's origin and facing).
pub fn bone_world(e: &Entity, bone: Option<u32>, manips: Option<&Manipulators>, weapons: Option<&Weapons>, clips: Option<&Clips>) -> (FxVec3, FxQuat) {
    let (Some(bone), Some(sk)) = (bone, &e.cold.skeleton) else { return (e.position(), e.orientation) };
    if bone as usize >= sk.bones.len() {
        return (e.position(), e.orientation);
    }
    // The chain bone -> root, and each one's local pose (rest to start).
    let mut chain = [0u32; MAX_DEPTH];
    let mut local = [(FxVec3::default(), FxQuat::IDENTITY); MAX_DEPTH];
    let mut n = 0;
    let mut cur = Some(bone);
    while let Some(b) = cur.filter(|_| n < MAX_DEPTH) {
        let Some(info) = sk.bones.get(b as usize) else { break };
        chain[n] = b;
        local[n] = (info.position, info.rotation);
        n += 1;
        cur = info.parent;
    }
    if let Some(m) = manips {
        let slot = |b: u32| chain[..n].iter().position(|&c| c == b);
        // Clips first, then the rest (as the renderer composes them).
        for pass_clips in [true, false] {
            for man in m.of(e.id).filter(|man| crate::engine::sim::manipulator::Manipulators::poses(man) && !m.aim_hidden(man)) {
                // A slave looks through the owner's manipulators: only for
                // a bone in the chain, and only once (not in the clip pass).
                if let ManipulatorKind::Slave { dest, .. } = man.kind
                    && (pass_clips || !dest.is_some_and(|d| slot(d).is_some())) {
                    continue;
                }
                ops_of(&man.kind, e.id, e.cold.scale, weapons, Some(m), |op| match op {
                    Op::Clip { clip, time, disabled, .. } if pass_clips => {
                        // Paths are stored lowercased (`PlayAnim`).
                        let Some(c) = clips.and_then(|c| c.0.get(clip)).and_then(|c| c.as_deref()) else { return };
                        let binding = c.binding(sk);
                        for i in 0..n {
                            if disabled.contains(&chain[i]) {
                                continue;
                            }
                            let Some(Some(k)) = binding.get(chain[i] as usize) else { continue };
                            if let Some(p) = c.sample_index(*k as usize, time) {
                                local[i] = p;
                            }
                        }
                    }
                    Op::Rotate { bone, axis, radians } if !pass_clips => {
                        if let Some(i) = slot(bone) {
                            local[i].1 = local[i].1.mul(axis_angle(axis, radians));
                        }
                    }
                    Op::Translate { bone, offset } if !pass_clips => {
                        if let Some(i) = slot(bone) {
                            // Along the bone's own axes (its local
                            // rotation), not its parent's: the UEF naval
                            // factories' arms are modelled turned, and
                            // their x goals run along the factory.
                            local[i].0 = local[i].0 + local[i].1.rotate(offset);
                        }
                    }
                    _ => {}
                });
            }
        }
    }
    // Compose root -> bone in mesh space, then into the world.
    let (mut pos, mut rot) = (FxVec3::default(), FxQuat::IDENTITY);
    for i in (0..n).rev() {
        pos = pos + rot.rotate(local[i].0);
        rot = rot.mul(local[i].1);
    }
    let s = e.cold.scale;
    let world = e.position() + e.orientation.rotate(FxVec3 { x: pos.x * s, y: pos.y * s, z: pos.z * s });
    (world, e.orientation.mul(rot))
}

/// Where an aim measures from: its bones as moho's composite transforms
/// stand when the aim runs (MohoEngine.dll 0x1021a420 goals, 0x1021b320
/// firing solution): the pose reset to rest, the unit's clips, then every
/// manipulator ordered before it (`Manipulators::of`), not the aim itself.
/// A mount another weapon turned (the Cybran ACU's torso, under its laser),
/// an earlier aim on the same bone (the gun after FAF's dummy weapon), a
/// walker's swaying torso all show in it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AimBase {
    /// The yaw bone's orientation in mesh space (the weapon's aim frame).
    pub yaw_frame: FxQuat,
    /// The pitch bone's orientation in that frame (its barrel's tilt).
    pub pitch_rest: FxQuat,
    /// Where the muzzle is, in the world, before the aim turns it.
    pub muzzle: FxVec3,
    /// The muzzle as finally posed, every manipulator applied: where the
    /// firing solution starts (from the aim's turn last tick, so it
    /// converges on the target as the turret turns) and whether it is under
    /// water.
    pub muzzle_posed: FxVec3,
}

/// The pitch bone's orientation in the yaw bone's frame with the skeleton at
/// rest: the barrel's modelled tilt (the Salvation's, 65 degrees up).
pub fn rest_pitch_frame(sk: &crate::engine::assets::scm::Skeleton, yaw_bone: Option<u32>, pitch_bone: Option<u32>) -> FxQuat {
    let Some(pitch) = pitch_bone.filter(|&b| (b as usize) < sk.bones.len()) else { return FxQuat::IDENTITY };
    // The local orientations from below the yaw bone down to the pitch bone
    // (exactly the identity for a barrel modelled straight).
    let mut below = FxQuat::IDENTITY;
    let mut cur = Some(pitch);
    while let Some(b) = cur {
        if Some(b) == yaw_bone {
            return below;
        }
        let Some(info) = sk.bones.get(b as usize) else { break };
        below = info.rotation.mul(below);
        cur = info.parent;
    }
    // A yaw bone off the chain: through mesh space.
    let model = |b: Option<u32>| b.and_then(|b| sk.model.get(b as usize)).map(|&(_, r)| r);
    model(yaw_bone).unwrap_or(FxQuat::IDENTITY).conjugate().mul(model(Some(pitch)).unwrap_or(FxQuat::IDENTITY))
}

/// [`AimBase`] for weapon `index` of `e`, whose aims turn `yaw_bone` /
/// `pitch_bone` and fire from `muzzle_bone` (else the pitch bone, else the
/// yaw bone, as moho's aim falls back): one walk down the muzzle's chain,
/// with the unit's clips and the manipulators ordered before the weapon's
/// first aim. A yaw or pitch bone off that chain keeps its rest
/// orientation.
#[allow(clippy::too_many_arguments)]
pub fn aim_base(e: &Entity, index: u32, yaw_bone: Option<u32>, pitch_bone: Option<u32>, muzzle_bone: Option<u32>, manips: &Manipulators, weapons: Option<&Weapons>, clips: Option<&Clips>) -> Option<AimBase> {
    let (Some(bone), Some(sk)) = (muzzle_bone.or(pitch_bone).or(yaw_bone), &e.cold.skeleton) else { return None };
    let mut chain = [0u32; MAX_DEPTH];
    let mut local = [(FxVec3::default(), FxQuat::IDENTITY); MAX_DEPTH];
    let mut n = 0;
    let mut cur = Some(bone);
    while let Some(b) = cur.filter(|_| n < MAX_DEPTH) {
        let Some(info) = sk.bones.get(b as usize) else { break };
        chain[n] = b;
        local[n] = (info.position, info.rotation);
        n += 1;
        cur = info.parent;
    }
    let slot = |b: u32| chain[..n].iter().position(|&c| c == b);
    let apply = |man: &crate::engine::sim::manipulator::Manipulator, pass_clips: bool, local: &mut [(FxVec3, FxQuat); MAX_DEPTH]| {
        if let ManipulatorKind::Slave { dest, .. } = man.kind
            && (pass_clips || !dest.is_some_and(|d| slot(d).is_some())) {
            return;
        }
        ops_of(&man.kind, e.id, e.cold.scale, weapons, Some(manips), |op| match op {
            Op::Clip { clip, time, disabled, .. } if pass_clips => {
                let Some(c) = clips.and_then(|c| c.0.get(clip)).and_then(|c| c.as_deref()) else { return };
                let binding = c.binding(sk);
                for i in 0..n {
                    if disabled.contains(&chain[i]) {
                        continue;
                    }
                    let Some(Some(k)) = binding.get(chain[i] as usize) else { continue };
                    if let Some(p) = c.sample_index(*k as usize, time) {
                        local[i] = p;
                    }
                }
            }
            Op::Rotate { bone, axis, radians } if !pass_clips => {
                if let Some(i) = slot(bone) {
                    local[i].1 = local[i].1.mul(axis_angle(axis, radians));
                }
            }
            Op::Translate { bone, offset } if !pass_clips => {
                if let Some(i) = slot(bone) {
                    local[i].0 = local[i].0 + local[i].1.rotate(offset);
                }
            }
            _ => {}
        });
    };
    let shown = |man: &&crate::engine::sim::manipulator::Manipulator| crate::engine::sim::manipulator::Manipulators::poses(man) && !manips.aim_hidden(man);
    // Clips first (as `bone_world` and the renderer compose them), then the
    // rest in order up to this weapon's first aim: the aim's base. The rest
    // after it give the muzzle as finally posed.
    for man in manips.of(e.id).filter(shown) {
        apply(man, true, &mut local);
    }
    let mut rest = manips.of(e.id).skip_while(|man| !matches!(&man.kind, ManipulatorKind::Aim(a) if a.weapon_index == index)).filter(shown).peekable();
    let first_aim = rest.peek().map(|m| m.id);
    for man in manips.of(e.id).take_while(|man| Some(man.id) != first_aim).filter(shown) {
        apply(man, false, &mut local);
    }
    let compose = |local: &[(FxVec3, FxQuat); MAX_DEPTH]| {
        let (mut pos, mut rot) = (FxVec3::default(), FxQuat::IDENTITY);
        for i in (0..n).rev() {
            pos = pos + rot.rotate(local[i].0);
            rot = rot.mul(local[i].1);
        }
        let s = e.cold.scale;
        e.position() + e.orientation.rotate(FxVec3 { x: pos.x * s, y: pos.y * s, z: pos.z * s })
    };
    // Root down: the yaw bone's frame and, below it, the pitch bone's in
    // that frame.
    let mut rot = FxQuat::IDENTITY;
    let (mut yaw_frame, mut below, mut pitch_rest) = (None, FxQuat::IDENTITY, None);
    for i in (0..n).rev() {
        rot = rot.mul(local[i].1);
        if yaw_frame.is_some() {
            below = below.mul(local[i].1);
        }
        if Some(chain[i]) == yaw_bone {
            yaw_frame = Some(rot);
        }
        if Some(chain[i]) == pitch_bone {
            pitch_rest = Some(if Some(chain[i]) == yaw_bone { FxQuat::IDENTITY } else { below });
        }
    }
    let model = |b: Option<u32>| b.and_then(|b| sk.model.get(b as usize)).map(|&(_, r)| r);
    let yaw_frame = yaw_frame.or_else(|| model(yaw_bone)).unwrap_or(rot);
    let pitch_rest = match pitch_bone {
        None => FxQuat::IDENTITY,
        Some(_) => pitch_rest.unwrap_or_else(|| rest_pitch_frame(sk, yaw_bone, pitch_bone)),
    };
    let muzzle = compose(&local);
    for man in rest {
        apply(man, false, &mut local);
    }
    Some(AimBase { yaw_frame, pitch_rest, muzzle, muzzle_posed: compose(&local) })
}

/// Radians up: the elevation of a pitch bone's +Z (its barrel) when it rests
/// at `pitch_rest` in its aim frame.
pub fn rest_elevation(pitch_rest: FxQuat) -> Fx {
    let barrel = pitch_rest.rotate(FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::ONE });
    let horizontal = sqrt(barrel.x.saturating_mul(barrel.x).saturating_add(barrel.z.saturating_mul(barrel.z)));
    crate::engine::sim::math::atan2(barrel.y, horizontal)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: FxVec3, b: (f64, f64, f64)) -> bool {
        let (x, y, z) = a.to_f64();
        (x - b.0).abs() < 1e-4 && (y - b.1).abs() < 1e-4 && (z - b.2).abs() < 1e-4
    }

    #[test]
    fn axis_angle_turns_like_the_renderer() {
        // 90 degrees about +y takes +z to +x (glam's convention).
        let q = axis_angle(axis_of("y"), Fx::from_num(std::f64::consts::FRAC_PI_2));
        assert!(close(q.rotate(FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::ONE }), (1.0, 0.0, 0.0)));
        // Angles past a full turn wrap.
        let q = axis_angle(axis_of("-x"), Fx::from_num(std::f64::consts::PI * 2.0 + std::f64::consts::FRAC_PI_2));
        assert!(close(q.rotate(FxVec3 { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO }), (0.0, 0.0, -1.0)), "{:?}", q.rotate(FxVec3 { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO }).to_f64());
        assert!((degrees_to_radians(Fx::from_num(180)).to_num::<f64>() - std::f64::consts::PI).abs() < 1e-8);
    }

    #[test]
    fn clip_samples_blend_between_keyframes() {
        let c = crate::engine::assets::sca::Clip {
            duration: 1.0,
            bones: vec!["Turret".into()],
            times: vec![0.0, 1.0],
            poses: vec![([0.0, 1.0, 0.0], [1.0, 0.0, 0.0, 0.0]), ([0.0, 3.0, 0.0], [0.0, 0.0, 1.0, 0.0])],
        };
        let s = SimClip::from_clip(&c);
        let (p, r) = s.sample("turret", Fx::from_num(0.5)).unwrap();
        assert!(close(p, (0.0, 2.0, 0.0)));
        // Halfway between identity and 180 degrees about y: 90 about y.
        assert!(close(r.rotate(FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::ONE }), (1.0, 0.0, 0.0)), "{:?}", r.to_f64());
        assert!(s.sample("barrel", Fx::ZERO).is_none());
    }
}
