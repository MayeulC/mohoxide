use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use crate::engine::vfs::Vfs;
use crate::lua_api::lua_fn;
use crate::lua_api::shapes::Shaped;

/// The table `DiskGetFileInfo` returns for an existing path.
#[derive(crate::lua_api::LuaTable)]
#[lua(name = "FileInfo", write_only, rename_all = "PascalCase")]
#[allow(dead_code)]
struct FileInfo {
    size_bytes: u64,
    is_folder: bool,
    is_read_only: bool,
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// `{ SizeBytes, IsFolder, IsReadOnly }` of a VFS path, or false if it
        /// doesn't exist.
        DiskGetFileInfo = |lua, path: String| {
            let vfs = lua
                .app_data_ref::<Arc<RwLock<Vfs>>>()
                .expect("VFS not in app_data")
                .clone();
            let info = vfs.read().expect("VFS poisoned").file_info(&path);
            let Some(info) = info else {
                return Ok(mlua::Either::Left(false));
            };
            // WriteTime/LastWriteTime are omitted on purpose: they differ per
            // machine and would break lockstep determinism if the sim read them.
            let t = lua.create_table_with_capacity(0, 3)?;
            t.raw_set("SizeBytes", info.size)?;
            t.raw_set("IsFolder", info.is_folder)?;
            t.raw_set("IsReadOnly", info.read_only)?;
            Ok(mlua::Either::Right(Shaped::<FileInfo>::new(t)))
        })
}
