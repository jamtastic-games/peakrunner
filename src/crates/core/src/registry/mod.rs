//! The engine's classes and the entries content defines with them
//! (docs/modding.md, "Engine classes"). The engine knows *kinds* of things
//! (an armor, a weapon, a projectile, a blast, a throwable, a deployable)
//! and how each kind behaves; every actual item is an entry in a class's
//! folder of a package: the base game's (`src/assets/mods/base/`, built in)
//! and then each mod's, in order.
//!
//! A mod reaches any entry by its id and redefines it:
//! - an id that already exists is patched key by key (`null` removes a key);
//! - `"replace": true` starts the entry over;
//! - `"extends": "<id>"` copies another entry, then patches the copy;
//! - a new id must be complete.
//!
//! Entries refer to each other by id (a weapon names its projectile, a
//! projectile its blast); building a registry checks every reference, every
//! number against `fields`, and gives each entry its wire number: the base
//! game's keep today's numbers, so its snapshots never move; the rest follow
//! in package order, ids sorted.
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

pub mod classes;
pub mod fields;
pub mod legacy;

pub use classes::*;

/// The engine's classes, each a folder of a package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class { Armor, Weapon, Projectile, Blast, Throwable, Deployable, Damage, Effect, Pack, Item, Vehicle }

impl Class {
    pub const ALL: [Class; 11] = [Class::Armor, Class::Weapon, Class::Projectile, Class::Blast, Class::Throwable, Class::Deployable, Class::Damage, Class::Effect, Class::Pack, Class::Item, Class::Vehicle];
    /// The folder a package keeps its entries in.
    pub fn folder(self) -> &'static str {
        match self {
            Class::Armor => "armors", Class::Weapon => "weapons", Class::Projectile => "projectiles",
            Class::Blast => "blasts", Class::Throwable => "throwables", Class::Deployable => "deployables", Class::Damage => "damage", Class::Effect => "effects", Class::Pack => "packs", Class::Item => "items", Class::Vehicle => "vehicles",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Class::Armor => "Armor", Class::Weapon => "Weapon", Class::Projectile => "Projectile",
            Class::Blast => "Blast", Class::Throwable => "Throwable", Class::Deployable => "Deployable", Class::Damage => "Damage kind", Class::Effect => "Effect", Class::Pack => "Pack", Class::Item => "Item", Class::Vehicle => "Vehicle",
        }
    }
    pub fn from_folder(folder: &str) -> Option<Class> { Class::ALL.into_iter().find(|c| c.folder() == folder) }
    /// Most entries a session may hold (their numbers travel as bytes).
    pub fn cap(self) -> usize {
        match self { Class::Armor | Class::Throwable | Class::Vehicle => 32, Class::Weapon | Class::Deployable | Class::Damage | Class::Effect | Class::Pack | Class::Item => 64, Class::Projectile | Class::Blast => 128 }
    }
    /// The base game's entries in wire order: their numbers never change.
    /// An empty name holds a retired number open.
    fn fixed(self) -> &'static [&'static str] {
        match self {
            Class::Armor => &["light", "heavy", "football"],
            Class::Weapon => &["disc", "chaingun", "grenade_launcher", "laser", "plasma", "blaster", "mortar", "railgun"],
            Class::Projectile => &["disc", "bullet", "grenade-shell", "turret-plasma", "", "mortar-shell", "hand-grenade", "mine",
                "laser", "rail-slug", "plasma-bolt", "blaster-bolt"],
            Class::Blast => &[],
            Class::Throwable => &["hand-grenade", "mine"],
            Class::Deployable => &["turret", "wall", "field", "ammo", "inventory"],
            Class::Damage | Class::Effect | Class::Item | Class::Vehicle => &[],
            Class::Pack => &["turret", "wall", "field", "ammo", "inventory"],
        }
    }
}

/// One entry: its id and what it defines.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry<T> { pub id: String, pub def: T }

