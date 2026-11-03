//! AI threat map (moho's "IMAP"): per-army threat summed over square map
//! cells, queried by AI brains (`GetHighestThreatPosition`,
//! `GetThreatAtPosition`, ...).
//!
//! Cell size follows the map like moho's (FAF's `IMAPConfiguration` mirrors
//! it): 32 world units up to 10 km maps, 64 at 20 km, 128 at 40 km, 256
//! beyond. Each unit carries a threat value per [`ThreatType`], computed once
//! at spawn from its blueprint (`Defense.*ThreatLevel`) and categories. The
//! grid is rebuilt lazily, at most once per tick, the first time a query
//! needs it (units move; queries in the same tick share the snapshot).
//!
//! Each brain sees other armies' threat only as far as it knows their units
//! (`knows` in [`ThreatMap::refresh`]: detected now, or structures it has
//! seen); its own army's threat is exact.
//!
//! Inferred, not verified against the binary: which categories feed each
//! type, that `restriction` doesn't change the intel rule (it's ignored), and
//! assigned threat decays linearly per second.

use crate::engine::sim::entity::{EntityKind, EntityWorld};
use crate::engine::sim::math::{Fx, FxVec3};

/// Moho `BrainThreatType`, in its declared order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum ThreatType {
    Overall,
    OverallNotAssigned,
    StructuresNotMex,
    Structures,
    Naval,
    Air,
    Land,
    Experimental,
    Commander,
    Artillery,
    AntiAir,
    AntiSurface,
    AntiSub,
    Economy,
    Unknown,
}

pub const THREAT_TYPES: usize = 15;

impl ThreatType {
    pub const ALL: [ThreatType; THREAT_TYPES] = [
        ThreatType::Overall,
        ThreatType::OverallNotAssigned,
        ThreatType::StructuresNotMex,
        ThreatType::Structures,
        ThreatType::Naval,
        ThreatType::Air,
        ThreatType::Land,
        ThreatType::Experimental,
        ThreatType::Commander,
        ThreatType::Artillery,
        ThreatType::AntiAir,
        ThreatType::AntiSurface,
        ThreatType::AntiSub,
        ThreatType::Economy,
        ThreatType::Unknown,
    ];

    /// The type a script names, ignoring case: moho's is the reflected enum
    /// `EThreatType` (`THREATTYPE_*`, FA's exe 0xa80e00), read the way gpg
    /// reads any enum, `_stricmp` (gpgcore.dll `REnumType::GetEnumValue`
    /// 0x100153f0). LOUD's bomber AI asks for 'LAND'.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.name().eq_ignore_ascii_case(s))
    }

    pub fn name(self) -> &'static str {
        match self {
            ThreatType::Overall => "Overall",
            ThreatType::OverallNotAssigned => "OverallNotAssigned",
            ThreatType::StructuresNotMex => "StructuresNotMex",
            ThreatType::Structures => "Structures",
            ThreatType::Naval => "Naval",
            ThreatType::Air => "Air",
            ThreatType::Land => "Land",
            ThreatType::Experimental => "Experimental",
            ThreatType::Commander => "Commander",
            ThreatType::Artillery => "Artillery",
            ThreatType::AntiAir => "AntiAir",
            ThreatType::AntiSurface => "AntiSurface",
            ThreatType::AntiSub => "AntiSub",
            ThreatType::Economy => "Economy",
            ThreatType::Unknown => "Unknown",
        }
    }
}

/// A unit's threat per type.
pub type ThreatLevels = [Fx; THREAT_TYPES];

/// What a unit is, for the category-based threat types.
#[derive(Clone, Copy, Debug, Default)]
pub struct ThreatKinds {
    pub structure: bool,
    pub mass_extraction: bool,
    pub naval: bool,
    pub air: bool,
    pub land: bool,
    pub experimental: bool,
    pub commander: bool,
    pub artillery: bool,
}

