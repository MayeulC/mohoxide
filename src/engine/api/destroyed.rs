//! `IsDestroyed(obj)` for any engine object.

use mlua::prelude::*;

use crate::engine::api::army::ArmyHandle;
use crate::engine::api::effect::EffectHandle;
use crate::engine::api::entity::EntityHandle;
use crate::engine::api::manipulator::ManipulatorHandle;
use crate::engine::api::platoon::PlatoonHandle;
use crate::engine::api::script_object::C_OBJECT;
use crate::engine::api::weapon::WeaponHandle;
use crate::engine::sim::entity::EntityWorld;
use crate::engine::sim::manipulator::Manipulators;
use crate::engine::sim::platoon::Platoons;
use crate::engine::sim::presentation::Presentation;
use crate::lua_api::{Obj, lua_fn, lua_method};
use crate::engine::api::moho;

fn entity_alive(lua: &Lua, id: u32) -> bool {
    lua.app_data_ref::<EntityWorld>()
        .and_then(|w| w.get(id).map(|e| !e.destroyed))
        .unwrap_or(false)
}

/// True unless `v` is a live engine object (non-engine values count as destroyed).
pub fn is_destroyed(lua: &Lua, v: &LuaValue) -> LuaResult<bool> {
    let LuaValue::Table(t) = v else { return Ok(true) };
    let LuaValue::UserData(ud) = t.raw_get::<LuaValue>(C_OBJECT)? else { return Ok(true) };
    if let Ok(h) = ud.borrow::<EntityHandle>() {
        return Ok(!entity_alive(lua, h.0));
    }
    if let Ok(h) = ud.borrow::<WeaponHandle>() {
        return Ok(!entity_alive(lua, h.unit));
    }
    if let Ok(h) = ud.borrow::<ManipulatorHandle>() {
        return Ok(lua.app_data_ref::<Manipulators>().is_none_or(|m| m.get(h.0).is_none()));
    }
    if let Ok(h) = ud.borrow::<PlatoonHandle>() {
        // A platoon whose last unit left is gone for scripts already, though
        // it ends between thread resumes / at compaction.
        return Ok(lua.app_data_ref::<Platoons>().is_none_or(|p| p.get(h.0).is_none() || p.is_doomed(h.0)));
    }
    if let Ok(h) = ud.borrow::<EffectHandle>() {
        return Ok(lua.app_data_ref::<Presentation>().is_none_or(|p| !p.effects().contains_key(&h.0)));
    }
    Ok(!ud.borrow::<ArmyHandle>().is_ok())
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// False for a live engine object; anything else (nil, a plain table, a destroyed object) counts as destroyed.
        IsDestroyed = |lua, v: LuaValue| is_destroyed(lua, &v))?;
    lua_method!(lua, moho::Entity,
        /// True once the entity is destroyed.
        BeenDestroyed = |lua, this: Obj<moho::Entity>| is_destroyed(lua, &LuaValue::Table(this.into_table())))?;
    lua_method!(lua, moho::Manipulator,
        /// True once the manipulator is destroyed. Moho has it on the slide
        /// manipulator only (SupremeCommander.exe); ours is on every one.
        BeenDestroyed = |lua, this: Obj<moho::Manipulator>| is_destroyed(lua, &LuaValue::Table(this.into_table())))?;
    lua_method!(lua, moho::IEffect,
        /// True once the effect is destroyed. Not moho's (it has it on entities,
        /// weapons and slide manipulators): ours, harmlessly wider.
        BeenDestroyed = |lua, this: Obj<moho::IEffect>| is_destroyed(lua, &LuaValue::Table(this.into_table())))?;
    Ok(())
}
