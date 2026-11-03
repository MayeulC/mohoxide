//! Sim clock: 10 ticks per second (moho's fixed sim rate), and the end of
//! the game (`EndGame` / `IsGameOver`).

use mlua::prelude::*;

use crate::engine::sim::scheduler::Scheduler;
use crate::lua_api::lua_fn;

pub const SECONDS_PER_TICK: f64 = 0.1;

/// Whether the session has ended (MohoEngine.dll `Sim::EndGame` 0x10319ea0,
/// `Sim::IsGameOver` 0x10133900, the latch at the end of `Sim::Sync`
/// 0x10317413, the skip in `Sim::AdvanceBeat` 0x10318fbb). `EndGame` sets
/// `requested` and `IsGameOver` is true from then on; the end of that tick's
/// beat latches it into `ended` (the user layer hears of it with that beat),
/// and every later tick does nothing. Sim state: every client calls
/// `EndGame` at the same tick, so every client stops at the same tick.
#[derive(Default, Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct GameOver {
    pub requested: bool,
    pub ended: bool,
}

fn tick(lua: &Lua) -> u64 {
    lua.app_data_ref::<Scheduler>().map_or(0, |s| s.tick())
}

/// The tick number this sim tick's snapshot will carry (`Scheduler::
/// snapshot_tick`): the clock effects are created and expired on.
pub fn snapshot_tick(lua: &Lua) -> u64 {
    lua.app_data_ref::<Scheduler>().map_or(0, |s| s.snapshot_tick())
}

/// The current sim tick (`GetGameTick`).
pub fn current_tick(lua: &Lua) -> u64 {
    tick(lua)
}

/// `EndGame()` was called (`IsGameOver`).
pub fn is_game_over(lua: &Lua) -> bool {
    lua.app_data_ref::<GameOver>().is_some_and(|g| g.requested || g.ended)
}

/// The game ended in an earlier tick: the sim no longer advances.
pub fn has_ended(lua: &Lua) -> bool {
    lua.app_data_ref::<GameOver>().is_some_and(|g| g.ended)
}

/// Ends the game (`EndGame`): the sim stops at the end of this tick.
pub fn end_game(lua: &Lua) {
    match lua.app_data_mut::<GameOver>() {
        Some(mut g) => g.requested = true,
        None => {
            lua.set_app_data(GameOver { requested: true, ended: false });
        }
    }
}

/// End of a tick's beat: an `EndGame` this tick ends the game.
pub fn latch(lua: &Lua) {
    let now = tick(lua);
    if let Some(mut g) = lua.app_data_mut::<GameOver>()
        && g.requested && !g.ended {
        g.ended = true;
        tracing::info!(tick = now, "game over");
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua_fn!(lua, global,
        /// The current sim tick.
        GetGameTick = |lua, ()| Ok(tick(lua)))?;
    // tick * 0.1 in f64 is deterministic (IEEE, same inputs on every client).
    lua_fn!(lua, global,
        /// Sim time in seconds (tick / 10).
        GetGameTimeSeconds = |lua, ()| Ok(tick(lua) as f64 * SECONDS_PER_TICK))?;
    lua_fn!(lua, global,
        /// True once `EndGame` has been called.
        IsGameOver = |lua, ()| Ok(is_game_over(lua)))?;
    lua_fn!(lua, global,
        /// Ends the game at the end of this tick. Moho's ignores any arguments.
        EndGame = |lua, _: LuaMultiValue| {
            end_game(lua);
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Seconds per sim tick (0.1).
        SecondsPerTick = |_, ()| Ok(SECONDS_PER_TICK))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_game_shows_at_once_and_latches_at_the_end_of_the_beat() {
        let lua = Lua::new();
        register(&lua).unwrap();
        let over = |lua: &Lua| lua.load("return IsGameOver()").eval::<bool>().unwrap();
        assert!(!over(&lua));
        latch(&lua);
        assert!(!has_ended(&lua));
        lua.load("EndGame()").exec().unwrap();
        assert!(over(&lua));
        assert!(!has_ended(&lua), "the tick that ends the game still finishes");
        latch(&lua);
        assert!(has_ended(&lua) && over(&lua));
    }
}
