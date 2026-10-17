//! Hosting from the game: the Host screen's settings, and the separate
//! `peakrunner-server` app it starts next to this executable. The client
//! never runs a match itself (docs/apps.md): it writes the server's own
//! `server.json` in a folder of its own, starts the server with a fresh host
//! key, shows what the server says, and joins it pinned to the key the server
//! reports over its private pipe. The server stops when the host stops it or
//! this client exits (its stdin closes).
use std::{io::{self, BufRead, Write}, path::{Path, PathBuf}, process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex}, time::{Duration, Instant}};
use peakrunner_core::{conditions::{Forecast, Pick, TimeOfDay, Weather}, map_catalog::{maps_for, SupportedMode}, sim::MAX_PLAYERS, terrain::MapId};

/// The server's config file in the host folder (the server's schema 1).
const CONFIG_FILE: &str = "server.json";
/// Everything the server printed in its last run, for troubleshooting.
const LOG_FILE: &str = "server.log";
/// Lines the Host screen keeps.
const MAX_LINES: usize = 200;
pub const DEFAULT_PORT: u16 = 7777;

/// The modes the Host screen offers, in menu order: every offered mode, and
/// Duel (a duel server: a lobby in Tenfold Tower and duels side by side;
/// not a mode menus play offline, so not "offered").
pub fn modes() -> Vec<SupportedMode> {
    SupportedMode::every().into_iter().filter(|m| m.offered() || *m == SupportedMode::Duel).collect()
}

/// What the Host screen edits. The password is never saved: it reaches the
/// server through its environment.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub name: String,
    pub location: String,
    pub mode: SupportedMode,
    /// The rotation, in play order.
    pub maps: Vec<MapId>,
    pub max_players: u32,
    pub bots: usize,
    /// Each round's time of day and weather: as the mode does, random, or fixed.
    pub forecast: Forecast,
    pub port: u16,
    /// List the server publicly (Find match); off: invite links only.
    pub announce: bool,
    /// Ask the router to forward the port (not on the screen; tests turn it off).
    pub upnp: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { name: "PeakRunner Community".into(), location: String::new(), mode: SupportedMode::Ctf,
            maps: maps_for(SupportedMode::Ctf).into_iter().take(1).collect(), max_players: 16, bots: 0,
            forecast: Forecast::default(), port: DEFAULT_PORT, announce: true, upnp: true }
    }
}

