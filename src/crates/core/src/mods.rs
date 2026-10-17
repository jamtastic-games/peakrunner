//! Mods (docs/modding.md, P4): folders that change definitions and add game
//! modes, as data only. A mod is `<content root>/mods/<id>/`:
//!
//! - `mod.json`: `{"name": "...", "note": "...", "api": 1}` (required);
//! - `defs.json`: only the world numbers it changes (movement, the repair
//!   tool, map equipment), laid over the base package's
//!   (`crate::defs::merge`), e.g. `{"movement": {"gravity": 12}}`. Older
//!   mods' item numbers here (`weapons.disc.reload`) still work: they
//!   patch the entries they meant (`registry::legacy`);
//! - class folders: `armors/`, `weapons/`, `projectiles/`,
//!   `blasts/`, `throwables/`, `deployables/`, `damage/`, `effects/`,
//!   `packs/`, `items/`, each holding `<name>.json` files of entries by id
//!   that patch, replace, extend or add to the base game's
//!   (`crate::registry`);
//! - `modes/<key>.json`: new modes, each a variant of an existing one:
//!   `{"base": "ctf", "label": "Quick CTF", "score_limit": 5}`;
//! - `rules/<key>.rhai`: rules scripts a mode names (`"script"`),
//!   run only by servers (P6, `crates/script`). They are never part of the
//!   listing, so they are never sent to players and never downloaded;
//! - `hooks/<name>.rhai`: scripts definitions name, kept on the
//!   server the same way.
//!
//! Every JSON file may hold `//` and `/* */` comments (`crate::jsonc`).
//!
//! Nothing else may be in the folder. A server runs the mods its config names
//! (`mods` in server.json, or `PEAKRUNNER_MODS`); players who lack them
//! download them like maps and play with exactly the server's definitions for
//! that session. A mod's identity is its fingerprint, the SHA-256 of its
//! listing (each file's SHA-256, size and path). A mod that fails a check is
//! refused with a reason, never played.
use crate::registry::Registry;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// The mod API level this engine implements (docs/modding.md, §2.2). It
/// stays 1 until the full release: playtests ship the engine, servers and
/// mods together, so nothing is gated by level yet.
pub const API: u32 = 1;
/// Most rules scripts a mod may hold.
const SCRIPTS_MAX: usize = 16;
/// Largest file a mod may hold, and most mode files.
const FILE_MAX: u64 = 64 * 1024;
const MODES_MAX: usize = 64;
/// Most files a mod's listing may hold.
const FILES_MAX: usize = 256;

/// A mod the engine knows: installed in a content root, or downloaded.
pub struct ModEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    folder: PathBuf,
    cached: bool,
    listing: OnceLock<Result<Listing, String>>,
}

