//! Temporary: measure the per-boot fixed cost.

use std::sync::{Arc, Mutex};

use rts_core_engine::debug_ui::DebugUiState;
use rts_core_engine::engine;

const INIT_LUA_PATH: &str = "../fa/setup/bin/init_local_development.lua";
const FA_PATH: &str = "../Supreme Commander Forged Alliance";

#[test]
fn boot_profile() {
    let _ = tracing_subscriber::fmt().with_writer(std::io::stderr).with_max_level(tracing::Level::INFO).try_init();
    if !rts_core_engine::test_data::require_game("boot profile") {
        return;
    }
    let n: usize = std::env::var("E1_BOOTS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    for i in 0..n {
        eprintln!("E1PROF ===== boot {i} =====");
        let t = std::time::Instant::now();
        let mut e = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
        let new_t = t.elapsed();
        let session = engine::session::SessionConfig {
            script: None,
            ticks: Some(0),
            create_initial_units: false,
            ai: false,
            ..Default::default()
        };
        e.start(&session).expect("session");
        eprintln!("E1PROF ===== boot {i}: new={new_t:?} start={:?} total={:?}", t.elapsed(), t.elapsed());
    }
}
