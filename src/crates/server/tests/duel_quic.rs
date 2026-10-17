//! A duel server over QUIC, as players reach it (docs/duels.md): the real
//! server app, a pinned game client that stays joined while its duel board
//! comes and changes (the board is a control message on the session's
//! stream), duels a bot and comes back to the lobby's tower.
use peakrunner_core::sim::duel::Place;
use peakrunner_net::{connect_with, JoinOptions, Lobby, Session};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wait_for(session: &Session, what: &str, done: impl Fn(&Lobby) -> bool) -> Lobby {
    let start = Instant::now();
    loop {
        let lobby = session.lobby();
        assert!(lobby.error.is_none(), "the session ended waiting for {what}: {:?}", lobby.error);
        if done(&lobby) { return lobby; }
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}: {lobby:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_duel_server_keeps_its_quic_players_through_board_changes() {
    let config = std::env::temp_dir().join(format!("peakrunner-duel-quic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&config);
    std::fs::create_dir_all(&config).unwrap();
    let port = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(config.join("server.json"), format!(r#"{{"schema": 1, "name": "Duel test", "location": "",
        "rotation": [{{"map": "old-holler", "mode": "duel"}}], "max_players": 8, "bots": 2, "port": {port},
        "announce": false, "upnp": false}}"#)).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_peakrunner-server"));
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|n| n.starts_with("PEAKRUNNER_")) { command.env_remove(name); }
    }
    let mut server = command.arg("--config-dir").arg(&config)
        .env("PEAKRUNNER_HOST_KEY", "cd".repeat(32)).env("RUST_LOG", "warn")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let pin = loop {
        let line = lines.next().expect("server exited before listening").unwrap();
        if let Some(pin) = line.strip_prefix(peakrunner_net::HOST_READY) { break pin.trim().to_string(); }
    };
    std::thread::spawn(move || for _ in lines {});
    let address = format!("quic://127.0.0.1:{port}#sha256={pin}");
    let session = connect_with(&address, 0, "Tester", "", JoinOptions { spectate: false, auto_download: 0, player: 0 }).unwrap();

    // Joined in the tower, with the board.
    let joined = wait_for(&session, "the duel board", |l| l.connected && l.duels.is_some());
    assert_eq!(joined.map, "Tenfold Tower");
    let me = joined.player_id;
    let board = joined.duels.unwrap();
    assert!(board.people.iter().any(|p| p.id == me && p.place == Place::Lobby), "{board:?}");
    let bot = board.bots.first().expect("bots on the board").clone();

    // The board changes as the duel starts: the session stays.
    assert!(session.challenge_bot(&bot));
    let dueling = wait_for(&session, "the duel", |l| l.duels.as_ref()
        .is_some_and(|b| b.people.iter().any(|p| p.id == me && p.place == Place::Dueling)));
    assert_eq!(dueling.duels.unwrap().duels.len(), 1);
    // Still joined a while later, snapshots flowing.
    std::thread::sleep(Duration::from_secs(2));
    let later = wait_for(&session, "snapshots in the duel", |l| l.snapshot.is_some());
    assert!(later.connected);

    drop(session);
    let _ = server.kill();
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&config);
}
