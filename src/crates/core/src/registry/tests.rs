use super::*;
use serde_json::json;

/// The base package's definitions as they were before the classes: every
/// value in the class files must reproduce them bit for bit.
const OLD_BASE: &str = include_str!("../legacy_base_defs.json");

fn old_base() -> crate::defs::Defs {
    let mut d: crate::defs::Defs = serde_json::from_str(OLD_BASE).unwrap();
    // The deliberate differences: the grenade launcher's shell bounced
    // until 1.65 s of its 2 s life were left, a constant the old field
    // `arm` (0, never read) didn't express. It is the shell's `arm` now.
    d.weapons.grenade_launcher.arm = 0.35;
    // Heavy armor carries five weapons since the three armors split the
    // arsenal (light 3, medium 4, heavy 5).
    d.armors.heavy.weapon_slots = 5;
    // The railgun's slug flies at 600 m/s (300 until 2026-10-09), led at
    // range, and does 126.67: a hit downs light armor and takes 95% of medium's.
    d.weapons.railgun.speed = 600.0;
    d.weapons.railgun.damage = 126.67;
    d
}

/// Every float as its exact bits, so equality is bit for bit.
fn bits(d: &crate::defs::Defs) -> String { serde_json::to_string(d).unwrap() }

fn with(mods: &[Source]) -> Result<Registry, String> { build(mods) }

fn files(list: &[(&str, serde_json::Value)]) -> Vec<(String, serde_json::Value)> {
    list.iter().map(|(p, v)| (p.to_string(), v.clone())).collect()
}

#[test]
fn the_base_package_matches_the_old_definitions_bit_for_bit() {
    let r = base();
    assert_eq!(bits(r.legacy()), bits(&old_base()));
    assert_eq!(r.legacy(), &old_base());
    // The shell's arming reproduces the old constant exactly.
    let shell = r.projectiles.by_id("grenade-shell").unwrap();
    assert_eq!((shell.life - shell.arm).to_bits(), 1.65f32.to_bits());
}

#[test]
fn base_entries_keep_their_wire_numbers() {
    let r = base();
    let kinds = [("disc", 0), ("bullet", 1), ("grenade-shell", 2), ("turret-plasma", 3), ("mortar-shell", 5), ("hand-grenade", 6),
        ("mine", 7), ("laser", 8), ("rail-slug", 9), ("plasma-bolt", 10), ("blaster-bolt", 11)];
    for (id, n) in kinds { assert_eq!(r.projectiles.index(id), Some(n), "{id}"); }
    assert!(r.projectiles.get(4).is_none(), "a retired number stays open");
    for (i, id) in ["disc", "chaingun", "grenade_launcher", "laser", "plasma", "blaster"].iter().enumerate() {
        assert_eq!(r.weapons.index(id), Some(i), "a light armor's weapons keep the old kind numbers");
    }
    assert_eq!((r.armors.index("light"), r.armors.index("heavy")), (Some(0), Some(1)));
    for (i, id) in ["turret", "wall", "field", "ammo", "inventory"].iter().enumerate() { assert_eq!(r.deployables.index(id), Some(i)); }
}