/// Every file of a mod, as its fingerprint covers them.
pub struct Listing {
    /// `<sha256> <size> <path>` per file, sorted by path.
    pub text: String,
    pub fingerprint: String,
    pub files: Vec<(String, String, u64)>,
    pub bytes: u64,
    /// `rules/<key>.rhai` and `hooks/<name>.rhai`: the server's alone,
    /// outside the listing.
    pub scripts: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta { name: String, #[serde(default)] note: String, api: u32 }

/// A mod's contents, read and checked; `scripts` (rules) and `hooks` are
/// (name, source), `files` its class files (path, parsed).
pub struct Loaded {
    pub defs: Option<serde_json::Value>,
    pub modes: Vec<(String, serde_json::Value)>,
    pub scripts: Vec<(String, String)>,
    pub hooks: Vec<(String, String)>,
    pub files: Vec<(String, serde_json::Value)>,
}

impl Loaded {
    /// This mod as a registry source.
    pub fn source<'a>(&'a self, name: &'a str) -> crate::registry::Source<'a> {
        crate::registry::Source { name, defs: self.defs.as_ref(), files: self.files.clone() }
    }
    /// Whether it changes any definitions (world numbers or entries).
    pub fn changes_definitions(&self) -> bool { self.defs.is_some() || !self.files.is_empty() }
}

impl ModEntry {
    pub fn folder(&self) -> &Path { &self.folder }

    /// The mod's files and fingerprint (computed once).
    pub fn listing(&self) -> Result<&Listing, String> {
        self.listing.get_or_init(|| listing(&self.folder)).as_ref().map_err(Clone::clone)
    }

    pub fn fingerprint(&self) -> Option<&str> { self.listing().ok().map(|l| l.fingerprint.as_str()) }

    /// A file a server may send for this mod (`ClientMsg::Fetch`): its
    /// listing (`listing.txt`, generated) or a listed file. Nothing else.
    pub fn download(&self, name: &str) -> Option<Vec<u8>> {
        let l = self.listing().ok()?;
        if name == "listing.txt" { return Some(l.text.clone().into_bytes()); }
        l.files.iter().find(|(p, _, _)| p == name).and_then(|(p, _, _)| std::fs::read(self.folder.join(p)).ok())
    }

    pub fn load(&self) -> Result<Loaded, String> { load(&self.folder) }
}

/// A file path a mod may hold (and a player may download: `rules/` and
/// `hooks/` never appear in a listing, and aren't allowed here).
pub fn allowed(path: &str) -> bool {
    if path == "mod.json" || path == "defs.json" { return true; }
    if path.strip_prefix("modes/").and_then(|f| f.strip_suffix(".json")).is_some_and(crate::modes::valid_key) { return true; }
    if path.strip_prefix("sounds/").and_then(|f| f.strip_suffix(".wav")).is_some_and(crate::registry::fields::valid_id) { return true; }
    let Some((folder, file)) = path.split_once('/') else { return false };
    crate::registry::Class::from_folder(folder).is_some() && file.strip_suffix(".json").is_some_and(crate::registry::fields::valid_id)
}

/// The folders whose scripts stay on the server: `rules/` (a mode's rules)
/// and `hooks/` (scripts definitions name).
const SERVER_ONLY: [&str; 2] = ["rules", "hooks"];

fn listing(dir: &Path) -> Result<Listing, String> {
    let (mut paths, mut scripts) = (Vec::new(), Vec::new());
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') { continue; }
        if e.path().is_dir() && SERVER_ONLY.contains(&name.as_str()) {
            for m in std::fs::read_dir(e.path()).map_err(|e| e.to_string())?.filter_map(Result::ok) {
                let file = m.file_name().to_string_lossy().to_string();
                if file.starts_with('.') { continue; }
                if !file.strip_suffix(".rhai").is_some_and(crate::modes::valid_key) {
                    return Err(format!("{name}/{file}: scripts are {name}/<key>.rhai"));
                }
                scripts.push(format!("{name}/{file}"));
            }
        } else if e.path().is_dir() && (name == "modes" || name == "sounds" || crate::registry::Class::from_folder(&name).is_some()) {
            for m in std::fs::read_dir(e.path()).map_err(|e| e.to_string())?.filter_map(Result::ok) {
                let file = m.file_name().to_string_lossy().to_string();
                if file.starts_with('.') { continue; }
                paths.push(format!("{name}/{file}"));
            }
        } else { paths.push(name); }
    }
    paths.sort();
    if let Some(bad) = paths.iter().find(|p| !allowed(p)) {
        return Err(format!("{bad}: a mod holds only mod.json, defs.json, modes/<key>.json, class folders (armors/, weapons/, projectiles/, blasts/, throwables/, deployables/, damage/, effects/, packs/, items/, vehicles/) of <name>.json, sounds/<name>.wav, rules/<key>.rhai and hooks/<name>.rhai"));
    }
    if scripts.len() > SCRIPTS_MAX { return Err(format!("more than {SCRIPTS_MAX} rules scripts")); }
    scripts.sort();
    if !paths.iter().any(|p| p == "mod.json") { return Err("no mod.json".into()); }
    if paths.iter().filter(|p| p.starts_with("modes/")).count() > MODES_MAX { return Err(format!("more than {MODES_MAX} modes")); }
    if paths.len() > FILES_MAX { return Err(format!("more than {FILES_MAX} files")); }
    let mut text = String::new();
    let mut files = Vec::new();
    let mut bytes = 0;
    for p in paths {
        let data = read(dir, &p)?;
        let sha = format!("{:x}", Sha256::digest(&data));
        text.push_str(&format!("{sha} {} {p}\n", data.len()));
        bytes += data.len() as u64;
        files.push((p, sha, data.len() as u64));
    }
    let fingerprint = format!("{:x}", Sha256::digest(text.as_bytes()));
    Ok(Listing { text, fingerprint, files, bytes, scripts })
}

fn read(dir: &Path, path: &str) -> Result<Vec<u8>, String> {
    let file = dir.join(path);
    let meta = file.metadata().map_err(|e| format!("{path}: {e}"))?;
    if meta.len() > FILE_MAX { return Err(format!("{path} is larger than 64 KiB")); }
    std::fs::read(file).map_err(|e| format!("{path}: {e}"))
}

fn json(dir: &Path, path: &str) -> Result<serde_json::Value, String> {
    let bytes = read(dir, path)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| format!("{path} isn't UTF-8 text"))?;
    crate::jsonc::parse(text).map_err(|e| format!("{path}: {e}"))
}