/// A class's entries by wire number (`None`: a retired number).
#[derive(Clone, Debug, PartialEq)]
pub struct Table<T> { entries: Vec<Option<Entry<T>>>, by_id: HashMap<String, usize> }

impl<T> Table<T> {
    fn new(entries: Vec<Option<Entry<T>>>) -> Self {
        let by_id = entries.iter().enumerate().filter_map(|(i, e)| e.as_ref().map(|e| (e.id.clone(), i))).collect();
        Table { entries, by_id }
    }
    /// The entry with wire number `i`.
    pub fn get(&self, i: usize) -> Option<&T> { self.entries.get(i).and_then(|e| e.as_ref()).map(|e| &e.def) }
    pub fn id(&self, i: usize) -> Option<&str> { self.entries.get(i).and_then(|e| e.as_ref()).map(|e| e.id.as_str()) }
    /// The wire number of `id`.
    pub fn index(&self, id: &str) -> Option<usize> { self.by_id.get(id).copied() }
    pub fn by_id(&self, id: &str) -> Option<&T> { self.index(id).and_then(|i| self.get(i)) }
    /// Every entry: (wire number, id, definition).
    pub fn iter(&self) -> impl Iterator<Item = (usize, &str, &T)> {
        self.entries.iter().enumerate().filter_map(|(i, e)| e.as_ref().map(|e| (i, e.id.as_str(), &e.def)))
    }
    /// Wire numbers in use run below this.
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

/// Everything a session plays with: the world's numbers and every class's
/// entries, base game first, then the mods in force.
#[derive(Clone, Debug, PartialEq)]
pub struct Registry {
    pub globals: Globals,
    pub armors: Table<Armor>,
    pub weapons: Table<Weapon>,
    pub projectiles: Table<Projectile>,
    pub blasts: Table<Blast>,
    pub throwables: Table<Throwable>,
    pub deployables: Table<Deployable>,
    pub damage: Table<Damage>,
    pub effects: Table<Effect>,
    pub packs: Table<Pack>,
    pub items: Table<Item>,
    pub vehicles: Table<Vehicle>,
    /// The definitions as the older `defs()` accessors read them (set once
    /// the registry is built and checked).
    legacy: OnceLock<crate::defs::Defs>,
}

/// One package's contribution, read from its folder.
pub struct Source<'a> {
    /// "base", or the mod's id (for messages).
    pub name: &'a str,
    /// `defs.json`: the world's numbers, and (from older mods) item numbers
    /// at their old paths, which `legacy::split` maps onto entries.
    pub defs: Option<&'a Value>,
    /// Class files: (`<folder>/<name>.json`, parsed).
    pub files: Vec<(String, Value)>,
}

macro_rules! base_files {
    ($($path:literal),* $(,)?) => { &[$(($path, include_str!(concat!("../../../../assets/mods/base/", $path)))),*] };
}
/// The base package, built into every engine.
pub const BASE_DEFS: &str = include_str!("../../../../assets/mods/base/defs.json");
pub const BASE_FILES: &[(&str, &str)] = base_files![
    "armors/base.json", "weapons/base.json", "projectiles/base.json", "projectiles/library.json", "blasts/base.json", "blasts/library.json",
    "throwables/base.json", "deployables/base.json", "deployables/library.json", "damage/base.json", "effects/library.json", "packs/base.json", "packs/library.json", "items/library.json", "vehicles/library.json",
    "weapons/vehicles.json", "projectiles/vehicles.json", "blasts/vehicles.json",
];

/// The base package as a source.
fn base_source() -> (Value, Vec<(String, Value)>) {
    let defs = crate::jsonc::parse(BASE_DEFS).expect("the built-in defs.json parses (tested)");
    let files = BASE_FILES.iter().map(|(p, t)| (p.to_string(), crate::jsonc::parse(t).unwrap_or_else(|e| panic!("built-in {p}: {e}")))).collect();
    (defs, files)
}

