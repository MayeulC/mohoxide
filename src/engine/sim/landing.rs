//! Idle aircraft landing (`AutoLandTime`): where an idle hovering aircraft
//! sets down, and the ground spots landing and landed aircraft hold.
//!
//! MohoEngine.dll `CUnitMotion::CalcMoveAir` (0x1029dcc7..0x1029de69): an
//! aircraft whose command queue has been empty (`MotionTick` 0x102987ee keeps
//! the tick it emptied, motion +0xac; `SetTarget` restarts it) for
//! floor(`AutoLandTime` x 10) ticks, and is within `StartTurnDistance` of its
//! motion target, looks for a spot near that target (0x102154c0, below). On
//! success it reserves the spot's grid rect (`Unit::ReserveOgridRect`) and
//! heads there to land; on failure it sets `CannotFindPlaceToLand` and the
//! wait starts over.

use crate::engine::sim::entity::{EntityId, EntityWorld};
use crate::engine::sim::math::{Fx, FxVec3};
use std::collections::{BTreeMap, HashMap};

/// Grid rect in cells: x0, z0 inclusive, x1, z1 exclusive.
pub type Rect = (i32, i32, i32, i32);

/// The spots landing and landed aircraft hold, by unit (sim state).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct LandingSpots(pub BTreeMap<EntityId, Rect>);

/// The cells the spots cover, so a search checks each candidate with a
/// lookup per cell rather than a scan of every spot (20k idle aircraft
/// each searching rings of up to 900 candidates, over 20k spots, took
/// seconds a tick). Only looked up, never iterated: the hash order can't
/// reach the sim.
#[allow(clippy::disallowed_types)]
#[derive(Default)]
pub struct ReservedCells(HashMap<(i32, i32), EntityId>);

/// A cell more than one spot covers.
const SHARED: EntityId = EntityId::MAX;

impl ReservedCells {
    /// The cells of `spots`, every one of them held (`land` drops those of
    /// aircraft no longer landing or landed first).
    pub fn new(spots: &LandingSpots) -> Self {
        #[allow(clippy::disallowed_types)]
        let mut cells = Self(HashMap::with_capacity(spots.0.len()));
        for (&unit, &r) in &spots.0 {
            cells.insert(unit, r);
        }
        cells
    }

    pub fn insert(&mut self, unit: EntityId, r: Rect) {
        for z in r.1..r.3 {
            for x in r.0..r.2 {
                self.0.entry((x, z)).and_modify(|owner| if *owner != unit { *owner = SHARED }).or_insert(unit);
            }
        }
    }

    pub fn remove(&mut self, unit: EntityId, r: Rect) {
        for z in r.1..r.3 {
            for x in r.0..r.2 {
                if self.0.get(&(x, z)) == Some(&unit) {
                    self.0.remove(&(x, z));
                }
            }
        }
    }

    /// Any of `r` covered by another unit's spot.
    fn held_by_other(&self, unit: EntityId, r: Rect) -> bool {
        (r.1..r.3).any(|z| (r.0..r.2).any(|x| self.0.get(&(x, z)).is_some_and(|&owner| owner != unit)))
    }
}

/// Cells moho's search looks at before giving up (0x10215a83).
const MAX_CELLS: u32 = 900;

/// What the search needs to know about the ground.
pub struct Ground<'a> {
    pub world: &'a EntityWorld,
    pub nav: Option<&'a crate::engine::sim::nav::NavGrid>,
    pub terrain: Option<&'a crate::engine::sim::terrain::Terrain>,
    /// The playable area (x0, z0, x1, z1), world units.
    pub bounds: (Fx, Fx, Fx, Fx),
    /// The held spots' cells.
    pub reserved: &'a ReservedCells,
}

/// The landing spot for `unit` near `goal` and the rect it holds (moho
/// 0x102154c0). The footprint's rect (`footprint` cells, unrotated) is
/// placed with its corner at `goal` less half its size, rounded; that cell
/// first, then square rings round it, `2 x max(SizeX, SizeZ)` cells apart,
/// each ring's cells by x then z; from the first ring with any free, the
/// one nearest the unit. Free: inside the playable area, ground the
/// footprint can stand on (water too for `on_water`: moho adds the Water
/// cap for `TRANSPORTATION` units over water), and no other unit or
/// reserved spot on it. The spot is the rect's centre.
pub fn find_spot(g: &Ground, unit: EntityId, at: FxVec3, goal: FxVec3, footprint: (u8, u8), on_water: bool) -> Option<(FxVec3, Rect)> {
    let (sx, sz) = (i32::from(footprint.0.max(1)), i32::from(footprint.1.max(1)));
    let step = 2 * sx.max(sz);
    let half = |s: i32| Fx::from_num(s) / 2;
    let x0 = (goal.x - half(sx)).round().to_num::<i32>();
    let z0 = (goal.z - half(sz)).round().to_num::<i32>();
    let centre = |(x, z, ..): Rect| FxVec3 { x: Fx::from_num(x) + half(sx), y: Fx::ZERO, z: Fx::from_num(z) + half(sz) };
    let rect = |dx: i32, dz: i32| -> Rect {
        let (x, z) = (x0 + dx * step, z0 + dz * step);
        (x, z, x + sx, z + sz)
    };
    let first = rect(0, 0);
    if free(g, unit, first, on_water) {
        return Some((centre(first), first));
    }
    let mut checked = 0;
    let mut r = 1;
    while checked < MAX_CELLS {
        let mut best: Option<(Fx, Rect)> = None;
        for dx in -r..=r {
            let edge = dx == -r || dx == r;
            let mut dz = -r;
            while dz <= r {
                checked += 1;
                let cand = rect(dx, dz);
                if free(g, unit, cand, on_water) {
                    let c = centre(cand);
                    let (ex, ez) = (c.x - at.x, c.z - at.z);
                    let d = ex.saturating_mul(ex).saturating_add(ez.saturating_mul(ez));
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, cand));
                    }
                }
                dz += if edge { 1 } else { 2 * r };
            }
        }
        if let Some((_, cand)) = best {
            return Some((centre(cand), cand));
        }
        r += 1;
    }
    None
}