/// The fingerprint of a mod folder as the game reads it: its listing's
/// SHA-256, the listing made from the files themselves.
pub fn fingerprint_of(dir: &Path) -> Result<String, String> { listing(dir).map(|l| l.fingerprint) }

/// Read and check a mod folder (not activating it).
pub fn load(dir: &Path) -> Result<Loaded, String> {
    let l = listing(dir)?;
    let meta: Meta = serde_json::from_value(json(dir, "mod.json")?).map_err(|e| format!("mod.json: {e}"))?;
    check_meta(&meta)?;
    let defs = l.files.iter().any(|(p, _, _)| p == "defs.json").then(|| json(dir, "defs.json")).transpose()?;
    if defs.as_ref().is_some_and(|d| !d.is_object()) { return Err("defs.json must be an object".into()); }
    let mut modes = Vec::new();
    for (p, _, _) in l.files.iter().filter(|(p, _, _)| p.starts_with("modes/")) {
        let key = p.trim_start_matches("modes/").trim_end_matches(".json").to_string();
        let value = json(dir, p)?;
        if !value.get("base").is_some_and(|b| b.is_string()) { return Err(format!("{p}: needs \"base\": the mode it varies")); }
        modes.push((key, value));
    }
    let (mut scripts, mut hooks) = (Vec::new(), Vec::new());
    for p in &l.scripts {
        let source = String::from_utf8(read(dir, p)?).map_err(|_| format!("{p} isn't UTF-8 text"))?;
        let (folder, file) = p.split_once('/').unwrap_or_default();
        let name = file.trim_end_matches(".rhai").to_string();
        if folder == "hooks" { hooks.push((name, source)); } else { scripts.push((name, source)); }
    }
    let mut files = Vec::new();
    for (p, _, _) in l.files.iter().filter(|(p, _, _)| p.split_once('/').is_some_and(|(f, _)| crate::registry::Class::from_folder(f).is_some())) {
        let value = json(dir, p)?;
        if !value.is_object() { return Err(format!("{p}: must be an object of entries by id")); }
        files.push((p.clone(), value));
    }
    // Sounds: small WAVE files.
    for p in listing(dir)?.files.iter().map(|f| &f.0).filter(|p| p.starts_with("sounds/")) {
        if !sound_ok(&read(dir, p)?) { return Err(format!("{p}: a sound is a WAVE file of at most {} KiB", SOUND_MAX / 1024)); }
    }
    Ok(Loaded { defs, modes, scripts, hooks, files })
}

impl Loaded {
    /// A mode naming a script this mod doesn't hold. Only a mod a server
    /// runs needs its scripts: a player's downloaded copy never has them.
    pub fn missing_script(&self) -> Option<String> {
        self.modes.iter().find_map(|(key, value)| {
            let name = value.get("script").and_then(|s| s.as_str())?;
            (!self.scripts.iter().any(|(n, _)| n == name)).then(|| format!("modes/{key}.json: no rules/{name}.rhai for its script"))
        })
    }
}

fn check_meta(meta: &Meta) -> Result<(), String> {
    let plain = |t: &str, max: usize| t.chars().count() <= max && !t.chars().any(char::is_control);
    if meta.name.trim().is_empty() || !plain(&meta.name, 48) || !plain(&meta.note, 400) {
        return Err("mod.json: name 1-48 and note up to 400 plain characters".into());
    }
    if meta.api == 0 || meta.api > API { return Err(format!("mod.json: needs engine API {}, this engine has {API}", meta.api)); }
    Ok(())
}

/// The registry the base package and `mods` make, in order, checked:
/// every field known, every number in range, every reference resolved.
pub fn effective(mods: &[(&str, &Loaded)]) -> Result<Registry, String> {
    let sources: Vec<crate::registry::Source> = mods.iter().map(|(id, l)| l.source(id)).collect();
    crate::registry::build(&sources)
}

/// A mod's modes in an order where each follows the mode it varies (a base
/// mode, one in `known`, or another of these).
fn in_order<'a>(modes: &'a [(String, serde_json::Value)], known: &[String]) -> Result<Vec<&'a (String, serde_json::Value)>, String> {
    let mut done: Vec<String> = crate::modes::BASE.iter().map(|(k, _)| k.to_string()).chain(known.iter().cloned()).collect();
    let mut pending: Vec<&(String, serde_json::Value)> = modes.iter().collect();
    let mut out = Vec::new();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|m| {
            if done.iter().any(|d| d == m.1["base"].as_str().unwrap_or_default()) { out.push(*m); done.push(m.0.clone()); false } else { true }
        });
        if pending.len() == before {
            return Err(format!("mode {}: no mode {} to vary", pending[0].0, pending[0].1["base"].as_str().unwrap_or_default()));
        }
    }
    Ok(out)
}

