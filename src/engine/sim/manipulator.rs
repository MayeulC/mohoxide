//! Bone manipulators (aim controllers, rotators, sliders, animators, ...).
//! Id-ordered so per-tick evaluation is deterministic.

use std::collections::BTreeMap;

use crate::engine::sim::entity::EntityId;
use crate::engine::sim::math::{Fx, FxQuat, FxVec3};

pub type ManipulatorId = u32;

/// Degrees; slews in degrees per second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FiringArc {
    pub min_heading: Fx,
    pub max_heading: Fx,
    pub heading_slew: Fx,
    pub min_pitch: Fx,
    pub max_pitch: Fx,
    pub pitch_slew: Fx,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Aim {
    pub weapon_index: u32,
    pub label: String,
    pub yaw_bone: Option<u32>,
    pub pitch_bone: Option<u32>,
    pub muzzle_bone: Option<u32>,
    pub arc: FiringArc,
    /// Seconds before returning to the idle pose.
    pub reset_pose_time: Fx,
    pub heading: Fx,
    pub pitch: Fx,
    pub heading_offset: Fx,
    /// Under another of its weapon's aims that only yaws (FAF's dual-arm
    /// weapons, `TurretDualManipulators`: a 'Torso' aim turns, the arms
    /// pitch): the weapon's heading turns that aim, so this one only
    /// pitches (its yaw bone is its pitch bone, and would turn again on top
    /// of the torso).
    pub under_yaw_aim: bool,
    /// The aim's own `SetEnabled` (moho's aim +0xb4, MohoEngine.dll
    /// 0x1021c190), apart from the manipulator's `Enable` / `Disable`:
    /// disabled, it doesn't turn its bones (they show their rest pose) and
    /// its weapon's angles and on-target state stay as they were
    /// (0x1021a820), unless its weapon is `AimsStraightOnDisable`.
    #[serde(default = "enabled_by_default")]
    pub active: bool,
    /// Its weapon's `AimsStraightOnDisable`: disabled by `SetEnabled`, it
    /// still turns, back to straight ahead.
    #[serde(default)]
    pub straight_on_disable: bool,
}

fn enabled_by_default() -> bool {
    true
}

/// Builder arm: aims at the unit being built/repaired/reclaimed.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuilderArm {
    pub yaw_bone: Option<u32>,
    pub pitch_bone: Option<u32>,
    pub aim_bone: Option<u32>,
    /// Radians and radians per second (`SetAimingArc` takes degrees).
    pub arc: FiringArc,
    /// Radians from the yaw bone's rest frame, positive toward +x (the
    /// weapons' convention, which scripts see too: moho's aims store it).
    pub heading: Fx,
    pub pitch: Fx,
    /// Aiming at something this tick (its unit has a focus): wins shared
    /// bones over the unit's weapon aims of no higher precedence.
    pub active: bool,
}

impl BuilderArm {
    /// One tick: turn toward `target` (world point) seen from `origin` in
    /// `frame` (the yaw bone's rest frame), within the arc and slew limits;
    /// back toward rest without one.
    pub fn aim(&mut self, origin: FxVec3, frame: crate::engine::sim::math::FxQuat, target: Option<FxVec3>) {
        use crate::engine::sim::math::{atan2, sqrt};
        let (want_h, want_p) = match target {
            Some(t) => {
                let local = frame.conjugate().rotate(t - origin);
                let horizontal = sqrt(local.x.saturating_mul(local.x).saturating_add(local.z.saturating_mul(local.z)));
                (atan2(local.x, local.z), atan2(local.y, horizontal))
            }
            None => (Fx::ZERO, Fx::ZERO),
        };
        self.active = target.is_some();
        let arc = &self.arc;
        let dt = Fx::ONE / Fx::from_num(10);
        let (want_h, want_p) = if arc.max_heading > arc.min_heading || arc.max_pitch > arc.min_pitch {
            (want_h.clamp(arc.min_heading, arc.max_heading), want_p.clamp(arc.min_pitch, arc.max_pitch))
        } else {
            (want_h, want_p)
        };
        self.heading = slew(self.heading, want_h, arc.heading_slew * dt);
        self.pitch = slew(self.pitch, want_p, arc.pitch_slew * dt);
    }
}

/// Moves `cur` toward `want` by at most `step` (no limit: snap).
fn slew(cur: Fx, want: Fx, step: Fx) -> Fx {
    if step <= Fx::ZERO {
        return want;
    }
    let d = want - cur;
    if d.abs() <= step { want } else if d > Fx::ZERO { cur + step } else { cur - step }
}

