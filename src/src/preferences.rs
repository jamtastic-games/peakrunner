//! Per-user client preferences, deliberately separate from game installs/maps.
//! Passwords, session tokens, chat and server-provided data are never serialized.
use peakrunner_core::bot_nav::Difficulty;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub schema: u32,
    pub name: String,
    pub directory: String,
    pub direct: String,
    /// 4x multisample anti-aliasing when the GPU supports it.
    pub antialiasing: bool,
    /// Offline bot difficulty. An unknown value reads as the default.
    #[serde(deserialize_with = "lenient_difficulty")]
    pub bot_difficulty: Difficulty,
    /// Glow around lights, flames and the sun. An unknown value reads as the default.
    #[serde(deserialize_with = "lenient_bloom")]
    pub bloom: Bloom,
    /// Keyboard bindings. Unknown or bad entries read as the defaults.
    pub keys: crate::keybinds::Keybinds,
    /// Graphics screen settings. Bad values read as the defaults.
    #[serde(deserialize_with = "lenient_graphics")]
    pub graphics: Graphics,
    /// A server's maps up to this many megabytes download without asking
    /// (docs/modding.md, P2); 0, the default, always asks.
    pub auto_download_mb: u32,
    /// Starred servers in Find match, shown (and joinable) even when they
    /// aren't listed.
    pub favorites: Vec<Favorite>,
    /// Saved inventory setups, used at a station with F1 to F4.
    pub setups: [Option<SavedSetup>; SETUPS_MAX],
    /// The game's anonymous ID: a random number (32 hex digits) made the
    /// first time it joins a server and sent when joining, so the official
    /// servers can count how many people play. Nothing ties it to the name.
    pub player: String,
}

/// Saved inventory setups (favorites) kept.
pub const SETUPS_MAX: usize = 4;

/// A saved inventory setup: an armor's id ("light", "heavy" or a mod's),
/// weapons in slot order by family (`arsenal::KIND_KEYS` for the base game:
/// "launcher" is the grenade launcher or the mortar, whichever the armor
/// carries) and a pack's id (empty for none). Names the session hasn't got
/// are skipped when it's used.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSetup { pub armor: String, pub weapons: Vec<String>, pub pack: String }

impl SavedSetup {
    /// What a player has on now.
    pub fn of(p: &peakrunner_core::sim::Player) -> Self {
        let weapons = &peakrunner_core::registry::registry().weapons;
        Self {
            armor: p.armor.id().to_string(),
            weapons: p.loadout.kinds().filter_map(|k| weapons.get(k as usize)).map(|w| w.family.clone()).collect(),
            pack: p.pack.map_or(String::new(), |k| k.key().to_string()),
        }
    }
    /// The armor it names, if the session has it.
    fn armor(&self) -> Option<peakrunner_core::sim::ArmorClass> { peakrunner_core::sim::ArmorClass::by_id(&self.armor) }
    /// Its weapons as wire numbers: each family's weapon for its armor (or
    /// any of the family; the server fits them to the armor worn).
    fn kinds(&self) -> Vec<u8> {
        use peakrunner_core::sim::arsenal::family_weapon;
        let r = peakrunner_core::registry::registry();
        self.weapons.iter().filter_map(|f| self.armor().and_then(|a| family_weapon(a, f))
            .or_else(|| r.weapons.iter().find(|(_, _, w)| w.family == *f).map(|(k, _, _)| k as u8))).collect()
    }
    /// The intent that equips it at a station.
    pub fn equip(&self) -> peakrunner_core::sim::loadout::Equip {
        use peakrunner_core::sim::{arsenal::Loadout, packs::PackKind, loadout::Equip};
        Equip { armor: self.armor().map_or(0, |a| a.0 + 1), weapons: Loadout::of(&self.kinds()),
            pack: PackKind::from_key(&self.pack).map_or(0, |k| k.0 + 1) }
    }
    /// "Heavy · Railgun, Plasma gun · Wall" for the inventory screen.
    pub fn describe(&self) -> String {
        use peakrunner_core::sim::{arsenal::name, packs::PackKind, loadout::capitalized};
        let weapons: Vec<&str> = self.kinds().into_iter().map(name).collect();
        let pack = PackKind::from_key(&self.pack).map(PackKind::name);
        let armor = self.armor().map(|a| capitalized(a.id()));
        [armor.as_deref(), Some(&*weapons.join(", ")).filter(|w| !w.is_empty()), pack]
            .into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }
}


