//! The interface's `Replay` table: the replay timeline's data and its
//! seeking. The renderer hands over the recording's timeline when the
//! game's interface starts (`UserVm::set_replay_timeline`) and does the
//! seeking (`UserVm::take_seek_request`, `set_seeking`).
//!
//! ```lua
//! for _, e in Replay.GetEvents() do LOG(e.tick, e.kind, e.army) end
//! Replay.Seek(1200)                       -- jump to tick 1200
//! Replay.Seek(e.tick - 100, e.position)   -- 10 s before it, looking there
//! ```

use mlua::prelude::*;

use crate::engine::replay::Timeline;
use crate::lua_api::{lua_fn, shapes};

/// The playing recording's timeline (none: not a replay).
#[derive(Clone, Default)]
pub struct ReplayTimeline(pub Option<std::sync::Arc<Timeline>>);

/// A seek the interface asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct SeekRequest {
    pub tick: u64,
    /// Where the main camera looks once there.
    pub focus: Option<[f32; 3]>,
}

/// The watched replay's keyframes (`docs/architecture/replays.md`), as the renderer
/// last listed them: a seek starts from the latest at or before its target.
#[derive(Clone, Default)]
pub struct ReplayKeyframes {
    /// Ticks with a keyframe, ascending.
    pub ticks: std::sync::Arc<Vec<u64>>,
    /// Ticks between keyframes (the options').
    pub every: u64,
    /// The replay is being played ahead to write them (the options').
    pub precompute: bool,
}

impl ReplayKeyframes {
    /// Where a seek is quick, as `(from, to)` tick ranges: within a
    /// keyframe interval after the start or after a keyframe (the sim plays
    /// at most that far from where it starts), merged, up to `length`.
    pub fn buffered(&self, length: u64) -> Vec<(u64, u64)> {
        let every = self.every.max(1);
        let mut out: Vec<(u64, u64)> = Vec::new();
        for from in std::iter::once(0).chain(self.ticks.iter().copied()) {
            let to = (from + every).min(length.max(from));
            match out.last_mut() {
                Some(last) if from <= last.1 => last.1 = last.1.max(to),
                _ => out.push((from, to)),
            }
        }
        out
    }
}