/// Slides a bone toward `goal` (offset from its rest pose) at `speed`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Slider {
    pub bone: Option<u32>,
    pub goal: FxVec3,
    pub current: FxVec3,
    /// Units per second; 0 = jump immediately.
    pub speed: Fx,
    pub acceleration: Fx,
    pub deceleration: Fx,
    /// Goal in world units rather than mesh units.
    pub world_units: bool,
}

impl Slider {
    /// Done for `WaitFor`: at its goal.
    pub fn finished(&self) -> bool {
        self.current == self.goal
    }

    /// Moves `current` toward `goal` at `speed` units per second, or jumps
    /// there when `speed <= 0`. Acceleration / deceleration aren't modelled
    /// (inferred; see docs/research/verifying-against-the-binary.md).
    pub fn step(&mut self) {
        if self.finished() {
            return;
        }
        let d = self.goal - self.current;
        let dist = crate::engine::sim::math::sqrt(d.x.saturating_mul(d.x).saturating_add(d.y.saturating_mul(d.y)).saturating_add(d.z.saturating_mul(d.z)));
        let step = self.speed / Fx::from_num(10);
        // Snap when within a rounding hair of the goal, so fixed-point
        // division error can't add an extra tick.
        if self.speed <= Fx::ZERO || dist <= step + Fx::from_bits(1 << 22) {
            self.current = self.goal;
        } else {
            let f = step / dist;
            self.current = FxVec3 { x: self.current.x + d.x * f, y: self.current.y + d.y * f, z: self.current.z + d.z * f };
        }
    }
}

/// Calls `owner:OnAnimTerrainCollision(bone, x, y, z)` when a watched bone
/// touches the terrain.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CollisionDetector {
    pub watched: Vec<u32>,
    pub terrain_check: bool,
    /// Per watched bone: touched the ground and has not lifted since.
    pub grounded: Vec<bool>,
}

/// IK that keeps a foot bone on the terrain (walkers). Presentation-only in
/// effect, but tracked so animation evaluation stays in one place.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FootPlant {
    pub foot_bone: Option<u32>,
    pub knee_bone: Option<u32>,
    pub hip_bone: Option<u32>,
    pub straight_legs: bool,
    pub max_foot_fall: Fx,
}

/// Plays a `.sca` animation clip on an entity's bones.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Animator {
    pub clip: Option<String>,
    pub looping: bool,
    /// Playback rate multiplier (1 = normal, 0 = paused).
    pub rate: Fx,
    /// Seconds into the clip.
    pub time: Fx,
    /// Clip length in seconds, once the clip is loaded.
    pub duration: Option<Fx>,
    /// `CreateAnimator(unit, true)`: the clip runs at `rate` x the unit's
    /// speed / its `MaxSpeed` (`Manipulators::step_aligned`).
    pub align_with_speed: bool,
    /// An aligned animator's owner orientation as the tick began, and
    /// whether it had changed over the tick before (`note_turns`).
    pub last_orientation: Option<FxQuat>,
    pub turned: bool,
    /// Bones the animation leaves alone (`SetBoneEnabled(bone, false)`).
    pub disabled_bones: std::collections::BTreeSet<u32>,
}

impl Animator {
    /// Clip length, 0 until a clip with a known duration plays.
    pub fn length(&self) -> Fx {
        self.duration.unwrap_or(Fx::ZERO)
    }

    /// A non-looping clip that has played to its end (or start, when
    /// running backwards). Paused clips (rate 0) never finish.
    pub fn finished(&self) -> bool {
        !self.looping
            && ((self.rate > Fx::ZERO && self.time >= self.length())
                || (self.rate < Fx::ZERO && self.time <= Fx::ZERO)
                || self.length() == Fx::ZERO)
    }

    /// Advances one sim tick: 0.1 s x `rate` x `pace` (1 unless aligned with
    /// the owner's speed).
    pub fn step(&mut self, pace: Fx) {
        let len = self.length();
        if len == Fx::ZERO {
            return;
        }
        let t = self.time + self.rate.saturating_mul(pace).saturating_mul(Fx::from_num(1) / Fx::from_num(10));
        self.time = if self.looping { t.rem_euclid(len) } else { t.clamp(Fx::ZERO, len) };
    }

    /// Seeks to `t` seconds, clamped to the clip.
    pub fn seek(&mut self, t: Fx) {
        self.time = t.clamp(Fx::ZERO, self.length());
    }
}

