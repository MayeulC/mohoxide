//! Formation slot assignment (`IssueFormMove` and friends).
//!
//! FAF's `/lua/formations.lua` lays a formation out as a list of slots
//! `{x, z, categoryFilter, row, rotate}` in formation units (x across, z
//! forward, back rows negative). The engine scales them by the group's
//! largest footprint + 2 (per the comment in `CalculateSizes`), turns them to
//! the formation's heading around the target and gives each slot, in list
//! order, the nearest still-unassigned unit its filter accepts (inferred:
//! the real assignment rule isn't documented). Units left without a slot
//! head for the target itself. The group's speed is 0.85 x its slowest
//! unit's (MohoEngine.dll 0x10155bb0: the minimum of `MaxSpeed` x 0.85 over
//! the members).
//!
//! Travel (orders to a point): each group moves as a block, as in
//! SupCom. Its centre starts at the members' mean position and follows one
//! path to the target (one search for the whole group, on the most
//! restrictive footprint class among its members). It is the group's
//! progress: for the order's first [`FORMING_TICKS`] it runs at 0.85 x the
//! group's speed, as every member does, then at the group's speed, and
//! each member's speed follows how far it is behind its slot riding with
//! the centre ([`member_speed`]). It turns towards each leg at
//! `TURN_RATE`, and the slots turn with it. Members steer for their slot
//! laid out at the centre's next waypoint (the final slot on the last leg)
//! while the line there is clear, so a block widens into its line on the
//! way instead of turning across; one cut off (a choke narrower than the
//! formation, a slot in a cliff) follows the centre's path instead, so the
//! block files through in a column. Once the centre is at the target,
//! members take their final slots and the order completes per unit as
//! before. (The real game's members: dev/probes probe_landform and
//! probe_formphase, docs/research/verifying-against-the-binary.md.)

use crate::engine::sim::category::CategorySet;
use crate::engine::sim::command::FormationSlot;
use crate::engine::sim::entity::EntityId;
use crate::engine::sim::math::{atan2, sin_cos, sqrt, Fx, FxVec3};

/// One slot as `formations.lua` returned it (formation units).
#[derive(Clone, Debug)]
pub struct RawSlot {
    pub x: Fx,
    pub z: Fx,
    /// Units this slot takes; `None` = any.
    pub filter: Option<CategorySet>,
    /// Turn with the formation's heading (FAF always sets it).
    pub rotate: bool,
}

/// A unit joining the formation.
#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub id: EntityId,
    pub position: FxVec3,
    pub category: Option<u32>,
    pub max_speed: Fx,
}

/// Heading (radians, 0 = +z) from the group's centre towards `target`; 0
/// when already there.
pub fn heading_towards(members: &[Member], target: FxVec3) -> Fx {
    if members.is_empty() {
        return Fx::ZERO;
    }
    let n = Fx::from_num(members.len() as i32);
    let (sx, sz) = members.iter().fold((Fx::ZERO, Fx::ZERO), |(x, z), m| (x + m.position.x / n, z + m.position.z / n));
    let (dx, dz) = (target.x - sx, target.z - sz);
    if dx == Fx::ZERO && dz == Fx::ZERO { Fx::ZERO } else { atan2(dx, dz) }
}

/// Slots for `members` (any order) heading `heading`, scaled by `scale`,
/// as formation group `group`.
pub fn assign(target: FxVec3, heading: Fx, scale: Fx, slots: &[RawSlot], members: &[Member], group: u8) -> Vec<FormationSlot> {
    let speed_cap = members.iter().map(|m| m.max_speed).min().unwrap_or(Fx::ZERO).saturating_mul(speed_factor());
    let (s, c) = sin_cos(heading);
    // Formation +z is the heading; +x is to its right.
    let mut taken = vec![false; members.len()];
    let mut out = Vec::with_capacity(members.len());
    for slot in slots {
        let (x, z) = (slot.x.saturating_mul(scale), slot.z.saturating_mul(scale));
        let (dx, dz) = if slot.rotate { (x * c + z * s, z * c - x * s) } else { (x, z) };
        let at = (target.x + dx, target.z + dz);
        let best = members
            .iter()
            .enumerate()
            .filter(|&(i, m)| !taken[i] && slot.filter.as_ref().is_none_or(|f| m.category.is_some_and(|c| f.contains(c))))
            .map(|(i, m)| {
                let (ex, ez) = (m.position.x - at.0, m.position.z - at.1);
                (ex.saturating_mul(ex).saturating_add(ez.saturating_mul(ez)), m.id, i)
            })
            .min_by_key(|&(d, id, _)| (d, id));
        if let Some((_, id, i)) = best {
            taken[i] = true;
            out.push(FormationSlot { unit: id, dx, dz, speed_cap, lx: x, lz: z, rotate: slot.rotate, group });
        }
    }
    // The rest still travel with the group.
    for (i, m) in members.iter().enumerate() {
        if !taken[i] {
            out.push(FormationSlot { unit: m.id, dx: Fx::ZERO, dz: Fx::ZERO, speed_cap, lx: Fx::ZERO, lz: Fx::ZERO, rotate: false, group });
        }
    }
    out.sort_unstable_by_key(|s| s.unit);
    out
}

