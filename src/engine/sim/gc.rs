//! When the sim VM collects garbage: at fixed ticks, never by allocation.
//!
//! Collection timing is visible to scripts (FAF's weak tables: TrashBags,
//! the reclaim grid; finalizers), so every peer must collect at the same
//! points. Luau triggers a collection when its heap has grown by so many
//! bytes, and byte counts are not the same across platforms: the same game
//! held 50 KB more on Windows x86 than on Mac ARM at tick 1000 (allocation
//! sizes and padding differ between compilers), so the machines collected
//! at different moments and a 4v4 split at tick 22006 (an AI engineer's
//! order existed on one machine only). The sim VM's automatic collector is
//! therefore off for good; it collects fully after each start-up stage
//! ([`collect`]) and every [`EVERY`] ticks at the end of the tick ([`tick`]),
//! the same on every machine.

use mlua::prelude::*;

/// Ticks between collections (10 s of game time). A full collection of a
/// 4v4's 150-200 MB heap takes 40-60 ms; Luau's byte trigger had been
/// collecting about every 25 s.
pub const EVERY: u64 = 100;

/// Turns the automatic collector off (from VM creation).
pub fn manual(lua: &Lua) {
    lua.gc_stop();
}

/// A full collection, leaving the automatic collector off (a collection
/// re-arms Luau's trigger). Through the keyframe raw call once the VM has it
/// (`heap_snapshot::prepare`): mlua's `gc_collect` makes two closures for
/// its protected call, garbage the collection itself can't take, so a
/// collection tick's heap wouldn't hold only live objects.
pub fn collect(lua: &Lua) -> LuaResult<()> {
    if crate::engine::lua::heap_snapshot::is_prepared(lua) {
        return crate::engine::lua::heap_snapshot::collect(lua);
    }
    lua.gc_collect()?;
    lua.gc_stop();
    Ok(())
}

/// Tick `now` ends with a collection.
pub fn due(now: u64) -> bool {
    now % EVERY == EVERY - 1
}

/// End of tick `now`.
pub fn tick(lua: &Lua, now: u64) -> LuaResult<()> {
    if due(now) {
        let _s = crate::prof_span!("lua gc");
        collect(lua)?;
    }
    Ok(())
}
