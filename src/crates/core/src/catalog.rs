//! The engine's content catalog: every map installed where the engine looks
//! for content. Menus, the Host screen, the server setup, rotations, votes
//! and server listings ask the catalog; nothing lists maps in code.
//!
//! **Content roots**, most preferred first (a key found in two roots comes
//! from the first):
//! 1. `PEAKRUNNER_PACK_DIR`: one or more folders (joined like `PATH`), for
//!    map and mod authors.
//! 2. Folders a program adds (`add_root`): a server's `content/` beside its
//!    `server.json`, where a host drops maps and mods.
//! 3. `packs/` beside the executable, and `../Resources/packs/` inside a
//!    macOS app: what installs ship.
//! 4. `src/assets/` of the source tree the engine was built from, when it
//!    exists: development builds and tests.
//!
//! `PEAKRUNNER_MAP_PACK=<map folder>` still replaces Old Holler's folder (the
//! older launcher sets it, and `docs/map-pipeline.md` previews new maps
//! there). The browser build has no folders: its default maps are embedded.
//!
//! **A map folder** is `<root>/maps/<key>/`: the pipeline's `map.json` and its
//! payloads (each also accepted zlib-compressed as `<name>.zlib`), plus an
//! optional hand-written `info.json` (`docs/modding.md`):
//!
//! ```json
//! { "name": "Old Holler", "note": "Menu description.",
//!   "bases": [[x, y, z], [x, y, z]], "dust": [r, g, b] }
//! ```
//!
//! A map's identity is its fingerprint, the SHA-256 of `map.json` (which
//! lists every payload's SHA-256). `info.json` is presentation only. Packs are
//! validated in full when first used; a bad one is refused with a reason
//! (`problem`), never crashes the game.
//!
//! **Downloads** (a game client only, `set_cache_dir`) live in a per-user
//! cache, `<cache>/maps/<key>/<fingerprint>/`. They are never listed in menus
//! and never answer a key on their own: a session with a server binds each key
//! the server needs to the exact fingerprint it named (`bind_session`), which
//! may be a download, until `end_session`.
use crate::map_pack::MapPack;
use crate::terrain::MapId;
use glam::Vec3;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Keys with fixed ids (the `MapId` constants), in menu order after Valley,
/// the procedural test fixture. Installed maps with other keys follow.
pub(crate) const WELL_KNOWN: [&str; 11] = ["valley", "old-holler", "tower-complex", "cairnhold", "frostline",
    "dustreach", "longfield", "highgoal", "ozarktic-blast", "reefbreak", "ashveil"];

/// Most maps one process can know (installed and downloaded).
const CAPACITY: usize = 1024;
/// Largest `info.json` read.
const INFO_MAX: u64 = 16 * 1024;
/// Largest `map.json` read while scanning (its name only).
const MANIFEST_MAX: u64 = 8_000_000;

/// One map the engine knows: installed or not, with its pack loaded on first use.
pub struct MapEntry {
    pub id: MapId,
    pub key: &'static str,
    /// Menu name and description.
    pub name: &'static str,
    pub note: &'static str,
    /// Colour of the soil or snow blasts throw up (0..1 RGB).
    pub dust: [f32; 3],
    bases: Option<[Vec3; 2]>,
    source: Source,
    /// A download: not in menus, found by fingerprint or a session binding.
    cached: bool,
    fingerprint: OnceLock<Option<String>>,
    pack: OnceLock<Result<&'static MapPack, String>>,
    /// Bot route graph (`bot_nav`), built once per map.
    pub(crate) nav: OnceLock<Option<crate::bot_nav::NavGraph>>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) nav_started: AtomicBool,
    /// Safe menu-orbit altitude (`terrain::overview_height`).
    pub(crate) overview: OnceLock<f32>,
}

enum Source {
    /// Valley, or a well-known map that isn't installed.
    None,
    Folder(PathBuf),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Embedded(crate::map_pack::Embedded),
}