#[test]
fn older_mods_play_exactly_as_they_did() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/mods");
    let mut patches: Vec<serde_json::Value> = ["low-gravity", "sidearms"].iter()
        .map(|m| serde_json::from_str(&std::fs::read_to_string(examples.join(m).join("defs.json")).unwrap()).unwrap()).collect();
    // Every old path at least once.
    patches.push(json!({
        "movement": {"gravity": 12.0}, "repair": {"range": 15.0}, "equipment": {"generator_hull": 600.0},
        "armors": {
            "light": {"movement": {"jet": 40.0}, "walk_scale": 1.2, "ammo": [20, 120, 12, 0], "plasma_ammo": 40, "weapon_slots": 4, "throwables": [6, 4]},
            "heavy": {"ammo": [30, 250, 12, 25], "damage_taken": 0.4, "mass_scale": 2.5},
            "football": {"jet": 50.0}
        },
        "weapons": {
            "disc": {"reload": 0.8, "speed": 100.0, "self_damage": 0.5, "spread": 1.0, "recoil": 0.01, "shake": 0.1, "inherit": 0.6, "life": 4.0},
            "chaingun": {"reload": 0.06}, "grenade_launcher": {"life": 2.5}, "mortar": {"arm": 0.5}, "plasma": {"speed": 80.0},
            "laser": {"energy": 20.0, "fade": 0.3, "range": 2000.0, "damage": 70.0, "reload": 1.0},
            "railgun": {"damage": 90.0, "speed": 700.0, "inherit": 0.2, "reload": 1.5, "life": 2.5},
            "blaster": {"bounce": 0.6, "energy": 8.0, "speed": 200.0, "inherit": 0.4, "life": 1.2, "reload": 0.25, "damage": 14.0}
        },
        "blasts": {"disc": {"core": 2.0}, "hand_grenade": {"radius": 12.0}, "mortar_structure": {"impulse": 3000.0}, "grenade_structure": {"max_damage": 80.0},
            "grenade": {"edge_fraction": 0.2}, "mortar": {"radius": 22.0}, "mine": {"impulse": 2500.0}, "plasma": {"max_damage": 40.0}},
        "throwables": {"grenade_throw": 22.0, "mine_throw": 26.0, "inherit": 0.9, "time": 0.2, "min_strength": 0.25, "grenade_fuse": 2.5,
            "mine_flight": 7.0, "mine_life": 600.0, "mine_arm": 1.5, "mine_trigger": 3.0, "mine_team_max": 20, "mine_sympathy": 5.0, "mine_shot_radius": 0.4},
        "deployables": {"range": 15.0, "turret": {"range": 90.0, "projectile": 3}, "health": {"wall": 500.0, "inventory": 350.0},
            "team_limit": {"turret": 6, "ammo": 4}}
    }));
    for patch in &patches {
        // The old way: merge onto the old base file and read it.
        let mut old: serde_json::Value = serde_json::from_str(OLD_BASE).unwrap();
        crate::defs::merge(&mut old, patch);
        let mut old: crate::defs::Defs = serde_json::from_value(old).unwrap();
        if patch.pointer("/weapons/grenade_launcher/arm").is_none() { old.weapons.grenade_launcher.arm = 0.35; }
        if patch.pointer("/armors/heavy/weapon_slots").is_none() { old.armors.heavy.weapon_slots = 5; }
        if patch.pointer("/weapons/railgun/speed").is_none() { old.weapons.railgun.speed = 600.0; }
        if patch.pointer("/weapons/railgun/damage").is_none() { old.weapons.railgun.damage = 126.67; }
        let new = with(&[Source { name: "old", defs: Some(patch), files: Vec::new() }]).unwrap();
        assert_eq!(bits(new.legacy()), bits(&old), "{patch}");
    }
}