/// A unit's threat from its blueprint `Defense` levels and categories:
/// the anti-* and economy types take their own level; overall and every
/// category type the unit belongs to take the sum of all levels.
pub fn unit_threat(air: Fx, surface: Fx, sub: Fx, economy: Fx, k: ThreatKinds) -> ThreatLevels {
    let total = air + surface + sub + economy;
    let mut t = [Fx::ZERO; THREAT_TYPES];
    let mut set = |ty: ThreatType, v: Fx| t[ty as usize] = v;
    set(ThreatType::Overall, total);
    set(ThreatType::OverallNotAssigned, total);
    set(ThreatType::AntiAir, air);
    set(ThreatType::AntiSurface, surface);
    set(ThreatType::AntiSub, sub);
    set(ThreatType::Economy, economy);
    let mut member = |ty: ThreatType, yes: bool| {
        if yes {
            t[ty as usize] = total;
        }
    };
    member(ThreatType::Structures, k.structure);
    member(ThreatType::StructuresNotMex, k.structure && !k.mass_extraction);
    member(ThreatType::Naval, k.naval);
    member(ThreatType::Air, k.air);
    member(ThreatType::Land, k.land);
    member(ThreatType::Experimental, k.experimental);
    member(ThreatType::Commander, k.commander);
    member(ThreatType::Artillery, k.artillery);
    t
}

/// Threat a brain placed by script (`AssignThreatAtPosition`).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Assigned {
    cell: u32,
    ty: ThreatType,
    value: Fx,
    /// Lost per second.
    decay: Fx,
}

