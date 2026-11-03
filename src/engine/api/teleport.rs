//! Teleport orders (moho): `IssueTeleport` / `IssueTeleportToBeacon`.
//!
//! When a unit's current order becomes a teleport, moho puts it in the
//! `Teleporting` state and calls `OnTeleportUnit(teleporter, location,
//! orientation)`; FAF's script does the rest (charges energy, then `Warp`s
//! while the unit is still `Teleporting`, clearing `TeleportThread` when
//! done). The order completes once that thread is gone. An order that goes
//! away first (cleared, unit dead) gets `OnFailedTeleport`, and the state is
//! dropped so FAF's thread won't warp.
//!
//! Inferred, not verified against the binary: the order ending when FAF's
//! `TeleportThread` ends, the teleporter argument (the beacon for
//! `IssueTeleportToBeacon`, else nil) and the orientation (the unit's own).

use std::collections::BTreeMap;

use mlua::prelude::*;

use crate::engine::api::callbacks::UnitCallbacks;
use crate::engine::api::entity;
use crate::engine::sim::command::{CommandId, CommandKind, CommandTarget, Commands};
use crate::engine::sim::entity::EntityId;
use crate::engine::sim::unit_state::UnitStates;

/// Units teleporting, with the order that started it.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Teleports(BTreeMap<EntityId, CommandId>);

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(Teleports::default());
    Ok(())
}

fn set_teleporting(lua: &Lua, unit: EntityId, on: bool) {
    if let Some(u) = entity::world(lua).get_mut(unit).and_then(|e| e.unit.as_deref_mut()) {
        u.states.set(UnitStates::index("Teleporting").expect("unit state"), on);
    }
}

/// Once a tick: start new teleports, finish or fail running ones.
pub fn tick(lua: &Lua) -> LuaResult<()> {
    // Teleport orders that just became current.
    let starting: Vec<(EntityId, CommandId, CommandTarget)> = match lua.app_data_ref::<Commands>() {
        Some(c) => {
            let running = lua.app_data_ref::<Teleports>().expect("Teleports");
            c.units_doing(&[CommandKind::Teleport])
                .into_iter()
                .filter_map(|u| c.current(u).map(|cmd| (u, cmd.id, cmd.target)))
                .filter(|(u, id, _)| running.0.get(u) != Some(id))
                .collect()
        }
        None => return Ok(()),
    };
    let running: Vec<(EntityId, CommandId)> = lua.app_data_ref::<Teleports>().expect("Teleports").0.iter().map(|(u, id)| (*u, *id)).collect();
    for (unit, id) in running {
        let alive = entity::world(lua).get(unit).is_some_and(|e| !e.destroyed);
        let current = lua.app_data_ref::<Commands>().and_then(|c| c.current(unit).map(|cmd| cmd.id)) == Some(id);
        let obj = if alive { entity::object(lua, unit)? } else { None };
        if !alive || !current {
            lua.app_data_mut::<Teleports>().expect("Teleports").0.remove(&unit);
            if let Some(o) = obj {
                set_teleporting(lua, unit, false);
                UnitCallbacks::on_failed_teleport(lua, &o, "unit")?;
            }
            continue;
        }
        // FAF's teleport thread finished (warped, or refused the jump).
        let thread_done = match &obj {
            Some(o) => o.get::<LuaValue>("TeleportThread")?.is_nil(),
            None => true,
        };
        if thread_done {
            lua.app_data_mut::<Teleports>().expect("Teleports").0.remove(&unit);
            set_teleporting(lua, unit, false);
            let mut c = lua.app_data_mut::<Commands>().expect("Commands");
            if c.current(unit).is_some_and(|cmd| cmd.id == id) {
                c.pop_front(unit);
            }
        }
    }
    for (unit, id, target) in starting {
        let Some(obj) = entity::object(lua, unit)? else { continue };
        let (location, teleporter) = match target {
            CommandTarget::Ground(p) => (p, LuaValue::Nil),
            CommandTarget::Entity(beacon) => match entity::world(lua).get(beacon).map(|e| e.position()) {
                Some(p) => (p, entity::object(lua, beacon)?.map_or(LuaValue::Nil, LuaValue::Table)),
                None => continue,
            },
            CommandTarget::None => continue,
        };
        let orientation = entity::world(lua).get(unit).map_or(crate::engine::sim::math::FxQuat::IDENTITY, |e| e.orientation);
        lua.app_data_mut::<Teleports>().expect("Teleports").0.insert(unit, id);
        set_teleporting(lua, unit, true);
        UnitCallbacks::on_teleport_unit(lua, &obj, "unit", crate::lua_api::shapes::Loose::new(teleporter), entity::vector(lua, location)?, entity::quaternion(lua, orientation)?)?;
    }
    Ok(())
}
