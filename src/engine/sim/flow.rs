//! Flow fields: one shared route to a point for every unit heading there.
//!
//! Many units given one plain order (a big `IssueMove` / aggressive move
//! without a formation, a rally point) all want the same thing: the way to
//! one cell from wherever they stand. Instead of a search each, a field is
//! built once per (footprint class, goal cell), shared by every order to
//! that cell, on top of the class's sector graph (`hpa`):
//! - a Dijkstra over the graph from the goal gives every portal node its
//!   cost to the goal (whole map, a few ms on a 4096 map);
//! - a sector's per-cell costs are filled in only when a unit stands in it,
//!   by an in-sector Dijkstra seeded with its nodes' costs (and the goal).
//! A unit walks downhill from its cell a few steps and steers for the
//! farthest of them it can see, so it needs no path of its own.
//!
//! Fields live while some unit uses them and are rebuilt when the grid
//! changes. Everything is integer and built in key order (fields and
//! sectors in parallel, merged by key), so steering is the same on every
//! machine.

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::sim::hpa;
use crate::engine::sim::math::FxVec3;
use crate::engine::sim::nav::{NavClass, NavGrid};

/// Cells walked downhill before steering for the farthest one in sight.
const LOOKAHEAD: usize = 8;

/// One goal's field for one class.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Field {
    goal: (u32, u32),
    /// Nav grid version it was built on.
    version: u32,
    /// Cost to the goal of each graph node (by `Hier::node_index`;
    /// `u32::MAX`: it can't reach the goal).
    nodes: Vec<u32>,
    /// Per sector (index) built so far: costs row-major in its rect.
    sectors: BTreeMap<u32, (hpa::Rect, Vec<u32>)>,
}

impl Field {
    fn cost(&self, h: &hpa::Hier, w: u32, x: u32, z: u32) -> Option<u32> {
        let s = h.sector_index(x, z);
        let c = match self.sectors.get(&s) {
            Some((r, costs)) => costs[((z - r.z0) * r.width() + x - r.x0) as usize],
            None => *self.nodes.get(h.node_index(z * w + x)?)?,
        };
        (c != u32::MAX).then_some(c)
    }

    /// Where a unit of `class` at `from` should steer: the farthest of the
    /// next few cells downhill that it can see. `None` if neither its cell
    /// nor a neighbour is in the field (it can't reach the goal, or the
    /// sector isn't built): it then searches as usual.
    pub fn next_point(&self, nav: &NavGrid, class: NavClass, from: FxVec3) -> Option<FxVec3> {
        let h = nav.hier_of(class)?;
        let w = nav.width;
        let (mut x, mut z) = nav.cell_for(class, from);
        if (x, z) == self.goal {
            return None;
        }
        // A unit on a cell outside the field (one its class can't stand on)
        // steps to any neighbour in it.
        let mut here = self.cost(h, w, x, z).unwrap_or(u32::MAX);
        let mut steps: [(u32, u32); LOOKAHEAD] = [(0, 0); LOOKAHEAD];
        let mut n = 0;
        while n < LOOKAHEAD && here > 0 {
            let mut best: Option<(u32, u32, u32)> = None;
            for (dx, dz) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, nz) = (x.wrapping_add_signed(dx), z.wrapping_add_signed(dz));
                if !nav.passable(class, nx, nz) || (dx != 0 && dz != 0 && (!nav.passable(class, nx, z) || !nav.passable(class, x, nz))) {
                    continue;
                }
                let Some(c) = self.cost(h, w, nx, nz) else { continue };
                // Lowest cost, then lowest cell index: the same on every machine.
                if c < here && best.is_none_or(|b| (c, nz * w + nx) < (b.0, b.2 * w + b.1)) {
                    best = Some((c, nx, nz));
                }
            }
            let Some((c, nx, nz)) = best else { break };
            (x, z, here) = (nx, nz, c);
            steps[n] = (x, z);
            n += 1;
        }
        let centre = |c: (u32, u32)| {
            let (x, z) = nav.point_of(class, c);
            FxVec3 { x, y: from.y, z }
        };
        let chosen = (1..n).rev().map(|k| centre(steps[k])).find(|&p| nav.path_clear(class, from, &[p]));
        chosen.or_else(|| (n > 0).then(|| centre(steps[0])))
            // Nowhere downhill: not in the field (can't reach the goal).
            .filter(|_| n > 0)
    }
}

/// The fields in use, by (class, goal cell index).
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Flows {
    fields: BTreeMap<(u8, u32), Field>,
}

impl Flows {
    pub fn get(&self, class: NavClass, goal: (u32, u32), w: u32) -> Option<&Field> {
        self.fields.get(&(class.0, goal.1 * w + goal.0))
    }

