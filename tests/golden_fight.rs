//! Golden test: the `dev/scenes/fight.lua` skirmish (2 UEF Strikers vs 2 Cybran
//! Mantises) run for a fixed number of ticks must reproduce the recorded sim
//! checksum every tick, and the same end state.
//!
//! Any change to sim behaviour changes the golden; when that is intended,
//! regenerate it and review the diff:
//!     UPDATE_GOLDEN=1 cargo test --test golden_fight
//! Needs the FAF checkout (`../fa`) and game data; fails without them (`RTS_SKIP_DATA_TESTS=1` skips).
//! The same file must pass on x86 and ARM (lockstep determinism).

use std::fmt::Write;
use std::sync::{Arc, Mutex};

use rts_core_engine::debug_ui::DebugUiState;
use rts_core_engine::engine::{self, sim};

const INIT_LUA_PATH: &str = "../fa/setup/bin/init_local_development.lua";
const FA_PATH: &str = "../Supreme Commander Forged Alliance";

/// Fails without the game data (`RTS_SKIP_DATA_TESTS=1` skips instead).
fn have_data() -> bool {
    rts_core_engine::test_data::require_game("golden_fight")
}
const GOLDEN: &str = "tests/golden/fight.txt";
const TICKS: u32 = 250;

/// The plain fight run, shared per process: `fight_matches_golden` and the
/// GC-timing test compare against the very same run (`cargo test` runs both
/// here; nextest runs one test per process and each computes its own).
fn normal_run() -> &'static String {
    static NORMAL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NORMAL.get_or_init(|| run_with(false))
}

/// The fight's per-tick checksums; `collect_every_tick` forces a full Lua
/// garbage collection after each tick.
fn run_with(collect_every_tick: bool) -> String {
    let mut engine = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
    let session = engine::session::SessionConfig {
        script: Some("dev/scenes/fight.lua".into()),
        ticks: Some(0),
        ..Default::default()
    };
    engine.start(&session).expect("session");
    let mut out = String::new();
    writeln!(out, "# dev/scenes/fight.lua, seed 0, {TICKS} ticks. Regenerate: UPDATE_GOLDEN=1 cargo test --test golden_fight").unwrap();
    writeln!(out, "start {:016x}", engine.checksum()).unwrap();
    for tick in 1..=TICKS {
        engine.tick().expect("tick");
        if collect_every_tick {
            engine.sim_lua().gc_collect().expect("gc");
        }
        writeln!(out, "tick {tick:3} {:016x}", engine.checksum()).unwrap();
    }
    writeln!(out, "# live units at the end: id blueprint army health position").unwrap();
    let world = engine.sim_lua().app_data_ref::<sim::entity::EntityWorld>().expect("world");
    for e in world.iter().filter(|e| !e.destroyed && e.kind == sim::entity::EntityKind::Unit) {
        let (x, y, z) = e.position().to_f64();
        writeln!(out, "unit {} {} {:?} {:.3} ({x:.3}, {y:.3}, {z:.3})", e.id, e.cold.blueprint_id, e.army, e.health.to_num::<f64>()).unwrap();
    }
    out
}

#[test]
fn fight_matches_golden() {
    if !have_data() {
        return;
    }
    let actual = normal_run().as_str();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN).expect("golden missing: run with UPDATE_GOLDEN=1");
    if actual != expected {
        let first = actual.lines().zip(expected.lines()).position(|(a, e)| a != e);
        let line = first.map(|i| format!("first difference at line {}:\n  expected: {}\n  actual:   {}", i + 1, expected.lines().nth(i).unwrap_or(""), actual.lines().nth(i).unwrap_or("")));
        panic!("sim diverged from {GOLDEN}; {}", line.unwrap_or_else(|| "line counts differ".into()));
    }
}