/// The base game's registry, built once.
pub fn base() -> &'static Registry {
    static BASE: OnceLock<Registry> = OnceLock::new();
    BASE.get_or_init(|| build(&[]).expect("the built-in base package is valid (tested)"))
}

/// The registry in force: the base game's, or with the active mods'.
pub fn registry() -> &'static Registry { crate::mods::current().unwrap_or_else(base) }

/// A class's entries while they're being layered: id → (value, the package
/// that introduced it).
type Layered = BTreeMap<String, (Value, usize)>;

/// Build a registry from the base package and `mods`, in order.
pub fn build(mods: &[Source]) -> Result<Registry, String> {
    let (base_defs, base_files) = base_source();
    let base = Source { name: "base", defs: None, files: base_files };
    let mut globals = base_defs;
    let mut classes: BTreeMap<Class, Layered> = Class::ALL.iter().map(|c| (*c, Layered::new())).collect();
    for (layer, source) in std::iter::once(&base).chain(mods).enumerate() {
        let mut patches: Vec<(Class, String, Value, String)> = Vec::new();
        if let Some(defs) = source.defs {
            let (world, items) = legacy::split(defs)?;
            crate::defs::merge(&mut globals, &world);
            patches.extend(items.into_iter().map(|(c, id, v)| (c, id, v, "defs.json".to_string())));
        }
        let first_file = patches.len();
        for (path, value) in &source.files {
            let class = path.split('/').next().and_then(Class::from_folder).ok_or_else(|| format!("{}: {path}: not a class folder", source.name))?;
            let Value::Object(entries) = value else { return Err(format!("{}: {path}: must be an object of entries by id", source.name)) };
            for (id, entry) in entries {
                if !fields::valid_id(id) { return Err(format!("{}: {path}: {id:?} isn't an id (1 to 32 of a-z, 0-9, _ and -)", source.name)); }
                if !entry.is_object() { return Err(format!("{}: {path}: {id} must be an object", source.name)); }
                if patches[first_file..].iter().any(|(c, i, _, _)| *c == class && i == id) {
                    return Err(format!("{}: {path}: {} {id} is defined twice in this package", source.name, class.folder()));
                }
                patches.push((class, id.clone(), entry.clone(), path.clone()));
            }
        }
        apply(&mut classes, patches, layer, source.name)?;
    }
    let globals: Globals = serde_json::from_value(globals).map_err(|e| format!("defs.json: {e}"))?;
    for (name, v) in [("movement.mass", globals.movement.mass), ("movement.energy_max", globals.movement.energy_max),
        ("repair.energy", globals.repair.energy)] {
        if v <= 0.0 { return Err(format!("{name} must be above 0")); }
    }
    let armors = table::<Armor>(Class::Armor, &classes)?;
    let mut weapons = table::<Weapon>(Class::Weapon, &classes)?;
    // Each mode: the weapon with the mode's changes laid over it.
    for entry in weapons.entries.iter_mut().flatten() {
        let value = &classes[&Class::Weapon][&entry.id].0;
        for (n, m) in entry.def.modes.iter().enumerate() {
            let mut v = value.clone();
            if let Some(o) = v.as_object_mut() { o.remove("modes"); }
            crate::defs::merge(&mut v, m);
            let at = format!("weapons/{}.modes[{n}]", entry.id);
            let mode: Weapon = serde_json::from_value(v.clone()).map_err(|e| format!("{at}: {e}"))?;
            fields::check(Class::Weapon, &v, &at)?;
            entry.def.mode_defs.push(mode);
        }
    }
    let projectiles = table::<Projectile>(Class::Projectile, &classes)?;
    let blasts = table::<Blast>(Class::Blast, &classes)?;
    let throwables = table::<Throwable>(Class::Throwable, &classes)?;
    let deployables = table::<Deployable>(Class::Deployable, &classes)?;
    let damage = table::<Damage>(Class::Damage, &classes)?;
    let effects = table::<Effect>(Class::Effect, &classes)?;
    let packs = table::<Pack>(Class::Pack, &classes)?;
    let items = table::<Item>(Class::Item, &classes)?;
    let vehicles = table::<Vehicle>(Class::Vehicle, &classes)?;
    let r = Registry { globals, armors, weapons, projectiles, blasts, throwables, deployables, damage, effects, packs, items, vehicles, legacy: OnceLock::new() };
    r.resolve()?;
    let _ = r.legacy.set(legacy::defs_view(&r)?);
    Ok(r)
}

