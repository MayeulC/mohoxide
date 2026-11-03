//! The unit stats moho keeps for each army: units built, lost and
//! killed, with their mass and energy value, read by FAF's score thread
//! (`/lua/sim/score.lua`) for the end-of-game screen and by `GetArmyStat` /
//! `GetBlueprintStat`.
//!
//! From MohoEngine.dll (retail):
//! - A unit finished (`Unit::Materialize` 0x102896fe, or made finished by the
//!   `Unit` constructor 0x10285e25) with `General.CapCost` > 0: `Units_Active`
//!   and `Units_History` + 1 (army and blueprint), `Units_MassValue_Built` /
//!   `Units_EnergyValue_Built` + its `BuildCostMass` / `BuildCostEnergy`.
//!   One made unfinished: blueprint `Units_BeingBuilt` + 1 (0x10286023), back
//!   down as it finishes.
//! - `Unit::Kill` (0x10287bae): the owner's `Units_Killed` + 1 (army and
//!   blueprint), `Units_MassValue_Lost` / `Units_EnergyValue_Lost` + its
//!   build costs (times its fraction complete while being built). An
//!   instigator whose army is an enemy of the owner's: `Enemies_Killed` + 1
//!   (army, and blueprint by the victim's), `Enemies_MassValue_Destroyed` /
//!   `Enemies_EnergyValue_Destroyed` + the same values, and
//!   `Enemies_Commanders_Destroyed` + 1 for a `COMMAND` unit.
//! - The `Unit` destructor (0x10286608): `Units_Active` - 1 (with CapCost >
//!   0), or blueprint `Units_BeingBuilt` - 1 while being built.
//!
//! - Damage (0x10307659, as moho applies it, armour included): with an
//!   instigator that has an army, its `DamageStats_TotalDamageDealt` + the
//!   damage, and its blueprint's `Units_TotalDamageDealt` if it's a unit;
//!   with an instigator, the target's army `DamageStats_TotalDamageReceived`
//!   + the damage, and its blueprint's `Units_TotalDamageReceive` if it's a
//!   unit. Nothing for damage 0.

use mlua::prelude::*;

use crate::engine::api::entity::{blueprint_table, world};
use crate::engine::sim::army::{Alliance, Armies};
use crate::engine::sim::entity::EntityId;
use crate::engine::sim::math::Fx;
use crate::engine::sim::unit_state::UnitStates;

/// What the stats need of a unit.
struct Counted {
    army: u32,
    blueprint: Option<u32>,
    counts: bool,
    being_built: bool,
    fraction_complete: Fx,
    blueprint_id: String,
}

fn counted(lua: &Lua, id: EntityId) -> Option<Counted> {
    let w = world(lua);
    let e = w.get(id)?;
    let u = e.unit.as_deref()?;
    let being_built = UnitStates::index("BeingBuilt").is_some_and(|i| u.states.contains(i));
    Some(Counted {
        army: e.army?,
        blueprint: e.category_index,
        counts: u.cap_cost > Fx::ZERO,
        being_built,
        fraction_complete: e.fraction_complete,
        blueprint_id: e.cold.blueprint_id.clone(),
    })
}

/// `Economy.BuildCostMass`, `BuildCostEnergy` of a unit blueprint.
fn build_costs(lua: &Lua, blueprint_id: &str) -> LuaResult<(f64, f64)> {
    let bp = blueprint_table(lua, crate::engine::api::blueprints::BlueprintKind::Unit, blueprint_id)?;
    Ok(match bp.get::<Option<LuaTable>>("Economy")? {
        Some(e) => (e.get::<Option<f64>>("BuildCostMass")?.unwrap_or(0.0), e.get::<Option<f64>>("BuildCostEnergy")?.unwrap_or(0.0)),
        None => (0.0, 0.0),
    })
}

/// A unit was made: finished (spawned complete) or as a fresh construction.
pub fn unit_created(lua: &Lua, id: EntityId) -> LuaResult<()> {
    let Some(c) = counted(lua, id) else { return Ok(()) };
    if c.being_built {
        if let (Some(bp), Some(a)) = (c.blueprint, lua.app_data_mut::<Armies>().expect("Armies").get_mut(c.army)) {
            a.add_blueprint_stat("Units_BeingBuilt", bp, 1.0);
        }
        return Ok(());
    }
    finished(lua, &c, false)
}

/// A construction was finished (while still in the `BeingBuilt` state,
/// before its `OnStopBeingBuilt`).
pub fn unit_finished(lua: &Lua, id: EntityId) -> LuaResult<()> {
    let Some(c) = counted(lua, id).filter(|c| c.being_built) else { return Ok(()) };
    finished(lua, &c, true)
}