/// Check mod folders the way the engine loads them: a content folder's
/// `mods/<id>/`, or one mod folder. Each: what it changes, or why it can't
/// be used (definitions, modes and all, as activating it would find).
pub fn check(path: &Path) -> Vec<(String, Result<String, String>)> {
    let dirs: Vec<PathBuf> = if path.join("mod.json").is_file() { vec![path.to_path_buf()] } else {
        let mut d: Vec<PathBuf> = std::fs::read_dir(path.join("mods")).into_iter().flatten().filter_map(Result::ok)
            .map(|e| e.path()).filter(|p| p.is_dir()).collect();
        d.sort();
        d
    };
    dirs.into_iter().map(|dir| {
        let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let folder = if path.join("mod.json").is_file() { id.clone() } else { format!("mods/{id}") };
        let result = (|| {
            if !crate::catalog::valid_key(&id) { return Err("the folder name is the mod's id: 1 to 32 of a-z, 0-9 and inner hyphens".to_string()); }
            let l = load(&dir)?;
            if let Some(missing) = l.missing_script() { return Err(missing); }
            let own = if l.changes_definitions() { Some(crate::registry::build(&[l.source(&id)])?) } else { None };
            let registry = own.as_ref().unwrap_or_else(|| crate::registry::base());
            let changes = if own.is_some() { crate::registry::changes(registry) } else { Vec::new() };
            let mut modes = Vec::new();
            let mut built: Vec<(String, crate::modes::ModeDef)> = Vec::new();
            for (key, value) in in_order(&l.modes, &[])? {
                if crate::modes::BASE.iter().any(|(k, _)| k == key) { return Err(format!("mode {key} is a base mode's name")); }
                let base_key = value["base"].as_str().unwrap_or_default();
                let base = match built.iter().find(|(k, _)| k == base_key) {
                    Some((_, d)) => d.clone(),
                    None => crate::modes::BASE.iter().position(|(k, _)| *k == base_key).map(crate::map_catalog::SupportedMode::from_index)
                        .ok_or_else(|| format!("mode {key}: no mode {base_key} to vary"))?.def().clone(),
                };
                let def = crate::modes::variant(&base, value).map_err(|e| format!("mode {key}: {e}"))?;
                if let Some(why) = registry.unknown_in(&def) { return Err(format!("mode {key}: {why}")); }
                built.push((key.clone(), def));
                modes.push(key.clone());
            }
            let fp = listing(&dir)?.fingerprint;
            let scripts: Vec<&str> = l.scripts.iter().map(|(n, _)| n.as_str()).collect();
            Ok(format!("{}{}{}, fingerprint {}", if changes.is_empty() { "no definition changes".to_string() } else { changes.join(", ") },
                if modes.is_empty() { String::new() } else { format!(", modes {}", modes.join(", ")) },
                if scripts.is_empty() { String::new() } else { format!(", rules scripts {} (server only)", scripts.join(", ")) }, &fp[..16]))
        })();
        (folder, result)
    }).collect()
}

// ---- The mods the engine knows ----

const CAPACITY: usize = 256;
static SLOTS: [OnceLock<&'static ModEntry>; CAPACITY] = [const { OnceLock::new() }; CAPACITY];
static COUNT: AtomicUsize = AtomicUsize::new(0);
static REGISTER: Mutex<()> = Mutex::new(());
static SCANNED: OnceLock<()> = OnceLock::new();

fn register(id: &str, dir: PathBuf, cached: bool) -> Result<&'static ModEntry, String> {
    let meta: Meta = serde_json::from_value(json(&dir, "mod.json")?).map_err(|e| format!("mod.json: {e}"))?;
    check_meta(&meta)?;
    let _held = REGISTER.lock().unwrap_or_else(|e| e.into_inner());
    let i = COUNT.load(Ordering::Acquire);
    // A download fetched again into its folder is the entry it was, unless
    // that entry read the folder wrong while it was gone or changed (a
    // fingerprint other than the folder's name).
    if cached {
        let named = dir.file_name().and_then(|n| n.to_str());
        let again = (0..i).filter_map(|n| SLOTS[n].get().copied()).find(|m| m.cached && m.id == id && m.folder == dir
            && m.listing.get().is_none_or(|l| l.as_ref().is_ok_and(|l| Some(l.fingerprint.as_str()) == named)));
        if let Some(m) = again { return Ok(m); }
    }
    if i >= CAPACITY { return Err("too many mods".into()); }
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let entry: &'static ModEntry = Box::leak(Box::new(ModEntry { id: leak(id.into()), name: leak(meta.name), note: leak(meta.note),
        folder: dir, cached, listing: OnceLock::new() }));
    let _ = SLOTS[i].set(entry);
    COUNT.store(i + 1, Ordering::Release);
    Ok(entry)
}

