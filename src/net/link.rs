//! A reliable, ordered byte channel to one peer over UDP.
//!
//! Lockstep needs every turn delivered, in order, quickly; it sends little
//! (one small turn per tick per peer), so this keeps to the simplest scheme
//! that is fast on a LAN and holds up on the internet:
//!
//! - Outgoing bytes are cut into numbered chunks (at most [`CHUNK`] bytes,
//!   so a packet fits a 1200-byte datagram anywhere, and iroh's).
//! - Every packet carries an ack: the next chunk id expected (everything
//!   before it arrived) and a 64-bit map of the chunks after it that also
//!   arrived, so one lost packet doesn't make the sender resend the rest.
//! - A chunk not acked within the retransmission timeout (RTT-based, as
//!   TCP's: `srtt + 4 * rttvar`, doubling on each retry) is sent again.
//! - The round-trip time is measured from every packet, keepalives
//!   included (as moho's UDP connection does: the lobby's `ping` stays
//!   current with no data flowing): a packet carries the sender's clock
//!   (`stamp`), the latest stamp it received (`echo`) and how long it held
//!   that stamp before sending (`held`); the stamp's owner gets
//!   `now - echo - held`, the network round trip without the peer's delay.
//! - The receiver holds chunks that arrive early and releases bytes in
//!   order; the caller cuts them into frames by their length prefix.
//! - With nothing to send, a packet still goes out every [`KEEPALIVE`]: the
//!   ack, and proof the peer is alive (a peer silent for long is lost).
//!
//! No I/O here: the network thread feeds packets in and sends what comes
//! out, which keeps this testable under simulated loss and reordering.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

const MAGIC: &[u8; 4] = b"RTU2";
/// Payload bytes per chunk.
pub const CHUNK: usize = 1100;
const HEADER: usize = 4 + 4 + 4 + 8 + 4 + 4 + 4 + 1;
/// Largest datagram sent: one full chunk with its header. Small chunks
/// share packets up to this, so a packet never outgrows a full chunk's
/// (iroh's QUIC datagrams allow 1162 bytes on a fresh path).
pub const MTU: usize = HEADER + 6 + CHUNK;
/// `held` of a packet that echoes nothing (no stamp received yet).
const NO_ECHO: u32 = u32::MAX;
/// Chunks in flight beyond the oldest unacked one.
const WINDOW: u32 = 1024;
pub const KEEPALIVE: Duration = Duration::from_millis(250);
const RTO_MIN: Duration = Duration::from_millis(30);
const RTO_MAX: Duration = Duration::from_millis(2000);
const RTO_INITIAL: Duration = Duration::from_millis(200);

struct Out {
    id: u32,
    data: Vec<u8>,
    /// Last time it went out (`None`: not yet).
    sent: Option<Instant>,
    tries: u32,
}

/// A parsed packet.
pub struct Packet {
    pub nonce: u32,
    pub ack: u32,
    pub sack: u64,
    /// The sender's clock (µs since its link began, wrapping).
    pub stamp: u32,
    /// The latest `stamp` the sender received from us.
    pub echo: u32,
    /// µs between the sender receiving `echo` and sending this ([`NO_ECHO`]: none).
    pub held: u32,
    pub chunks: Vec<(u32, Vec<u8>)>,
}

impl Packet {
    pub fn decode(b: &[u8]) -> Option<Packet> {
        if b.len() < HEADER || &b[..4] != MAGIC {
            return None;
        }
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().expect("4 bytes"));
        let nonce = u32_at(4);
        let ack = u32_at(8);
        let sack = u64::from_le_bytes(b[12..20].try_into().expect("8 bytes"));
        let stamp = u32_at(20);
        let echo = u32_at(24);
        let held = u32_at(28);
        let n = b[HEADER - 1] as usize;
        let mut at = HEADER;
        let mut chunks = Vec::with_capacity(n);
        for _ in 0..n {
            if b.len() < at + 6 {
                return None;
            }
            let id = u32_at(at);
            let len = u16::from_le_bytes([b[at + 4], b[at + 5]]) as usize;
            at += 6;
            if len > CHUNK || b.len() < at + len {
                return None;
            }
            chunks.push((id, b[at..at + len].to_vec()));
            at += len;
        }
        Some(Packet { nonce, ack, sack, stamp, echo, held, chunks })
    }
}

/// Round trips kept for the 99th percentile (a few seconds to a minute of
/// traffic, depending on how busy the link is).
const RECENT_SAMPLES: usize = 512;

