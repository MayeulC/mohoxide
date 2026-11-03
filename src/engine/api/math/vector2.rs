use mlua::prelude::*;

use crate::lua_api::lua_fn;

#[derive(Clone, Copy, Debug)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let metatable = lua.create_table()?;

    // Maps x/y/z/w string keys to raw numeric indices 1/2/3/4.
    // Unknown string keys and all non-string keys return nil.
    // (vec.z on a Vector2 returns nil cleanly since index 3 is unpopulated.)
    let index_fn =
        lua.create_function(|_, (t, k): (LuaTable, LuaValue)| -> LuaResult<LuaValue> {
            if let LuaValue::String(ref s) = k {
                let idx: Option<i64> = match &*s.to_str()? {
                    "x" => Some(1),
                    "y" => Some(2),
                    "z" => Some(3),
                    "w" => Some(4),
                    _ => None,
                };
                if let Some(i) = idx {
                    return t.raw_get(i);
                }
            }
            Ok(LuaValue::Nil)
        })?;
    metatable.raw_set("__index", index_fn)?;

    // Routes x/y/z/w string writes to raw numeric indices 1/2/3/4.
    // Other keys are written as-is.
    let newindex_fn = lua.create_function(
        |_, (t, k, v): (LuaTable, LuaValue, LuaValue)| -> LuaResult<()> {
            if let LuaValue::String(ref s) = k {
                let idx: Option<i64> = match &*s.to_str()? {
                    "x" => Some(1),
                    "y" => Some(2),
                    "z" => Some(3),
                    "w" => Some(4),
                    _ => None,
                };
                if let Some(i) = idx {
                    return t.raw_set(i, v);
                }
            }
            t.raw_set(k, v)
        },
    )?;
    metatable.raw_set("__newindex", newindex_fn)?;

    // Stored in the registry so both constructors can retrieve it without ownership issues.
    lua.set_named_registry_value("vector_metatable", metatable)?;

    lua_fn!(lua, global,
        /// A 2D vector `{x, y}` (`v.x` / `v.y` read indices 1 and 2).
        Vector2 = |lua, (x, y): (f64, f64)| {
            let t = lua.create_table()?;
            t.raw_set(1, x)?;
            t.raw_set(2, y)?;
            let mt: LuaTable = lua.named_registry_value("vector_metatable")?;
            t.set_metatable(Some(mt));
            Ok(crate::lua_api::shapes::Vector2(t))
        })?;

    lua_fn!(lua, global,
        /// A 3D vector `{x, y, z}` (`v.x` / `v.y` / `v.z` read indices 1 to 3).
        Vector = |lua, (x, y, z): (f64, f64, f64)| {
            let t = lua.create_table()?;
            t.raw_set(1, x)?;
            t.raw_set(2, y)?;
            t.raw_set(3, z)?;
            let mt: LuaTable = lua.named_registry_value("vector_metatable")?;
            t.set_metatable(Some(mt));
            Ok(crate::lua_api::shapes::Vector(t))
        })?;

    Ok(())
}
