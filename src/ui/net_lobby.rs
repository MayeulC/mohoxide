//! The interface's side of networked lobbies: the natives `lobby.lua`'s
//! `CLobby` (protocol `UDP` / `TCP`) and `CDiscoveryService` are built on.
//! They wrap `net::core` (the connection mesh) and `net::discovery` (LAN
//! beacons); `lobby.lua` turns the events into FAF's lobby callbacks.
//!
//! Peer ids are strings in Lua (`UILobbyPeerId`), numbers on the wire.

use std::sync::Arc;
use std::time::Instant;

use mlua::prelude::*;

use crate::engine::sim::lua_data::LuaData;
use crate::lua_api::shapes::{List, Shaped};
use crate::lua_api::lua_fn;
use crate::net::core::{Event, NetCore};
use crate::net::discovery::{BEACON_EVERY, Beacon, Found, Listener};
use crate::net::wire::Msg;

/// The ping grid (`__net_ping_grid`). The table also has `local`, our own
/// player id (a number; a Luau keyword, so it isn't listed here).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EnginePingGrid", write_only)]
struct PingGridData {
    players: Vec<PingGridPlayer>,
    /// `rows[from][to]`, indexed by player id (numbers; ids may leave holes).
    rows: Vec<Vec<PingGridEntry>>,
}

/// One player of the ping grid.
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EnginePingGridPlayer", write_only)]
struct PingGridPlayer {
    id: u32,
    name: String,
    /// The fastest speed step the player's machine keeps up with (in a game).
    max: Option<i32>,
    /// The player's input delay in ticks (in a game).
    delay: Option<u32>,
}

/// One measured link of the ping grid.
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EnginePingGridEntry", write_only)]
struct PingGridEntry {
    /// Round trip, milliseconds.
    ms: f64,
    /// 99th percentile round trip, milliseconds.
    p99: f64,
    /// `"udp"`, `"direct"` or `"relay"`.
    path: String,
}

/// How a map's download from FAF's vault is going (`__map_download`).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EngineMapDownload", write_only)]
struct MapDownloadData {
    /// `"downloading"`, `"done"` or `"failed"`.
    state: String,
    /// Bytes so far (downloading).
    received: Option<f64>,
    /// The zip's size in bytes, when the vault said (downloading).
    total: Option<f64>,
    /// Why it failed (failed).
    reason: Option<String>,
}

/// A connected peer (`__net_peers`).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EngineNetPeer", write_only)]
struct NetPeer {
    /// The peer id (a string).
    id: String,
    name: String,
    /// Milliseconds.
    ping: f64,
    /// Milliseconds since it was last heard from.
    quiet: f64,
    /// Always `"Established"`.
    status: String,
    #[lua(rename = "establishedPeers")]
    established_peers: Vec<String>,
}

/// What happened in the lobby (`__net_poll`); `kind` says which keys are set:
/// `"hosting"`, `"welcomed"` (`local`, `host`), `"join_failed"` (`reason`),
/// `"peer_connected"` (`id`, `name`), `"peer_disconnected"` (`id`, `name`,
/// `reason`), `"data"` (`from`, `name`, `data`), `"eject"` (`from`, `name`,
/// `reason`) or `"launch"` (`from`, `name`, `config`, `seed`, `delay`). Peer
/// ids are strings. (The table also has `local` on `"welcomed"`: our id.)
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EngineNetEvent", write_only)]
struct NetEvent {
    kind: String,
    host: Option<String>,
    reason: Option<String>,
    id: Option<String>,
    name: Option<String>,
    from: Option<String>,
    data: Option<LuaValue>,
    config: Option<LuaValue>,
    /// The launch seed, as a string (a u64 doesn't fit a Lua number).
    seed: Option<String>,
    delay: Option<u32>,
}

/// A client of the running session (`__net_session_clients`), as
/// `GetSessionClients` returns it. The table also has `local` (true for our
/// own client; a Luau keyword, so it isn't listed here).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "SessionClient", write_only)]
pub(crate) struct SessionClientData {
    name: String,
    /// The peer id (a string).
    uid: String,
    connected: bool,
    /// Milliseconds.
    ping: f64,
    /// Milliseconds since it was last heard from.
    quiet: f64,
    /// Session client indices (1-based) of the clients that voted to eject it.
    #[lua(rename = "ejectedBy")]
    ejected_by: Vec<u32>,
    /// Command sources (1-based) it may issue orders for.
    #[lua(rename = "authorizedCommandSources")]
    authorized_command_sources: Vec<u32>,
}