impl MapEntry {
    /// Installed: a folder (or embedded copy) to load. Valley is always
    /// available but has no pack.
    pub fn installed(&self) -> bool { !matches!(self.source, Source::None) }

    /// The validated pack, loading it on first use. `None` for Valley, a map
    /// that isn't installed, or one that failed validation (see `problem`).
    pub fn pack(&self) -> Option<&'static MapPack> {
        if matches!(self.source, Source::None) { return None; }
        self.pack.get_or_init(|| {
            let loaded = match &self.source {
                Source::Folder(dir) => MapPack::load(dir).map_err(|e| format!("{}: {e}", dir.display())),
                Source::Embedded(assets) => MapPack::embedded(*assets),
                Source::None => unreachable!(),
            };
            match loaded {
                Ok(pack) => Ok(&*Box::leak(Box::new(pack))),
                Err(e) => {
                    eprintln!("PeakRunner: map {} is unusable and was left out: {e}", self.key);
                    Err(e)
                }
            }
        }).as_ref().ok().copied()
    }

    /// The map's fingerprint (SHA-256 of `map.json`), read without loading
    /// the payloads; `None` for Valley or a map that isn't installed.
    pub fn fingerprint(&self) -> Option<&str> {
        self.fingerprint.get_or_init(|| {
            use sha2::{Digest, Sha256};
            let json = match &self.source {
                Source::Folder(dir) => {
                    let path = dir.join("map.json");
                    if path.metadata().ok()?.len() > MANIFEST_MAX { return None; }
                    std::fs::read(path).ok()?
                }
                Source::Embedded(assets) => assets("map.json").ok()?,
                Source::None => return None,
            };
            Some(format!("{:x}", Sha256::digest(&json)))
        }).as_deref()
    }

    /// The folder this map loads from (none: Valley, embedded or missing).
    pub fn folder(&self) -> Option<&Path> {
        match &self.source { Source::Folder(dir) => Some(dir), _ => None }
    }

    /// A file a server may send for this map (`ClientMsg::Fetch`): `map.json`,
    /// `info.json` or a payload the manifest lists, as it is stored (the path,
    /// and whether it is zlib-compressed). Nothing else.
    pub fn download_file(&self, name: &str) -> Option<(PathBuf, bool)> {
        let dir = self.folder()?;
        let listed = name == "map.json" || name == "info.json" || self.pack()?.manifest.files.contains_key(name);
        if !listed || name.contains(['/', '\\']) { return None; }
        let raw = dir.join(name);
        if raw.is_file() { return Some((raw, false)); }
        let packed = dir.join(format!("{name}.zlib"));
        packed.is_file().then_some((packed, true))
    }

    /// Bytes a client downloads for this map (its files as stored).
    pub fn download_bytes(&self) -> u64 {
        let Some(pack) = self.pack() else { return 0 };
        ["map.json", "info.json"].into_iter().chain(pack.manifest.files.keys().map(String::as_str))
            .filter_map(|name| self.download_file(name))
            .filter_map(|(path, _)| path.metadata().ok()).map(|m| m.len()).sum()
    }

    /// Why this map can't be played, if it can't.
    pub fn problem(&self) -> Option<String> {
        if matches!(self.source, Source::None) {
            return (self.id != MapId::Valley).then(|| format!("map {} is not installed", self.key));
        }
        self.pack();
        self.pack.get().and_then(|r| r.as_ref().err().cloned())
    }

    /// The two base centres: `info.json`, else the pack's flags.
    pub fn bases(&self) -> [Vec3; 2] {
        if let Some(b) = self.bases { return b; }
        match self.pack() {
            Some(p) => [Vec3::from_array(p.manifest.flags[0]), Vec3::from_array(p.manifest.flags[1])],
            None => [crate::terrain::EMBER_HOME, crate::terrain::GLACIER_HOME],
        }
    }
}

