//! Moho gives `nil` a type metatable so indexing nil yields nil instead of
//! raising (`/lua/system/config.lua` calls `getmetatable(nil)`). FAF code relies
//! on this everywhere, e.g. `bp.Transport.DockingSlots` for units without a
//! `Transport` table.
//!
//! Luau supports per-type metatables for non-table types via the C API, so we
//! install `{ __index = {} }` on nil. The lookup only runs on the would-be-error
//! path, and an empty table (rather than a function) avoids a call per access.
//! Writes to nil still raise.
//!
//! The metatables themselves stay writable: FAF's `config.lua` (`metacleanup`)
//! gives each of nil, false, 0 and '' a metatable whose `__newindex` raises,
//! and Luau can't set a metatable on a readonly table. Their `__index` tables
//! are readonly, so nothing can make indexing nil yield a value.

use mlua::prelude::*;

/// `{ __index = <readonly empty table> }`.
fn index_to_nil(lua: &Lua) -> LuaResult<LuaTable> {
    let index = lua.create_table()?;
    index.set_readonly(true);
    let mt = lua.create_table()?;
    mt.raw_set("__index", index)?;
    Ok(mt)
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let mt = index_to_nil(lua)?;
    // SAFETY: stack holds [mt]; push nil, copy mt, set it as nil's type
    // metatable (pops the copy), then exec_raw's StackGuard restores the stack.
    unsafe {
        lua.exec_raw::<()>(mt, |state| {
            mlua::ffi::lua_pushnil(state);
            mlua::ffi::lua_pushvalue(state, -2);
            mlua::ffi::lua_setmetatable(state, -2);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_nil_yields_nil() {
        let lua = Lua::new();
        register(&lua).unwrap();
        let v: LuaValue = lua
            .load("local bp = {} return bp.Transport.DockingSlots")
            .eval()
            .unwrap();
        assert!(v.is_nil());
    }

    #[test]
    fn writing_to_nil_still_errors() {
        let lua = Lua::new();
        register(&lua).unwrap();
        assert!(lua.load("local t = {} t.a.b = 1").exec().is_err());
    }

    /// Guards the vendored Luau patch (vendor/luau0-src/MOHO_PATCHES.md).
    #[test]
    fn mismatched_types_order_by_type() {
        let lua = Lua::new();
        // nil sorts first: `nil > 80` false, `nil < 80` true, `1 <= nil`
        // false, `80 > nil` true (FAF's GetTopmostWorldViewAt).
        let r: (bool, bool, bool, bool) = lua
            .load("local t = {} return t.x > 80, t.x < 80, 1 <= t.x, 80 > t.x")
            .eval()
            .unwrap();
        assert_eq!(r, (false, true, false, true));
        // Same-type values with no order still raise, as in moho.
        assert!(lua.load("local t = {} return t.x >= t.y").exec().is_err());
        assert!(lua.load("return true < false").exec().is_err());
        // boolean < number < string.
        let mixed: (bool, bool) = lua.load("return false > 0, 1 < 'a'").eval().unwrap();
        assert_eq!(mixed, (false, true));
        // Same-type comparisons are unchanged.
        let same: (bool, bool) = lua.load("return 1 < 2, 'a' < 'b'").eval().unwrap();
        assert_eq!(same, (true, true));
    }

    /// FAF's `config.lua` `metacleanup`, verbatim in behaviour: it must run
    /// on our type metatables and then block writes to them.
    #[test]
    fn faf_metacleanup_runs_and_blocks_writes() {
        let lua = Lua::new();
        register(&lua).unwrap();
        register_booleans(&lua).unwrap();
        register_number_metatable(&lua).unwrap();
        for v in ["nil", "false", "0", "''"] {
            lua.load(format!("setmetatable(getmetatable({v}), {{ __newindex = function() error('blocked', 2) end }})"))
                .exec()
                .unwrap();
        }
        for v in ["nil", "false", "0", "''"] {
            assert!(lua.load(format!("getmetatable({v}).x = 1")).exec().is_err(), "{v}");
        }
        assert!(lua.load("getmetatable(nil).__index.x = 1").exec().is_err());
        // The sim's number metatable doesn't make numbers indexable.
        assert!(lua.load("local n = 0 return n.x").exec().is_err());
    }

    #[test]
    fn calling_nil_still_errors() {
        let lua = Lua::new();
        register(&lua).unwrap();
        assert!(lua.load("local t = {} t.missing()").exec().is_err());
    }
}

/// Numbers index to nil too (user layer): FAF's `LazyVar.Destroy` does
/// `val.Destroy` on whatever value the var holds, numbers included (text
/// controls' point sizes), which only works if indexing a number yields
/// nil. Inferred from that code; not yet checked for the sim state.
pub fn register_numbers(lua: &Lua) -> LuaResult<()> {
    set_number_metatable(lua, index_to_nil(lua)?)
}

/// The sim's numbers get an empty metatable: `config.lua` needs
/// `getmetatable(0)` to be a table, but indexing a number still raises there.
pub fn register_number_metatable(lua: &Lua) -> LuaResult<()> {
    set_number_metatable(lua, lua.create_table()?)
}

fn set_number_metatable(lua: &Lua, mt: LuaTable) -> LuaResult<()> {
    // SAFETY: as `register`, with a number as the type's representative.
    unsafe {
        lua.exec_raw::<()>(mt, |state| {
            mlua::ffi::lua_pushnumber(state, 0.0);
            mlua::ffi::lua_pushvalue(state, -2);
            mlua::ffi::lua_setmetatable(state, -2);
        })
    }
}

/// Booleans index to nil too (both layers), as in moho's LuaPlus: FAF's
/// `campaignmanager.lua` does `GetNextOperation(...).opID` where that
/// returns `false` before any operation is finished (the operation list of
/// the campaign screen). FAF's `config.lua` only blocks writes to booleans.
pub fn register_booleans(lua: &Lua) -> LuaResult<()> {
    let mt = index_to_nil(lua)?;
    // SAFETY: as `register`, with a boolean as the type's representative.
    unsafe {
        lua.exec_raw::<()>(mt, |state| {
            mlua::ffi::lua_pushboolean(state, 0);
            mlua::ffi::lua_pushvalue(state, -2);
            mlua::ffi::lua_setmetatable(state, -2);
        })
    }
}

#[cfg(test)]
mod boolean_tests {
    #[test]
    fn indexing_a_boolean_yields_nil_but_writing_still_errors() {
        let lua = mlua::Lua::new();
        super::register_booleans(&lua).unwrap();
        let v: mlua::Value = lua.load("local f = function() return false end return f().opID").eval().unwrap();
        assert!(v.is_nil());
        assert!(lua.load("local b = true b.x = 1").exec().is_err());
    }
}
