//! Zero-configuration hosting: `server.json`, the first-run setup questions,
//! and how the file and the environment combine into what the server runs.
//!
//! The file lives in the per-user config directory (`config_dir`), next to
//! the generated server key (`identity`). Environment variables override it
//! field by field; with no file at all the server reads only the
//! environment, exactly as the Docker deployments always have.
use std::{io::{self, BufRead, Write}, net::SocketAddr, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};
use peakrunner_core::{conditions::{Forecast, Pick, TimeOfDay, Weather}, map_catalog::{maps_for, SupportedMode}, terrain::{self, MapId}};
use crate::{rotation::Rotation, wire::invalid};

pub const CONFIG_FILE: &str = "server.json";
pub const DEFAULT_DIRECTORY: &str = "https://dir.peakrunner.net/announce";
pub const DEFAULT_PORT: u16 = 7777;
/// Names the public directory keeps for the official servers.
const RESERVED_PREFIX: &str = "springdale";

/// `server.json`. Written by the setup questions; safe to edit by hand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub schema: u32,
    pub name: String,
    /// Shown in the server list ("Denver, US"); empty for none.
    #[serde(default)]
    pub location: String,
    /// A rotation as `PEAKRUNNER_MATCH_ROTATION` takes it: playlists
    /// (`{"playlist":"ctf"}`) and/or `{"map":...,"mode":...}` entries.
    pub rotation: serde_json::Value,
    /// The order the rotation plays: empty (or "in_order") as listed, or
    /// "random", a new order each time through.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rotation_order: String,
    pub max_players: u32,
    /// Bots fill the match to this many players; people take their places.
    #[serde(default)]
    pub bots: usize,
    /// Match password; empty for none. Stored in plain text in this file,
    /// which only its owner can read (Unix).
    #[serde(default)]
    pub password: String,
    pub port: u16,
    /// Announce to the public server list.
    pub announce: bool,
    #[serde(default = "default_directory")]
    pub directory: String,
    /// How players reach this server (an IP or host name); empty to ask the
    /// directory (`GET /whoami`).
    #[serde(default)]
    pub public_address: String,
    /// Ask the router to forward the port (UPnP).
    #[serde(default = "yes")]
    pub upnp: bool,
    /// Mods to run, by id, in order (docs/making-content.md); installed in a
    /// content folder beside the server. Players download them on joining.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<String>,
    /// Each round's time of day and weather: empty (or "default") as the
    /// mode does (the map's own look; the deathmatch modes roll their own),
    /// "random", or one of `conditions::TimeOfDay::ALL` / `Weather::ALL`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub time_of_day: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub weather: String,
}
fn default_directory() -> String { DEFAULT_DIRECTORY.into() }
fn yes() -> bool { true }

impl Default for ServerConfig {
    fn default() -> Self {
        Self { schema: 1, name: "PeakRunner Community".into(), location: String::new(),
            rotation: serde_json::json!([{"playlist": "ctf"}]), rotation_order: String::new(), max_players: 16, bots: 0, password: String::new(),
            port: DEFAULT_PORT, announce: true, directory: default_directory(), public_address: String::new(), upnp: true, mods: Vec::new(), time_of_day: String::new(), weather: String::new() }
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if !peakrunner_discovery::http::status_fields_ok(name, "Old Holler", 0, 2) || name.trim() != name {
        return Err("use 1 to 64 plain characters, without leading or trailing spaces".into());
    }
    if name.to_ascii_lowercase().starts_with(RESERVED_PREFIX) {
        return Err("names starting with \"Springdale\" are reserved for the official servers".into());
    }
    Ok(())
}
fn check_password(password: &str) -> Result<(), String> {
    if password.len() > 64 || password.chars().any(char::is_control) || password.trim() != password {
        return Err("use up to 64 characters, without leading or trailing spaces".into());
    }
    Ok(())
}
fn check_location(location: &str) -> Result<(), String> {
    if peakrunner_discovery::valid_location(location) { Ok(()) }
    else { Err("up to 40 letters, digits, spaces, commas, periods or hyphens".into()) }
}
fn check_public_address(address: &str) -> Result<(), String> {
    if address.is_empty() { return Ok(()); }
    let host = if address.contains(':') && !address.starts_with('[') { format!("[{address}]") } else { address.to_string() };
    peakrunner_discovery::quic::endpoint_url(&format!("quic://{host}:{DEFAULT_PORT}")).map(|_| ())
        .map_err(|_| "an IP address or host name, without a port".into())
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 { return Err(format!("unsupported schema {}", self.schema)); }
        check_name(&self.name).map_err(|e| format!("name: {e}"))?;
        check_location(&self.location).map_err(|e| format!("location: {e}"))?;
        // A rotation may name the mods' modes: they come first.
        peakrunner_core::mods::activate_home(&self.mods).map_err(|e| format!("mods: {e}"))?;
        Rotation::parse(&self.rotation.to_string()).map_err(|e| format!("rotation: {e}"))?;
        random_order(&self.rotation_order).map_err(|e| format!("rotation_order: {e}"))?;
        if !(2..=peakrunner_core::sim::MAX_PLAYERS as u32).contains(&self.max_players) {
            return Err(format!("max_players must be 2 to {}", peakrunner_core::sim::MAX_PLAYERS));
        }
        if self.bots > self.max_players as usize { return Err("bots can't be more than max_players".into()); }
        check_password(&self.password).map_err(|e| format!("password: {e}"))?;
        if self.port < 1024 { return Err("port must be 1024 to 65535".into()); }
        if self.announce && !(self.directory.starts_with("https://") || self.directory.starts_with("http://")) {
            return Err("directory must be an https:// URL".into());
        }
        check_public_address(&self.public_address).map_err(|e| format!("public_address: {e}"))?;
        forecast(&self.time_of_day, &self.weather)?;
        Ok(())
    }
}

/// The folder beside `server.json` for a host's own maps and mods.
pub const CONTENT_DIR: &str = "content";