/// How fast a speed-aligned animator runs (MohoEngine.dll
/// `CAnimationManipulator::ManipulatorUpdate` 0x10228f00): the owner's speed
/// this tick over its blueprint `MaxSpeed`, and at least a quarter when it
/// turned over the tick before (moho compares the transforms' quaternions),
/// so legs walk while it turns on the spot. Plain rate without a unit, or for
/// a unit with no `MaxSpeed` (moho divides by it). Real game (dev/probes
/// probe_walk): a Mantis turning on the spot advances its clip 8 x 0.25 x
/// 0.1 a tick, the quarter holding one tick past its last turn.
fn speed_pace(a: &Animator, owner: Option<&crate::engine::sim::entity::Entity>) -> Fx {
    let Some(e) = owner else { return Fx::ONE };
    let Some(u) = e.unit.as_deref() else { return Fx::ONE };
    let max_speed = u.motion_params.max_speed;
    if max_speed <= Fx::ZERO {
        return Fx::ONE;
    }
    // |velocity| (`movement::velocity`: a unit direction x speed, plus climb).
    let motion = &u.motion_state;
    let speed = FxVec3 { x: motion.speed, y: motion.climb, z: Fx::ZERO }.length();
    let pace = speed / max_speed;
    if a.turned { pace.max(Fx::from_num(0.25)) } else { pace }
}

/// Rotates a bone about one axis (`CreateRotator`), degrees.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rotator {
    pub bone: Option<u32>,
    /// "x" | "y" | "z", optionally negated.
    pub axis: String,
    pub angle: Fx,
    /// Turn to this angle and stop; `None` = spin at `target_speed`.
    pub goal: Option<Fx>,
    /// Degrees per second toward the goal.
    pub speed: Fx,
    /// Degrees per second² (0 = instant speed changes).
    pub accel: Fx,
    /// Spin speed without a goal, degrees per second.
    pub target_speed: Fx,
    /// Current spin speed (ramps toward `target_speed` by `accel`).
    pub spin: Fx,
}

impl Rotator {
    /// Done for `WaitFor`: at its goal, or spun down with no goal.
    pub fn finished(&self) -> bool {
        match self.goal {
            Some(g) => self.angle == g,
            None => self.spin == Fx::ZERO && self.target_speed == Fx::ZERO,
        }
    }