/// Turn rate of a travelling formation (radians per second).
fn turn_rate() -> Fx {
    Fx::FRAC_PI_4
}

/// A group's speed over its slowest unit's (MohoEngine.dll 0x10155bb0), and
/// a member's while the group is forming (0x1018a990's default).
fn speed_factor() -> Fx {
    Fx::from_num(0.85)
}

/// Ticks from the order during which every member runs at 0.85 x the
/// group's speed and the centre keeps with them: the real game's members
/// switch to keeping their slots ~100 ticks after the order whatever the
/// distance, speed, size or starting layout (dev/probes probe_formphase).
pub const FORMING_TICKS: u64 = 100;

/// A member's speed: 0.85 x the group's `speed` while it forms, then the
/// group's speed x `1 + 0.1 x clamp(k x lag, -5, 20)` (0.5 to 3), `lag`
/// being how far it is behind its slot along the group's heading, `k` 4
/// for surface groups and 1.5 for air (MohoEngine.dll 0x1018a990, the
/// formation's per-unit speed). The caller caps it at the unit's top speed.
pub fn member_speed(speed: Fx, forming: bool, lag: Fx, surface: bool) -> Fx {
    if forming {
        return speed.saturating_mul(speed_factor());
    }
    let k = if surface { Fx::from_num(4) } else { Fx::from_num(1.5) };
    let m = Fx::ONE + k.saturating_mul(lag).clamp(Fx::from_num(-5), Fx::from_num(20)) / 10;
    speed.saturating_mul(m)
}
/// Within this of a waypoint the centre turns for the next one.
fn waypoint_reach() -> Fx {
    Fx::ONE
}

/// One formation group on its way to the target (see the module docs).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Travel {
    pub target: FxVec3,
    /// Heading the final layout faces.
    pub final_heading: Fx,
    pub speed_cap: Fx,
    /// Distance between neighbouring slots (the layout's scale): slots
    /// moved off blocked ground keep at least this apart.
    pub spacing: Fx,
    /// Surface group (paths on the nav grid); air flies straight.
    pub surface: bool,
    pub planned: bool,
    pub done: bool,
    pub centre: FxVec3,
    pub heading: Fx,
    /// `sin_cos(heading)`.
    sc: (Fx, Fx),
    /// The centre's speed this tick.
    pub speed: Fx,
    pub path: Vec<FxVec3>,
    pub next: usize,
    /// Nav grid version the path was last checked against.
    version: u32,
    /// Sim tick of the order: the group forms for [`FORMING_TICKS`] after.
    pub issued: u64,
    /// Where members aim: the centre's next waypoint, and `sin_cos` of its
    /// leg's heading ([`Travel::aim_position`]).
    aim_at: FxVec3,
    aim_sc: (Fx, Fx),
}

impl Travel {
    pub fn new(target: FxVec3, final_heading: Fx, speed_cap: Fx, surface: bool) -> Self {
        Self {
            target,
            final_heading,
            speed_cap,
            spacing: Fx::from_num(3),
            surface,
            planned: false,
            done: false,
            centre: target,
            heading: final_heading,
            sc: sin_cos(final_heading),
            speed: Fx::ZERO,
            path: Vec::new(),
            next: 0,
            version: 0,
            issued: 0,
            aim_at: target,
            aim_sc: sin_cos(final_heading),
        }
    }

