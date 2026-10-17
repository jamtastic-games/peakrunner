//! A mod made only of class files recombines the base game's capabilities
//! (docs/mod-format.md): a turret that fires discs, a new armor carrying a
//! quicker disc launcher, and a patched blast, all taking effect in play.
//! Its own test binary: activating a mod changes the process's definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{deploy::{DeployKind, Deployable}, loadout, ArmorClass, Team, World, MAX_PLAYERS, STEP};
use peakrunner_core::terrain::MapId;
use peakrunner_core::{catalog, combat, equipment, mods, registry};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn a_class_mod_recombines_base_capabilities_in_play() {
    let root = std::env::temp_dir().join(format!("peakrunner-classes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/recombine";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Recombine", "note": "Class files only.", "api": 1}"#);
    write(&root, &format!("{m}/weapons/discs.json"), r#"
        // A quicker disc launcher, built from the base one.
        { "rapid-disc": { "extends": "disc", "name": "Rapid disc", "family": "rapid-disc", "reload": 0.4 } }"#);
    write(&root, &format!("{m}/armors/skirmisher.json"), r#"
        { "skirmisher": { "extends": "light", "name": "Skirmisher", "about": "Quick, with the rapid disc.",
            "walk_scale": 1.2, "weapons": { "rapid-disc": 20, "blaster": null }, "kit": ["rapid-disc", "chaingun"] } }"#);
    write(&root, &format!("{m}/deployables/turrets.json"), r#"
        /* The base turret, firing the disc launcher's round. */
        { "disc-turret": { "extends": "turret", "name": "Disc turret",
            "turret": { "projectile": "disc", "speed": 95.0, "cooldown": 1.5 } } }"#);
    write(&root, &format!("{m}/blasts/disc.json"), r#"{ "disc": { "radius": 9.0 } }"#);
    catalog::add_root(root.clone());
    mods::activate_home(&["recombine".to_string()]).unwrap();

    let r = registry::registry();
    let rapid = r.weapons.index("rapid-disc").expect("added") as u8;
    let skirmisher = ArmorClass::by_id("skirmisher").expect("added");
    let disc_round = r.projectiles.index("disc").unwrap() as u8;
    assert_eq!(combat::player_weapon_blast(disc_round).unwrap().radius, 9.0, "the patched blast");
    assert!(!skirmisher.def().weapons.contains_key("blaster"), "null removed it");

    // At their station, a player puts on the new armor and keeps its kit.
    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    let station = equipment::definitions(map).iter()
        .find(|d| d.kind == equipment::Kind::Inventory && d.team == 0).unwrap().pos();
    w.players[0].pos = station;
    w.input.buy = loadout::BUY_ARMOR + skirmisher.0 as u16;
    w.tick(STEP * 2.0);
    w.input.buy = loadout::BUY_NONE;
    let p = &w.players[0];
    assert_eq!(p.armor, skirmisher);
    assert_eq!(p.loadout.kinds().collect::<Vec<_>>(), [rapid, 1], "its kit");
    assert_eq!(p.ammo[rapid as usize], 20, "its rounds");

    // Out on open ground, the rapid disc fires the disc's round, on its own
    // reload.
    let ground = |x: f32, z: f32| Vec3::new(x, peakrunner_core::terrain::support_on(map, Vec3::new(x, 400.0, z)).0, z);
    w.players[0].pos = ground(1060.0, 700.0) + Vec3::Y * 0.9;
    w.players[0].vel = Vec3::ZERO;
    w.players[0].yaw = 0.0;
    w.players[0].pitch = 0.1;
    w.input.weapon = rapid;
    w.input.fire = true;
    w.tick(STEP * 2.0);
    w.input.fire = false;
    let shot = w.discs.iter().find(|d| d.kind == disc_round && d.owner == 0).expect("a disc in flight");
    assert!((shot.vel.length() - 95.0).abs() < 20.0, "{}", shot.vel.length());
    assert!(w.players[0].cooldown > 0.3 && w.players[0].cooldown <= 0.4, "{}", w.players[0].cooldown);
    assert_eq!(w.players[0].ammo[rapid as usize], 19);

    // A disc turret on the field fires discs at an enemy in sight.
    w.discs.clear();
    let kind = DeployKind(r.deployables.index("disc-turret").unwrap() as u8);
    let at = ground(1060.0, 700.0);
    w.deployables.push(Deployable { owner: 0, kind, team: Team::Ember, pos: at, yaw: 0.0, health: 150.0, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.pos = ground(1060.0, 680.0) + Vec3::Y * 0.9;
    w.players.push(foe);
    w.players[0].pos = ground(1000.0, 760.0) + Vec3::Y * 0.9;
    let mut fired = false;
    for _ in 0..120 {
        w.tick(STEP);
        fired |= w.discs.iter().any(|d| d.kind == disc_round && d.owner == MAX_PLAYERS);
        if fired { break; }
    }
    assert!(fired, "the turret fires the disc launcher's round");
    mods::end_session();
    let _ = std::fs::remove_dir_all(&root);
}
