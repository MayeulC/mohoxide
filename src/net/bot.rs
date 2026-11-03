//! A stand-in player for headless network tests (`--net-bot`): every few
//! seconds it orders some of its army's mobile units to a random point, as
//! a person clicking would. The orders go through the lockstep like any
//! player's, so a test game carries real input traffic and every client
//! must apply the same orders on the same tick.
//!
//! Its randomness is local (seeded by the peer id), never the sim's: like a
//! player, it decides on one machine and the decision crosses the network.

use mlua::prelude::*;

use crate::engine::sim::command::{CommandKind, CommandTarget};
use crate::engine::sim::entity::{EntityKind, EntityWorld};
use crate::engine::sim::input::PlayerCommand;
use crate::engine::sim::math::FxVec3;
use crate::engine::sim::movement::MotionKind;
use crate::engine::sim::terrain::Terrain;

/// Ticks between orders.
const EVERY: u64 = 30;
/// Most units in one order.
const GROUP: usize = 12;

/// Sim VM app data in a `--net-bot` client.
pub struct Bot {
    army: u32,
    rng: u64,
}

impl Bot {
    pub fn new(army: u32, peer: u32) -> Bot {
        Bot { army, rng: 0x9e37_79b9_7f4a_7c15 ^ (peer as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9) | 1 }
    }

    fn next(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    /// The orders it gives at tick `now` (reading the sim, never writing).
    pub fn commands(&mut self, lua: &Lua, now: u64) -> Vec<PlayerCommand> {
        if now % EVERY != (self.army as u64 * 7) % EVERY {
            return Vec::new();
        }
        let (w, h) = lua.app_data_ref::<Terrain>().map_or((256, 256), |t| (t.width, t.height));
        let mut units: Vec<u32> = match lua.app_data_ref::<EntityWorld>() {
            Some(world) => world
                .iter()
                .filter(|e| !e.destroyed && e.kind == EntityKind::Unit && e.army == Some(self.army))
                .filter(|e| e.unit.as_deref().is_some_and(|u| u.motion_params.kind != MotionKind::None))
                .map(|e| e.id)
                .collect(),
            None => return Vec::new(),
        };
        if units.is_empty() {
            return Vec::new();
        }
        // A random handful.
        for i in (1..units.len()).rev() {
            let j = (self.next() % (i as u64 + 1)) as usize;
            units.swap(i, j);
        }
        units.truncate(GROUP);
        units.sort_unstable();
        let x = 8 + self.next() % (w.max(17) as u64 - 16);
        let z = 8 + self.next() % (h.max(17) as u64 - 16);
        let target = FxVec3::from_f64(x as f64, 0.0, z as f64);
        vec![PlayerCommand {
            army: self.army,
            kind: CommandKind::Move,
            units,
            target: CommandTarget::Ground(target),
            queue: false,
            formation: None,
            facing: None,
            blueprint: None,
            count: 0,
            toggle: None,
            script: Vec::new(),
            callback: None,
            category: None,
            retarget: None,
        }]
    }
}