#[test]
fn a_mod_patches_extends_replaces_and_removes_by_id() {
    let mod_files = files(&[
        ("weapons/tuning.json", json!({
            "disc": {"reload": 0.5},
            // Built from another entry, in any order within the package.
            "spinner": {"extends": "rapid-disc", "name": "Spinner", "reload": 0.3},
            "rapid-disc": {"extends": "disc", "name": "Rapid disc", "reload": 0.4},
            "blaster": {"replace": true, "name": "Pea shooter", "family": "blaster", "projectile": "bullet", "reload": 0.2,
                "view": "blaster", "sound": "blaster"}
        })),
        ("armors/changes.json", json!({
            "light": {"weapons": {"blaster": null, "rapid-disc": 12}},
            "trooper": {"name": "Trooper armor", "movement": {"jet": 30.0, "side": 15.0, "drain": 13.0, "regen": 9.0, "min_jet": 3.0,
                "jump": 1.0, "fade_up": true, "air_cap": 15.0, "ski_drag": 0.0}, "walk_scale": 0.8, "damage_taken": 0.7, "mass_scale": 1.5,
                "weapon_slots": 3, "weapons": {"disc": 20}}
        })),
    ]);
    let r = with(&[Source { name: "m", defs: None, files: mod_files }]).unwrap();
    let disc = r.weapons.by_id("disc").unwrap();
    assert_eq!((disc.reload, disc.name.as_str(), disc.projectile.as_str()), (0.5, "Disc launcher", "disc"), "only the named field changed");
    assert_eq!(r.legacy().weapons.disc.reload, 0.5, "the sim sees it");
    let rapid = r.weapons.by_id("rapid-disc").unwrap();
    assert_eq!((rapid.reload, rapid.projectile.as_str(), rapid.sound.as_str()), (0.4, "disc", "disc"));
    assert_eq!(r.weapons.by_id("spinner").unwrap().reload, 0.3);
    let blaster = r.weapons.by_id("blaster").unwrap();
    assert_eq!((blaster.energy, blaster.shake, blaster.projectile.as_str()), (0.0, 0.0, "bullet"), "replaced: nothing of the old one left");
    let light = r.armors.by_id("light").unwrap();
    assert!(!light.weapons.contains_key("blaster") && light.weapons["rapid-disc"] == 12);
    // New entries take numbers after the base game's, sorted by id.
    let n = base().weapons.len();
    assert_eq!(r.weapons.index("blaster"), Some(5), "a replaced entry keeps its number");
    assert_eq!((r.weapons.index("rapid-disc"), r.weapons.index("spinner")), (Some(n), Some(n + 1)));
    assert_eq!(r.armors.index("trooper"), Some(4));
    let summary = changes(&with(&[Source { name: "m", defs: None, files: files(&[("weapons/a.json", json!({"disc": {"reload": 0.5}, "zap": {"extends": "disc"}}))]) }]).unwrap());
    assert_eq!(summary, vec!["adds weapons zap".to_string(), "changes weapons disc".to_string()]);
}

#[test]
fn later_mods_layer_over_earlier_ones_and_number_after_them() {
    let a = Source { name: "a", defs: None, files: files(&[("weapons/w.json", json!({"zz": {"extends": "disc"}, "disc": {"reload": 0.9}}))]) };
    let b = Source { name: "b", defs: None, files: files(&[("weapons/w.json", json!({"aa": {"extends": "zz"}, "disc": {"reload": 0.7}}))]) };
    let r = with(&[a, b]).unwrap();
    assert_eq!(r.weapons.by_id("disc").unwrap().reload, 0.7, "the later mod wins");
    assert_eq!(r.weapons.by_id("zz").unwrap().reload, 0.9, "copied as its own package left it, before the second mod's patch");
    assert_eq!(r.weapons.by_id("aa").unwrap().reload, 0.9);
    let n = base().weapons.len();
    assert_eq!((r.weapons.index("zz"), r.weapons.index("aa")), (Some(n), Some(n + 1)), "package order first, then ids");
}