/// Lay one package's patches onto the classes. Plain patches go first, so
/// an entry that extends another copies it as this package leaves it;
/// entries that extend wait until their source exists (in this package or
/// an earlier one).
fn apply(classes: &mut BTreeMap<Class, Layered>, mut pending: Vec<(Class, String, Value, String)>, layer: usize, name: &str) -> Result<(), String> {
    pending.sort_by_key(|(_, _, v, _)| v.get("extends").is_some());
    while !pending.is_empty() {
        let before = pending.len();
        let mut later = Vec::new();
        for (class, id, mut patch, path) in pending {
            let table = classes.get_mut(&class).expect("every class has a table");
            let obj = patch.as_object_mut().expect("entries are objects");
            let extends = obj.remove("extends");
            let replace = obj.remove("replace");
            let replace = match replace { None => false, Some(Value::Bool(b)) => b, Some(_) => return Err(format!("{name}: {path}: {id}: replace is true or false")) };
            let at = format!("{name}: {path}: {id}");
            match extends {
                Some(Value::String(from)) => {
                    let Some((source, _)) = table.get(&from).cloned() else {
                        // Not yet: maybe later in this package.
                        let mut back = patch;
                        back.as_object_mut().expect("object").insert("extends".into(), Value::String(from));
                        if replace { back.as_object_mut().expect("object").insert("replace".into(), Value::Bool(true)); }
                        later.push((class, id, back, path));
                        continue;
                    };
                    if table.contains_key(&id) && !replace { return Err(format!("{at}: {id} exists already; extending into it needs \"replace\": true")); }
                    let mut value = source;
                    crate::defs::merge(&mut value, &patch);
                    let introduced = table.get(&id).map_or(layer, |(_, l)| *l);
                    table.insert(id, (value, introduced));
                }
                Some(_) => return Err(format!("{at}: extends names an id")),
                None => match table.get_mut(&id) {
                    Some(slot) if !replace => crate::defs::merge(&mut slot.0, &patch),
                    Some(slot) => slot.0 = patch,
                    None => { table.insert(id, (patch, layer)); }
                },
            }
        }
        if later.len() == before {
            let (class, id, patch, path) = &later[0];
            return Err(format!("{name}: {path}: {id}: extends {} {}, which no package defines (or the extends go round in a circle)",
                class.folder(), patch["extends"].as_str().unwrap_or_default()));
        }
        pending = later;
    }
    Ok(())
}

/// A class's typed table: wire numbers given, every entry checked.
fn table<T: DeserializeOwned>(class: Class, classes: &BTreeMap<Class, Layered>) -> Result<Table<T>, String> {
    let layered = &classes[&class];
    let fixed = class.fixed();
    let mut order: Vec<Option<&str>> = fixed.iter().map(|id| (!id.is_empty()).then_some(*id)).collect();
    for id in fixed.iter().filter(|id| !id.is_empty()) {
        if !layered.contains_key(*id) { return Err(format!("base: {} {id} is missing", class.folder())); }
    }
    let layers = layered.values().map(|(_, l)| *l).max().unwrap_or(0);
    for layer in 0..=layers {
        for (id, (_, l)) in layered {
            if *l == layer && !fixed.contains(&id.as_str()) { order.push(Some(id)); }
        }
    }
    if order.len() > class.cap() { return Err(format!("more than {} {} in a session", class.cap(), class.folder())); }
    let mut entries = Vec::with_capacity(order.len());
    for id in order {
        let Some(id) = id else { entries.push(None); continue };
        let value = &layered[id].0;
        let at = format!("{}/{id}", class.folder());
        let def: T = serde_json::from_value(value.clone()).map_err(|e| format!("{at}: {e}"))?;
        fields::check(class, value, &at)?;
        entries.push(Some(Entry { id: id.to_string(), def }));
    }
    Ok(Table::new(entries))
}