/// A starred server: its listing ID, the name it had, and the address to
/// join it by (an invite link keeps its public key fingerprint).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite { pub id: String, pub name: String, pub address: String }

/// Most favorites kept (the settings file stays small).
pub const FAVORITES_MAX: usize = 32;

impl Favorite {
    /// A favorite fit to save: a plain ID and name, and an address with no
    /// credentials or secrets (as the direct address).
    fn checked(&self) -> Option<Favorite> {
        let plain = |t: &str, max: usize| !t.trim().is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control);
        (plain(&self.id, 128) && plain(&self.name, 64)).then(|| ())?;
        Some(Favorite { id: self.id.clone(), name: self.name.clone(), address: address(&self.address)? })
    }
}

/// Render scale steps offered on the Graphics screen (percent of the
/// window's pixels the 3D scene is drawn at; the HUD stays sharp).
pub(crate) const RENDER_SCALES: [u8; 4] = [100, 85, 70, 50];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Graphics {
    pub render_scale: u8,
    /// Wait for the display's refresh (applied live, and at launch).
    pub vsync: bool,
    pub fullscreen: bool,
    /// The frame-timing / FPS overlay (also F3).
    pub show_fps: bool,
}

impl Default for Graphics {
    fn default() -> Self { Self { render_scale: 100, vsync: true, fullscreen: false, show_fps: false } }
}

impl Graphics {
    fn sanitized(mut self) -> Self {
        if !RENDER_SCALES.contains(&self.render_scale) { self.render_scale = 100; }
        self
    }
    pub fn scale(self) -> f32 { self.sanitized().render_scale as f32 / 100.0 }
}

fn lenient_graphics<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Graphics, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value::<Graphics>(value).unwrap_or_default().sanitized())
}

/// The saved graphics settings for window creation (vsync, fullscreen),
/// before the app exists.
pub fn startup_graphics() -> (bool, bool) {
    let g = Store::user().saved.graphics;
    (g.vsync, g.fullscreen)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Bloom {
    Off,
    #[default]
    Low,
    High,
}

impl Bloom {
    /// `scene::BLOOM_LEVEL` value.
    pub fn level(self) -> u8 {
        match self {
            Bloom::Off => 0,
            Bloom::Low => 1,
            Bloom::High => 2,
        }
    }
}

fn lenient_difficulty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Difficulty, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn lenient_bloom<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Bloom, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema: 1,
            name: "Skier".into(),
            directory: "https://dir.peakrunner.net/servers".into(),
            direct: "quic://play.peakrunner.net:7777".into(),
            antialiasing: true,
            bot_difficulty: Difficulty::default(),
            bloom: Bloom::default(),
            keys: Default::default(),
            graphics: Graphics::default(),
            auto_download_mb: 0,
            favorites: Vec::new(),
            setups: Default::default(),
            player: String::new(),
        }
    }
}

/// A saved anonymous ID: 32 lowercase hex digits, not all zero.
fn player_id(saved: &str) -> Option<u128> {
    if saved.len() != 32 || !saved.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return None; }
    u128::from_str_radix(saved, 16).ok().filter(|&id| id != 0)
}

fn address(raw: &str) -> Option<String> {
    let value = raw.trim();
    // A server fingerprint (`#sha256=` and 64 hex digits, from an invite
    // link) is public, not a secret, so it may be kept; no other fragment.
    let (base, pin) = match value.split_once('#') {
        Some((base, pin)) => (base, Some(pin)),
        None => (value, None),
    };
    let pin_ok = pin.is_none_or(|p| p.strip_prefix("sha256=")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())));
    // No credentials or URL query secrets in a preferences file. Network code
    // still validates scheme/host/transport independently before connecting.
    (!value.is_empty()
        && raw.len() <= 2048
        && !raw.chars().any(char::is_control)
        && !base.contains(['@', '?', '#'])
        && pin_ok)
    .then(|| value.to_owned())
}

impl Preferences {
    pub fn edited(&self, name: &str, directory: &str, direct: &str) -> Self {
        Self {
            schema: 1,
            name: peakrunner_core::names::validate(name)
                .map(str::to_owned)
                .unwrap_or_else(|| self.name.clone()),
            directory: address(directory).unwrap_or_else(|| self.directory.clone()),
            direct: address(direct).unwrap_or_else(|| self.direct.clone()),
            antialiasing: self.antialiasing,
            bot_difficulty: self.bot_difficulty,
            bloom: self.bloom,
            keys: self.keys.clone(),
            graphics: self.graphics,
            auto_download_mb: self.auto_download_mb,
            favorites: self.favorites.clone(),
            setups: self.setups.clone(),
            player: self.player.clone(),
        }
    }
}

