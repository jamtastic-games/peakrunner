//! Map features (docs/map-pipeline.md): a copy of a shipped map with a lift,
//! a team door, a hazard, a safe zone, a portal, a low-gravity zone, an updraft,
//! an item and a launch pad added to its manifest. The lift carries whoever stands on it,
//! the door opens for its team and stops shots shut, the zones do what they
//! say, the item comes back, and the pad stands from the start.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{deploy::DeployKind, features, Team, World, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::catalog;

fn step(w: &mut World) { w.tick(STEP); }

/// Old Holler, copied with features added; and the ground under (x, z).
fn fixture() -> (MapId, impl Fn(f32, f32) -> Vec3) {
    let source = catalog::entry(MapId::OldHoller).pack().expect("Old Holler installed").root.clone();
    let dir = std::env::temp_dir().join(format!("peakrunner-features-{}", std::process::id())).join("maps/feature-holler");
    std::fs::create_dir_all(&dir).unwrap();
    for f in std::fs::read_dir(&source).unwrap().filter_map(Result::ok) {
        let name = f.file_name();
        if name == "map.json" || name == "info.json" { continue; }
        let to = dir.join(&name);
        if std::fs::hard_link(f.path(), &to).is_err() { std::fs::copy(f.path(), &to).unwrap(); }
    }
    std::fs::write(dir.join("info.json"), r#"{"name":"Feature Holler"}"#).unwrap();
    let ground = |x: f32, z: f32| {
        Vec3::new(x, peakrunner_core::terrain::support_on(MapId::OldHoller, Vec3::new(x, 400.0, z)).0, z)
    };
    let (lift, door, hazard, safe, portal, kit, pad) =
        (ground(1060.0, 700.0), ground(1080.0, 700.0), ground(1100.0, 700.0), ground(1120.0, 700.0), ground(1140.0, 700.0),
         ground(1060.0, 740.0), ground(1100.0, 740.0));
    let mut manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(source.join("map.json")).unwrap()).unwrap();
    let at = |v: Vec3, up: f32| serde_json::json!([v.x, v.y + up, v.z]);
    manifest["moveables"] = serde_json::json!([
        { "size": [1.5, 0.2, 1.5], "path": [at(lift, 0.2), at(lift, 10.2)], "speed": 4.0, "wait": 1.0, "drive": "loop", "look": "lift" },
        { "size": [2.0, 1.6, 0.2], "path": [at(door, 1.6), at(door, 5.0)], "speed": 4.0, "drive": "team", "team": 0, "range": 6.0, "look": "door" }
    ]);
    manifest["zones"] = serde_json::json!([
        { "kind": "hazard", "at": at(hazard, 1.0), "size": [3.0, 3.0, 3.0], "damage": 30.0, "message": "TOO HOT" },
        { "kind": "safe", "at": at(safe, 1.0), "size": [3.0, 3.0, 3.0] },
        { "kind": "portal", "at": at(portal, 1.0), "size": [2.0, 2.0, 2.0], "to": at(ground(1000.0, 650.0), 1.0), "facing": 90.0 },
        { "kind": "gravity", "at": at(ground(1160.0, 700.0), 15.0), "size": [3.0, 15.0, 3.0], "gravity": 0.1 },
        { "kind": "updraft", "at": at(ground(1180.0, 700.0), 15.0), "size": [4.0, 15.0, 4.0], "lift": 30.0 }
    ]);
    manifest["items"] = serde_json::json!([{ "item": "repair-kit", "at": at(kit, 0.0) }]);
    manifest["deployables"] = serde_json::json!([{ "deployable": "launch-pad", "team": 0, "at": at(pad, 0.0) }]);
    std::fs::write(dir.join("map.json"), serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    (catalog::add_folder("feature-holler", dir).expect("the copy loads"), ground)
}

fn world(map: MapId) -> World {
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    w
}

#[test]
fn moveables_zones_items_and_units() {
    let (map, ground) = fixture();
    assert!(catalog::entry(map).pack().is_some(), "{:?}", catalog::entry(map).problem());
    assert_eq!(features::moveables(map).len(), 2);

    // The lift: every machine puts it in the same place at the same step,
    // and it carries whoever stands on it.
    let mut w = world(map);
    let lift = &features::moveables(map)[0];
    assert_eq!(lift.at(12_345, 0.0), lift.at(12_345, 0.7), "a loop goes by the step alone");
    let base = ground(1060.0, 700.0);
    // At the bottom, waiting: stand on it.
    let start = (0..1000u64).find(|&s| lift.at(w.steps + s, 0.0).y <= base.y + 0.21 && lift.at(w.steps + s + 1, 0.0).y <= base.y + 0.21).unwrap();
    for _ in 0..start { step(&mut w); }
    w.players[0].pos = lift.at(w.steps, 0.0) + Vec3::Y * (0.2 + PLAYER_RADIUS + 0.05);
    w.players[0].vel = Vec3::ZERO;
    for _ in 0..(2.2 / STEP) as usize { step(&mut w); }
    let y = w.players[0].pos.y;
    assert!(y > base.y + 3.0, "carried up: {y} over {}", base.y);

    // The door opens for its team and stops shots while shut.
    let mut w = world(map);
    let door = ground(1080.0, 700.0);
    w.players[0].pos = door + Vec3::new(0.0, PLAYER_RADIUS, 20.0);
    step(&mut w);
    let a = door + Vec3::new(0.0, 1.0, -4.0);
    let b = door + Vec3::new(0.0, 1.0, 4.0);
    assert!(w.blocked(a, b, 0.0, None).is_some(), "shut: it stops a shot");
    w.players[0].pos = door + Vec3::new(0.0, PLAYER_RADIUS, 4.0);
    for _ in 0..90 { step(&mut w); }
    assert!(w.moveable_open[1] > 0.99, "open for its team: {}", w.moveable_open[1]);
    assert!(w.blocked(a, b, 0.0, None).is_none(), "open: shots pass");
    w.players[0].team = Team::Glacier;
    for _ in 0..90 { step(&mut w); }
    assert!(w.moveable_open[1] < 0.01, "shut on the other team");

    // A hazard hurts; a safe zone protects; a portal takes you away.
    let mut w = world(map);
    w.players[0].pos = ground(1100.0, 700.0) + Vec3::Y * PLAYER_RADIUS;
    for _ in 0..30 { step(&mut w); }
    assert!(w.players[0].health < 90.0, "{}", w.players[0].health);
    assert_eq!(w.message, "TOO HOT");
    w.players[0].health = 100.0;
    w.players[0].pos = ground(1120.0, 700.0) + Vec3::Y * PLAYER_RADIUS;
    step(&mut w);
    w.damage(0, 50.0, "blast", None, "Test");
    assert_eq!(w.players[0].health, 100.0, "safe");
    w.players[0].pos = ground(1140.0, 700.0) + Vec3::Y * PLAYER_RADIUS;
    step(&mut w);
    assert!(w.players[0].pos.distance(ground(1000.0, 650.0)) < 3.0, "through the portal: {}", w.players[0].pos);

    // In a low-gravity zone a falling player drops a tenth as far.
    let fall = |at: Vec3| {
        let mut w = world(map);
        w.players[0].pos = at;
        w.players[0].vel = Vec3::ZERO;
        for _ in 0..(1.0 / STEP) as usize { step(&mut w); }
        at.y - w.players[0].pos.y
    };
    let (inside, outside) = (fall(ground(1160.0, 700.0) + Vec3::Y * 20.0), fall(ground(1200.0, 700.0) + Vec3::Y * 20.0));
    assert!(inside < 1.5 && outside > 7.0, "fell {inside} in low gravity, {outside} outside");

    // An updraft carries someone who jumps into it, and they hang near its
    // top rather than flying out of it.
    let mut w = world(map);
    let foot = ground(1180.0, 700.0);
    w.players[0].pos = foot + Vec3::Y * (PLAYER_RADIUS + 1.5);
    w.players[0].vel = Vec3::ZERO;
    let mut highest = 0.0f32;
    for _ in 0..(6.0 / STEP) as usize { step(&mut w); highest = highest.max(w.players[0].pos.y - foot.y); }
    let rise = w.players[0].pos.y - foot.y;
    assert!(rise > 12.0, "lifted {rise} m");
    assert!(highest < 33.0, "rose to {highest} m: out of the top");
    assert_eq!(features::lift_at(map, Team::Ember, foot + Vec3::new(10.0, 2.0, 0.0)), 0.0, "nothing outside it");

    // The map's item mends the hurt and comes back; its pad stands.
    let mut w = world(map);
    assert!(w.deployables.iter().any(|d| d.kind == DeployKind::from_key("launch-pad").unwrap() && d.owner == 0));
    let kit = ground(1060.0, 740.0);
    w.players[0].pos = kit + Vec3::Y * PLAYER_RADIUS;
    step(&mut w);
    assert_eq!(w.map_item_clocks[0], 0.0, "not hurt: it stays");
    w.players[0].health = 40.0;
    step(&mut w);
    assert!(w.players[0].health > 79.0 && w.map_item_clocks[0] > 29.0);
    w.players[0].pos = ground(1000.0, 650.0) + Vec3::Y * PLAYER_RADIUS;
    for _ in 0..(30.5 / STEP) as usize { step(&mut w); }
    assert_eq!(w.map_item_clocks[0], 0.0, "back");
}