static SLOTS: [OnceLock<&'static MapEntry>; CAPACITY] = [const { OnceLock::new() }; CAPACITY];
static COUNT: AtomicUsize = AtomicUsize::new(0);
static REGISTER: Mutex<()> = Mutex::new(());
static SCANNED: OnceLock<()> = OnceLock::new();
static CACHE: OnceLock<PathBuf> = OnceLock::new();
static EXTRA: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// This session's maps by key (`bind_session`).
static SESSION: Mutex<Vec<(String, MapId)>> = Mutex::new(Vec::new());

/// The entry for `id` (every `MapId` has one).
pub fn entry(id: MapId) -> &'static MapEntry {
    scan();
    SLOTS[id.index()].get().copied().expect("every MapId is registered")
}

/// The map a key means: this session's binding if there is one, else the
/// installed map found first (downloads answer only through a session).
pub fn find(key: &str) -> Option<MapId> {
    scan();
    if let Some((_, id)) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        return Some(*id);
    }
    let known = (0..COUNT.load(Ordering::Acquire)).filter_map(|i| SLOTS[i].get().copied())
        .filter(|e| !e.cached && e.key.eq_ignore_ascii_case(key));
    let mut first = None;
    for e in known {
        if e.installed() { return Some(e.id); }
        first = first.or(Some(e.id));
    }
    first
}

/// Installed maps (not Valley), in menu order: the well-known keys first,
/// then the rest by content root and key.
pub fn maps() -> Vec<MapId> {
    scan();
    (1..COUNT.load(Ordering::Acquire)).filter_map(|i| SLOTS[i].get().copied())
        .filter(|e| e.installed() && !e.cached).map(|e| e.id).collect()
}

/// A known map with this key and fingerprint, installed or downloaded.
pub fn by_fingerprint(key: &str, fingerprint: &str) -> Option<MapId> {
    scan();
    (0..COUNT.load(Ordering::Acquire)).filter_map(|i| SLOTS[i].get().copied())
        // A download trimmed from the cache since it was registered is gone,
        // as is one that failed to load (a newer entry for it may be fine).
        .filter(|e| !e.cached || (e.folder().is_some_and(|d| d.join("map.json").is_file()) && !matches!(e.pack.get(), Some(Err(_)))))
        .find(|e| e.key == key && e.fingerprint() == Some(fingerprint)).map(|e| e.id)
}

/// The SHA-256 of a content list: each map's key and fingerprint, in order.
pub fn content_digest<'a>(items: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for (key, fingerprint) in items { hash.update(key.as_bytes()); hash.update(b"="); hash.update(fingerprint.as_bytes()); hash.update(b"\n"); }
    format!("{:x}", hash.finalize())
}

/// Bind a server's maps for this session: each key to the map with exactly
/// that fingerprint; or the positions of the maps this game doesn't hold.
pub fn bind_session(items: &[(&str, &str)]) -> Result<(), Vec<usize>> {
    let mut bound = Vec::new();
    let mut missing = Vec::new();
    for (i, (key, fingerprint)) in items.iter().enumerate() {
        match by_fingerprint(key, fingerprint) {
            Some(id) => bound.push((key.to_string(), id)),
            None => missing.push(i),
        }
    }
    if !missing.is_empty() { return Err(missing); }
    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = bound;
    Ok(())
}

/// Forget the session's bindings: keys mean the installed maps again.
pub fn end_session() { SESSION.lock().unwrap_or_else(|e| e.into_inner()).clear(); }

/// Where a game client keeps downloaded maps. Set once, before the catalog
/// is first used; servers never download.
pub fn set_cache_dir(dir: PathBuf) { let _ = CACHE.set(dir); }

/// The download cache, if this program has one.
pub fn cache_dir() -> Option<&'static Path> { CACHE.get().map(PathBuf::as_path) }

/// Register a verified download (`<cache>/maps/<key>/<fingerprint>/`).
pub fn add_cached(key: &str, dir: PathBuf) -> Result<MapId, String> {
    if !valid_key(key) { return Err(format!("{key:?} isn't a map key")); }
    scan();
    try_register(key, folder_meta(key, &dir), Source::Folder(dir), true)
}

