//! Game modes as data (docs/modding.md, P4). A mode is a rule set (what
//! scores and what wins: `Rules`, engine code) plus everything else as a
//! definition: its clock, warmup, respawn, score limit, teams or free-for-all,
//! which systems run, the movement model, the map features it needs, and the
//! text players see. The base package's `modes/*.json` are built into the
//! engine; a mod adds modes as variants of any mode (`base` plus the fields it
//! changes). `map_catalog::SupportedMode` is the handle every other part of
//! the game passes around.
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock, atomic::{AtomicUsize, Ordering}};

/// The rule sets the engine implements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rules { #[default] Ctf, CaptureAndHold, Football, Deathmatch, TeamDeathmatch, Rabbit, TeamRabbit, Duel }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Teams { Two, FreeForAll }

/// Which of a map's capture points run: all of them, only those marked
/// `ctf_active`, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Points { All, CtfActive, None }

/// Skiing with the armor classes, or Football's classic running and jet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementModel { Skier, Classic }

/// Map features a mode needs: a football field (true) or none (false), and
/// at least this many capture points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires { pub football_field: bool, #[serde(default)] pub control_points: usize }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeDef {
    /// Menu name, the short label server lists show, the menu description,
    /// the HUD's goal line and the banner at the start of a match.
    pub label: String, pub short: String, pub note: String, pub goal: String, pub start_message: String,
    pub rules: Rules,
    /// Listed in menus and accepted in rotations.
    pub offered: bool,
    /// Match length in seconds, in one or two halves (a break at half time).
    pub clock: f32, pub halves: u32,
    /// Countdown before a server's first round (later rounds count down 10 s).
    pub warmup: f32,
    /// Seconds from death to respawn, and from Ctrl+K to respawn (0: at once).
    pub respawn: f32, pub suicide_delay: f32,
    /// Captures, points, touchdowns or frags to win.
    pub score_limit: u32,
    pub teams: Teams,
    pub control_points: Points,
    /// A random time of day, weather and twist each round.
    pub conditions: bool,
    pub movement: MovementModel,
    /// Armor scales damage taken and blast kicks.
    pub armor_scaling: bool,
    /// Systems in play: fixed equipment (turrets, sensors, generators),
    /// deployables, ammo stations, grenades and mines, dropped ammo, the
    /// repair tool, and inventory purchases.
    pub equipment: bool, pub deployables: bool, pub ammo_stations: bool, pub throwables: bool,
    pub loot: bool, pub repair_tool: bool, pub purchases: bool,
    /// The weapons players may use, by id: absent, every weapon
    /// in the session; a list, only those (`[]`: no guns, with grenades and
    /// mines as `throwables` allows). `["disc"]` leaves the disc launcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapons: Option<Vec<String>>,
    /// Weapons and throwables it takes out of play, by id, whatever
    /// `weapons` and `throwables` allow (the classic Rabbit's mortar and
    /// mines).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub banned: Vec<String>,
    pub requires: Requires,
    /// Rules from a script instead of the engine's: `rules/<script>.rhai` in
    /// the mod, run only by servers (P6). It decides what scores, what wins
    /// and what is announced; `rules` still picks the engine's systems
    /// (flags, capture points, the ball) and the HUD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// Seconds a dropped flag lies before it returns home.
    #[serde(default = "flag_return")]
    pub flag_return: f32,
    /// Share of damage teammates' fire does to each other (0: none).
    #[serde(default)]
    pub friendly_fire: f32,
    /// What players spawn wearing: the armor they chose (`setup`), or a
    /// random one of those stations offer (`random`).
    #[serde(default)]
    pub spawn_armor: SpawnArmor,
    /// The armors and packs its stations offer, by id (default: every one
    /// whose entry is `offered`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<Vec<String>>,
    /// Credits, earned and spent on what stations hand out (default none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub economy: Option<Economy>,
    /// What a death leaves on the ground besides its rounds.
    #[serde(default, skip_serializing_if = "DeathDrops::is_none")]
    pub death_drops: DeathDrops,
    /// Players may drop the weapon in hand and the pack they wear for
    /// others to take (the drop keys).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub player_drops: bool,
    /// Hooks (`hooks/<name>.rhai`, server only) that hear every event.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
}
fn flag_return() -> f32 { 14.0 }