fn all() -> impl Iterator<Item = &'static ModEntry> {
    scan();
    (0..COUNT.load(Ordering::Acquire)).filter_map(|i| SLOTS[i].get().copied())
}

fn scan() {
    SCANNED.get_or_init(|| {
        let mut seen: Vec<String> = Vec::new();
        for root in crate::catalog::roots() {
            let Ok(read) = std::fs::read_dir(root.join("mods")) else { continue };
            let mut dirs: Vec<PathBuf> = read.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.join("mod.json").is_file()).collect();
            dirs.sort();
            for dir in dirs {
                let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                // The base package is built in; its folder is its source.
                if id == "base" || seen.contains(&id) { continue; }
                if !crate::catalog::valid_key(&id) { eprintln!("PeakRunner: skipped mod folder {}: not a key (a-z, 0-9, -)", dir.display()); continue; }
                match register(&id, dir.clone(), false) {
                    Ok(_) => seen.push(id),
                    Err(e) => eprintln!("PeakRunner: skipped mod {}: {e}", dir.display()),
                }
            }
        }
        if let Some(cache) = crate::catalog::cache_dir() {
            for id_dir in std::fs::read_dir(cache.join("mods")).into_iter().flatten().filter_map(Result::ok) {
                let id = id_dir.file_name().to_string_lossy().to_string();
                if !crate::catalog::valid_key(&id) { continue; }
                for v in std::fs::read_dir(id_dir.path()).into_iter().flatten().filter_map(Result::ok) {
                    if v.path().join("mod.json").is_file() { let _ = register(&id, v.path(), true); }
                }
            }
        }
    });
}

/// An installed mod by id (downloads answer only through a session).
pub fn find(id: &str) -> Option<&'static ModEntry> { all().find(|m| !m.cached && m.id == id) }

/// A known mod with this id and fingerprint, installed or downloaded.
pub fn by_fingerprint(id: &str, fingerprint: &str) -> Option<&'static ModEntry> {
    all().find(|m| m.id == id && (!m.cached || m.folder.join("mod.json").is_file()) && m.fingerprint() == Some(fingerprint))
}

/// Every installed mod.
pub fn installed() -> Vec<&'static ModEntry> { all().filter(|m| !m.cached).collect() }

/// Register a verified download (`<cache>/mods/<id>/<fingerprint>/`).
pub fn add_cached(id: &str, dir: PathBuf) -> Result<&'static ModEntry, String> {
    if !crate::catalog::valid_key(id) { return Err(format!("{id:?} isn't a mod key")); }
    scan();
    register(id, dir, true)
}

// ---- The mods in force ----

static CURRENT: AtomicPtr<Registry> = AtomicPtr::new(std::ptr::null_mut());
static ACTIVE: Mutex<Vec<&'static ModEntry>> = Mutex::new(Vec::new());
/// Mods this program runs outside a session (a server's own, or a player's
/// `PEAKRUNNER_MODS` offline): what `end_session` returns to.
static HOME: Mutex<Vec<&'static ModEntry>> = Mutex::new(Vec::new());
/// Modes already registered, by the mod fingerprint and key they came from.
static REGISTERED: Mutex<Vec<(String, String, u16)>> = Mutex::new(Vec::new());

/// The registry in force: the base package's with the active mods'.
pub(crate) fn current() -> Option<&'static Registry> {
    let p = CURRENT.load(Ordering::Acquire);
    // SAFETY: only `set` stores here, always a pointer from `Box::leak`,
    // which is never freed; a stale pointer stays valid for the process.
    (!p.is_null()).then(|| unsafe { &*p })
}

fn set(registry: Option<Registry>) {
    let p = registry.map_or(std::ptr::null_mut(), |r| Box::leak(Box::new(r)) as *mut Registry);
    CURRENT.store(p, Ordering::Release);
}

/// The mods in force.
pub fn active() -> Vec<&'static ModEntry> { ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).clone() }

