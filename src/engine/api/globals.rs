use mlua::prelude::*;

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.globals().set("Engine", "rts-core-engine")?;
    lua.globals().set("ClientVersion", "0")?;
    lua.globals().set("GameVersion", "0")?;
    lua.globals().set("GameType", "0")?;
    lua.globals().set("custom_vault_path", LuaNil)?;
    lua.globals().set("__active_mods", lua.create_table()?)?;
    // Set by the engine before sim init (see /lua/userInit.lua). TODO: from
    // game options once a lobby/launch config exists.
    lua.globals().set("__language", "us")?;
    Ok(())
}