/// A mode's credits (`economy`): the team's (`shared`) or each player's,
/// from `start`, up to `max`, earned by the minute, by frags, captures
/// and returns, and spent on armors, weapons and packs at their `price`.
/// What a player chose comes back at respawn only if it can be paid for
/// again; otherwise they spawn with their armor's kit.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Economy {
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub start: u32,
    #[serde(default = "credits_max")]
    pub max: u32,
    #[serde(default)]
    pub per_minute: u32,
    #[serde(default)]
    pub frag: u32,
    #[serde(default)]
    pub capture: u32,
    #[serde(default, rename = "return")]
    pub returned: u32,
}
fn credits_max() -> u32 { 100_000 }

/// What a death leaves: the pack worn, the weapon in hand (with its
/// rounds), and items (`items/`), for anyone to take.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeathDrops {
    #[serde(default)]
    pub pack: bool,
    #[serde(default)]
    pub weapon: bool,
    #[serde(default)]
    pub items: Vec<String>,
}
impl DeathDrops { fn is_none(&self) -> bool { *self == DeathDrops::default() } }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnArmor { #[default] Setup, Random }

/// The base game's weapons by id.
pub const WEAPONS: [&str; 8] = ["disc", "chaingun", "grenade_launcher", "mortar", "laser", "railgun", "plasma", "blaster"];

impl ModeDef {
    /// Whether weapon `kind` (a wire number) is allowed.
    pub fn allows(&self, kind: u8) -> bool {
        let Some(id) = crate::registry::registry().weapons.id(kind as usize) else { return false };
        self.weapons.as_ref().is_none_or(|list| list.iter().any(|w| w == id)) && !self.banned.iter().any(|b| b == id)
    }

    /// Whether its stations offer armor `class`: the mode's `armors` list
    /// when it has one, else every entry marked `offered` (so a mod's own
    /// classes stay in its own modes when it marks them not offered).
    pub fn offers_armor(&self, class: crate::sim::ArmorClass) -> bool {
        match &self.armors {
            Some(list) => list.iter().any(|a| a == class.id()),
            None => class.def().offered,
        }
    }
    /// The armors its stations offer, in its own order (its `armors` list),
    /// else light to heavy (registry order within a size).
    pub fn offered_armors(&self) -> Vec<crate::sim::ArmorClass> {
        match &self.armors {
            Some(list) => list.iter().filter_map(|id| crate::sim::ArmorClass::by_id(id)).filter(|&a| self.offers_armor(a)).collect(),
            None => {
                let mut all: Vec<crate::sim::ArmorClass> = crate::registry::registry().armors.iter()
                    .map(|(k, _, _)| crate::sim::ArmorClass(k as u8)).filter(|&a| self.offers_armor(a)).collect();
                all.sort_by_key(|a| a.def().build as u8);
                all
            }
        }
    }

    /// Whether its stations offer pack `kind`: the mode's `packs` list when
    /// it has one, else every entry marked `offered`.
    pub fn offers_pack(&self, kind: crate::sim::packs::PackKind) -> bool {
        match &self.packs {
            Some(list) => list.iter().any(|p| p == kind.key()),
            None => kind.def().offered,
        }
    }
    /// Whether its stations offer pack `kind` to someone wearing `armor`
    /// (an armor's `packs`, when it lists any).
    pub fn offers_pack_to(&self, kind: crate::sim::packs::PackKind, armor: crate::sim::ArmorClass) -> bool {
        self.offers_pack(kind) && kind.fits(armor)
    }