impl Settings {
    /// Why the server would refuse these settings, if it would.
    pub fn problem(&self) -> Option<&'static str> {
        let name = self.name.as_str();
        if name.is_empty() || name.len() > 64 || name.trim() != name || name.chars().any(char::is_control) {
            return Some("Server name: 1 to 64 plain characters, without leading or trailing spaces");
        }
        if self.location.len() > 40 { return Some("Location: up to 40 characters"); }
        if self.maps.is_empty() { return Some("Pick at least one map"); }
        if self.port < 1024 { return Some("Port: 1024 to 65535"); }
        None
    }

    /// The server's `server.json` for these settings.
    pub fn config(&self) -> serde_json::Value {
        let rotation: Vec<serde_json::Value> = self.maps.iter()
            .map(|m| serde_json::json!({"map": m.key(), "mode": self.mode.key()})).collect();
        let mut config = serde_json::json!({
            "schema": 1, "name": self.name, "location": self.location.trim(), "rotation": rotation,
            "max_players": self.max_players, "bots": self.bots, "port": self.port,
            "announce": self.announce, "upnp": self.upnp,
        });
        // Left out as the mode does, as the server writes them.
        if self.forecast.time != Pick::Mode { config["time_of_day"] = self.forecast.time.name(&TimeOfDay::ALL).into(); }
        if self.forecast.weather != Pick::Mode { config["weather"] = self.forecast.weather.name(&Weather::ALL).into(); }
        config
    }

    /// Settings from a saved `server.json`, keeping the defaults for what it
    /// doesn't say (or says in a way the Host screen can't show).
    pub fn from_config(value: &serde_json::Value) -> Self {
        let mut s = Self::default();
        if let Some(name) = value.get("name").and_then(|v| v.as_str()) { s.name = name.into(); }
        if let Some(location) = value.get("location").and_then(|v| v.as_str()) { s.location = location.into(); }
        if let Some(n) = value.get("max_players").and_then(|v| v.as_u64()) { s.max_players = (n as u32).clamp(2, MAX_PLAYERS as u32); }
        if let Some(n) = value.get("bots").and_then(|v| v.as_u64()) { s.bots = (n as usize).min(s.max_players as usize) & !1; }
        if let Some(port) = value.get("port").and_then(|v| v.as_u64()).and_then(|p| u16::try_from(p).ok()) { s.port = port; }
        if let Some(announce) = value.get("announce").and_then(|v| v.as_bool()) { s.announce = announce; }
        if let Some(upnp) = value.get("upnp").and_then(|v| v.as_bool()) { s.upnp = upnp; }
        let text = |key: &str| value.get(key).and_then(|v| v.as_str()).unwrap_or("");
        if let Ok(time) = Pick::parse(text("time_of_day"), &TimeOfDay::ALL) { s.forecast.time = time; }
        if let Ok(weather) = Pick::parse(text("weather"), &Weather::ALL) { s.forecast.weather = weather; }
        let entries = value.get("rotation").and_then(|r| r.as_array());
        let mode = entries.and_then(|e| e.first()).and_then(|e| e.get("mode"))
            .and_then(|m| serde_json::from_value::<SupportedMode>(m.clone()).ok()).filter(|m| modes().contains(m));
        if let Some(mode) = mode {
            let maps: Vec<MapId> = entries.into_iter().flatten()
                .filter(|e| e.get("mode").and_then(|m| m.as_str()) == Some(mode.key()))
                .filter_map(|e| e.get("map").and_then(|m| m.as_str()).and_then(MapId::parse))
                .filter(|m| maps_for(mode).contains(m)).collect();
            if !maps.is_empty() { s.mode = mode; s.maps = maps; }
        }
        s
    }

    /// Choose a mode, keeping the maps that host it (else its first map).
    pub fn set_mode(&mut self, mode: SupportedMode) {
        self.mode = mode;
        let usable = maps_for(mode);
        self.maps.retain(|m| usable.contains(m));
        if self.maps.is_empty() { self.maps.extend(usable.first()); }
    }

    /// Add a map at the end of the rotation, or take it out.
    pub fn toggle_map(&mut self, map: MapId) {
        match self.maps.iter().position(|&m| m == map) {
            Some(i) => { self.maps.remove(i); }
            None => self.maps.push(map),
        }
    }
}

/// The host folder, beside the client's settings (`PeakRunner/host`).
pub fn folder(client_settings: Option<&Path>) -> Option<PathBuf> {
    client_settings?.parent().map(|dir| dir.join("host"))
}

/// The saved settings, or the defaults.
pub fn load(folder: Option<&Path>) -> Settings {
    folder.and_then(|f| std::fs::read(f.join(CONFIG_FILE)).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .map_or_else(Settings::default, |v| Settings::from_config(&v))
}

/// `peakrunner-server` next to this executable (inside the Mac app too).
pub fn server_app() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join(if cfg!(windows) { "peakrunner-server.exe" } else { "peakrunner-server" });
    path.is_file().then_some(path)
}

/// Create the folder (owner-only on Unix) and write `server.json`.
fn save(folder: &Path, settings: &Settings) -> io::Result<()> {
    std::fs::create_dir_all(folder)?;
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?; }
    let mut json = serde_json::to_vec_pretty(&settings.config()).map_err(io::Error::other)?;
    json.push(b'\n');
    let temp = folder.join("server.json.tmp");
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    options.open(&temp)?.write_all(&json)?;
    std::fs::rename(&temp, folder.join(CONFIG_FILE))
}