/// Put `mods` in force, in order: their definitions over the base package's
/// and their modes registered. All or nothing: on an error nothing changes.
pub fn activate(mods: &[&'static ModEntry]) -> Result<(), String> {
    let loaded: Vec<(&ModEntry, Loaded)> = mods.iter().map(|m| m.load().map(|l| (*m, l)).map_err(|e| format!("mod {}: {e}", m.id)))
        .collect::<Result<_, _>>()?;
    let changing: Vec<(&str, &Loaded)> = loaded.iter().filter(|(_, l)| l.changes_definitions()).map(|(m, l)| (m.id, l)).collect();
    let registry = if changing.is_empty() { None } else { Some(effective(&changing)?) };
    let mut bindings: Vec<(String, u16)> = Vec::new();
    for (m, l) in &loaded {
        let fp = m.fingerprint().unwrap_or_default().to_string();
        let known: Vec<String> = bindings.iter().map(|(k, _)| k.clone()).collect();
        for (key, value) in in_order(&l.modes, &known).map_err(|e| format!("mod {}: {e}", m.id))? {
            if crate::modes::BASE.iter().any(|(k, _)| k == key) || bindings.iter().any(|(k, _)| k == key) {
                return Err(format!("mod {}: mode {key} is already a mode's name", m.id));
            }
            let base_key = value["base"].as_str().unwrap_or_default();
            let base = bindings.iter().find(|(k, _)| k == base_key).map(|(_, i)| crate::map_catalog::SupportedMode::from_index(*i as usize))
                .or_else(|| crate::modes::BASE.iter().position(|(k, _)| *k == base_key).map(crate::map_catalog::SupportedMode::from_index))
                .ok_or_else(|| format!("mod {}: mode {key}: no mode {base_key} to vary", m.id))?;
            let def = crate::modes::variant(base.def(), value).map_err(|e| format!("mod {}: mode {key}: {e}", m.id))?;
            if let Some(why) = registry.as_ref().unwrap_or_else(|| crate::registry::base()).unknown_in(&def) {
                return Err(format!("mod {}: mode {key}: {why}", m.id));
            }
            let mut registered = REGISTERED.lock().unwrap_or_else(|e| e.into_inner());
            let id = match registered.iter().find(|(f, k, _)| *f == fp && k == key) {
                Some((_, _, i)) => *i,
                None => {
                    let i = crate::modes::add(key, def)? as u16;
                    registered.push((fp.clone(), key.clone(), i));
                    i
                }
            };
            bindings.push((key.clone(), id));
        }
    }
    set(registry);
    crate::modes::bind_session(bindings);
    *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = mods.to_vec();
    Ok(())
}

/// The mods this program runs outside sessions, by id (a server's config,
/// or `PEAKRUNNER_MODS` offline). Installed mods only.
pub fn activate_home(ids: &[String]) -> Result<(), String> {
    let mods: Vec<&'static ModEntry> = ids.iter().map(|id| find(id).ok_or_else(|| format!("mod {id} is not installed")))
        .collect::<Result<_, _>>()?;
    activate(&mods)?;
    *HOME.lock().unwrap_or_else(|e| e.into_inner()) = mods;
    Ok(())
}

/// A session is over: back to the home mods (none: the base game).
pub fn end_session() {
    let home = HOME.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if activate(&home).is_err() { set(None); crate::modes::end_session(); ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).clear(); }
}

/// A rules script by name, from the installed mods in force: `(mod id,
/// source)`. Never from a download: scripts aren't in listings, and a
/// downloaded mod is skipped even if its folder somehow held one.
pub fn rules_script(name: &str) -> Option<(&'static str, String)> {
    active().into_iter().filter(|m| !m.cached).find_map(|m| {
        let path = m.folder.join("rules").join(format!("{name}.rhai"));
        path.is_file().then(|| read(&m.folder, &format!("rules/{name}.rhai")).ok()).flatten()
            .and_then(|b| String::from_utf8(b).ok()).map(|s| (m.id, s))
    })
}

/// Most bytes a mod's sound may take.
pub const SOUND_MAX: usize = 64 * 1024;

/// Whether `bytes` look like a sound a mod may ship: a small RIFF WAVE file.
pub fn sound_ok(bytes: &[u8]) -> bool {
    bytes.len() <= SOUND_MAX && bytes.len() > 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

/// The sounds of the active mods (`sounds/<name>.wav`), by name, the first
/// mod's where two share one. Players' games play them by name.
pub fn sounds() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for m in active() {
        let Ok(read_dir) = std::fs::read_dir(m.folder.join("sounds")) else { continue };
        let mut names: Vec<String> = read_dir.filter_map(Result::ok).filter_map(|e| e.file_name().to_str().map(String::from)).collect();
        names.sort();
        for file in names {
            let Some(name) = file.strip_suffix(".wav").filter(|n| crate::registry::fields::valid_id(n)) else { continue };
            if out.iter().any(|(n, _)| n == name) { continue; }
            if let Ok(bytes) = read(&m.folder, &format!("sounds/{file}")) { if sound_ok(&bytes) { out.push((name.to_string(), bytes)); } }
        }
    }
    out
}

