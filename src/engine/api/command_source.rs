//! Command sources: who sent the command being processed, and which armies
//! they may command.
//!
//! A command source is a player (each human army's, in army order) or an
//! observer after them, as in moho's lobby. Rust keeps them 0-based; Lua sees
//! `GetCurrentCommandSource` 1-based (FAF indexes `ArmyBrains` and its
//! source-to-army map with it) while `SetCommandSource` takes 0-based indices
//! (FAF subtracts 1 before calling it).

use mlua::prelude::*;

use crate::engine::sim::army::Armies;
use crate::lua_api::lua_fn;

/// The command source (0-based) whose command / sim callback is being
/// processed; `None` outside of one.
#[derive(Clone, Copy, Debug, Default)]
pub struct CurrentCommandSource(pub Option<u32>);

/// The local player's command source (0-based), stamped on their input.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalCommandSource(pub u32);

/// The session's `CheatsEnabled` option, and the sources caught trying to
/// cheat (sorted).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Cheats {
    pub enabled: bool,
    pub cheaters: Vec<u32>,
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(CurrentCommandSource::default());
    lua.set_app_data(LocalCommandSource::default());
    lua.set_app_data(Cheats::default());
    lua_fn!(lua, global,
        /// The command source (1-based) whose command / sim callback is being
        /// processed; nil outside of one.
        GetCurrentCommandSource = |lua, ()| Ok(current(lua).map(|s| s + 1)))?;
    lua_fn!(lua, global,
        /// The session's `CheatsEnabled` option. Asking when it is off counts
        /// as trying to cheat: the source is noted and it is logged.
        CheatsEnabled = |lua, ()| Ok(cheats_enabled(lua)))?;
    lua_fn!(lua, global,
        /// Whether the current command source may command `army` (1-based;
        /// nil or below 1 counts as 0).
        OkayToMessWithArmy = |lua, army: Option<f64>| Ok(okay_to_mess_with_army(lua, army.filter(|a| *a >= 1.0).map_or(0, |a| a as u32))))?;
    lua_fn!(lua, global,
        /// Lets or stops command source `source` (0-based) commanding `army`
        /// (0-based; FAF subtracts 1 before calling).
        SetCommandSource = |lua, (army, source, on): (u32, u32, bool)| {
            let mut armies = lua.app_data_mut::<Armies>().expect("Armies not in app_data");
            let Some(a) = armies.get_mut(army + 1) else { return Err(LuaError::runtime(format!("SetCommandSource: invalid army {army}"))) };
            if on {
                a.command_sources.insert(source);
            } else {
                a.command_sources.remove(&source);
            }
            Ok(())
        })?;
    Ok(())
}

pub fn current(lua: &Lua) -> Option<u32> {
    lua.app_data_ref::<CurrentCommandSource>().and_then(|s| s.0)
}

/// Runs `f` as command source `source` (`GetCurrentCommandSource`,
/// `OkayToMessWithArmy`), restoring the previous one after.
pub fn with_source<R>(lua: &Lua, source: u32, f: impl FnOnce() -> R) -> R {
    let previous = lua.app_data_mut::<CurrentCommandSource>().and_then(|mut s| s.0.replace(source));
    let result = f();
    if let Some(mut s) = lua.app_data_mut::<CurrentCommandSource>() {
        s.0 = previous;
    }
    result
}

/// Moho's `Sim::CheatsEnabled`: the session option. Asking when they're off
/// counts as trying to cheat: the source is noted and it is logged.
pub fn cheats_enabled(lua: &Lua) -> bool {
    let source = current(lua);
    let Some(mut cheats) = lua.app_data_mut::<Cheats>() else { return false };
    if cheats.enabled {
        return true;
    }
    if let Some(s) = source
        && let Err(i) = cheats.cheaters.binary_search(&s) {
        cheats.cheaters.insert(i, s);
    }
    drop(cheats);
    tracing::warn!(target: "lua", "{} is trying to cheat!", source_name(lua, source));
    false
}

/// Moho's `Sim::OkayToMessWith(army)`: never a defeated army; else the
/// current source must be one of the army's, or cheats on.
pub fn okay_to_mess_with_army(lua: &Lua, army: u32) -> bool {
    let source = current(lua);
    let allowed = {
        let armies = lua.app_data_ref::<Armies>().expect("Armies not in app_data");
        match armies.get(army) {
            Some(a) if a.out_of_game => return false,
            Some(a) => source.is_some_and(|s| a.command_sources.contains(&s)),
            None => false,
        }
    };
    allowed || cheats_enabled(lua)
}

/// A source's player name (its human army's nickname), for logs.
fn source_name(lua: &Lua, source: Option<u32>) -> String {
    let Some(s) = source else { return "the sim".into() };
    let armies = lua.app_data_ref::<Armies>().expect("Armies not in app_data");
    armies.list.iter().filter(|a| a.human).nth(s as usize).map_or_else(|| format!("observer (source {})", s + 1), |a| a.nickname.clone())
}

/// After the armies exist: the local player's source is their army's
/// player (`human_army`), else an observer after every player. The sim's
/// `GetFocusArmy` is that player's army (-1: observer), so FAF's score
/// thread syncs only their own and allied scores to the UI.
pub fn set_local_source(lua: &Lua, human_army: Option<u32>) {
    let (source, focus) = {
        let armies = lua.app_data_ref::<Armies>().expect("Armies not in app_data");
        let humans = armies.list.iter().filter(|a| a.human).count() as u32;
        let source = human_army.and_then(|h| armies.get(h)).and_then(|a| a.command_sources.first().copied()).unwrap_or(humans);
        (source, armies.list.iter().find(|a| a.command_sources.contains(&source)).map_or(-1, |a| a.index as i32))
    };
    lua.set_app_data(LocalCommandSource(source));
    lua.set_app_data(crate::engine::api::army::FocusArmy(focus));
}

pub fn local_source(lua: &Lua) -> u32 {
    lua.app_data_ref::<LocalCommandSource>().map_or(0, |s| s.0)
}