    /// Readies the fields for this tick's users: `wants` holds (class, goal
    /// cell, the cell the unit stands in) per unit following a field. Builds
    /// missing or stale fields, then the sectors those units stand in, and
    /// drops fields nobody wants. Call after `NavGrid::refresh`. Returns the
    /// fields built and the work (`nav::Search::work` units), for profiling.
    pub fn prepare(&mut self, nav: &NavGrid, wants: &[(NavClass, (u32, u32), (u32, u32))]) -> (u64, u64) {
        use rayon::prelude::*;
        let w = nav.width;
        let keys: BTreeSet<(u8, u32)> = wants.iter().map(|(c, g, _)| (c.0, g.1 * w + g.0)).collect();
        self.fields.retain(|k, f| keys.contains(k) && f.version == nav.version);
        let missing: Vec<(u8, u32)> = keys.iter().copied().filter(|k| !self.fields.contains_key(k)).collect();
        let built: Vec<((u8, u32), Field, u64)> = missing
            .par_iter()
            .filter_map(|&(c, g)| {
                let _s = crate::prof_span!("flow field (worker)");
                let class = NavClass(c);
                let h = nav.hier_of(class)?;
                let goal = (g % w, g / w);
                let mut work = 0;
                let nodes = h.costs_to(&nav.open_fn(class), &mut hpa::Scratch::default(), goal, &mut work);
                Some(((c, g), Field { goal, version: nav.version, nodes, sectors: BTreeMap::new() }, work))
            })
            .collect();
        let mut stats = (0u64, 0u64);
        for (k, f, work) in built {
            stats = (stats.0 + 1, stats.1 + work);
            self.fields.insert(k, f);
        }
        // Sectors units stand in that their field hasn't filled in yet.
        let mut need: BTreeSet<((u8, u32), u32)> = BTreeSet::new();
        for &(class, g, (x, z)) in wants {
            let key = (class.0, g.1 * w + g.0);
            let (Some(f), Some(h)) = (self.fields.get(&key), nav.hier_of(class)) else { continue };
            let s = h.sector_index(x, z);
            if !f.sectors.contains_key(&s) {
                need.insert((key, s));
            }
        }
        let need: Vec<((u8, u32), u32)> = need.into_iter().collect();
        let this = &*self;
        let sectors: Vec<(((u8, u32), u32), hpa::Rect, Vec<u32>, u64)> = need
            .par_iter()
            .filter_map(|&(key, s)| {
                let _s = crate::prof_span!("flow sector (worker)");
                let class = NavClass(key.0);
                let h = nav.hier_of(class)?;
                let f = this.fields.get(&key)?;
                // Any cell of the sector names it.
                let (sw, size) = (w.div_ceil(hpa::SECTOR), hpa::SECTOR);
                let (x, z) = ((s % sw) * size, (s / sw) * size);
                let (rect, nodes) = h.sector_nodes(x, z);
                let mut seeds: Vec<((u32, u32), u32)> =
                    nodes.filter_map(|cell| Some(((cell % w, cell / w), *f.nodes.get(h.node_index(cell)?)?)).filter(|&(_, c)| c != u32::MAX)).collect();
                if rect.contains(f.goal.0, f.goal.1) {
                    seeds.push((f.goal, 0));
                }
                let mut costs = Vec::new();
                let settled = h.sector_costs(&nav.open_fn(class), x, z, &seeds, &mut hpa::Scratch::default(), &mut costs);
                Some(((key, s), rect, costs, settled as u64 * 8))
            })
            .collect();
        for ((key, s), rect, costs, work) in sectors {
            stats.1 += work;
            if let Some(f) = self.fields.get_mut(&key) {
                f.sectors.insert(s, (rect, costs));
            }
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::assets::scmap::Heightmap;
    use crate::engine::sim::terrain::Terrain;

    fn grid(n: u32, wall: bool) -> NavGrid {
        let mut samples = vec![0u16; ((n + 1) * (n + 1)) as usize];
        if wall {
            // A ridge at x = 50 from z = 0 to 100, open above.
            for z in 0..=100 {
                samples[(z * (n + 1) + 50) as usize] = 60 * 128;
            }
        }
        let t = Terrain::from_heightmap(&Heightmap { width: n, height: n, scale: 1.0 / 128.0, samples });
        let mut g = NavGrid::build(&t, &[false; 256]);
        g.refresh();
        g
    }

    /// Follows the field from `from` until it stops; returns the end and the steps.
    fn follow(g: &NavGrid, f: &mut Flows, class: NavClass, goal: (u32, u32), from: FxVec3) -> (FxVec3, usize) {
        let mut p = from;
        for step in 0..400 {
            f.prepare(g, &[(class, goal, g.cell_of(p))]);
            match f.get(class, goal, g.width).unwrap().next_point(g, class, p) {
                Some(next) => {
                    assert!(g.path_clear(class, p, &[next]), "steers only where it can go: {p:?} -> {next:?}");
                    p = next;
                }
                None => return (p, step),
            }
        }
        (p, 400)
    }

    #[test]
    fn units_flow_downhill_to_the_goal_round_walls() {
        let g = grid(128, true);
        let land = g.class_named("Vehicle1x1").unwrap();
        let goal = (100, 20);
        let mut f = Flows::default();
        let (end, steps) = follow(&g, &mut f, land, goal, FxVec3::from_f64(20.5, 0.0, 20.5));
        assert_eq!(g.cell_of(end), goal, "reached the goal ({steps} steps)");
        assert!(steps > 10, "went round the ridge, not through it");
    }

    #[test]
    fn a_field_is_shared_and_dropped_when_unused() {
        let g = grid(64, false);
        let land = g.class_named("Vehicle1x1").unwrap();
        let mut f = Flows::default();
        let (built, _) = f.prepare(&g, &[(land, (60, 60), (2, 2)), (land, (60, 60), (40, 2))]);
        assert_eq!(built, 1, "one field for both");
        assert_eq!(f.fields.values().next().unwrap().sectors.len(), 2, "the two sectors they stand in");
        f.prepare(&g, &[]);
        assert!(f.fields.is_empty());
    }
}