/// Where downloaded maps are kept (the catalog's cache): user data, not
/// settings, so on Windows it is local, never in a roaming profile.
/// `PEAKRUNNER_CONFIG_DIR` moves it beside the settings.
pub fn download_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PEAKRUNNER_CONFIG_DIR") {
        let dir = PathBuf::from(dir);
        return dir.is_absolute().then(|| dir.join("downloads"));
    }
    let absolute = |p: Option<PathBuf>| p.filter(|p| p.is_absolute());
    let home = absolute(std::env::home_dir());
    let base = match std::env::consts::OS {
        "macos" => home?.join("Library/Application Support/PeakRunner"),
        "windows" => absolute(std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
            .or_else(|| home.map(|p| p.join("AppData/Local")))?.join("PeakRunner"),
        _ => absolute(std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
            .or_else(|| home.map(|p| p.join(".local/share")))?.join("PeakRunner"),
    };
    Some(base.join("downloads"))
}

/// The settings file, `client.json`, when there is an absolute place for it.
fn settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PEAKRUNNER_CONFIG_DIR") {
        let dir = PathBuf::from(dir);
        return dir.is_absolute().then(|| dir.join("client.json"));
    }
    path_for(
        std::env::consts::OS,
        std::env::home_dir(),
        std::env::var_os("APPDATA").map(PathBuf::from),
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
    )
}

/// The player's own maps and mods: `content/` in the Host game's folder,
/// which this game and the server it hosts both read
/// (docs/making-content.md).
pub fn own_content_dir() -> Option<PathBuf> {
    crate::host::folder(settings_path().as_deref()).map(|dir| dir.join("content"))
}

fn path_for(
    os: &str,
    home: Option<PathBuf>,
    roaming: Option<PathBuf>,
    xdg: Option<PathBuf>,
) -> Option<PathBuf> {
    let absolute = |p: Option<PathBuf>| p.filter(|p| p.is_absolute());
    let home = absolute(home);
    let base = match os {
        "macos" => home?.join("Library/Application Support"),
        "windows" => absolute(roaming).or_else(|| home.map(|p| p.join("AppData/Roaming")))?,
        _ => absolute(xdg).or_else(|| home.map(|p| p.join(".config")))?,
    };
    Some(base.join("PeakRunner").join("client.json"))
}

pub(crate) struct Store {
    pub path: Option<PathBuf>,
    pub saved: Preferences,
    pub warning: Option<String>,
}

impl Store {
    pub fn memory() -> Self {
        Self {
            path: None,
            saved: Preferences::default(),
            warning: None,
        }
    }

    pub fn user() -> Self {
        // Automated direct-join sessions must not overwrite the player's settings.
        if std::env::var_os("PEAKRUNNER_JOIN").is_some() {
            return Self::memory();
        }
        match settings_path() {
            Some(path) => Self::load(path),
            None => Self {
                warning: Some(
                    "No absolute user configuration folder; settings are temporary.".into(),
                ),
                ..Self::memory()
            },
        }
    }

    pub fn load(path: PathBuf) -> Self {
        let mut store = Self {
            path: Some(path.clone()),
            ..Self::memory()
        };
        match read(&path) {
            Ok(p) => store.saved = p,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {
                store.warning = Some(
                    "Couldn't read saved settings; using defaults. The original file is unchanged."
                        .into(),
                )
            }
        }
        store
    }

    pub fn save(&mut self, name: &str, directory: &str, direct: &str) {
        let next = self.saved.edited(name, directory, direct);
        self.commit(next);
    }

    /// The game's anonymous ID (`Preferences::player`), made and saved the
    /// first time it's asked for. 0 (none sent) if no randomness is to be had.
    pub fn player_id(&mut self) -> u128 {
        if let Some(id) = player_id(&self.saved.player) { return id; }
        let mut bytes = [0u8; 16];
        if getrandom::fill(&mut bytes).is_err() { return 0; }
        let id = u128::from_le_bytes(bytes).max(1);
        let next = Preferences { player: format!("{id:032x}"), ..self.saved.clone() };
        self.commit(next);
        id
    }

    pub fn set_antialiasing(&mut self, on: bool) {
        let next = Preferences { antialiasing: on, ..self.saved.clone() };
        self.commit(next);
    }

