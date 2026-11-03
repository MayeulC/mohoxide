use mlua::prelude::*;

use crate::lua_api::lua_fn;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// Logs the arguments (tab-joined, via `tostring`) at info level.
        LOG = |lua, args: LuaMultiValue| {
            let msg = format_args(lua, args)?;
            tracing::info!(target: "lua", "{msg}");
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Logs the arguments (tab-joined, via `tostring`) at info level, as
        /// `LOG` (LuaPlus_1081.dll 0x1000afa0 registers one function as both).
        _ALERT = |lua, args: LuaMultiValue| {
            let msg = format_args(lua, args)?;
            tracing::info!(target: "lua", "{msg}");
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Logs the arguments (tab-joined, via `tostring`) at warning level.
        WARN = |lua, args: LuaMultiValue| {
            let msg = format_args(lua, args)?;
            tracing::warn!(target: "lua", "{msg}");
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Logs the arguments (tab-joined, via `tostring`) at debug level.
        SPEW = |lua, args: LuaMultiValue| {
            let msg = format_args(lua, args)?;
            tracing::debug!(target: "lua", "{msg}");
            Ok(())
        })?;
    Ok(())
}

/// Joins all arguments with tabs, converting via `tostring` (honours `__tostring`).
/// Fast path: a single string argument is used as-is without calling into Lua.
fn format_args(lua: &Lua, args: LuaMultiValue) -> LuaResult<String> {
    if args.len() == 1
        && let Some(LuaValue::String(s)) = args.front() {
        return Ok(s.to_string_lossy());
    }
    let tostring: LuaFunction = lua.globals().get("tostring")?;
    let mut out = String::new();
    for (i, v) in args.into_iter().enumerate() {
        if i > 0 {
            out.push('\t');
        }
        match v {
            LuaValue::String(s) => out.push_str(&s.to_string_lossy()),
            v => out.push_str(&tostring.call::<LuaString>(v)?.to_string_lossy()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_mixed_args() {
        let lua = Lua::new();
        let args = lua
            .load("return 'a', 1, nil, true")
            .eval::<LuaMultiValue>()
            .unwrap();
        assert_eq!(format_args(&lua, args).unwrap(), "a\t1\tnil\ttrue");
    }
}