impl Registry {
    /// Every reference names an entry of the right class, and each entry
    /// makes sense as a whole.
    fn resolve(&self) -> Result<(), String> {
        let has = |class: Class, id: &str| -> bool {
            match class {
                Class::Armor => self.armors.index(id).is_some(), Class::Weapon => self.weapons.index(id).is_some(),
                Class::Projectile => self.projectiles.index(id).is_some(), Class::Blast => self.blasts.index(id).is_some(),
                Class::Throwable => self.throwables.index(id).is_some(), Class::Deployable => self.deployables.index(id).is_some(),
                Class::Damage => self.damage.index(id).is_some(),
                Class::Effect => self.effects.index(id).is_some(),
                Class::Pack => self.packs.index(id).is_some(),
                Class::Item => self.items.index(id).is_some(),
                Class::Vehicle => self.vehicles.index(id).is_some(),
            }
        };
        let need = |class: Class, id: &str, at: String| -> Result<(), String> {
            if has(class, id) { Ok(()) } else { Err(format!("{at}: no {} {id:?}", class.title().to_lowercase())) }
        };
        for (_, id, a) in self.armors.iter() {
            for w in a.weapons.keys() { need(Class::Weapon, w, format!("armors/{id}.weapons"))?; }
            for w in &a.kit {
                if !a.weapons.contains_key(w) { return Err(format!("armors/{id}.kit: {w} isn't among the weapons it may carry")); }
            }
            if a.kit.len() > a.weapon_slots as usize { return Err(format!("armors/{id}.kit: more weapons than its {} slots", a.weapon_slots)); }
            for k in a.resist.keys() { need(Class::Damage, k, format!("armors/{id}.resist"))?; }
            for k in &a.packs { need(Class::Pack, k, format!("armors/{id}.packs"))?; }
            if let Some(b) = &a.ability {
                let at = format!("armors/{id}.ability");
                for e in b.effects.iter().chain(&b.strain_effect) { need(Class::Effect, e, format!("{at}.effects"))?; }
                match b.use_ {
                    PackUse::Use => match &b.action { Some(x) => self.check_action(x, &at)?, None => return Err(format!("{at}: a class key that is used needs an \"action\"")) },
                    PackUse::Toggle => {}
                    _ => return Err(format!("{at}.use: a class key is \"toggle\" or \"use\"")),
                }
            }
            if let Some(t) = &a.touch {
                need(Class::Damage, &t.damage_kind, format!("armors/{id}.touch.damage_kind"))?;
                for e in t.ally_effect.iter().chain(&t.effect) { need(Class::Effect, e, format!("armors/{id}.touch"))?; }
            }
            if a.mounts.len() > crate::sim::packs::MAX_MOUNTS { return Err(format!("armors/{id}.mounts: at most {}", crate::sim::packs::MAX_MOUNTS)); }
            for (name, m) in &a.mounts {
                if let Some(w) = &m.weapon {
                    need(Class::Weapon, w, format!("armors/{id}.mounts.{name}.weapon"))?;
                    if m.range <= 0.0 { return Err(format!("armors/{id}.mounts.{name}: a mounted weapon needs a range")); }
                }
            }
            for (key, c) in [("grenade", &a.grenade), ("mine", &a.mine)] {
                if let Some(c) = c { need(Class::Throwable, &c.throwable, format!("armors/{id}.{key}"))?; }
            }
            if a.movement.drain > 0.0 && a.movement.min_jet <= 0.0 && a.movement.regen <= 0.0 {
                return Err(format!("armors/{id}: a jet that drains needs regen or min_jet above 0"));
            }
        }
        for (_, id, weapon) in self.weapons.iter() {
            for w in std::iter::once(weapon).chain(&weapon.mode_defs) {
                need(Class::Projectile, &w.projectile, format!("weapons/{id}.projectile"))?;
                let p = self.projectiles.by_id(&w.projectile);
                if p.is_some_and(|p| p.class == ProjectileClass::Thrown) {
                    return Err(format!("weapons/{id}: fires {}, which is thrown, not fired", w.projectile));
                }
                if (w.fire == Fire::Sustained) != p.is_some_and(|p| p.class == ProjectileClass::Stream) {
                    return Err(format!("weapons/{id}: a sustained weapon fires a stream, and a stream needs a sustained weapon"));
                }
                if w.fire == Fire::Burst && w.burst == 0 { return Err(format!("weapons/{id}: a burst needs \"burst\" shots")); }
                if w.fire == Fire::Spin && w.spin_up <= 0.0 { return Err(format!("weapons/{id}: spin needs \"spin_up\" seconds")); }
                if w.fire == Fire::Charge && w.charge <= 0.0 { return Err(format!("weapons/{id}: a charge needs \"charge\" seconds")); }
            }
        }
        for (_, id, p) in self.projectiles.iter() {
            for b in [&p.blast, &p.structure_blast].into_iter().flatten() { need(Class::Blast, b, format!("projectiles/{id}"))?; }
            need(Class::Damage, &p.damage_kind, format!("projectiles/{id}.damage_kind"))?;
            if let Some(e) = &p.effect { need(Class::Effect, e, format!("projectiles/{id}.effect"))?; }
            if let Some(d) = &p.becomes { need(Class::Deployable, d, format!("projectiles/{id}.becomes"))?; }
            if let Some(t) = &p.drops { need(Class::Item, t, format!("projectiles/{id}.drops"))?; }
            let bursts = p.expire == Expire::Explode || p.trigger.is_some() || p.class == ProjectileClass::Shell
                || (p.class == ProjectileClass::Straight && p.damage == 0.0);
            if bursts && p.blast.is_none() && !matches!(p.class, ProjectileClass::Beam | ProjectileClass::Stream) {
                return Err(format!("projectiles/{id}: it bursts, so it needs a blast (or \"expire\": \"vanish\" and a damage)"));
            }
            if p.class == ProjectileClass::Beam && p.range <= 0.0 { return Err(format!("projectiles/{id}: a beam needs a range")); }
            if p.class == ProjectileClass::Stream && p.stream.is_none() { return Err(format!("projectiles/{id}: a stream needs \"stream\"")); }
            if p.trigger.is_some() && p.class != ProjectileClass::Thrown { return Err(format!("projectiles/{id}: only a thrown one lies in wait")); }
        }
        for (_, id, t) in self.throwables.iter() {
            need(Class::Projectile, &t.projectile, format!("throwables/{id}.projectile"))?;
            if self.projectiles.by_id(&t.projectile).is_some_and(|p| p.class != ProjectileClass::Thrown) {
                return Err(format!("throwables/{id}: {} isn't a thrown projectile", t.projectile));
            }
        }
        for (_, id, e) in self.effects.iter() {
            need(Class::Damage, &e.damage_kind, format!("effects/{id}.damage_kind"))?;
            for k in &e.absorb_kinds { need(Class::Damage, k, format!("effects/{id}.absorb_kinds"))?; }
            for a in &e.immune { need(Class::Armor, a, format!("effects/{id}.immune"))?; }
        }
        for (_, id, v) in self.vehicles.iter() {
            let at = format!("vehicles/{id}");
            if !v.seats.contains_key("pilot") || v.seats.len() > crate::sim::vehicles::MAX_SEATS {
                return Err(format!("{at}.seats: a \"pilot\" seat, and at most {} in all", crate::sim::vehicles::MAX_SEATS));
            }
            for (name, s) in &v.seats { if let Some(w) = &s.weapon { need(Class::Weapon, w, format!("{at}.seats.{name}.weapon"))?; } }
            if let Some(b) = &v.blast { need(Class::Blast, b, format!("{at}.blast"))?; }
            for (k, p) in v.parts.iter().enumerate() {
                if p.aim.as_ref().is_some_and(|s| !v.seats.contains_key(s)) { return Err(format!("{at}.parts[{k}].aim: not one of its seats")); }
            }
        }
        for (_, id, d) in self.deployables.iter() {
            for v in &d.vehicles { need(Class::Vehicle, v, format!("deployables/{id}.vehicles"))?; }
        }
        for (_, id, t) in self.items.iter() {
            if let Some(e) = &t.effect { need(Class::Effect, e, format!("items/{id}.effect"))?; }
        }
        for (_, id, p) in self.packs.iter() {
            let at = format!("packs/{id}");
            if let Some(d) = &p.deployable { need(Class::Deployable, d, format!("{at}.deployable"))?; }
            for e in &p.effects { need(Class::Effect, e, format!("{at}.effects"))?; }
            match p.use_ {
                PackUse::Place if p.deployable.is_none() => return Err(format!("{at}: a pack that places needs \"deployable\"")),
                PackUse::Use if p.action.is_none() => return Err(format!("{at}: a pack that is used needs an \"action\"")),
                _ => {}
            }
            if let Some(a) = &p.action { self.check_action(a, &at)?; }
        }
        for (_, id, d) in self.deployables.iter() {
            if let Some(t) = &d.turret { need(Class::Projectile, &t.projectile, format!("deployables/{id}.turret"))?; }
            if let Some(a) = &d.aura { need(Class::Effect, &a.effect, format!("deployables/{id}.aura.effect"))?; }
            if let Some(s) = &d.shock {
                need(Class::Damage, &s.damage_kind, format!("deployables/{id}.shock.damage_kind"))?;
                if let Some(e) = &s.effect { need(Class::Effect, e, format!("deployables/{id}.shock.effect"))?; }
            }
            if d.behavior == Behavior::Turret && d.turret.is_none() { return Err(format!("deployables/{id}: a turret needs \"turret\"")); }
        }
        Ok(())
    }

