//! Older definitions, both ways:
//! - `split` reads a mod's `defs.json` written before the classes, where
//!   items sat at fixed paths (`weapons.disc.reload`,
//!   `armors.heavy.ammo`), and turns those into patches of the entries they
//!   meant, so every older mod plays as it did;
//! - `defs_view` builds the old `Defs` shape from a registry, for the sim
//!   code that still reads it.
use super::{Class, Registry};
use crate::defs::{ArmorDef, Armors, Blaster, Blasts, Defs, Deployables, Launcher, Laser, PerDeployable, Railgun, Throwables, Weapons};
use serde_json::{json, Map, Value};

/// A patch for one entry.
type Patch = (Class, String, Value);

/// Every number in an older `defs.json` is finite and 0 to 1000000.
fn numbers(v: &Value, at: &str) -> Result<(), String> {
    match v {
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(f64::NAN);
            if !x.is_finite() || !(0.0..=1e6).contains(&x) { return Err(format!("{at}: numbers must be 0 to 1000000")); }
        }
        Value::Object(o) => for (k, v) in o { numbers(v, &format!("{at}.{k}"))?; },
        Value::Array(a) => for (i, v) in a.iter().enumerate() { numbers(v, &format!("{at}[{i}]"))?; },
        _ => {}
    }
    Ok(())
}

fn object<'a>(v: &'a Value, at: &str) -> Result<&'a Map<String, Value>, String> {
    v.as_object().ok_or_else(|| format!("defs.json: {at} must be an object"))
}

/// An entry patch from some of `src`'s fields, renamed: `(from, to)`.
fn pick(src: &Map<String, Value>, names: &[(&str, &str)]) -> Value {
    let mut out = Map::new();
    for (from, to) in names {
        if let Some(v) = src.get(*from) { out.insert(to.to_string(), v.clone()); }
    }
    Value::Object(out)
}

fn has_any(v: &Value) -> bool { v.as_object().is_some_and(|o| !o.is_empty()) }

