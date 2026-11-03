//! A game launched from FAF's lobby (`CLobby:LaunchGame`): the sim takes the
//! lobby's options and players, gives each player the army of their slot's
//! start spot (numbered in the scenario's army order, free spots left out) and
//! makes the local player's army the one this client commands. Needs the FAF
//! checkout (`../fa`) and game data; fails without them (`RTS_SKIP_DATA_TESTS=1` skips).

use std::sync::{Arc, Mutex};

use mlua::prelude::*;
use rts_core_engine::debug_ui::DebugUiState;
use rts_core_engine::engine;
use rts_core_engine::engine::sim::lua_data::LuaData;

const INIT_LUA_PATH: &str = "../fa/setup/bin/init_local_development.lua";
const FA_PATH: &str = "../Supreme Commander Forged Alliance";

/// What FAF's lobby hands `LaunchGame` (flattened `gameInfo`), trimmed to
/// the fields the launch reads: the local player in slot 3, an AI in slot 1.
/// Both keep the default `StartSpot = 1`, as the retail lobby and LOUD's do:
/// the slot is what places them (the exe never reads `StartSpot`).
const LAUNCH: &str = r#"
return {
    LocalID = '0',
    Config = {
        GameOptions = {
            ScenarioFile = '/maps/scmp_009/scmp_009_scenario.lua',
            Victory = 'sandbox', UnitCap = '750', CheatsEnabled = 'false', FogOfWar = 'explored',
            PrebuiltUnits = 'Off', NoRushOption = 'Off', Timeouts = '3', GameSpeed = 'normal', CivilianAlliance = 'enemy',
        },
        PlayerOptions = {
            [3] = { StartSpot = 1, Human = true, OwnerID = '0', PlayerName = 'Me', Faction = 2, Team = 1, PlayerColor = 1, ArmyColor = 1, AIPersonality = '' },
            [1] = { StartSpot = 1, Human = false, OwnerID = '0', PlayerName = 'Bot', Faction = 3, Team = 1, PlayerColor = 2, ArmyColor = 2, AIPersonality = 'adaptive' },
        },
        Observers = {},
        GameMods = {},
    },
}
"#;

#[test]
fn lobby_players_get_their_slots_armies() {
    if !rts_core_engine::test_data::require_game("lobby_launch") {
        return;
    }
    let launch = LuaData::from_lua(&Lua::new().load(LAUNCH).eval::<LuaValue>().expect("launch table"));
    let mut engine = engine::Engine::new(INIT_LUA_PATH, FA_PATH, Arc::new(Mutex::new(DebugUiState::new()))).expect("engine");
    engine.start(&engine::session::SessionConfig { ticks: Some(0), launch: Some(launch), ..Default::default() }).expect("session");

    let lua = engine.sim_lua();
    // Each army as "name index human faction player".
    let (options, armies): (LuaTable, Vec<String>) = lua
        .load(
            r#"
            local list = {}
            for name, setup in ScenarioInfo.ArmySetup do
                list[setup.ArmyIndex] = string.format('%s %d %s %d %s', name, setup.ArmyIndex, tostring(setup.Human), setup.Faction, setup.PlayerName)
            end
            return ScenarioInfo.Options, list
            "#,
        )
        .eval()
        .expect("scenario info");
    assert_eq!(options.get::<String>("Victory").unwrap(), "sandbox", "the lobby's options are the game's");
    assert_eq!(options.get::<String>("UnitCap").unwrap(), "750");
    let players: Vec<_> = armies.iter().filter(|a| !a.ends_with(" civilian")).cloned().collect();
    assert_eq!(
        players,
        ["ARMY_1 1 false 3 Bot", "ARMY_3 2 true 2 Me"],
        "spot 1's army first, spot 3's next; spot 2 has no army"
    );
    // This client commands the army its lobby player took.
    let source = engine::api::command_source::local_source(lua);
    let sim_armies = lua.app_data_ref::<engine::sim::army::Armies>().expect("armies");
    let local = sim_armies.list.iter().find(|a| a.command_sources.contains(&source)).map(|a| a.name.clone());
    assert_eq!(local.as_deref(), Some("ARMY_3"));
}