#[test]
fn bad_entries_say_where_and_why() {
    let cases: Vec<(serde_json::Value, &str, &str)> = vec![
        (json!({"disc": {"reloda": 1.0}}), "weapons/x.json", "reloda"),
        (json!({"disc": {"reload": 0.0}}), "weapons/x.json", "weapons/disc: reload must be 0.016 to 30"),
        (json!({"disc": {"projectile": "nope"}}), "weapons/x.json", "weapons/disc.projectile: no projectile \"nope\""),
        (json!({"new-gun": {"name": "New"}}), "weapons/x.json", "weapons/new-gun: missing field"),
        (json!({"a": {"extends": "b"}, "b": {"extends": "a"}}), "weapons/x.json", "circle"),
        (json!({"a": {"extends": "nothing"}}), "weapons/x.json", "which no package defines"),
        (json!({"Bad Id": {"extends": "disc"}}), "weapons/x.json", "isn't an id"),
        (json!({"disc": {"projectile": "mine"}}), "weapons/x.json", "thrown, not fired"),
        (json!({"laser": {"range": 0.0}}), "projectiles/x.json", "a beam needs a range"),
        (json!({"disc": {"blast": null}}), "projectiles/x.json", "needs a blast"),
        (json!({"light": {"weapons": {"nope": 3}}}), "armors/x.json", "armors/light.weapons: no weapon \"nope\""),
        (json!({"light": {"movement": {"jet": 900.0}}}), "armors/x.json", "movement.jet must be 0 to 200"),
        (json!({"light": {"name": "A\u{202e}B"}}), "armors/x.json", "plain text"),
        (json!({"mine": {"team_max": 9999}}), "throwables/x.json", "team_max must be a whole number 0 to 500"),
        (json!({"turret": {"turret": null}}), "deployables/x.json", "a turret needs"),
        (json!({"disc": {"radius": -1.0}}), "blasts/x.json", "radius must be 0 to 200"),
        (json!({"disc": {"reload": 0.5}}), "gadgets/x.json", "not a class folder"),
    ];
    for (entries, path, why) in cases {
        let e = with(&[Source { name: "m", defs: None, files: files(&[(path, entries.clone())]) }]).unwrap_err();
        assert!(e.contains(why), "{entries}: {e}");
    }
    let twice = with(&[Source { name: "m", defs: None, files: files(&[("weapons/a.json", json!({"disc": {"reload": 0.5}})),
        ("weapons/b.json", json!({"disc": {"reload": 0.6}}))]) }]).unwrap_err();
    assert!(twice.contains("defined twice"), "{twice}");
}

