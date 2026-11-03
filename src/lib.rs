//! Engine library: everything except the windowed front end (`main.rs`).
//! Split out so benchmarks (`benches/`) and future tools can link the engine.

pub mod audio;
pub mod debug_ui;
pub mod diagnostics;
pub mod engine;
pub mod install;
pub mod lua_api;
pub mod net;
pub mod render_pose;
pub mod test_data;
pub mod threads;
pub mod ui;
pub mod weapon_audit;