/// Chat sent to this client in a game without a network (single player,
/// watching a replay), for the next `__net_chat_poll`: moho delivers it to
/// the local client as to any other (FAF's notify overlay, the ACU upgrade
/// ETA, reaches its own player this way).
#[derive(Default)]
struct LocalChat(Vec<(String, LuaData)>);

/// `SessionSendChatMessage` without a network: a message for this client
/// (named, or everyone: `to` is nil) is kept for the next poll, once, from
/// the local client (`GetSessionClients`).
fn local_chat_send(lua: &Lua, to: Option<Vec<u32>>, data: &LuaValue) -> LuaResult<()> {
    let Some(clients) = lua.globals().raw_get::<Option<LuaFunction>>("GetSessionClients")?.map(|f| f.call::<LuaTable>(())).transpose()? else { return Ok(()) };
    let local = |i: u32| -> LuaResult<Option<String>> {
        let Some(c) = clients.raw_get::<Option<LuaTable>>(i)? else { return Ok(None) };
        Ok(if c.raw_get::<Option<bool>>("local")?.unwrap_or(false) { c.raw_get("name")? } else { None })
    };
    let indices: Vec<u32> = match to {
        Some(list) => list,
        None => (1..=clients.raw_len() as u32).collect(),
    };
    for i in indices {
        if let Some(name) = local(i)? {
            // (A copy, as the network sends: the sender may change its table.)
            let data = LuaData::from_lua(data);
            match lua.app_data_mut::<LocalChat>() {
                Some(mut q) => q.0.push((name, data)),
                None => drop(lua.set_app_data(LocalChat(vec![(name, data)]))),
            }
            break;
        }
    }
    Ok(())
}

/// A received chat message (`__net_chat_poll`).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EngineChatMessage", write_only)]
struct ChatMessage {
    /// The sender's name.
    sender: String,
    data: LuaValue,
}

/// A pause or resume (`__net_pause_poll`).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EnginePauseEvent", write_only)]
struct PauseEvent {
    paused: bool,
    /// The pauser's session client index (1-based; -1 when unknown).
    by: i64,
}

/// A desync (`__net_desync_poll`).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EngineDesyncEvent", write_only)]
struct DesyncEvent {
    /// The tick whose checksums differ (moho's beat).
    beat: u64,
    /// Our checksum, then each differing player's: `name: checksum`.
    strings: Vec<String>,
}

/// A change in the LAN games list (`__disc_poll`).
#[derive(crate::lua_api::LuaTable)]
#[allow(dead_code)]
#[lua(name = "EngineDiscoveryEvent", write_only)]
struct DiscoveryEvent {
    /// `"found"`, `"updated"` or `"removed"`.
    kind: String,
    index: u32,
    /// The game's beacon info, with its `Address` added (not on `"removed"`).
    info: Option<LuaTable>,
}

/// The LAN screen's listener and the host's beacon (user VM app data).
#[derive(Default)]
struct Discovery {
    listener: Option<Listener>,
    beacon: Option<(Beacon, Instant)>,
}

fn net() -> LuaResult<Arc<NetCore>> {
    crate::net::core::current().ok_or_else(|| LuaError::runtime("no network lobby"))
}

fn id_of(s: &str) -> LuaResult<u32> {
    s.parse().map_err(|_| LuaError::runtime(format!("bad peer id '{s}'")))
}

