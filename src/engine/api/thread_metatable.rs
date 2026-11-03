//! Moho's thread type metatable. `getmetatable(thread)` returns an engine
//! table `T` with `T.__index = T`; `/lua/system/config.lua` then gives `T` its
//! own metatable (`{ Destroy = KillThread }`), so `thread:Destroy()` resolves
//! through the chain. Installed as Luau's per-type metatable for threads.

use mlua::prelude::*;

pub fn register(lua: &Lua) -> LuaResult<()> {
    let t = lua.create_table()?;
    t.raw_set("__index", t.clone())?;
    let sample = lua.create_thread(lua.create_function(|_, ()| Ok(()))?)?;
    // SAFETY: stack holds [T, thread]; set T as the metatable of the thread
    // value, which Luau stores as the type metatable for all threads.
    unsafe {
        lua.exec_raw::<()>((t, sample), |state| {
            mlua::ffi::lua_pushvalue(state, -2);
            mlua::ffi::lua_setmetatable(state, -2);
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn thread_methods_resolve_through_config_chain() {
        let lua = mlua::Lua::new();
        super::register(&lua).unwrap();
        let r: String = lua
            .load(
                "local mt = getmetatable(coroutine.create(function() end))
                 setmetatable(mt, { __index = { Destroy = function(co) return type(co) end } })
                 return coroutine.create(function() end):Destroy()",
            )
            .eval()
            .unwrap();
        assert_eq!(r, "thread");
    }
}
