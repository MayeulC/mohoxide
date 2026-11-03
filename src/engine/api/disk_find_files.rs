use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use crate::engine::vfs::Vfs;
use crate::lua_api::lua_fn;
use crate::lua_api::shapes::List;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// The files under `directory` (recursive) whose name matches
        /// `pattern`, sorted.
        DiskFindFiles = |lua, (directory, pattern): (String, String)| {
            find_files(lua, &directory, &pattern).map(List::<String>::new)
        })
}

/// Returns files under `directory` (recursive) whose name matches `pattern`,
/// sorted so blueprint/script load order is deterministic.
fn find_files(lua: &Lua, directory: &str, pattern: &str) -> LuaResult<LuaTable> {
    let vfs_arc = lua
        .app_data_ref::<Arc<RwLock<Vfs>>>()
        .expect("VFS not in app_data")
        .clone();
    let vfs = vfs_arc.read().expect("VFS poisoned");
    let paths: Vec<&str> = vfs
        .find_files(directory, pattern)
        .map_err(mlua::Error::external)?
        .collect();

    let t = lua.create_table_with_capacity(paths.len(), 0)?;
    for (i, p) in paths.into_iter().enumerate() {
        t.raw_set(i + 1, p)?;
    }
    Ok(t)
}