/// Make the content folder (`maps/`, `mods/` and a README) if it isn't there;
/// its path, or None when it can't be made.
pub fn prepare_content(dir: &Path) -> Option<PathBuf> {
    let content = dir.join(CONTENT_DIR);
    for sub in ["maps", "mods"] { std::fs::create_dir_all(content.join(sub)).ok()?; }
    let readme = content.join("README.txt");
    if !readme.exists() {
        std::fs::write(&readme, "Maps and mods this server runs beside the installed ones.\n\n\
            maps/<key>/   a map folder: map.json, its payloads and info.json\n\
            mods/<id>/    a mod folder: mod.json, defs.json, modes/<key>.json\n\n\
            Name maps in the rotation and mods in \"mods\" in server.json. Check with:\n\
              peakrunner-server --check\n\
            Players who lack them download them when they join, after they\n\
            agree. The server's README.md explains more.\n").ok()?;
    }
    Some(content)
}

/// Where `server.json` and the server key live: `--config-dir DIR`, else
/// `PEAKRUNNER_SERVER_DIR`, else the per-user config directory
/// (macOS `~/Library/Application Support/PeakRunner/server`, Windows
/// `%APPDATA%\PeakRunner\server`, elsewhere `$XDG_CONFIG_HOME/PeakRunner/server`
/// or `~/.config/PeakRunner/server`), the same place the client keeps its
/// settings.
pub fn config_dir(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(i) = args.iter().position(|a| a == "--config-dir") { return args.get(i + 1).map(PathBuf::from); }
    if let Some(dir) = env("PEAKRUNNER_SERVER_DIR").filter(|d| !d.is_empty()) { return Some(dir.into()); }
    user_dir(std::env::consts::OS, env("HOME").map(PathBuf::from), env("APPDATA").map(PathBuf::from),
        env("XDG_CONFIG_HOME").map(PathBuf::from))
}
fn user_dir(os: &str, home: Option<PathBuf>, roaming: Option<PathBuf>, xdg: Option<PathBuf>) -> Option<PathBuf> {
    let absolute = |p: Option<PathBuf>| p.filter(|p| p.is_absolute());
    let home = absolute(home);
    let base = match os {
        "macos" => home?.join("Library/Application Support"),
        "windows" => absolute(roaming).or_else(|| home.map(|p| p.join("AppData/Roaming")))?,
        _ => absolute(xdg).or_else(|| home.map(|p| p.join(".config")))?,
    };
    Some(base.join("PeakRunner").join("server"))
}

/// The saved config, if there is one. A file that doesn't parse or validate
/// is an error, never silently replaced.
pub fn load(path: &Path) -> io::Result<Option<ServerConfig>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let config: ServerConfig = serde_json::from_slice(&bytes)
        .map_err(|e| invalid(&format!("{}: {e}; fix it or run with --setup", path.display())))?;
    config.validate().map_err(|e| invalid(&format!("{}: {e}; fix it or run with --setup", path.display())))?;
    Ok(Some(config))
}

/// Write the config (owner-only on Unix: it may hold the match password).
pub fn save(path: &Path, config: &ServerConfig) -> io::Result<()> {
    config.validate().map_err(|e| invalid(&e))?;
    let dir = path.parent().ok_or_else(|| invalid("config path has no directory"))?;
    crate::identity::private_dir(dir)?;
    let mut json = serde_json::to_vec_pretty(config).map_err(io::Error::other)?;
    json.push(b'\n');
    let temp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&temp);
    crate::identity::write_private(&temp, &json)?;
    std::fs::rename(&temp, path)
}

/// Whether the environment already configures the match, so a terminal run
/// without `server.json` shouldn't stop to ask questions.
pub fn env_configured(env: &dyn Fn(&str) -> Option<String>) -> bool {
    ["PEAKRUNNER_TLS_CERT", "PEAKRUNNER_TLS_KEY", "PEAKRUNNER_MATCH_ROTATION", "PEAKRUNNER_MATCH_MAP", "PEAKRUNNER_ADVERTISE_URL"]
        .iter().any(|k| env(k).is_some())
}

/// What the match itself runs with.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchSettings {
    pub name: String, pub map: String, pub rotation: Option<String>, pub max_players: u32, pub bots: usize,
    /// Play the rotation in a random order (`rotation_order`,
    /// `PEAKRUNNER_ROTATION_ORDER`).
    pub random_order: bool,
    pub password: String, pub location: String,
    /// Mods in force, by id (`PEAKRUNNER_MODS` wins over server.json).
    pub mods: Vec<String>,
    /// Each round's time of day and weather (`PEAKRUNNER_TIME_OF_DAY`,
    /// `PEAKRUNNER_WEATHER`).
    pub forecast: Forecast,
    /// The local HTTP `/status` listener (Docker health checks).
    pub health: Option<String>,
    /// Started by a client to host (`PEAKRUNNER_HOST_KEY`): its key starts
    /// the match, and the server stops when that client's pipe closes.
    pub host_key: Option<String>,
}
/// Where to announce, and as what (`None`: ask the directory).
#[derive(Clone, Debug, PartialEq)]
pub struct AnnouncePlan { pub directory: String, pub url: Option<String> }
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub settings: MatchSettings, pub bind: SocketAddr, pub announce: Option<AnnouncePlan>, pub upnp: bool,
    /// A private server's public address, when `public_address` names it.
    pub private_url: Option<String>,
    /// A private server without `public_address`: the directory it may ask
    /// (`GET /whoami`, only with `--publicip`) for its public address.
    pub lookup: Option<String>,
}
impl Plan {
    /// The directory to ask for the public address: only when the operator
    /// ran with `--publicip`. Nothing is contacted otherwise.
    pub fn public_ip_lookup(&self, asked: bool) -> Option<String> { self.lookup.clone().filter(|_| asked) }
}

fn number<T: std::str::FromStr>(env: &dyn Fn(&str) -> Option<String>, key: &str) -> io::Result<Option<T>> {
    env(key).map(|v| v.trim().parse().map_err(|_| invalid(&format!("{key} must be a number")))).transpose()
}

/// A time of day and weather as hosts write them (`Pick::parse`).
pub fn forecast(time: &str, weather: &str) -> Result<Forecast, String> {
    Ok(Forecast { time: Pick::parse(time, &TimeOfDay::ALL).map_err(|e| format!("time_of_day: {e}"))?,
        weather: Pick::parse(weather, &Weather::ALL).map_err(|e| format!("weather: {e}"))? })
}

