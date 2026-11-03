//! Player commands: the only way input reaches the sim.
//!
//! The renderer (or a network peer, or a replay) never touches sim state.
//! It sends [`PlayerCommand`]s, which hold only sim values (entity ids,
//! fixed-point positions, whole degrees), so every peer decodes the same bits.
//! The [`CommandStream`] stamps each one with the tick it runs on (`now +
//! delay`, moho's lockstep input lag), and at the start of that tick the sim
//! applies everything due in `(tick, client, seq)` order. In multiplayer, a tick
//! may only run once every client's commands for it have arrived. The single
//! local client used here is the trivial case.
//!
//! Commands are also the replay format: [`encode`] / [`decode`] give one text
//! line per command, so a recorded game replays bit-identically headless.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, Sender};

use crate::engine::sim::command::{CommandId, CommandKind, CommandTarget};
use crate::engine::sim::entity::EntityId;
use crate::engine::sim::lua_data::LuaData;
use crate::engine::sim::math::{Fx, FxVec3};

/// One order from a player to some of its army's units.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlayerCommand {
    /// Army (1-based) giving the order; units of other armies are ignored.
    pub army: u32,
    /// `Stop` clears the units' queues and issues nothing.
    pub kind: CommandKind,
    pub units: Vec<EntityId>,
    pub target: CommandTarget,
    /// Shift-queued: append instead of replacing the units' orders.
    pub queue: bool,
    /// Formation for move-like orders to the ground (`formations.lua`
    /// function name); `None` = every unit heads for the point itself.
    pub formation: Option<String>,
    /// Final facing of a formation, whole degrees (0 = south, 90 = east);
    /// `None` faces the way they travel.
    pub facing: Option<i32>,
    /// Builds and upgrades: the blueprint id.
    pub blueprint: Option<String>,
    /// Factory builds: how many.
    pub count: u32,
    /// `TransportUnloadSpecificUnits`: the category expression of the cargo
    /// to let off (`None`: everything, like a plain unload — the interface
    /// has no cargo picker yet, so that is all it sends).
    pub category: Option<String>,
    /// A unit setting instead of an order (the order panel's toggles).
    pub toggle: Option<Toggle>,
    /// Script orders (`UNITCOMMAND_Script`, e.g. `TaskName = EnhanceTask`,
    /// `Enhancement = ...`): the order table's string fields, in key order.
    pub script: Vec<(String, String)>,
    /// A `SimCallback` from the interface instead of an order: `units` is
    /// the sender's selection (when it asked for it), and nothing else is
    /// read.
    pub callback: Option<SimCallback>,
    /// Moves the queued command with this id to `target` instead of
    /// ordering anything (moho's `SetCommandTarget` / `SetCommandCells`:
    /// a waypoint or build site dragged in the command graph); nothing else
    /// is read.
    pub retarget: Option<CommandId>,
}

/// `SimCallback{Func = ..., Args = ...}`: runs `DoCallback(Func, Args,
/// units)` from FAF's `/lua/SimCallbacks.lua` in the sim.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SimCallback {
    pub func: String,
    pub args: LuaData,
    /// Pass the selection (`units`) on; else `DoCallback` gets nil.
    pub with_selection: bool,
}

impl SimCallback {
    /// The replay field: hex of `with_selection`, `func`, `args`.
    fn encode(&self) -> String {
        let mut bytes = vec![self.with_selection as u8];
        LuaData::string(&self.func).encode(&mut bytes);
        self.args.encode(&mut bytes);
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn decode(hex: &str) -> Result<Self, String> {
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()).ok_or_else(|| format!("bad callback hex `{hex}`")))
            .collect::<Result<Vec<u8>, _>>()?;
        let (&flag, mut rest) = bytes.split_first().ok_or("empty callback")?;
        let LuaData::String(func) = LuaData::decode(&mut rest)? else { return Err("callback without a function name".into()) };
        let args = LuaData::decode(&mut rest)?;
        Ok(SimCallback { func: String::from_utf8_lossy(&func).into_owned(), args, with_selection: flag != 0 })
    }
}

