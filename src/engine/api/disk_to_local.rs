use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use crate::engine::vfs::Vfs;
use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// The path on disk of a VFS path.
        DiskToLocal = |lua, path: String| {
            let vfs = lua
                .app_data_ref::<Arc<RwLock<Vfs>>>()
                .expect("VFS not in app_data")
                .clone();
            let local = vfs.read().expect("VFS poisoned").disk_to_local(&path);
            Ok(local)
        })
}
