use std::{collections::{BTreeMap, VecDeque}, io, time::{Duration, Instant}};
use peakrunner_core::sim::{Command, Snapshot, MAX_PLAYERS};
use serde::{Deserialize, Serialize};
use peakrunner_discovery::wire::invalid;
use crate::delta::{self, Prepared};

/// Snapshot datagrams: at least this big (every QUIC path carries it), at
/// most the largest a path-MTU-discovered connection may offer.
pub const MIN_DATAGRAM: usize = 1060;
pub const MAX_DATAGRAM: usize = 1452;
/// Fragment header: tick (8), index, count.
const HEADER: usize = 10;
const MAX_PARTS: usize = 12;
/// Largest message: what twelve minimum-size fragments carry.
pub const MAX_MESSAGE: usize = MAX_PARTS * (MIN_DATAGRAM - HEADER);
/// Largest snapshot body, leaving room for the message header and the
/// recipient's own-state trailer.
pub const MAX_BODY: usize = MAX_MESSAGE - 128;
/// Snapshots a client keeps as baselines; the server's relays keep fewer
/// (`HISTORY`), so every acknowledged one is still here.
const BASELINES: usize = 40;
/// Snapshots a server relay keeps as possible baselines (1.6 s at 20 Hz).
pub const HISTORY: usize = 32;

/// A client input datagram: the last three commands (redundancy against
/// loss) and the newest snapshot tick the client has decoded, its delta
/// baseline (0: none yet).
#[derive(Serialize, Deserialize)]
pub struct Inputs { pub count: u8, pub frames: [Command; 3], pub ack: u64 }
pub fn inputs(history: &VecDeque<Command>, ack: u64) -> Vec<u8> {
    let mut data = Inputs { count: history.len() as u8, frames: [Command::default(); 3], ack };
    for (i, command) in history.iter().enumerate() { data.frames[i] = *command; }
    postcard::to_allocvec(&data).expect("fixed input packet")
}
/// Commands and the acknowledged snapshot tick.
pub fn decode_inputs(bytes: &[u8]) -> io::Result<(Vec<Command>, u64)> {
    if bytes.len() > 256 { return Err(invalid("input packet too large")); }
    let data: Inputs = postcard::from_bytes(bytes).map_err(|_| invalid("invalid input packet"))?;
    if !(1..=3).contains(&data.count) { return Err(invalid("invalid input count")); }
    let frames = &data.frames[..data.count as usize];
    if frames.iter().any(|c| !c.valid()) || frames.windows(2).any(|w| w[0].seq >= w[1].seq) {
        return Err(invalid("invalid input sequence"));
    }
    Ok((frames.to_vec(), data.ack))
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 { out.push(v as u8 | 0x80); v >>= 7; }
    out.push(v as u8);
}
fn get_varint(bytes: &[u8], at: &mut usize) -> io::Result<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *bytes.get(*at).ok_or_else(|| invalid("truncated snapshot header"))?;
        *at += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b < 0x80 { return Ok(v); }
    }
    Err(invalid("invalid snapshot header"))
}

/// A snapshot message: how many ticks back its baseline is (0: a full
/// snapshot), the encoded body, and the recipient's own-state trailer.
pub fn message(gap: u64, body: &[u8], own: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + own.len() + 8);
    put_varint(&mut out, gap); put_varint(&mut out, body.len() as u64);
    out.extend_from_slice(body); out.extend_from_slice(own);
    out
}

/// A message as datagrams no larger than `datagram` (the connection's
/// current maximum, clamped to MIN_DATAGRAM..=MAX_DATAGRAM).
pub fn fragments(tick: u64, message: &[u8], datagram: usize) -> io::Result<Vec<Vec<u8>>> {
    if message.is_empty() || message.len() > MAX_MESSAGE { return Err(invalid("snapshot limit exceeded")); }
    let chunk = datagram.clamp(MIN_DATAGRAM, MAX_DATAGRAM) - HEADER;
    let count = message.len().div_ceil(chunk);
    Ok(message.chunks(chunk).enumerate().map(|(i, part)| {
        let mut packet = Vec::with_capacity(part.len() + HEADER);
        packet.extend_from_slice(&tick.to_le_bytes());
        packet.push(i as u8); packet.push(count as u8); packet.extend_from_slice(part); packet
    }).collect())
}

