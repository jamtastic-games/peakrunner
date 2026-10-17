//! Damage kinds, resistances, hit zones and knocks, armor caps and tanks,
//! and the limits on deployables and throwables, set by a mod and seen in
//! play. Its own test binary: activating a mod changes the process's
//! definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{arsenal, deploy::{DeployKind, Deployable, Used}, loadout, throwables, ArmorClass, Team, World, STEP};
use peakrunner_core::terrain::{MapId, EYE, PLAYER_RADIUS};
use peakrunner_core::{catalog, mods};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// A shooter on open ground with the railgun, and a foe `ahead` metres in
/// front, wearing `armor`.
fn range(armor: ArmorClass, ahead: f32) -> World {
    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.players.truncate(1);
    let ground = |x: f32, z: f32| Vec3::new(x, peakrunner_core::terrain::support_on(map, Vec3::new(x, 400.0, z)).0 + PLAYER_RADIUS, z);
    w.players[0].pos = ground(1060.0, 700.0);
    w.players[0].armor = ArmorClass::Heavy;
    w.players[0].loadout.push(arsenal::RAILGUN);
    w.players[0].ammo[arsenal::RAILGUN as usize] = 20;
    w.players[0].yaw = 0.0;
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.armor = armor;
    foe.pos = ground(1060.0, 700.0 - ahead);
    foe.yaw = 0.0;
    w.players.push(foe);
    w
}

/// Fire the railgun at the foe's chest (or head) and let the slug arrive.
fn shoot(w: &mut World, head: bool) {
    let target = w.players[1].pos + Vec3::Y * if head { EYE } else { 0.9 };
    let d = target - (w.players[0].pos + Vec3::Y * EYE);
    w.players[0].pitch = (d.y / d.length()).asin();
    w.players[0].cooldown = 0.0;
    w.input.weapon = arsenal::RAILGUN;
    w.input.fire = true;
    w.tick(STEP * 2.0);
    w.input.fire = false;
    for _ in 0..20 { w.tick(STEP); }
}

