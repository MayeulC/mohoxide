use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use crate::engine::vfs::Vfs;
use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// True if a file or folder exists in the VFS.
        exists = |lua, name: String| {
            let vfs = lua
                .app_data_ref::<Arc<RwLock<Vfs>>>()
                .expect("VFS not in app_data")
                .clone();
            let found = vfs.read().expect("VFS poisoned").file_info(&name).is_some();
            Ok(found)
        })
}