pub struct Link {
    out_next: u32,
    unacked: VecDeque<Out>,
    in_next: u32,
    early: BTreeMap<u32, Vec<u8>>,
    /// Received in-order bytes not yet taken as frames.
    stream: Vec<u8>,
    srtt: Option<f64>,
    rttvar: f64,
    /// The latest round trips measured, ms, oldest first (at most
    /// [`RECENT_SAMPLES`]): the ping grid's 99th percentile.
    recent: VecDeque<f32>,
    /// Our clock's zero (`stamp`s count from it).
    epoch: Instant,
    /// The newest stamp received and when it arrived (echoed back).
    peer_stamp: Option<(u32, Instant)>,
    /// The last echo measured (each is sampled once).
    last_echo: Option<u32>,
    pub last_heard: Instant,
    last_sent: Instant,
    ack_owed: bool,
}

impl Link {
    pub fn new(now: Instant) -> Self {
        Link { out_next: 0, unacked: VecDeque::new(), in_next: 0, early: BTreeMap::new(), stream: Vec::new(), srtt: None, rttvar: 0.0, recent: VecDeque::with_capacity(RECENT_SAMPLES), epoch: now, peer_stamp: None, last_echo: None, last_heard: now, last_sent: now, ack_owed: false }
    }

    /// Queues bytes to deliver (they go out on the next [`Self::transmit`]).
    pub fn queue(&mut self, bytes: &[u8]) {
        for piece in bytes.chunks(CHUNK) {
            self.unacked.push_back(Out { id: self.out_next, data: piece.to_vec(), sent: None, tries: 0 });
            self.out_next = self.out_next.wrapping_add(1);
        }
    }

    /// Smoothed round-trip time, ms (until measured: the initial timeout).
    pub fn rtt_ms(&self) -> f64 {
        self.srtt.unwrap_or(RTO_INITIAL.as_secs_f64() * 1000.0)
    }

    /// Bytes queued but not yet acknowledged.
    pub fn in_flight(&self) -> usize {
        self.unacked.iter().map(|o| o.data.len()).sum()
    }

    fn rto(&self) -> Duration {
        match self.srtt {
            Some(s) => Duration::from_secs_f64((s + 4.0 * self.rttvar) / 1000.0).clamp(RTO_MIN, RTO_MAX),
            None => RTO_INITIAL,
        }
    }

    /// Takes a packet in. Returns false if the link broke (an oversized frame).
    pub fn receive(&mut self, p: &Packet, now: Instant) -> bool {
        self.last_heard = now;
        // Round trip, from the echo of our clock.
        if p.held != NO_ECHO && self.last_echo != Some(p.echo) {
            let elapsed = self.clock(now).wrapping_sub(p.echo).wrapping_sub(p.held);
            // A negative (wrapped) result is a stale or bogus echo.
            if (elapsed as i32) >= 0 {
                self.last_echo = Some(p.echo);
                self.sample(Duration::from_micros(elapsed as u64));
            }
        }
        if self.peer_stamp.is_none_or(|(s, _)| seq_lt(s, p.stamp)) {
            self.peer_stamp = Some((p.stamp, now));
        }
        // Acks.
        let before = self.unacked.len();
        while let Some(o) = self.unacked.front() {
            if seq_lt(o.id, p.ack) {
                self.unacked.pop_front();
            } else {
                break;
            }
        }
        if p.sack != 0 {
            let ack = p.ack;
            self.unacked.retain(|o| {
                let d = o.id.wrapping_sub(ack).wrapping_sub(1);
                !(d < 64 && p.sack & (1 << d) != 0)
            });
        }
        let _ = before;
        // Data.
        for (id, data) in &p.chunks {
            self.ack_owed = true;
            if seq_lt(*id, self.in_next) {
                continue; // a resend of something we have
            }
            if id.wrapping_sub(self.in_next) > WINDOW * 2 {
                continue; // nonsense
            }
            self.early.insert(*id, data.clone());
        }
        while let Some(data) = self.early.remove(&self.in_next) {
            self.stream.extend_from_slice(&data);
            self.in_next = self.in_next.wrapping_add(1);
        }
        true
    }

    /// µs since the link began, wrapping (the `stamp` clock).
    fn clock(&self, now: Instant) -> u32 {
        now.saturating_duration_since(self.epoch).as_micros() as u32
    }

    /// The 99th percentile of the latest round trips, ms (none measured yet:
    /// `None`).
    pub fn rtt_p99_ms(&self) -> Option<f64> {
        if self.recent.is_empty() {
            return None;
        }
        let mut sorted: Vec<f32> = self.recent.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let rank = ((sorted.len() as f64 * 0.99).ceil() as usize).clamp(1, sorted.len());
        Some(sorted[rank - 1] as f64)
    }