/// The tick the renderer is seeking to (fast-forwarding; after a restart
/// when back), if it is.
#[derive(Clone, Copy, Default)]
pub struct Seeking(pub Option<u64>);

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(ReplayTimeline::default());
    lua.set_app_data(Seeking::default());
    lua.set_app_data(ReplayKeyframes::default());
    let replay = lua.create_table()?;
    lua_fn!(lua, table(replay, "Replay"),
        /// The recording's major moments in tick order: `{ tick, kind, army,
        /// blueprint, position }` each (`kind`: `'acu_death'`,
        /// `'experimental_built'`, `'nuke_launch'` or a mod's own; `army`
        /// 1-based). Empty outside a replay.
        GetEvents = |lua, ()| {
            let timeline = lua.app_data_ref::<ReplayTimeline>().and_then(|t| t.0.clone());
            let events = timeline.as_ref().map_or(&[][..], |t| t.events.as_slice());
            let out = lua.create_table_with_capacity(events.len(), 0)?;
            for e in events {
                let t = lua.create_table_with_capacity(0, 5)?;
                t.raw_set("tick", e.tick)?;
                t.raw_set("kind", e.kind.as_str())?;
                t.raw_set("army", e.army)?;
                t.raw_set("blueprint", e.blueprint.as_str())?;
                t.raw_set("position", lua.create_sequence_from(e.position)?)?;
                out.raw_push(t)?;
            }
            Ok(out)
        })?;
    lua_fn!(lua, table(replay, "Replay"),
        /// How long the recorded game ran, in ticks (0 outside a replay).
        GetLength = |lua, ()| Ok(lua.app_data_ref::<ReplayTimeline>().and_then(|t| t.0.as_ref().map(|t| t.length)).unwrap_or(0)))?;
    lua_fn!(lua, table(replay, "Replay"),
        /// Plays on to `tick` as fast as the machine can (an earlier tick
        /// restarts the replay first), then carries on at the speed it had;
        /// with `focus`, the main camera then looks there. Replays only.
        Seek = |lua, (tick, focus): (f64, Option<shapes::Vector>)| {
            if lua.app_data_ref::<ReplayTimeline>().is_none_or(|t| t.0.is_none()) {
                return Ok(());
            }
            let focus = focus.map(|v| -> LuaResult<[f32; 3]> { Ok([v.raw_get(1)?, v.raw_get(2)?, v.raw_get(3)?]) }).transpose()?;
            lua.set_app_data(SeekRequest { tick: tick.max(0.0) as u64, focus });
            Ok(())
        })?;
    lua_fn!(lua, table(replay, "Replay"),
        /// The ticks with a keyframe on disk, ascending: a seek starts from
        /// the latest at or before its target. Empty outside a replay.
        GetKeyframes = |lua, ()| {
            let ticks = lua.app_data_ref::<ReplayKeyframes>().map(|k| k.ticks.clone()).unwrap_or_default();
            lua.create_sequence_from(ticks.iter().copied())
        })?;
    lua_fn!(lua, table(replay, "Replay"),
        /// Where a seek is quick, like a video's buffered bar: `{ from, to }`
        /// tick ranges within a keyframe interval after the start or after a
        /// keyframe, ascending. Empty outside a replay.
        GetBuffered = |lua, ()| {
            let length = lua.app_data_ref::<ReplayTimeline>().and_then(|t| t.0.as_ref().map(|t| t.length)).unwrap_or(0);
            let ranges = lua.app_data_ref::<ReplayKeyframes>().map(|k| k.buffered(length)).unwrap_or_default();
            let out = lua.create_table_with_capacity(ranges.len(), 0)?;
            for (from, to) in ranges {
                let r = lua.create_table_with_capacity(0, 2)?;
                r.raw_set("from", from)?;
                r.raw_set("to", to)?;
                out.raw_push(r)?;
            }
            Ok(out)
        })?;
    lua_fn!(lua, table(replay, "Replay"),
        /// Whether the replay is being played ahead to write keyframes (the
        /// Options' "Replay: Play Ahead"), and the latest keyframe so far:
        /// `{ enabled, tick }`.
        GetPrecompute = |lua, ()| {
            let k = lua.app_data_ref::<ReplayKeyframes>().map(|k| (*k).clone()).unwrap_or_default();
            let out = lua.create_table_with_capacity(0, 2)?;
            out.raw_set("enabled", k.precompute)?;
            out.raw_set("tick", k.ticks.last().copied().unwrap_or(0))?;
            Ok(out)
        })?;
    lua_fn!(lua, table(replay, "Replay"),
        /// A seek is under way (`Seek`).
        IsSeeking = |lua, ()| Ok(lua.app_data_ref::<Seeking>().is_some_and(|s| s.0.is_some())))?;
    lua_fn!(lua, table(replay, "Replay"),
        /// The tick a seek under way goes to; nil when none is.
        GetSeekTarget = |lua, ()| Ok(lua.app_data_ref::<Seeking>().and_then(|s| s.0)))?;
    lua.globals().set("Replay", replay)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::replay::Event;

    #[test]
    fn events_length_and_seeks_through_the_table() {
        let lua = Lua::new();
        register(&lua).unwrap();
        let seek = |code: &str| {
            lua.load(code).exec().unwrap();
            lua.remove_app_data::<SeekRequest>()
        };
        assert_eq!(seek("Replay.Seek(10)"), None, "not a replay: no seeks");
        let e = Event { tick: 812, kind: "acu_death".into(), army: 3, blueprint: "uel0001".into(), position: [412.5, 20.0, 380.0] };
        lua.set_app_data(ReplayTimeline(Some(std::sync::Arc::new(Timeline { events: vec![e], length: 2000, marked: true }))));
        let (len, n, kind, x): (u64, usize, String, f32) =
            lua.load("local e = Replay.GetEvents() return Replay.GetLength(), table.getn(e), e[1].kind, e[1].position[1]").eval().unwrap();
        assert_eq!((len, n, kind.as_str(), x), (2000, 1, "acu_death", 412.5));
        assert_eq!(seek("Replay.Seek(712, {412.5, 20, 380})"), Some(SeekRequest { tick: 712, focus: Some([412.5, 20.0, 380.0]) }));
        assert_eq!(seek("Replay.Seek(-5)"), Some(SeekRequest { tick: 0, focus: None }));
        lua.set_app_data(Seeking(Some(712)));
        let (on, target): (bool, Option<u64>) = lua.load("return Replay.IsSeeking(), Replay.GetSeekTarget()").eval().unwrap();
        assert_eq!((on, target), (true, Some(712)));
    }
}