/// How a choice is saved: nothing for the default, else its name.
fn saved_pick<T: Copy + PartialEq>(pick: Pick<T>, all: &[(T, &'static str)]) -> String {
    if pick == Pick::Mode { String::new() } else { pick.name(all).to_string() }
}

/// A rotation saved in `server.json`, without entries for maps this version
/// doesn't have (a map can be renamed or removed after setup ran), so an older
/// file still starts a server. With nothing left, the dropped entries' mode
/// playlist is used. Rotations from the environment stay strict.
fn saved_rotation(rotation: &serde_json::Value) -> String {
    let Some(entries) = rotation.as_array() else { return rotation.to_string() };
    fn unknown(e: &serde_json::Value) -> Option<&str> {
        e.get("map").and_then(|m| m.as_str()).filter(|key| MapId::parse(key).is_none())
    }
    let skipped: Vec<&str> = entries.iter().filter_map(unknown).collect();
    if skipped.is_empty() { return rotation.to_string(); }
    eprintln!("server.json names maps this version doesn't have ({}); skipping them. Run setup again to choose maps.", skipped.join(", "));
    let kept: Vec<serde_json::Value> = entries.iter().filter(|e| unknown(e).is_none()).cloned().collect();
    if !kept.is_empty() { return serde_json::Value::Array(kept).to_string(); }
    let mode = entries.iter().find_map(|e| e.get("mode").and_then(|m| m.as_str()))
        .filter(|m| ["ctf", "capture_and_hold", "team_deathmatch", "rabbit", "team_rabbit", "football"].contains(m)).unwrap_or("ctf_cnh");
    serde_json::json!([{"playlist": mode}]).to_string()
}

/// Combine `server.json` (if any) with the environment, which wins. With no
/// file this is exactly the environment-only behaviour of earlier releases.
pub fn plan(config: Option<&ServerConfig>, env: &dyn Fn(&str) -> Option<String>) -> io::Result<Plan> {
    let port = config.map_or(DEFAULT_PORT, |c| c.port);
    let bind: SocketAddr = env("PEAKRUNNER_UDP_BIND").unwrap_or_else(|| format!("0.0.0.0:{port}")).parse().map_err(io::Error::other)?;
    let (rotation, map) = match (env("PEAKRUNNER_MATCH_ROTATION"), env("PEAKRUNNER_MATCH_MAP"), config) {
        (Some(rotation), map, _) => (Some(rotation), map.unwrap_or_else(|| "old-holler".into())),
        (None, Some(map), _) => (None, map),
        (None, None, Some(c)) => (Some(saved_rotation(&c.rotation)), "old-holler".into()),
        (None, None, None) => (None, "old-holler".into()),
    };
    let settings = MatchSettings {
        name: env("PEAKRUNNER_MATCH_NAME").unwrap_or_else(|| config.map_or("Springdale Central".into(), |c| c.name.clone())),
        map, rotation,
        random_order: random_order(&env("PEAKRUNNER_ROTATION_ORDER").unwrap_or_else(|| config.map_or(String::new(), |c| c.rotation_order.clone())))
            .map_err(|e| invalid(&format!("rotation order: {e}")))?,
        // Match capacity: 8 by default, up to 28 (14 a side).
        max_players: number(env, "PEAKRUNNER_MAX_PLAYERS")?.unwrap_or(config.map_or(8, |c| c.max_players)),
        // Bots: fill the match to this many players; people replace them.
        bots: number(env, "PEAKRUNNER_BOTS")?.unwrap_or(config.map_or(0, |c| c.bots)),
        password: env("PEAKRUNNER_MATCH_PASSWORD").unwrap_or_else(|| config.map_or(String::new(), |c| c.password.clone())),
        location: env("PEAKRUNNER_LOCATION").unwrap_or_else(|| config.map_or(String::new(), |c| c.location.clone())),
        // The container health check reads 8080. A server with a
        // server.json serves the same page on loopback only, over TCP at its
        // game port's number: `--healthcheck` works anywhere, nothing opens.
        health: env("PEAKRUNNER_HEALTH_BIND").or_else(|| Some(match config {
            None => "0.0.0.0:8080".into(),
            Some(_) => format!("127.0.0.1:{}", bind.port()),
        })),
        host_key: env("PEAKRUNNER_HOST_KEY").filter(|k| !k.is_empty()),
        mods: env("PEAKRUNNER_MODS").map(|m| m.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect())
            .unwrap_or_else(|| config.map_or(Vec::new(), |c| c.mods.clone())),
        forecast: forecast(&env("PEAKRUNNER_TIME_OF_DAY").unwrap_or_else(|| config.map_or(String::new(), |c| c.time_of_day.clone())),
            &env("PEAKRUNNER_WEATHER").unwrap_or_else(|| config.map_or(String::new(), |c| c.weather.clone()))).map_err(|e| invalid(&e))?,
    };
    let announce = match config {
        // Environment only: announce when both are set, as before.
        None => match (env("PEAKRUNNER_DIRECTORY_URL"), env("PEAKRUNNER_ADVERTISE_URL")) {
            (Some(directory), Some(url)) => Some(AnnouncePlan { directory, url: Some(url) }),
            _ => None,
        },
        Some(c) => env("PEAKRUNNER_DIRECTORY_URL").or_else(|| c.announce.then(|| c.directory.clone())).map(|directory| AnnouncePlan {
            directory,
            url: env("PEAKRUNNER_ADVERTISE_URL").or_else(|| (!c.public_address.is_empty()).then(|| quic_url(&c.public_address, bind.port()))),
        }),
    };
    if let Some(url) = announce.as_ref().and_then(|a| a.url.as_deref()) { peakrunner_discovery::quic::endpoint_url(url)?; }
    let private = config.filter(|_| announce.is_none());
    let private_url = private.and_then(|c| env("PEAKRUNNER_ADVERTISE_URL")
        .or_else(|| (!c.public_address.is_empty()).then(|| quic_url(&c.public_address, bind.port()))));
    let lookup = private.filter(|_| private_url.is_none()).map(|c| c.directory.clone());
    Ok(Plan { settings, bind, announce, upnp: config.is_some_and(|c| c.upnp), private_url, lookup })
}

/// A rotation order: "" or "in_order" (false), "random" (true).
fn random_order(order: &str) -> Result<bool, String> {
    match order.trim() {
        "" | "in_order" => Ok(false),
        "random" => Ok(true),
        other => Err(format!("\"{other}\" isn't in_order or random")),
    }
}

/// Where `--healthcheck` reads the status page `plan` serves.
pub fn health_url(plan: &Plan) -> String {
    let bind = plan.settings.health.clone().unwrap_or_else(|| "127.0.0.1:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    format!("http://127.0.0.1:{port}/status")
}

/// `quic://host:port`, bracketing an IPv6 address.
pub fn quic_url(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') { format!("quic://[{host}]:{port}") } else { format!("quic://{host}:{port}") }
}

/// The modes the setup offers: one mode on chosen maps, or the mixed
/// playlist; then the modes of the mods chosen (no playlist of their own).
struct Choice { label: String, playlist: &'static str, mode: Option<SupportedMode> }
fn choices() -> Vec<Choice> {
    let base = |label: &str, playlist, mode| Choice { label: label.into(), playlist, mode };
    let mut all = vec![
        base("Capture the Flag", "ctf", Some(SupportedMode::Ctf)),
        base("Capture & Hold", "capture_and_hold", Some(SupportedMode::CaptureAndHold)),
        base("Team Deathmatch", "team_deathmatch", Some(SupportedMode::TeamDeathmatch)),
        base("Football (stadium maps)", "football", Some(SupportedMode::Football)),
        base("Capture the Flag, then Capture & Hold, on every map", "ctf_cnh", None),
        base("Rabbit", "rabbit", Some(SupportedMode::Rabbit)),
        base("Team Rabbit", "team_rabbit", Some(SupportedMode::TeamRabbit)),
        // A duel server: a lobby, and one on one duels on these maps.
        base("Duels (a lobby; one on one duels on these maps)", "duel", Some(SupportedMode::Duel)),
    ];
    let builtin = peakrunner_core::modes::BASE_COUNT;
    all.extend(SupportedMode::every().into_iter().skip(builtin).filter(|m| m.offered())
        .map(|m| Choice { label: format!("{} (mod)", m.label()), playlist: "", mode: Some(m) }));
    all
}

/// A rotation the questions can show again: its choice and maps (None for
/// all of them). None when it's something else (hand-edited).
fn describe(rotation: &serde_json::Value, choices: &[Choice]) -> Option<(usize, Option<Vec<MapId>>)> {
    let entries = rotation.as_array()?;
    if let [only] = entries.as_slice() {
        if let Some(p) = only.get("playlist").and_then(|p| p.as_str()) {
            return choices.iter().position(|c| !c.playlist.is_empty() && c.playlist == p).map(|i| (i, None));
        }
    }
    let mut mode = None;
    let mut maps = Vec::new();
    for e in entries {
        let m: SupportedMode = serde_json::from_value(e.get("mode")?.clone()).ok()?;
        if mode.is_some_and(|x| x != m) || e.as_object()?.len() != 2 { return None; }
        mode = Some(m);
        maps.push(MapId::parse(e.get("map")?.as_str()?)?);
    }
    let i = choices.iter().position(|c| c.mode.is_some() && c.mode == mode)?;
    Some((i, Some(maps)))
}

fn build_rotation(choice: &Choice, maps: Option<&[MapId]>) -> serde_json::Value {
    match (choice.mode, maps) {
        (Some(mode), Some(maps)) => serde_json::Value::Array(maps.iter()
            .map(|m| serde_json::json!({"map": m.key(), "mode": mode.key()})).collect()),
        _ => serde_json::json!([{"playlist": choice.playlist}]),
    }
}

/// "Capture the Flag: Old Holler, Tower Complex" for a valid rotation.
pub fn rotation_summary(rotation: &str) -> String {
    let Ok(r) = Rotation::parse(rotation) else { return rotation.into() };
    let mut groups: Vec<(SupportedMode, Vec<&str>)> = Vec::new();
    for (map, mode) in r.choices() {
        let name = terrain::info(map).name;
        match groups.iter_mut().find(|g| g.0 == mode) { Some(g) => g.1.push(name), None => groups.push((mode, vec![name])) }
    }
    groups.iter().map(|(mode, maps)| format!("{}: {}", mode.label(), maps.join(", "))).collect::<Vec<_>>().join("; ")
}

/// `1,3` / `all` against a list of `n` items: 0-based indices, in order,
/// without repeats. None for all.
fn parse_selection(text: &str, n: usize) -> Result<Option<Vec<usize>>, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("all") || text == "*" { return Ok(None); }
    let mut out = Vec::new();
    for part in text.split([',', ' ']).filter(|p| !p.is_empty()) {
        let i: usize = part.parse().map_err(|_| format!("\"{part}\" isn't a number; use numbers like 1,3 or all"))?;
        if !(1..=n).contains(&i) { return Err(format!("{i} isn't on the list (1 to {n})")); }
        if !out.contains(&(i - 1)) { out.push(i - 1); }
    }
    if out.is_empty() { return Err("pick at least one".into()); }
    if out.len() == n { return Ok(None); }
    Ok(Some(out))
}

fn yes_no(text: &str) -> Result<bool, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(true), "n" | "no" => Ok(false), _ => Err("answer yes or no".into()),
    }
}

/// Ask one question: `label [default]: `. Enter keeps the default; an
/// invalid answer explains itself and asks again. End of input cancels.
fn ask<T>(input: &mut impl BufRead, out: &mut impl Write, label: &str, default: &str,
          parse: impl Fn(&str) -> Result<T, String>) -> io::Result<T> {
    loop {
        write!(out, "{label} [{default}]: ")?;
        out.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "setup cancelled")); }
        let answer = line.trim();
        match parse(if answer.is_empty() { default } else { answer }) {
            Ok(value) => return Ok(value),
            Err(why) => writeln!(out, "  {why}")?,
        }
    }
}