    /// An action names exactly one thing to do, and only entries that exist.
    pub(crate) fn check_action(&self, a: &Action, at: &str) -> Result<(), String> {
        let n = [a.fire.is_some(), a.throw.is_some(), a.place.is_some(), a.recall.is_some(), a.effect.is_some(), a.impulse.is_some(), a.armor.is_some()]
            .iter().filter(|x| **x).count();
        if n != 1 { return Err(format!("{at}.action: exactly one of fire, throw, place, recall, effect, impulse and armor")); }
        let has = |found: bool, what: &str, id: &str| if found { Ok(()) } else { Err(format!("{at}.action: no {what} {id:?}")) };
        if let Some(w) = &a.fire { has(self.weapons.index(w).is_some(), "weapon", w)?; }
        if let Some(t) = &a.throw { has(self.throwables.index(t).is_some(), "throwable", t)?; }
        if let Some(d) = a.place.as_ref().or(a.recall.as_ref()) { has(self.deployables.index(d).is_some(), "deployable", d)?; }
        if let Some(e) = &a.effect { has(self.effects.index(e).is_some(), "effect", e)?; }
        if let Some(r) = &a.armor { has(self.armors.index(r).is_some(), "armor", r)?; }
        Ok(())
    }

    /// The definitions in the shape the older accessors (`defs()`) read.
    pub fn legacy(&self) -> &crate::defs::Defs { self.legacy.get().expect("set when built") }

