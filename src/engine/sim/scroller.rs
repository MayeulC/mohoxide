//! Texture scrollers (moho `CTextureScroller`, MohoEngine.dll 0x1025efe0,
//! ticked at 0x10342550): two scroll values per entity that the mesh shader
//! adds to U in the treads' bands of the texture (V above 0.95: the first,
//! above 0.90: the second; FA's `mesh.fx` `ComputeScrolledTexcoord`).
//!
//! - Thread (`AddThreadScroller(sideDist, scrollMult)`, "motion derived"):
//!   each tick a track `sideDist` either side of the centre advances by its
//!   forward travel since last tick x `scrollMult`, so turning on the spot
//!   runs the tracks opposite ways.
//! - Manual (`AddManualScroller(speed1, speed2)`): constant speeds per tick.
//! - Ping-pong (`AddPingPongScroller(ping1, speed1, pong1, speed2, ...)`,
//!   aircraft): each value runs from ping to pong at its speed and back
//!   (inferred: moho toggles on tick countdowns).
//!
//! Presentation state, never read by Lua; fixed point like the rest of the
//! sim. Only entities with a scroller are visited (`Scrolled`).

use crate::engine::sim::entity::{EntityId, EntityWorld};
use crate::engine::sim::math::{Fx, FxQuat, FxVec3};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScrollKind {
    Thread { side: Fx, mult: Fx },
    Manual { speed: (Fx, Fx) },
    /// Per value: (ping, ping speed, pong, pong speed), and which way it's going.
    PingPong { values: [(Fx, Fx, Fx, Fx); 2], back: [bool; 2] },
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scroller {
    pub kind: ScrollKind,
    /// Last tick's values and this tick's (the renderer blends them).
    pub previous: (Fx, Fx),
    pub current: (Fx, Fx),
    /// The transform it last saw (thread scrollers).
    seen: (FxVec3, FxQuat),
}

impl Scroller {
    pub fn new(kind: ScrollKind, position: FxVec3, orientation: FxQuat) -> Self {
        let start = match kind {
            ScrollKind::PingPong { values, .. } => (values[0].0, values[1].0),
            _ => (Fx::ZERO, Fx::ZERO),
        };
        Self { kind, previous: start, current: start, seen: (position, orientation) }
    }

    /// One tick, the entity now at `position` / `orientation`.
    pub fn step(&mut self, position: FxVec3, orientation: FxQuat) {
        self.previous = self.current;
        match &mut self.kind {
            ScrollKind::Thread { side, mult } => {
                let (p0, q0) = self.seen;
                if p0 == position && q0 == orientation {
                    return;
                }
                let x = FxVec3 { x: Fx::ONE, y: Fx::ZERO, z: Fx::ZERO };
                let z = FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::ONE };
                let (r0, r1) = (q0.rotate(x), orientation.rotate(x));
                let (f0, f1) = (q0.rotate(z), orientation.rotate(z));
                let forward = FxVec3 { x: (f0.x + f1.x) / 2, y: (f0.y + f1.y) / 2, z: (f0.z + f1.z) / 2 };
                let travel = |sign: Fx| {
                    let a = p0 + r0 * (*side * sign);
                    let b = position + r1 * (*side * sign);
                    let d = b - a;
                    (forward.x * d.x + forward.y * d.y + forward.z * d.z) * *mult
                };
                self.current = (self.current.0 + travel(Fx::ONE), self.current.1 + travel(-Fx::ONE));
                self.seen = (position, orientation);
            }
            ScrollKind::Manual { speed } => self.current = (self.current.0 + speed.0, self.current.1 + speed.1),
            ScrollKind::PingPong { values, back } => {
                let run = |v: Fx, (ping, ping_speed, pong, pong_speed): (Fx, Fx, Fx, Fx), back: &mut bool| {
                    let (to, speed) = if *back { (ping, pong_speed) } else { (pong, ping_speed) };
                    let step = speed.abs();
                    if (to - v).abs() <= step {
                        *back = !*back;
                        to
                    } else if to > v {
                        v + step
                    } else {
                        v - step
                    }
                };
                let (mut b0, mut b1) = (back[0], back[1]);
                self.current = (run(self.current.0, values[0], &mut b0), run(self.current.1, values[1], &mut b1));
                *back = [b0, b1];
            }
        }
    }
}

/// Ids of entities with a scroller (ascending), so the tick doesn't visit
/// the whole world.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Scrolled(pub std::collections::BTreeSet<EntityId>);

/// Steps every scroller (after movement).
pub fn tick(world: &mut EntityWorld, scrolled: &mut Scrolled) {
    scrolled.0.retain(|&id| {
        let Some(e) = world.get_mut(id).filter(|e| !e.destroyed) else { return false };
        let (p, q) = (e.position(), e.orientation);
        match e.cold.scroller.as_mut() {
            Some(s) => {
                s.step(p, q);
                true
            }
            None => false,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn treads_follow_forward_travel_and_turns_run_them_apart() {
        let kind = ScrollKind::Thread { side: Fx::ONE, mult: Fx::from_num(0.5) };
        let mut s = Scroller::new(kind, FxVec3::default(), FxQuat::IDENTITY);
        s.step(FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::from_num(2) }, FxQuat::IDENTITY);
        assert_eq!(s.current, (Fx::ONE, Fx::ONE));
        // A turn on the spot: one track forward, the other back.
        let (q, _, _) = FxQuat::from_heading(Fx::from_num(0.2));
        let before = s.current;
        s.step(FxVec3 { x: Fx::ZERO, y: Fx::ZERO, z: Fx::from_num(2) }, q);
        let (d1, d2) = (s.current.0 - before.0, s.current.1 - before.1);
        assert!(d1 * d2 < Fx::ZERO && (d1 + d2).abs() < Fx::from_num(0.001), "{d1} {d2}");
    }
}
