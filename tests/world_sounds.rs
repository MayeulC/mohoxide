//! World sounds leave the sim through snapshots only: the `dev/scenes/fight.lua`
//! skirmish's weapon, impact and death cues arrive positioned, and a run
//! without a renderer keeps none (nothing piles up, nothing reaches the sim).
//! Needs the FAF checkout (`../fa`) and game data; fails without them (`RTS_SKIP_DATA_TESTS=1` skips).

use std::sync::{Arc, Mutex};

use rts_core_engine::debug_ui::DebugUiState;
use rts_core_engine::engine::{self, sim};

const INIT_LUA_PATH: &str = "../fa/setup/bin/init_local_development.lua";
const FA_PATH: &str = "../Supreme Commander Forged Alliance";

/// Fails without the game data (`RTS_SKIP_DATA_TESTS=1` skips instead).
fn have_data() -> bool {
    rts_core_engine::test_data::require_game("world_sounds")
}

fn fight() -> engine::Engine {
    let mut engine = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
    let session = engine::session::SessionConfig { script: Some("dev/scenes/fight.lua".into()), ticks: Some(0), ..Default::default() };
    engine.start(&session).expect("session");
    engine
}

#[test]
fn fight_sounds_reach_snapshots() {
    if !have_data() {
        return;
    }
    let mut engine = fight();
    let (sink, rx, _request) = engine::snapshot::channel();
    engine.set_snapshot_sink(sink);
    let mut sounds = Vec::new();
    for _ in 0..150 {
        engine.tick().expect("tick");
        while let Ok(mut snap) = rx.try_recv() {
            drop(snap.ticket.take());
            sounds.append(&mut snap.sounds);
        }
    }
    let banks: std::collections::BTreeSet<&str> = sounds.iter().map(|s| &*s.bank).collect();
    assert!(banks.contains("UELWeapon") && banks.contains("URLWeapon"), "{banks:?}");
    assert!(sounds.iter().all(|s| s.position.is_some_and(|p| p[0] > 0.0 && p[2] > 0.0)), "world sounds are positioned");
    let presentation = engine.sim_lua().app_data_ref::<sim::presentation::Presentation>().expect("presentation");
    assert!(presentation.sounds.is_empty(), "delivered sounds are drained");
}

#[test]
fn headless_runs_keep_no_sounds() {
    if !have_data() {
        return;
    }
    let mut engine = fight();
    for _ in 0..100 {
        engine.tick().expect("tick");
    }
    let presentation = engine.sim_lua().app_data_ref::<sim::presentation::Presentation>().expect("presentation");
    assert!(presentation.sounds.is_empty());
}

/// The positioned sounds of the fight's first 150 ticks as `listener` hears
/// them, each with whether `listener` saw its spot when it was published.
fn heard_by(listener: Option<u32>) -> Vec<(rts_core_engine::audio::WorldSound, bool)> {
    use sim::math::{Fx, FxVec3};
    let mut engine = fight();
    let (sink, rx, request) = engine::snapshot::channel();
    request.lock().unwrap().view_army = listener;
    engine.set_snapshot_sink(sink);
    let mut heard = Vec::new();
    for _ in 0..150 {
        engine.tick().expect("tick");
        while let Ok(mut snap) = rx.try_recv() {
            drop(snap.ticket.take());
            let grids = engine.sim_lua().app_data_ref::<sim::vision::IntelGrids>().expect("intel grids");
            for s in snap.sounds.drain(..) {
                let p = s.position.expect("positioned");
                let seen = listener.is_some_and(|a| grids.sees_point(a, FxVec3 { x: Fx::from_num(p[0]), y: Fx::from_num(p[1]), z: Fx::from_num(p[2]) }, false));
                heard.push((s, seen));
            }
        }
    }
    heard
}

/// Moho's `snd_CheckLOS`: with fog of war on (the fight's `FogOfWar` is
/// 'explored'), the listener hears only what its side sees; an army with
/// nothing on the map hears nothing; observers hear everything.
#[test]
fn unseen_sounds_are_dropped() {
    if !have_data() {
        return;
    }
    let everything = heard_by(None);
    assert!(!everything.is_empty());
    let fighter = heard_by(Some(1));
    assert!(!fighter.is_empty(), "army 1 sees its own fight");
    assert!(fighter.iter().all(|(_, seen)| *seen), "army 1 hears only what it sees");
    assert!(fighter.len() <= everything.len());
    // The map has more start spots than the fight's two armies: army 3
    // has no units, so no vision.
    assert!(heard_by(Some(3)).is_empty(), "an army that sees nothing hears nothing");
}