    /// Save (or clear) inventory setup `n` (0-based).
    pub fn set_setup(&mut self, n: usize, setup: Option<SavedSetup>) {
        if n >= SETUPS_MAX { return; }
        let mut setups = self.saved.setups.clone();
        setups[n] = setup;
        let next = Preferences { setups, ..self.saved.clone() };
        self.commit(next);
    }

    pub fn set_bot_difficulty(&mut self, difficulty: Difficulty) {
        let next = Preferences { bot_difficulty: difficulty, ..self.saved.clone() };
        self.commit(next);
    }

    pub fn set_bloom(&mut self, bloom: Bloom) {
        let next = Preferences { bloom, ..self.saved.clone() };
        self.commit(next);
    }

    pub fn set_graphics(&mut self, graphics: Graphics) {
        let next = Preferences { graphics: graphics.sanitized(), ..self.saved.clone() };
        self.commit(next);
    }

    /// Star or unstar a server (by its listing ID).
    pub fn toggle_favorite(&mut self, favorite: Favorite) {
        let mut favorites = self.saved.favorites.clone();
        match favorites.iter().position(|f| f.id == favorite.id) {
            Some(i) => { favorites.remove(i); }
            None => {
                let Some(favorite) = favorite.checked() else { return };
                if favorites.len() >= FAVORITES_MAX { return; }
                favorites.push(favorite);
            }
        }
        let next = Preferences { favorites, ..self.saved.clone() };
        self.commit(next);
    }

    pub fn set_keys(&mut self, keys: crate::keybinds::Keybinds) {
        let next = Preferences { keys, ..self.saved.clone() };
        self.commit(next);
    }

    fn commit(&mut self, next: Preferences) {
        if next == self.saved {
            return;
        }
        // The change applies for this session even when it can't be saved.
        let result = self.path.as_deref().map(|path| write(path, &next));
        self.saved = next;
        match result {
            None => {}
            Some(Ok(())) => self.warning = None,
            Some(Err(_)) => {
                self.warning =
                    Some("Couldn't save settings. Changes will last only for this session.".into())
            }
        }
    }
}

fn read(path: &Path) -> io::Result<Preferences> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(16 * 1024 + 1)
        .read_to_end(&mut data)?;
    if data.len() > 16 * 1024 {
        return Err(io::Error::other("oversized preferences"));
    }
    let raw: Preferences = serde_json::from_slice(&data).map_err(io::Error::other)?;
    if raw.schema != 1 {
        return Err(io::Error::other("unsupported preferences schema"));
    }
    let mut prefs = Preferences::default().edited(&raw.name, &raw.directory, &raw.direct);
    prefs.antialiasing = raw.antialiasing;
    prefs.bot_difficulty = raw.bot_difficulty;
    prefs.bloom = raw.bloom;
    prefs.keys = raw.keys;
    prefs.graphics = raw.graphics.sanitized();
    prefs.auto_download_mb = raw.auto_download_mb.min(10_000);
    prefs.favorites = raw.favorites.iter().filter_map(Favorite::checked).take(FAVORITES_MAX).collect();
    // A damaged ID is dropped; a new one is made when it's next needed.
    prefs.player = player_id(&raw.player).map(|id| format!("{id:032x}")).unwrap_or_default();
    Ok(prefs)
}

