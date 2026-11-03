use mlua::prelude::*;

use crate::lua_api::lua_fn;

/// Registers SHGetFolderPath.
pub fn register_unsafe(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// The Windows Shell function FA uses to locate user data directories:
        /// `"LOCAL_APPDATA"` (AppData) or `"PERSONAL"` (Documents), with a
        /// trailing slash; empty for any other folder.
        SHGetFolderPath = |_, folder: String| {
            let path = match folder.as_str() {
                "LOCAL_APPDATA" if cfg!(windows) => std::env::var("LOCALAPPDATA").unwrap_or_default() + "/",
                // macOS: the per-user application data folder.
                "LOCAL_APPDATA" => std::env::var("HOME").unwrap_or_default() + "/Library/Application Support/",
                "PERSONAL" => documents_dir(),
                _ => String::new(),
            };
            Ok(path)
        })
}

/// My Documents (`SHGetFolderPath('PERSONAL')`), with a trailing slash.
/// USERPROFILE is the profile root; Documents is always the subfolder on
/// default Windows installs.
pub fn documents_dir() -> String {
    std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_default() + "/Documents/"
}
