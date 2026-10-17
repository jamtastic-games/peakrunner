//! A mode with an economy, drops and items (docs/mod-format.md): credits
//! from the start, from frags; prices at the station; a setup that comes
//! back at respawn only when paid for again; the drop keys; and what a
//! death leaves for others.
//! Its own test binary: activating a mod changes the process's definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{arsenal, items, loadout, packs::PackKind, ArmorClass, Team, World, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::{catalog, equipment, mods, registry};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn step(w: &mut World) { w.tick(STEP); }

#[test]
fn credits_prices_drops_and_items() {
    let root = std::env::temp_dir().join(format!("peakrunner-economy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/market";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Market", "note": "Credits.", "api": 1}"#);
    write(&root, &format!("{m}/armors/prices.json"), r#"{ "heavy": { "price": 300 } }"#);
    write(&root, &format!("{m}/weapons/prices.json"), r#"{ "railgun": { "price": 80 } }"#);
    write(&root, &format!("{m}/packs/offer.json"), r#"{ "shield": { "offered": true, "price": 50 } }"#);
    write(&root, &format!("{m}/modes/market-ctf.json"), r#"{
        "base": "ctf", "label": "Market CTF", "short": "MCTF", "note": "Credits buy armor.", "goal": "Capture",
        "start_message": "MARKET", "economy": { "start": 100, "frag": 250, "per_minute": 0 },
        "player_drops": true, "death_drops": { "pack": true, "items": ["repair-kit"] },
        "packs": ["shield", "wall"] }"#);
    catalog::add_root(root.clone());
    mods::activate_home(&["market".to_string()]).unwrap();
    let mode = SupportedMode::parse("market-ctf").expect("the mod's mode");
    let r = registry::registry();

    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(mode);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    assert_eq!(w.credits_of(0), 100.0, "the mode's start");
    let station = equipment::definitions(map).iter()
        .find(|d| d.kind == equipment::Kind::Inventory && d.team == 0).unwrap().pos();
    let at_station = |w: &mut World, buy: u16| {
        w.players[0].pos = station;
        w.input.interact = true;
        w.input.buy = buy;
        step(w);
        w.input.interact = false;
    };

    // Heavy armor costs 300: too dear for now.
    at_station(&mut w, loadout::BUY_ARMOR + ArmorClass::Heavy.0 as u16);
    assert_eq!(w.players[0].armor, ArmorClass::Light);
    assert_eq!(w.message, "NOT ENOUGH CREDITS");
    // A shield pack (50) is fine; the mode doesn't offer the turret pack.
    at_station(&mut w, loadout::buy_for(PackKind::from_key("shield").unwrap()));
    assert_eq!(w.players[0].pack, PackKind::from_key("shield"));
    assert_eq!(w.credits_of(0), 50.0);
    at_station(&mut w, loadout::buy_for(PackKind::Turret));
    assert_eq!(w.players[0].pack, PackKind::from_key("shield"), "not offered here");

    // A frag earns 250: now heavy armor.
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    w.players.push(foe);
    w.players[1].health = 1.0;
    w.players[1].pack = Some(PackKind::Wall);
    w.players[1].pos = Vec3::new(1060.0, peakrunner_core::terrain::support_on(map, Vec3::new(1060.0, 400.0, 700.0)).0 + PLAYER_RADIUS, 700.0);
    let loot_before = w.loot.len();
    w.damage(1, 50.0, "blast", Some(0), "Test");
    assert_eq!(w.credits_of(0), 300.0);
    // Its death left its pack and a repair kit (and its rounds).
    let kit = r.items.index("repair-kit").unwrap() as u8 + 1;
    let left: Vec<_> = w.loot[loot_before..].to_vec();
    assert!(left.iter().any(|l| l.pack == PackKind::Wall.0 + 1), "the pack");
    assert!(left.iter().any(|l| l.item == kit), "the repair kit");
    w.players.truncate(1);
    at_station(&mut w, loadout::BUY_ARMOR + ArmorClass::Heavy.0 as u16);
    assert_eq!(w.players[0].armor, ArmorClass::Heavy);
    assert_eq!(w.credits_of(0), 0.0);

    // Dying with no credits: the setup (heavy armor, the shield pack) waits;
    // they spawn in the first armor offered, with its kit.
    w.damage(0, 1000.0, "blast", None, "Test");
    w.players[0].respawn = 0.0;
    step(&mut w);
    assert!(w.players[0].alive);
    assert_eq!(w.players[0].armor, ArmorClass::Light, "unpaid: the kit");
    assert_eq!(w.players[0].pack, None);
    assert_eq!(w.players[0].setup.armor, Some(ArmorClass::Heavy), "kept for later");

    // The drop keys: the weapon in hand, then the pack, for someone else.
    w.players[0].pack = Some(PackKind::Wall);
    let held = w.players[0].weapon;
    assert!(w.players[0].loadout.carries(held));
    w.loot.clear();
    w.input.drop = items::DROP_WEAPON;
    step(&mut w);
    assert!(!w.players[0].loadout.carries(held), "dropped");
    assert_eq!(w.loot.len(), 1);
    assert_eq!(w.loot[0].weapon, held + 1);
    w.input.drop = items::DROP_PACK;
    step(&mut w);
    assert_eq!(w.players[0].pack, None);
    // Not straight back into the same hands; a moment later, yes.
    for _ in 0..30 { step(&mut w); }
    assert!(w.loot.iter().any(|l| l.pack > 0));
    let spot = w.loot.iter().find(|l| l.pack > 0).unwrap().pos;
    w.players[0].pos = spot + Vec3::Y * PLAYER_RADIUS;
    for _ in 0..80 { w.players[0].pos = spot + Vec3::Y * PLAYER_RADIUS; w.players[0].vel = Vec3::ZERO; step(&mut w); }
    assert_eq!(w.players[0].pack, Some(PackKind::Wall), "taken back");

    // A repair kit mends whoever runs over it, if they're hurt.
    w.loot.clear();
    let here = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    w.loot.push(loadout::Loot { pos: here, item: kit, ..Default::default() });
    step(&mut w);
    assert_eq!(w.loot.len(), 1, "not hurt: it stays");
    w.players[0].health = 50.0;
    step(&mut w);
    assert!(w.loot.is_empty() && w.players[0].health >= 89.9, "{}", w.players[0].health);
    assert!(arsenal::slots(w.players[0].armor) > 0);
    let _ = std::fs::remove_dir_all(&root);
}