#[test]
fn every_field_has_a_range_and_a_line_about_it() {
    // Every field a class's struct has (Serialize writes them all) is in its
    // table, so the loader checks it and --schema documents it.
    fn paths(v: &serde_json::Value, prefix: &str, rows: &[&fields::Field], out: &mut Vec<String>) {
        let Some(map) = v.as_object() else { return };
        let parent = rows.iter().find(|r| r.path == prefix.trim_end_matches('.')).map(|r| r.kind);
        for (k, v) in map {
            if v.is_null() { continue; }
            let path = if matches!(parent, Some(fields::Kind::Map(_) | fields::Kind::Named)) { format!("{prefix}*") } else { format!("{prefix}{k}") };
            out.push(path.clone());
            if v.is_object() { paths(v, &format!("{path}."), rows, out); }
        }
    }
    let r = base();
    let mut seen: Vec<(Class, String)> = Vec::new();
    let mut check = |class: Class, v: serde_json::Value| {
        let rows = fields::table(class);
        let mut found = Vec::new();
        paths(&v, "", &rows, &mut found);
        for p in found {
            let path = p;
            assert!(rows.iter().any(|r| r.path == path), "{} has no row for {path}", class.folder());
            seen.push((class, path));
        }
    };
    for (_, _, a) in r.armors.iter() { check(Class::Armor, serde_json::to_value(a).unwrap()); }
    for (_, _, w) in r.weapons.iter() { check(Class::Weapon, serde_json::to_value(w).unwrap()); }
    for (_, _, p) in r.projectiles.iter() { check(Class::Projectile, serde_json::to_value(p).unwrap()); }
    for (_, _, b) in r.blasts.iter() { check(Class::Blast, serde_json::to_value(b).unwrap()); }
    for (_, _, t) in r.throwables.iter() { check(Class::Throwable, serde_json::to_value(t).unwrap()); }
    for (_, _, d) in r.deployables.iter() { check(Class::Deployable, serde_json::to_value(d).unwrap()); }
    for (_, _, d) in r.damage.iter() { check(Class::Damage, serde_json::to_value(d).unwrap()); }
    for (_, _, e) in r.effects.iter() { check(Class::Effect, serde_json::to_value(e).unwrap()); }
    for (_, _, p) in r.packs.iter() { check(Class::Pack, serde_json::to_value(p).unwrap()); }
    for (_, _, t) in r.items.iter() { check(Class::Item, serde_json::to_value(t).unwrap()); }
    for (_, _, v) in r.vehicles.iter() { check(Class::Vehicle, serde_json::to_value(v).unwrap()); }
    // And every row names a real field: written into a base entry, the
    // engine's own parser accepts it.
    fn sample(kind: fields::Kind, r: &Registry) -> serde_json::Value {
        use fields::Kind::*;
        match kind {
            Num(lo, _) => json!(lo), Int(lo, _) => json!(lo), Bool => json!(true), Text(_) | Key => json!("x"),
            Ref(c) => json!(match c { Class::Armor => "light", Class::Weapon => "disc", Class::Projectile => "disc", Class::Blast => "disc",
                Class::Throwable => "mine", Class::Deployable => "wall", Class::Damage => r.damage.id(0).unwrap(),
                Class::Effect => "shielded", Class::Pack => "wall", Class::Item => "repair-kit", Class::Vehicle => "skiff" }),
            Tint => json!([1.0, 0.0, 0.0, 0.5]), Pair(lo, _) => json!([lo, lo]), Pair3(lo, _) => json!([lo, lo, lo]), Objects(_) => json!([]), Choice(words) => json!(words[0]), Choices(words) => json!([words[0]]), Object | Map(_) | Named => json!({}), List(_, _) | Variants => json!([]),
        }
    }
    /// An object with every field its rows name.
    fn built(rows: &[&fields::Field], path: &str, r: &Registry) -> serde_json::Value {
        let mut o = serde_json::Map::new();
        for row in rows {
            let Some(rest) = row.path.strip_prefix(path).and_then(|x| x.strip_prefix('.')) else { continue };
            if rest.contains('.') || rest == "*" { continue; }
            let v = if matches!(row.kind, fields::Kind::Object) { built(rows, row.path, r) } else { sample(row.kind, r) };
            o.insert(rest.to_string(), v);
        }
        serde_json::Value::Object(o)
    }
    fn parses(class: Class, v: serde_json::Value) -> Result<(), String> {
        let e = |e: serde_json::Error| e.to_string();
        match class {
            Class::Armor => serde_json::from_value::<Armor>(v).map(|_| ()).map_err(e),
            Class::Weapon => serde_json::from_value::<Weapon>(v).map(|_| ()).map_err(e),
            Class::Projectile => serde_json::from_value::<Projectile>(v).map(|_| ()).map_err(e),
            Class::Blast => serde_json::from_value::<Blast>(v).map(|_| ()).map_err(e),
            Class::Throwable => serde_json::from_value::<Throwable>(v).map(|_| ()).map_err(e),
            Class::Deployable => serde_json::from_value::<Deployable>(v).map(|_| ()).map_err(e),
            Class::Damage => serde_json::from_value::<Damage>(v).map(|_| ()).map_err(e),
            Class::Effect => serde_json::from_value::<Effect>(v).map(|_| ()).map_err(e),
            Class::Pack => serde_json::from_value::<Pack>(v).map(|_| ()).map_err(e),
            Class::Item => serde_json::from_value::<Item>(v).map(|_| ()).map_err(e),
            Class::Vehicle => serde_json::from_value::<Vehicle>(v).map(|_| ()).map_err(e),
        }
    }
    let first = |class: Class| -> serde_json::Value {
        match class {
            Class::Armor => serde_json::to_value(r.armors.by_id("light").unwrap()), Class::Weapon => serde_json::to_value(r.weapons.by_id("disc").unwrap()),
            Class::Projectile => serde_json::to_value(r.projectiles.by_id("mine").unwrap()), Class::Blast => serde_json::to_value(r.blasts.by_id("disc").unwrap()),
            Class::Throwable => serde_json::to_value(r.throwables.by_id("mine").unwrap()), Class::Deployable => serde_json::to_value(r.deployables.by_id("turret").unwrap()),
            Class::Damage => serde_json::to_value(r.damage.get(0).unwrap()),
            Class::Effect => serde_json::to_value(r.effects.by_id("shielded").unwrap()),
            Class::Pack => serde_json::to_value(r.packs.by_id("wall").unwrap()),
            Class::Item => serde_json::to_value(r.items.by_id("repair-kit").unwrap()),
            Class::Vehicle => serde_json::to_value(r.vehicles.by_id("skiff").unwrap()),
        }.unwrap()
    };
    for class in Class::ALL {
        let rows = fields::table(class);
        for row in &rows {
            assert!(!row.doc.is_empty(), "{}: {}", class.folder(), row.path);
            let mut v = first(class);
            let mut at = &mut v;
            let parts: Vec<&str> = row.path.split('.').collect();
            for (n, part) in parts.iter().enumerate() {
                let key = if *part == "*" { match rows.iter().find(|r| r.path == parts[..n].join(".")).map(|r| r.kind) {
                    Some(fields::Kind::Map(c)) => sample(fields::Kind::Ref(c), r).as_str().unwrap().to_string(), _ => "x".into() } } else { part.to_string() };
                let obj = at.as_object_mut().unwrap();
                if n + 1 == parts.len() {
                    // An object that's there already stays (its fields are rows of their own);
                    // a missing one is built from its fields' rows.
                    let keep = matches!(row.kind, fields::Kind::Object | fields::Kind::Map(_) | fields::Kind::Named) && obj.get(&key).is_some_and(|v| v.is_object());
                    if !keep {
                        let value = if matches!(row.kind, fields::Kind::Object) { built(&rows, row.path, r) } else { sample(row.kind, r) };
                        obj.insert(key, value);
                    }
                    break;
                }
                let prefix = parts[..=n].join(".");
                let next = obj.entry(key).or_insert_with(|| if rows.iter().any(|x| x.path == prefix && matches!(x.kind, fields::Kind::Object)) {
                    built(&rows, &prefix, r) } else { json!({}) });
                if !next.is_object() {
                    *next = if rows.iter().any(|x| x.path == prefix && matches!(x.kind, fields::Kind::Object)) { built(&rows, &prefix, r) } else { json!({}) };
                }
                at = next;
            }
            // A list of objects was built as an object of samples: make it a list.
            fn listify(v: &mut serde_json::Value, segs: &[&str]) {
                let Some((first, rest)) = segs.split_first() else { return };
                let Some(obj) = v.as_object_mut() else { return };
                if rest.is_empty() {
                    if let Some(x) = obj.get_mut(*first).filter(|x| x.is_object()) {
                        *x = serde_json::Value::Array(x.as_object().unwrap().values().cloned().collect());
                    }
                    return;
                }
                let children: Vec<&mut serde_json::Value> = if *first == "*" { obj.values_mut().collect() } else { obj.get_mut(*first).into_iter().collect() };
                for c in children {
                    if let Some(a) = c.as_array_mut() { for e in a { listify(e, rest); } } else { listify(c, rest); }
                }
            }
            for list in rows.iter().filter(|r| matches!(r.kind, fields::Kind::Objects(_))) {
                listify(&mut v, &list.path.split('.').collect::<Vec<_>>());
            }
            parses(class, v).unwrap_or_else(|e| panic!("{}: row {} names no field: {e}", class.folder(), row.path));
        }
    }
    assert!(fields::markdown().contains("| `movement.jet` | 0 to 200 |"));
}

/// `docs/mod-format.md`'s field reference is what `--schema` prints. Write it
/// again with `PEAKRUNNER_SCHEMA_BLESS=1`.
#[test]
fn the_documented_reference_is_current() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/mod-format.md");
    let Ok(doc) = std::fs::read_to_string(&path) else { return }; // builds from src/ alone have no docs
    let (start, end) = ("<!-- schema:start -->\n", "<!-- schema:end -->");
    let (Some(a), Some(b)) = (doc.find(start), doc.find(end)) else { panic!("docs/mod-format.md lost its schema markers") };
    let current = format!("{}{}{}", &doc[..a + start.len()], fields::markdown(), &doc[b..]);
    if std::env::var_os("PEAKRUNNER_SCHEMA_BLESS").is_some() { std::fs::write(&path, &current).unwrap(); return; }
    assert!(doc == current, "docs/mod-format.md's reference is stale: run PEAKRUNNER_SCHEMA_BLESS=1 cargo test -p peakrunner-core --lib registry");
}