/// A full snapshot as minimum-size datagrams, without an own-state trailer
/// (tests and tools).
pub fn chunks(snapshot: &Snapshot) -> io::Result<Vec<Vec<u8>>> {
    let body = Prepared::new(snapshot).encode(None);
    if body.len() > MAX_BODY { return Err(invalid("snapshot limit exceeded")); }
    fragments(snapshot.tick, &message(0, &body, &[]), MIN_DATAGRAM)
}

/// Prepare a snapshot view and encode it in full, trimming what matters
/// least until the full encoding fits, so a busy scene degrades instead of
/// failing (which would skip the snapshot for everyone on the same tick,
/// and a scene that stays over the cap would time every client out). Deltas
/// against it are smaller than this or sent in full instead. In order: smoke
/// puffs, the kill feed and chat (to the newest few), explosion echoes, loot
/// packs; then, only if projectiles alone overflow, the oldest half of the
/// projectiles at a time (clients only draw them; hits are the server's),
/// and last everything but the players, flags, ball and control points.
/// Players are never cut, so a snapshot always goes out.
pub fn prepare_fitting(snapshot: &mut Snapshot) -> io::Result<(Prepared, Vec<u8>)> {
    let mut step = 0;
    loop {
        let prepared = Prepared::new(snapshot);
        let body = prepared.encode(None);
        if body.len() <= MAX_BODY { return Ok((prepared, body)); }
        match step {
            0 => snapshot.smoke.truncate(snapshot.smoke.len() / 4),
            1 => { let n = snapshot.feed.len(); if n > 4 { snapshot.feed.drain(..n - 4); } }
            2 => snapshot.smoke.clear(),
            3 => snapshot.explosions.retain(|e| e.age < 0.15),
            4 => snapshot.loot.clear(),
            // Projectiles are in firing order: drop the oldest half, again
            // and again, while there are any.
            5 if !snapshot.discs.is_empty() => { let n = snapshot.discs.len().div_ceil(2); snapshot.discs.drain(..n); continue; }
            5 => {}
            6 => { snapshot.explosions.clear(); snapshot.feed.clear(); snapshot.deployables.clear(); snapshot.vote_choices.clear(); }
            _ => return Err(invalid("snapshot limit exceeded")),
        }
        step += 1;
    }
}

struct Assembly { born: Instant, parts: Vec<Option<Vec<u8>>> }
/// Rebuilds messages from fragments: newest first, at most four incomplete.
#[derive(Default)]
pub struct Reassembly { frames: BTreeMap<u64, Assembly>, completed: u64 }
impl Reassembly {
    /// A complete message and its tick.
    pub fn accept(&mut self, packet: &[u8]) -> io::Result<Option<(u64, Vec<u8>)>> {
        if packet.len() <= HEADER || packet.len() > MAX_DATAGRAM { return Err(invalid("invalid snapshot fragment")); }
        let tick = u64::from_le_bytes(packet[..8].try_into().unwrap());
        let index = packet[8] as usize; let count = packet[9] as usize;
        if count == 0 || count > MAX_PARTS || index >= count { return Err(invalid("invalid fragment count")); }
        if tick <= self.completed { return Ok(None); }
        self.frames.retain(|_, a| a.born.elapsed() < Duration::from_millis(250));
        if !self.frames.contains_key(&tick) && self.frames.len() >= 4 {
            if tick < *self.frames.first_key_value().unwrap().0 { return Ok(None); }
            self.frames.pop_first();
        }
        let frame = self.frames.entry(tick).or_insert_with(|| Assembly { born: Instant::now(), parts: vec![None; count] });
        if frame.parts.len() != count { return Err(invalid("inconsistent snapshot fragments")); }
        frame.parts[index] = Some(packet[HEADER..].to_vec());
        if frame.parts.iter().any(Option::is_none) { return Ok(None); }
        let bytes: Vec<u8> = frame.parts.iter().flat_map(|p| p.as_ref().unwrap().iter().copied()).collect();
        if bytes.len() > MAX_MESSAGE { return Err(invalid("snapshot limit exceeded")); }
        self.completed = tick; self.frames.retain(|key, _| *key > tick);
        Ok(Some((tick, bytes)))
    }
    pub fn pending_frames(&self) -> usize { self.frames.len() }
}