    /// Refuse values that would break a match: a mod's mistake is reported,
    /// never played.
    pub fn validate(&self) -> Result<(), String> {
        let text = |t: &str, max: usize| t.chars().count() <= max && !t.chars().any(char::is_control);
        if self.label.trim().is_empty() || !text(&self.label, 32) || !text(&self.short, 12) || !text(&self.note, 200)
            || !text(&self.goal, 80) || !text(&self.start_message, 80) {
            return Err("mode text: label 1-32, short up to 12, note up to 200, goal and start_message up to 80 plain characters".into());
        }
        let secs = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
        if !secs(self.clock, 30.0, 7200.0) { return Err("clock must be 30 to 7200 seconds".into()); }
        if !(1..=2).contains(&self.halves) { return Err("halves must be 1 or 2".into()); }
        if !secs(self.warmup, 0.0, 120.0) || !secs(self.respawn, 0.0, 60.0) || !secs(self.suicide_delay, 0.0, 60.0) {
            return Err("warmup 0-120, respawn and suicide_delay 0-60 seconds".into());
        }
        if !(1..=100_000).contains(&self.score_limit) { return Err("score_limit must be 1 to 100000".into()); }
        if let Some(w) = self.weapons.iter().flatten().find(|w| !crate::registry::fields::valid_id(w)) {
            return Err(format!("weapons: \"{w}\" isn't a weapon id"));
        }
        if let Some(b) = self.banned.iter().find(|b| !crate::registry::fields::valid_id(b)) {
            return Err(format!("banned: \"{b}\" isn't an id"));
        }
        if self.script.as_deref().is_some_and(|s| !valid_key(s)) { return Err("script names rules/<key>.rhai: a-z, 0-9, _ and -".into()); }
        if !secs(self.flag_return, 0.0, 600.0) { return Err("flag_return must be 0 to 600 seconds".into()); }
        if !(self.friendly_fire.is_finite() && (0.0..=2.0).contains(&self.friendly_fire)) { return Err("friendly_fire must be 0 to 2".into()); }
        for (what, list) in [("armors", &self.armors), ("packs", &self.packs)] {
            if let Some(x) = list.iter().flatten().find(|x| !crate::registry::fields::valid_id(x)) { return Err(format!("{what}: \"{x}\" isn't an id")); }
        }
        if let Some(x) = self.death_drops.items.iter().find(|x| !crate::registry::fields::valid_id(x)) { return Err(format!("death_drops.items: \"{x}\" isn't an id")); }
        if self.death_drops.items.len() > 8 { return Err("death_drops.items: at most 8".into()); }
        if self.hooks.len() > 8 || self.hooks.iter().any(|h| !valid_key(h)) { return Err("hooks: at most 8 names of hooks/<name>.rhai: a-z, 0-9, _ and -".into()); }
        if let Some(e) = &self.economy {
            if e.start > e.max || e.max > 10_000_000 { return Err("economy: start must be at most max, and max at most 10000000".into()); }
        }
        if (self.teams == Teams::FreeForAll) != matches!(self.rules, Rules::Deathmatch | Rules::Rabbit) {
            return Err("free_for_all teams go with deathmatch or rabbit rules, and those with free_for_all".into());
        }
        if self.control_points == Points::All && self.requires.control_points < 1 && self.rules == Rules::CaptureAndHold {
            return Err("Capture & Hold needs requires.control_points of at least 1".into());
        }
        Ok(())
    }
}

/// The base modes, built into the engine, in menu order.
pub(crate) const BASE: [(&str, &str); 9] = [
    ("ctf", include_str!("../../../assets/mods/base/modes/ctf.json")),
    ("capture_and_hold", include_str!("../../../assets/mods/base/modes/capture_and_hold.json")),
    ("football", include_str!("../../../assets/mods/base/modes/football.json")),
    ("deathmatch", include_str!("../../../assets/mods/base/modes/deathmatch.json")),
    ("team_deathmatch", include_str!("../../../assets/mods/base/modes/team_deathmatch.json")),
    ("rabbit", include_str!("../../../assets/mods/base/modes/rabbit.json")),
    ("team_rabbit", include_str!("../../../assets/mods/base/modes/team_rabbit.json")),
    // A duel server's arenas and its lobby (`sim::duel`); servers only.
    ("duel", include_str!("../../../assets/mods/base/modes/duel.json")),
    ("duel_lobby", include_str!("../../../assets/mods/base/modes/duel_lobby.json")),
];

/// How many modes are built in (`SupportedMode::every` lists them first).
pub const BASE_COUNT: usize = BASE.len();

/// A mode key: 1 to 32 lowercase letters, digits, `_` and `-`.
pub fn valid_key(key: &str) -> bool {
    (1..=32).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub(crate) struct Entry { pub key: &'static str, pub def: &'static ModeDef }

const CAPACITY: usize = 256;
static SLOTS: [OnceLock<Entry>; CAPACITY] = [const { OnceLock::new() }; CAPACITY];
static COUNT: AtomicUsize = AtomicUsize::new(0);
static REGISTER: Mutex<()> = Mutex::new(());
static BASE_READY: OnceLock<()> = OnceLock::new();

fn base() {
    BASE_READY.get_or_init(|| {
        for (key, json) in BASE {
            let def: ModeDef = serde_json::from_str(json).unwrap_or_else(|e| panic!("built-in mode {key}: {e}"));
            def.validate().unwrap_or_else(|e| panic!("built-in mode {key}: {e}"));
            push(key, def);
        }
    });
}

fn push(key: &str, def: ModeDef) -> usize {
    let _held = REGISTER.lock().unwrap_or_else(|e| e.into_inner());
    let i = COUNT.load(Ordering::Acquire);
    assert!(i < CAPACITY, "more than {CAPACITY} modes");
    let _ = SLOTS[i].set(Entry { key: Box::leak(key.to_string().into_boxed_str()), def: Box::leak(Box::new(def)) });
    COUNT.store(i + 1, Ordering::Release);
    i
}

pub(crate) fn entry(i: usize) -> &'static Entry {
    base();
    SLOTS[i].get().expect("every mode id is registered")
}

#[cfg(test)]
pub(crate) fn count() -> usize { base(); COUNT.load(Ordering::Acquire) }

/// Register a mode (a mod's), returning its index.
pub(crate) fn add(key: &str, def: ModeDef) -> Result<usize, String> {
    if !valid_key(key) { return Err(format!("{key:?} isn't a mode key (a-z, 0-9, _ and -)")); }
    def.validate().map_err(|e| format!("mode {key}: {e}"))?;
    base();
    Ok(push(key, def))
}

/// This session's modes by key (a server's mods), found before the built-in ones.
static SESSION: Mutex<Vec<(String, u16)>> = Mutex::new(Vec::new());

pub(crate) fn session_modes() -> Vec<u16> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, i)| *i).collect()
}