#[test]
fn damage_kinds_resist_zone_and_knock_and_limits_hold() {
    let root = std::env::temp_dir().join(format!("peakrunner-damage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/hardhits";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Hard hits", "api": 1}"#);
    write(&root, &format!("{m}/damage/kinds.json"), r#"{
        "slug": { "head": 2.0, "knock_flag": 50.0, "disarm": 120.0 },
        "bullet": { "drop_pack": 5.0 } }"#);
    write(&root, &format!("{m}/armors/armors.json"), r#"{
        "heavy": { "resist": { "slug": 0.5 }, "energy_max": 90.0, "team_cap": 1 } }"#);
    write(&root, &format!("{m}/deployables/limits.json"), r#"{
        "wall": { "per_player": 1, "match_limit": 2 },
        "field": { "per_teammate": 1.0 } }"#);
    write(&root, &format!("{m}/throwables/limits.json"), r#"{ "hand-grenade": { "match_limit": 1 } }"#);
    // A slug that leaves light armor standing, so each multiple shows.
    write(&root, &format!("{m}/projectiles/slug.json"), r#"{ "rail-slug": { "damage": 78.0 } }"#);
    catalog::add_root(root.clone());
    mods::activate_home(&["hardhits".to_string()]).unwrap();
    let slug = 78.0;

    // A chest hit on light armor: the slug as is. On heavy: half for the
    // armor, half again for its resistance to slugs.
    let mut w = range(ArmorClass::Light, 30.0);
    shoot(&mut w, false);
    assert!((w.players[1].health - (100.0 - slug)).abs() < 0.01, "light chest {}", w.players[1].health);
    let mut w = range(ArmorClass::Heavy, 30.0);
    shoot(&mut w, false);
    assert!((w.players[1].health - (100.0 - slug * 0.25)).abs() < 0.01, "heavy chest {}", w.players[1].health);
    // In the head, twice; hard enough (156) to knock the weapon out of the hand.
    let mut w = range(ArmorClass::Heavy, 30.0);
    let held = w.players[1].weapon;
    assert!(w.players[1].loadout.carries(held));
    shoot(&mut w, true);
    assert!((w.players[1].health - (100.0 - slug * 2.0 * 0.25)).abs() < 0.01, "heavy head {}", w.players[1].health);
    assert!(!w.players[1].loadout.carries(held), "disarmed");

    // The flag comes off a carrier hit from behind, not from the front.
    let mut w = range(ArmorClass::Heavy, 30.0);
    w.players[1].yaw = std::f32::consts::PI; // facing the shooter
    w.players[1].carrying = Some(Team::Ember);
    w.flags[0].carrier = Some(1);
    shoot(&mut w, false);
    assert_eq!(w.players[1].carrying, Some(Team::Ember), "a hit from the front keeps it");
    w.players[1].yaw = 0.0; // back to the shooter
    shoot(&mut w, false);
    assert_eq!((w.players[1].carrying, w.flags[0].carrier), (None, None), "knocked off from behind");

    // A tank of its own.
    assert_eq!(loadout::energy_max(ArmorClass::Heavy), 90.0);
    let mut w = range(ArmorClass::Light, 30.0);
    w.players[0].energy = 80.0;
    for _ in 0..240 { w.tick(STEP); }
    assert!(w.players[0].energy > 85.0 && w.players[0].energy <= 90.0, "heavy fills to 90: {}", w.players[0].energy);

    // One heavy a team: the second is refused at the station.
    let mut w = range(ArmorClass::Light, 30.0);
    w.players[0].armor = ArmorClass::Light;
    let mut mate = w.players[0].clone();
    mate.armor = ArmorClass::Heavy;
    mate.pos += Vec3::new(20.0, 0.0, 0.0);
    w.players.push(mate);
    let station = peakrunner_core::equipment::definitions(MapId::OldHoller).iter()
        .find(|d| d.kind == peakrunner_core::equipment::Kind::Inventory && d.team == 0).unwrap().pos();
    w.players[0].pos = station;
    w.input.buy = loadout::BUY_HEAVY;
    w.tick(STEP * 2.0);
    w.input.buy = loadout::BUY_NONE;
    assert_eq!(w.players[0].armor, ArmorClass::Light, "the team's one heavy is taken");

    // Deployable limits: one wall a player, two a round; fields grow with the team.
    let mut w = range(ArmorClass::Light, 30.0);
    w.players.truncate(1);
    let ember = w.players[0].team;
    w.players[0].pitch = -0.4;
    let free = (0..16).map(|k| k as f32 * 0.39).find(|&y| { w.players[0].yaw = y; w.place(0, DeployKind::Wall, 0.0).is_ok() });
    assert!(free.is_some(), "somewhere to put a wall");
    let mine = Deployable { owner: 1, kind: DeployKind::Wall, team: ember, pos: w.players[0].pos + Vec3::new(40.0, 0.0, 0.0),
        yaw: 0.0, health: 400.0, cooldown: 0.0, aim: Vec3::Z, age: 0.0 };
    w.deployables.push(mine.clone());
    assert_eq!(w.place(0, DeployKind::Wall, 0.0).err(), Some("YOU HAVE THE MOST OF THOSE YOU MAY PLACE"));
    w.deployables.clear();
    w.uses.insert((Used::Placed, ember.idx() as u8, DeployKind::Wall.0), 2);
    assert_eq!(w.place(0, DeployKind::Wall, 0.0).err(), Some("YOUR TEAM HAS PLACED ALL OF THOSE IT MAY THIS ROUND"));
    let base = w.team_limit(DeployKind::Field, ember);
    let mut mate = w.players[0].clone();
    mate.pos += Vec3::new(30.0, 0.0, 0.0);
    w.players.push(mate);
    assert_eq!(w.team_limit(DeployKind::Field, ember), base + 1, "one more field per teammate");

    // One hand grenade a round per team.
    let mut w = range(ArmorClass::Light, 30.0);
    w.players[0].armor = ArmorClass::Light;
    w.players[0].throwables = throwables::max_throwables(ArmorClass::Light);
    w.input.throw = throwables::THROW_GRENADE;
    w.input.throw_strength = 1.0;
    w.tick(STEP);
    w.input.throw = throwables::THROW_NONE;
    for _ in 0..30 { w.tick(STEP); }
    w.input.throw = throwables::THROW_GRENADE;
    w.tick(STEP);
    assert_eq!(w.message, "YOUR TEAM HAS THROWN ALL THE GRENADES IT MAY THIS ROUND");

    mods::end_session();
    let _ = std::fs::remove_dir_all(&root);
}
