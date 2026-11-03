//! LAN game discovery (moho's `CDiscoveryService`): hosts announce
//! themselves, the LAN screen lists what it hears.
//!
//! A hosting lobby sends a beacon every second: the game's description
//! (FAF's `GameConfigRequested` table: name, host, options, player count)
//! and the port it accepts joins on. It goes out as a broadcast (every
//! machine on the subnet) and to a multicast group, which also reaches
//! every client on the sending machine (several local clients share the
//! port; unicast would reach only one of them). Listeners fill in the
//! address from where the beacon came from, so it is the one the host is
//! reachable at, and forget a game they haven't heard from in
//! [`FORGET_AFTER`].

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::engine::sim::lua_data::LuaData;

pub const DISCOVERY_PORT: u16 = 16000;
const GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 77);
const MAGIC: &[u8; 5] = b"RTSD1";
pub const BEACON_EVERY: Duration = Duration::from_secs(1);
const FORGET_AFTER: Duration = Duration::from_secs(5);

/// Sends a hosting lobby's beacons.
pub struct Beacon {
    socket: UdpSocket,
}

impl Beacon {
    pub fn new() -> std::io::Result<Beacon> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        socket.set_multicast_loop_v4(true)?;
        socket.set_multicast_ttl_v4(1)?;
        Ok(Beacon { socket })
    }

    /// Announces a game that accepts joins on `port`: to each interface's
    /// subnet broadcast address, the all-ones broadcast and the multicast
    /// group. (macOS's Local Network privacy can refuse some of them for an
    /// app the user hasn't allowed yet; the subnet broadcast has been the
    /// one that gets through, and it also reaches clients on this machine.)
    pub fn send(&self, port: u16, info: &LuaData) {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&port.to_le_bytes());
        info.encode(&mut bytes);
        let mut targets: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| match i.addr {
                if_addrs::IfAddr::V4(v4) if !v4.ip.is_loopback() => v4.broadcast,
                _ => None,
            })
            .collect();
        targets.sort();
        targets.dedup();
        targets.extend([Ipv4Addr::BROADCAST, GROUP]);
        for to in targets {
            if let Err(e) = self.socket.send_to(&bytes, SocketAddrV4::new(to, DISCOVERY_PORT)) {
                tracing::trace!(%to, error = %e, "discovery: beacon not sent");
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// A game at `index` (0-based, stable while it's listed), reachable at
    /// `address`.
    New { index: usize, address: String, info: LuaData },
    Updated { index: usize, address: String, info: LuaData },
    Removed { index: usize },
}

/// The LAN screen's listener.
pub struct Listener {
    socket: UdpSocket,
    games: Vec<Option<(SocketAddr, LuaData, Instant)>>,
}

impl Listener {
    pub fn new() -> std::io::Result<Listener> {
        let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        s.set_reuse_address(true)?;
        #[cfg(unix)]
        s.set_reuse_port(true)?;
        s.bind(&SocketAddr::from(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())?;
        // On every interface the system picks for the group; failure (no
        // multicast route) still leaves broadcasts.
        if let Err(e) = s.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED) {
            tracing::warn!(error = %e, "discovery: could not join the multicast group");
        }
        s.set_nonblocking(true)?;
        Ok(Listener { socket: s.into(), games: Vec::new() })
    }

    pub fn count(&self) -> usize {
        self.games.iter().filter(|g| g.is_some()).count()
    }

    /// Clears the list (`Reset`).
    pub fn reset(&mut self) -> Vec<Found> {
        let removed = self.games.iter().enumerate().filter(|(_, g)| g.is_some()).map(|(index, _)| Found::Removed { index }).collect();
        self.games.clear();
        removed
    }

    /// What changed since the last call.
    pub fn poll(&mut self) -> Vec<Found> {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        let now = Instant::now();
        loop {
            let (n, from) = match self.socket.recv_from(&mut buf) {
                Ok(r) => r,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            };
            let b = &buf[..n];
            if b.len() < MAGIC.len() + 2 || &b[..MAGIC.len()] != MAGIC {
                continue;
            }
            let port = u16::from_le_bytes([b[MAGIC.len()], b[MAGIC.len() + 1]]);
            let mut rest = &b[MAGIC.len() + 2..];
            let Ok(info) = LuaData::decode(&mut rest) else { continue };
            let addr = SocketAddr::new(from.ip(), port);
            let address = addr.to_string();
            match self.games.iter().position(|g| g.as_ref().is_some_and(|(a, _, _)| *a == addr)) {
                Some(index) => {
                    let g = self.games[index].as_mut().expect("listed");
                    g.2 = now;
                    if g.1 != info {
                        g.1 = info.clone();
                        out.push(Found::Updated { index, address, info });
                    }
                }
                None => {
                    let index = self.games.iter().position(Option::is_none).unwrap_or(self.games.len());
                    if index == self.games.len() {
                        self.games.push(None);
                    }
                    self.games[index] = Some((addr, info.clone(), now));
                    out.push(Found::New { index, address, info });
                }
            }
        }
        for (index, g) in self.games.iter_mut().enumerate() {
            if g.as_ref().is_some_and(|(_, _, seen)| now.duration_since(*seen) > FORGET_AFTER) {
                *g = None;
                out.push(Found::Removed { index });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A beacon reaches two listeners on this machine (several local clients
    /// share the discovery port).
    #[test]
    fn local_listeners_all_hear_a_beacon() {
        let (Ok(mut a), Ok(mut b)) = (Listener::new(), Listener::new()) else {
            eprintln!("skipping: discovery port unavailable");
            return;
        };
        let beacon = Beacon::new().unwrap();
        let info = LuaData::Table(vec![(LuaData::string("GameName"), LuaData::string("test game"))]);
        let mut heard = (false, false);
        for _ in 0..50 {
            beacon.send(16123, &info);
            std::thread::sleep(Duration::from_millis(20));
            heard.0 |= a.poll().iter().any(|f| matches!(f, Found::New { address, .. } if address.ends_with(":16123")));
            heard.1 |= b.poll().iter().any(|f| matches!(f, Found::New { address, .. } if address.ends_with(":16123")));
            if heard == (true, true) {
                break;
            }
        }
        assert_eq!(heard, (true, true));
    }
}