pub(crate) fn session(key: &str) -> Option<u16> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, _)| k == key).map(|(_, i)| *i)
}

/// Bind a session's modes (a server's mods) by key; `end_session` forgets them.
pub fn bind_session(modes: Vec<(String, u16)>) { *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = modes; }
pub fn end_session() { SESSION.lock().unwrap_or_else(|e| e.into_inner()).clear(); }

/// A mode as a mod writes it: `base` names the mode it varies, every other
/// field replaces that mode's (objects merge field by field).
pub fn variant(base_def: &ModeDef, json: &serde_json::Value) -> Result<ModeDef, String> {
    let mut value = serde_json::to_value(base_def).map_err(|e| e.to_string())?;
    let mut over = json.clone();
    if let Some(o) = over.as_object_mut() { o.remove("base"); }
    crate::defs::merge(&mut value, &over);
    let def: ModeDef = serde_json::from_value(value).map_err(|e| e.to_string())?;
    def.validate()?;
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_modes_load_and_a_variant_changes_only_what_it_says() {
        assert!(count() >= 5);
        let ctf = entry(0).def;
        assert_eq!((entry(0).key, ctf.label.as_str(), ctf.score_limit), ("ctf", "Capture the Flag", 3));
        let quick = variant(ctf, &serde_json::json!({"base": "ctf", "label": "Quick CTF", "score_limit": 5, "requires": {"control_points": 0}})).unwrap();
        assert_eq!((quick.label.as_str(), quick.score_limit, quick.rules, quick.clock), ("Quick CTF", 5, Rules::Ctf, 480.0));
        assert!(!quick.requires.football_field);
        assert!(quick.weapons.is_none() && (0..8).all(|k| quick.allows(k)), "every weapon unless a mode says");
        // Unlisted, the stations go light to heavy; a list keeps its order
        // and wins over `offered`.
        let ids = |d: &ModeDef| d.offered_armors().iter().map(|a| a.id()).collect::<Vec<_>>();
        assert_eq!(ids(ctf), ["light", "medium", "heavy"]);
        let listed = variant(ctf, &serde_json::json!({"armors": ["heavy", "football"]})).unwrap();
        assert_eq!(ids(&listed), ["heavy", "football"]);
        let none = variant(ctf, &serde_json::json!({"weapons": []})).unwrap();
        assert!(!(0..8).any(|k| none.allows(k)));
        let two = variant(ctf, &serde_json::json!({"weapons": ["disc", "railgun"]})).unwrap();
        assert_eq!((0..8).map(|k| two.allows(k)).collect::<Vec<_>>(), [true, false, false, false, false, false, false, true]);
        for bad in [serde_json::json!({"clock": 1.0}), serde_json::json!({"score_limit": 0}), serde_json::json!({"no_such": 1}),
            serde_json::json!({"label": ""}), serde_json::json!({"rules": "tag"}), serde_json::json!({"weapons": ["Spoon!"]}),
            serde_json::json!({"weapons": "disc"})] {
            assert!(variant(ctf, &bad).is_err(), "{bad}");
        }
    }
}