/// The client's snapshot stream: reassembles, decodes deltas against the
/// snapshots it kept, and knows which tick to acknowledge.
#[derive(Default)]
pub struct Receiver {
    reassembly: Reassembly,
    baselines: VecDeque<Prepared>,
    /// Deltas whose baseline was already gone (skipped, not fatal).
    pub unmatched: u64,
}
impl Receiver {
    /// A complete snapshot. Corrupt data is an error; a delta against a
    /// baseline this client no longer has is skipped.
    pub fn accept(&mut self, packet: &[u8]) -> io::Result<Option<Snapshot>> {
        let Some((tick, bytes)) = self.reassembly.accept(packet)? else { return Ok(None) };
        let mut at = 0;
        let gap = get_varint(&bytes, &mut at)?;
        let len = get_varint(&bytes, &mut at)?;
        let end = at.checked_add(usize::try_from(len).map_err(|_| invalid("invalid snapshot length"))?)
            .filter(|end| *end <= bytes.len()).ok_or_else(|| invalid("invalid snapshot length"))?;
        let base = match gap {
            0 => None,
            gap => match tick.checked_sub(gap).and_then(|t| self.baselines.iter().find(|b| b.tick == t)) {
                Some(base) => Some(base),
                None => { self.unmatched += 1; return Ok(None); }
            },
        };
        let prepared = Prepared::decode(tick, &bytes[at..end], base)?;
        let mut snapshot = prepared.to_snapshot()?;
        if end < bytes.len() { delta::apply_own(&mut snapshot, &bytes[end..])?; }
        if snapshot.players.len() != MAX_PLAYERS || snapshot.acks.len() != MAX_PLAYERS { return Err(invalid("invalid snapshot identity")); }
        self.baselines.push_back(prepared);
        while self.baselines.len() > BASELINES { self.baselines.pop_front(); }
        Ok(Some(snapshot))
    }
    /// The newest snapshot decoded: the baseline to acknowledge.
    pub fn acked(&self) -> u64 { self.baselines.back().map_or(0, |b| b.tick) }
    pub fn pending_frames(&self) -> usize { self.reassembly.pending_frames() }
}

#[cfg(test)]
mod size_probe {
    use super::*;
    use peakrunner_core::{sim::Match, terrain::MapId};
    /// Snapshot sizes in a busy full match: the previous LZ4 full snapshots
    /// against quantized full and delta snapshots. Run with --release
    /// --ignored --nocapture.
    #[test]
    #[ignore = "measurement"]
    fn snapshot_size_probe() {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..MAX_PLAYERS { m.join(100 + i as u32, &format!("Player {i}")).unwrap(); }
        let (mut max_raw, mut max_lz, mut max_full, mut players_raw, mut lz_sum, mut full_sum) = (0, 0, 0, 0, 0, 0);
        let (mut deltas, mut delta_max, mut delta_sum, mut delta2_sum, mut singles) = (0, 0, 0, 0, 0);
        let mut history: Vec<Prepared> = Vec::new();
        let (mut prepare_ns, mut delta_ns, mut encodes) = (0u128, 0u128, 0u32);
        for seq in 1..=1200u64 {
            let commands: Vec<Option<Command>> = (0..MAX_PLAYERS).map(|i| Some(Command { seq, move_z: 1.0,
                move_x: if i % 2 == 0 { -0.5 } else { 0.5 }, yaw: i as f32 + seq as f32 * 0.01, jet: seq % 240 < 100,
                fire: true, weapon: ((seq / 120) % 3) as u8, ..Command::default() })).collect();
            m.step(&commands);
            if seq % 3 != 0 { continue; }
            let s = m.snapshot_for(0);
            let raw = postcard::to_allocvec(&s).unwrap();
            let lz = lz4_flex::compress_prepend_size(&raw).len();
            max_raw = max_raw.max(raw.len()); max_lz = max_lz.max(lz); lz_sum += lz;
            players_raw = players_raw.max(postcard::to_allocvec(&s.players).unwrap().len());
            let t = Instant::now();
            let cur = Prepared::new(&s);
            let full = cur.encode(None);
            let prepared = t.elapsed().as_nanos();
            max_full = max_full.max(full.len()); full_sum += full.len();
            if let Some(base) = history.last() {
                let t = Instant::now();
                let d = cur.encode(Some(base));
                prepare_ns += prepared; delta_ns += t.elapsed().as_nanos(); encodes += 1;
                deltas += 1; delta_max = delta_max.max(d.len()); delta_sum += d.len();
                if message(1, &d, &[0; 12]).len() <= 1400 - HEADER { singles += 1; }
            }
            if history.len() >= 2 { delta2_sum += cur.encode(Some(&history[history.len() - 2])).len(); }
            history.push(cur);
        }
        let dg = |n: usize, mtu: usize| n.div_ceil(mtu - HEADER);
        println!("{} players, {} snapshots:", MAX_PLAYERS, deltas);
        let n = history.len();
        println!("  before: max raw postcard {max_raw} B (players {players_raw} B), LZ4 mean {} B, max {max_lz} B = {} datagrams of {MIN_DATAGRAM} B; mean {:.0} kbit/s per client",
            lz_sum / n, dg(max_lz, MIN_DATAGRAM), (lz_sum / n) as f64 * 160.0 / 1000.0);
        println!("  full quantized: mean {} B, max {max_full} B = {} datagrams of {MIN_DATAGRAM} B", full_sum / n, dg(max_full, MIN_DATAGRAM));
        println!("  delta vs previous: mean {} B, max {delta_max} B; vs two back: mean {} B; {:.0}% fit one 1400 B datagram",
            delta_sum / deltas, delta2_sum / (deltas - 1), singles as f64 * 100.0 / deltas as f64);
        println!("  delta at 20 Hz: mean {:.0} kbit/s, worst {:.0} kbit/s per client (plus 10 B header per datagram and UDP/QUIC overhead)",
            delta_sum as f64 / deltas as f64 * 160.0 / 1000.0, delta_max as f64 * 160.0 / 1000.0);
        println!("  server: prepare + full encode {:.1} us per view, each delta {:.1} us", prepare_ns as f64 / encodes as f64 / 1000.0,
            delta_ns as f64 / encodes as f64 / 1000.0);
        let (last, prev) = (&history[history.len() - 1], &history[history.len() - 2]);
        let d = last.encode(Some(prev));
        let t = Instant::now();
        for _ in 0..200 { std::hint::black_box(Prepared::decode(last.tick, &d, Some(prev)).unwrap().to_snapshot().unwrap()); }
        println!("  client decode + to_snapshot: {:.1} us", t.elapsed().as_secs_f64() * 1e6 / 200.0);
    }
}