    /// The order moved to `target` (its waypoint dragged): the group plans
    /// again from where its members are.
    pub fn retarget(&mut self, target: FxVec3) {
        self.target = target;
        self.planned = false;
        self.done = false;
        self.path.clear();
        self.next = 0;
        self.aim_at = target;
    }

    /// Still forming at `tick` (see [`FORMING_TICKS`]).
    pub fn forming(&self, tick: u64) -> bool {
        tick < self.issued.saturating_add(FORMING_TICKS)
    }

    /// `sin_cos` of the centre's heading (a member's lag is measured along
    /// it).
    pub fn forward(&self) -> (Fx, Fx) {
        self.sc
    }

    /// Where `slot`'s unit steers: its slot laid out at the centre's next
    /// waypoint, facing that leg (its final slot on the last leg), so it
    /// closes on its place in the line over the leg rather than turning
    /// across for the slot beside it. The real game's members head for
    /// their final slots from the start (heading off by atan(lateral offset
    /// / distance to go), dev/probes probe_landform).
    pub fn aim_position(&self, slot: &FormationSlot) -> FxVec3 {
        if self.done || self.aim_at == self.target {
            return FxVec3 { x: self.target.x + slot.dx, y: self.target.y, z: self.target.z + slot.dz };
        }
        let (s, c) = self.aim_sc;
        let (dx, dz) = if slot.rotate { (slot.lx * c + slot.lz * s, slot.lz * c - slot.lx * s) } else { (slot.lx, slot.lz) };
        FxVec3 { x: self.aim_at.x + dx, y: self.aim_at.y, z: self.aim_at.z + dz }
    }

    /// The point members' aims are laid out round ([`Travel::aim_position`]).
    pub fn aim_centre(&self) -> FxVec3 {
        if self.aim_at == self.target { self.target } else { self.aim_at }
    }

    /// Lays the aim out at the centre's next waypoint (the target on the
    /// last leg), facing the leg into it.
    fn update_aim(&mut self) {
        let last = self.next + 1 >= self.path.len();
        let at = if last { self.target } else { self.path[self.next] };
        if at == self.aim_at {
            return;
        }
        let from = if self.next == 0 { self.centre } else { self.path[self.next - 1] };
        self.aim_sc = if last {
            sin_cos(self.final_heading)
        } else if at == from {
            self.sc
        } else {
            sin_cos(atan2(at.x - from.x, at.z - from.z))
        };
        self.aim_at = at;
    }

    /// Where `slot` is now: riding with the centre, or (once there) its
    /// final place round the target.
    pub fn slot_position(&self, slot: &FormationSlot) -> FxVec3 {
        if self.done {
            return FxVec3 { x: self.target.x + slot.dx, y: self.target.y, z: self.target.z + slot.dz };
        }
        let (s, c) = self.sc;
        let (dx, dz) = if slot.rotate { (slot.lx * c + slot.lz * s, slot.lz * c - slot.lx * s) } else { (slot.lx, slot.lz) };
        FxVec3 { x: self.centre.x + dx, y: self.centre.y, z: self.centre.z + dz }
    }

    /// Where a member cut off from its slot heads: the centre's next
    /// waypoint (fixed for a while, so its own path is searched rarely).
    pub fn follow_point(&self) -> FxVec3 {
        self.path.get(self.next).copied().unwrap_or(self.target)
    }

    fn set_heading(&mut self, h: Fx) {
        if h != self.heading {
            self.heading = h;
            self.sc = sin_cos(h);
        }
    }
}

fn dist2d(a: FxVec3, b: FxVec3) -> Fx {
    let (dx, dz) = (b.x - a.x, b.z - a.z);
    sqrt(dx.saturating_mul(dx).saturating_add(dz.saturating_mul(dz)))
}

