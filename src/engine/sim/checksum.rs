//! Per-tick sim checksum: a hash of the sim state that must be bit-identical
//! on every machine (lockstep desync detection, golden tests).
//!
//! The raw fixed-point bits are packed little-endian into one buffer in a
//! fixed order (entities, projectiles and weapons are all stored id-ordered,
//! armies by index) and hashed once with XXH3 (FNV-1a byte-at-a-time was
//! 844 µs for 10k entities; see docs/archive/benchmark-results.md).

use crate::engine::sim::army::Armies;
use crate::engine::sim::command::{CommandTarget, Commands};
use crate::engine::sim::entity::EntityWorld;
use crate::engine::sim::math::{Fx, FxQuat, FxVec3};
use crate::engine::sim::projectile::Projectiles;
use crate::engine::sim::rng::SimRng;
use crate::engine::sim::weapon::{WeaponTarget, Weapons};

struct Packer(Vec<u8>);

impl Packer {
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn fx(&mut self, v: Fx) {
        self.u64(v.to_bits() as u64);
    }
    fn v3(&mut self, v: FxVec3) {
        self.fx(v.x);
        self.fx(v.y);
        self.fx(v.z);
    }
    fn q(&mut self, q: FxQuat) {
        self.fx(q.x);
        self.fx(q.y);
        self.fx(q.z);
        self.fx(q.w);
    }
}

/// What a checksum covers, part by part: a desync's report names the
/// parts that differ (`net::lockstep`), which points at the subsystem.
pub const PART_NAMES: [&str; 8] = ["entities", "motion", "orders", "projectiles", "weapons", "economy", "rng", "mods"];

/// One hash per [`PART_NAMES`] entry.
pub type Parts = [u64; PART_NAMES.len()];

/// The sim state a checksum reads.
#[derive(Default, Clone, Copy)]
pub struct Sources<'a> {
    pub projectiles: Option<&'a Projectiles>,
    pub weapons: Option<&'a Weapons>,
    pub armies: Option<&'a Armies>,
    pub commands: Option<&'a Commands>,
    pub rng: Option<&'a SimRng>,
    /// Hash of the session's sim mods: peers running different sim mods
    /// run different code, so they differ from the first check.
    pub mods: u64,
}

/// The parts of the checksum of the sim held by `lua` (all 0 before the
/// world exists). Lua state isn't included: it follows from these plus the
/// deterministic Lua VM.
pub fn parts_of_lua(lua: &mlua::Lua) -> Parts {
    let Some(world) = lua.app_data_ref::<EntityWorld>() else { return Parts::default() };
    let projectiles = lua.app_data_ref::<Projectiles>();
    let weapons = lua.app_data_ref::<Weapons>();
    let armies = lua.app_data_ref::<Armies>();
    let commands = lua.app_data_ref::<Commands>();
    let rng = lua.app_data_ref::<SimRng>();
    let mods = lua.app_data_ref::<crate::engine::mods::ModsHash>().map_or(0, |h| h.0);
    let sources = Sources { projectiles: projectiles.as_deref(), weapons: weapons.as_deref(), armies: armies.as_deref(), commands: commands.as_deref(), rng: rng.as_deref(), mods };
    parts(&world, sources)
}

/// The checksum of the sim held by `lua` (0 before the world exists).
pub fn of_lua(lua: &mlua::Lua) -> u64 {
    if lua.app_data_ref::<EntityWorld>().is_none() {
        return 0;
    }
    combine(&parts_of_lua(lua))
}

/// One checksum from its parts.
pub fn combine(parts: &Parts) -> u64 {
    let mut bytes = [0u8; 8 * PART_NAMES.len()];
    for (chunk, p) in bytes.chunks_exact_mut(8).zip(parts) {
        chunk.copy_from_slice(&p.to_le_bytes());
    }
    xxhash_rust::xxh3::xxh3_64(&bytes)
}

pub fn checksum(world: &EntityWorld, sources: Sources) -> u64 {
    combine(&parts(world, sources))
}

thread_local! {
    /// The packing buffers, kept between checksums: a fresh 1.5 MB each
    /// tick costs more in page faults than the hashing.
    static BUFFERS: std::cell::RefCell<(Vec<u8>, Vec<u8>)> = const { std::cell::RefCell::new((Vec::new(), Vec::new())) };
}

/// Each part packed in turn into one buffer, then each hashed.
pub fn parts(world: &EntityWorld, sources: Sources) -> Parts {
    BUFFERS.with_borrow_mut(|(main_buffer, motion_buffer)| {
        let mut main = Packer(std::mem::take(main_buffer));
        let mut motion = Packer(std::mem::take(motion_buffer));
        main.0.clear();
        motion.0.clear();
        let out = pack_and_hash(world, sources, &mut main, &mut motion);
        *main_buffer = main.0;
        *motion_buffer = motion.0;
        out
    })
}

