//! Sacrifice orders (moho): `IssueSacrifice(units, target)` (Aeon engineers
//! and the ACU-class units with `RULEUCC_Sacrifice`).
//!
//! A unit whose current order is a sacrifice into a unit under construction
//! walks into build range (movement), then gives itself up in one go:
//! `OnStartSacrifice(target)`, the target's build progress (and health) rises
//! by `min(mass * SacrificeMassMult / targetMass, energy *
//! SacrificeEnergyMult / targetEnergy)` (FAF's `CalculateSacrificeBonus`,
//! blueprint costs), completing it if that reaches 1, then
//! `OnStopSacrifice(target)` (FAF destroys the sacrificer). A target that is
//! gone or already finished ends the order.
//!
//! Inferred, not verified against the binary: instant (moho may take time),
//! the reach rule, and blueprint costs (FAF's own helper adds enhancements).

use mlua::prelude::*;

use crate::engine::api::callbacks::UnitCallbacks;
use crate::engine::api::entity;
use crate::lua_api::Obj;
use crate::engine::sim::command::{CommandKind, CommandTarget, Commands};
use crate::engine::sim::entity::EntityId;
use crate::engine::sim::math::{from_lua, Fx};

/// Blueprint `Economy` (BuildCostMass, BuildCostEnergy, SacrificeMassMult,
/// SacrificeEnergyMult) of a unit.
fn economy(lua: &Lua, unit: EntityId) -> LuaResult<(f64, f64, f64, f64)> {
    let bp_id = entity::world(lua).get(unit).map(|e| e.cold.blueprint_id.clone()).unwrap_or_default();
    let bp = entity::blueprint_table(lua, crate::engine::api::blueprints::BlueprintKind::Unit, &bp_id)?;
    Ok(match bp.get::<Option<LuaTable>>("Economy")? {
        Some(e) => (
            e.get::<Option<f64>>("BuildCostMass")?.unwrap_or(0.0),
            e.get::<Option<f64>>("BuildCostEnergy")?.unwrap_or(0.0),
            e.get::<Option<f64>>("SacrificeMassMult")?.unwrap_or(0.0),
            e.get::<Option<f64>>("SacrificeEnergyMult")?.unwrap_or(0.0),
        ),
        None => (0.0, 0.0, 0.0, 0.0),
    })
}

/// Once a tick (after movement): sacrificers in reach of their target.
pub fn tick(lua: &Lua) -> LuaResult<()> {
    let orders: Vec<(EntityId, crate::engine::sim::command::CommandId, CommandTarget)> = match lua.app_data_ref::<Commands>() {
        Some(c) => c
            .units_doing(&[CommandKind::Sacrifice])
            .into_iter()
            .filter_map(|u| c.current(u).map(|cmd| (u, cmd.id, cmd.target)))
            .collect(),
        None => return Ok(()),
    };
    for (unit, id, target) in orders {
        let pop = |lua: &Lua| {
            let mut c = lua.app_data_mut::<Commands>().expect("Commands");
            if c.current(unit).is_some_and(|cmd| cmd.id == id) {
                c.pop_front(unit);
            }
        };
        let CommandTarget::Entity(target) = target else {
            pop(lua);
            continue;
        };
        // Reach: build range plus both radii, XZ.
        let (in_reach, valid) = {
            let w = entity::world(lua);
            let (Some(u), Some(t)) = (w.get(unit).filter(|e| !e.destroyed), w.get(target).filter(|e| !e.destroyed)) else {
                drop(w);
                pop(lua);
                continue;
            };
            let valid = t.fraction_complete < Fx::ONE;
            let r = |e: &crate::engine::sim::entity::Entity| e.unit.as_deref().map_or(Fx::ZERO, |d| d.radius);
            let reach = u.unit.as_deref().map_or(Fx::ZERO, |d| d.build_range) + r(u) + r(t) + Fx::ONE;
            let (a, b) = (u.position(), t.position());
            let (dx, dz) = (a.x - b.x, a.z - b.z);
            (dx.saturating_mul(dx).saturating_add(dz.saturating_mul(dz)) <= reach.saturating_mul(reach), valid)
        };
        if !valid {
            pop(lua);
            continue;
        }
        if !in_reach {
            continue;
        }
        let (Some(u_obj), Some(t_obj)) = (entity::object(lua, unit)?, entity::object(lua, target)?) else { continue };
        pop(lua);
        UnitCallbacks::on_start_sacrifice(lua, &u_obj, "unit", Obj::new(t_obj.clone()))?;
        let (mass, energy, mass_mult, energy_mult) = economy(lua, unit)?;
        let (t_mass, t_energy, ..) = economy(lua, target)?;
        let ratio = |v: f64, of: f64| if of > 0.0 { v / of } else { f64::INFINITY };
        let bonus = from_lua(ratio(mass * mass_mult, t_mass).min(ratio(energy * energy_mult, t_energy)).min(1.0));
        let completed = {
            let mut w = entity::world(lua);
            match w.get_mut(target).filter(|t| !t.destroyed && t.fraction_complete < Fx::ONE) {
                Some(t) => {
                    let old = t.fraction_complete;
                    let new = (old + bonus).min(Fx::ONE);
                    t.fraction_complete = new;
                    t.health = (t.health + t.max_health * (new - old)).min(t.max_health);
                    new >= Fx::ONE
                }
                None => false,
            }
        };
        if completed {
            crate::engine::api::construction::complete_without_builder(lua, target, &t_obj, &u_obj)?;
        }
        UnitCallbacks::on_stop_sacrifice(lua, &u_obj, "unit", Obj::new(t_obj))?;
    }
    Ok(())
}