/// Moves every travelling formation one tick (serially, in command order;
/// before the movement pass, which steers members to the new slots).
/// Plans a group's path on its first tick. The centre doesn't wait for
/// stragglers: members behind their slots speed up instead
/// ([`member_speed`]). Returns the path searches made and their work
/// (`nav::Search::work`), for profiling.
pub fn travel_tick(
    commands: &mut crate::engine::sim::command::Commands,
    world: &crate::engine::sim::entity::EntityWorld,
    nav: Option<&crate::engine::sim::nav::NavGrid>,
    tick: u64,
    search: &mut crate::engine::sim::nav::Search,
) -> (u64, u64) {
    let dt = Fx::ONE / Fx::from_num(10);
    let mut stats = (0u64, 0u64);
    for id in commands.travelling() {
        let Some(cmd) = commands.get(id) else { continue };
        // Members still on this order, per group: (position, slot).
        let groups: Vec<Vec<(FxVec3, FormationSlot, Option<crate::engine::sim::nav::NavClass>)>> = (0..cmd.travel.len())
            .map(|g| {
                cmd.formation
                    .iter()
                    .filter(|s| s.group as usize == g && commands.current(s.unit).is_some_and(|c| c.id == id))
                    .filter_map(|s| {
                        let e = world.get(s.unit).filter(|e| !e.destroyed)?;
                        let class = nav.and_then(|n| crate::engine::sim::movement::nav_class(e.unit.as_deref()?, n));
                        Some((e.position(), *s, class))
                    })
                    .collect()
            })
            .collect();
        let Some(cmd) = commands.get_mut(id) else { continue };
        let crate::engine::sim::command::Command { travel: travels, formation, .. } = cmd;
        for (travel, members) in travels.iter_mut().zip(groups) {
            if travel.done || members.is_empty() {
                continue;
            }
            if !travel.planned {
                let n = Fx::from_num(members.len() as i32);
                let (x, z) = members.iter().fold((Fx::ZERO, Fx::ZERO), |(x, z), m| (x + m.0.x / n, z + m.0.z / n));
                travel.centre = FxVec3 { x, y: travel.target.y, z };
                travel.planned = true;
                plan(travel, &members, nav, search, &mut stats);
            } else if let Some(nav) = nav.filter(|n| travel.surface && n.version != travel.version) {
                // The grid changed: re-plan if the rest of the way is blocked.
                let class = group_class(&members, nav);
                let clear = class.is_none_or(|c| nav.path_clear(c, travel.centre, &travel.path[travel.next.min(travel.path.len())..]));
                travel.version = nav.version;
                if !clear {
                    plan(travel, &members, Some(nav), search, &mut stats);
                }
            }
            // Forming, the centre keeps with the members (all at 0.85 x).
            travel.speed = if travel.forming(tick) { travel.speed_cap.saturating_mul(speed_factor()) } else { travel.speed_cap };
            // Along the path, turning for each leg.
            let mut step = travel.speed * dt;
            while step > Fx::ZERO && travel.next < travel.path.len() {
                let w = travel.path[travel.next];
                let d = dist2d(travel.centre, w);
                if d <= step || d <= waypoint_reach() && travel.next + 1 < travel.path.len() {
                    step -= d.min(step);
                    travel.centre = FxVec3 { x: w.x, y: travel.centre.y, z: w.z };
                    travel.next += 1;
                    continue;
                }
                travel.centre.x += (w.x - travel.centre.x) * step / d;
                travel.centre.z += (w.z - travel.centre.z) * step / d;
                step = Fx::ZERO;
            }
            if travel.next >= travel.path.len() {
                travel.done = true;
                travel.speed = Fx::ZERO;
                if let (true, Some(nav)) = (travel.surface, nav) {
                    fit_final(nav, travel.target, travel.spacing, &members, formation);
                }
                continue;
            }
            travel.update_aim();
            let w = travel.path[travel.next];
            let want = if w == travel.centre { travel.heading } else { atan2(w.x - travel.centre.x, w.z - travel.centre.z) };
            let mut err = want - travel.heading;
            if err > Fx::PI {
                err -= Fx::PI * 2;
            } else if err <= -Fx::PI {
                err += Fx::PI * 2;
            }
            let turn = turn_rate() * dt;
            let mut h = travel.heading + err.clamp(-turn, turn);
            if h > Fx::PI {
                h -= Fx::PI * 2;
            } else if h <= -Fx::PI {
                h += Fx::PI * 2;
            }
            travel.set_heading(h);
        }
    }
    stats
}