fn pack_and_hash(world: &EntityWorld, sources: Sources, h: &mut Packer, motion: &mut Packer) -> Parts {
    let mut ends = [0usize; PART_NAMES.len()];
    let mut part = 0;
    let mut end = |h: &Packer, part: &mut usize| {
        ends[*part] = h.0.len();
        *part += 1;
    };
    // Motion (its own part, packed in the same pass): where units are
    // heading, not just where they are (two clients can agree on a
    // position while already moving apart). Speed, course and spin, with
    // the facing, are the velocity.
    for e in world.iter().filter(|e| !e.destroyed) {
        h.u64(e.id as u64);
        h.u64(e.kind as u64);
        h.u64(e.army.map_or(u64::MAX, u64::from));
        h.v3(e.position());
        h.q(e.orientation);
        h.fx(e.health);
        h.fx(e.max_health);
        h.fx(e.fraction_complete);
        if let Some(u) = e.unit.as_deref() {
            let m = &u.motion_state;
            motion.u64(e.id as u64);
            motion.fx(m.speed);
            motion.fx(m.course.unwrap_or(Fx::MIN));
            motion.v3(m.spin);
        }
    }
    end(h, &mut part);
    h.0.extend_from_slice(&motion.0);
    end(h, &mut part);
    // Orders: every busy unit's queue (ids, kinds, targets, counts), so a
    // queue change shows before it moves anything.
    if let Some(commands) = sources.commands {
        for unit in commands.busy_units() {
            h.u64(unit as u64);
            for c in commands.queue(unit) {
                h.u64(c.id as u64);
                h.u64(c.kind as u64);
                match c.target {
                    CommandTarget::None => h.u64(u64::MAX),
                    CommandTarget::Entity(id) => h.u64(id as u64),
                    CommandTarget::Ground(p) => h.v3(p),
                }
                h.u64(c.count as u64);
            }
        }
    }
    end(h, &mut part);
    for p in sources.projectiles.into_iter().flat_map(|p| p.iter()) {
        h.u64(p.id as u64);
        h.v3(p.velocity());
        h.u64(p.lifetime_ticks.map_or(u64::MAX, u64::from));
    }
    end(h, &mut part);
    for w in sources.weapons.into_iter().flat_map(|w| w.iter()) {
        h.u64(((w.unit as u64) << 32) | w.index as u64);
        match w.target {
            Some(WeaponTarget::Entity(id)) => h.u64(id as u64),
            Some(WeaponTarget::Ground(p)) => h.v3(p),
            None => h.u64(u64::MAX),
        }
        h.fx(w.aim_heading);
        h.fx(w.aim_pitch);
        h.u64(w.fire_clock as u64);
    }
    end(h, &mut part);
    // The whole economy record: income and spending part before storage does.
    for a in sources.armies.into_iter().flat_map(|a| a.list.iter()) {
        h.u64(a.index as u64);
        for r in [&a.economy.mass, &a.economy.energy] {
            for v in [r.stored, r.granted_storage, r.capacity, r.income, r.requested, r.expense, r.total_produced, r.total_consumed, r.reclaimed, r.accumulated_excess, r.peak_stored, r.banked, r.total_shared] {
                h.fx(v);
            }
        }
    }
    end(h, &mut part);
    if let Some(rng) = sources.rng {
        h.u64(rng.state_hash());
    }
    end(h, &mut part);
    h.u64(sources.mods);
    end(h, &mut part);
    debug_assert_eq!(part, PART_NAMES.len());
    let mut out = Parts::default();
    let mut start = 0;
    for (o, &e) in out.iter_mut().zip(&ends) {
        *o = xxhash_rust::xxh3::xxh3_64(&h.0[start..e]);
        start = e;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A change shows in its own part only, and the checksum is the parts'.
    #[test]
    fn a_change_shows_in_its_own_part() {
        let world = EntityWorld::default();
        let mut rng = SimRng::new(3);
        let base = parts(&world, Sources { rng: Some(&rng), mods: 1, ..Default::default() });
        assert_eq!(checksum(&world, Sources { rng: Some(&rng), mods: 1, ..Default::default() }), combine(&base));
        rng.next_u64();
        let moved = parts(&world, Sources { rng: Some(&rng), mods: 2, ..Default::default() });
        let differing: Vec<&str> = PART_NAMES.iter().zip(base.iter().zip(&moved)).filter(|(_, (a, b))| a != b).map(|(n, _)| *n).collect();
        assert_eq!(differing, ["rng", "mods"]);
    }
}