/// A hook's script among the active mods: `hooks/<name>.rhai`, with the id
/// of the mod it came from (the first mod that has it).
pub fn hook_script(name: &str) -> Option<(&'static str, String)> {
    active().into_iter().filter(|m| !m.cached).find_map(|m| {
        let path = m.folder.join("hooks").join(format!("{name}.rhai"));
        path.is_file().then(|| read(&m.folder, &format!("hooks/{name}.rhai")).ok()).flatten()
            .and_then(|b| String::from_utf8(b).ok()).map(|s| (m.id, s))
    })
}

/// Every hook the session's definitions and modes name (definitions by
/// class/id, modes by key), for checking that each has its script.
pub fn named_hooks() -> Vec<(String, String)> {
    let r = crate::registry::registry();
    let mut out = Vec::new();
    macro_rules! each { ($t:ident, $c:literal) => { for (_, id, d) in r.$t.iter() { if let Some(h) = &d.hook { out.push((format!("{}/{id}", $c), h.clone())); } } }; }
    each!(armors, "armors"); each!(weapons, "weapons"); each!(packs, "packs"); each!(deployables, "deployables");
    each!(items, "items"); each!(effects, "effects"); each!(vehicles, "vehicles");
    for m in crate::map_catalog::SupportedMode::every() {
        for h in &m.def().hooks { out.push((format!("modes/{}", m.key()), h.clone())); }
    }
    out
}