/// The centre has arrived: final slots its members can't stand on (or
/// reach) move to ground they can (`NavGrid::fit_slots`), so the edges of
/// a formation sent against a cliff or the shore fold in instead of leaving
/// units stuck short of slots they'll never reach.
fn fit_final(nav: &crate::engine::sim::nav::NavGrid, target: FxVec3, spacing: Fx, members: &[(FxVec3, FormationSlot, Option<crate::engine::sim::nav::NavClass>)], formation: &mut [FormationSlot]) {
    // Members are in unit order, as `formation` is.
    let mut slots: Vec<(crate::engine::sim::nav::NavClass, FxVec3)> = Vec::with_capacity(members.len());
    let mut units = Vec::with_capacity(members.len());
    for (_, s, class) in members {
        let Some(c) = class else { continue };
        slots.push((*c, FxVec3 { x: target.x + s.dx, y: target.y, z: target.z + s.dz }));
        units.push(s.unit);
    }
    if nav.fit_slots(target, spacing.to_num::<u32>().max(1), &mut slots) == 0 {
        return;
    }
    for (unit, (_, p)) in units.into_iter().zip(slots) {
        if let Ok(i) = formation.binary_search_by_key(&unit, |s| s.unit) {
            formation[i].dx = p.x - target.x;
            formation[i].dz = p.z - target.z;
        }
    }
}

/// The footprint class a surface group's centre paths as: the biggest
/// among members that don't walk over structures (so the path suits the
/// tanks, not just the experimentals), else the first member's.
fn group_class(members: &[(FxVec3, FormationSlot, Option<crate::engine::sim::nav::NavClass>)], nav: &crate::engine::sim::nav::NavGrid) -> Option<crate::engine::sim::nav::NavClass> {
    let classes = nav.classes();
    let respecting = members.iter().filter_map(|m| m.2).filter(|c| !classes[c.0 as usize].ignore_structures).max_by_key(|c| (classes[c.0 as usize].size, std::cmp::Reverse(c.0)));
    respecting.or_else(|| members.iter().find_map(|m| m.2))
}