    fn sample(&mut self, rtt: Duration) {
        let r = rtt.as_secs_f64() * 1000.0;
        if self.recent.len() == RECENT_SAMPLES {
            self.recent.pop_front();
        }
        self.recent.push_back(r as f32);
        match self.srtt {
            None => {
                self.srtt = Some(r);
                self.rttvar = r / 2.0;
            }
            Some(s) => {
                self.rttvar = 0.75 * self.rttvar + 0.25 * (s - r).abs();
                self.srtt = Some(0.875 * s + 0.125 * r);
            }
        }
    }

    /// Complete frames received so far (each a `u32` length prefix and a
    /// payload; the payloads are returned). `Err` on a frame longer than
    /// `max`.
    pub fn frames(&mut self, max: usize) -> Result<Vec<Vec<u8>>, String> {
        let mut out = Vec::new();
        let mut at = 0;
        while self.stream.len() >= at + 4 {
            let len = u32::from_le_bytes(self.stream[at..at + 4].try_into().expect("4 bytes")) as usize;
            if len > max {
                return Err(format!("frame of {len} bytes"));
            }
            if self.stream.len() < at + 4 + len {
                break;
            }
            out.push(self.stream[at + 4..at + 4 + len].to_vec());
            at += 4 + len;
        }
        self.stream.drain(..at);
        Ok(out)
    }

    /// Packets to send now: new and overdue chunks, else an ack or a
    /// keepalive when due.
    pub fn transmit(&mut self, nonce: u32, now: Instant) -> Vec<Vec<u8>> {
        let rto = self.rto();
        let limit = self.unacked.front().map_or(self.out_next, |o| o.id.wrapping_add(WINDOW));
        let mut packets = Vec::new();
        let mut current: Option<(Vec<u8>, u8)> = None;
        let (ack, sack) = self.ack_fields();
        let (echo, held) = match self.peer_stamp {
            Some((stamp, at)) => (stamp, (now.saturating_duration_since(at).as_micros() as u32).min(NO_ECHO - 1)),
            None => (0, NO_ECHO),
        };
        let stamp = self.clock(now);
        let header = |ack, sack| header(nonce, ack, sack, stamp, echo, held);
        for o in self.unacked.iter_mut() {
            if !seq_lt(o.id, limit) {
                break;
            }
            let due = match o.sent {
                None => true,
                Some(t) => now.duration_since(t) >= (rto * (1 << o.tries.saturating_sub(1).min(6))).min(RTO_MAX),
            };
            if !due {
                continue;
            }
            let need = 6 + o.data.len();
            if current.as_ref().is_some_and(|(b, n)| b.len() + need > MTU || *n == u8::MAX) {
                packets.push(finish(current.take().expect("current")));
            }
            let (buf, n) = current.get_or_insert_with(|| (header(ack, sack), 0));
            buf.extend_from_slice(&o.id.to_le_bytes());
            buf.extend_from_slice(&(o.data.len() as u16).to_le_bytes());
            buf.extend_from_slice(&o.data);
            *n += 1;
            o.sent = Some(now);
            o.tries += 1;
        }
        if let Some(c) = current {
            packets.push(finish(c));
        }
        if packets.is_empty() && (self.ack_owed || now.duration_since(self.last_sent) >= KEEPALIVE) {
            packets.push(finish((header(ack, sack), 0)));
        }
        if !packets.is_empty() {
            self.ack_owed = false;
            self.last_sent = now;
        }
        packets
    }

    fn ack_fields(&self) -> (u32, u64) {
        let mut sack = 0u64;
        for &id in self.early.keys() {
            let d = id.wrapping_sub(self.in_next).wrapping_sub(1);
            if d < 64 {
                sack |= 1 << d;
            }
        }
        (self.in_next, sack)
    }
}

fn header(nonce: u32, ack: u32, sack: u64, stamp: u32, echo: u32, held: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(MTU);
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&nonce.to_le_bytes());
    b.extend_from_slice(&ack.to_le_bytes());
    b.extend_from_slice(&sack.to_le_bytes());
    b.extend_from_slice(&stamp.to_le_bytes());
    b.extend_from_slice(&echo.to_le_bytes());
    b.extend_from_slice(&held.to_le_bytes());
    b.push(0);
    b
}

fn finish((mut b, n): (Vec<u8>, u8)) -> Vec<u8> {
    b[HEADER - 1] = n;
    b
}

