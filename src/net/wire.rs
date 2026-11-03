//! Messages between peers and their bytes.
//!
//! Every message travels as one frame on the connection's single QUIC
//! stream (reliable and ordered per peer): a little-endian `u32` length,
//! then a tag byte and the fields. Lua tables (lobby data, the launch
//! configuration) go as [`LuaData`], whose encoding is canonical (fields in
//! key order), so every peer rebuilds the same table. Player commands go as
//! replay lines (`sim::input::encode`): exact (raw fixed-point bits) and
//! already tested to round-trip.

use crate::engine::sim::lua_data::LuaData;

/// Bumped whenever a message changes shape: peers with another version are
/// turned away at the handshake.
pub const PROTOCOL: u32 = 9;

/// Largest frame accepted (a launch configuration with every mod is a few
/// KB; lobby chat far less).
pub const MAX_FRAME: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// First frame on every connection. `peer_id` is set when an existing
    /// lobby member connects to complete the mesh (`None` = joining via
    /// the host, which assigns an id). `listen_port` is where the sender
    /// accepts connections; `iroh` its internet join code (empty: none).
    Hello { protocol: u32, build: String, name: String, uid: String, listen_port: u16, peer_id: Option<u32>, iroh: String },
    /// The host's answer to a joiner: its id, the host's, and the other
    /// members to connect to (`addr` with an empty host part = the address
    /// the joiner reached the host on, for peers on the host's machine).
    Welcome { your_id: u32, host_id: u32, peers: Vec<PeerAddr> },
    /// The handshake failed (full lobby, wrong version, game started).
    Reject { reason: String },
    /// Lobby `SendData` / `BroadcastData`.
    Data(LuaData),
    /// The host removed this peer (`EjectPeer`).
    Eject { reason: String },
    /// The host launched: the game configuration, the sim seed and the
    /// input delay (ticks) everyone starts with.
    Launch { config: LuaData, seed: u64, delay: u32 },
    /// One lockstep turn from the sender: its commands for tick `sched`
    /// (possibly none), meaning "nothing more from me up to `sched`".
    /// `checksums` are the sender's sim checksums (tick, checksum) not sent
    /// yet: normally the one after the previous tick. `capacity` is the fastest game speed
    /// (percent) the sender's machine can keep up with; everyone runs at the
    /// slowest client's. `at` is the tick the sender was starting when it
    /// sent the turn (clients that fall behind catch up). `slack` (once a
    /// second, else empty) is how the sender's last second of turns from
    /// each peer arrived: ticks to spare, negative = ticks it waited for
    /// them (each sender sizes its input delay from these).
    Turn { sched: u64, at: u64, commands: Vec<String>, checksums: Vec<(u64, u64)>, capacity: u32, slack: Vec<(u32, i32)> },
    /// A peer is gone mid-game: the sender has its turns up to `watermark`.
    /// Everyone stops waiting for it once all agree on the last turn.
    Dropped { peer: u32, watermark: u64 },
    /// Turns of a dropped peer relayed by a member that has them.
    Forward { origin: u32, sched: u64, commands: Vec<String> },
    /// The sender is leaving (closing its lobby or game).
    Bye,
    /// The members the sender has a connection to (FAF's lobby checks
    /// everyone can reach everyone before launching: `establishedPeers`).
    Established { peers: Vec<u32> },
    /// In-game chat (`SessionSendChatMessage`): a Lua table (or value) for
    /// FAF's `ReceiveChat`. Outside the sim and the replay, as in moho.
    Chat(LuaData),
    /// The sender votes to eject a player (`EjectSessionClient`, FAF's
    /// disconnect dialog). A player is dropped once every remaining client
    /// has voted.
    EjectVote { target: u32 },
    /// The sender paused or resumed the game: everyone does.
    Pause { paused: bool },
    /// The sender's round trip to each member it's connected to, about once
    /// a second, so every client can show the whole mesh's pings (the ping
    /// grid). Not passed to the lobby or the lockstep.
    Pings { pings: Vec<PeerPing> },
    /// The sender's installed sim mods, (uid, hash of its files) by uid,
    /// sent once to each member on connecting: the host refuses to launch
    /// while anyone's copy of a game mod is missing or differs
    /// ([`NetCore::mod_problems`](super::core::NetCore::mod_problems)).
    Mods { mods: Vec<(String, u64)> },
    /// The sender's checksum after `tick` part by part
    /// (`sim::checksum::PART_NAMES`), sent on its first desync with each
    /// peer: the receiver logs which parts differ.
    ChecksumParts { tick: u64, parts: Vec<u64> },
}