fn write(path: &Path, prefs: &Preferences) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    fs::create_dir_all(parent)?;
    static NONCE: AtomicU64 = AtomicU64::new(0);
    let temp = parent.join(format!(
        ".client-{}-{}.tmp",
        std::process::id(),
        NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    let result = (|| {
        file.write_all(&serde_json::to_vec_pretty(prefs).map_err(io::Error::other)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        // Same-directory rename replaces atomically on supported native systems;
        // never delete the previous preferences file first.
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "peakrunner-prefs-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    #[test]
    fn paths_follow_native_conventions_and_ignore_relative_xdg() {
        let root = std::env::temp_dir().join("prefs-path-test");
        let home = Some(root.join("home"));
        assert_eq!(
            path_for("macos", home.clone(), None, None).unwrap(),
            root.join("home/Library/Application Support/PeakRunner/client.json")
        );
        assert_eq!(
            path_for("linux", home.clone(), None, Some("relative".into())).unwrap(),
            root.join("home/.config/PeakRunner/client.json")
        );
        assert_eq!(
            path_for("linux", home.clone(), None, Some(root.join("custom"))).unwrap(),
            root.join("custom/PeakRunner/client.json")
        );
        assert_eq!(
            path_for("windows", home, Some(root.join("roaming")), None).unwrap(),
            root.join("roaming/PeakRunner/client.json")
        );
        assert!(path_for("linux", None, None, None).is_none());
    }
    #[test]
    fn the_anonymous_id_is_made_once_and_kept() {
        let dir = temp();
        let path = dir.join("client.json");
        let mut s = Store::load(path.clone());
        assert!(s.saved.player.is_empty(), "none until it's needed");
        let id = s.player_id();
        assert_ne!(id, 0);
        assert_eq!(s.saved.player, format!("{id:032x}"));
        assert_eq!(s.player_id(), id, "the same every time");
        // Saved, and kept through a rename and a reload.
        s.save("Ridge", "https://dir.peakrunner.net/servers", "quic://play.peakrunner.net:7777");
        let mut again = Store::load(path);
        assert_eq!(again.saved.name, "Ridge");
        assert_eq!(again.player_id(), id);
        // A damaged one is replaced.
        assert_eq!(player_id("not an id"), None);
        assert_eq!(player_id(&"0".repeat(32)), None);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn round_trip_replacement_and_password_exclusion() {
        let dir = temp();
        let path = dir.join("client.json");
        let mut s = Store::load(path.clone());
        s.save(
            "  Pilot 2  ",
            "https://example.net/servers",
            "quic://example.net:7777",
        );
        assert_eq!(Store::load(path.clone()).saved.name, "Pilot 2");
        s.save(
            "Pilot 3",
            &s.saved.directory.clone(),
            &s.saved.direct.clone(),
        );
        assert_eq!(Store::load(path.clone()).saved.name, "Pilot 3");
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        // schema, name, directory, direct, antialiasing, bot_difficulty, bloom, keys, graphics,
        // auto_download_mb, favorites, setups, player (empty until it's made): nothing else.
        assert_eq!(value.as_object().unwrap().len(), 13);
        assert_eq!(value["auto_download_mb"], 0, "downloads ask by default");
        assert!(value.get("password").is_none());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn antialiasing_defaults_on_persists_and_survives_name_edits() {
        let dir = temp();
        let path = dir.join("client.json");
        // A file written before the setting existed keeps anti-aliasing on.
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, br#"{"schema":1,"name":"Pilot","directory":"https://example.net/s","direct":"quic://example.net:7777"}"#).unwrap();
        let mut s = Store::load(path.clone());
        assert!(s.saved.antialiasing);
        s.set_antialiasing(false);
        assert!(!Store::load(path.clone()).saved.antialiasing);
        s.save("Pilot 9", &s.saved.directory.clone(), &s.saved.direct.clone());
        let loaded = Store::load(path.clone()).saved;
        assert_eq!(loaded.name, "Pilot 9");
        assert!(!loaded.antialiasing, "editing the name must not reset anti-aliasing");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bloom_defaults_low_persists_and_tolerates_unknown_values() {
        let dir = temp();
        let path = dir.join("client.json");
        fs::create_dir_all(&dir).unwrap();
        // A file written before the setting existed gets the default.
        fs::write(&path, br#"{"schema":1,"name":"Pilot","directory":"https://example.net/s","direct":"quic://example.net:7777"}"#).unwrap();
        let mut s = Store::load(path.clone());
        assert_eq!(s.saved.bloom, Bloom::Low);
        s.set_bloom(Bloom::High);
        assert_eq!(Store::load(path.clone()).saved.bloom, Bloom::High);
        s.save("Pilot 5", &s.saved.directory.clone(), &s.saved.direct.clone());
        assert_eq!(Store::load(path.clone()).saved.bloom, Bloom::High, "a name edit keeps bloom");
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains(r#""bloom": "high""#) || raw.contains(r#""bloom":"high""#));
        fs::write(&path, raw.replace("high", "blinding")).unwrap();
        assert_eq!(Store::load(path.clone()).saved.bloom, Bloom::Low);
        assert_eq!([Bloom::Off.level(), Bloom::Low.level(), Bloom::High.level()], [0, 1, 2]);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn bot_difficulty_defaults_to_normal_persists_and_tolerates_unknown_values() {
        let dir = temp();
        let path = dir.join("client.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, br#"{"schema":1,"name":"Pilot","directory":"https://example.net/s","direct":"quic://example.net:7777"}"#).unwrap();
        let mut s = Store::load(path.clone());
        assert_eq!(s.saved.bot_difficulty, Difficulty::Normal);
        s.set_bot_difficulty(Difficulty::Hard);
        assert_eq!(Store::load(path.clone()).saved.bot_difficulty, Difficulty::Hard);
        s.save("Pilot 4", &s.saved.directory.clone(), &s.saved.direct.clone());
        assert_eq!(Store::load(path.clone()).saved.bot_difficulty, Difficulty::Hard, "editing the name keeps it");
        // A value from a newer build must not throw away the name.
        fs::write(&path, br#"{"schema":1,"name":"Pilot 5","directory":"https://example.net/s","direct":"quic://example.net:7777","bot_difficulty":"Brutal"}"#).unwrap();
        let loaded = Store::load(path.clone()).saved;
        assert_eq!((loaded.name.as_str(), loaded.bot_difficulty), ("Pilot 5", Difficulty::Normal));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_or_secret_fields_keep_last_valid_values() {
        let old = Preferences::default();
        assert_eq!(
            old.edited(
                "<script>",
                "https://name:secret@example.net",
                "quic://host?token=secret"
            ),
            old
        );
        assert_eq!(old.edited("\n", &"x".repeat(2049), "host\n"), old);
    }
    #[test]
    fn an_invite_fingerprint_is_kept_but_no_other_fragment() {
        let old = Preferences::default();
        let pin = "ab".repeat(32);
        for kept in [format!("peakrunner://join/203.0.113.5:7777#sha256={pin}"), format!("quic://203.0.113.5:7777#sha256={pin}")] {
            assert_eq!(old.edited("Pilot", "", &kept).direct, kept);
        }
        for refused in [format!("quic://h:7777#sha256={}", &pin[1..]), "quic://h:7777#token=secret".into(),
            format!("quic://h:7777#sha256={pin}#x"), format!("quic://u@h:7777#sha256={pin}"), format!("quic://h?x#sha256={pin}")] {
            assert_eq!(old.edited("Pilot", "", &refused).direct, old.direct, "{refused}");
        }
    }
    #[test]
    fn a_stored_map_key_for_a_removed_map_is_ignored() {
        // Preferences never persist a map; the client starts on its default map.
        // A file edited by hand or by a future build must not break loading.
        let dir = temp();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("client.json");
        fs::write(&path, br#"{"schema":1,"name":"Pilot","map":"no-such-map"}"#).unwrap();
        let s = Store::load(path);
        assert!(s.warning.is_none());
        assert_eq!(s.saved.name, "Pilot");
        assert_eq!(peakrunner_core::terrain::MapId::parse("no-such-map"), None);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn corrupt_oversized_future_and_unwritable_files_are_nonfatal() {
        let dir = temp();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("client.json");
        for bytes in [
            b"not json".to_vec(),
            vec![b'x'; 16385],
            br#"{"schema":2}"#.to_vec(),
        ] {
            fs::write(&path, &bytes).unwrap();
            let s = Store::load(path.clone());
            assert!(s.warning.is_some());
            assert_eq!(s.saved, Preferences::default());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let mut s = Store::load(path.join("cannot-exist.json"));
        s.save(
            "Pilot",
            &Preferences::default().directory,
            &Preferences::default().direct,
        );
        assert!(s.warning.is_some());
        assert_eq!(s.saved.name, "Pilot", "unsaved changes still last for the session");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn favorites_and_the_download_limit_survive_a_restart() {
        let path = temp().join("client.json");
        let mut s = Store::load(path.clone());
        let spree = Favorite { id: "198.51.100.7-7781".into(), name: "Trial Spree TDM".into(),
            address: format!("quic://198.51.100.7:7781#sha256={}", "ab".repeat(32)) };
        s.toggle_favorite(spree.clone());
        s.toggle_favorite(Favorite { id: "bad".into(), name: "Bad".into(), address: "quic://user:pw@example.net:7777".into() });
        assert_eq!(Store::load(path.clone()).saved.favorites, vec![spree.clone()], "credentials are never saved");
        // A hand-edited file: the download limit is kept, a bad favorite dropped.
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["auto_download_mb"] = 50.into();
        value["favorites"].as_array_mut().unwrap().push(serde_json::json!({"id": "x\n", "name": "", "address": "quic://h:1"}));
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let mut s = Store::load(path.clone());
        assert_eq!((s.saved.auto_download_mb, s.saved.favorites.len()), (50, 1));
        s.toggle_favorite(spree);
        assert!(Store::load(path).saved.favorites.is_empty(), "starring again removes it");
    }
}