#[cfg(test)]
mod fit_tests {
    use super::*;
    use peakrunner_core::sim::{deploy::{Deployable, DeployKind}, loadout::Loot, Disc, Explosion, Match, SmokePuff, Team};
    use peakrunner_core::terrain::MapId;
    use glam::Vec3;

    /// Every cap at once: all deployables standing, full mines, loot, smoke
    /// and explosions, a feed of long chat, and a busy sky. Worst-case noise
    /// (hashed positions) so compression can't hide it.
    #[test]
    fn a_snapshot_at_every_cap_still_fits_and_keeps_all_gameplay_state() {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..MAX_PLAYERS { m.join(100 + i as u32, &format!("Player {i}")).unwrap(); }
        let mut s = m.snapshot();
        let r = |i: usize, k: f32| ((i as f32 * 12.9898 + k * 78.233).sin() * 43_758.547).fract() * 1000.0;
        let kinds = [(DeployKind::Turret, 4), (DeployKind::Wall, 8), (DeployKind::Field, 8), (DeployKind::Ammo, 8)];
        for team in [Team::Ember, Team::Glacier] {
            for (kind, n) in kinds {
                for i in 0..n {
                    s.deployables.push(Deployable { owner: 0, kind, team, pos: Vec3::new(r(i, 1.0), r(i, 2.0), r(i, 3.0)), yaw: r(i, 4.0),
                        health: r(i, 5.0), cooldown: r(i, 6.0), aim: Vec3::new(r(i, 7.0), r(i, 8.0), r(i, 9.0)), age: 0.0 });
                }
            }
        }
        for i in 0..24 { s.loot.push(Loot { pos: Vec3::new(r(i, 10.0), r(i, 11.0), r(i, 12.0)), ammo: vec![5, 90, 4, 0], age: r(i, 13.0), grenades: 2, ..Default::default() }); }
        // Beyond the smoke and explosion caps, to force the scene over the
        // limit whatever the fight looks like; trimming must still save it.
        for i in 0..700 { s.smoke.push(SmokePuff { pos: Vec3::new(r(i, 14.0), r(i, 15.0), r(i, 16.0)), age: r(i, 17.0) }); }
        for i in 0..120 { s.explosions.push(Explosion { pos: Vec3::new(r(i, 18.0), r(i, 19.0), r(i, 20.0)), age: 0.05 + r(i, 21.0) / 5000.0, max_r: 9.0, kind: 2 }); }
        for i in 0..90 { s.discs.push(Disc { pos: Vec3::new(r(i, 22.0), r(i, 23.0), r(i, 24.0)), vel: Vec3::new(r(i, 25.0), r(i, 26.0), r(i, 27.0)),
            team: Team::Ember, owner: i % MAX_PLAYERS, life: 1.0, kind: (i % 8) as u8, spin: 0.0, target: 0, power: 1.0 }); }
        for k in 0..16 {
            let noise: String = (0..240).map(|i| (b'!' + (r(i + k * 240, 28.0) as u8 % 90)) as char).collect();
            s.feed.push(peakrunner_core::feed::Entry::Chat { sender: format!("Griefer {k}"), text: noise });
        }
        let raw = Prepared::new(&s).encode(None).len();
        eprintln!("untrimmed worst case: {raw} bytes (cap {MAX_BODY})");
        assert!(chunks(&s).is_err(), "the scene is over the cap untrimmed");
        let (players, discs, deploys) = (s.players.len(), s.discs.len(), s.deployables.len());
        let (_, body) = prepare_fitting(&mut s).expect("trimmed to fit");
        assert!(fragments(s.tick, &message(0, &body, &[0; 100]), MIN_DATAGRAM).unwrap().len() <= MAX_PARTS);
        assert_eq!((s.players.len(), s.discs.len(), s.deployables.len()), (players, discs, deploys), "gameplay state kept");
    }