/// The first-run questions, starting from `start` (the saved config, or the
/// defaults). Returns a validated config; nothing is saved here.
pub fn wizard(input: &mut impl BufRead, out: &mut impl Write, start: &ServerConfig) -> io::Result<ServerConfig> {
    let mut c = start.clone();
    writeln!(out, "PeakRunner server setup. Press Enter to accept the answer in [brackets].")?;
    loop {
        writeln!(out)?;
        c.name = ask(input, out, "Server name", &c.name, |s| check_name(s).map(|_| s.to_string()))?;

        // Mods first: the ones in content/mods, whose modes are offered next.
        let installed = peakrunner_core::mods::installed();
        if !installed.is_empty() {
            writeln!(out, "\nMods in your content folder:")?;
            for (n, m) in installed.iter().enumerate() {
                writeln!(out, "  {}) {}{}", n + 1, m.name, if m.note.is_empty() { String::new() } else { format!(": {}", m.note) })?;
            }
            let default = c.mods.iter().filter_map(|id| installed.iter().position(|m| m.id == id)).map(|n| (n + 1).to_string())
                .collect::<Vec<_>>().join(",");
            let default = if default.is_empty() { "none".into() } else { default };
            c.mods = ask(input, out, "Mods to run (numbers like 1,2, or none)", &default, |s| {
                let picks = if s == "none" { Vec::new() } else { parse_selection(s, installed.len())?.unwrap_or_else(|| (0..installed.len()).collect()) };
                let ids: Vec<String> = picks.into_iter().map(|n| installed[n].id.to_string()).collect();
                peakrunner_core::mods::activate_home(&ids).map(|_| ids)
            })?;
        }

        // Mode, then maps: numbered lists from the map catalog.
        let choices = choices();
        let current = describe(&c.rotation, &choices);
        writeln!(out, "\nGame mode:")?;
        if current.is_none() { writeln!(out, "  0) Keep the current rotation ({})", rotation_summary(&c.rotation.to_string()))?; }
        for (i, choice) in choices.iter().enumerate() { writeln!(out, "  {}) {}", i + 1, choice.label)?; }
        let default_mode = current.as_ref().map_or("0".to_string(), |(i, _)| (i + 1).to_string());
        let keep = current.is_none();
        let picked = ask(input, out, "Mode", &default_mode, |s| match s.parse::<usize>() {
            Ok(0) if keep => Ok(None),
            Ok(i) if (1..=choices.len()).contains(&i) => Ok(Some(i - 1)),
            _ => Err(format!("pick a number from the list (1 to {})", choices.len())),
        })?;
        if let Some(i) = picked {
            let choice = &choices[i];
            let maps = match choice.mode {
                None => None,
                Some(mode) => {
                    // Every map is opened to see what it holds: slow on a small machine.
                    writeln!(out, "\nLooking through the maps for {}...", choice.label)?;
                    out.flush()?;
                    let all = maps_for(mode);
                    writeln!(out, "Maps for {}:", choice.label)?;
                    for (n, map) in all.iter().enumerate() { writeln!(out, "  {}) {}", n + 1, terrain::info(*map).name)?; }
                    let default = match current.as_ref().filter(|(j, _)| *j == i) {
                        Some((_, Some(maps))) => maps.iter().filter_map(|m| all.iter().position(|a| a == m)).map(|n| (n + 1).to_string())
                            .collect::<Vec<_>>().join(","),
                        _ => "all".into(),
                    };
                    let default = if default.is_empty() { "all".into() } else { default };
                    ask(input, out, "Maps (numbers like 1,3, or all)", &default, |s| parse_selection(s, all.len()))?
                        .map(|picks| picks.into_iter().map(|n| all[n]).collect::<Vec<_>>())
                }
            };
            c.rotation = build_rotation(choice, maps.as_deref());
        }

        writeln!(out)?;
        let most = peakrunner_core::sim::MAX_PLAYERS as u32;
        c.max_players = ask(input, out, &format!("Max players (2-{most})"), &c.max_players.to_string(), |s| match s.parse::<u32>() {
            Ok(n) if (2..=most).contains(&n) => Ok(n),
            _ => Err(format!("a number from 2 to {most}")),
        })?;
        let max = c.max_players as usize;
        let bots_default = if c.bots == 0 { "no".to_string() } else { c.bots.min(max).to_string() };
        let question = if duel_rotation(&c.rotation) {
            format!("Sparring bots: fill the lobby's tower with blade-fighting bots up to how many players? (no, or 2-{max})")
        } else { format!("Bots: fill the match with bots up to how many players? (no, or 2-{max})") };
        c.bots = ask(input, out, &question, &bots_default,
            |s| match s.to_ascii_lowercase().as_str() {
                "no" | "none" | "n" | "0" => Ok(0),
                "yes" | "y" => Ok(max),
                s => match s.parse::<usize>() { Ok(n) if (2..=max).contains(&n) => Ok(n), _ => Err(format!("no, or a number from 2 to {max}")) },
            })?;
        let names = |all: &[&str]| all.join(", ");
        let times: Vec<&str> = TimeOfDay::ALL.iter().map(|(_, n)| *n).collect();
        let time = forecast(&c.time_of_day, "").map_err(|e| invalid(&e))?.time.name(&TimeOfDay::ALL);
        c.time_of_day = ask(input, out, &format!("Time of day each round (default, random, {})", names(&times)), time,
            |s| Pick::parse(s, &TimeOfDay::ALL).map(|p| saved_pick(p, &TimeOfDay::ALL)))?;
        let weathers: Vec<&str> = Weather::ALL.iter().map(|(_, n)| *n).collect();
        let weather = forecast("", &c.weather).map_err(|e| invalid(&e))?.weather.name(&Weather::ALL);
        c.weather = ask(input, out, &format!("Weather each round (default, random, {})", names(&weathers)), weather,
            |s| Pick::parse(s, &Weather::ALL).map(|p| saved_pick(p, &Weather::ALL)))?;
        let pw_default = if c.password.is_empty() { "none" } else { "keep" };
        let old = c.password.clone();
        c.password = ask(input, out, "Password players must enter (none for an open server)", pw_default, |s| match s {
            "none" => Ok(String::new()),
            "keep" if !old.is_empty() => Ok(old.clone()),
            s => check_password(s).map(|_| s.to_string()),
        })?;
        c.port = ask(input, out, "UDP port", &c.port.to_string(), |s| match s.parse::<u16>() {
            Ok(p) if p >= 1024 => Ok(p),
            _ => Err("a port from 1024 to 65535".into()),
        })?;
        c.announce = ask(input, out, "Announce to the public server list? (yes/no)", if c.announce { "yes" } else { "no" }, yes_no)?;
        if c.announce {
            let default = if c.location.is_empty() { "none".to_string() } else { c.location.clone() };
            c.location = ask(input, out, "Location shown in the list, like Denver, US (none to leave out)", &default, |s| {
                let s = if s == "none" { "" } else { s };
                check_location(s).map(|_| s.to_string())
            })?;
        }
        c.upnp = ask(input, out, "Ask your router to open the port automatically (UPnP)? (yes/no)", if c.upnp { "yes" } else { "no" }, yes_no)?;

        writeln!(out, "\n{}", summary(&c))?;
        if ask(input, out, "Start with these settings? (Y/n)", "Y", yes_no)? {
            c.validate().map_err(|e| invalid(&e))?;
            return Ok(c);
        }
        writeln!(out, "\nOnce more from the top; your answers are the new defaults.")?;
    }
}