/// Split an older-style `defs.json` into the world's part (movement, the
/// repair tool, map equipment, placement) and patches for entries.
pub fn split(defs: &Value) -> Result<(Value, Vec<Patch>), String> {
    numbers(defs, "defs")?;
    let top = object(defs, "the file")?;
    let mut world = Map::new();
    let mut out: Vec<Patch> = Vec::new();
    let mut push = |class: Class, id: &str, v: Value| if has_any(&v) { out.push((class, id.to_string(), v)); };
    for (key, v) in top {
        match key.as_str() {
            "movement" | "repair" | "equipment" | "placement" | "blade" => { world.insert(key.clone(), v.clone()); }
            "armors" => for (armor, a) in object(v, "armors")? {
                if armor == "football" { push(Class::Armor, "football", json!({ "movement": a })); continue; }
                let heavy = match armor.as_str() { "light" => false, "heavy" => true, other => return Err(format!("defs.json: unknown field armors.{other}")) };
                let a = object(a, &format!("armors.{armor}"))?;
                for k in a.keys() {
                    if !["movement", "walk_scale", "damage_taken", "mass_scale", "ammo", "plasma_ammo", "weapon_slots", "throwables"].contains(&k.as_str()) {
                        return Err(format!("defs.json: unknown field armors.{armor}.{k}"));
                    }
                }
                let mut patch = pick(a, &[("movement", "movement"), ("walk_scale", "walk_scale"), ("damage_taken", "damage_taken"),
                    ("mass_scale", "mass_scale"), ("weapon_slots", "weapon_slots")]);
                let p = patch.as_object_mut().expect("object");
                let mut weapons = Map::new();
                if let Some(ammo) = a.get("ammo") {
                    let ammo = ammo.as_array().filter(|x| x.len() == 4)
                        .ok_or_else(|| format!("defs.json: armors.{armor}.ammo needs 4 numbers (disc, chaingun, launcher, rifle)"))?;
                    let names = if heavy { ["disc", "chaingun", "mortar", "railgun"] } else { ["disc", "chaingun", "grenade_launcher", "laser"] };
                    for (n, v) in names.iter().zip(ammo) { weapons.insert(n.to_string(), v.clone()); }
                }
                if let Some(v) = a.get("plasma_ammo") { weapons.insert("plasma".into(), v.clone()); }
                if !weapons.is_empty() { p.insert("weapons".into(), Value::Object(weapons)); }
                if let Some(t) = a.get("throwables") {
                    let t = t.as_array().filter(|x| x.len() == 2)
                        .ok_or_else(|| format!("defs.json: armors.{armor}.throwables needs 2 numbers (grenades, mines)"))?;
                    p.insert("grenade".into(), json!({ "count": t[0] }));
                    p.insert("mine".into(), json!({ "count": t[1] }));
                }
                push(Class::Armor, armor, patch);
            },
            "weapons" => for (weapon, w) in object(v, "weapons")? {
                let w = object(w, &format!("weapons.{weapon}"))?;
                let (projectile, weapon_fields, projectile_fields): (&str, &[(&str, &str)], &[(&str, &str)]) = match weapon.as_str() {
                    "disc" | "chaingun" | "grenade_launcher" | "mortar" | "plasma" => (
                        match weapon.as_str() { "disc" => "disc", "chaingun" => "bullet", "grenade_launcher" => "grenade-shell", "mortar" => "mortar-shell", _ => "plasma-bolt" },
                        &[("reload", "reload"), ("spread", "spread"), ("recoil", "recoil"), ("shake", "shake")],
                        &[("speed", "speed"), ("inherit", "inherit"), ("life", "life"), ("arm", "arm"), ("self_damage", "self_damage")]),
                    "laser" => ("laser", &[("energy", "energy"), ("reload", "reload")], &[("damage", "damage"), ("range", "range"), ("fade", "life")]),
                    "railgun" => ("rail-slug", &[("reload", "reload")], &[("speed", "speed"), ("inherit", "inherit"), ("damage", "damage"), ("life", "life")]),
                    "blaster" => ("blaster-bolt", &[("reload", "reload"), ("energy", "energy")],
                        &[("speed", "speed"), ("inherit", "inherit"), ("life", "life"), ("damage", "damage"), ("bounce", "keep")]),
                    other => return Err(format!("defs.json: unknown field weapons.{other}")),
                };
                if let Some(k) = w.keys().find(|k| !weapon_fields.iter().chain(projectile_fields).any(|(f, _)| f == k)) {
                    return Err(format!("defs.json: unknown field weapons.{weapon}.{k}"));
                }
                push(Class::Weapon, weapon, pick(w, weapon_fields));
                push(Class::Projectile, projectile, pick(w, projectile_fields));
            },
            "blasts" => for (blast, b) in object(v, "blasts")? {
                let id = match blast.as_str() {
                    "disc" | "grenade" | "mortar" | "mine" | "plasma" => blast.clone(),
                    "hand_grenade" | "grenade_structure" | "mortar_structure" => blast.replace('_', "-"),
                    other => return Err(format!("defs.json: unknown field blasts.{other}")),
                };
                let b = object(b, &format!("blasts.{blast}"))?;
                if let Some(k) = b.keys().find(|k| !["radius", "max_damage", "edge_fraction", "impulse", "core"].contains(&k.as_str())) {
                    return Err(format!("defs.json: unknown field blasts.{blast}.{k}"));
                }
                push(Class::Blast, &id, Value::Object(b.clone()));
            },
            "throwables" => {
                let t = object(v, "throwables")?;
                let mut grenade = Map::new();
                let mut mine = Map::new();
                let mut grenade_round = Map::new();
                let mut mine_round = Map::new();
                let mut trigger = Map::new();
                for (k, v) in t {
                    match k.as_str() {
                        "grenade_throw" => { grenade.insert("speed".into(), v.clone()); }
                        "mine_throw" => { mine.insert("speed".into(), v.clone()); }
                        "inherit" | "time" | "min_strength" => { grenade.insert(k.clone(), v.clone()); mine.insert(k.clone(), v.clone()); }
                        "mine_team_max" => { mine.insert("team_max".into(), v.clone()); }
                        "grenade_fuse" => { grenade_round.insert("life".into(), v.clone()); }
                        "mine_flight" => { mine_round.insert("life".into(), v.clone()); }
                        "mine_life" => { trigger.insert("rest_life".into(), v.clone()); }
                        "mine_arm" => { trigger.insert("arm".into(), v.clone()); }
                        "mine_trigger" => { trigger.insert("radius".into(), v.clone()); }
                        "mine_sympathy" => { trigger.insert("sympathy".into(), v.clone()); }
                        "mine_shot_radius" => { trigger.insert("shot_radius".into(), v.clone()); }
                        other => return Err(format!("defs.json: unknown field throwables.{other}")),
                    }
                }
                if !trigger.is_empty() { mine_round.insert("trigger".into(), Value::Object(trigger)); }
                push(Class::Throwable, "hand-grenade", Value::Object(grenade));
                push(Class::Throwable, "mine", Value::Object(mine));
                push(Class::Projectile, "hand-grenade", Value::Object(grenade_round));
                push(Class::Projectile, "mine", Value::Object(mine_round));
            }
            "deployables" => for (k, v) in object(v, "deployables")? {
                match k.as_str() {
                    "range" => { world.insert("placement".into(), json!({ "range": v })); }
                    "turret" => {
                        let mut t = object(v, "deployables.turret")?.clone();
                        if let Some(n) = t.get("projectile").and_then(Value::as_u64) {
                            let id = Class::Projectile.fixed().get(n as usize).filter(|id| !id.is_empty())
                                .ok_or_else(|| format!("defs.json: deployables.turret.projectile {n} isn't a projectile"))?;
                            t.insert("projectile".into(), Value::String(id.to_string()));
                        }
                        push(Class::Deployable, "turret", json!({ "turret": t }));
                    }
                    "health" | "team_limit" => for (kind, n) in object(v, &format!("deployables.{k}"))? {
                        if !["turret", "wall", "field", "ammo", "inventory"].contains(&kind.as_str()) {
                            return Err(format!("defs.json: unknown field deployables.{k}.{kind}"));
                        }
                        push(Class::Deployable, kind, json!({ if k == "health" { "hull" } else { "team_limit" }: n }));
                    },
                    other => return Err(format!("defs.json: unknown field deployables.{other}")),
                }
            },
            other => return Err(format!("defs.json: unknown field {other}")),
        }
    }
    Ok((Value::Object(world), out))
}

