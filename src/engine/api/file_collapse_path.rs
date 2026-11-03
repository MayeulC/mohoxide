use mlua::prelude::*;

use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// Collapses `.` and `..` segments in a path.
        FileCollapsePath = |_, path: LuaString| Ok(collapse(&path.to_str()?)))
}

/// Collapses `.` and `..` segments (and duplicate slashes) in a path, keeping
/// a leading `/`. `..` above the root is dropped, as moho does.
pub fn collapse(path: &str) -> String {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::with_capacity(8);
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let mut out = String::with_capacity(path.len());
    if absolute {
        out.push('/');
    }
    out.push_str(&parts.join("/"));
    if path.ends_with('/') && !parts.is_empty() {
        out.push('/');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::collapse;

    #[test]
    fn collapses_relative_segments() {
        assert_eq!(collapse("/lua/sim/../ui/./foo.lua"), "/lua/ui/foo.lua");
        assert_eq!(collapse("/lua/system/import.lua/../x.lua"), "/lua/system/x.lua");
        assert_eq!(collapse("/../a"), "/a");
        assert_eq!(collapse("a//b\\c"), "a/b/c");
        assert_eq!(collapse("/maps/"), "/maps/");
    }
}
