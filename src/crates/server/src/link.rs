//! In-process link between the QUIC relays and the game thread.
//!
//! The game thread prepares each snapshot view once (`Frame`: quantized, its
//! full encoding and every player's exact own state) and hands every QUIC
//! session of that view the same frame. Each relay sends a delta against
//! the newest frame its client acknowledged; a frame encodes each delta
//! once, for every relay with that baseline. Loopback TCP stays for LAN
//! hosting and tests.
use std::{io, net::IpAddr, sync::{Arc, Mutex, OnceLock, Weak, mpsc, atomic::AtomicU32}};
use peakrunner_core::sim::Snapshot;
use peakrunner_protocol::{delta::{self, Prepared}, packets};
use crate::{proto::{ClientMsg, ServerMsg}, wire::invalid};

/// From the game thread to one relay.
pub enum Outbound {
    Control(Box<ServerMsg>),
    /// One snapshot, shared by every peer with this view.
    Snapshot(Arc<Frame>),
}

/// Deltas a frame keeps, one per distinct baseline in use.
const DELTAS: usize = 48;
type Delta = Arc<OnceLock<Option<Arc<Vec<u8>>>>>;

/// One snapshot view, prepared once and shared by its relays.
pub struct Frame {
    pub tick: u64,
    prepared: Prepared,
    full: Arc<Vec<u8>>,
    /// Each person's exact own state, by net ID (`delta::own_state`).
    own: Vec<(u32, Vec<u8>)>,
    /// Encodings against baseline frames, made by the first relay that needs one.
    deltas: Mutex<Vec<(Weak<Frame>, Delta)>>,
}

impl Frame {
    /// Trims the snapshot until its full encoding fits (`packets::prepare_fitting`).
    pub fn new(mut state: Snapshot) -> io::Result<Self> {
        let (prepared, full) = packets::prepare_fitting(&mut state)?;
        let own = state.players.iter().enumerate().filter(|(_, p)| p.net_id != 0 && !p.is_bot)
            .map(|(slot, p)| (p.net_id, delta::own_state(&prepared, slot, p))).collect();
        Ok(Self { tick: state.tick, prepared, full: Arc::new(full), own, deltas: Mutex::new(Vec::new()) })
    }

    /// The message for one peer: a delta against `base` when there is one
    /// and it is smaller than the full snapshot, and `player`'s own state.
    pub fn message(&self, base: Option<&Arc<Frame>>, player: Option<u32>) -> Vec<u8> {
        let own = player.and_then(|id| self.own.iter().find(|(p, _)| *p == id)).map_or(&[][..], |(_, o)| o);
        match base.filter(|b| b.tick < self.tick).and_then(|b| self.delta(b).map(|d| (b.tick, d))) {
            Some((base, body)) => packets::message(self.tick - base, &body, own),
            None => packets::message(0, &self.full, own),
        }
    }

    fn delta(&self, base: &Arc<Frame>) -> Option<Arc<Vec<u8>>> {
        let cell = {
            let mut deltas = self.deltas.lock().unwrap();
            deltas.retain(|(frame, _)| frame.strong_count() > 0);
            match deltas.iter().find(|(frame, _)| frame.as_ptr() == Arc::as_ptr(base)) {
                Some((_, cell)) => cell.clone(),
                None => {
                    let cell = Delta::default();
                    if deltas.len() < DELTAS { deltas.push((Arc::downgrade(base), cell.clone())); }
                    cell
                }
            }
        };
        // Encoded outside the lock: relays with other baselines don't wait.
        cell.get_or_init(|| {
            let body = self.prepared.encode(Some(&base.prepared));
            (body.len() < self.full.len()).then(|| Arc::new(body))
        }).clone()
    }
}

/// Messages a relay may queue for the game thread before it drops inputs.
const INBOUND: usize = 64;
/// Outbound messages a relay may fall behind by; snapshots beyond it are skipped.
const OUTBOUND: usize = 8;

/// The game thread's end of a relay session.
pub(crate) struct LocalPeer {
    pub(crate) inbound: mpsc::Receiver<ClientMsg>,
    pub(crate) outbound: tokio::sync::mpsc::Sender<Outbound>,
    /// Address group the player connected from (for one vote per address).
    pub(crate) origin: Option<IpAddr>,
    /// The relay's measured transport RTT (ms), copied into snapshot `pings`.
    pub(crate) rtt: Arc<AtomicU32>,
}

/// A relay's end: messages in, controls and snapshots out. Dropping it ends
/// the session in the game thread.
pub struct Link {
    pub inbound: mpsc::SyncSender<ClientMsg>,
    pub outbound: tokio::sync::mpsc::Receiver<Outbound>,
    /// Keep current with the connection's RTT in milliseconds.
    pub rtt: Arc<AtomicU32>,
}

