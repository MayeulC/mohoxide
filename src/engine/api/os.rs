use mlua::prelude::*;

use crate::lua_api::lua_fn;

/// The init VM's `os`: `os.remove` is accepted and ignored. FAF's client
/// init file (`init_faf.lua`) empties the real game's shader cache with it;
/// the engine has no shader cache and leaves the game's files alone.
pub fn register_unsafe(lua: &Lua) -> LuaResult<()> {
    let os = lua.create_table()?;

    lua_fn!(lua, table(os, "os"),
        /// Accepted and ignored: the engine doesn't delete files.
        remove = |_, path: String| {
            tracing::debug!(path, "os.remove ignored: the engine doesn't delete files");
            Ok(())
        })?;

    lua.globals().set("os", os)
}
