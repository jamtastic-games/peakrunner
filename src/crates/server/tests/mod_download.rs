//! A server runs mods this game lacks (docs/modding.md, P4): the real server
//! app runs Low Gravity (a definition change), Quick CTF (a mode variant)
//! and Drain Gun (an engine class, which players once couldn't download)
//! from its content folder; a game client downloads both, plays the session
//! with exactly the server's definitions and mode, and is back to the base
//! game when it leaves.
use peakrunner_core::{catalog, defs::defs, map_catalog::SupportedMode};
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
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for {what}: {:?} {:?}", lobby.error, lobby.download);
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for f in std::fs::read_dir(from).unwrap().filter_map(Result::ok) {
        if f.path().is_dir() { copy(&f.path(), &to.join(f.file_name())); } else { std::fs::copy(f.path(), to.join(f.file_name())).unwrap(); }
    }
}

#[test]
fn a_server_s_mods_download_play_and_leave_no_trace() {
    let tmp = std::env::temp_dir().join(format!("peakrunner-mods-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mods");
    for m in ["low-gravity", "quick-ctf", "spree", "drain-gun"] { copy(&examples.join(m), &tmp.join("server-content/mods").join(m)); }
    catalog::set_cache_dir(tmp.join("cache"));
    let base_gravity = defs().movement.gravity;
    assert!(SupportedMode::parse("quick-ctf").is_none(), "this game has no such mode");

    let port = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let config = tmp.join("server");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("server.json"), format!(r#"{{"schema": 1, "name": "Mod test", "location": "",
        "rotation": [{{"map": "old-holler", "mode": "quick-ctf"}}, {{"map": "old-holler", "mode": "spree-tdm"}}], "max_players": 8,
        "bots": 0, "port": {port}, "announce": false, "upnp": false, "mods": ["low-gravity", "quick-ctf", "spree", "drain-gun"]}}"#)).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_peakrunner-server"));
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|n| n.starts_with("PEAKRUNNER_")) { command.env_remove(name); }
    }
    let mut server = command.arg("--config-dir").arg(&config)
        .env("PEAKRUNNER_HOST_KEY", "ab".repeat(32)).env("PEAKRUNNER_PACK_DIR", tmp.join("server-content")).env("RUST_LOG", "warn")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let pin = loop {
        let line = lines.next().expect("server exited before listening").unwrap();
        if let Some(pin) = line.strip_prefix(peakrunner_net::HOST_READY) { break pin.trim().to_string(); }
    };
    let address = format!("quic://127.0.0.1:{port}#sha256={pin}");

    let session = connect_with(&address, 0, "Modder", "", JoinOptions { spectate: false, auto_download: 0, player: 0 }).unwrap();
    let asked = wait_for(&session, "the download question", |l| l.download.as_ref().is_some_and(|d| d.asking));
    let names: Vec<String> = asked.download.unwrap().maps.into_iter().map(|m| m.0).collect();
    assert_eq!(names, ["Low Gravity (mod)", "Quick CTF (mod)", "Spree (mod)", "Drain Gun (mod)"], "the mods, named as mods; the map is installed");
    session.answer_download(true);
    let joined = wait_for(&session, "playing on the modded server", |l| l.snapshot.is_some() || l.error.is_some());
    assert!(joined.error.is_none(), "{:?}", joined.error);
    // The session plays by the server's definitions and mode.
    assert_eq!(defs().movement.gravity, 12.0);
    let mode = joined.snapshot.as_ref().unwrap().mode;
    assert_eq!((mode.key(), mode.label(), mode.def().score_limit), ("quick-ctf", "Quick CTF", 5));
    session.leave();
    // Left: the base game again, and the mod's mode is gone from menus.
    let start = Instant::now();
    while defs().movement.gravity != base_gravity && start.elapsed() < Duration::from_secs(5) { std::thread::sleep(Duration::from_millis(20)); }
    assert_eq!(defs().movement.gravity, base_gravity);
    assert!(SupportedMode::parse("quick-ctf").is_none() && !SupportedMode::every().iter().any(|m| m.key() == "quick-ctf"));
    assert!(tmp.join("cache/mods/low-gravity").is_dir() && tmp.join("cache/mods/quick-ctf").is_dir(), "kept for next time");
    // The class mod arrived with its class file.
    let drain: Vec<_> = std::fs::read_dir(tmp.join("cache/mods/drain-gun")).unwrap().filter_map(Result::ok).collect();
    assert!(drain.len() == 1 && drain[0].path().join("projectiles/drain.json").is_file(), "the class file downloaded");
    // A scripted mod arrives without its script: rules run on the server only.
    let spree: Vec<_> = std::fs::read_dir(tmp.join("cache/mods/spree")).unwrap().filter_map(Result::ok).collect();
    assert_eq!(spree.len(), 1);
    assert!(spree[0].path().join("modes/spree-tdm.json").is_file() && !spree[0].path().join("rules").exists(), "no script downloaded");

    drop(server.stdin.take());
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&tmp);
}
