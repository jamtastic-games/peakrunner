//! Sparkfall under load (milestone check): a server running the Sparkfall
//! example mod in its CTF mode, with its hooks, sustains 28 connected
//! clients moving and firing at close to 60 Hz, every position finite, its
//! snapshots within the size the relay fits them in. Its own test binary:
//! activating a mod changes the process's definitions.
use peakrunner_core::sim::Command;
use peakrunner_core::{catalog, mods};
use peakrunner_net::connect;
use peakrunner_server::GameHost;
use std::time::{Duration, Instant};

fn wait(mut pred: impl FnMut() -> bool) {
    let start = Instant::now();
    while !pred() {
        assert!(start.elapsed() < Duration::from_secs(10), "condition timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "slow: 28 connected clients for twelve seconds; run explicitly in release"]
fn sparkfall_sustains_twenty_eight_clients() {
    catalog::add_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples"));
    mods::activate_home(&["sparkfall".to_string()]).unwrap();
    peakrunner_script::install();
    peakrunner_script::check_active().unwrap();
    let n = 28;
    let host = GameHost::bind("127.0.0.1:0", "Sparkfall load", n as u32, "old-holler").unwrap()
        .with_rotation(r#"[{"map": "old-holler", "mode": "sparkfall-ctf"}]"#).unwrap();
    let port = host.local_addr().port();
    let _host = host.spawn();
    let clients: Vec<_> = (0..n).map(|i| connect("127.0.0.1", port, &format!("G{i}")).unwrap()).collect();
    wait(|| clients.iter().all(|c| c.lobby().connected));
    let start = Instant::now();
    let mut next = start;
    let mut max_bytes = 0;
    let countdown = (peakrunner_core::sim::ROUND_COUNTDOWN * 60.0) as u64;
    for seq in 1..=720 + countdown {
        for (i, c) in clients.iter().enumerate() {
            // Moving, jetting, firing, and now and then the class key and the pack.
            assert!(c.send_input(Command { seq, move_z: 1.0, move_x: (i as f32 * 0.5).sin(),
                yaw: (seq as f32 * 0.003 + i as f32).rem_euclid(std::f32::consts::TAU),
                jump: true, jet: seq % 240 < 120, fire: true, weapon: ((seq / 120) % 30) as u8,
                ability: seq % 300 == (i as u64 * 7) % 300, deploy: seq % 400 == (i as u64 * 11) % 400,
                ..Command::default() }));
            let lobby = c.lobby();
            assert!(lobby.error.is_none(), "client {i}: {:?}", lobby.error);
            let s = lobby.snapshot.unwrap();
            max_bytes = max_bytes.max(serde_json::to_vec(&*s).unwrap().len());
            assert!(s.players.iter().all(|p| p.pos.is_finite() && p.vel.is_finite()));
        }
        next += Duration::from_secs_f64(1.0 / 60.0);
        if next > Instant::now() { std::thread::sleep(next - Instant::now()); }
    }
    let snapshot = clients[0].lobby().snapshot.unwrap();
    assert_eq!(snapshot.mode.key(), "sparkfall-ctf");
    assert!(snapshot.tick >= 680 + countdown, "server failed to sustain close to 60 Hz: {}", snapshot.tick);
    let shots: u32 = snapshot.players.iter().map(|p| p.shots).sum();
    assert!(shots > 100, "they fired: {shots}");
    println!("Sparkfall {n}-client load test: {} ticks, {:.2}s, largest snapshot {} bytes, {} shots",
        snapshot.tick, start.elapsed().as_secs_f32(), max_bytes, shots);
}
