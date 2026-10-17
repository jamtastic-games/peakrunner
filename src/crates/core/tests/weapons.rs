//! The projectile and weapon classes of phase 4, made by a mod and seen in
//! play: rockets that speed up, homing rounds and lock-on, guided rounds,
//! pellets, held beams that drain and mend, marking beams, a barrel that
//! spins up, a charged beam (and its overload), bursts, remote detonation and
//! fire modes. Its own test binary: activating a mod changes the process's
//! definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{Team, World, STEP};
use peakrunner_core::terrain::{MapId, EYE, PLAYER_RADIUS};
use peakrunner_core::{catalog, mods, registry};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

const PROJECTILES: &str = r#"{
  "rocket": { "class": "straight", "speed": 30.0, "accel": 200.0, "max_speed": 120.0, "life": 4.0, "blast": "disc", "kill": "Rocket", "look": "disc" },
  "hunter": { "class": "straight", "speed": 60.0, "life": 5.0, "blast": "disc", "kill": "Hunter",
    "seek": { "turn": 240.0, "cone": 70.0, "range": 200.0, "proximity": 3.0 } },
  "guided-round": { "class": "straight", "speed": 50.0, "life": 5.0, "guided": 360.0, "blast": "disc", "kill": "Guided" },
  "pellet": { "class": "straight", "speed": 300.0, "life": 0.5, "damage": 5.0, "pellets": 8, "cone": 12.0,
    "expire": "vanish", "water": "fizzle", "kill": "Scatter", "look": "tracer", "damage_kind": "bullet" },
  "drain-stream": { "class": "stream", "life": 0.1, "stream": { "range": 60.0, "damage": 20.0, "drain": 30.0, "heal": 15.0, "repair": 40.0 },
    "kill": "Drain gun", "damage_kind": "electric" },
  "marker": { "class": "beam", "life": 0.1, "range": 500.0, "marks": 10.0, "expire": "vanish", "kill": "Marker" }
}"#;

const WEAPONS: &str = r#"{
  "rocket-launcher": { "name": "Rocket launcher", "family": "rocket", "projectile": "rocket", "reload": 1.0, "view": "launcher", "sound": "mortar" },
  "hunter-launcher": { "name": "Hunter", "family": "hunter", "projectile": "hunter", "reload": 1.0, "view": "launcher", "sound": "mortar",
    "lock": 0.5, "lock_cone": 10.0, "lock_range": 200.0 },
  "guide": { "name": "Guide", "family": "guide", "projectile": "guided-round", "reload": 1.0, "view": "launcher", "sound": "mortar" },
  "scatter": { "name": "Scatter", "family": "scatter", "projectile": "pellet", "reload": 0.5, "view": "chaingun", "sound": "chain" },
  "drain-gun": { "name": "Drain gun", "family": "drain", "projectile": "drain-stream", "fire": "sustained", "energy": 10.0, "reload": 0.1,
    "view": "rifle", "sound": "laser" },
  "marker": { "name": "Marker", "family": "marker", "projectile": "marker", "reload": 0.5, "view": "rifle", "sound": "laser" },
  "spinner": { "extends": "chaingun", "name": "Spinner", "family": "spinner", "fire": "spin", "spin_up": 0.5, "spin_down": 0.5 },
  "lance": { "extends": "laser", "name": "Lance", "family": "lance", "energy": 0.0, "fire": "charge", "charge": 1.0, "charge_scale": 2.0,
    "overload": 1.0, "overload_damage": 30.0 },
  "burster": { "extends": "disc", "name": "Burster", "family": "burster", "fire": "burst", "burst": 3, "burst_gap": 0.1, "reload": 1.0 },
  "sticky": { "extends": "grenade_launcher", "name": "Sticky launcher", "family": "sticky", "remote": true,
    "modes": [{ "name": "Lobber", "reload": 0.4, "remote": false }] }
}"#;