fn plan(
    travel: &mut Travel,
    members: &[(FxVec3, FormationSlot, Option<crate::engine::sim::nav::NavClass>)],
    nav: Option<&crate::engine::sim::nav::NavGrid>,
    search: &mut crate::engine::sim::nav::Search,
    stats: &mut (u64, u64),
) {
    travel.next = 0;
    travel.path = match nav.filter(|_| travel.surface).and_then(|n| Some((n, group_class(members, n)?))) {
        Some((nav, class)) => {
            travel.version = nav.version;
            let mut p = crate::engine::sim::nav::find_path(nav, search, class, travel.centre, travel.target, crate::engine::sim::movement::PATH_BUDGET);
            *stats = (stats.0 + 1, stats.1 + search.work.max(1));
            // The search ends at the reachable point nearest an unreachable
            // target: the formation lays out there.
            if let Some(&end) = p.last() {
                travel.target = FxVec3 { x: end.x, y: travel.target.y, z: end.z };
            }
            if p.is_empty() {
                p.push(travel.target);
            }
            p
        }
        None => vec![travel.target],
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: EntityId, x: f64, z: f64, speed: f64) -> Member {
        Member { id, position: FxVec3::from_f64(x, 0.0, z), category: Some(0), max_speed: Fx::from_num(speed) }
    }

    fn slot(x: f64, z: f64) -> RawSlot {
        RawSlot { x: Fx::from_num(x), z: Fx::from_num(z), filter: None, rotate: true }
    }

    #[test]
    fn a_row_faces_the_heading_and_keeps_units_on_their_side() {
        let target = FxVec3::from_f64(100.0, 0.0, 100.0);
        // Two units side by side heading +x (east): heading pi/2.
        let members = [member(1, 0.0, 95.0, 3.0), member(2, 0.0, 105.0, 2.0)];
        let h = heading_towards(&members, target);
        assert!((h.to_num::<f64>() - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
        let out = assign(target, h, Fx::from_num(3), &[slot(-1.0, 0.0), slot(1.0, 0.0)], &members, 0);
        // Heading east, the formation's right (+x) is south (-z).
        let s1 = out.iter().find(|s| s.unit == 1).unwrap();
        let s2 = out.iter().find(|s| s.unit == 2).unwrap();
        assert!(s1.dz.to_num::<f64>() < -2.9 && s2.dz.to_num::<f64>() > 2.9, "no crossing: {s1:?} {s2:?}");
        assert!(s1.dx.abs() < Fx::from_num(0.001));
        assert_eq!(s1.speed_cap, Fx::from_num(2).saturating_mul(Fx::from_num(0.85)), "0.85 x the slowest unit's speed");
    }

    #[test]
    fn filters_pick_matching_units_and_leftovers_still_join() {
        let only_cat1 = CategorySet::from_indices(4, [1]);
        let mut a = member(1, 0.0, 0.0, 3.0);
        a.category = Some(0);
        let mut b = member(2, 50.0, 50.0, 3.0);
        b.category = Some(1);
        let c = member(3, 1.0, 0.0, 3.0);
        let slots = [RawSlot { filter: Some(only_cat1), ..slot(0.0, 0.0) }];
        let out = assign(FxVec3::default(), Fx::ZERO, Fx::ONE, &slots, &[a, b, c], 0);
        assert_eq!(out.len(), 3, "everyone gets an entry");
        assert_eq!(out.iter().map(|s| s.unit).collect::<Vec<_>>(), vec![1, 2, 3], "sorted by unit");
    }

    fn travelling(units: &[(f64, f64)], target: (f64, f64)) -> (crate::engine::sim::entity::EntityWorld, crate::engine::sim::command::Commands, crate::engine::sim::command::CommandId, Vec<EntityId>) {
        use crate::engine::api::blueprints::BlueprintKind;
        use crate::engine::sim::command::{CommandKind, CommandTarget, Commands};
        use crate::engine::sim::entity::{EntityKind, EntityWorld};
        use crate::engine::sim::math::FxQuat;
        let mut w = EntityWorld::default();
        let ids: Vec<EntityId> = units.iter().map(|&(x, z)| w.spawn(EntityKind::Unit, BlueprintKind::Unit, String::new(), Some(1), FxVec3::from_f64(x, 0.0, z), FxQuat::IDENTITY)).collect();
        let mut c = Commands::default();
        let t = FxVec3::from_f64(target.0, 0.0, target.1);
        let cmd = c.issue(&ids, CommandKind::FormMove, CommandTarget::Ground(t), None, 0);
        let members: Vec<Member> = ids.iter().zip(units).map(|(&id, &(x, z))| member(id, x, z, 2.0)).collect();
        let h = heading_towards(&members, t);
        let slots = assign(t, h, Fx::from_num(3), &[slot(-1.0, 0.0), slot(1.0, 0.0)], &members, 0);
        c.set_formation(cmd, slots);
        c.set_travel(cmd, vec![Travel::new(t, h, Fx::from_num(2), true)]);
        (w, c, cmd, ids)
    }

    /// A tick once the group has formed (issued at 0).
    fn tick(w: &crate::engine::sim::entity::EntityWorld, c: &mut crate::engine::sim::command::Commands) {
        travel_tick(c, w, None, FORMING_TICKS, &mut Default::default());
    }

    #[test]
    fn a_formation_moves_at_its_cap_once_formed_and_finishes_at_the_target() {
        // Two units already on their slots (3 either side of the centre,
        // heading +z), target 10 ahead.
        let (mut w, mut c, cmd, ids) = travelling(&[(-3.0, 0.0), (3.0, 0.0)], (0.0, 10.0));
        tick(&w, &mut c);
        let t = &c.get(cmd).unwrap().travel[0];
        assert!(t.planned && !t.done);
        assert_eq!(t.speed, Fx::from_num(2), "formed: the group's speed");
        assert!((t.centre.z.to_num::<f64>() - 0.2).abs() < 1e-6, "0.2 a tick at 2/s: {:?}", t.centre);
        // Members ride along: put them on their moving slots each tick.
        for _ in 0..60 {
            let cmd_ref = c.get(cmd).unwrap();
            let moves: Vec<(EntityId, FxVec3)> = ids.iter().map(|&u| (u, cmd_ref.travel[0].slot_position(cmd_ref.slot(u).unwrap()))).collect();
            for (u, p) in moves {
                w.set_transform(u, p, crate::engine::sim::math::FxQuat::IDENTITY);
            }
            tick(&w, &mut c);
        }
        let t = &c.get(cmd).unwrap().travel[0];
        assert!(t.done, "at the target after 10 / 2 = 5 s: {:?}", t.centre);
        assert!(c.travelling().is_empty());
    }

    #[test]
    fn a_forming_group_runs_at_085_and_none_waits_for_stragglers() {
        // One unit 60 behind the other: both ~30 from their slots.
        let (w, mut c, cmd, _) = travelling(&[(-3.0, 0.0), (3.0, -60.0)], (0.0, 100.0));
        travel_tick(&mut c, &w, None, FORMING_TICKS - 1, &mut Default::default());
        assert_eq!(c.get(cmd).unwrap().travel[0].speed, Fx::from_num(2).saturating_mul(Fx::from_num(0.85)), "forming");
        tick(&w, &mut c);
        assert_eq!(c.get(cmd).unwrap().travel[0].speed, Fx::from_num(2), "formed: full speed, stragglers or not");
    }

    #[test]
    fn members_keep_their_slots_by_speed() {
        let f = Fx::from_num(2);
        let at = |lag: f64, surface| member_speed(f, false, Fx::from_num(lag), surface).to_num::<f64>();
        assert_eq!(member_speed(f, true, Fx::from_num(5), true), f.saturating_mul(Fx::from_num(0.85)), "forming: 0.85 x, lag or not");
        assert!((at(0.0, true) - 2.0).abs() < 1e-6, "on its slot: the group's speed");
        assert!((at(1.0, true) - 2.8).abs() < 1e-6, "1 behind: 1.4 x");
        assert!((at(-2.0, true) - 1.0).abs() < 1e-6, "ahead: down to half");
        assert!((at(100.0, true) - 6.0).abs() < 1e-6, "far behind: at most 3 x");
        assert!((at(1.0, false) - 2.3).abs() < 1e-6, "aircraft: 0.15 a unit behind");
    }

    #[test]
    fn members_aim_at_their_slot_at_the_next_waypoint() {
        let (w, mut c, cmd, ids) = travelling(&[(-3.0, 0.0), (3.0, 0.0)], (0.0, 100.0));
        tick(&w, &mut c);
        let slot = *c.get(cmd).unwrap().slot(ids[1]).unwrap();
        // Last leg: the final slot.
        let t = &c.get(cmd).unwrap().travel[0];
        assert_eq!(t.aim_position(&slot), FxVec3 { x: t.target.x + slot.dx, y: t.target.y, z: t.target.z + slot.dz });
        // A corner first: laid out at the corner, facing the leg into it (east).
        {
            let t = &mut c.get_mut(cmd).unwrap().travel[0];
            t.path = vec![FxVec3::from_f64(0.0, 0.0, 50.0), FxVec3::from_f64(50.0, 0.0, 50.0), t.target];
            t.next = 1;
        }
        tick(&w, &mut c);
        let t = &c.get(cmd).unwrap().travel[0];
        let p = t.aim_position(&slot);
        let (dx, dz) = ((p.x - Fx::from_num(50)).to_num::<f64>(), (p.z - Fx::from_num(50)).to_num::<f64>());
        let (rx, rz) = (slot.lx.to_num::<f64>(), slot.lz.to_num::<f64>());
        // Facing +x: formation right (+x local) is world -z.
        assert!((dx - rz).abs() < 1e-3 && (dz + rx).abs() < 1e-3, "slot ({rx}, {rz}) at the corner, facing east: ({dx}, {dz})");
    }

    #[test]
    fn slots_turn_with_the_formation_at_its_turn_rate() {
        let (w, mut c, cmd, ids) = travelling(&[(-3.0, 0.0), (3.0, 0.0)], (0.0, 100.0));
        tick(&w, &mut c);
        // Now send it east: the centre's path turns 90 degrees.
        {
            let t = &mut c.get_mut(cmd).unwrap().travel[0];
            t.path = vec![FxVec3::from_f64(100.0, 0.0, t.centre.z.to_num::<f64>())];
            t.next = 0;
        }
        tick(&w, &mut c);
        let t = &c.get(cmd).unwrap().travel[0];
        let turned = t.heading.to_num::<f64>();
        assert!((turned - std::f64::consts::FRAC_PI_4 / 10.0).abs() < 1e-3, "45 deg/s for a tick: {turned}");
        // A rotating slot sits off the centre at the new heading.
        let cmd_ref = c.get(cmd).unwrap();
        let p = t.slot_position(cmd_ref.slot(ids[1]).unwrap());
        let (dx, dz) = ((p.x - t.centre.x).to_num::<f64>(), (p.z - t.centre.z).to_num::<f64>());
        assert!((dx.hypot(dz) - 3.0).abs() < 1e-3 && dz.abs() > 0.1, "slot rotated about the centre: ({dx}, {dz})");
    }
}