/// Where the engine looks for content, most preferred first: folders holding
/// `maps/` or `mods/`.
pub fn roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(list) = std::env::var_os("PEAKRUNNER_PACK_DIR") {
            roots.extend(std::env::split_paths(&list).filter(|p| !p.as_os_str().is_empty()));
        }
        roots.extend(EXTRA.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned());
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            roots.push(dir.join("packs"));
            roots.push(dir.join("../Resources/packs"));
        }
        roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    }
    let mut seen = Vec::new();
    roots.retain(|r| (r.join("maps").is_dir() || r.join("mods").is_dir()) && {
        let canonical = r.canonicalize().unwrap_or_else(|_| r.clone());
        let fresh = !seen.contains(&canonical);
        seen.push(canonical);
        fresh
    });
    roots
}

/// Look for content in `dir` too (its `maps/` and `mods/`), after
/// `PEAKRUNNER_PACK_DIR` and before the installed `packs/`: a server's
/// `content/` beside its `server.json`. Call it before anything asks the
/// catalog or the mods about content; later calls come too late to count.
pub fn add_root(dir: PathBuf) {
    if SCANNED.get().is_some() { eprintln!("PeakRunner: content folder {} was added too late to be read", dir.display()); return; }
    EXTRA.lock().unwrap_or_else(|e| e.into_inner()).push(dir);
}

/// A map key: 1 to 32 lowercase letters, digits and inner hyphens.
pub fn valid_key(key: &str) -> bool {
    (1..=32).contains(&key.len()) && !key.starts_with('-') && !key.ends_with('-')
        && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Info {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    bases: Option<[[f32; 3]; 2]>,
    #[serde(default)]
    dust: Option<[f32; 3]>,
}

/// Blast dust on a map whose `info.json` doesn't say.
pub const DEFAULT_DUST: [f32; 3] = [0.42, 0.38, 0.30];

/// Menu text a pack may carry: printable, no control or bidi characters.
fn plain(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(|c| c.is_control()
        || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
}

/// A map folder's `info.json`, or the defaults with a reason when it's unusable.
fn read_info(dir: &Path) -> Result<Info, String> {
    let path = dir.join("info.json");
    let Ok(meta) = path.metadata() else { return Ok(Info::default()) };
    if meta.len() > INFO_MAX { return Err("info.json is larger than 16 KiB".into()); }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    parse_info(&bytes)
}

fn parse_info(bytes: &[u8]) -> Result<Info, String> {
    let info: Info = serde_json::from_slice(bytes).map_err(|e| format!("info.json: {e}"))?;
    if info.name.as_deref().is_some_and(|n| n.trim().is_empty() || !plain(n, 48)) {
        return Err("info.json: name must be 1 to 48 plain characters".into());
    }
    if info.note.as_deref().is_some_and(|n| !plain(n, 400)) {
        return Err("info.json: note must be at most 400 plain characters".into());
    }
    if info.bases.iter().flatten().flatten().any(|v| !v.is_finite() || v.abs() > 10000.0) {
        return Err("info.json: bases must be finite positions".into());
    }
    if info.dust.iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
        return Err("info.json: dust must be three numbers from 0 to 1".into());
    }
    Ok(info)
}

/// The `name` in a `map.json`, without validating the rest.
fn manifest_name(dir: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct Named { #[serde(default)] name: String }
    let path = dir.join("map.json");
    if path.metadata().ok()?.len() > MANIFEST_MAX { return None; }
    let named: Named = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let name = named.name.trim();
    (!name.is_empty() && plain(name, 48)).then(|| name.to_string())
}

fn leak(s: String) -> &'static str { Box::leak(s.into_boxed_str()) }

struct Meta { name: String, note: String, dust: [f32; 3], bases: Option<[Vec3; 2]> }

impl Meta {
    fn from_info(info: Info, fallback_name: impl FnOnce() -> String) -> Self {
        Meta { name: info.name.unwrap_or_else(fallback_name), note: info.note.unwrap_or_default(),
            dust: info.dust.unwrap_or(DEFAULT_DUST), bases: info.bases.map(|[a, b]| [Vec3::from_array(a), Vec3::from_array(b)]) }
    }
}

fn register(key: &str, meta: Meta, source: Source, cached: bool) -> MapId {
    try_register(key, meta, source, cached).unwrap_or_else(|e| panic!("{e}"))
}

/// `register`, refused past the catalog's capacity. A download fetched
/// again into its folder (trimmed from the cache meanwhile) is the entry
/// it was, unless that entry read the folder wrong while it was gone or
/// changed (a fingerprint other than the folder's name, or a failed pack).
fn try_register(key: &str, meta: Meta, source: Source, cached: bool) -> Result<MapId, String> {
    let Meta { name, note, dust, bases } = meta;
    let _held = REGISTER.lock().unwrap_or_else(|e| e.into_inner());
    let index = COUNT.load(Ordering::Acquire);
    if let (true, Source::Folder(dir)) = (cached, &source) {
        let named = dir.file_name().and_then(|n| n.to_str());
        let again = (0..index).filter_map(|i| SLOTS[i].get().copied()).find(|e| e.cached && e.key == key && e.folder() == Some(dir.as_path())
            && e.fingerprint.get().is_none_or(|f| f.as_deref() == named) && !matches!(e.pack.get(), Some(Err(_))));
        if let Some(e) = again { return Ok(e.id); }
    }
    if index >= CAPACITY { return Err(format!("more than {CAPACITY} maps")); }
    let id = MapId::from_index(index);
    let entry: &'static MapEntry = Box::leak(Box::new(MapEntry {
        id, key: leak(key.into()), name: leak(name), note: leak(note), dust, bases, source, cached,
        fingerprint: OnceLock::new(), pack: OnceLock::new(), nav: OnceLock::new(), nav_started: AtomicBool::new(false), overview: OnceLock::new(),
    }));
    let _ = SLOTS[index].set(entry);
    COUNT.store(index + 1, Ordering::Release);
    Ok(id)
}