/// `a` before `b` in wrapping sequence order.
fn seq_lt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) < 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut f = (payload.len() as u32).to_le_bytes().to_vec();
        f.extend_from_slice(payload);
        f
    }

    /// Two links over a lossy, reordering wire: everything arrives once,
    /// in order.
    #[test]
    fn delivers_in_order_through_loss_and_reordering() {
        let t0 = Instant::now();
        let (mut a, mut b) = (Link::new(t0), Link::new(t0));
        let sent: Vec<Vec<u8>> = (0..400u32).map(|i| (0..(i * 37 % 3000)).map(|k| (k ^ i) as u8).collect()).collect();
        for s in &sent {
            a.queue(&frame(s));
        }
        let mut got = Vec::new();
        let mut rng = 12345u64;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut wire_ab: Vec<Vec<u8>> = Vec::new();
        let mut wire_ba: Vec<Vec<u8>> = Vec::new();
        for step in 0..20_000u64 {
            let now = t0 + Duration::from_millis(step * 5);
            wire_ab.extend(a.transmit(1, now));
            wire_ba.extend(b.transmit(2, now));
            // 30% loss, then shuffle what's left (reordering).
            wire_ab.retain(|_| next() % 10 >= 3);
            wire_ba.retain(|_| next() % 10 >= 3);
            for i in (1..wire_ab.len()).rev() {
                wire_ab.swap(i, (next() as usize) % (i + 1));
            }
            for p in wire_ab.drain(..) {
                assert!(b.receive(&Packet::decode(&p).unwrap(), now));
            }
            for p in wire_ba.drain(..) {
                assert!(a.receive(&Packet::decode(&p).unwrap(), now));
            }
            got.extend(b.frames(1 << 20).unwrap());
            if got.len() == sent.len() && a.in_flight() == 0 {
                break;
            }
        }
        assert_eq!(got, sent);
        assert_eq!(a.in_flight(), 0, "all acked");
    }

    #[test]
    fn measures_round_trip_time() {
        let t0 = Instant::now();
        let (mut a, mut b) = (Link::new(t0), Link::new(t0));
        for i in 0..50u64 {
            let now = t0 + Duration::from_millis(i * 100);
            a.queue(&frame(b"turn"));
            for p in a.transmit(1, now) {
                b.receive(&Packet::decode(&p).unwrap(), now + Duration::from_millis(20));
            }
            b.frames(1024).unwrap();
            for p in b.transmit(2, now + Duration::from_millis(20)) {
                a.receive(&Packet::decode(&p).unwrap(), now + Duration::from_millis(40));
            }
        }
        assert!((a.rtt_ms() - 40.0).abs() < 2.0, "rtt {}", a.rtt_ms());
    }

    /// With no data flowing (an idle lobby) the keepalives alone keep the
    /// round trip current, and the peer's holding time isn't counted.
    #[test]
    fn keepalives_measure_round_trip_time() {
        let t0 = Instant::now();
        let (mut a, mut b) = (Link::new(t0), Link::new(t0));
        // One way takes `delay` ms; each side only sends keepalives, so b
        // holds a's stamp for up to a keepalive period before echoing it.
        let run = |a: &mut Link, b: &mut Link, from_ms: u64, to_ms: u64, delay: u64| {
            let mut to_b: Vec<(u64, Vec<u8>)> = Vec::new();
            let mut to_a: Vec<(u64, Vec<u8>)> = Vec::new();
            for ms in from_ms..to_ms {
                let now = t0 + Duration::from_millis(ms);
                to_b.extend(a.transmit(1, now).into_iter().map(|p| (ms + delay, p)));
                to_a.extend(b.transmit(2, now).into_iter().map(|p| (ms + delay, p)));
                for p in to_b.extract_if(.., |(at, _)| *at <= ms) {
                    b.receive(&Packet::decode(&p.1).unwrap(), now);
                }
                for p in to_a.extract_if(.., |(at, _)| *at <= ms) {
                    a.receive(&Packet::decode(&p.1).unwrap(), now);
                }
            }
        };
        assert_eq!(a.rtt_ms(), RTO_INITIAL.as_secs_f64() * 1000.0, "nothing measured yet");
        run(&mut a, &mut b, 0, 10_000, 15);
        assert!((a.rtt_ms() - 30.0).abs() < 2.0, "rtt {}", a.rtt_ms());
        assert!((b.rtt_ms() - 30.0).abs() < 2.0, "rtt {}", b.rtt_ms());
        // The path slows: the measurement follows without any data.
        run(&mut a, &mut b, 10_000, 30_000, 100);
        assert!((a.rtt_ms() - 200.0).abs() < 5.0, "rtt {}", a.rtt_ms());
    }

    #[test]
    fn garbage_packets_are_ignored() {
        assert!(Packet::decode(b"").is_none());
        assert!(Packet::decode(b"RTU2\0\0\0\0").is_none());
        let mut p = header(1, 0, 0, 0, 0, NO_ECHO);
        p[HEADER - 1] = 3; // claims chunks it doesn't have
        assert!(Packet::decode(&p).is_none());
    }
}