/// Most mods a [`Msg::Mods`] may list.
const MAX_MODS: usize = 1 << 16;

/// One entry of [`Msg::Pings`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerPing {
    pub peer: u32,
    /// Round trip (smoothed), microseconds.
    pub rtt_us: u32,
    /// The 99th percentile of the latest round trips, microseconds.
    pub rtt99_us: u32,
    pub path: PathKind,
}

/// How a link's packets travel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    Udp = 0,
    IrohDirect = 1,
    IrohRelay = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerAddr {
    pub id: u32,
    pub name: String,
    pub uid: String,
    pub addr: String,
}

impl Msg {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0; 4];
        let w = &mut out;
        match self {
            Msg::Hello { protocol, build, name, uid, listen_port, peer_id, iroh } => {
                w.push(1);
                put_u32(w, *protocol);
                put_str(w, build);
                put_str(w, name);
                put_str(w, uid);
                put_u32(w, *listen_port as u32);
                put_opt_u32(w, *peer_id);
                put_str(w, iroh);
            }
            Msg::Welcome { your_id, host_id, peers } => {
                w.push(2);
                put_u32(w, *your_id);
                put_u32(w, *host_id);
                put_u32(w, peers.len() as u32);
                for p in peers {
                    put_u32(w, p.id);
                    put_str(w, &p.name);
                    put_str(w, &p.uid);
                    put_str(w, &p.addr);
                }
            }
            Msg::Reject { reason } => {
                w.push(3);
                put_str(w, reason);
            }
            Msg::Data(data) => {
                w.push(4);
                data.encode(w);
            }
            Msg::Eject { reason } => {
                w.push(5);
                put_str(w, reason);
            }
            Msg::Launch { config, seed, delay } => {
                w.push(6);
                config.encode(w);
                put_u64(w, *seed);
                put_u32(w, *delay);
            }
            Msg::Turn { sched, at, commands, checksums, capacity, slack } => {
                w.push(7);
                put_u64(w, *sched);
                put_u64(w, *at);
                put_u32(w, *capacity);
                put_strs(w, commands);
                put_u32(w, checksums.len() as u32);
                for (t, c) in checksums {
                    put_u64(w, *t);
                    put_u64(w, *c);
                }
                put_u32(w, slack.len() as u32);
                for (peer, s) in slack {
                    put_u32(w, *peer);
                    put_u32(w, *s as u32);
                }
            }
            Msg::Dropped { peer, watermark } => {
                w.push(9);
                put_u32(w, *peer);
                put_u64(w, *watermark);
            }
            Msg::Forward { origin, sched, commands } => {
                w.push(10);
                put_u32(w, *origin);
                put_u64(w, *sched);
                put_strs(w, commands);
            }
            Msg::Bye => w.push(11),
            Msg::Chat(data) => {
                w.push(13);
                data.encode(w);
            }
            Msg::EjectVote { target } => {
                w.push(14);
                put_u32(w, *target);
            }
            Msg::Pause { paused } => {
                w.push(15);
                w.push(*paused as u8);
            }
            Msg::Pings { pings } => {
                w.push(16);
                put_u32(w, pings.len() as u32);
                for p in pings {
                    put_u32(w, p.peer);
                    put_u32(w, p.rtt_us);
                    put_u32(w, p.rtt99_us);
                    w.push(p.path as u8);
                }
            }
            Msg::ChecksumParts { tick, parts } => {
                w.push(18);
                put_u64(w, *tick);
                put_u32(w, parts.len() as u32);
                for p in parts {
                    put_u64(w, *p);
                }
            }
            Msg::Mods { mods } => {
                w.push(17);
                put_u32(w, mods.len() as u32);
                for (uid, hash) in mods {
                    put_str(w, uid);
                    put_u64(w, *hash);
                }
            }
            Msg::Established { peers } => {
                w.push(12);
                put_u32(w, peers.len() as u32);
                for p in peers {
                    put_u32(w, *p);
                }
            }
        }
        let len = (out.len() - 4) as u32;
        out[..4].copy_from_slice(&len.to_le_bytes());
        out
    }

    /// One frame's payload (without its length prefix).
    pub fn decode(mut b: &[u8]) -> Result<Msg, String> {
        let r = &mut b;
        let msg = match take_u8(r)? {
            1 => Msg::Hello {
                protocol: take_u32(r)?,
                build: take_str(r)?,
                name: take_str(r)?,
                uid: take_str(r)?,
                listen_port: take_u32(r)? as u16,
                peer_id: take_opt_u32(r)?,
                iroh: take_str(r)?,
            },
            2 => {
                let your_id = take_u32(r)?;
                let host_id = take_u32(r)?;
                let n = take_u32(r)? as usize;
                let mut peers = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    peers.push(PeerAddr { id: take_u32(r)?, name: take_str(r)?, uid: take_str(r)?, addr: take_str(r)? });
                }
                Msg::Welcome { your_id, host_id, peers }
            }
            3 => Msg::Reject { reason: take_str(r)? },
            4 => Msg::Data(LuaData::decode(r)?),
            5 => Msg::Eject { reason: take_str(r)? },
            6 => Msg::Launch { config: LuaData::decode(r)?, seed: take_u64(r)?, delay: take_u32(r)? },
            7 => {
                let sched = take_u64(r)?;
                let at = take_u64(r)?;
                let capacity = take_u32(r)?;
                let commands = take_strs(r)?;
                let n = take_u32(r)? as usize;
                if n > 1024 {
                    return Err(format!("{n} checksums"));
                }
                let checksums = (0..n).map(|_| Ok((take_u64(r)?, take_u64(r)?))).collect::<Result<_, String>>()?;
                let n = take_u32(r)? as usize;
                if n > 1024 {
                    return Err(format!("{n} slack entries"));
                }
                let slack = (0..n).map(|_| Ok((take_u32(r)?, take_u32(r)? as i32))).collect::<Result<_, String>>()?;
                Msg::Turn { sched, at, commands, checksums, capacity, slack }
            }
            // 8 was `Speed`: speed changes are commands now (`Toggle::GameSpeed`).
            9 => Msg::Dropped { peer: take_u32(r)?, watermark: take_u64(r)? },
            10 => Msg::Forward { origin: take_u32(r)?, sched: take_u64(r)?, commands: take_strs(r)? },
            11 => Msg::Bye,
            13 => Msg::Chat(LuaData::decode(r)?),
            14 => Msg::EjectVote { target: take_u32(r)? },
            15 => Msg::Pause { paused: take_u8(r)? != 0 },
            12 => {
                let n = take_u32(r)? as usize;
                if n > 1024 {
                    return Err("too many peers".into());
                }
                Msg::Established { peers: (0..n).map(|_| take_u32(r)).collect::<Result<_, _>>()? }
            }
            16 => {
                let n = take_u32(r)? as usize;
                if n > 1024 {
                    return Err("too many peers".into());
                }
                let mut pings = Vec::with_capacity(n);
                for _ in 0..n {
                    let (peer, rtt_us, rtt99_us) = (take_u32(r)?, take_u32(r)?, take_u32(r)?);
                    let path = match take_u8(r)? {
                        1 => PathKind::IrohDirect,
                        2 => PathKind::IrohRelay,
                        _ => PathKind::Udp,
                    };
                    pings.push(PeerPing { peer, rtt_us, rtt99_us, path });
                }
                Msg::Pings { pings }
            }
            17 => {
                let n = take_u32(r)? as usize;
                if n > MAX_MODS {
                    return Err(format!("{n} mods"));
                }
                Msg::Mods { mods: (0..n).map(|_| Ok((take_str(r)?, take_u64(r)?))).collect::<Result<_, String>>()? }
            }
            18 => {
                let tick = take_u64(r)?;
                let n = take_u32(r)? as usize;
                if n > 64 {
                    return Err(format!("{n} checksum parts"));
                }
                Msg::ChecksumParts { tick, parts: (0..n).map(|_| take_u64(r)).collect::<Result<_, _>>()? }
            }
            t => return Err(format!("unknown message tag {t}")),
        };
        if !r.is_empty() {
            return Err(format!("{} trailing bytes", r.len()));
        }
        Ok(msg)
    }
}