/// Whether a rotation plays duels, whose bots spar in the lobby's tower.
fn duel_rotation(rotation: &serde_json::Value) -> bool {
    Rotation::parse(&rotation.to_string()).is_ok_and(|r| r.duels().is_some())
}

/// The settings, for the confirmation and the startup banner.
pub fn summary(c: &ServerConfig) -> String {
    let bots = if c.bots == 0 { "no bots".to_string() }
        else if duel_rotation(&c.rotation) { format!("sparring bots in the tower fill to {}", c.bots) }
        else { format!("bots fill to {}", c.bots) };
    let listing = if c.announce {
        format!("yes{}", if c.location.is_empty() { String::new() } else { format!(", as in {}", c.location) })
    } else { "no (share the invite link; run with --publicip to also show your public one)".into() };
    let or_default = |s: &str| if s.is_empty() { "default".to_string() } else { s.to_string() };
    format!("  Name:        {}\n  Rotation:    {}\n  Players:     up to {} ({bots})\n  Sky:         {} time, {} weather\n  Password:    {}\n  Port:        UDP {}\n  Public list: {listing}\n  UPnP:        {}",
        c.name, rotation_summary(&c.rotation.to_string()), c.max_players, or_default(&c.time_of_day), or_default(&c.weather),
        if c.password.is_empty() { "none" } else { "set" }, c.port, if c.upnp { "yes" } else { "no" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_rotations_skip_maps_this_version_lacks() {
        let keep = serde_json::json!([{"map": "old-holler", "mode": "ctf"}, {"map": "gone-map", "mode": "ctf"}]);
        assert_eq!(saved_rotation(&keep), serde_json::json!([{"map": "old-holler", "mode": "ctf"}]).to_string());
        let none = serde_json::json!([{"map": "gone-map", "mode": "capture_and_hold"}]);
        assert_eq!(saved_rotation(&none), serde_json::json!([{"playlist": "capture_and_hold"}]).to_string());
        let playlist = serde_json::json!([{"playlist": "football"}]);
        assert_eq!(saved_rotation(&playlist), playlist.to_string());
        assert!(Rotation::parse(&saved_rotation(&none)).is_ok());
    }
    fn run(answers: &str, start: &ServerConfig) -> (io::Result<ServerConfig>, String) {
        let mut out = Vec::new();
        let result = wizard(&mut io::Cursor::new(answers.as_bytes()), &mut out, start);
        (result, String::from_utf8(out).unwrap())
    }

    #[test]
    fn enter_everywhere_gives_the_defaults() {
        let (config, text) = run(&"\n".repeat(14), &ServerConfig::default());
        assert_eq!(config.unwrap(), ServerConfig::default());
        assert!(text.contains("Time of day each round (default, random, map, dawn, day, dusk, night) [default]: "), "{text}");
        assert!(text.contains("Sky:         default time, default weather"), "{text}");
        assert!(text.contains("Server name [PeakRunner Community]: "), "{text}");
        assert!(text.contains("UDP port [7777]: "));
        assert!(text.contains("Announce to the public server list? (yes/no) [yes]: "));
        assert!(text.contains("Capture the Flag: Old Holler"), "the summary names the maps: {text}");
    }

    #[test]
    fn answers_build_a_valid_rotation_and_bad_answers_are_asked_again() {
        let ctf = maps_for(SupportedMode::Ctf);
        let answers = [
            "Springdale Fake", "Pine Ridge CTF",   // a reserved name is refused, then accepted
            "9", "1",                             // mode out of range, then CTF
            "2, 1, 2, x", "2,1",                  // a bad pick, then maps 2 and 1 (in that order)
            "40", "12",                           // players
            "13", "8",                            // bots
            "noon", "night",                      // time of day
            "random",                             // weather
            "hunter2",                            // password
            "80", "27015",                        // privileged port refused
            "maybe", "yes",                       // announce
            "<b>", "Denver, US",                  // location
            "no",                                 // UPnP
            "",                                   // confirm
        ].join("\n") + "\n";
        let (config, text) = run(&answers, &ServerConfig::default());
        let c = config.unwrap();
        assert_eq!(c.name, "Pine Ridge CTF");
        assert_eq!(c.rotation, serde_json::json!([{"map": ctf[1].key(), "mode": "ctf"}, {"map": ctf[0].key(), "mode": "ctf"}]));
        assert_eq!((c.max_players, c.bots, c.password.as_str(), c.port), (12, 8, "hunter2", 27015));
        assert_eq!((c.announce, c.location.as_str(), c.upnp), (true, "Denver, US", false));
        assert_eq!((c.time_of_day.as_str(), c.weather.as_str()), ("night", "random"));
        assert!(Rotation::parse(&c.rotation.to_string()).is_ok());
        for why in ["reserved for the official servers", "pick a number from the list", "isn't a number", "\"noon\" isn't one of",
            "a number from 2 to 28", "no, or a number from 2 to 12", "a port from 1024", "answer yes or no", "up to 40 letters"] {
            assert!(text.contains(why), "missing {why:?} in {text}");
        }
    }

    #[test]
    fn a_duel_servers_bots_are_sparring_bots_in_the_tower() {
        let duels = ServerConfig { rotation: serde_json::json!([{"playlist": "duel"}]), bots: 6, ..ServerConfig::default() };
        assert!(summary(&duels).contains("(sparring bots in the tower fill to 6)"), "{}", summary(&duels));
        let (config, text) = run(&("\n".repeat(12) + "y\n"), &duels);
        assert_eq!(config.unwrap().bots, 6);
        assert!(text.contains("Sparring bots: fill the lobby's tower"), "{text}");
        let ctf = ServerConfig { bots: 6, ..ServerConfig::default() };
        assert!(summary(&ctf).contains("(bots fill to 6)"));
    }

    #[test]
    fn a_saved_config_is_the_next_setups_default_and_no_redoes_the_questions() {
        let start = ServerConfig { name: "Night Owls".into(), rotation: serde_json::json!([{"map": "reefbreak", "mode": "team_deathmatch"}]),
            password: "secret".into(), bots: 6, max_players: 10, ..ServerConfig::default() };
        let tdm = maps_for(SupportedMode::TeamDeathmatch);
        let reef = tdm.iter().position(|m| *m == MapId::Reefbreak).unwrap() + 1;
        // Everything kept, but "no" at the confirmation: the questions come
        // round again (with the same defaults), then "yes".
        let (config, text) = run(&("\n".repeat(12) + "n\n" + "\n".repeat(12).as_str() + "y\n"), &start);
        assert_eq!(config.unwrap(), start);
        assert!(text.contains("Mode [3]: "), "{text}");
        assert!(text.contains(&format!("Maps (numbers like 1,3, or all) [{reef}]: ")), "{text}");
        assert!(text.contains("[keep]: ") && text.contains("up to how many players? (no, or 2-10) [6]: "), "{text}");
        assert!(text.contains("Once more from the top"));
        // "none" clears the password; "all" becomes the playlist.
        let (config, _) = run(&("\n\nall\n\n\n\n\nnone\n".to_string() + "\n".repeat(5).as_str()), &start);
        let c = config.unwrap();
        assert_eq!((c.password.as_str(), c.rotation.clone()), ("", serde_json::json!([{"playlist": "team_deathmatch"}])));
    }

    #[test]
    fn mixed_and_hand_edited_rotations() {
        // The mixed playlist asks no map question.
        let (config, text) = run(&("\n5\n".to_string() + "\n".repeat(11).as_str()), &ServerConfig::default());
        assert_eq!(config.unwrap().rotation, serde_json::json!([{"playlist": "ctf_cnh"}]));
        assert!(!text.contains("Maps for"));
        // A rotation the questions can't show is kept by default.
        let custom = ServerConfig { rotation: serde_json::json!([{"playlist": "ctf"}, {"playlist": "football"}]), ..ServerConfig::default() };
        let (config, text) = run(&"\n".repeat(13), &custom);
        assert_eq!(config.unwrap().rotation, custom.rotation);
        assert!(text.contains("0) Keep the current rotation (Capture the Flag: Old Holler"), "{text}");
    }

    #[test]
    fn rotation_order_comes_from_server_json_or_the_environment() {
        let none = |_: &str| None;
        assert!(!plan(Some(&ServerConfig::default()), &none).unwrap().settings.random_order, "in order by default");
        let random = ServerConfig { rotation_order: "random".into(), ..ServerConfig::default() };
        assert!(random.validate().is_ok());
        assert!(plan(Some(&random), &none).unwrap().settings.random_order);
        let env = |k: &str| (k == "PEAKRUNNER_ROTATION_ORDER").then(|| "in_order".to_string());
        assert!(!plan(Some(&random), &env).unwrap().settings.random_order, "the environment wins");
        let bad = ServerConfig { rotation_order: "sideways".into(), ..ServerConfig::default() };
        assert!(bad.validate().unwrap_err().contains("rotation_order"));
        let back: ServerConfig = serde_json::from_str(&serde_json::to_string(&random).unwrap()).unwrap();
        assert_eq!(back, random);
    }

    #[test]
    fn rabbit_and_team_rabbit_are_offered_on_their_maps() {
        // Rabbit, then "all" its maps: the playlist; Team Rabbit, then map 1.
        let (config, text) = run(&("\n6\nall\n".to_string() + "\n".repeat(11).as_str()), &ServerConfig::default());
        assert_eq!(config.unwrap().rotation, serde_json::json!([{"playlist": "rabbit"}]));
        assert!(text.contains("6) Rabbit") && text.contains("7) Team Rabbit"), "{text}");
        let first = maps_for(SupportedMode::TeamRabbit)[0];
        let (config, _) = run(&("\n7\n1\n".to_string() + "\n".repeat(11).as_str()), &ServerConfig::default());
        let rotation = config.unwrap().rotation;
        assert_eq!(rotation, serde_json::json!([{"map": first.key(), "mode": "team_rabbit"}]));
        assert!(Rotation::parse(&rotation.to_string()).is_ok());
    }

    #[test]
    fn end_of_input_cancels_instead_of_guessing() {
        let (config, _) = run("My Server\n1\n", &ServerConfig::default());
        assert_eq!(config.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn the_config_file_round_trips_and_bad_files_are_errors() {
        let dir = std::env::temp_dir().join(format!("peakrunner-setup-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(CONFIG_FILE);
        assert!(load(&path).unwrap().is_none());
        let config = ServerConfig { name: "Round Trip".into(), password: "pw".into(), public_address: "203.0.113.5".into(), ..ServerConfig::default() };
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), Some(config.clone()));
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "it may hold the password");
        }
        // Older fields missing take their defaults; unknown ones are refused.
        std::fs::write(&path, r#"{"schema":1,"name":"Hand Made","rotation":[{"playlist":"football"}],"max_players":16,"port":7780,"announce":false}"#).unwrap();
        let hand = load(&path).unwrap().unwrap();
        assert_eq!((hand.upnp, hand.directory.as_str(), hand.bots), (true, DEFAULT_DIRECTORY, 0));
        for bad in [r#"{"schema":1,"name":"X","rotation":[],"max_players":16,"port":7777,"announce":true}"#,
                    r#"{"schema":1,"name":"X","rotation":[{"playlist":"ctf"}],"max_players":16,"port":7777,"announce":true,"typo":1}"#,
                    r#"{"schema":1,"name":"X","rotation":[{"playlist":"ctf"}],"max_players":99,"port":7777,"announce":true}"#, "not json"] {
            std::fs::write(&path, bad).unwrap();
            assert!(load(&path).unwrap_err().to_string().contains("--setup"), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_environment_overrides_the_file_and_alone_behaves_as_before() {
        let none = |_: &str| None;
        // No file, no environment: what the Docker image always did.
        let p = plan(None, &none).unwrap();
        assert_eq!((p.settings.name.as_str(), p.settings.map.as_str(), p.settings.max_players, p.settings.rotation.clone()),
            ("Springdale Central", "old-holler", 8, None));
        assert_eq!((p.bind.to_string(), p.settings.health.as_deref(), p.announce.clone(), p.upnp),
            ("0.0.0.0:7777".into(), Some("0.0.0.0:8080"), None, false));
        let official = |k: &str| match k {
            "PEAKRUNNER_DIRECTORY_URL" => Some(DEFAULT_DIRECTORY.to_string()),
            "PEAKRUNNER_ADVERTISE_URL" => Some("quic://play.peakrunner.net:7778".to_string()),
            _ => None,
        };
        assert_eq!(plan(None, &official).unwrap().announce,
            Some(AnnouncePlan { directory: DEFAULT_DIRECTORY.into(), url: Some("quic://play.peakrunner.net:7778".into()) }));
        // A file: its values, a loopback status page, and the address comes from the directory.
        let file = ServerConfig { port: 27015, bots: 4, ..ServerConfig::default() };
        let p = plan(Some(&file), &none).unwrap();
        assert_eq!((p.settings.name.as_str(), p.bind.port(), p.settings.bots, p.settings.health.as_deref(), p.upnp),
            ("PeakRunner Community", 27015, 4, Some("127.0.0.1:27015"), true));
        assert_eq!(p.settings.rotation.as_deref(), Some(r#"[{"playlist":"ctf"}]"#));
        assert_eq!(p.announce, Some(AnnouncePlan { directory: DEFAULT_DIRECTORY.into(), url: None }));
        let fixed = ServerConfig { public_address: "2001:db8::5".into(), ..file.clone() };
        assert_eq!(plan(Some(&fixed), &none).unwrap().announce.unwrap().url.as_deref(), Some("quic://[2001:db8::5]:27015"));
        assert_eq!((p.lookup.clone(), p.private_url.clone()), (None, None), "announcing: no separate lookup");
        // Private: the directory is asked only with --publicip, and never
        // when the address is given.
        let private = plan(Some(&ServerConfig { announce: false, ..file.clone() }), &none).unwrap();
        assert_eq!((private.announce.clone(), private.private_url.clone()), (None, None));
        assert_eq!(private.public_ip_lookup(false), None, "no directory request without --publicip");
        assert_eq!(private.public_ip_lookup(true).as_deref(), Some(DEFAULT_DIRECTORY));
        let named = plan(Some(&ServerConfig { announce: false, public_address: "198.51.100.4".into(), ..file.clone() }), &none).unwrap();
        assert_eq!((named.public_ip_lookup(true), named.private_url.as_deref()), (None, Some("quic://198.51.100.4:27015")));
        assert_eq!(plan(None, &none).unwrap().public_ip_lookup(true), None, "environment-only servers contact nothing new");
        // The environment wins, field by field.
        let env = |k: &str| match k {
            "PEAKRUNNER_MATCH_NAME" => Some("Env Name".to_string()),
            "PEAKRUNNER_MAX_PLAYERS" => Some("20".to_string()),
            "PEAKRUNNER_MATCH_MAP" => Some("reefbreak".to_string()),
            "PEAKRUNNER_UDP_BIND" => Some("0.0.0.0:7790".to_string()),
            _ => None,
        };
        let p = plan(Some(&file), &env).unwrap();
        assert_eq!((p.settings.name.as_str(), p.settings.max_players, p.settings.map.as_str(), p.settings.rotation.clone(), p.bind.port()),
            ("Env Name", 20, "reefbreak", None, 7790));
        assert!(plan(None, &|k: &str| (k == "PEAKRUNNER_BOTS").then(|| "lots".to_string())).is_err());
        // Time of day and weather: as the mode does unless the file or the
        // environment says.
        assert_eq!(p.settings.forecast, Forecast::default());
        let rainy = ServerConfig { time_of_day: "random".into(), weather: "rain".into(), ..file.clone() };
        assert_eq!(plan(Some(&rainy), &none).unwrap().settings.forecast, Forecast { time: Pick::Random, weather: Pick::Always(Weather::Rain) });
        let json = serde_json::to_string(&file).unwrap();
        assert!(!json.contains("time_of_day") && !json.contains("weather"), "left out of files that don't use them");
        let sky = |k: &str| match k { "PEAKRUNNER_TIME_OF_DAY" => Some("Night".to_string()), "PEAKRUNNER_WEATHER" => Some("default".to_string()), _ => None };
        assert_eq!(plan(Some(&rainy), &sky).unwrap().settings.forecast, Forecast { time: Pick::Always(TimeOfDay::Night), weather: Pick::Mode });
        assert!(plan(None, &|k: &str| (k == "PEAKRUNNER_WEATHER").then(|| "hail".to_string())).is_err());
        assert!(ServerConfig { weather: "hail".into(), ..file.clone() }.validate().unwrap_err().contains("weather"));
    }

    #[test]
    fn config_lives_in_the_per_user_directory_unless_told_otherwise() {
        let home = Some(PathBuf::from("/home/pat"));
        assert_eq!(user_dir("linux", home.clone(), None, None), Some(PathBuf::from("/home/pat/.config/PeakRunner/server")));
        assert_eq!(user_dir("macos", Some("/Users/pat".into()), None, None),
            Some(PathBuf::from("/Users/pat/Library/Application Support/PeakRunner/server")));
        assert_eq!(user_dir("linux", Some("relative".into()), None, None), None);
        let env = |k: &str| (k == "PEAKRUNNER_SERVER_DIR").then(|| "/srv/pr".to_string());
        assert_eq!(config_dir(&[], &env), Some(PathBuf::from("/srv/pr")));
        assert_eq!(config_dir(&["--config-dir".into(), "/tmp/two".into()], &env), Some(PathBuf::from("/tmp/two")));
    }

    #[test]
    fn the_content_folder_is_made_beside_server_json_and_its_readme_kept() {
        let dir = std::env::temp_dir().join(format!("peakrunner-content-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let content = prepare_content(&dir).unwrap();
        assert_eq!(content, dir.join(CONTENT_DIR));
        assert!(content.join("maps").is_dir() && content.join("mods").is_dir());
        assert!(std::fs::read_to_string(content.join("README.txt")).unwrap().contains("--check"));
        std::fs::write(content.join("README.txt"), "mine").unwrap();
        prepare_content(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(content.join("README.txt")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
