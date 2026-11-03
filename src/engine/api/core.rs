//! `/engine/Core.lua` globals shared by the sim and the user state that
//! read game data rather than sim state.

use std::sync::{Arc, RwLock};

use mlua::prelude::*;

use crate::engine::vfs::Vfs;
use crate::engine::api::moho::{self, MohoClass};
use crate::lua_api::{Status, lua_fn};

/// A `local <name> = "<value>"` field of FAF's `/lua/version.lua`.
pub fn version_lua_field(version_lua: &str, name: &str) -> Option<String> {
    version_lua.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("local ")?.strip_prefix(name)?.trim_start().strip_prefix('=')?;
        let v = rest.trim().trim_start_matches(['"', '\'']);
        Some(v[..v.find(['"', '\''])?].to_string())
    })
}

fn vfs(lua: &Lua) -> LuaResult<Arc<RwLock<Vfs>>> {
    lua.app_data_ref::<Arc<RwLock<Vfs>>>().map(|v| v.clone()).ok_or_else(|| LuaError::runtime("no VFS"))
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// "1.5.<patch>" (FAF's `VERSION`), the patch from
        /// `/lua/version.lua`. The same on every peer: that file is game code,
        /// which the network version check matches.
        GetVersion = |lua, ()| {
            let vfs = vfs(lua)?;
            let src = vfs.read().ok().and_then(|v| v.read("/lua/version.lua").ok());
            let patch = src.and_then(|s| version_lua_field(&String::from_utf8_lossy(&s), "Version"));
            Ok(format!("1.5.{}", patch.as_deref().unwrap_or("0")))
        })?;
    lua_fn!(lua, global,
        /// Length of the movie in seconds, 0 if it can't be played (FAF's
        /// transmissions fall back to a generic face then).
        GetMovieDuration = |lua, path: String| {
            let reader = vfs(lua)?.read().ok().and_then(|v| v.open(&path).ok());
            Ok(reader.and_then(crate::engine::assets::mpeg::Mpeg::open).map_or(0.0, |m| m.duration()))
        })?;
    // Prefetching is a loading hint; the set accepts updates and does nothing.
    for m in ["Update", "Reset"] {
        crate::lua_api::add_class_fn(lua, moho::CPrefetchSet::CLASS, m, None, "Prefetching is a loading hint: accepted and ignored.", Status::Implemented, |_, _: LuaMultiValue| Ok(()))?;
    }
    lua_fn!(lua, global,
        /// A prefetch set (`Update`, `Reset`); prefetching does nothing.
        CreatePrefetchSet = |lua, ()| {
            let t = lua.create_table()?;
            t.set_metatable(Some(super::moho::class_table(lua, "CPrefetchSet")?));
            Ok(crate::lua_api::Obj::<super::moho::CPrefetchSet>::new(t))
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_lua_fields() {
        let src = "local GameType = 'unknown'  -- x\nlocal Commit = 'unknown'\nlocal Version = \"3839\"\n";
        assert_eq!(super::version_lua_field(src, "Version").as_deref(), Some("3839"));
        assert_eq!(super::version_lua_field(src, "GameType").as_deref(), Some("unknown"));
        assert_eq!(super::version_lua_field(src, "Missing"), None);
    }
}