/// `PEAKRUNNER_MODS`: mod ids, comma-separated.
pub fn from_env() -> Vec<String> {
    std::env::var("PEAKRUNNER_MODS").unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("peakrunner-mod-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, body) in files {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        dir
    }

    /// The definitions an older-style `defs.json` makes.
    fn effective_defs(patches: &[&serde_json::Value]) -> Result<crate::defs::Defs, String> {
        let sources: Vec<crate::registry::Source> = patches.iter().map(|p| crate::registry::Source { name: "test", defs: Some(p), files: Vec::new() }).collect();
        crate::registry::build(&sources).map(|r| r.legacy().clone())
    }

    #[test]
    fn a_mod_changes_only_what_it_names_and_bad_ones_say_why() {
        let low = serde_json::json!({"movement": {"gravity": 12.0}, "weapons": {"disc": {"reload": 0.8}}});
        let defs = effective_defs(&[&low]).unwrap();
        let base = crate::defs::defs();
        assert_eq!((defs.movement.gravity, defs.weapons.disc.reload), (12.0, 0.8));
        assert_eq!(defs.movement.mass, base.movement.mass, "untouched values stay");
        for (bad, why) in [(serde_json::json!({"movement": {"gravty": 1.0}}), "gravty"),
            (serde_json::json!({"movement": {"gravity": -1.0}}), "0 to 1000000"),
            (serde_json::json!({"movement": {"mass": 0.0}}), "movement.mass"),
            (serde_json::json!({"weapons": {"disc": {"speed": 1e9}}}), "0 to 1000000"),
            (serde_json::json!({"armors": {"light": {"ammo": [1, 2]}}}), "defs.json")] {
            let e = effective_defs(&[&bad]).unwrap_err();
            assert!(e.contains(why), "{bad}: {e}");
        }
    }

    /// A mod download fetched again into its folder (trimmed from the cache
    /// meanwhile) is the entry it was: each fetch took another of the 256
    /// slots mods are registered in, until "too many mods".
    #[test]
    fn a_mod_fetched_again_is_the_entry_it_was() {
        let root = std::env::temp_dir().join(format!("peakrunner-mod-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |dir: &Path| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("mod.json"), r#"{"name": "Again", "api": 1}"#).unwrap();
        };
        write(&root.join("scratch"));
        let fp = fingerprint_of(&root.join("scratch")).unwrap();
        let dir = root.join("again-mod").join(&fp);
        write(&dir);
        let first = add_cached("again-mod", dir.clone()).unwrap();
        assert!(std::ptr::eq(add_cached("again-mod", dir.clone()).unwrap(), first), "no new slot");
        assert!(by_fingerprint("again-mod", &fp).is_some_and(|m| std::ptr::eq(m, first)));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(by_fingerprint("again-mod", &fp).is_none(), "gone while trimmed");
        write(&dir);
        assert!(std::ptr::eq(add_cached("again-mod", dir.clone()).unwrap(), first), "fetched again after a trim");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mod_folders_hold_only_data_and_their_fingerprint_covers_it() {
        let good = folder("good", &[("mod.json", r#"{"name": "Quick", "api": 1}"#),
            ("modes/quick-ctf.json", r#"{"base": "ctf", "label": "Quick CTF", "score_limit": 5}"#)]);
        let l = listing(&good).unwrap();
        assert_eq!(l.files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["mod.json", "modes/quick-ctf.json"]);
        assert!(load(&good).unwrap().modes.len() == 1);
        std::fs::write(good.join("modes/quick-ctf.json"), r#"{"base": "ctf", "score_limit": 6}"#).unwrap();
        assert_ne!(listing(&good).unwrap().fingerprint, l.fingerprint, "any change is a new mod");
        for (name, files, why) in [
            ("script", vec![("mod.json", r#"{"name": "X", "api": 1}"#), ("rules.rhai", "x")], "only mod.json"),
            ("future", vec![("mod.json", r#"{"name": "X", "api": 99}"#)], "API"),
            ("nobase", vec![("mod.json", r#"{"name": "X", "api": 1}"#), ("modes/m.json", "{}")], "base"),
            ("nometa", vec![("defs.json", "{}")], "mod.json"),
            ("api2", vec![("mod.json", r#"{"name": "X", "api": 2}"#)], "needs engine API 2, this engine has 1"),
        ] {
            let dir = folder(name, &files);
            let e = load(&dir).err().unwrap_or_default();
            assert!(e.contains(why), "{name}: {e}");
            let _ = std::fs::remove_dir_all(dir);
        }
        let _ = std::fs::remove_dir_all(good);
        // Every setting works at "api": 1.
        let every = folder("every", &[("mod.json", r#"{"name": "Every", "api": 1}"#),
            ("modes/m.json", r#"{"base": "ctf", "weapons": ["disc", "plasma", "blaster"]}"#),
            ("defs.json", r#"{"armors": {"light": {"weapon_slots": 5}}}"#),
            ("projectiles/p.json", r#"{"disc": {"trail": {"style": "smoke", "color": [1, 0, 0]}}}"#)]);
        assert!(load(&every).is_ok(), "{:?}", load(&every).err());
        let _ = std::fs::remove_dir_all(every);
    }

    #[test]
    fn class_folders_load_and_hooks_stay_on_the_server() {
        let dir = folder("classes", &[
            ("mod.json", r#"{"name": "Classes", "api": 1}"#),
            ("weapons/guns.json", "// a faster disc\n{ \"rapid-disc\": { \"extends\": \"disc\", \"reload\": 0.4 /* quick */ } }"),
            ("hooks/zap.rhai", "fn on_hit(game, a, b) { }"),
        ]);
        let l = listing(&dir).unwrap();
        assert_eq!(l.files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["mod.json", "weapons/guns.json"]);
        assert_eq!(l.scripts, vec!["hooks/zap.rhai".to_string()], "hooks are never in the listing players fetch");
        let loaded = load(&dir).unwrap();
        assert_eq!((loaded.files.len(), loaded.hooks.len(), loaded.scripts.len()), (1, 1, 0));
        let r = effective(&[("classes", &loaded)]).unwrap();
        assert_eq!(r.weapons.by_id("rapid-disc").unwrap().reload, 0.4);
        assert!(check(&dir)[0].1.clone().unwrap().contains("adds weapons rapid-disc"), "{:?}", check(&dir));
        // Only class folders, and <id>.json files in them.
        std::fs::create_dir_all(dir.join("gadgets")).unwrap();
        std::fs::write(dir.join("gadgets/x.json"), "{}").unwrap();
        assert!(listing(&dir).err().unwrap().contains("class folders"));
        std::fs::remove_dir_all(dir.join("gadgets")).unwrap();
        std::fs::write(dir.join("weapons/Bad Name.json"), "{}").unwrap();
        assert!(listing(&dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rules_scripts_stay_on_the_server() {
        let spree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mods/spree");
        let l = listing(&spree).unwrap();
        assert_eq!(l.scripts, vec!["rules/spree.rhai".to_string()]);
        assert!(l.files.iter().all(|(p, _, _)| !p.starts_with("rules/")) && !l.text.contains("rules/"), "never in the listing players fetch");
        let loaded = load(&spree).unwrap();
        assert_eq!(loaded.scripts.len(), 1);
        // A mode's script must exist.
        let dir = std::env::temp_dir().join(format!("peakrunner-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("rules")).unwrap();
        std::fs::create_dir_all(dir.join("modes")).unwrap();
        std::fs::write(dir.join("mod.json"), r#"{"name": "Scripted", "api": 1}"#).unwrap();
        std::fs::write(dir.join("rules/x.rhai"), "fn on_frag(game, k, v) { }").unwrap();
        std::fs::write(dir.join("modes/m.json"), r#"{"base": "ctf", "script": "missing"}"#).unwrap();
        assert!(load(&dir).is_ok(), "a player's copy has no scripts");
        assert!(check(&dir)[0].1.clone().unwrap_err().contains("rules/missing.rhai"), "a server's must");
        std::fs::write(dir.join("rules/bad name.rhai"), "").unwrap();
        assert!(listing(&dir).err().unwrap().contains("rules/<key>.rhai"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
