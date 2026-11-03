//! The `moho` global: engine (C++ in the original) classes exported to Lua.
//!
//! Each class is a table of native methods; base classes are stored as nested
//! tables at integer keys. `/lua/globalInit.lua` flattens each one in place via
//! `ConvertCClassToLuaSimplifiedClass`, keeping the *same* table, so the engine
//! can keep using its reference (we keep one in the Lua registry per class) to
//! attach metatables to the objects it creates.
//!
//! Methods are added per class with `lua_api::lua_method!` (typed `self`,
//! recorded for the generated definitions).

use mlua::prelude::*;

/// Sim + core classes and their bases, in dependency order (bases first).
/// Inheritance follows FAF's `/engine/**/*.lua` annotations.
pub const SIM_CLASSES: &[(&str, &[&str])] = &[
    ("entity_methods", &[]),
    ("manipulator_methods", &[]),
    ("unit_methods", &["entity_methods"]),
    ("projectile_methods", &["entity_methods"]),
    ("prop_methods", &["entity_methods"]),
    ("blip_methods", &["entity_methods"]),
    ("shield_methods", &["entity_methods"]),
    ("CollisionBeamEntity", &["entity_methods"]),
    ("weapon_methods", &[]),
    ("aibrain_methods", &[]),
    ("aipersonality_methods", &[]),
    ("CAiAttackerImpl_methods", &[]),
    ("navigator_methods", &[]),
    ("platoon_methods", &[]),
    ("ScriptTask_Methods", &[]),
    ("AimManipulator", &["manipulator_methods"]),
    ("AnimationManipulator", &["manipulator_methods"]),
    ("BoneEntityManipulator", &["manipulator_methods"]),
    ("BuilderArmManipulator", &["manipulator_methods"]),
    ("CollisionManipulator", &["manipulator_methods"]),
    ("FootPlantManipulator", &["manipulator_methods"]),
    ("RotateManipulator", &["manipulator_methods"]),
    ("SlaveManipulator", &["manipulator_methods"]),
    ("SlideManipulator", &["manipulator_methods"]),
    ("StorageManipulator", &["manipulator_methods"]),
    ("ThrustManipulator", &["manipulator_methods"]),
    ("CDamage", &[]),
    ("CDecalHandle", &[]),
    ("EconomyEvent", &[]),
    ("IEffect", &[]),
    ("MotorFallDown", &[]),
    // core (shared with the UI state)
    ("sound_methods", &[]),
    ("EntityCategory", &[]),
    ("CPrefetchSet", &[]),
];

fn registry_key(class: &str) -> String {
    format!("moho.{class}")
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    register_classes(lua, SIM_CLASSES)
}

/// The `moho` global with `classes` (the user layer has its own set).
pub fn register_classes(lua: &Lua, classes: &[(&'static str, &[&'static str])]) -> LuaResult<()> {
    let moho = lua.create_table_with_capacity(0, classes.len())?;
    for &(name, bases) in classes {
        let class = lua.create_table()?;
        for (i, base) in bases.iter().enumerate() {
            let base: LuaTable = moho.raw_get(*base)?;
            class.raw_set(i + 1, base)?;
        }
        lua.set_named_registry_value(&registry_key(name), class.clone())?;
        moho.raw_set(name, class)?;
        crate::lua_api::register_class(lua, name, bases);
    }
    lua.globals().set("moho", moho)
}

/// Class tables already looked up, by name (each VM its own).
#[derive(Default)]
struct ClassTables(std::collections::BTreeMap<String, LuaTable>);

/// The class table for `class` (stable for the lifetime of the VM). Cached:
/// it's fetched per created object (every effect, blip, ...), and the
/// registry key was a fresh string each time.
pub fn class_table(lua: &Lua, class: &str) -> LuaResult<LuaTable> {
    if let Some(t) = lua.app_data_ref::<ClassTables>().and_then(|c| c.0.get(class).cloned()) {
        return Ok(t);
    }
    let t: LuaTable = lua.named_registry_value(&registry_key(class))?;
    if lua.app_data_ref::<ClassTables>().is_none() {
        lua.set_app_data(ClassTables::default());
    }
    if let Some(mut c) = lua.app_data_mut::<ClassTables>() {
        c.0.insert(class.to_owned(), t.clone());
    }
    Ok(t)
}

/// The moho classes as Rust types (`moho::Unit`, ...), for typed methods
/// (`lua_method!`), `self` (`Obj<moho::Unit>`) and callbacks.
pub use crate::lua_api::class::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_are_nested_and_registry_is_same_table() {
        let lua = Lua::new();
        register(&lua).unwrap();
        crate::lua_api::add_class_fn(&lua, "entity_methods", "GetEntityId", None, "", crate::lua_api::Status::Implemented, |_, ()| Ok("1")).unwrap();
        let ok: bool = lua
            .load("return moho.unit_methods[1] == moho.entity_methods and moho.unit_methods[1].GetEntityId ~= nil")
            .eval()
            .unwrap();
        assert!(ok);
        let t = class_table(&lua, "unit_methods").unwrap();
        let same: bool = lua.load("return ... == moho.unit_methods").call(t).unwrap();
        assert!(same);
    }
}