    /// Projectiles alone over the cap (a sky full of chaingun rounds): the
    /// oldest go until it fits, so a snapshot is still sent to everyone.
    #[test]
    fn a_projectile_flood_is_trimmed_oldest_first_and_still_sends() {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..MAX_PLAYERS { m.join(100 + i as u32, &format!("Player {i}")).unwrap(); }
        let mut s = m.snapshot();
        s.tick = 1;
        let r = |i: usize, k: f32| ((i as f32 * 12.9898 + k * 78.233).sin() * 43_758.547).fract() * 1000.0;
        for i in 0..2000 { s.discs.push(Disc { pos: Vec3::new(r(i, 1.0), r(i, 2.0), r(i, 3.0)), vel: Vec3::new(r(i, 4.0), r(i, 5.0), r(i, 6.0)),
            team: Team::Glacier, owner: i % MAX_PLAYERS, life: i as f32, kind: 1, spin: 0.0, target: 0, power: 1.0 }); }
        assert!(chunks(&s).is_err(), "projectiles alone are over the cap");
        let (_, body) = prepare_fitting(&mut s).expect("a snapshot is always sent");
        assert_eq!(s.players.len(), MAX_PLAYERS);
        assert!(!s.discs.is_empty() && s.discs.len() < 2000);
        assert_eq!(s.discs.last().unwrap().life, 1999.0, "the newest projectiles are kept");
        let packets = fragments(s.tick, &message(0, &body, &[]), MIN_DATAGRAM).unwrap();
        assert!(packets.len() <= MAX_PARTS);
        let mut receiver = Receiver::default();
        let decoded = packets.iter().filter_map(|p| receiver.accept(p).unwrap()).next().unwrap();
        assert_eq!(decoded.discs.len(), s.discs.len());
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use peakrunner_core::{sim::Match, terrain::MapId};

    fn busy_match() -> Match {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..MAX_PLAYERS { m.join(100 + i as u32, &format!("Player {i}")).unwrap(); }
        m
    }
    fn step(m: &mut Match, seq: u64) {
        let commands: Vec<Option<Command>> = (0..MAX_PLAYERS).map(|i| Some(Command { seq, move_z: 1.0, yaw: i as f32 + seq as f32 * 0.01,
            jet: seq % 240 < 100, fire: true, weapon: ((seq / 120) % 3) as u8, ..Command::default() })).collect();
        m.step(&commands);
    }
    fn json(s: &Snapshot) -> String { serde_json::to_string(s).unwrap() }

    /// A relay's side, as `server::quic` does it: keep what was sent, send
    /// deltas against the newest acknowledged, fall back to full.
    struct Sender { sent: VecDeque<Prepared>, acked: u64 }
    impl Sender {
        fn send(&mut self, cur: Prepared, datagram: usize) -> Vec<Vec<u8>> {
            let base = self.sent.iter().find(|b| b.tick == self.acked);
            let (gap, body) = match base { Some(b) => (cur.tick - b.tick, cur.encode(Some(b))), None => (0, cur.encode(None)) };
            let out = fragments(cur.tick, &message(gap, &body, &[]), datagram).unwrap();
            self.sent.push_back(cur);
            while self.sent.len() > HISTORY { self.sent.pop_front(); }
            out
        }
    }

    #[test]
    fn baselines_advance_with_acks_and_fall_back_to_full() {
        let mut m = busy_match();
        let mut tx = Sender { sent: VecDeque::new(), acked: 0 };
        let mut rx = Receiver::default();
        let (mut sent, mut full) = (0, 0);
        for seq in 1..=300u64 {
            step(&mut m, seq);
            if seq % 3 != 0 { continue; }
            let s = m.snapshot();
            let packets = tx.send(Prepared::new(&s), 1400);
            sent += packets.iter().map(Vec::len).sum::<usize>(); full += chunks(&s).unwrap().iter().map(Vec::len).sum::<usize>();
            let got = packets.iter().filter_map(|p| rx.accept(p).unwrap()).next().expect("delivered");
            assert_eq!(json(&got), json(&Prepared::new(&s).to_snapshot().unwrap()));
            // Acks arrive one snapshot late.
            if seq % 6 == 0 { tx.acked = rx.acked(); }
        }
        assert!(full > 3 * sent, "deltas send {sent} B where full snapshots take {full} B");
        // The acknowledged baseline is evicted (a long silence): full again.
        tx.acked = 3;
        step(&mut m, 301);
        let full = tx.send(Prepared::new(&m.snapshot()), 1400);
        assert_eq!(full[0][HEADER], 0, "gap 0: a full snapshot");
        assert!(full.iter().filter_map(|p| rx.accept(p).unwrap()).next().is_some());
        // A delta against a baseline the client dropped is skipped, not fatal.
        let mut fresh = Receiver::default();
        tx.acked = tx.sent.back().unwrap().tick;
        step(&mut m, 302); m.tick += 2;
        let delta = tx.send(Prepared::new(&m.snapshot()), 1400);
        assert!(delta.iter().all(|p| fresh.accept(p).unwrap().is_none()));
        assert_eq!(fresh.unmatched, 1);
    }

    #[test]
    fn datagram_size_follows_the_path_within_bounds() {
        let mut m = busy_match();
        for seq in 1..=60 { step(&mut m, seq); }
        let body = Prepared::new(&m.snapshot()).encode(None);
        let msg = message(0, &body, &[]);
        for (mtu, largest) in [(500, MIN_DATAGRAM), (1200, 1200), (1452, 1452), (9000, MAX_DATAGRAM)] {
            let packets = fragments(1, &msg, mtu).unwrap();
            assert_eq!(packets.iter().map(Vec::len).max().unwrap(), largest.min(msg.len() + HEADER));
            let mut rx = Receiver::default();
            assert!(packets.iter().filter_map(|p| rx.accept(p).unwrap()).next().is_some());
        }
        assert!(Receiver::default().accept(&vec![0; MAX_DATAGRAM + 1]).is_err());
    }

    /// Independent loss on the path: with deltas against the newest acked
    /// snapshot, a lost snapshot costs only itself.
    #[test]
    fn snapshots_survive_packet_loss() {
        for (loss, at_least) in [(0.01, 0.97), (0.02, 0.95), (0.05, 0.90)] {
            let mut m = busy_match();
            let mut tx = Sender { sent: VecDeque::new(), acked: 0 };
            let mut rx = Receiver::default();
            let mut rng = 0x1234_5678_9abc_u64;
            let mut lost = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; (rng % 10_000) as f64 / 10_000.0 < loss };
            let (mut sent, mut got, mut datagrams) = (0, 0, 0);
            for seq in 1..=1800u64 {
                step(&mut m, seq);
                // The client acks with every input (60 Hz); inputs are lost too.
                if !lost() { tx.acked = tx.acked.max(rx.acked()); }
                if seq % 3 != 0 { continue; }
                let packets = tx.send(Prepared::new(&m.snapshot()), 1400);
                sent += 1; datagrams += packets.len();
                for p in packets { if !lost() && rx.accept(&p).unwrap().is_some() { got += 1; } }
            }
            let delivered = got as f64 / sent as f64;
            eprintln!("{:.0}% loss: {:.1}% of snapshots delivered, {:.2} datagrams each", loss * 100.0, delivered * 100.0, datagrams as f64 / sent as f64);
            assert!(delivered >= at_least, "{loss}: {delivered}");
        }
    }
}