/// Opens in-process sessions to a `GameHost` (see `GameHost::connector`).
#[derive(Clone)]
pub struct Connector {
    pub(crate) joins: mpsc::SyncSender<LocalPeer>,
}

impl Connector {
    /// A new session; the first message sent must be the client's Hello.
    pub fn connect(&self, origin: Option<IpAddr>, rtt_ms: u32) -> io::Result<Link> {
        let (in_tx, in_rx) = mpsc::sync_channel(INBOUND);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel(OUTBOUND);
        let rtt = Arc::new(AtomicU32::new(rtt_ms));
        self.joins.try_send(LocalPeer { inbound: in_rx, outbound: out_tx, origin, rtt: rtt.clone() })
            .map_err(|_| invalid("server is busy; try again"))?;
        Ok(Link { inbound: in_tx, outbound: out_rx, rtt })
    }
}

/// An address's group: IPv4 as is, IPv6 by its /64 (one host's usual share).
pub fn address_group(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => { let s = v6.segments(); IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0)) }
        v4 => v4,
    }
}

/// `address_group` as a vote key.
pub(crate) fn group_key(ip: IpAddr) -> u128 {
    match address_group(ip) {
        IpAddr::V4(a) => u128::from(a.to_ipv6_mapped()),
        IpAddr::V6(a) => u128::from(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::{sim::{Command, Match, MAX_PLAYERS}, terrain::MapId};
    use peakrunner_protocol::packets::{fragments, Receiver, MAX_DATAGRAM};

    #[test]
    fn a_frame_encodes_each_baseline_once_and_sends_exact_own_state() {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..MAX_PLAYERS { m.join(1 + i as u32, &format!("P{i}")).unwrap(); }
        let mut frames = Vec::new();
        for seq in 1..=12u64 {
            m.step(&(0..MAX_PLAYERS).map(|i| Some(Command { seq, move_z: 1.0, yaw: i as f32, jet: true, fire: true, ..Command::default() })).collect::<Vec<_>>());
            if seq % 3 == 0 { frames.push(Arc::new(Frame::new(m.snapshot()).unwrap())); }
        }
        let (first, last) = (frames[0].clone(), frames[3].clone());
        let mut rx = Receiver::default();
        let deliver = |rx: &mut Receiver, frame: &Frame, message: Vec<u8>| fragments(frame.tick, &message, MAX_DATAGRAM).unwrap()
            .iter().filter_map(|p| rx.accept(p).unwrap()).next();
        deliver(&mut rx, &first, first.message(None, Some(5))).expect("full snapshot");
        // Two peers with the same baseline share one encoding.
        let (a, b) = (last.message(Some(&first), Some(5)), last.message(Some(&first), Some(9)));
        assert_eq!(last.deltas.lock().unwrap().len(), 1);
        assert!(a.len() < last.message(None, Some(5)).len() && a[0] as u64 == last.tick - first.tick && b[0] == a[0]);
        let got = deliver(&mut rx, &last, a).expect("delta against the acknowledged snapshot");
        // The peer's own player is exact; the others are quantized.
        let exact = m.snapshot();
        let own = got.players.iter().position(|p| p.net_id == 5).unwrap();
        assert_eq!((got.players[own].pos, got.players[own].vel, got.players[own].energy),
            (exact.players[own].pos, exact.players[own].vel, exact.players[own].energy));
        assert!(got.players.iter().zip(&exact.players).any(|(g, e)| g.pos != e.pos));
        // A baseline no relay holds any more is forgotten; without one, full.
        drop(first); frames.remove(0);
        assert!(last.message(Some(&frames[1]), None)[0] > 0);
        assert!(last.deltas.lock().unwrap().iter().all(|(f, _)| f.strong_count() > 0));
        assert_eq!(last.message(None, None)[0], 0);
        // A spectator has no own state; once seated in a slot it does.
        m.join_spectator(500, "Watcher").unwrap();
        let watching = Frame::new(m.snapshot()).unwrap();
        assert_eq!(watching.message(None, Some(500)), watching.message(None, None));
        m.leave(3); m.unspectate(500, MAX_PLAYERS).unwrap();
        let seated = Frame::new(m.snapshot()).unwrap();
        assert!(seated.message(None, Some(500)).len() > seated.message(None, None).len());
        assert_eq!(last.message(Some(&last), None)[0], 0, "never against itself or a newer one");
    }

    #[test]
    fn ipv6_groups_by_64_and_ipv4_by_address() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::9".parse().unwrap();
        let c: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(address_group(a), address_group(b));
        assert_ne!(address_group(a), address_group(c));
        assert_ne!(group_key("198.51.100.7".parse().unwrap()), group_key("198.51.100.8".parse().unwrap()));
        assert_eq!(group_key("::ffff:198.51.100.7".parse().unwrap()), group_key("198.51.100.7".parse().unwrap()));
    }
}