/// A shooter on open ground holding `weapon`, a foe `ahead` metres in front
/// standing still, and a teammate beside them.
fn range(weapon: &str, ahead: f32) -> World {
    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.players.truncate(1);
    let ground = |x: f32, z: f32| Vec3::new(x, peakrunner_core::terrain::support_on(map, Vec3::new(x, 400.0, z)).0 + PLAYER_RADIUS, z);
    let k = registry::registry().weapons.index(weapon).unwrap() as u8;
    w.players[0].pos = ground(1060.0, 700.0);
    w.players[0].team = Team::Ember;
    w.players[0].loadout = peakrunner_core::sim::arsenal::Loadout::of(&[k]);
    w.players[0].weapon = k;
    w.input.weapon = k;
    w.players[0].yaw = 0.0;
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.pos = ground(1060.0, 700.0 - ahead);
    w.players.push(foe);
    let mut mate = w.players[0].clone();
    mate.pos = ground(1066.0, 700.0);
    w.players.push(mate);
    aim(&mut w, 1);
    w
}

fn aim(w: &mut World, at: usize) {
    let d = (w.players[at].pos + Vec3::Y * 0.9) - (w.players[0].pos + Vec3::Y * EYE);
    w.players[0].yaw = (-d.x).atan2(-d.z);
    w.players[0].pitch = (d.y / d.length()).asin();
}

/// Ticks with fire held (or not), the targets standing still.
fn run(w: &mut World, fire: bool, seconds: f32) {
    let still: Vec<Vec3> = w.players.iter().map(|p| p.pos).collect();
    w.input.fire = fire;
    for _ in 0..(seconds / STEP).round() as usize {
        w.tick(STEP);
        for (k, p) in w.players.iter_mut().enumerate().skip(1) { p.pos = still[k]; p.vel = Vec3::ZERO; }
    }
    w.input.fire = false;
}

fn kind(id: &str) -> u8 { registry::registry().projectiles.index(id).unwrap() as u8 }

