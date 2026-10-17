//! What armors do beyond moving and carrying (docs/mod-format.md): a class
//! key used for a shove that builds strain until too much hurts, one
//! switched on to hold an effect, touches that drain enemies and mend
//! teammates, a plate that stops shots from the front, and a shoulder gun
//! that fires by itself.
//! Its own test binary: activating a mod changes the process's definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{ArmorClass, Team, World, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::{catalog, mods, registry};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn step(w: &mut World) { w.tick(STEP); }

/// Player 0 in `armor` on open ground, and another player `ahead` metres in
/// front of them on `team`.
fn pair(armor: ArmorClass, ahead: f32, team: Team) -> World {
    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    let ground = |x: f32, z: f32| Vec3::new(x, peakrunner_core::terrain::support_on(map, Vec3::new(x, 400.0, z)).0 + PLAYER_RADIUS, z);
    w.players[0].team = Team::Ember;
    w.players[0].armor = armor;
    w.players[0].pos = ground(1060.0, 700.0);
    w.players[0].vel = Vec3::ZERO;
    w.players[0].yaw = 0.0;
    w.players[0].pitch = 0.0;
    w.players[0].energy = 60.0;
    let mut other = w.players[0].clone();
    other.team = team;
    other.armor = ArmorClass::Light;
    other.pos = ground(1060.0, 700.0 - ahead);
    w.players.push(other);
    w
}

#[test]
fn class_keys_touches_and_mounts_work_in_play() {
    let root = std::env::temp_dir().join(format!("peakrunner-abilities-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/classkit";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Class kit", "note": "Armors that do things.", "api": 1}"#);
    write(&root, &format!("{m}/effects/strain.json"), r#"
        // Pushed too hard: it hurts, and the jets won't light.
        { "overstrained": { "name": "Overstrained", "seconds": 2.0, "damage": 10.0, "blocks": { "jet": true } } }"#);
    write(&root, &format!("{m}/armors/classes.json"), r#"{
        "dasher": { "extends": "light", "name": "Dasher",
            "ability": { "name": "Dash", "use": "use", "energy": 10.0, "cooldown": 1.0, "action": { "impulse": [14.0, 4.0] },
                "strain": 30.0, "strain_decay": 10.0, "strain_limit": 50.0, "strain_effect": "overstrained" },
            "touch": { "steal_energy": 0.5, "damage": 5.0, "cooldown": 1.0 } },
        "medic": { "extends": "light", "name": "Medic",
            "ability": { "name": "Mend", "use": "toggle", "effects": ["mending"], "drain": 4.0, "min_energy": 2.0 },
            "touch": { "heal": 20.0, "energy": 10.0, "cooldown": 2.0 } },
        "guard": { "extends": "heavy", "name": "Guard",
            "mounts": { "plate": { "at": "front", "blocker": { "arc": 60.0, "share": 0.25 } },
                        "gun": { "at": "shoulder", "weapon": "chaingun", "range": 60.0, "cooldown": 0.3 } } }
    }"#);
    catalog::add_root(root.clone());
    mods::activate_home(&["classkit".to_string()]).unwrap();
    let r = registry::registry();
    let armor = |id: &str| ArmorClass::by_id(id).unwrap_or_else(|| panic!("{id}"));
    let overstrained = r.effects.index("overstrained").unwrap() as u8;

    // The dash: a shove for its energy, on its cooldown; twice in quick
    // succession is too much strain.
    let mut w = pair(armor("dasher"), 40.0, Team::Glacier);
    w.input.ability = true;
    step(&mut w);
    let v = w.players[0].vel;
    assert!(v.z < -8.0 && v.y > 2.0, "dashed: {v}");
    assert!((w.players[0].energy - 50.0).abs() < 1.0, "{}", w.players[0].energy);
    assert!(w.players[0].strain > 29.0 && !w.has_effect(0, overstrained));
    w.input.ability = true;
    step(&mut w);
    assert_eq!(w.players[0].ability_uses, 1, "cooling down");
    w.players[0].ability_clock = 0.0;
    w.input.ability = true;
    step(&mut w);
    assert!(w.players[0].strain >= 50.0 && w.has_effect(0, overstrained), "too much: {}", w.players[0].strain);
    let health = w.players[0].health;
    for _ in 0..30 { step(&mut w); }
    assert!(w.players[0].health < health - 3.0, "and it hurts");

    // A dasher's touch drains an enemy's energy into their own, and hurts.
    let mut w = pair(armor("dasher"), 0.9, Team::Glacier);
    w.players[1].energy = 60.0;
    w.players[0].energy = 10.0;
    step(&mut w);
    assert!(w.players[1].energy < 35.0 && w.players[0].energy > 35.0, "{} {}", w.players[1].energy, w.players[0].energy);
    assert!(w.players[1].health <= 95.0);
    let once = w.players[1].health;
    step(&mut w);
    assert_eq!(w.players[1].health, once, "once a cooldown");

    // A medic's touch mends a teammate; switched on, Mend holds its effect.
    let mut w = pair(armor("medic"), 0.9, Team::Ember);
    w.players[1].health = 50.0;
    step(&mut w);
    assert!(w.players[1].health >= 69.9, "{}", w.players[1].health);
    let mending = r.effects.index("mending").unwrap() as u8;
    w.input.ability = true;
    step(&mut w);
    assert!(w.players[0].ability_on && w.has_effect(0, mending));

    // The guard's plate stops three quarters of what comes from ahead, and
    // nothing from behind; its shoulder gun fires at an enemy in sight.
    let mut w = pair(armor("guard"), 30.0, Team::Glacier);
    let ahead = Vec3::new(0.0, 0.0, 1.0);
    assert_eq!(w.blocked_share(0, ahead), 0.25, "a shot travelling toward their front");
    assert_eq!(w.blocked_share(0, -ahead), 1.0, "from behind");
    let bullet = r.projectiles.index("bullet").unwrap() as u8;
    let mut fired = false;
    for _ in 0..6 {
        step(&mut w);
        fired |= w.discs.iter().any(|d| d.owner == 0 && d.kind == bullet);
    }
    assert!(fired, "the shoulder gun fired");
    let _ = std::fs::remove_dir_all(&root);
}