/// A map folder's menu text and look, from its `info.json` (ignored with a
/// warning when unusable).
fn folder_meta(key: &str, dir: &Path) -> Meta {
    let info = read_info(dir).unwrap_or_else(|e| {
        eprintln!("PeakRunner: {}: {e}; using defaults", dir.display());
        Info::default()
    });
    Meta::from_info(info, || manifest_name(dir).unwrap_or_else(|| key.to_string()))
}

/// The map folders in one content root, by key.
fn folders(root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read) = std::fs::read_dir(root.join("maps")) else { return Vec::new() };
    let mut found: Vec<(String, PathBuf)> = read.filter_map(Result::ok).filter_map(|e| {
        let key = e.file_name().to_str()?.to_string();
        let dir = e.path();
        if !dir.join("map.json").is_file() { return None; }
        if !valid_key(&key) {
            eprintln!("PeakRunner: skipped map folder {}: its name isn't a map key (a-z, 0-9, -)", dir.display());
            return None;
        }
        Some((key, dir))
    }).collect();
    found.sort();
    found
}

/// Downloads: `<maps>/<key>/<fingerprint>/` folders, by key.
fn folders_by_key(maps: &Path) -> Vec<(String, Vec<PathBuf>)> {
    let Ok(read) = std::fs::read_dir(maps) else { return Vec::new() };
    let mut out: Vec<(String, Vec<PathBuf>)> = read.filter_map(Result::ok).filter_map(|e| {
        let key = e.file_name().to_str()?.to_string();
        if !valid_key(&key) { return None; }
        let mut versions: Vec<PathBuf> = std::fs::read_dir(e.path()).ok()?.filter_map(Result::ok)
            .map(|v| v.path()).filter(|p| p.join("map.json").is_file()).collect();
        versions.sort();
        Some((key, versions))
    }).collect();
    out.sort();
    out
}

