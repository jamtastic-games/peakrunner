//! Real transport load check: PEAKRUNNER_TEST_URL=quic://... cargo run -p
//! peakrunner-net --example public_smoke --release -- 8 30
use std::time::{Instant, Duration};
use peakrunner_core::sim::{Command, Phase};
fn main() {
    let endpoint = std::env::var("PEAKRUNNER_TEST_URL").unwrap_or("quic://play.peakrunner.net:7777".into());
    let mut args = std::env::args().skip(1);
    let count = args.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(2).clamp(2, 8);
    // Long enough to see play begin after a new round's 10 s countdown.
    let seconds = args.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(15).clamp(5, 120);
    let clients: Vec<_> = (0..count).map(|i| peakrunner_net::connect_private(&endpoint, 7777, &format!("NetCheck{i}"),
        &std::env::var("PEAKRUNNER_MATCH_PASSWORD").unwrap_or_default()).unwrap()).collect();
    let start = Instant::now();
    // People only: a bot server also lists its bots.
    let people = |s: &peakrunner_core::sim::Snapshot| s.players.iter().filter(|p| p.net_id != 0 && !p.is_bot).count();
    // Wait for our own clients (other people may be playing on a live server).
    while !clients.iter().all(|c| { let l = c.lobby(); l.connected && l.snapshot.as_ref().is_some_and(|s| people(s) >= count) }) {
        for c in &clients { assert!(c.lobby().error.is_none(), "{:?}", c.lobby().error); }
        assert!(start.elapsed() < Duration::from_secs(12), "join timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut next = Instant::now();
    let mut playing = false;
    let mut max_ack_gap = 0;
    let mut previous = vec![0; count];
    let mut delivered = vec![0u64; count];
    let mut last_snapshot = vec![Instant::now(); count];
    let mut max_snapshot_gap = Duration::ZERO;
    for seq in 1..=seconds * 60 {
        for (i, client) in clients.iter().enumerate() {
            assert!(client.send_input(Command { seq, move_z: 1.0, move_x: if i % 2 == 0 { -0.5 } else { 0.5 },
                yaw: i as f32 + seq as f32 * 0.004, jump: true, jet: seq % 240 < 100,
                fire: true, weapon: ((seq / 120) % 3) as u8, ..Command::default() }));
            let lobby = client.lobby();
            assert!(lobby.connected && lobby.error.is_none(), "client {i}: {:?}", lobby.error);
            let state = lobby.snapshot.unwrap();
            if state.tick > previous[i] {
                previous[i] = state.tick; delivered[i] += 1;
                max_snapshot_gap = max_snapshot_gap.max(last_snapshot[i].elapsed());
                last_snapshot[i] = Instant::now();
            }
            assert!(people(&state) >= count);
            assert!(state.players.iter().all(|p| p.pos.is_finite() && p.vel.is_finite()));
            let slot = state.players.iter().position(|p| p.net_id == lobby.player_id).unwrap();
            max_ack_gap = max_ack_gap.max(seq.saturating_sub(state.acks[slot]));
            playing |= state.phase == Phase::Playing;
        }
        next += Duration::from_secs_f64(1.0 / 60.0);
        if next > Instant::now() { std::thread::sleep(next - Instant::now()); }
    }
    assert!(playing, "match never started");
    assert!(max_ack_gap < 120, "input acknowledgement lag exceeded two seconds: {max_ack_gap} ticks; snapshots={delivered:?}; max snapshot gap={max_snapshot_gap:?}");
    assert!(max_snapshot_gap < Duration::from_secs(2), "snapshot stalls exceeded two seconds: {max_snapshot_gap:?}");
    println!("PASS: {count} clients, {seconds}s, shared active match, all weapons/movement, max ack gap {max_ack_gap} ticks, max snapshot gap {}ms, snapshots per client {:?}", max_snapshot_gap.as_millis(), delivered);
}