#[test]
fn every_new_weapon_class_plays() {
    let root = std::env::temp_dir().join(format!("peakrunner-weapons-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/arsenal";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Arsenal", "api": 1}"#);
    write(&root, &format!("{m}/projectiles/new.json"), PROJECTILES);
    write(&root, &format!("{m}/weapons/new.json"), WEAPONS);
    let ids = ["rocket-launcher", "hunter-launcher", "guide", "scatter", "drain-gun", "marker", "spinner", "lance", "burster", "sticky"];
    let carried = ids.iter().map(|id| format!("\"{id}\": 20")).collect::<Vec<_>>().join(", ");
    write(&root, &format!("{m}/armors/light.json"), &format!(r#"{{ "light": {{ "weapons": {{ {carried} }} }} }}"#));
    catalog::add_root(root.clone());
    mods::activate_home(&["arsenal".to_string()]).unwrap();

    // A rocket speeds up, to its top speed.
    let mut w = range("rocket-launcher", 200.0);
    w.players[0].pitch += 0.1;
    run(&mut w, true, STEP * 2.0);
    let first = w.discs.iter().find(|d| d.kind == kind("rocket")).expect("a rocket").vel.length();
    run(&mut w, false, 0.5);
    let later = w.discs.iter().find(|d| d.kind == kind("rocket")).expect("still flying").vel.length();
    assert!(first < 45.0 && later > first + 50.0 && later <= 120.01, "{first} then {later}");

    // A homing round fired off to one side turns onto its target.
    let mut w = range("hunter-launcher", 40.0);
    w.players[0].yaw += 0.6;
    run(&mut w, true, STEP * 2.0);
    let round = w.discs.iter().find(|d| d.kind == kind("hunter")).expect("a hunter");
    assert_eq!(round.target, 2, "it chose the foe");
    run(&mut w, false, 2.0);
    assert!(w.players[1].health < 100.0, "it found them: {}", w.players[1].health);
    // Lock-on: held in the crosshair for half a second.
    let mut w = range("hunter-launcher", 60.0);
    w.players[0].cooldown = 5.0;
    run(&mut w, true, 0.6);
    assert_eq!(w.players[0].lock_target, 2);
    assert!(w.players[0].lock_time >= 0.5);

    // A guided round follows the shooter's aim.
    let mut w = range("guide", 200.0);
    run(&mut w, true, STEP * 2.0);
    w.players[0].yaw += 0.8;
    run(&mut w, false, 0.5);
    let g = w.discs.iter().find(|d| d.kind == kind("guided-round")).expect("in flight");
    let look = Vec3::new(-w.players[0].yaw.sin(), 0.0, -w.players[0].yaw.cos());
    assert!(Vec3::new(g.vel.x, 0.0, g.vel.z).normalize().dot(look) > 0.95, "turned to the new aim");

    // A scatter shot: eight pellets.
    let mut w = range("scatter", 30.0);
    run(&mut w, true, STEP);
    assert_eq!(w.discs.iter().filter(|d| d.kind == kind("pellet")).count(), 8);

    // A held drain beam: the foe loses health and energy, the shooter energy.
    let mut w = range("drain-gun", 20.0);
    let (energy, foe_energy) = (w.players[0].energy, w.players[1].energy);
    run(&mut w, true, 1.0);
    let foe = &w.players[1];
    assert!((foe.health - 80.0).abs() < 2.0, "foe {}", foe.health);
    assert!(foe_energy - foe.energy > 25.0, "drained {}", foe.energy);
    assert!(energy - w.players[0].energy > 5.0, "it costs the shooter");
    assert!(w.players[0].stream.is_some(), "the beam shows where it lands");
    run(&mut w, false, STEP);
    assert!(w.players[0].stream.is_none(), "and stops when released");
    // On a hurt teammate it mends.
    let mut w = range("drain-gun", 20.0);
    w.players[2].health = 50.0;
    aim(&mut w, 2);
    run(&mut w, true, 1.0);
    assert!(w.players[2].health > 60.0, "mended to {}", w.players[2].health);

    // A marking beam marks the foe for the shooter's team.
    let mut w = range("marker", 50.0);
    run(&mut w, true, STEP * 2.0);
    assert!(w.marked(Team::Ember, 1), "marked");

    // A barrel that spins up: no shot for half a second, then shots.
    let mut w = range("spinner", 30.0);
    run(&mut w, true, 0.4);
    assert_eq!(w.players[0].shots, 0, "still spinning up");
    run(&mut w, true, 0.3);
    assert!(w.players[0].shots > 0, "then it fires");

    // A charged beam: held a second and released, twice the damage (a light
    // foe dies); held two seconds past full, it bites the shooter.
    let mut w = range("lance", 40.0);
    run(&mut w, true, 1.0);
    run(&mut w, false, STEP * 2.0);
    assert!(!w.players[1].alive, "a full charge kills: {}", w.players[1].health);
    let mut w = range("lance", 40.0);
    run(&mut w, true, 2.2);
    assert!(w.players[0].health < 75.0, "overloaded: {}", w.players[0].health);

    // A burst: three discs from one pull.
    let mut w = range("burster", 150.0);
    w.players[0].pitch += 0.2;
    run(&mut w, true, STEP);
    run(&mut w, false, 0.35);
    assert_eq!(w.players[0].shots, 3);

    // Remote detonation, and a second fire mode without it.
    let mut w = range("sticky", 60.0);
    w.players[0].pitch += 0.3;
    run(&mut w, true, STEP * 2.0);
    run(&mut w, false, 0.2);
    assert_eq!(w.discs.iter().filter(|d| d.kind == kind("grenade-shell")).count(), 1);
    let blasts = w.blast_serial;
    run(&mut w, true, STEP * 2.0);
    assert!(w.blast_serial > blasts, "set off");
    assert_eq!(w.players[0].shots, 1, "instead of firing again");
    w.input.mode = true;
    w.tick(STEP);
    assert_eq!(w.weapon_def(0).unwrap().name, "Lobber");
    assert!(!w.weapon_def(0).unwrap().remote && (w.weapon_def(0).unwrap().reload - 0.4).abs() < 1e-6);
    w.input.mode = true;
    w.tick(STEP);
    assert_eq!(w.weapon_def(0).unwrap().name, "Sticky launcher", "round to its first mode");

    mods::end_session();
    let _ = std::fs::remove_dir_all(&root);
}
