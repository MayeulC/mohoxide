//! Structure adjacency callbacks (moho): when a structure is finished, each
//! same-army structure whose build skirt touches its own gets
//! `OnAdjacentTo(other, trigger)` in both directions (FAF's adjacency
//! bonuses: mass storage by extractors, power by factories, ...); when one
//! is destroyed, `OnNotAdjacentTo(other)` in both directions.

use mlua::prelude::*;

use crate::engine::api::callbacks::UnitCallbacks;
use crate::engine::api::entity;
use crate::lua_api::Obj;
use crate::engine::sim::entity::EntityId;

/// `id` (a structure) is finished: it and each structure touching it learn
/// of each other.
pub fn on_finished(lua: &Lua, id: EntityId) -> LuaResult<()> {
    let others = crate::engine::api::build::adjacent_structures(lua, id)?;
    let Some(me) = entity::object(lua, id)? else { return Ok(()) };
    for o in others {
        let Some(other) = entity::object(lua, o)? else { continue };
        UnitCallbacks::on_adjacent_to(lua, &me, "unit", Obj::new(other.clone()), Obj::new(me.clone()))?;
        UnitCallbacks::on_adjacent_to(lua, &other, "unit", Obj::new(me.clone()), Obj::new(me.clone()))?;
    }
    Ok(())
}

/// `id` (a structure) is about to be destroyed: it and each structure
/// touching it part ways.
pub fn on_destroying(lua: &Lua, id: EntityId) -> LuaResult<()> {
    let others = crate::engine::api::build::adjacent_structures(lua, id)?;
    if others.is_empty() {
        return Ok(());
    }
    let Some(me) = entity::object(lua, id)? else { return Ok(()) };
    for o in others {
        let Some(other) = entity::object(lua, o)? else { continue };
        UnitCallbacks::on_not_adjacent_to(lua, &me, "unit", Obj::new(other.clone()))?;
        UnitCallbacks::on_not_adjacent_to(lua, &other, "unit", Obj::new(me.clone()))?;
    }
    Ok(())
}