/// A setting a player changes on units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Toggle {
    /// 0 return fire, 1 hold fire, 2 hold ground.
    FireState(i32),
    Paused(bool),
    /// `RULEUTC_*` toggle bit and its new state.
    ScriptBit(u8, bool),
    /// Factories re-queue what they finish (`ProcessInfo('SetRepeatQueue')`).
    RepeatQueue(bool),
    /// The interface found no free (`true`: all busy) air staging for a
    /// dock order: the army's brain plays the VO.
    StagingVo(bool),
    /// `IncreaseBuildCountInQueue` / `DecreaseBuildCountInQueue`: the
    /// factory queue entry (1-based) and how many more (or fewer).
    BuildCount(u16, i32),
    /// Auto construction / assist (`SetAutoMode`: silos build missiles
    /// unordered).
    AutoMode(bool),
    /// Submarines surface on their own (`SetAutoSurfaceMode`).
    AutoSurfaceMode(bool),
    /// Not a unit setting: the sender asks for game speed `step` (moho's
    /// `SimControl::percent_of`, 0 normal). It travels and applies like an
    /// order, so every client changes speed at the same tick, in the same
    /// order (`net::lockstep::apply_game_speed`).
    GameSpeed(i32),
}

/// A command and when it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Scheduled {
    pub tick: u64,
    /// The command source that sent it (0-based: human armies' players in
    /// army order, then observers); `GetCurrentCommandSource` (1-based)
    /// while it applies.
    pub client: u32,
    /// Per-client sequence number (keeps a client's same-tick order).
    pub seq: u32,
    pub command: PlayerCommand,
}

/// Ticks between a command being sent and it running (lockstep input lag:
/// time for it to reach every peer).
pub const INPUT_DELAY: u64 = 1;

/// Commands waiting for their tick (app data).
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct CommandStream {
    pending: BTreeMap<(u64, u32, u32), PlayerCommand>,
}

impl CommandStream {
    pub fn schedule(&mut self, s: Scheduled) {
        self.pending.insert((s.tick, s.client, s.seq), s.command);
    }