fn scan() {
    SCANNED.get_or_init(|| {
        let roots = roots();
        let mut located: Vec<(String, PathBuf)> = Vec::new();
        for root in &roots {
            for (key, dir) in folders(root) {
                if !located.iter().any(|(k, _)| *k == key) { located.push((key, dir)); }
            }
        }
        // The older launcher's single map folder has no info.json: it keeps
        // the installed Old Holler's menu text.
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut legacy: Option<PathBuf> = None;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = std::env::var_os("PEAKRUNNER_MAP_PACK") { legacy = Some(PathBuf::from(dir)); }
        for key in WELL_KNOWN {
            if key == "old-holler" {
                if let Some(dir) = legacy.take() {
                    let mut meta = folder_meta(key, &dir);
                    if !dir.join("info.json").is_file() {
                        let normal = located.iter().find(|(k, _)| k == key).map(|(_, d)| d.clone());
                        if let Some(dir) = normal { meta = folder_meta(key, &dir); }
                    }
                    register(key, meta, Source::Folder(dir), false);
                    continue;
                }
            }
            if key == "valley" {
                register(key, Meta { name: "Valley".into(), note: "256 m. The small rift.".into(), dust: [0.86, 0.88, 0.91],
                    bases: Some([crate::terrain::EMBER_HOME, crate::terrain::GLACIER_HOME]) }, Source::None, false);
            } else if let Some((_, dir)) = located.iter().find(|(k, _)| k == key) {
                register(key, folder_meta(key, dir), Source::Folder(dir.clone()), false);
            } else if let Some((assets, info)) = crate::map_pack::embedded(key) {
                let meta = Meta::from_info(parse_info(info.as_bytes()).unwrap_or_default(), || key.into());
                register(key, meta, Source::Embedded(assets), false);
            } else {
                register(key, Meta::from_info(Info::default(), || key.into()), Source::None, false);
            }
        }
        for (key, dir) in located {
            if WELL_KNOWN.contains(&key.as_str()) { continue; }
            register(&key, folder_meta(&key, &dir), Source::Folder(dir), false);
        }
        if let Some(cache) = CACHE.get() {
            for (key, versions) in folders_by_key(&cache.join("maps")) {
                for dir in versions {
                    if let Err(e) = try_register(&key, folder_meta(&key, &dir), Source::Folder(dir.clone()), true) {
                        eprintln!("PeakRunner: left out the download {}: {e}", dir.display());
                    }
                }
            }
        }
    });
}

/// Make a map folder known at runtime (a download, or a folder a person
/// picked), under its key. Returns its id; the pack loads on first use.
pub fn add_folder(key: &str, dir: PathBuf) -> Result<MapId, String> {
    if !valid_key(key) { return Err(format!("{key:?} isn't a map key (a-z, 0-9, -)")); }
    if !dir.join("map.json").is_file() { return Err(format!("{} has no map.json", dir.display())); }
    scan();
    Ok(register(key, folder_meta(key, &dir), Source::Folder(dir), false))
}

/// One map folder, checked by `check` (`peakrunner-server --check`).
pub struct Checked {
    /// The folder, relative to what was checked.
    pub folder: String,
    /// Menu name, fingerprint and the modes it can host; or why it can't be used.
    pub result: Result<(String, String, Vec<crate::map_catalog::SupportedMode>), String>,
}

