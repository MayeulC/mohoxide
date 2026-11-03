use std::sync::Arc;

use miette::Report;
use mlua::prelude::*;
use mlua::Lua;
use tracing::error;

use crate::engine::lua::error::prettify_lua_error;

pub fn exec_lua_string(lua: &Lua, code: &str, file_name: &str) -> LuaResult<()> {
    lua.load(code)
        .set_name(file_name)
        .exec()
        .inspect_err(|e: &mlua::Error| {
            let pretty =
                prettify_lua_error(e, code, |path| match std::fs::read_to_string(path) {
                    Ok(v) => Some(Arc::new(v)),
                    Err(_e) => None,
                });
            println!("{:?}", Report::new(pretty));
            error!("lua script error in {file_name}");
        })?;
    Ok(())
}