    /// Everything due by `tick`, in `(tick, client, seq)` order. Late
    /// commands (stamped for a tick already run, e.g. a replay starting
    /// mid-game) run now rather than never.
    pub fn take_due(&mut self, tick: u64) -> Vec<Scheduled> {
        let later = self.pending.split_off(&(tick + 1, 0, 0));
        let due = std::mem::replace(&mut self.pending, later);
        due.into_iter().map(|((tick, client, seq), command)| Scheduled { tick, client, seq, command }).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// The sim's end of the local client's input channel (app data).
pub struct InputLink {
    rx: Receiver<PlayerCommand>,
    next_seq: u32,
}

impl InputLink {
    /// Everything received since the last call (a networked game stamps
    /// them itself: `net::lockstep`).
    pub fn drain(&mut self) -> Vec<PlayerCommand> {
        self.rx.try_iter().collect()
    }

    /// Stamps everything received since the last tick for `now + delay`, as
    /// sent by command source `client`, into `stream`: those `keep` passes
    /// (the rest are dropped).
    pub fn drain_into(&mut self, stream: &mut CommandStream, now: u64, client: u32, keep: impl Fn(&PlayerCommand) -> bool) {
        while let Ok(command) = self.rx.try_recv() {
            if !keep(&command) {
                continue;
            }
            stream.schedule(Scheduled { tick: now + INPUT_DELAY, client, seq: self.next_seq, command });
            self.next_seq = self.next_seq.wrapping_add(1);
        }
    }
}

/// A connected (sender for the renderer, link for the sim) pair.
pub fn channel() -> (Sender<PlayerCommand>, InputLink) {
    let (tx, rx) = std::sync::mpsc::channel();
    (tx, InputLink { rx, next_seq: 0 })
}

/// Records applied commands as replay lines (`--record`).
pub struct Recorder(pub std::io::BufWriter<std::fs::File>);

/// One replay line: `tick client seq army kind queue formation facing target
/// units blueprint count toggle script callback category retarget` (`-` for none;
/// positions as raw fixed-point bits, so they replay exactly; a callback as
/// hex bytes; a category expression percent-encoded, as it may hold spaces).
/// Lines without the trailing fields (older recordings) still read.
pub fn encode(s: &Scheduled) -> String {
    let c = &s.command;
    let target = match c.target {
        CommandTarget::None => "-".to_string(),
        CommandTarget::Entity(id) => format!("e{id}"),
        CommandTarget::Ground(p) => format!("g{},{},{}", p.x.to_bits(), p.y.to_bits(), p.z.to_bits()),
    };
    let units: Vec<String> = c.units.iter().map(|u| u.to_string()).collect();
    format!(
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        s.tick,
        s.client,
        s.seq,
        c.army,
        c.kind as u8,
        c.queue as u8,
        encode_text(c.formation.as_deref()),
        c.facing.map_or("-".to_string(), |f| f.to_string()),
        target,
        if units.is_empty() { "-".to_string() } else { units.join(",") },
        encode_text(c.blueprint.as_deref()),
        c.count,
        match c.toggle {
            None => "-".to_string(),
            Some(Toggle::FireState(s)) => format!("f{s}"),
            Some(Toggle::Paused(p)) => format!("p{}", p as u8),
            Some(Toggle::ScriptBit(b, on)) => format!("s{b}:{}", on as u8),
            Some(Toggle::RepeatQueue(on)) => format!("r{}", on as u8),
            Some(Toggle::StagingVo(busy)) => format!("v{}", busy as u8),
            Some(Toggle::BuildCount(i, d)) => format!("q{i}:{d}"),
            Some(Toggle::AutoMode(on)) => format!("a{}", on as u8),
            Some(Toggle::AutoSurfaceMode(on)) => format!("u{}", on as u8),
            Some(Toggle::GameSpeed(step)) => format!("w{step}"),
        },
        if c.script.is_empty() { "-".to_string() } else { c.script.iter().map(|(k, v)| format!("{}={}", escape(k), escape(v))).collect::<Vec<_>>().join(";") },
        c.callback.as_ref().map_or("-".to_string(), SimCallback::encode),
        encode_text(c.category.as_deref()),
        c.retarget.map_or("-".to_string(), |id| id.to_string()),
    )
}

/// A text field (formation, blueprint, category expression): `-` for
/// none, `%` for empty, else [`escape`]d.
fn encode_text(text: Option<&str>) -> String {
    match text {
        None => "-".to_string(),
        Some("") => "%".to_string(),
        Some("-") => "%2D".to_string(),
        Some(t) => escape(t),
    }
}

/// `t` with whitespace, `%`, `;` and `=` as `%XX` per UTF-8 byte: replay
/// lines split on whitespace, script pairs on `;` and `=`, and every client
/// must read back exactly what the sender meant.
fn escape(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        if c.is_whitespace() || c.is_control() || matches!(c, '%' | ';' | '=') {
            let mut bytes = [0; 4];
            for b in c.encode_utf8(&mut bytes).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn decode_text(field: Option<&str>) -> Result<Option<String>, String> {
    match field {
        None | Some("-") => Ok(None),
        Some(e) => unescape(e).map(Some),
    }
}

fn unescape(e: &str) -> Result<String, String> {
    if e == "%" {
        return Ok(String::new());
    }
    let mut out = Vec::with_capacity(e.len());
    let mut bytes = e.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let hex: Vec<u8> = bytes.by_ref().take(2).collect();
            let hex = String::from_utf8_lossy(&hex);
            out.push(u8::from_str_radix(&hex, 16).map_err(|_| format!("bad escape `%{hex}`"))?);
        } else {
            out.push(b);
        }
    }
    String::from_utf8(out).map_err(|e| format!("bad text: {e}"))
}

/// Parses an [`encode`]d line.
pub fn decode(line: &str) -> Result<Scheduled, String> {
    let f: Vec<&str> = line.split_whitespace().collect();
    let (fields, extra) = match f.len() {
        10 => (&f[..10], None),
        12..=17 => (&f[..10], Some((f[10], f[11]))),
        n => return Err(format!("expected 10 or 12 to 17 fields, got {n}: `{line}`")),
    };
    let [toggle, script, callback, category, retarget] = std::array::from_fn(|i| f.get(12 + i).copied());
    let callback = match callback {
        None | Some("-") => None,
        Some(hex) => Some(SimCallback::decode(hex)?),
    };
    let script = match script {
        None | Some("-") => Vec::new(),
        Some(s) => s.split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| Ok((unescape(k)?, unescape(v)?))).collect::<Result<_, String>>()?,
    };
    let toggle = match toggle {
        None | Some("-") => None,
        Some(t) => Some(match (t.as_bytes().first(), &t[1..]) {
            (Some(b'f'), v) => Toggle::FireState(v.parse().map_err(|e| format!("`{t}`: {e}"))?),
            (Some(b'p'), v) => Toggle::Paused(v == "1"),
            (Some(b'r'), v) => Toggle::RepeatQueue(v == "1"),
            (Some(b'v'), v) => Toggle::StagingVo(v == "1"),
            (Some(b'a'), v) => Toggle::AutoMode(v == "1"),
            (Some(b'u'), v) => Toggle::AutoSurfaceMode(v == "1"),
            (Some(b'w'), v) => Toggle::GameSpeed(v.parse().map_err(|e| format!("`{t}`: {e}"))?),
            (Some(b'q'), v) => {
                let (i, d) = v.split_once(':').ok_or_else(|| format!("bad toggle `{t}`"))?;
                Toggle::BuildCount(i.parse().map_err(|e| format!("`{t}`: {e}"))?, d.parse().map_err(|e| format!("`{t}`: {e}"))?)
            }
            (Some(b's'), v) => {
                let (b, on) = v.split_once(':').ok_or_else(|| format!("bad toggle `{t}`"))?;
                Toggle::ScriptBit(b.parse().map_err(|e| format!("`{t}`: {e}"))?, on == "1")
            }
            _ => return Err(format!("bad toggle `{t}`")),
        }),
    };
    let [tick, client, seq, army, kind, queue, formation, facing, target, units] = fields[..] else { unreachable!() };
    let num = |s: &str| s.parse::<i64>().map_err(|e| format!("`{s}`: {e}"));
    let kind = CommandKind::from_u8(num(kind)? as u8).ok_or_else(|| format!("unknown command kind {kind}"))?;
    let target = match target {
        "-" => CommandTarget::None,
        t if t.starts_with('e') => CommandTarget::Entity(num(&t[1..])? as EntityId),
        t if t.starts_with('g') => {
            let v: Vec<i64> = t[1..].split(',').map(num).collect::<Result<_, _>>()?;
            let [x, y, z] = v[..] else { return Err(format!("bad position `{t}`")) };
            CommandTarget::Ground(FxVec3 { x: Fx::from_bits(x), y: Fx::from_bits(y), z: Fx::from_bits(z) })
        }
        t => return Err(format!("bad target `{t}`")),
    };
    let units = match units {
        "-" => Vec::new(),
        u => u.split(',').map(|x| num(x).map(|v| v as EntityId)).collect::<Result<_, _>>()?,
    };
    Ok(Scheduled {
        tick: num(tick)? as u64,
        client: num(client)? as u32,
        seq: num(seq)? as u32,
        command: PlayerCommand {
            army: num(army)? as u32,
            kind,
            units,
            target,
            queue: queue == "1",
            formation: decode_text(Some(formation))?,
            facing: if facing == "-" { None } else { Some(num(facing)? as i32) },
            blueprint: decode_text(extra.map(|(b, _)| b))?,
            count: match extra {
                Some((_, c)) => num(c)? as u32,
                None => 0,
            },
            toggle,
            script,
            callback,
            category: decode_text(category)?,
            retarget: match retarget {
                None | Some("-") => None,
                Some(id) => Some(num(id)? as CommandId),
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(army: u32) -> PlayerCommand {
        PlayerCommand {
            army,
            kind: CommandKind::Move,
            units: vec![4, 9],
            target: CommandTarget::Ground(FxVec3 { x: Fx::from_num(410.25), y: Fx::from_num(-3), z: Fx::from_bits(0x1234_5678_9abc) }),
            queue: true,
            formation: Some("AttackFormation".into()),
            facing: Some(-90),
            blueprint: None,
            count: 0,
            toggle: None,
            script: Vec::new(),
            callback: None,
            category: None,
            retarget: None,
        }
    }

    #[test]
    fn replay_lines_round_trip_exactly() {
        for c in [
            cmd(1),
            PlayerCommand { kind: CommandKind::Attack, target: CommandTarget::Entity(77), formation: None, facing: None, queue: false, ..cmd(2) },
            PlayerCommand { kind: CommandKind::Stop, target: CommandTarget::None, units: vec![], ..cmd(3) },
            PlayerCommand { kind: CommandKind::BuildFactory, target: CommandTarget::None, blueprint: Some("uel0101".into()), count: 5, ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, toggle: Some(Toggle::ScriptBit(3, true)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, toggle: Some(Toggle::FireState(2)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, toggle: Some(Toggle::Paused(true)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, toggle: Some(Toggle::AutoMode(true)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, toggle: Some(Toggle::AutoSurfaceMode(false)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, toggle: Some(Toggle::BuildCount(2, -5)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, units: vec![], toggle: Some(Toggle::GameSpeed(-3)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, units: vec![], toggle: Some(Toggle::GameSpeed(4)), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Stop, units: vec![], target: CommandTarget::Ground(FxVec3 { x: Fx::from_num(3), y: Fx::ZERO, z: Fx::from_num(-7) }), retarget: Some(41), ..cmd(1) },
            PlayerCommand { kind: CommandKind::Script, script: vec![("Enhancement".into(), "AdvancedEngineering".into()), ("TaskName".into(), "EnhanceTask".into())], ..cmd(1) },
            PlayerCommand {
                kind: CommandKind::Stop,
                callback: Some(SimCallback {
                    func: "SpawnPing".into(),
                    args: LuaData::Table(vec![(LuaData::string("Name"), LuaData::string("a ping; with spaces")), (LuaData::string("X"), LuaData::number(0.1))]),
                    with_selection: true,
                }),
                ..cmd(1)
            },
            // Text a peer must read back exactly (a mod's names).
            PlayerCommand {
                kind: CommandKind::Script,
                script: vec![("Enhancement".into(), "Shield Upgrade;x=1 50%".into()), ("".into(), "-".into()), ("Name".into(), "tab\there é".into())],
                ..cmd(1)
            },
            PlayerCommand { kind: CommandKind::FormMove, formation: Some("Attack Formation".into()), blueprint: Some("".into()), ..cmd(1) },
            PlayerCommand { kind: CommandKind::FormMove, formation: Some("-".into()), blueprint: Some("%".into()), ..cmd(1) },
        ] {
            let s = Scheduled { tick: 12, client: 1, seq: 3, command: c };
            let line = encode(&s);
            assert_eq!(line.split_whitespace().count(), 17, "{line}");
            assert_eq!(decode(&line).unwrap(), s);
        }
        assert!(decode("1 2 3").is_err());
        // Older recordings (no blueprint / count) still read.
        let old = decode("12 1 3 1 2 1 - - e5 4,9").unwrap();
        assert_eq!((old.command.blueprint, old.command.count), (None, 0));
        // A category expression with spaces and a literal `%` survives the
        // whitespace-split line exactly.
        let cat = PlayerCommand { kind: CommandKind::TransportUnloadSpecificUnits, category: Some("TECH1 * LAND 100%".into()), ..cmd(1) };
        let s = Scheduled { tick: 12, client: 1, seq: 3, command: cat };
        let line = encode(&s);
        assert_eq!(line.split_whitespace().count(), 17, "{line}");
        assert_eq!(decode(&line).unwrap(), s);
        assert!(decode("12 1 3 1 25 0 - - e5 4,9 - 0 - - - TECH1%20*%20LAND").unwrap().command.category == Some("TECH1 * LAND".into()));
        assert!(decode("12 1 3 1 2 1 - - e5 4,9").unwrap().command.category.is_none(), "older lines have no category");
        assert!(decode("12 1 3 1 2 1 - - e5 4,9 - 0 - - - %zz").is_err(), "a bad escape fails");
        assert!(decode("12 1 3 1 2 1 - - e5 4,9 - 0 - - - -").unwrap().command.retarget.is_none(), "older lines move no command");
    }

    #[test]
    fn due_commands_come_out_in_tick_client_seq_order() {
        let mut s = CommandStream::default();
        for (tick, client, seq) in [(5, 1, 0), (3, 1, 1), (3, 0, 7), (3, 1, 0), (9, 0, 0)] {
            s.schedule(Scheduled { tick, client, seq, command: cmd(client + 1) });
        }
        let due: Vec<_> = s.take_due(5).iter().map(|d| (d.tick, d.client, d.seq)).collect();
        assert_eq!(due, vec![(3, 0, 7), (3, 1, 0), (3, 1, 1), (5, 1, 0)]);
        assert!(s.take_due(8).is_empty());
        assert_eq!(s.take_due(9).len(), 1);
        assert!(s.is_empty());
    }

    #[test]
    fn link_stamps_commands_with_the_input_delay() {
        let (tx, mut link) = channel();
        tx.send(cmd(1)).unwrap();
        tx.send(cmd(2)).unwrap();
        let mut s = CommandStream::default();
        link.drain_into(&mut s, 10, 2, |_| true);
        assert!(s.take_due(10).is_empty(), "not before the delay");
        let due = s.take_due(10 + INPUT_DELAY);
        assert_eq!(due.iter().map(|d| (d.client, d.seq, d.command.army)).collect::<Vec<_>>(), vec![(2, 0, 1), (2, 1, 2)]);
    }
}
