//! The sim's `Replay` table: marks a game's major moments in its recording
//! for the replay timeline (`engine::replay::Event`). Engine hooks record
//! an ACU dying, an experimental finished and a nuke launched; mods add
//! their own:
//!
//! ```lua
//! Replay.AddEvent('my_event', unit)
//! ```
//!
//! Only the recording sees them (nothing in the sim reads them back), so
//! they never change the game; without a recording (a replay playing, a
//! headless run) they do nothing.

use mlua::prelude::*;

use crate::engine::api::entity::{entity_id, world};
use crate::engine::api::moho;
use crate::engine::sim::input::Recorder;
use crate::lua_api::{lua_fn, Obj};

pub fn register(lua: &Lua) -> LuaResult<()> {
    let replay = lua.create_table()?;
    lua_fn!(lua, table(replay, "Replay"),
        /// Marks this tick in the recording as `kind` (one word, e.g.
        /// `'acu_death'`) at `unit`: its army, blueprint and position.
        AddEvent = |lua, (kind, unit): (String, Obj<moho::Unit>)| {
            if lua.app_data_ref::<Recorder>().is_none() {
                return Ok(());
            }
            let id = entity_id(&unit)?;
            let event = {
                let w = world(lua);
                let Some(e) = w.get(id) else { return Ok(()) };
                let p = e.position();
                crate::engine::replay::Event {
                    tick: crate::engine::api::sim_time::current_tick(lua),
                    kind,
                    army: e.army.map_or(-1, |a| a as i64),
                    blueprint: e.cold.blueprint_id.clone(),
                    position: [p.x.to_num(), p.y.to_num(), p.z.to_num()],
                }
            };
            write_line(lua, &event.line());
            Ok(())
        })?;
    lua.globals().set("Replay", replay)
}

/// The tick a replay's recording ends at (its `# end` or last `# tick`):
/// the replayed game ends there.
#[derive(Clone, Copy, Debug)]
pub struct ReplayEnd(pub u64);

/// Writes a line to the recording, if there is one (flushed: a game closed
/// abruptly keeps it).
pub fn write_line(lua: &Lua, line: &str) {
    use std::io::Write;
    if let Some(mut r) = lua.app_data_mut::<Recorder>() {
        let _ = writeln!(r.0, "{line}");
        let _ = r.0.flush();
    }
}

/// The game is over or closed: the recording notes its length (`# end`)
/// and is closed, once.
pub fn end_recording(lua: &Lua) {
    if lua.app_data_ref::<Recorder>().is_some() {
        write_line(lua, &format!("# end {}", crate::engine::api::sim_time::current_tick(lua)));
        lua.remove_app_data::<Recorder>();
    }
}