    /// The look a projectile of wire number `kind` is drawn with, as its
    /// base number (`Look::kind`); a number with no entry (the generator
    /// burst, 4) is its own.
    pub fn look_kind(&self, kind: u8) -> u8 { self.projectiles.get(kind as usize).map_or(kind, |p| p.look().kind()) }

    /// Whether a projectile of wire number `kind` trails smoke.
    pub fn smokes(&self, kind: u8) -> bool { self.projectiles.get(kind as usize).is_some_and(|p| p.smoke) }

    /// A projectile's wire number by id (it was checked to exist).
    pub fn projectile_kind(&self, id: &str) -> u8 { self.projectiles.index(id).unwrap_or(0) as u8 }

    /// What `--check` says a package adds and changes: per class, the ids it
    /// introduced and the ones it patched.
    pub fn summary(&self) -> String {
        let counts = [("armors", self.armors.iter().count()), ("weapons", self.weapons.iter().count()),
            ("projectiles", self.projectiles.iter().count()), ("blasts", self.blasts.iter().count()),
            ("throwables", self.throwables.iter().count()), ("deployables", self.deployables.iter().count())];
        counts.iter().map(|(n, c)| format!("{c} {n}")).collect::<Vec<_>>().join(", ")
    }
}

/// Which entries a built registry adds to or changes from the base game's,
/// by class: for `--check`.
pub fn changes(built: &Registry) -> Vec<String> {
    let base = base();
    let mut out = Vec::new();
    macro_rules! diff {
        ($t:ident, $name:literal) => {{
            let added: Vec<&str> = built.$t.iter().filter(|(_, id, _)| base.$t.index(id).is_none()).map(|(_, id, _)| id).collect();
            let changed: Vec<&str> = built.$t.iter().filter(|(_, id, d)| base.$t.by_id(id).is_some_and(|b| b != *d)).map(|(_, id, _)| id).collect();
            if !added.is_empty() { out.push(format!("adds {} {}", $name, added.join(" "))); }
            if !changed.is_empty() { out.push(format!("changes {} {}", $name, changed.join(" "))); }
        }};
    }
    diff!(armors, "armors");
    diff!(weapons, "weapons");
    diff!(projectiles, "projectiles");
    diff!(blasts, "blasts");
    diff!(throwables, "throwables");
    diff!(deployables, "deployables");
    diff!(damage, "damage kinds");
    diff!(effects, "effects");
    diff!(packs, "packs");
    diff!(items, "items");
    diff!(vehicles, "vehicles");
    if built.globals != base.globals { out.push("changes world numbers".into()); }
    out
}

