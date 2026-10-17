//! A server needs a map this game lacks (docs/modding.md, P2): the real
//! server app serves a copied arena from the content/ folder beside its
//! server.json, and a game client declines the download, then approves it,
//! downloads and joins, then joins again straight from its cache.
use peakrunner_core::{catalog, terrain::MapId};
use peakrunner_net::{connect_with, JoinOptions, Lobby, Session};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wait_for(session: &Session, what: &str, done: impl Fn(&Lobby) -> bool) -> Lobby {
    let start = Instant::now();
    loop {
        let lobby = session.lobby();
        if done(&lobby) { return lobby; }
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}: {lobby:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn copy_map(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for f in std::fs::read_dir(from).unwrap().filter_map(Result::ok).filter(|f| f.file_name() != "info.json") {
        std::fs::copy(f.path(), to.join(f.file_name())).unwrap();
    }
}

#[test]
fn a_missing_map_downloads_after_the_player_approves() {
    let tmp = std::env::temp_dir().join(format!("peakrunner-download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    // The server's content: Highgoal again under a new key, with its own name.
    let config = tmp.join("server");
    let arena = config.join("content/maps/test-arena");
    copy_map(catalog::entry(MapId::Highgoal).folder().expect("Highgoal installed"), &arena);
    std::fs::write(arena.join("info.json"), r#"{"name": "Test Arena", "note": "A copy for the download test."}"#).unwrap();
    // This game's downloads go here; it has no map called test-arena.
    catalog::set_cache_dir(tmp.join("cache"));
    assert!(MapId::parse("test-arena").is_none());

    let port = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    std::fs::write(config.join("server.json"), format!(r#"{{"schema": 1, "name": "Download test", "location": "",
        "rotation": [{{"map": "test-arena", "mode": "football"}}], "max_players": 8, "bots": 0, "port": {port},
        "announce": false, "upnp": false}}"#)).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_peakrunner-server"));
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|n| n.starts_with("PEAKRUNNER_")) { command.env_remove(name); }
    }
    let mut server = command.arg("--config-dir").arg(&config)
        .env("PEAKRUNNER_HOST_KEY", "ab".repeat(32)).env("RUST_LOG", "warn")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let pin = loop {
        let line = lines.next().expect("server exited before listening").unwrap();
        if let Some(pin) = line.strip_prefix(peakrunner_net::HOST_READY) { break pin.trim().to_string(); }
    };
    let address = format!("quic://127.0.0.1:{port}#sha256={pin}");
    let join = |auto_download| connect_with(&address, 0, "Tester", "", JoinOptions { spectate: false, auto_download, player: 0 }).unwrap();

    // Asked, and declined: nothing is downloaded and the join ends with why.
    let first = join(0);
    let asked = wait_for(&first, "the download question", |l| l.download.as_ref().is_some_and(|d| d.asking));
    let download = asked.download.unwrap();
    assert_eq!(download.maps.len(), 1);
    assert_eq!(download.maps[0].0, "Test Arena");
    assert!(download.total > 1_000_000, "{} bytes", download.total);
    first.answer_download(false);
    let ended = wait_for(&first, "the refusal", |l| l.error.is_some());
    assert!(ended.error.unwrap().contains("chose not to download"));
    assert!(!tmp.join("cache/maps/test-arena").exists());

    // Asked, and approved: downloaded, checked, and joined on the new map.
    let second = join(0);
    wait_for(&second, "the download question", |l| l.download.as_ref().is_some_and(|d| d.asking));
    second.answer_download(true);
    let joined = wait_for(&second, "joining", |l| l.connected || l.error.is_some());
    assert!(joined.error.is_none(), "{:?}", joined.error);
    assert_eq!(joined.map, "Test Arena");
    let fingerprint = catalog::entry(MapId::Highgoal).fingerprint().unwrap().to_string();
    let cached = tmp.join("cache/maps/test-arena").join(&fingerprint);
    assert!(cached.join("map.json").is_file() && cached.join("info.json").is_file());
    second.leave();
    std::thread::sleep(Duration::from_millis(300));
    // Outside a session, a download never answers the key.
    assert!(MapId::parse("test-arena").is_none(), "downloads stay out of menus");

    // Next time it joins from the cache without asking.
    let third = join(0);
    let joined = wait_for(&third, "joining from the cache", |l| l.connected || l.error.is_some() || l.download.is_some());
    assert!(joined.download.is_none() && joined.error.is_none(), "{joined:?}");
    third.leave();

    drop(server.stdin.take());
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&tmp);
}