fn put_u32(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(w: &mut Vec<u8>, v: u64) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    put_u32(w, s.len() as u32);
    w.extend_from_slice(s.as_bytes());
}

fn put_strs(w: &mut Vec<u8>, v: &[String]) {
    put_u32(w, v.len() as u32);
    for s in v {
        put_str(w, s);
    }
}

fn put_opt_u32(w: &mut Vec<u8>, v: Option<u32>) {
    match v {
        Some(v) => {
            w.push(1);
            put_u32(w, v);
        }
        None => w.push(0),
    }
}

fn take<'a>(r: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if r.len() < n {
        return Err("truncated message".into());
    }
    let (head, tail) = r.split_at(n);
    *r = tail;
    Ok(head)
}

fn take_u8(r: &mut &[u8]) -> Result<u8, String> {
    Ok(take(r, 1)?[0])
}

fn take_u32(r: &mut &[u8]) -> Result<u32, String> {
    Ok(u32::from_le_bytes(take(r, 4)?.try_into().expect("4 bytes")))
}

fn take_u64(r: &mut &[u8]) -> Result<u64, String> {
    Ok(u64::from_le_bytes(take(r, 8)?.try_into().expect("8 bytes")))
}

fn take_str(r: &mut &[u8]) -> Result<String, String> {
    let n = take_u32(r)? as usize;
    if n > MAX_FRAME {
        return Err("string too long".into());
    }
    String::from_utf8(take(r, n)?.to_vec()).map_err(|_| "string is not UTF-8".into())
}