impl Registry {
    /// A weapon a mode's list names that this registry hasn't got.
    pub fn unknown_weapon<'a>(&self, mode: &'a crate::modes::ModeDef) -> Option<&'a str> {
        mode.weapons.iter().flatten().find(|w| self.weapons.index(w).is_none()).map(|w| w.as_str())
    }

    /// The first id a mode names that no entry has, as "field: id": its
    /// weapons, banned weapons and throwables, armors, packs and death
    /// drops' items.
    pub fn unknown_in(&self, mode: &crate::modes::ModeDef) -> Option<String> {
        if let Some(w) = self.unknown_weapon(mode) { return Some(format!("weapons: no weapon \"{w}\"")); }
        if let Some(b) = mode.banned.iter().find(|b| self.weapons.index(b).is_none() && self.throwables.index(b).is_none()) {
            return Some(format!("banned: no weapon or throwable \"{b}\""));
        }
        if let Some(a) = mode.armors.iter().flatten().find(|a| self.armors.index(a).is_none()) { return Some(format!("armors: no armor \"{a}\"")); }
        if let Some(p) = mode.packs.iter().flatten().find(|p| self.packs.index(p).is_none()) { return Some(format!("packs: no pack \"{p}\"")); }
        mode.death_drops.items.iter().find(|t| self.items.index(t).is_none()).map(|t| format!("death_drops.items: no item \"{t}\""))
    }
}

#[cfg(test)]
mod tests;