/// Check content the way the engine will load it: a content folder (holding
/// `maps/`) or a single map folder (holding `map.json`). Every map folder is
/// validated in full; nothing is registered.
pub fn check(path: &Path) -> Vec<Checked> {
    use crate::map_catalog::SupportedMode;
    let folders: Vec<PathBuf> = if path.join("map.json").is_file() { vec![path.to_path_buf()] } else {
        let Ok(read) = std::fs::read_dir(path.join("maps")) else {
            return vec![Checked { folder: path.display().to_string(),
                result: Err("neither a map folder (map.json) nor a content folder (maps/)".into()) }];
        };
        let mut all: Vec<PathBuf> = read.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        all.sort();
        all
    };
    folders.into_iter().map(|dir| {
        let key = dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let folder = if path.join("map.json").is_file() { key.clone() } else { format!("maps/{key}") };
        let result = (|| {
            if !valid_key(&key) { return Err("the folder name is the map key: 1 to 32 of a-z, 0-9 and inner hyphens".to_string()); }
            if !dir.join("map.json").is_file() { return Err("no map.json".into()); }
            let info = read_info(&dir)?;
            let pack = MapPack::load(&dir)?;
            crate::sim::features::check(&pack.manifest)?;
            let modes = SupportedMode::ALL.into_iter().filter(|m| m.offered() && crate::map_catalog::pack_supports(Some(&pack), *m)).collect();
            let name = info.name.or_else(|| manifest_name(&dir)).unwrap_or_else(|| key.clone());
            Ok((name, pack.fingerprint, modes))
        })();
        Checked { folder, result }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A download fetched again into its folder (trimmed from the cache
    /// meanwhile) is the entry it was: each fetch took another of the
    /// catalog's slots, and running out of them panicked.
    #[test]
    fn a_download_fetched_again_is_the_entry_it_was() {
        let root = std::env::temp_dir().join(format!("peakrunner-again-{}", std::process::id()));
        let fp = entry(MapId::Highgoal).fingerprint().unwrap().to_string();
        let copy = |dir: &Path| {
            std::fs::create_dir_all(dir).unwrap();
            for f in std::fs::read_dir(entry(MapId::Highgoal).folder().unwrap()).unwrap().filter_map(Result::ok) {
                std::fs::copy(f.path(), dir.join(f.file_name())).unwrap();
            }
        };
        // As the cache holds it: <key>/<fingerprint>.
        let dir = root.join("again-arena").join(&fp);
        copy(&dir);
        let first = add_cached("again-arena", dir.clone()).unwrap();
        assert!(add_cached("again-arena", dir.clone()) == Ok(first), "no new slot");
        // Played, trimmed from the cache, and fetched again.
        assert!(by_fingerprint("again-arena", &fp) == Some(first));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(by_fingerprint("again-arena", &fp).is_none(), "gone while trimmed");
        copy(&dir);
        assert!(add_cached("again-arena", dir.clone()) == Ok(first), "fetched again after a trim");
        assert!(by_fingerprint("again-arena", &fp) == Some(first));
        // One that found its folder gone isn't taken again.
        let gone = root.join("gone-arena").join(&fp);
        copy(&gone);
        let third = add_cached("gone-arena", gone.clone()).unwrap();
        std::fs::remove_dir_all(&gone).unwrap();
        assert!(entry(third).fingerprint().is_none());
        copy(&gone);
        let fourth = add_cached("gone-arena", gone.clone()).unwrap();
        assert!(fourth != third && entry(fourth).fingerprint() == Some(fp.as_str()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_default_map_is_installed_in_menu_order_with_its_name() {
        let names: Vec<&str> = maps().into_iter().take(10).map(|id| entry(id).name).collect();
        assert_eq!(names, ["Old Holler", "Tower Complex", "Cairnhold", "Frostline", "Dustreach", "Longfield",
            "Highgoal", "Ozarktic Blast", "Reefbreak", "Ashveil"]);
        for (i, key) in WELL_KNOWN.iter().enumerate() {
            assert_eq!(find(key).map(MapId::index), Some(i), "{key}");
            assert_eq!(find(&key.to_uppercase()).map(MapId::index), Some(i), "keys match in any case");
        }
        assert!(!maps().contains(&MapId::Valley), "Valley is a test fixture, not a menu map");
        assert!(entry(MapId::Valley).pack().is_none() && entry(MapId::Valley).problem().is_none());
        assert_eq!(find("no-such-map"), None);
    }

    #[test]
    fn map_keys_are_short_lowercase_words() {
        for good in ["old-holler", "a", "map2", "x-1-y"] { assert!(valid_key(good), "{good}"); }
        for bad in ["", "Old", "-a", "a-", "a b", "a/b", "..", "ä", &"a".repeat(33)] { assert!(!valid_key(bad), "{bad}"); }
    }

    #[test]
    fn info_files_are_checked_and_never_required() {
        let ok = parse_info(br#"{"name":"Somewhere","note":"A place.","bases":[[1,2,3],[4,5,6]]}"#).unwrap();
        assert_eq!((ok.name.as_deref(), ok.note.as_deref()), (Some("Somewhere"), Some("A place.")));
        for bad in [&br#"{"name":""}"#[..], br#"{"name":"a\nb"}"#, br#"{"name":"x","extra":1}"#,
            br#"{"bases":[[1e9,0,0],[0,0,0]]}"#, br#"{"name":"\u202etxt"}"#, b"not json"] {
            assert!(parse_info(bad).is_err(), "{}", String::from_utf8_lossy(bad));
        }
        assert!(read_info(Path::new("/nonexistent")).unwrap().name.is_none(), "no info.json: defaults");
    }

    /// A map folder added at runtime (as a download will be) gets its own id,
    /// loads, and a broken one is refused with a reason instead of a crash.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn added_folders_load_and_broken_ones_are_refused_with_a_reason() {
        let source = entry(MapId::Highgoal).pack().expect("Highgoal installed").root.clone();
        let tmp = std::env::temp_dir().join(format!("peakrunner-catalog-{}", std::process::id()));
        let good = tmp.join("maps/copied-arena");
        std::fs::create_dir_all(&good).unwrap();
        // Link the payloads only: info.json is written fresh below, and
        // writing through a link would change the installed map's.
        for f in std::fs::read_dir(&source).unwrap().filter_map(Result::ok).filter(|f| f.file_name() != "info.json") {
            let to = good.join(f.file_name());
            if std::fs::hard_link(f.path(), &to).is_err() { std::fs::copy(f.path(), &to).unwrap(); }
        }
        std::fs::write(good.join("info.json"), r#"{"name":"Copied Arena"}"#).unwrap();
        let id = add_folder("copied-arena", good.clone()).unwrap();
        assert!(id.index() >= WELL_KNOWN.len());
        assert_eq!((id.key(), entry(id).name), ("copied-arena", "Copied Arena"));
        assert_eq!(entry(id).pack().unwrap().fingerprint, entry(MapId::Highgoal).pack().unwrap().fingerprint);
        assert!(maps().contains(&id) && find("copied-arena") == Some(id));

        let broken = tmp.join("maps/broken-arena");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::copy(source.join("map.json"), broken.join("map.json")).unwrap();
        let bad = add_folder("broken-arena", broken).unwrap();
        assert!(entry(bad).pack().is_none());
        assert!(entry(bad).problem().is_some_and(|p| p.contains("vertices.bin")), "{:?}", entry(bad).problem());
        assert!(add_folder("Bad Key", good).is_err());
        std::fs::create_dir_all(tmp.join("maps/Bad Key")).unwrap();
        let report = check(&tmp);
        let find = |f: &str| report.iter().find(|c| c.folder == f).unwrap_or_else(|| panic!("{f} checked"));
        let (name, fingerprint, modes) = find("maps/copied-arena").result.as_ref().unwrap();
        assert_eq!((name.as_str(), fingerprint, modes.as_slice()),
            ("Copied Arena", &entry(MapId::Highgoal).pack().unwrap().fingerprint, &[crate::map_catalog::SupportedMode::Football][..]));
        assert!(find("maps/broken-arena").result.as_ref().is_err_and(|e| e.contains("vertices.bin")));
        assert!(find("maps/Bad Key").result.is_err());
        assert_eq!(check(&tmp.join("maps/copied-arena")).len(), 1, "a single map folder");
        assert!(check(&tmp.join("nowhere"))[0].result.is_err());
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
