use crate::engine::lua::lua_pretty::*;
use crate::lua_api::lua_fn;
use mlua::prelude::*;

pub fn register_safe(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// A readable rendering of any value, tables expanded.
        repr = |_lua, value: LuaValue| Ok(PrettyLua(value).to_string()))
}
