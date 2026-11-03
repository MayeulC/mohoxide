//! Networked games without the interface, for tests and soak runs:
//! `--net-host N` hosts and launches once N players (itself included) are
//! connected to each other; `--net-join ADDR` joins and waits for the
//! launch. Both end with the same launch a lobby's `LaunchGame` gives
//! (`{Config, LocalID, Seed, Delay}`), so the session starts exactly as one
//! launched from FAF's lobby would.

use std::time::{Duration, Instant};

use super::core::{Event, NetCore};
use super::wire::Msg;
use crate::engine::session::SessionConfig;
use crate::engine::sim::lua_data::LuaData;

/// How long to wait for players to join (or for the host to launch).
const LOBBY_TIMEOUT: Duration = Duration::from_secs(300);

fn s(v: &str) -> LuaData {
    LuaData::string(v)
}

fn n(v: f64) -> LuaData {
    LuaData::number(v)
}

/// A table from fields (put in canonical order by a round trip through
/// the encoder's ordering: `LuaData` keeps fields sorted when built from
/// Lua, so sort the same way here).
fn table(mut fields: Vec<(LuaData, LuaData)>) -> LuaData {
    fields.sort_by(|a, b| crate::engine::sim::lua_data::key_order(&a.0, &b.0));
    LuaData::Table(fields)
}

fn launch_table(config: LuaData, local: u32, seed: u64, delay: u32) -> LuaData {
    table(vec![(s("Config"), config), (s("LocalID"), s(&local.to_string())), (s("Seed"), s(&seed.to_string())), (s("Delay"), n(delay as f64))])
}

/// Hosts until `players` clients (this one included) are all connected to
/// each other, then launches `session`'s map with them in the first start
/// spots and AI in the rest (`session.ai_armies` armies in all).
pub fn host(players: u32, port: u16, name: &str, session: &SessionConfig) -> Result<LuaData, String> {
    let net = NetCore::create(port, name, "")?;
    net.host();
    tracing::info!(players, port = net.port(), code = net.join_code().as_deref().unwrap_or("-"), "net harness: hosting, waiting for players (--net-join 127.0.0.1:<port> or <code>)");
    let started = Instant::now();
    let mut said = Instant::now();
    loop {
        net.wait(Duration::from_millis(100));
        let peers = net.peers();
        let ids: Vec<u32> = peers.iter().map(|p| p.id).collect();
        // Everyone connected to everyone (each says so).
        let meshed = peers.len() as u32 + 1 == players && peers.iter().all(|p| ids.iter().all(|&o| o == p.id || p.established.contains(&o)) && p.established.contains(&1));
        if meshed {
            break;
        }
        if started.elapsed() > LOBBY_TIMEOUT {
            return Err(format!("only {} of {players} players joined", peers.len() + 1));
        }
        if said.elapsed() > Duration::from_secs(5) {
            said = Instant::now();
            tracing::info!(joined = peers.len() + 1, players, "net harness: waiting for players");
        }
    }
    let mut peers = net.peers();
    peers.sort_by_key(|p| p.id);
    let mut owners: Vec<(u32, String)> = vec![(1, name.to_owned())];
    owners.extend(peers.iter().map(|p| (p.id, p.name.clone())));
    let armies = session.ai_armies.max(players);
    let teams = session.teams;
    let mut slots = Vec::new();
    for i in 1..=armies {
        let (owner, player_name, human) = match owners.get(i as usize - 1) {
            Some((id, nm)) => (*id, nm.clone(), true),
            None => (1, format!("AI {i}"), false),
        };
        let team = if teams > 1 { (i - 1) % teams + 2 } else { 1 };
        let k = if teams > 1 { (i - 1) / teams } else { i - 1 };
        slots.push((
            n(i as f64),
            table(vec![
                (s("StartSpot"), n(i as f64)),
                (s("Team"), n(team as f64)),
                (s("Faction"), n((k % 4 + 1) as f64)),
                (s("PlayerName"), s(&player_name)),
                (s("OwnerID"), s(&owner.to_string())),
                (s("Human"), LuaData::Bool(human)),
                (s("AIPersonality"), s(if human { "" } else { "adaptive" })),
                (s("ArmyColor"), n(i as f64)),
                (s("PlayerColor"), n(i as f64)),
                (s("Civilian"), LuaData::Bool(false)),
                (s("Ready"), LuaData::Bool(true)),
            ]),
        ));
    }
    // Dev: `RTS_NET_GAME_SPEED=adjustable|fast` (the lobby's Game Speed).
    let game_speed = std::env::var("RTS_NET_GAME_SPEED").unwrap_or_else(|_| "normal".into());
    let options = table(vec![
        (s("ScenarioFile"), s(&session.scenario)),
        (s("FogOfWar"), s("explored")),
        (s("NoRushOption"), s("Off")),
        (s("PrebuiltUnits"), s(if session.prebuilt { "On" } else { "Off" })),
        (s("Difficulty"), n(2.0)),
        (s("DoNotShareUnitCap"), LuaData::Bool(true)),
        (s("Timeouts"), s("-1")),
        (s("GameSpeed"), s(&game_speed)),
        (s("UnitCap"), s("1000")),
        (s("Victory"), s("sandbox")),
        (s("CheatsEnabled"), s("false")),
        (s("CivilianAlliance"), s("enemy")),
        (s("TeamShareOverflow"), s("enabled")),
    ]);
    let config = table(vec![(s("GameOptions"), options), (s("PlayerOptions"), table(slots)), (s("Observers"), LuaData::Table(Vec::new()))]);
    let peers = net.peers();
    let rtt = peers.iter().map(|p| p.ping_ms).fold(0.0, f64::max);
    let delay = super::lockstep::delay_for(super::lockstep::one_way_ms(&peers), 100, super::lockstep::margin_ms()) as u32;
    let seed = session.seed;
    net.mark_launched();
    net.broadcast(&Msg::Launch { config: config.clone(), seed, delay });
    tracing::info!(players, armies, seed, delay, rtt_ms = rtt, "net harness: launched");
    Ok(launch_table(config, 1, seed, delay))
}

/// Joins `addr` and waits for the host's launch.
pub fn join(addr: &str, port: u16, name: &str) -> Result<LuaData, String> {
    let net = NetCore::create(port, name, "")?;
    net.join(addr);
    tracing::info!(addr, port = net.port(), "net harness: joining");
    let started = Instant::now();
    loop {
        let mut events = net.wait(Duration::from_millis(100));
        while !events.is_empty() {
            match events.remove(0) {
                Event::JoinFailed { reason } => return Err(reason),
                Event::Welcomed { local_id, .. } => tracing::info!(local_id, "net harness: joined, waiting for the launch"),
                Event::Message { msg: Msg::Launch { config, seed, delay }, .. } => {
                    net.mark_launched();
                    // The first turns may have come with it: they're the game's.
                    net.unpoll(events);
                    let local = net.local_id().ok_or("launched before being welcomed")?;
                    tracing::info!(local, seed, delay, "net harness: launched by the host");
                    return Ok(launch_table(config, local, seed, delay));
                }
                Event::Message { msg: Msg::Eject { reason }, .. } => return Err(format!("ejected: {reason}")),
                _ => {}
            }
        }
        if started.elapsed() > LOBBY_TIMEOUT {
            return Err("the host never launched".into());
        }
    }
}