    pub fn step(&mut self) {
        let dt = Fx::ONE / Fx::from_num(10);
        match self.goal {
            Some(g) => {
                let d = g - self.angle;
                let step = self.speed * dt;
                self.angle = if self.speed <= Fx::ZERO || d.abs() <= step { g } else if d > Fx::ZERO { self.angle + step } else { self.angle - step };
            }
            None => {
                let ds = self.target_speed - self.spin;
                let step = self.accel * dt;
                self.spin = if self.accel <= Fx::ZERO || ds.abs() <= step { self.target_speed } else if ds > Fx::ZERO { self.spin + step } else { self.spin - step };
                self.angle = (self.angle + self.spin * dt) % Fx::from_num(360);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ManipulatorKind {
    Rotator(Rotator),
    Animator(Animator),
    FootPlant(FootPlant),
    CollisionDetector(CollisionDetector),
    Aim(Aim),
    BuilderArm(BuilderArm),
    Slider(Slider),
    /// Engine pod steering (`CreateThrustController`): the bone turns toward
    /// the unit's thrust each tick (`Thrust::step`). Presentation.
    Thrust(Thrust),
    /// Copies `src` bone's motion onto `dest` (`CreateSlaver`); presentation
    /// only.
    Slave { dest: Option<u32>, src: Option<u32> },
    /// Storage fill indicator (`CreateStorageManip`): slides a bone between
    /// two offsets with the army's stored fraction; presentation only.
    Storage(Storage),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Thrust {
    pub label: String,
    pub bone: Option<u32>,
    /// `SetThrustingParam(xCapMin, xCapMax, yCapMin, yCapMax, zCapMin,
    /// zCapMax, turnForceMult, turnSpeed)` (moho's own argument names; FAF's
    /// comments swap min and max). Defaults: caps -100 / 100, mult 1,
    /// speed 0.3 (`CThrustManipulator` ctor, `MohoEngine.dll` 0x10232790).
    pub params: [Fx; 8],
    /// The bone's turn now (applied in its own frame).
    pub current: Option<FxQuat>,
    /// Last tick's position, velocity (per tick) and heading: the unit's
    /// acceleration and yaw rate come from their differences.
    pub last: Option<(FxVec3, FxVec3, Fx)>,
    /// Reached its target with the unit's motion unchanged since: nothing
    /// to do until the unit moves differently.
    pub settled: bool,
}

impl Default for Thrust {
    fn default() -> Self {
        let (lo, hi) = (Fx::from_num(-100), Fx::from_num(100));
        Self { label: String::new(), bone: None, params: [lo, hi, lo, hi, lo, hi, Fx::ONE, Fx::from_num(0.3)], current: None, last: None, settled: false }
    }
}

/// The shortest rotation taking unit vector `from` to unit vector `to`
/// (moho's `Align`, 0x100f9e70: half-way vector).
fn align(from: FxVec3, to: FxVec3) -> FxQuat {
    let sum = FxVec3 { x: from.x + to.x, y: from.y + to.y, z: from.z + to.z };
    match sum.normalized() {
        Some(h) => {
            let w = from.x * h.x + from.y * h.y + from.z * h.z;
            let (x, y, z) = (from.y * h.z - from.z * h.y, from.z * h.x - from.x * h.z, from.x * h.y - from.y * h.x);
            FxQuat { x, y, z, w }.normalized()
        }
        // Opposite: half a turn about any axis square to `from`.
        None => {
            let side = if from.x.abs() < Fx::from_num(0.9) { FxVec3 { x: Fx::ONE, y: Fx::ZERO, z: Fx::ZERO } } else { FxVec3 { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO } };
            let a = FxVec3 { x: from.y * side.z - from.z * side.y, y: from.z * side.x - from.x * side.z, z: from.x * side.y - from.y * side.x };
            let a = a.normalized().unwrap_or(side);
            FxQuat { x: a.x, y: a.y, z: a.z, w: Fx::ZERO }
        }
    }
}

impl Thrust {
    /// One tick of `CThrustManipulator`'s update (`MohoEngine.dll`
    /// 0x10232ad0): the unit's thrust (acceleration less gravity; moho's
    /// `CUnitMotion` force, here per unit mass, inferred) plus `turnMult` x
    /// the sideways shift a yaw turn gives the bone, into the bone's frame,
    /// clamped per axis to the caps and normalised. The bone's local +Z (in
    /// its parent's frame) is aimed along it, turning at most `turnSpeed`
    /// radians a tick. `bone_offset` = bone world position - unit position;
    /// `bone_rotation` = the bone's world orientation; `local_rest` its
    /// rotation relative to its parent.
    pub fn step(&mut self, position: FxVec3, heading: Fx, bone_offset: FxVec3, bone_rotation: FxQuat, local_rest: FxQuat, push: Option<FxVec3>) {
        let p = self.params;
        let zero = FxVec3::default();
        let (velocity, accel, yaw) = match self.last {
            Some((last_pos, last_vel, last_heading)) => {
                let v = FxVec3 { x: position.x - last_pos.x, y: position.y - last_pos.y, z: position.z - last_pos.z };
                let a = FxVec3 { x: v.x - last_vel.x, y: v.y - last_vel.y, z: v.z - last_vel.z };
                let mut d = heading - last_heading;
                if d > Fx::PI {
                    d -= Fx::PI * 2;
                } else if d < -Fx::PI {
                    d += Fx::PI * 2;
                }
                (v, a, d)
            }
            None => (zero, zero, Fx::ZERO),
        };
        // Same velocity and heading as last tick: same target, so a pod
        // already there stays (most aircraft hover or cruise).
        let steady = self.last.is_some_and(|(_, v, h)| v == velocity && h == heading);
        self.last = Some((position, velocity, heading));
        if steady && self.settled {
            return;
        }
        // A hovering aircraft's push from its controller per unit mass
        // (KMove x D, not the net force: the real game's gunship pods stay
        // tilted forward at cruise, its transport's upright flying straight
        // (a hair's pitch leaks 0.03 sideways there, so it isn't scaled up)
        // and snapped to their side limit by the smallest turn;
        // dev/probes probe_turns); else the acceleration per second squared
        // (10 ticks a second) less gravity (0, -4.9, 0).
        let thrust = match push {
            Some(f) => f,
            None => {
                let hundred = Fx::from_num(100);
                FxVec3 { x: accel.x * hundred, y: accel.y * hundred + Fx::from_num(4.9), z: accel.z * hundred }
            }
        };
        let turned = FxQuat::from_rotation_vector(FxVec3 { x: Fx::ZERO, y: yaw * 10, z: Fx::ZERO }).rotate(bone_offset);
        let v = FxVec3 {
            x: thrust.x + p[6] * (turned.x - bone_offset.x),
            y: thrust.y + p[6] * (turned.y - bone_offset.y),
            z: thrust.z + p[6] * (turned.z - bone_offset.z),
        };
        let l = bone_rotation.conjugate().rotate(v);
        let clamped = FxVec3 { x: l.x.clamp(p[0], p[1].max(p[0])), y: l.y.clamp(p[2], p[3].max(p[2])), z: l.z.clamp(p[4], p[5].max(p[4])) };
        let reference = local_rest.rotate(FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::ONE });
        let current = *self.current.get_or_insert_with(|| align(reference, FxVec3 { x: Fx::ZERO, y: Fx::ONE, z: Fx::ZERO }));
        if clamped.length() <= Fx::from_num(0.000001) {
            return;
        }
        let Some(c) = clamped.normalized() else { return };
        let target = align(reference, c);
        // Toward the target by at most `turnSpeed` radians.
        let d = current.conjugate().mul(target);
        let speed = p[7].abs();
        let (s, cos) = crate::engine::sim::math::sin_cos(speed / 2);
        let xyz = FxVec3 { x: d.x, y: d.y, z: d.z };
        let capped = speed / 2 < Fx::FRAC_PI_2 && xyz.length() > s;
        self.settled = !capped;
        let out = if capped {
            let axis = xyz.normalized().unwrap_or(zero);
            let sign = if d.w < Fx::ZERO { -Fx::ONE } else { Fx::ONE };
            current.mul(FxQuat { x: axis.x * s * sign, y: axis.y * s * sign, z: axis.z * s * sign, w: cos }).normalized()
        } else {
            target
        };
        self.current = Some(out);
    }

    /// The current turn as (axis, radians), for the pose.
    pub fn axis_angle(&self) -> Option<(FxVec3, Fx)> {
        let q = self.current?;
        let xyz = FxVec3 { x: q.x, y: q.y, z: q.z };
        let len = xyz.length();
        if len == Fx::ZERO {
            return None;
        }
        let axis = FxVec3 { x: q.x / len, y: q.y / len, z: q.z / len };
        Some((axis, crate::engine::sim::math::atan2(len, q.w) * 2))
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Storage {
    pub bone: Option<u32>,
    /// `'MASS'` / `'ENERGY'`.
    pub resource: String,
    pub min: FxVec3,
    pub max: FxVec3,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manipulator {
    pub id: ManipulatorId,
    pub owner: EntityId,
    pub precedence: i32,
    /// Its place among its owner's manipulators of equal precedence: set
    /// when made and again by `SetPrecedence`, which moves it to the end of
    /// its new group, as moho's actor list does (MohoEngine.dll 0x10224a20).
    #[serde(default)]
    pub seq: u64,
    pub enabled: bool,
    pub kind: ManipulatorKind,
}

/// A manipulator's place in [`Manipulators`]' owner index.
type OwnerKey = (EntityId, i32, u64, ManipulatorId);

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Manipulators {
    next_id: ManipulatorId,
    #[serde(default)]
    next_seq: u64,
    map: BTreeMap<ManipulatorId, Manipulator>,
    /// (owner, precedence, seq, id) of every manipulator: an entity's in the
    /// order moho applies them, ascending precedence, equal precedence in
    /// the order made or last re-ranked (`CAniActor::UpdateManipulators`,
    /// MohoEngine.dll 0x10223f50), without a full scan.
    by_owner: std::collections::BTreeSet<OwnerKey>,
    /// Thrust controllers, so the per-tick pass visits only them.
    thrusts: std::collections::BTreeSet<ManipulatorId>,
    /// Speed-aligned animators (walk cycles), stepped after movement.
    aligned: std::collections::BTreeSet<ManipulatorId>,
}

impl Manipulators {
    pub fn add(&mut self, owner: EntityId, kind: ManipulatorKind) -> ManipulatorId {
        let id = self.next_id;
        self.next_id += 1;
        if matches!(kind, ManipulatorKind::Thrust(_)) {
            self.thrusts.insert(id);
        }
        if matches!(&kind, ManipulatorKind::Animator(a) if a.align_with_speed) {
            self.aligned.insert(id);
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.map.insert(id, Manipulator { id, owner, precedence: 0, seq, enabled: true, kind });
        self.by_owner.insert((owner, 0, seq, id));
        id
    }

    /// `SetPrecedence`: re-ranks `id` among its owner's manipulators, at the
    /// end of its new precedence's group.
    pub fn set_precedence(&mut self, id: ManipulatorId, precedence: i32) {
        let seq = self.next_seq;
        let Some(m) = self.map.get_mut(&id) else { return };
        self.next_seq += 1;
        self.by_owner.remove(&(m.owner, m.precedence, m.seq, id));
        m.precedence = precedence;
        m.seq = seq;
        self.by_owner.insert((m.owner, precedence, seq, id));
    }

    pub fn get_mut(&mut self, id: ManipulatorId) -> Option<&mut Manipulator> {
        self.map.get_mut(&id)
    }

    pub fn get(&self, id: ManipulatorId) -> Option<&Manipulator> {
        self.map.get(&id)
    }

    /// All manipulators in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Manipulator> {
        self.map.values()
    }

    /// Every manipulator, by id, mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Manipulator> {
        self.map.values_mut()
    }

    pub fn remove(&mut self, id: ManipulatorId) -> Option<Manipulator> {
        let m = self.map.remove(&id)?;
        self.by_owner.remove(&(m.owner, m.precedence, m.seq, id));
        self.thrusts.remove(&id);
        self.aligned.remove(&id);
        Some(m)
    }

    /// Turns every thrust controller's bone toward its unit's thrust (id
    /// order; after movement). Unfinished or destroyed owners are skipped.
    pub fn step_thrust(&mut self, world: &crate::engine::sim::entity::EntityWorld) {
        for id in &self.thrusts {
            let Some(m) = self.map.get_mut(id) else { continue };
            let ManipulatorKind::Thrust(t) = &mut m.kind else { continue };
            if !m.enabled {
                continue;
            }
            let Some(bone) = t.bone else { continue };
            let Some(e) = world.get(m.owner).filter(|e| !e.destroyed && e.fraction_complete >= Fx::ONE) else { continue };
            let local_rest = e.cold.skeleton.as_ref().and_then(|s| s.bones.get(bone as usize)).map_or(FxQuat::IDENTITY, |b| b.rotation);
            let p = e.position();
            let offset = e.bone_world_position(Some(bone));
            let offset = FxVec3 { x: offset.x - p.x, y: offset.y - p.y, z: offset.z - p.z };
            // (In the air only: landed, its last push would hold the pods.)
            let push = e.unit.as_deref().filter(|u| u.layer == crate::engine::sim::layer::Layer::Air).and_then(|u| u.motion_state.thrust);
            t.step(p, e.orientation.heading(), offset, e.bone_world_rotation(Some(bone)), local_rest, push);
        }
    }

    /// Advances every animator by one tick (id order; allocation-free),
    /// before movement. Speed-aligned ones only note whether their owner
    /// turned over the tick before; `step_aligned` advances them.
    pub fn step(&mut self, world: &crate::engine::sim::entity::EntityWorld) {
        for m in self.map.values_mut() {
            match &mut m.kind {
                // Whether the owner turned since the last tick began (its
                // first tick: no); disabled ones too, as moho's transforms
                // move on regardless.
                ManipulatorKind::Animator(a) if a.align_with_speed => {
                    if let Some(e) = world.get(m.owner) {
                        a.turned = a.last_orientation.is_some_and(|q| q != e.orientation);
                        a.last_orientation = Some(e.orientation);
                    }
                    continue;
                }
                _ if !m.enabled => continue,
                ManipulatorKind::Animator(a) => a.step(Fx::ONE),
                ManipulatorKind::Rotator(r) => r.step(),
                ManipulatorKind::Slider(s) => s.step(),
                _ => {}
            }
        }
    }

    /// Advances the speed-aligned animators (walk cycles) by the motion just
    /// made (id order; after movement, as moho's speed reads this tick's).
    pub fn step_aligned(&mut self, world: &crate::engine::sim::entity::EntityWorld) {
        for id in &self.aligned {
            let Some(m) = self.map.get_mut(id) else { continue };
            let ManipulatorKind::Animator(a) = &mut m.kind else { continue };
            if !m.enabled {
                continue;
            }
            let pace = speed_pace(a, world.get(m.owner));
            a.step(pace);
        }
    }

    /// Whether `m` turns its bones in the pose: enabled, and for an aim
    /// disabled by its own `SetEnabled`, only one that aims straight on
    /// disable (moho's aim update returns before turning, MohoEngine.dll
    /// 0x1021a86d).
    pub fn poses(m: &Manipulator) -> bool {
        m.enabled && !matches!(&m.kind, ManipulatorKind::Aim(a) if !a.active && !a.straight_on_disable)
    }

    /// Drops every manipulator owned by an entity in `owners` (sorted
    /// ascending) in one pass.
    /// Returns the removed ids.
    pub fn remove_owners(&mut self, owners: &[EntityId]) -> Vec<ManipulatorId> {
        let mut removed = Vec::new();
        self.map.retain(|&id, m| {
            let keep = owners.binary_search(&m.owner).is_err();
            if !keep {
                removed.push(id);
            }
            keep
        });
        self.by_owner.retain(|(o, ..)| owners.binary_search(o).is_err());
        for id in &removed {
            self.aligned.remove(id);
        }
        removed
    }

    /// Every manipulator's id by owner, each owner's in the order moho
    /// applies them (precedence, then made or re-ranked), without fetching it.
    pub fn owner_order(&self) -> impl Iterator<Item = ManipulatorId> + '_ {
        self.by_owner.iter().map(|&(.., id)| id)
    }

    /// Every manipulator, by owner in that same order.
    pub fn by_owner(&self) -> impl Iterator<Item = &Manipulator> {
        self.by_owner.iter().filter_map(|(.., id)| self.map.get(id))
    }

    /// An entity's manipulators in the order moho applies them.
    pub fn of(&self, owner: EntityId) -> impl Iterator<Item = &Manipulator> {
        self.by_owner.range((owner, i32::MIN, 0, ManipulatorId::MIN)..=(owner, i32::MAX, u64::MAX, ManipulatorId::MAX)).filter_map(|(.., id)| self.map.get(id))
    }

    /// Whether `m`'s aim doesn't show because a builder arm drives the same
    /// bones (the UEF ACU's arm is its gun and its builder arm): an active
    /// builder arm at precedence >= a weapon aim's wins; otherwise the
    /// weapon aim does (an idle builder arm stays hidden). Moho stacks every
    /// manipulator on a bone (`CAniPoseBone::Rotate` multiplies, MohoEngine.dll
    /// 0x1013c080); weapon aims do here too, each measured from the pose the
    /// earlier ones left ([`crate::engine::sim::pose::aim_base`]), but the
    /// builder arm is still measured from rest, so it and an aim can't both
    /// show.
    pub fn aim_hidden(&self, m: &Manipulator) -> bool {
        let bones = |k: &ManipulatorKind| match k {
            ManipulatorKind::Aim(a) => [a.yaw_bone, a.pitch_bone],
            ManipulatorKind::BuilderArm(b) => [b.yaw_bone, b.pitch_bone],
            _ => [None, None],
        };
        let mine = bones(&m.kind);
        let shares = |o: &Manipulator| o.id != m.id && Self::poses(o) && bones(&o.kind).iter().flatten().any(|b| mine.contains(&Some(*b)));
        match &m.kind {
            ManipulatorKind::Aim(_) => self.of(m.owner).any(|o| matches!(&o.kind, ManipulatorKind::BuilderArm(b) if b.active) && o.precedence >= m.precedence && shares(o)),
            ManipulatorKind::BuilderArm(b) => {
                let weapon = self.of(m.owner).filter(|o| matches!(o.kind, ManipulatorKind::Aim(_)) && shares(o)).map(|o| o.precedence).max();
                weapon.is_some_and(|p| !b.active || p > m.precedence)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(looping: bool, rate: f64) -> Animator {
        Animator {
            clip: Some("a.sca".into()),
            looping,
            rate: Fx::from_num(rate),
            time: Fx::ZERO,
            duration: Some(Fx::from_num(1)),
            align_with_speed: false,
            last_orientation: None,
            turned: false,
            disabled_bones: Default::default(),
        }
    }

    /// `of` (owner index) matches a scan of all manipulators through adds,
    /// removes and owner removal, in id order.
    #[test]
    fn owner_index_tracks_adds_and_removes() {
        let mut m = Manipulators::default();
        let ids: Vec<_> = (0..12).map(|k| m.add(k % 3 + 1, ManipulatorKind::Slider(Slider::default()))).collect();
        m.remove(ids[4]);
        m.remove_owners(&[3]);
        for owner in 1..=4 {
            let scan: Vec<_> = m.iter().filter(|x| x.owner == owner).map(|x| x.id).collect();
            let indexed: Vec<_> = m.of(owner).map(|x| x.id).collect();
            assert_eq!(indexed, scan, "owner {owner}");
        }
        assert_eq!(m.of(2).count(), 3, "owner 2 lost the removed one");
        assert_eq!(m.of(3).count(), 0);
    }

    #[test]
    fn animator_plays_to_end_or_loops() {
        let mut a = anim(false, 4.0);
        a.step(Fx::ONE);
        a.step(Fx::ONE);
        assert!(!a.finished());
        a.step(Fx::ONE);
        assert_eq!(a.time, Fx::from_num(1));
        assert!(a.finished());
        let mut l = anim(true, 4.0);
        for _ in 0..3 {
            l.step(Fx::ONE);
        }
        assert!(l.time < Fx::from_num(1) && !l.finished());
        let mut paused = anim(false, 0.0);
        paused.step(Fx::ONE);
        assert!(!paused.finished());
    }

    /// `CreateAnimator(unit, true)`: after movement the clip runs at rate x
    /// this tick's speed / MaxSpeed, at least a quarter when the unit turned
    /// over the tick before; plain rate otherwise.
    #[test]
    fn speed_aligned_animator_follows_the_unit() {
        use crate::engine::api::blueprints::BlueprintKind;
        use crate::engine::sim::entity::{EntityKind, EntityWorld};
        let mut world = EntityWorld::default();
        let unit = world.spawn(EntityKind::Unit, BlueprintKind::Unit, String::new(), Some(1), FxVec3::default(), FxQuat::IDENTITY);
        world.get_mut(unit).unwrap().unit.as_mut().unwrap().motion_params.max_speed = Fx::from_num(4);
        let mut m = Manipulators::default();
        let walk = m.add(unit, ManipulatorKind::Animator(Animator { align_with_speed: true, duration: Some(Fx::from_num(100)), ..anim(true, 8.0) }));
        let plain = m.add(unit, ManipulatorKind::Animator(Animator { duration: Some(Fx::from_num(100)), ..anim(true, 8.0) }));
        let time = |m: &Manipulators, id| match &m.get(id).unwrap().kind {
            ManipulatorKind::Animator(a) => a.time.to_num::<f64>(),
            _ => unreachable!(),
        };
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        // One tick: the pre-movement step, movement (`motion`), then the
        // aligned step.
        let tick = |m: &mut Manipulators, world: &mut EntityWorld, speed: f64, heading: f64| {
            m.step(world);
            let e = world.get_mut(unit).unwrap();
            e.unit.as_mut().unwrap().motion_state.speed = Fx::from_num(speed);
            e.orientation = FxQuat::from_hpr(Fx::from_num(heading), Fx::ZERO, Fx::ZERO);
            m.step_aligned(world);
        };
        // Standing still: no progress (the plain one runs 0.8 s a tick).
        tick(&mut m, &mut world, 0.0, 0.0);
        assert!(near(time(&m, walk), 0.0) && near(time(&m, plain), 0.8));
        // Half speed: half the rate, this tick's speed.
        tick(&mut m, &mut world, 2.0, 0.0);
        assert!(near(time(&m, walk), 0.4), "{}", time(&m, walk));
        // Turning on the spot: the quarter starts the tick after the turn
        // starts and holds one tick past it.
        tick(&mut m, &mut world, 0.0, 0.3);
        assert!(near(time(&m, walk), 0.4), "{}", time(&m, walk));
        tick(&mut m, &mut world, 0.0, 0.3);
        assert!(near(time(&m, walk), 0.6), "{}", time(&m, walk));
        tick(&mut m, &mut world, 0.0, 0.3);
        assert!(near(time(&m, walk), 0.6), "{}", time(&m, walk));
    }

    #[test]
    fn slider_moves_at_speed_then_finishes() {
        let mut s = Slider { goal: FxVec3::from_f64(0.0, 6.0, 0.0), speed: Fx::from_num(20), ..Default::default() };
        s.step();
        assert!((s.current.y - Fx::from_num(2)).abs() < Fx::from_num(0.0001), "20 units/s = 2 per tick, got {}", s.current.y);
        assert!(!s.finished());
        s.step();
        s.step();
        assert!(s.finished() && s.current == s.goal);
        // speed <= 0 jumps.
        s.goal = FxVec3::from_f64(0.0, -6.0, 0.0);
        s.speed = Fx::ZERO;
        s.step();
        assert!(s.finished());
    }
}
