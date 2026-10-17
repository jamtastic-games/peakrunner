//! Packs a mod offers (docs/mod-format.md): the base game's switched shield
//! pack and its ammo pack, and new packs used for an action (a shove, a shot
//! from the shoulder, a borrowed armor), each with its energy, cooldown and
//! charges, given to light armor (which wears only the packs it lists).
//! Its own test binary: activating a mod changes the process's definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{loadout, packs::PackKind, ArmorClass, Team, World, STEP};
use peakrunner_core::terrain::MapId;
use peakrunner_core::{catalog, equipment, mods, registry};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// One tick.
fn step(w: &mut World) { w.tick(STEP); }

#[test]
fn packs_switch_drain_and_act() {
    let root = std::env::temp_dir().join(format!("peakrunner-packs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/kitbag";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Kitbag", "note": "Packs.", "api": 1}"#);
    write(&root, &format!("{m}/packs/kit.json"), r#"
        {
          // A shove along the view: twice, then it's spent.
          "booster": { "name": "Booster", "use": "use", "energy": 10.0, "cooldown": 2.0, "charges": 2,
              "action": { "impulse": [12.0, 6.0] } },
          // A disc from the shoulder.
          "launcher": { "name": "Shoulder launcher", "use": "use", "cooldown": 1.0, "action": { "fire": "disc" } },
          // Heavy armor for three seconds.
          "bulk": { "name": "Bulk", "use": "use", "cooldown": 10.0, "action": { "armor": "heavy", "seconds": 3.0 } } }"#);
    // Light armor lists the packs it may wear: these replace the list.
    write(&root, &format!("{m}/armors/light.json"), r#"{ "light": { "packs": ["shield", "ammo-pack", "booster", "launcher", "bulk"] } }"#);
    catalog::add_root(root.clone());
    mods::activate_home(&["kitbag".to_string()]).unwrap();
    let r = registry::registry();
    let pack = |id: &str| PackKind::from_key(id).unwrap_or_else(|| panic!("{id}"));
    assert_eq!(PackKind::Wall.key(), "wall", "the base packs keep their numbers");

    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.players.truncate(1);
    w.countdown = 0.0;
    w.players[0].team = Team::Ember;
    let station = equipment::definitions(map).iter()
        .find(|d| d.kind == equipment::Kind::Inventory && d.team == 0).unwrap().pos();
    let buy = |w: &mut World, kind: PackKind| {
        w.players[0].pos = station;
        w.input.interact = true;
        w.input.buy = loadout::buy_for(kind);
        step(w);
        assert_eq!(w.players[0].pack, Some(kind));
        // Using the station a moment longer restocks to what it carries now.
        step(w);
        w.input.interact = false;
    };
    let ground = |x: f32, z: f32| Vec3::new(x, peakrunner_core::terrain::support_on(map, Vec3::new(x, 400.0, z)).0, z);
    let out = |w: &mut World| {
        w.players[0].pos = ground(1060.0, 700.0) + Vec3::Y * 0.9;
        w.players[0].vel = Vec3::ZERO;
        w.players[0].yaw = 0.0;
        w.players[0].pitch = 0.1;
    };

    // The ammo pack: half again as many rounds.
    let disc = r.weapons.index("disc").unwrap();
    let plain = loadout::max_ammo(w.players[0].armor)[disc];
    buy(&mut w, pack("ammo-pack"));
    assert_eq!(w.players[0].ammo[disc], (plain as f32 * 1.5).round() as u16);

    // The shield pack: the key switches it on and off; on, it holds its
    // effect and drains energy; out of energy, it switches off.
    let shielded = r.effects.index("shielded").unwrap() as u8;
    buy(&mut w, pack("shield"));
    assert!(w.players[0].ammo[disc] <= plain, "no more than the armor carries without the ammo pack");
    out(&mut w);
    w.input.deploy = true;
    step(&mut w);
    assert!(w.players[0].pack_on && w.has_effect(0, shielded), "switched on");
    for _ in 0..30 { step(&mut w); }
    assert!(w.has_effect(0, shielded), "held on");
    w.input.deploy = true;
    step(&mut w);
    step(&mut w);
    step(&mut w);
    assert!(!w.players[0].pack_on && !w.has_effect(0, shielded), "switched off");
    w.input.deploy = true;
    step(&mut w);
    assert!(w.players[0].pack_on);
    w.players[0].energy = 0.0;
    step(&mut w);
    assert!(!w.players[0].pack_on, "out of energy");

    // The booster: a shove along the view, for its energy, on its cooldown,
    // twice.
    buy(&mut w, pack("booster"));
    out(&mut w);
    w.players[0].energy = 100.0;
    w.input.deploy = true;
    step(&mut w);
    let v = w.players[0].vel;
    assert!(v.z.abs() + v.x.abs() > 8.0 && v.y > 3.0, "shoved: {v}");
    assert!(w.players[0].pack_clock > 1.9 && w.players[0].pack_uses == 1);
    let energy = w.players[0].energy;
    out(&mut w);
    w.input.deploy = true;
    step(&mut w);
    assert!(w.players[0].vel.length() < 3.0, "cooling down");
    assert!(w.players[0].energy >= energy - 0.01, "and nothing spent");
    w.players[0].pack_clock = 0.0;
    w.input.deploy = true;
    step(&mut w);
    assert_eq!(w.players[0].pack_uses, 2);
    w.players[0].pack_clock = 0.0;
    out(&mut w);
    w.input.deploy = true;
    step(&mut w);
    assert!(w.players[0].vel.length() < 3.0, "both charges spent");

    // The shoulder launcher: a disc at the crosshair, apart from the
    // weapon held.
    buy(&mut w, pack("launcher"));
    out(&mut w);
    w.discs.clear();
    let held = w.players[0].weapon;
    w.input.deploy = true;
    step(&mut w);
    let round = r.projectiles.index("disc").unwrap() as u8;
    let shot = w.discs.iter().find(|d| d.kind == round && d.owner == 0).expect("a disc in flight");
    assert!(shot.vel.z.abs() + shot.vel.x.abs() > 40.0, "{}", shot.vel);
    assert_eq!(w.players[0].weapon, held);

    // Bulk: heavy armor for three seconds, then back.
    buy(&mut w, pack("bulk"));
    out(&mut w);
    w.input.deploy = true;
    step(&mut w);
    assert_eq!(w.players[0].armor, ArmorClass::Heavy);
    for _ in 0..(3.2 / STEP) as usize { step(&mut w); }
    assert_eq!(w.players[0].armor, ArmorClass::Light, "back");

    // Packs that place still place (the base wall), for an armor that may
    // wear them: not light armor, here.
    w.players[0].pos = station;
    w.input.buy = loadout::buy_for(PackKind::Wall);
    step(&mut w);
    assert_eq!(w.players[0].pack, Some(pack("bulk")), "not on light armor");
    w.input.buy = loadout::BUY_HEAVY;
    step(&mut w);
    w.input.buy = loadout::buy_for(PackKind::Wall);
    step(&mut w);
    assert_eq!(w.players[0].pack, Some(PackKind::Wall));
    assert_eq!(PackKind::Wall.deployable().map(|d| d.key()), Some("wall"));
    assert!(pack("shield").deployable().is_none());
    let _ = std::fs::remove_dir_all(&root);
}