/// 32 random bytes as lowercase hex.
fn new_key() -> io::Result<String> {
    let mut bytes = [0u8; peakrunner_net::HOST_KEY_LEN / 2];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What the server said, shared with its reader threads.
#[derive(Default)]
struct Output {
    lines: Vec<String>,
    /// The server's key fingerprint (hex), once it listens.
    ready: Option<String>,
}

/// A running (or just stopped) server this client started.
pub struct Hosted {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    output: Arc<Mutex<Output>>,
    pub key: String,
    pub port: u16,
    pub name: String,
    pub announce: bool,
    /// Set once it has exited: how.
    pub exited: Option<String>,
}

impl Hosted {
    /// Save the settings and start the server. `password` stays in memory.
    pub fn start(server: &Path, folder: &Path, settings: &Settings, password: &str) -> io::Result<Self> {
        save(folder, settings)?;
        // The player's own maps and mods (`preferences::own_content_dir`):
        // made here so they can find it; the server reads it as its content/.
        for sub in ["maps", "mods"] { let _ = std::fs::create_dir_all(folder.join("content").join(sub)); }
        // A launcher install marks only the game executable runnable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::fs::metadata(server)?.permissions().mode() & 0o100 == 0 {
                std::fs::set_permissions(server, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let key = new_key()?;
        let mut command = Command::new(server);
        command.arg("--config-dir").arg(folder).current_dir(folder)
            // Unlisted: ask the server list once for this computer's public
            // address, so the invite link works over the internet.
            .args((!settings.announce).then_some("--publicip"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // server.json alone configures it: none of this client's own
        // PEAKRUNNER_* variables may override it. Where content lives and
        // which mods to play aren't configuration: the server finds the same
        // maps and runs the same mods this game does.
        for (name, _) in std::env::vars_os() {
            if name.to_str().is_some_and(|n| n.starts_with("PEAKRUNNER_") && n != "PEAKRUNNER_PACK_DIR" && n != "PEAKRUNNER_MODS") { command.env_remove(name); }
        }
        command.env("PEAKRUNNER_HOST_KEY", &key).env("RUST_LOG", "warn");
        if !password.is_empty() { command.env("PEAKRUNNER_MATCH_PASSWORD", password); }
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); } // CREATE_NO_WINDOW
        let mut child = command.spawn()?;
        let output = Arc::new(Mutex::new(Output::default()));
        let log = std::fs::File::create(folder.join(LOG_FILE)).ok().map(|f| Arc::new(Mutex::new(f)));
        for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn io::Read + Send>),
                       child.stderr.take().map(|s| Box::new(s) as Box<dyn io::Read + Send>)].into_iter().flatten() {
            let (output, log) = (output.clone(), log.clone());
            std::thread::Builder::new().name("peakrunner-host-output".into()).spawn(move || {
                for line in io::BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    if let Some(log) = &log { let _ = writeln!(log.lock().unwrap(), "{line}"); }
                    let mut out = output.lock().unwrap();
                    match line.strip_prefix(peakrunner_net::HOST_READY) {
                        Some(pin) => out.ready = Some(pin.trim().into()),
                        None => {
                            out.lines.push(line);
                            if out.lines.len() > MAX_LINES { out.lines.remove(0); }
                        }
                    }
                }
            })?;
        }
        Ok(Self { stdin: child.stdin.take(), child: Some(child), output, key, port: settings.port,
            name: settings.name.clone(), announce: settings.announce, exited: None })
    }

    /// The address to join, pinned to the server's key, once it listens.
    pub fn join_address(&self) -> Option<String> {
        let pin = self.output.lock().unwrap().ready.clone()?;
        (pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| format!("quic://127.0.0.1:{}#sha256={pin}", self.port))
    }

    pub fn lines(&self) -> Vec<String> { self.output.lock().unwrap().lines.clone() }

    /// The first invite link the server printed (the public one first).
    pub fn invite(&self) -> Option<String> {
        let lines = self.lines();
        let link = |l: &String| l.trim().starts_with("peakrunner://join/").then(|| l.trim().to_string());
        let after = |label: &str| lines.iter().position(|l| l.contains(label)).and_then(|i| lines.get(i + 1)).and_then(link);
        after("players on the internet").or_else(|| after("players on your network")).or_else(|| lines.iter().find_map(link))
    }

    /// Notice the server exiting. True while it runs.
    pub fn running(&mut self) -> bool {
        if self.exited.is_some() { return false; }
        let Some(child) = self.child.as_mut() else { return false };
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                self.exited = Some(if status.success() { "The server stopped.".into() } else { format!("The server stopped ({status}).") });
                false
            }
            Err(e) => { self.exited = Some(format!("Lost track of the server: {e}")); false }
        }
    }

    /// Ask the server to stop (it releases its router port mapping), and
    /// make sure it's gone a few seconds later.
    pub fn stop(&mut self) {
        self.stdin.take();
        let Some(mut child) = self.child.take() else { return };
        if self.exited.is_none() { self.exited = Some("The server stopped.".into()); }
        let _ = std::thread::Builder::new().name("peakrunner-host-stop".into()).spawn(move || {
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                if !matches!(child.try_wait(), Ok(None)) { return; }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }
}

impl Drop for Hosted {
    fn drop(&mut self) { self.stop(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_servers_config() {
        let mut s = Settings::default();
        s.name = "Friday Night".into();
        s.set_mode(SupportedMode::Football);
        let stadiums = maps_for(SupportedMode::Football);
        assert_eq!(s.maps, stadiums[..1], "a mode keeps only its own maps");
        for &m in stadiums.iter().rev() { if !s.maps.contains(&m) { s.toggle_map(m); } }
        s.announce = false;
        s.max_players = 12; s.bots = 6; s.port = 7790;
        assert!(s.config().get("weather").is_none(), "as the mode does: left out");
        s.forecast = Forecast { time: Pick::Always(TimeOfDay::Dusk), weather: Pick::Random };
        let config = s.config();
        assert_eq!((config["time_of_day"].as_str(), config["weather"].as_str()), (Some("dusk"), Some("random")));
        assert_eq!(config["rotation"][0]["mode"], "football");
        assert_eq!(config["announce"], false);
        assert!(config.get("password").is_none(), "passwords are never written");
        assert_eq!(Settings::from_config(&config), s);
        // The order picked is the rotation order.
        s.toggle_map(s.maps[0]);
        assert_eq!(s.maps.len(), stadiums.len() - 1);
        assert_eq!(Settings::from_config(&s.config()).maps, s.maps);
    }

    #[test]
    fn duels_can_be_hosted() {
        let mut s = Settings::default();
        assert!(modes().contains(&SupportedMode::Duel));
        s.set_mode(SupportedMode::Duel);
        for &m in maps_for(SupportedMode::Duel).iter().skip(1).take(2) { s.toggle_map(m); }
        s.bots = 6;
        let config = s.config();
        let rotation = config["rotation"].as_array().unwrap();
        assert!(rotation.len() == 3 && rotation.iter().all(|e| e["mode"] == "duel"), "{rotation:?}");
        assert!(rotation.iter().all(|e| e["map"] != "tenfold" && e["map"] != "longfield"), "duels play on the open maps, never the lobby");
        assert_eq!(Settings::from_config(&config), s);
    }

    #[test]
    fn a_hand_edited_config_falls_back_to_defaults_it_cant_show() {
        let odd = serde_json::json!({"schema": 1, "name": "Mixed", "rotation": [{"playlist": "ctf_cnh"}], "max_players": 99, "port": 7777, "announce": true});
        let s = Settings::from_config(&odd);
        assert_eq!((s.name.as_str(), s.mode, s.max_players), ("Mixed", SupportedMode::Ctf, MAX_PLAYERS as u32));
        assert!(!s.maps.is_empty());
        assert_eq!(Settings { name: " x".into(), ..Settings::default() }.problem().map(|p| p.starts_with("Server name")), Some(true));
        assert_eq!(Settings { maps: vec![], ..Settings::default() }.problem(), Some("Pick at least one map"));
        assert_eq!(Settings::default().problem(), None);
    }

    /// The real server app: `PEAKRUNNER_TEST_SERVER_APP=target/release/peakrunner-server
    /// cargo test --release -p peakrunner --lib hosting_ -- --ignored`.
    #[test]
    #[ignore]
    fn hosting_starts_the_server_app_joins_it_and_the_host_key_starts_the_match() {
        let app = PathBuf::from(std::env::var("PEAKRUNNER_TEST_SERVER_APP").expect("PEAKRUNNER_TEST_SERVER_APP"));
        let app = std::fs::canonicalize(app).unwrap();
        let folder = std::env::temp_dir().join(format!("peakrunner-host-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        // As a launcher installs it: not marked runnable.
        std::fs::create_dir_all(folder.join("game")).unwrap();
        let app = { let copy = folder.join("game/peakrunner-server"); std::fs::copy(&app, &copy).unwrap(); copy };
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o600)).unwrap(); }
        let settings = Settings { name: "Host Test".into(), port: 7791, announce: false, upnp: false, ..Settings::default() };
        let mut hosted = Hosted::start(&app, &folder, &settings, "letmein").unwrap();
        let saved = std::fs::read_to_string(folder.join(CONFIG_FILE)).unwrap();
        assert!(!saved.contains("letmein"), "the password stays out of server.json");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(folder.join(CONFIG_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let wait = |what: &str, mut done: Box<dyn FnMut() -> bool + '_>| {
            let until = Instant::now() + Duration::from_secs(20);
            while !done() { assert!(Instant::now() < until, "timed out waiting for {what}"); std::thread::sleep(Duration::from_millis(50)); }
        };
        let mut address = None;
        wait("the server to listen", Box::new(|| { address = hosted.join_address(); address.is_some() }));
        assert!(hosted.running(), "{:?}", hosted.lines());
        let address = address.unwrap();
        assert!(address.starts_with("quic://127.0.0.1:7791#sha256="), "{address}");
        assert!(peakrunner_net::connect_as(&address, 7791, "Host", "wrong", false).is_ok());
        let session = peakrunner_net::connect_as(&address, 7791, "Host", "letmein", false).unwrap();
        let phase = || session.lobby().snapshot.map(|s| s.phase);
        wait("the snapshot", Box::new(|| phase().is_some()));
        assert_eq!(phase(), Some(peakrunner_core::sim::Phase::Waiting), "alone, the match waits");
        assert!(session.start_match(&hosted.key));
        wait("the countdown", Box::new(|| phase() == Some(peakrunner_core::sim::Phase::Countdown)));
        drop(session);
        hosted.stop();
        assert!(!hosted.running());
        std::thread::sleep(Duration::from_secs(2));
        // Nothing listens any more: a new join fails.
        let late = peakrunner_net::connect_as(&address, 7791, "Late", "letmein", false).unwrap();
        wait("the refused join", Box::new(|| late.lobby().error.is_some()));
        let log = std::fs::read_to_string(folder.join(LOG_FILE)).unwrap();
        assert!(log.contains(peakrunner_net::HOST_READY), "{log}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn host_keys_are_fresh_lowercase_hex() {
        let (a, b) = (new_key().unwrap(), new_key().unwrap());
        assert_eq!(a.len(), peakrunner_net::HOST_KEY_LEN);
        assert!(a.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
        assert_ne!(a, b);
    }
}