/// Cell edge (world units) for a map of the given size.
pub fn cell_size(width: u32, height: u32) -> u32 {
    match width.max(height) {
        0..=512 => 32,
        513..=1024 => 64,
        1025..=2048 => 128,
        _ => 256,
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct ThreatMap {
    cell: u32,
    cols: u32,
    rows: u32,
    /// `cells[army][cell]`, army = moho army index (1-based; 0 unused).
    cells: Vec<Vec<ThreatLevels>>,
    /// `known[brain][army][cell]`: the part of `cells[army]` that `brain`
    /// knows about; valid while `known_fresh[brain]`.
    known: Vec<Vec<Vec<ThreatLevels>>>,
    known_fresh: Vec<bool>,
    /// Rebuild before the next query.
    dirty: bool,
    /// Script-assigned threat per brain (army index).
    assigned: Vec<Vec<Assigned>>,
}

impl ThreatMap {
    pub fn new(width: u32, height: u32) -> Self {
        let cell = cell_size(width, height);
        let cols = width.div_ceil(cell).max(1);
        let rows = height.div_ceil(cell).max(1);
        Self { cell, cols, rows, cells: Vec::new(), known: Vec::new(), known_fresh: Vec::new(), dirty: true, assigned: Vec::new() }
    }

    pub fn cell_size(&self) -> u32 {
        self.cell
    }

    /// Marks the snapshot stale (called once per tick).
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn axis(&self, v: Fx, n: u32) -> u32 {
        let c = v.floor().to_num::<i64>() / self.cell as i64;
        c.clamp(0, n as i64 - 1) as u32
    }

    /// (col, row) of world position `p`.
    pub fn cell_of(&self, p: FxVec3) -> (u32, u32) {
        (self.axis(p.x, self.cols), self.axis(p.z, self.rows))
    }

    /// World centre of a cell (y = 0).
    pub fn center(&self, col: u32, row: u32) -> FxVec3 {
        let half = Fx::from_num(self.cell) / 2;
        FxVec3 {
            x: Fx::from_num(col * self.cell) + half,
            y: Fx::ZERO,
            z: Fx::from_num(row * self.cell) + half,
        }
    }

    /// Rebuilds the per-army sums if stale. Units are visited in id order,
    /// so the (exact, fixed-point) sums are the same on every machine. With
    /// `knows = (brain, f)` (intel), also builds what `brain` knows of other
    /// armies (`f(unit)`), once per tick per brain, only for brains that ask.
    pub fn refresh(&mut self, world: &EntityWorld, knows: Option<(u32, &dyn Fn(&crate::engine::sim::entity::Entity) -> bool)>) {
        let n = (self.cols * self.rows) as usize;
        if self.dirty {
            self.dirty = false;
            self.known_fresh.iter_mut().for_each(|f| *f = false);
            for grid in self.cells.iter_mut() {
                grid.iter_mut().for_each(|c| *c = [Fx::ZERO; THREAT_TYPES]);
            }
            for e in world.iter() {
                if e.kind != EntityKind::Unit || e.destroyed {
                    continue;
                }
                let (Some(army), Some(data)) = (e.army, e.unit.as_deref()) else { continue };
                let army = army as usize;
                let (c, r) = self.cell_of(e.position());
                let index = (r * self.cols + c) as usize;
                if self.cells.len() <= army {
                    self.cells.resize_with(army + 1, Vec::new);
                }
                let grid = &mut self.cells[army];
                if grid.len() != n {
                    grid.resize(n, [Fx::ZERO; THREAT_TYPES]);
                }
                for (acc, v) in grid[index].iter_mut().zip(data.threat.iter()) {
                    *acc = acc.saturating_add(*v);
                }
            }
        }
        let Some((brain, knows)) = knows else { return };
        let b = brain as usize;
        if self.known_fresh.len() <= b {
            self.known_fresh.resize(b + 1, false);
            self.known.resize_with(b + 1, Vec::new);
        }
        if self.known_fresh[b] {
            return;
        }
        self.known_fresh[b] = true;
        for grid in self.known[b].iter_mut() {
            grid.iter_mut().for_each(|c| *c = [Fx::ZERO; THREAT_TYPES]);
        }
        for e in world.iter() {
            if e.kind != EntityKind::Unit || e.destroyed {
                continue;
            }
            let (Some(army), Some(data)) = (e.army, e.unit.as_deref()) else { continue };
            if army == brain || !knows(e) {
                continue;
            }
            let army = army as usize;
            let (c, r) = self.cell_of(e.position());
            let index = (r * self.cols + c) as usize;
            let per_brain = &mut self.known[b];
            if per_brain.len() <= army {
                per_brain.resize_with(army + 1, Vec::new);
            }
            let grid = &mut per_brain[army];
            if grid.len() != n {
                grid.resize(n, [Fx::ZERO; THREAT_TYPES]);
            }
            for (acc, v) in grid[index].iter_mut().zip(data.threat.iter()) {
                *acc = acc.saturating_add(*v);
            }
        }
    }

    /// Ages assigned threat by one tick, dropping what has decayed away.
    pub fn decay_assigned(&mut self) {
        let dt = Fx::ONE / Fx::from_num(10);
        for list in self.assigned.iter_mut() {
            for a in list.iter_mut() {
                a.value = (a.value - a.decay * dt).max(Fx::ZERO);
            }
            list.retain(|a| a.value > Fx::ZERO);
        }
    }

    /// `AssignThreatAtPosition`: extra threat `brain` believes is at `p`.
    pub fn assign(&mut self, brain: u32, p: FxVec3, value: Fx, decay: Fx, ty: ThreatType) {
        let (c, r) = self.cell_of(p);
        let brain = brain as usize;
        if self.assigned.len() <= brain {
            self.assigned.resize_with(brain + 1, Vec::new);
        }
        self.assigned[brain].push(Assigned { cell: r * self.cols + c, ty, value, decay });
    }

    /// Threat of type `ty` in cell `index` from armies passing `counts`,
    /// plus what `brain` assigned there (assigned threat feeds its own type
    /// and `Overall`, never `OverallNotAssigned`).
    /// `brain`'s knowledge tables, if built this tick.
    fn known_for(&self, brain: u32) -> Option<&[Vec<ThreatLevels>]> {
        let b = brain as usize;
        (self.known_fresh.get(b).copied() == Some(true)).then(|| self.known[b].as_slice())
    }

    fn cell_threat(&self, index: u32, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> Fx {
        self.cell_threat_with(self.known_for(brain), index, ty, brain, counts)
    }

    fn cell_threat_with(&self, known: Option<&[Vec<ThreatLevels>]>, index: u32, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> Fx {
        let mut sum = Fx::ZERO;
        let (i, t) = (index as usize, ty as usize);
        match known {
            // Other armies through what this brain knows (intel on).
            Some(k) => {
                for (army, grid) in self.cells.iter().enumerate() {
                    if grid.is_empty() || !counts(army as u32) {
                        continue;
                    }
                    let grid = if army as u32 == brain { grid } else { match k.get(army) { Some(g) if !g.is_empty() => g, _ => continue } };
                    sum = sum.saturating_add(grid[i][t]);
                }
            }
            None => {
                for (army, grid) in self.cells.iter().enumerate() {
                    if !grid.is_empty() && counts(army as u32) {
                        sum = sum.saturating_add(grid[i][t]);
                    }
                }
            }
        }
        if let Some(list) = self.assigned.get(brain as usize) {
            for a in list.iter().filter(|a| a.cell == index) {
                if a.ty == ty || (ty == ThreatType::Overall && a.ty != ThreatType::OverallNotAssigned) {
                    sum = sum.saturating_add(a.value);
                }
            }
        }
        sum
    }

    /// Every cell's threat (row-major) for debug views: (cols, rows, cell
    /// size, values).
    pub fn grid_values(&self, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> (u32, u32, u32, Vec<f32>) {
        let values = (0..self.cols * self.rows).map(|i| self.cell_threat(i, ty, brain, counts).to_num::<f32>()).collect();
        (self.cols, self.rows, self.cell, values)
    }

    /// Cells within `rings` of (col, row), row-major.
    fn ring(&self, col: u32, row: u32, rings: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        let (c0, c1) = (col.saturating_sub(rings), (col + rings).min(self.cols - 1));
        let (r0, r1) = (row.saturating_sub(rings), (row + rings).min(self.rows - 1));
        (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| (c, r)))
    }

    fn ring_threat(&self, col: u32, row: u32, rings: u32, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> Fx {
        let known = self.known_for(brain);
        self.ring(col, row, rings)
            .map(|(c, r)| self.cell_threat_with(known, r * self.cols + c, ty, brain, counts))
            .fold(Fx::ZERO, |a, b| a.saturating_add(b))
    }

    /// Threat around `p`: the cell plus `rings` cells around it.
    pub fn threat_at(&self, p: FxVec3, rings: u32, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> Fx {
        let (c, r) = self.cell_of(p);
        self.ring_threat(c, r, rings, ty, brain, counts)
    }

    /// The cell whose ring holds the most threat: its centre and the value.
    /// Ties go to the first cell in row-major order.
    pub fn highest(&self, rings: u32, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> (FxVec3, Fx) {
        let mut best = (0, 0, Fx::ZERO);
        for r in 0..self.rows {
            for c in 0..self.cols {
                let t = self.ring_threat(c, r, rings, ty, brain, counts);
                if t > best.2 {
                    best = (c, r, t);
                }
            }
        }
        (self.center(best.0, best.1), best.2)
    }

    /// Cells around `p` (within `rings`) holding threat: (centre x, z, value),
    /// row-major.
    pub fn around(&self, p: FxVec3, rings: u32, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> Vec<(Fx, Fx, Fx)> {
        let (c, r) = self.cell_of(p);
        self.ring(c, r, rings)
            .filter_map(|(c, r)| {
                let t = self.cell_threat(r * self.cols + c, ty, brain, counts);
                (t > Fx::ZERO).then(|| {
                    let m = self.center(c, r);
                    (m.x, m.z, t)
                })
            })
            .collect()
    }

    /// Threat summed over the distinct cells the segment `a -> b` crosses
    /// (sampled every half cell, so no cell is skipped).
    pub fn between(&self, a: FxVec3, b: FxVec3, ty: ThreatType, brain: u32, counts: &dyn Fn(u32) -> bool) -> Fx {
        let d = b - a;
        let len = d.length();
        let step = Fx::from_num(self.cell) / 2;
        let samples = (len / step).ceil().to_num::<u32>().max(1);
        let mut seen: Vec<u32> = Vec::new();
        let mut sum = Fx::ZERO;
        for i in 0..=samples {
            let p = a + d * (Fx::from_num(i) / Fx::from_num(samples));
            let (c, r) = self.cell_of(p);
            let index = r * self.cols + c;
            if !seen.contains(&index) {
                seen.push(index);
                sum = sum.saturating_add(self.cell_threat(index, ty, brain, counts));
            }
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::api::blueprints::BlueprintKind;
    use crate::engine::sim::math::FxQuat;

    fn unit(w: &mut EntityWorld, army: u32, x: f64, z: f64, surface: i32, kinds: ThreatKinds) {
        let id = w.spawn(EntityKind::Unit, BlueprintKind::Unit, String::new(), Some(army), FxVec3::from_f64(x, 0.0, z), FxQuat::IDENTITY);
        let t = unit_threat(Fx::ZERO, Fx::from_num(surface), Fx::ZERO, Fx::ZERO, kinds);
        w.get_mut(id).unwrap().unit.as_mut().unwrap().threat = t;
    }

    #[test]
    fn sizes_follow_the_map() {
        assert_eq!(cell_size(256, 256), 32);
        assert_eq!(cell_size(1024, 512), 64);
        assert_eq!(cell_size(4096, 4096), 256);
    }

    #[test]
    fn highest_and_rings_and_army_filter() {
        let mut w = EntityWorld::default();
        let land = ThreatKinds { land: true, ..Default::default() };
        let structure = ThreatKinds { structure: true, ..Default::default() };
        unit(&mut w, 2, 40.0, 40.0, 10, land); // cell (1,1)
        unit(&mut w, 2, 50.0, 45.0, 5, structure); // cell (1,1)
        unit(&mut w, 3, 200.0, 200.0, 12, land); // cell (6,6)
        unit(&mut w, 1, 210.0, 210.0, 99, land); // own army: never counted
        let mut map = ThreatMap::new(256, 256);
        map.refresh(&w, None);
        let enemies = |a: u32| a != 1;
        let (pos, t) = map.highest(0, ThreatType::Overall, 1, &enemies);
        assert_eq!((pos, t), (FxVec3::from_f64(48.0, 0.0, 48.0), Fx::from_num(15)));
        let (pos, t) = map.highest(0, ThreatType::Land, 1, &enemies);
        assert_eq!((pos, t), (FxVec3::from_f64(208.0, 0.0, 208.0), Fx::from_num(12)));
        assert_eq!(map.threat_at(FxVec3::from_f64(70.0, 0.0, 70.0), 0, ThreatType::Overall, 1, &enemies), Fx::ZERO);
        assert_eq!(map.threat_at(FxVec3::from_f64(70.0, 0.0, 70.0), 1, ThreatType::Overall, 1, &enemies), Fx::from_num(15));
        let only3 = |a: u32| a == 3;
        assert_eq!(map.highest(0, ThreatType::Overall, 1, &only3).1, Fx::from_num(12));
        assert_eq!(map.around(FxVec3::from_f64(40.0, 0.0, 40.0), 1, ThreatType::Structures, 1, &enemies), vec![(Fx::from_num(48), Fx::from_num(48), Fx::from_num(5))]);
        let line = map.between(FxVec3::from_f64(10.0, 0.0, 10.0), FxVec3::from_f64(250.0, 0.0, 250.0), ThreatType::Overall, 1, &enemies);
        assert_eq!(line, Fx::from_num(27));
    }

    #[test]
    fn brains_only_count_enemies_they_know() {
        let mut w = EntityWorld::default();
        let land = ThreatKinds { land: true, ..Default::default() };
        unit(&mut w, 2, 40.0, 40.0, 10, land);
        unit(&mut w, 2, 200.0, 200.0, 12, land);
        let mut map = ThreatMap::new(256, 256);
        // Brain 1 knows only units west of x = 100; brain 2 owns them all.
        let knows = |e: &crate::engine::sim::entity::Entity| e.position().x < Fx::from_num(100);
        map.refresh(&w, Some((1, &knows)));
        let enemies = |a: u32| a == 2;
        assert_eq!(map.highest(0, ThreatType::Overall, 1, &enemies).1, Fx::from_num(10), "the unknown 12 doesn't count");
        assert_eq!(map.threat_at(FxVec3::from_f64(200.0, 0.0, 200.0), 0, ThreatType::Overall, 1, &enemies), Fx::ZERO);
        let own = |a: u32| a == 2;
        assert_eq!(map.highest(0, ThreatType::Overall, 2, &own).1, Fx::from_num(12), "own threat is exact");
    }

    #[test]
    fn assigned_threat_decays() {
        let mut map = ThreatMap::new(256, 256);
        map.refresh(&EntityWorld::default(), None);
        map.assign(1, FxVec3::from_f64(100.0, 0.0, 100.0), Fx::from_num(10), Fx::from_num(5), ThreatType::Land);
        let none = |_: u32| false;
        let p = FxVec3::from_f64(100.0, 0.0, 100.0);
        assert_eq!(map.threat_at(p, 0, ThreatType::Overall, 1, &none), Fx::from_num(10));
        assert_eq!(map.threat_at(p, 0, ThreatType::OverallNotAssigned, 1, &none), Fx::ZERO);
        assert_eq!(map.threat_at(p, 0, ThreatType::Land, 2, &none), Fx::ZERO, "only the assigning brain sees it");
        for _ in 0..10 {
            map.decay_assigned();
        }
        let left = map.threat_at(p, 0, ThreatType::Land, 1, &none).to_num::<f64>();
        assert!((left - 5.0).abs() < 1e-6, "{left}");
        for _ in 0..11 {
            map.decay_assigned();
        }
        assert_eq!(map.threat_at(p, 0, ThreatType::Land, 1, &none), Fx::ZERO);
    }
}