fn take_strs(r: &mut &[u8]) -> Result<Vec<String>, String> {
    let n = take_u32(r)? as usize;
    if n > MAX_FRAME / 4 {
        return Err("list too long".into());
    }
    (0..n).map(|_| take_str(r)).collect()
}

fn take_opt_u32(r: &mut &[u8]) -> Result<Option<u32>, String> {
    Ok(match take_u8(r)? {
        0 => None,
        _ => Some(take_u32(r)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_round_trips() {
        let table = LuaData::Table(vec![(LuaData::string("Type"), LuaData::string("AddPlayer")), (LuaData::string("Slot"), LuaData::Number(3.0f64.to_bits()))]);
        let msgs = [
            Msg::Hello { protocol: PROTOCOL, build: "abc".into(), name: "Tom".into(), uid: "7".into(), listen_port: 16010, peer_id: Some(3), iroh: "abc".into() },
            Msg::Hello { protocol: PROTOCOL, build: String::new(), name: "Ann".into(), uid: String::new(), listen_port: 1, peer_id: None, iroh: String::new() },
            Msg::Welcome { your_id: 4, host_id: 1, peers: vec![PeerAddr { id: 2, name: "a".into(), uid: "u".into(), addr: "10.0.0.2:16010".into() }] },
            Msg::Reject { reason: "LobbyFull".into() },
            Msg::Data(table.clone()),
            Msg::Eject { reason: "KickedByHost".into() },
            Msg::Launch { config: table, seed: u64::MAX, delay: 3 },
            Msg::Turn { sched: 1234, at: 1231, commands: vec!["10 0 0 1 Move".into()], checksums: vec![(1229, 7), (1230, 0xdead_beef)], capacity: 350, slack: vec![(2, 3), (5, -4)] },
            Msg::Turn { sched: 5, at: 3, commands: vec![], checksums: vec![], capacity: 0, slack: vec![] },
            Msg::Dropped { peer: 2, watermark: 99 },
            Msg::Forward { origin: 2, sched: 98, commands: vec!["x".into()] },
            Msg::Bye,
            Msg::Established { peers: vec![1, 3, 4] },
            Msg::Chat(LuaData::string("gl hf")),
            Msg::EjectVote { target: 3 },
            Msg::Pause { paused: true },
            Msg::ChecksumParts { tick: 12418, parts: vec![1, 2, u64::MAX] },
            Msg::Mods { mods: vec![("74A9EAB2-E851-11DB-A1F1-F2C755D89593".into(), 0x45062f596479d75e), ("x".into(), 0)] },
            Msg::Pings { pings: vec![PeerPing { peer: 2, rtt_us: 45_000, rtt99_us: 61_000, path: PathKind::IrohRelay }, PeerPing { peer: 3, rtt_us: 900, rtt99_us: 2_100, path: PathKind::Udp }] },
        ];
        for m in msgs {
            let bytes = m.encode();
            let len = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
            assert_eq!(len, bytes.len() - 4);
            assert_eq!(Msg::decode(&bytes[4..]).unwrap(), m);
        }
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(Msg::decode(&[]).is_err());
        assert!(Msg::decode(&[7, 1, 2]).is_err());
        assert!(Msg::decode(&[99]).is_err());
        assert!(Msg::decode(&[3, 255, 255, 255, 255]).is_err());
    }
}
