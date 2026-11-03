use mlua::prelude::*;

use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// Drives the loading-screen animation in moho; called once per
        /// blueprint file, so it is a zero-cost native no-op until a loading
        /// UI exists.
        BlueprintLoaderUpdateProgress = |_, ()| Ok(()))
}