fn finished(lua: &Lua, c: &Counted, was_being_built: bool) -> LuaResult<()> {
    if !c.counts {
        return Ok(());
    }
    let (mass, energy) = build_costs(lua, &c.blueprint_id)?;
    let mut armies = lua.app_data_mut::<Armies>().expect("Armies");
    let Some(a) = armies.get_mut(c.army) else { return Ok(()) };
    a.add_stat("Units_Active", 1.0);
    a.add_stat("Units_History", 1.0);
    a.add_stat("Units_MassValue_Built", mass);
    a.add_stat("Units_EnergyValue_Built", energy);
    if let Some(bp) = c.blueprint {
        a.add_blueprint_stat("Units_Active", bp, 1.0);
        a.add_blueprint_stat("Units_History", bp, 1.0);
        if was_being_built {
            a.add_blueprint_stat("Units_BeingBuilt", bp, -1.0);
        }
    }
    Ok(())
}

/// A unit was killed (`Kill`, the first time), by `instigator`'s army if
/// any.
pub fn unit_killed(lua: &Lua, id: EntityId, instigator: Option<EntityId>) -> LuaResult<()> {
    let Some(c) = counted(lua, id) else { return Ok(()) };
    let (mut mass, mut energy) = build_costs(lua, &c.blueprint_id)?;
    if c.fraction_complete < Fx::ONE {
        let fraction = c.fraction_complete.to_num::<f64>();
        mass *= fraction;
        energy *= fraction;
    }
    let killer = instigator.and_then(|i| world(lua).get(i).and_then(|e| e.army));
    let commander = match (crate::engine::api::categories::domain(lua)?.named.get("COMMAND"), c.blueprint) {
        (Some(set), Some(bp)) => set.contains(bp),
        _ => false,
    };
    let mut armies = lua.app_data_mut::<Armies>().expect("Armies");
    if let Some(a) = armies.get_mut(c.army) {
        a.add_stat("Units_Killed", 1.0);
        a.add_stat("Units_MassValue_Lost", mass);
        a.add_stat("Units_EnergyValue_Lost", energy);
        if let Some(bp) = c.blueprint {
            a.add_blueprint_stat("Units_Killed", bp, 1.0);
        }
    }
    let Some(killer) = killer.filter(|&k| armies.alliance(k, c.army) == Alliance::Enemy) else { return Ok(()) };
    if let Some(a) = armies.get_mut(killer) {
        a.add_stat("Enemies_Killed", 1.0);
        a.add_stat("Enemies_MassValue_Destroyed", mass);
        a.add_stat("Enemies_EnergyValue_Destroyed", energy);
        if let Some(bp) = c.blueprint {
            a.add_blueprint_stat("Enemies_Killed", bp, 1.0);
        }
        if commander {
            a.add_stat("Enemies_Commanders_Destroyed", 1.0);
        }
    }
    Ok(())
}

/// A unit is being destroyed (before `World::destroy`).
pub fn unit_destroyed(lua: &Lua, id: EntityId) {
    let Some(c) = counted(lua, id) else { return };
    let mut armies = lua.app_data_mut::<Armies>().expect("Armies");
    let Some(a) = armies.get_mut(c.army) else { return };
    if c.being_built {
        if let Some(bp) = c.blueprint {
            a.add_blueprint_stat("Units_BeingBuilt", bp, -1.0);
        }
    } else if c.counts {
        a.add_stat("Units_Active", -1.0);
        if let Some(bp) = c.blueprint {
            a.add_blueprint_stat("Units_Active", bp, -1.0);
        }
    }
}

/// `amount` damage (after armour) dealt to `target` by `instigator`.
/// Called per damage event: no allocation, no Lua.
pub fn damage(lua: &Lua, instigator: Option<EntityId>, target: EntityId, amount: f64) {
    let Some(instigator) = instigator else { return };
    // (army, blueprint if a unit) of each side.
    let side = |id: EntityId| {
        let w = world(lua);
        w.get(id).and_then(|e| Some((e.army?, e.unit.is_some().then_some(e.category_index).flatten())))
    };
    let (from, to) = (side(instigator), side(target));
    let mut armies = lua.app_data_mut::<Armies>().expect("Armies");
    if let Some((army, blueprint)) = from
        && let Some(a) = armies.get_mut(army) {
        a.add_stat("DamageStats_TotalDamageDealt", amount);
        if let Some(bp) = blueprint {
            a.add_blueprint_stat("Units_TotalDamageDealt", bp, amount);
        }
    }
    if let Some((army, blueprint)) = to
        && let Some(a) = armies.get_mut(army) {
        a.add_stat("DamageStats_TotalDamageReceived", amount);
        if let Some(bp) = blueprint {
            a.add_blueprint_stat("Units_TotalDamageReceive", bp, amount);
        }
    }
}