/// When Lua's garbage collector runs must not change the sim. Its timing
/// depends on allocation, which in Luau depends on memory addresses (hashing
/// of table keys), so it differs between runs and machines: anything the sim
/// does through weak tables (FAF's TrashBags) must not lose live engine
/// objects. A run collecting after every tick has to match a normal one.
#[test]
fn garbage_collection_timing_does_not_change_the_sim() {
    if !have_data() {
        return;
    }
    // Both engines run at once (the plain one reuses `normal_run`'s run when
    // one already happened on this process): the compare takes as long as one run.
    let (normal, collected) = std::thread::scope(|s| {
        let normal = s.spawn(|| normal_run().clone());
        let collected = s.spawn(|| run_with(true));
        (normal.join().expect("normal run"), collected.join().expect("collected run"))
    });
    if normal != collected {
        let first = normal.lines().zip(collected.lines()).position(|(a, b)| a != b).unwrap_or(0);
        panic!("forced GC changed the sim at line {}:\n  normal:    {}\n  collected: {}", first + 1, normal.lines().nth(first).unwrap_or(""), collected.lines().nth(first).unwrap_or(""));
    }
}

/// The same for a normal AI game (two adaptive AIs building and fighting):
/// per-tick checksums with a full collection every tick equal a normal run.
#[test]
fn garbage_collection_timing_does_not_change_an_ai_game() {
    if !have_data() {
        return;
    }
    let run = |collect: bool| -> Vec<u64> {
        let mut engine = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
        engine.start(&engine::session::SessionConfig { ticks: Some(0), ..Default::default() }).expect("session");
        (0..400)
            .map(|_| {
                engine.tick().expect("tick");
                if collect {
                    engine.sim_lua().gc_collect().expect("gc");
                }
                engine.checksum()
            })
            .collect()
    };
    // Both engines run at once: the comparison takes as long as one game.
    let (normal, collected) = std::thread::scope(|s| {
        let normal = s.spawn(|| run(false));
        let collected = s.spawn(|| run(true));
        (normal.join().expect("normal game"), collected.join().expect("collected game"))
    });
    let first = normal.iter().zip(&collected).position(|(a, b)| a != b);
    assert!(first.is_none(), "forced GC changed the AI game from tick {}", first.unwrap_or(0) + 1);
}

/// Projectiles carry their blueprint's strategic-view icon (moho draws them
/// as a `StrategicIconSize` square zoomed out): the fight's shells have one,
/// and the icon is the blueprint's.
#[test]
fn projectiles_carry_their_strategic_icon() {
    if !have_data() {
        return;
    }
    let mut engine = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
    engine.start(&engine::session::SessionConfig { script: Some("dev/scenes/fight.lua".into()), ticks: Some(0), ..Default::default() }).expect("session");
    let mut checked = 0;
    for _ in 0..200 {
        engine.tick().expect("tick");
        let lua = engine.sim_lua();
        let world = lua.app_data_ref::<sim::entity::EntityWorld>().expect("world");
        let icons = lua.app_data_ref::<sim::projectile::ProjectileIcons>();
        for e in world.iter().filter(|e| !e.destroyed && e.kind == sim::entity::EntityKind::Projectile) {
            let want: f64 = lua
                .load(format!("local d = __blueprints['{}'].Display return d and d.StrategicIconSize or 0", e.cold.blueprint_id))
                .eval()
                .expect("blueprint");
            let icon = e.cold.strategic_icon.and_then(|i| icons.as_ref().and_then(|icons| icons.0.get(i as usize)).cloned());
            if want > 0.0 {
                assert_eq!(icon.map(|i| i.size), Some(want as f32), "{}", e.cold.blueprint_id);
                checked += 1;
            }
        }
        if checked > 0 {
            break;
        }
    }
    assert!(checked > 0, "the fight fired shells with an icon size");
}

/// Restarting (`RestartSession`, the replay timeline's backward seek) runs
/// the session again in a fresh engine on the same thread: nothing the first
/// left behind (thread-locals, statics, the archive cache) may change the
/// second. Two AI armies (they draw on the sim's random numbers every tick)
/// must checksum the same, tick for tick, both times.
#[test]
fn a_second_session_on_the_thread_plays_the_same() {
    if !have_data() {
        return;
    }
    let play = || {
        let mut engine = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
        engine.start(&engine::session::SessionConfig { seed: 7, ticks: Some(0), ..Default::default() }).expect("session");
        (0..300).map(|_| {
            engine.tick().expect("tick");
            engine.checksum()
        }).collect::<Vec<_>>()
    };
    let first = play();
    let second = play();
    let diverged = first.iter().zip(&second).position(|(a, b)| a != b);
    assert_eq!(diverged, None, "the second session diverged at tick {}", diverged.map_or(0, |t| t + 1));
}