fn free(g: &Ground, unit: EntityId, r: Rect, on_water: bool) -> bool {
    let (x0, z0, x1, z1) = (Fx::from_num(r.0), Fx::from_num(r.1), Fx::from_num(r.2), Fx::from_num(r.3));
    if x0 < g.bounds.0 || z0 < g.bounds.1 || x1 > g.bounds.2 || z1 > g.bounds.3 {
        return false;
    }
    // A reservation counts while its aircraft is landing or landed.
    if g.reserved.held_by_other(unit, r) {
        return false;
    }
    let land = g.nav.and_then(|n| n.class_for(crate::engine::sim::nav::caps::LAND, 1, 1).map(|c| (n, c)));
    for z in r.1..r.3 {
        for x in r.0..r.2 {
            let (cx, cz) = (x as u32, z as u32);
            let wet = on_water && g.terrain.is_some_and(|t| {
                let (px, pz) = (Fx::from_num(x) + Fx::from_num(0.5), Fx::from_num(z) + Fx::from_num(0.5));
                t.surface_at(px, pz) > t.height_at(px, pz)
            });
            let ok = match land {
                Some((n, c)) => n.passable(c, cx, cz) || (wet && !n.blocked_cell(cx, cz)),
                None => true,
            };
            if !ok {
                return false;
            }
        }
    }
    // Nothing on the ground there (units on it, landed aircraft).
    let others = g.world.units_in_rect(x0, z0, x1, z1);
    !others.iter().any(|&id| {
        id != unit
            && g.world.get(id).is_some_and(|e| {
                !e.destroyed && e.attachment.is_none() && e.unit.as_deref().is_some_and(|u| u.layer != crate::engine::sim::layer::Layer::Air)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::api::blueprints::BlueprintKind;
    use crate::engine::sim::entity::EntityKind;
    use crate::engine::sim::math::FxQuat;

    fn ground<'a>(w: &'a EntityWorld, reserved: &'a ReservedCells) -> Ground<'a> {
        Ground { world: w, nav: None, terrain: None, bounds: (Fx::ZERO, Fx::ZERO, Fx::from_num(256), Fx::from_num(256)), reserved }
    }

    #[test]
    fn lands_on_its_goal_when_free() {
        let w = EntityWorld::default();
        let spots = ReservedCells::default();
        let (p, r) = find_spot(&ground(&w, &spots), 1, FxVec3::from_f64(100.0, 20.0, 100.0), FxVec3::from_f64(100.0, 0.0, 100.0), (1, 4), false).unwrap();
        assert_eq!(r, (100, 98, 101, 102));
        assert_eq!(p, FxVec3::from_f64(100.5, 0.0, 100.0));
    }

    #[test]
    fn a_held_spot_sends_the_next_one_to_the_nearest_ring_cell() {
        let mut w = EntityWorld::default();
        let other = w.spawn(EntityKind::Unit, BlueprintKind::Unit, String::new(), Some(1), FxVec3::from_f64(100.5, 0.0, 100.5), FxQuat::IDENTITY);
        w.get_mut(other).unwrap().unit.as_mut().unwrap().layer = crate::engine::sim::layer::Layer::Land;
        let spots = ReservedCells::default();
        // Coming from +x: the ring cell on that side (step 2).
        let (p, _) = find_spot(&ground(&w, &spots), 99, FxVec3::from_f64(110.0, 20.0, 100.0), FxVec3::from_f64(100.5, 0.0, 100.5), (1, 1), false).unwrap();
        assert_eq!(p, FxVec3::from_f64(102.5, 0.0, 100.5));
        // A spot reserved by an aircraft landing there counts too.
        let mut held = LandingSpots::default();
        held.0.insert(other, (102, 100, 103, 101));
        let spots = ReservedCells::new(&held);
        let (p, _) = find_spot(&ground(&w, &spots), 99, FxVec3::from_f64(110.0, 20.0, 100.0), FxVec3::from_f64(100.5, 0.0, 100.5), (1, 1), false).unwrap();
        assert_ne!(p, FxVec3::from_f64(102.5, 0.0, 100.5));
    }
}
