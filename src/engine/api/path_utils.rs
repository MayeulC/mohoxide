//! Moho path helpers. Pure string functions; accept `\\` or `/` separators.

use mlua::prelude::*;

use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// Everything before the last `/` or `\\` (no trailing slash); empty
        /// if there is no separator.
        Dirname = |_, path: LuaString| Ok(dirname(&path.to_str()?).to_owned()))?;
    Ok(())
}

fn last_sep(path: &str) -> Option<usize> {
    path.rfind(['/', '\\'])
}

/// Everything before the last separator, without a trailing slash
/// (`/mods/foo/mod_info.lua` → `/mods/foo`). No separator → empty string.
pub fn dirname(path: &str) -> &str {
    match last_sep(path) {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirname_strips_last_component() {
        assert_eq!(dirname("/mods/foo/mod_info.lua"), "/mods/foo");
        assert_eq!(dirname("C:\\games\\fa\\x.lua"), "C:\\games\\fa");
        assert_eq!(dirname("/file.lua"), "/");
        assert_eq!(dirname("file.lua"), "");
    }
}