/// The old `Defs` shape, read from a registry.
pub fn defs_view(r: &Registry) -> Result<Defs, String> {
    let missing = |what: &str| format!("the base game's {what} is missing");
    let armor = |id: &str| -> Result<ArmorDef, String> {
        let a = r.armors.by_id(id).ok_or_else(|| missing(id))?;
        // The rounds of the armor's weapon in each of the old fixed slots.
        let rounds = |family: &str| a.weapons.iter()
            .find(|(w, _)| r.weapons.by_id(w).is_some_and(|w| w.family == family)).map_or(0, |(_, n)| *n);
        Ok(ArmorDef {
            movement: a.movement, walk_scale: a.walk_scale, damage_taken: a.damage_taken, mass_scale: a.mass_scale,
            ammo: [rounds("disc"), rounds("chaingun"), rounds("launcher"), rounds("rifle")],
            plasma_ammo: rounds("plasma"), weapon_slots: a.weapon_slots,
            throwables: [a.grenade.as_ref().map_or(0, |c| c.count), a.mine.as_ref().map_or(0, |c| c.count)],
        })
    };
    let weapon = |id: &str| r.weapons.by_id(id).ok_or_else(|| missing(id));
    let round = |id: &str| r.projectiles.by_id(id).ok_or_else(|| missing(id));
    let launcher = |w: &str| -> Result<Launcher, String> {
        let wd = weapon(w)?;
        let p = round(&wd.projectile)?;
        Ok(Launcher { speed: p.speed, inherit: p.inherit, life: p.life, reload: wd.reload, spread: wd.spread, recoil: wd.recoil,
            shake: wd.shake, self_damage: p.self_damage, arm: p.arm })
    };
    let blast = |id: &str| r.blasts.by_id(id).map(|b| b.profile()).ok_or_else(|| missing(id));
    let deploy = |id: &str| r.deployables.by_id(id).ok_or_else(|| missing(id));
    let laser = (weapon("laser")?, round("laser")?);
    let rail = (weapon("railgun")?, round("rail-slug")?);
    let blaster = (weapon("blaster")?, round("blaster-bolt")?);
    let grenade = (r.throwables.by_id("hand-grenade").ok_or_else(|| missing("hand-grenade"))?, round("hand-grenade")?);
    let mine = (r.throwables.by_id("mine").ok_or_else(|| missing("mine"))?, round("mine")?);
    let trigger = mine.1.trigger.ok_or_else(|| missing("mine trigger"))?;
    let turret = deploy("turret")?.turret.as_ref().ok_or_else(|| missing("turret profile"))?;
    let per = |f: &dyn Fn(&super::Deployable) -> f32| -> Result<PerDeployable<f32>, String> {
        Ok(PerDeployable { turret: f(deploy("turret")?), wall: f(deploy("wall")?), field: f(deploy("field")?),
            ammo: f(deploy("ammo")?), inventory: f(deploy("inventory")?) })
    };
    let limit = per(&|d| d.team_limit as f32)?;
    Ok(Defs {
        movement: r.globals.movement,
        armors: Armors { light: armor("light")?, heavy: armor("heavy")?, football: r.armors.by_id("football").ok_or_else(|| missing("football"))?.movement },
        weapons: Weapons {
            disc: launcher("disc")?, chaingun: launcher("chaingun")?, grenade_launcher: launcher("grenade_launcher")?,
            mortar: launcher("mortar")?, plasma: launcher("plasma")?,
            laser: Laser { energy: laser.0.energy, damage: laser.1.damage, reload: laser.0.reload, range: laser.1.range, fade: laser.1.life },
            railgun: Railgun { speed: rail.1.speed, inherit: rail.1.inherit, damage: rail.1.damage, reload: rail.0.reload, life: rail.1.life },
            blaster: Blaster { speed: blaster.1.speed, inherit: blaster.1.inherit, life: blaster.1.life, reload: blaster.0.reload,
                energy: blaster.0.energy, damage: blaster.1.damage, bounce: blaster.1.keep },
        },
        blasts: Blasts {
            disc: blast("disc")?, grenade: blast("grenade")?, mortar: blast("mortar")?, hand_grenade: blast("hand-grenade")?,
            mine: blast("mine")?, plasma: blast("plasma")?, grenade_structure: blast("grenade-structure")?, mortar_structure: blast("mortar-structure")?,
        },
        throwables: Throwables {
            grenade_throw: grenade.0.speed, mine_throw: mine.0.speed, inherit: grenade.0.inherit, time: grenade.0.time,
            min_strength: grenade.0.min_strength, grenade_fuse: grenade.1.life, mine_flight: mine.1.life,
            mine_life: trigger.rest_life, mine_arm: trigger.arm, mine_trigger: trigger.radius,
            mine_team_max: mine.0.team_max as usize, mine_sympathy: trigger.sympathy, mine_shot_radius: trigger.shot_radius,
        },
        repair: r.globals.repair,
        equipment: r.globals.equipment,
        deployables: Deployables {
            turret: crate::equipment::TurretProfile { range: turret.range, sensed_range: turret.sensed_range, speed: turret.speed,
                life: turret.life, cooldown: turret.cooldown, leads: turret.leads, fires: turret.fires,
                projectile: r.projectile_kind(&turret.projectile), jetting_only: false },
            health: per(&|d| d.hull)?,
            team_limit: PerDeployable { turret: limit.turret as usize, wall: limit.wall as usize, field: limit.field as usize,
                ammo: limit.ammo as usize, inventory: limit.inventory as usize },
            range: r.globals.placement.range,
        },
    })
}