fn table_of(lua: &Lua, data: &LuaData) -> LuaResult<LuaValue> {
    data.to_lua(lua)
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    lua.set_app_data(Discovery::default());
    lua_fn!(lua, global,
        /// Opens the lobby's connection on `port` (0 = any) under `name`/`uid`. Returns true and the bound port, or false and nil when it can't.
        #[lua(internal)]
        __net_create = |_, (port, name, uid): (Option<u16>, String, Option<String>)| match NetCore::create(port.unwrap_or(0), &name, &uid.unwrap_or_default()) {
            Ok(net) => Ok((true, LuaValue::Number(net.port() as f64))),
            Err(e) => {
                tracing::warn!(e, "network: lobby not created");
                Ok((false, LuaValue::Nil))
            }
        })?;
    lua_fn!(lua, global,
        /// Starts hosting the lobby.
        #[lua(internal)]
        __net_host = |_, ()| net().map(|n| n.host()))?;
    lua_fn!(lua, global,
        /// Joins the lobby at `addr`.
        #[lua(internal)]
        __net_join = |_, addr: String| net().map(|n| n.join(&addr)))?;
    lua_fn!(lua, global,
        /// Our peer id (string), nil outside a networked lobby or before it is assigned.
        #[lua(internal)]
        __net_local_id = |_, ()| Ok(crate::net::core::current().and_then(|n| n.local_id()).map(|i| i.to_string())))?;
    lua_fn!(lua, global,
        /// The host's peer id (string), nil when unknown.
        #[lua(internal)]
        __net_host_id = |_, ()| Ok(crate::net::core::current().and_then(|n| n.host_id()).map(|i| i.to_string())))?;
    lua_fn!(lua, global,
        /// Why the game can't launch with the sim mods `mods` (FAF's
        /// `gameInfo.GameMods`: a set of uids, or a `ModInfo` list): one line
        /// per player missing a mod, with a different copy of one, or whose
        /// mods haven't arrived. Empty outside a networked lobby.
        #[lua(internal)]
        __net_mod_problems = |_, mods: Option<LuaTable>| {
            let Some(net) = crate::net::core::current() else { return Ok(Vec::new()) };
            let mut uids = Vec::new();
            let Some(mods) = mods else { return Ok(net.mod_problems(&uids)) };
            for pair in mods.pairs::<LuaValue, LuaValue>() {
                match pair? {
                    (LuaValue::String(uid), LuaValue::Boolean(true)) => uids.push(uid.to_str()?.to_string()),
                    (_, LuaValue::Table(info)) => uids.extend(info.get::<Option<String>>("uid")?),
                    _ => {}
                }
            }
            Ok(net.mod_problems(&uids))
        })?;
    lua_fn!(lua, global,
        /// Downloads the map of `scenario` (a scenario path) from FAF's vault
        /// unless that has started already (`net::map_download`), and says how
        /// it is going; nil when there's nothing to download (the map is
        /// there, or isn't a vault map). Asks the VFS, not `DiskGetFileInfo`:
        /// FAF's `userInit.lua` caches that, a missing file included, so asking
        /// it before the download would hide the downloaded map from the lobby.
        #[lua(internal)]
        __map_download = |lua, scenario: String| {
            use crate::net::map_download::{self, Status};
            let status = match map_download::status(&scenario) {
                Some(status) => Some(status),
                None => {
                    let Some(vfs) = lua.app_data_ref::<Arc<std::sync::RwLock<crate::engine::vfs::Vfs>>>().map(|v| v.clone()) else { return Ok(None) };
                    if vfs.read().unwrap().file_info(&scenario).is_some() {
                        return Ok(None);
                    }
                    map_download::start(&scenario, vfs)
                }
            };
            let Some(status) = status else { return Ok(None) };
            let out = lua.create_table_with_capacity(0, 3)?;
            match status {
                Status::Downloading { received, total } => {
                    out.set("state", "downloading")?;
                    out.set("received", received as f64)?;
                    out.set("total", total.map(|t| t as f64))?;
                }
                Status::Done => out.set("state", "done")?,
                Status::Failed(reason) => {
                    out.set("state", "failed")?;
                    out.set("reason", reason)?;
                }
            }
            Ok(Some(Shaped::<MapDownloadData>::new(out)))
        })?;
    lua_fn!(lua, global,
        /// The port the lobby's connection listens on, nil outside a networked lobby.
        #[lua(internal)]
        __net_port = |_, ()| Ok(crate::net::core::current().map(|n| n.port())))?;
    lua_fn!(lua, global,
        /// The ping grid's data (`ping_grid.lua`): nil outside a networked
        /// lobby or game; `{local, players = {{id, name, max}}, rows = {[from] =
        /// {[to] = {ms, p99, path}}}}` with ids as numbers; in a game `max` is
        /// the fastest speed step the player's machine keeps up with and `delay`
        /// its input delay (ticks).
        #[lua(internal)]
        __net_ping_grid = |lua, ()| {
            let grid = match (fake_players(), crate::net::core::current()) {
                (Some(n), _) => fake_grid(n),
                (None, Some(net)) => net.ping_grid(),
                (None, None) => return Ok(None),
            };
            let out = lua.create_table_with_capacity(0, 3)?;
            out.set("local", grid.local)?;
            let players = lua.create_table_with_capacity(grid.players.len(), 0)?;
            for (id, name) in &grid.players {
                let p = lua.create_table_with_capacity(0, 3)?;
                p.set("id", *id)?;
                p.set("name", name.as_str())?;
                // Moho speed step its machine keeps up with (once a game runs).
                if let Some(&percent) = grid.max_speed.get(id) {
                    p.set("max", crate::engine::snapshot::SimControl::step_of(percent))?;
                }
                // Input delay in ticks (once a game runs).
                if let Some(&ticks) = grid.input_delay.get(id) {
                    p.set("delay", ticks)?;
                }
                players.push(p)?;
            }
            out.set("players", players)?;
            let rows = lua.create_table()?;
            for (from, pings) in &grid.rows {
                let row = lua.create_table()?;
                for p in pings {
                    let e = lua.create_table_with_capacity(0, 3)?;
                    e.set("ms", p.rtt_us as f64 / 1000.0)?;
                    e.set("p99", p.rtt99_us as f64 / 1000.0)?;
                    e.set("path", match p.path {
                        crate::net::wire::PathKind::Udp => "udp",
                        crate::net::wire::PathKind::IrohDirect => "direct",
                        crate::net::wire::PathKind::IrohRelay => "relay",
                    })?;
                    row.set(p.peer, e)?;
                }
                rows.set(*from, row)?;
            }
            out.set("rows", rows)?;
            Ok(Some(Shaped::<PingGridData>::new(out)))
        })?;
    lua_fn!(lua, global,
        /// The code others use to join this lobby over the internet, nil when there is none.
        #[lua(internal)]
        __net_join_code = |_, ()| Ok(crate::net::core::current().and_then(|n| n.join_code())))?;
    lua_fn!(lua, global,
        /// Sends `data` to peer `to`.
        #[lua(internal)]
        __net_send = |_, (to, data): (String, LuaValue)| {
            net()?.send(id_of(&to)?, &Msg::Data(LuaData::from_lua(&data)));
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Sends `data` to every peer.
        #[lua(internal)]
        __net_broadcast = |_, data: LuaValue| {
            net()?.broadcast(&Msg::Data(LuaData::from_lua(&data)));
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Disconnects peer `id`, telling it `reason` when given.
        #[lua(internal)]
        __net_disconnect = |_, (id, reason): (String, Option<String>)| {
            net()?.disconnect(id_of(&id)?, reason.as_deref());
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// The connected peers: `{id, name, ping, quiet, status, establishedPeers}` each (ids as strings).
        #[lua(internal)]
        __net_peers = |lua, ()| {
            let peers = crate::net::core::current().map(|n| n.peers()).unwrap_or_default();
            let out = lua.create_table_with_capacity(peers.len(), 0)?;
            for p in peers {
                let t = lua.create_table_with_capacity(0, 6)?;
                t.set("id", p.id.to_string())?;
                t.set("name", p.name)?;
                // Milliseconds as moho's float (FAF floors it for display).
                t.set("ping", p.ping_ms)?;
                t.set("quiet", p.quiet_ms.round())?;
                t.set("status", "Established")?;
                let est = lua.create_table_with_capacity(p.established.len(), 0)?;
                for e in p.established {
                    est.push(e.to_string())?;
                }
                t.set("establishedPeers", est)?;
                out.push(t)?;
            }
            Ok(List::<Shaped<NetPeer>>::new(out))
        })?;
    lua_fn!(lua, global,
        /// Everything that happened, as tables for lobby.lua to dispatch.
        #[lua(internal)]
        __net_poll = |lua, ()| {
            let empty = || lua.create_table().map(List::<Shaped<NetEvent>>::new);
            let Some(net) = crate::net::core::current() else { return empty() };
            // After the launch the sim thread owns the events.
            if net.launched() {
                return empty();
            }
            let mut events = net.poll();
            // Everything after a launch is the game's (the first turns can
            // arrive with it): it stays queued for the sim thread.
            if let Some(i) = events.iter().position(|e| matches!(e, Event::Message { msg: Msg::Launch { .. }, .. })) {
                net.unpoll(events.split_off(i + 1));
            }
            let out = lua.create_table_with_capacity(events.len(), 0)?;
            let names: std::collections::BTreeMap<u32, String> = net.peers().into_iter().map(|p| (p.id, p.name)).collect();
            for e in events {
                let t = lua.create_table()?;
                match e {
                    Event::Hosting => t.set("kind", "hosting")?,
                    Event::Welcomed { local_id, host_id } => {
                        t.set("kind", "welcomed")?;
                        t.set("local", local_id.to_string())?;
                        t.set("host", host_id.to_string())?;
                    }
                    Event::JoinFailed { reason } => {
                        t.set("kind", "join_failed")?;
                        t.set("reason", reason)?;
                    }
                    Event::PeerConnected { id, name, .. } => {
                        t.set("kind", "peer_connected")?;
                        t.set("id", id.to_string())?;
                        t.set("name", name)?;
                    }
                    Event::PeerDisconnected { id, name, reason } => {
                        t.set("kind", "peer_disconnected")?;
                        t.set("id", id.to_string())?;
                        t.set("name", name)?;
                        t.set("reason", reason)?;
                    }
                    Event::Message { from, msg } => {
                        t.set("from", from.to_string())?;
                        t.set("name", names.get(&from).cloned().unwrap_or_default())?;
                        match msg {
                            Msg::Data(data) => {
                                t.set("kind", "data")?;
                                t.set("data", table_of(lua, &data)?)?;
                            }
                            Msg::Eject { reason } => {
                                t.set("kind", "eject")?;
                                t.set("reason", reason)?;
                            }
                            Msg::Launch { config, seed, delay } => {
                                net.mark_launched();
                                t.set("kind", "launch")?;
                                t.set("config", table_of(lua, &config)?)?;
                                // As a string: a u64 doesn't fit a Lua number.
                                t.set("seed", seed.to_string())?;
                                t.set("delay", delay)?;
                            }
                            _ => continue,
                        }
                    }
                }
                out.push(t)?;
            }
            Ok(List::new(out))
        })?;
    lua_fn!(lua, global,
        /// The host launches: everyone gets the configuration, a seed and the
        /// starting input delay. Returns the seed (string) and delay.
        #[lua(internal)]
        __net_launch = |_, config: LuaValue| {
            let net = net()?;
            let (seed, delay) = launch_params(&net);
            net.mark_launched();
            net.broadcast(&Msg::Launch { config: LuaData::from_lua(&config), seed, delay });
            tracing::info!(seed, delay, peers = net.peers().len(), "network: game launched");
            Ok((seed.to_string(), delay))
        })?;
    lua_fn!(lua, global,
        /// The lobby is gone: close the connection unless a game took it over.
        #[lua(internal)]
        __net_close = |lua, ()| {
            if let Some(mut d) = lua.app_data_mut::<Discovery>() {
                d.beacon = None;
            }
            if crate::net::core::current().is_some_and(|n| !n.launched()) {
                crate::net::core::close_current();
            }
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// A hosting lobby's beacon (at most once a second).
        #[lua(internal)]
        __net_beacon = |lua, info: LuaValue| {
            let Some(net) = crate::net::core::current() else { return Ok(()) };
            let Some(mut d) = lua.app_data_mut::<Discovery>() else { return Ok(()) };
            if d.beacon.as_ref().is_some_and(|(_, at)| at.elapsed() < BEACON_EVERY) {
                return Ok(());
            }
            if d.beacon.is_none() {
                match Beacon::new() {
                    Ok(b) => d.beacon = Some((b, Instant::now())),
                    Err(e) => {
                        tracing::warn!(error = %e, "discovery: no beacon socket");
                        return Ok(());
                    }
                }
            }
            let (beacon, at) = d.beacon.as_mut().expect("set");
            beacon.send(net.port(), &LuaData::from_lua(&info));
            *at = Instant::now();
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// In game: the session's clients (nil when not networked), in command
        /// source order: `GetSessionClients`.
        #[lua(internal)]
        __net_session_clients = |lua, ()| {
            let Some(net) = crate::net::core::current() else { return Ok(None) };
            let clients = net.session_clients();
            if clients.is_empty() {
                return Ok(None);
            }
            let pings: std::collections::BTreeMap<u32, f64> = net.peers().into_iter().map(|p| (p.id, p.ping_ms)).collect();
            // Session client index (1-based) of each peer, for `ejectedBy`.
            let index_of: std::collections::BTreeMap<u32, usize> = clients.iter().enumerate().map(|(i, c)| (c.peer, i + 1)).collect();
            let out = lua.create_table_with_capacity(clients.len(), 0)?;
            for c in clients {
                let t = lua.create_table_with_capacity(0, 9)?;
                t.set("quiet", net.quiet_ms(&c).round())?;
                t.set("ping", pings.get(&c.peer).copied().unwrap_or(0.0).round())?;
                let ejected = lua.create_table()?;
                for v in &c.ejected_by {
                    if let Some(i) = index_of.get(v) {
                        ejected.push(*i)?;
                    }
                }
                t.set("name", c.name)?;
                t.set("uid", c.peer.to_string())?;
                t.set("connected", c.connected)?;
                t.set("local", c.local)?;
                t.set("ejectedBy", ejected)?;
                t.set("authorizedCommandSources", lua.create_sequence_from([c.source + 1])?)?;
                out.push(t)?;
            }
            Ok(Some(List::<Shaped<SessionClientData>>::new(out)))
        })?;
    lua_fn!(lua, global,
        /// `SessionSendChatMessage`: to the given clients (1-based session
        /// client indices; nil = everyone), the local one included when named.
        #[lua(internal)]
        __net_chat_send = |lua, (to, data): (Option<Vec<u32>>, LuaValue)| {
            let Some(net) = crate::net::core::current() else { return local_chat_send(lua, to, &data) };
            let clients = net.session_clients();
            let data = LuaData::from_lua(&data);
            let targets: Vec<&crate::net::core::SessionClient> = match &to {
                Some(list) => list.iter().filter_map(|i| clients.get((*i as usize).wrapping_sub(1))).collect(),
                None => clients.iter().collect(),
            };
            for c in targets {
                if c.local {
                    net.push_chat(c.name.clone(), data.clone());
                } else if c.connected {
                    net.send(c.peer, &Msg::Chat(data.clone()));
                }
            }
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Pauses and resumes since the last call: `{paused, by}` with `by` the
        /// pauser's session client index (FAF's `gamemain.OnPause(pausedBy, ...)`).
        #[lua(internal)]
        __net_pause_poll = |lua, ()| {
            let Some(net) = crate::net::core::current() else { return lua.create_table().map(List::<Shaped<PauseEvent>>::new) };
            let events = net.take_pause_events();
            let clients = net.session_clients();
            let out = lua.create_table_with_capacity(events.len(), 0)?;
            for (paused, peer) in events {
                let t = lua.create_table_with_capacity(0, 2)?;
                t.set("paused", paused)?;
                t.set("by", clients.iter().position(|c| c.peer == peer).map_or(-1, |i| i as i64 + 1))?;
                out.push(t)?;
            }
            Ok(List::new(out))
        })?;
    lua_fn!(lua, global,
        /// Desyncs since the last call: `{beat, strings}` each, for FAF's
        /// `uimain.ShowDesyncDialog(beat, strings)`.
        #[lua(internal)]
        __net_desync_poll = |lua, ()| {
            let Some(net) = crate::net::core::current() else { return lua.create_table().map(List::<Shaped<DesyncEvent>>::new) };
            let events = net.take_desyncs();
            let out = lua.create_table_with_capacity(events.len(), 0)?;
            for (beat, lines) in events {
                let t = lua.create_table_with_capacity(0, 2)?;
                t.set("beat", beat)?;
                t.set("strings", lua.create_sequence_from(lines)?)?;
                out.push(t)?;
            }
            Ok(List::new(out))
        })?;
    lua_fn!(lua, global,
        /// `EjectSessionClient(index)`: our vote to eject session client `index`.
        #[lua(internal)]
        __net_eject = |_, index: usize| {
            let Some(net) = crate::net::core::current() else { return Ok(()) };
            if let Some(c) = net.session_clients().get(index.wrapping_sub(1)) {
                tracing::warn!(index, name = c.name, "network: voting to eject");
                net.vote_eject(c.peer);
            }
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// Chat messages received since the last call: `{sender, data}` each.
        #[lua(internal)]
        __net_chat_poll = |lua, ()| {
            let mut chat = crate::net::core::current().map(|n| n.take_chat()).unwrap_or_default();
            if let Some(mut local) = lua.app_data_mut::<LocalChat>() {
                chat.append(&mut local.0);
            }
            let out = lua.create_table_with_capacity(chat.len(), 0)?;
            for (from, data) in chat {
                let t = lua.create_table_with_capacity(0, 2)?;
                t.set("sender", from)?;
                t.set("data", data.to_lua(lua)?)?;
                out.push(t)?;
            }
            Ok(List::<Shaped<ChatMessage>>::new(out))
        })?;
    lua_fn!(lua, global,
        /// Starts listening for LAN game beacons. False when the socket can't be opened.
        #[lua(internal)]
        __disc_open = |lua, ()| {
            let Some(mut d) = lua.app_data_mut::<Discovery>() else { return Ok(false) };
            if d.listener.is_none() {
                match Listener::new() {
                    Ok(l) => d.listener = Some(l),
                    Err(e) => {
                        tracing::warn!(error = %e, "discovery: cannot listen for LAN games");
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        })?;
    lua_fn!(lua, global,
        /// Stops listening for LAN game beacons.
        #[lua(internal)]
        __disc_close = |lua, ()| {
            if let Some(mut d) = lua.app_data_mut::<Discovery>() {
                d.listener = None;
            }
            Ok(())
        })?;
    lua_fn!(lua, global,
        /// How many LAN games are known.
        #[lua(internal)]
        __disc_count = |lua, ()| Ok(lua.app_data_ref::<Discovery>().and_then(|d| d.listener.as_ref().map(|l| l.count())).unwrap_or(0)))?;
    lua_fn!(lua, global,
        /// LAN games found, updated or removed since the last poll (`{kind, index, info}`); with `reset` everything is reported again.
        #[lua(internal)]
        __disc_poll = |lua, reset: Option<bool>| {
            let found = {
                let Some(mut d) = lua.app_data_mut::<Discovery>() else { return lua.create_table().map(List::<Shaped<DiscoveryEvent>>::new) };
                let Some(l) = d.listener.as_mut() else { return lua.create_table().map(List::<Shaped<DiscoveryEvent>>::new) };
                if reset == Some(true) { l.reset() } else { l.poll() }
            };
            let out = lua.create_table_with_capacity(found.len(), 0)?;
            for f in found {
                let t = lua.create_table()?;
                let (kind, index, entry) = match f {
                    Found::New { index, address, info } => ("found", index, Some((address, info))),
                    Found::Updated { index, address, info } => ("updated", index, Some((address, info))),
                    Found::Removed { index } => ("removed", index, None),
                };
                t.set("kind", kind)?;
                t.set("index", index)?;
                if let Some((address, info)) = entry {
                    let info = table_of(lua, &info)?;
                    if let LuaValue::Table(i) = &info {
                        i.set("Address", address)?;
                    }
                    t.set("info", info)?;
                }
                out.push(t)?;
            }
            Ok(List::<Shaped<DiscoveryEvent>>::new(out))
        })?;
    Ok(())
}

/// The host's launch parameters: a fresh seed (any value; it only has to be
/// the same on every peer, which the launch message makes it) and the
/// starting input delay for the measured round trips at normal speed.
fn launch_params(net: &NetCore) -> (u64, u32) {
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let peers = net.peers();
    let delay = crate::net::lockstep::delay_for(crate::net::lockstep::one_way_ms(&peers), 100, crate::net::lockstep::margin_ms());
    (seed, delay as u32)
}

/// `RTS_PING_GRID_FAKE=<players>` (dev, 2..=64): the ping grid shows made up
/// players and pings instead of the network's, to see its layout at any size
/// without that many clients.
fn fake_players() -> Option<usize> {
    std::env::var("RTS_PING_GRID_FAKE").ok()?.trim().parse::<usize>().ok().map(|n| n.clamp(2, 64))
}

/// Made up players, with names of every length, and pings that wander a
/// little each second: mostly direct, some relayed, some LAN.
fn fake_grid(n: usize) -> crate::net::core::PingGrid {
    use crate::net::wire::{PathKind, PeerPing};
    const NAMES: [&str; 16] = ["Tom", "Fake_Player_With_Long_Name", "Ann", "MadeUpCommander", "Bob", "Test_Pilot", "Zed", "NotARealName", "Quantum_Gate", "Ed", "Sample_Sam", "Placeholder", "Lag_Machine", "Mo", "Dummy_Seraphim", "Xy"];
    let mix = |mut x: u64| {
        x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    };
    let second = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let players: Vec<(u32, String)> = (0..n)
        .map(|i| {
            let base = NAMES[i % NAMES.len()];
            let name = if i < NAMES.len() { base.to_string() } else { format!("{base}{}", i / NAMES.len() + 1) };
            (i as u32 + 1, name)
        })
        .collect();
    let mut rows = std::collections::BTreeMap::new();
    for a in 0..n as u64 {
        let pings = (0..n as u64)
            .filter(|&b| b != a)
            .map(|b| {
                // The pair's own distance (the same both ways), this
                // second's jitter, and a rare spike for the 99th.
                let (lo, hi) = (a.min(b), a.max(b));
                let pair = mix(lo * 131 + hi);
                let base = 8 + pair % 180;
                let jitter = mix(pair ^ second ^ (a << 32)) % 12;
                let spike = if mix(pair ^ 7) % 5 == 0 { 40 + mix(pair ^ 11) % 260 } else { mix(pair ^ 13) % 25 };
                let path = match pair % 7 {
                    0 => PathKind::IrohRelay,
                    1 | 2 => PathKind::Udp,
                    _ => PathKind::IrohDirect,
                };
                let ms = base + jitter;
                PeerPing { peer: b as u32 + 1, rtt_us: (ms * 1000) as u32, rtt99_us: ((ms + spike) * 1000) as u32, path }
            })
            .collect();
        rows.insert(a as u32 + 1, pings);
    }
    // Machines from 0.6x to 20x, one well below the rest (the limit).
    let max_speed = (1..=n as u32).map(|id| (id, if id == 3 { 60 } else { 250 + (mix(id as u64 ^ 17) % 1750) as u32 })).collect();
    // Mostly 2-3 ticks; one far away (a long delay).
    let input_delay = (1..=n as u32).map(|id| (id, if id == 5 { 6 } else { 2 + (mix(id as u64 ^ 23) % 2) as u32 })).collect();
    crate::net::core::PingGrid { players, local: 1, rows, max_speed, input_delay }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Two clients over loopback, idle after the join (keepalives only):
    /// each side's `GetPeers` entry for the other has a measured ping (not
    /// the link's 200 ms placeholder) and what FAF's `CalcConnectionStatus`
    /// needs for "Connected" (status 'Established', establishedPeers
    /// holding our id, both strings as `GetLocalPlayerID` returns).
    #[test]
    fn lobby_peers_show_measured_ping_and_connection_status() {
        let lua = Lua::new();
        register(&lua).unwrap();
        let (ok, port): (bool, u16) = lua.load("return __net_create(0, 'host', 'u1')").eval().unwrap();
        assert!(ok);
        lua.load("__net_host()").exec().unwrap();
        let joiner = NetCore::open(0, "joiner", "u2").unwrap();
        joiner.join(&format!("127.0.0.1:{port}"));
        let host = crate::net::core::current().unwrap();
        let start = Instant::now();
        while !(joiner.peers().iter().any(|p| p.established.contains(&2)) && host.peers().iter().any(|p| p.established.contains(&1))) {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out joining");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Idle: only keepalives (every 250 ms) cross now.
        std::thread::sleep(Duration::from_millis(700));
        let (ping, status, connection): (f64, String, i32) = lua
            .load(
                r#"
                local localId = __net_local_id()
                local peers = __net_peers()
                local peer = peers[1]
                assert(#peers == 1 and peer.id == '2' and peer.name == 'joiner', 'one peer, the joiner')
                -- FAF's CalcConnectionStatus (lobby.lua), side effects left out.
                local function has(list, value)
                    for _, v in list do
                        if v == value then return true end
                    end
                    return false
                end
                local function status(peer)
                    if peer.status ~= 'Established' then return 3 end
                    if not has(peer.establishedPeers, localId) then return 1 end
                    return 2
                end
                return peer.ping, peer.status, status(peer)
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!((status.as_str(), connection), ("Established", 2), "FAF shows the peer as connected");
        assert!(ping > 0.0 && ping < 50.0, "loopback ping measured, got {ping}");
        let back = joiner.peers()[0].ping_ms;
        assert!(back > 0.0 && back < 50.0, "the joiner measures the host too, got {back}");
        joiner.close();
        crate::net::core::close_current();
    }
}
