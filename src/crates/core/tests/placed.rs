//! Placed things from the stock library (docs/mod-format.md): pads throw,
//! platforms hold you up, teleporters link in pairs, a shock floor hurts
//! enemies, pylons hold effects on teammates, sensors sense enemies and
//! sound alarms, the recall pack goes back to its beacon, and a command
//! station works a turret and sets a waypoint.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{deploy::{DeployKind, Deployable}, packs::PackKind, Team, World, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::registry;

fn step(w: &mut World) { w.tick(STEP); }

const MAP: MapId = MapId::OldHoller;

fn floor(x: f32, z: f32) -> Vec3 { Vec3::new(x, peakrunner_core::terrain::support_on(MAP, Vec3::new(x, 400.0, z)).0, z) }

/// Player 0 on open ground, no one else.
fn alone() -> World {
    let mut w = World::new();
    w.set_map(MAP);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    w.players[0].pos = floor(1060.0, 700.0) + Vec3::Y * PLAYER_RADIUS;
    w.players[0].vel = Vec3::ZERO;
    w.players[0].yaw = 0.0;
    w.players[0].pitch = 0.0;
    w
}

fn unit(id: &str, team: Team, pos: Vec3, yaw: f32) -> Deployable {
    let kind = DeployKind::from_key(id).unwrap_or_else(|| panic!("{id}"));
    Deployable { owner: 0, kind, team, pos, yaw, health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 }
}

/// An enemy beside player 0, `ahead` metres in front.
fn enemy(w: &mut World, ahead: f32) -> usize {
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.pos = floor(1060.0, 700.0 - ahead) + Vec3::Y * PLAYER_RADIUS;
    w.players.push(foe);
    w.players.len() - 1
}

#[test]
fn pads_and_platforms() {
    // A launch pad underfoot: straight up.
    let mut w = alone();
    let at = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    w.deployables.push(unit("launch-pad", Team::Ember, at - Vec3::Y * 0.3, 0.0));
    w.players[0].pos.y = at.y + PLAYER_RADIUS;
    step(&mut w);
    assert!(w.players[0].vel.y > 25.0, "launched: {}", w.players[0].vel);
    // An accelerator pad throws along the way it was placed.
    let mut w = alone();
    let at = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    w.deployables.push(unit("accelerator-pad", Team::Ember, at - Vec3::Y * 0.3, std::f32::consts::FRAC_PI_2));
    w.players[0].pos.y = at.y + PLAYER_RADIUS;
    step(&mut w);
    let v = w.players[0].vel;
    assert!(v.x < -20.0 && v.y > 8.0, "thrown the way it faces: {v}");
    // A platform in the air holds a player up.
    let mut w = alone();
    let deck = w.players[0].pos + Vec3::Y * 6.0;
    w.deployables.push(unit("platform", Team::Ember, deck, 0.0));
    w.players[0].pos = deck + Vec3::Y * (0.3 + PLAYER_RADIUS + 0.4);
    for _ in 0..60 { step(&mut w); }
    let y = w.players[0].pos.y;
    assert!((y - (deck.y + 0.3 + PLAYER_RADIUS)).abs() < 0.1, "standing on it: {y} over {}", deck.y);
}

#[test]
fn teleporters_shocks_auras_and_sensors() {
    let r = registry::registry();
    // Two teleporters: in one, out the other; not straight back.
    let mut w = alone();
    let here = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    let there = floor(1100.0, 760.0);
    w.deployables.push(unit("teleporter", Team::Ember, here, 0.0));
    w.deployables.push(unit("teleporter", Team::Ember, there, 0.0));
    step(&mut w);
    assert!(w.players[0].pos.distance(there) < 4.0, "came out at the other: {}", w.players[0].pos);
    step(&mut w);
    assert!(w.players[0].pos.distance(there) < 4.0, "not straight back");
    assert_eq!(w.partner(0), Some(1));

    // A shock floor hurts an enemy on it, once a cooldown.
    let mut w = alone();
    let foe = enemy(&mut w, 30.0);
    let at = w.players[foe].pos - Vec3::Y * PLAYER_RADIUS;
    w.deployables.push(unit("shock-floor", Team::Ember, at - Vec3::Y * 0.3, 0.0));
    w.players[foe].pos.y = at.y + PLAYER_RADIUS;
    w.players[foe].vel = Vec3::ZERO;
    step(&mut w);
    let hurt = w.players[foe].health;
    assert!(hurt < 100.0, "shocked: {hurt}");
    step(&mut w);
    assert_eq!(w.players[foe].health, hurt, "once a cooldown");
    // A teammate standing on it is fine.
    assert_eq!(w.players[0].health, 100.0);

    // A jammer pylon hides teammates near from sensors; a motion sensor
    // senses a moving enemy, with an alarm, and turrets reach their sensed
    // range.
    let mut w = alone();
    let foe = enemy(&mut w, 40.0);
    let mine = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    w.deployables.push(unit("jammer-pylon", Team::Glacier, w.players[foe].pos - Vec3::Y * PLAYER_RADIUS + Vec3::X * 2.0, 0.0));
    step(&mut w);
    let jammed = r.effects.index("jammed").unwrap() as u8;
    assert!(w.has_effect(foe, jammed), "the pylon's aura");
    w.deployables.clear();
    w.players[foe].effects.clear();
    w.deployables.push(unit("motion-sensor", Team::Ember, mine + Vec3::X * 3.0, 0.0));
    assert!(w.has_sensor(Team::Ember));
    w.players[foe].vel = Vec3::ZERO;
    assert!(w.sensed(Team::Ember).is_empty(), "standing still");
    w.players[foe].vel = Vec3::new(8.0, 0.0, 0.0);
    assert_eq!(w.sensed(Team::Ember), vec![foe], "moving");
    w.message.clear();
    step(&mut w);
    assert_eq!(w.message, "SENSOR ALARM");
}

#[test]
fn recall_and_command_stations() {
    // The recall pack: a beacon down, then back to it.
    let mut w = alone();
    w.players[0].pack = PackKind::from_key("recall");
    w.players[0].pitch = -0.3;
    w.players[0].energy = 60.0;
    w.input.deploy = true;
    step(&mut w);
    let beacon = DeployKind::from_key("beacon").unwrap();
    let placed = w.deployables.iter().find(|e| e.kind == beacon).expect("a beacon down").pos;
    w.players[0].pos = floor(1120.0, 640.0) + Vec3::Y * PLAYER_RADIUS;
    w.players[0].pack_clock = 0.0;
    w.input.deploy = true;
    step(&mut w);
    assert!(w.players[0].pos.distance(placed) < 1.5, "back at the beacon");
    assert!(!w.deployables.iter().any(|e| e.kind == beacon), "used up");

    // A command station: work a team turret, and set a waypoint.
    let mut w = alone();
    let me = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    w.deployables.push(unit("command-station", Team::Ember, me + Vec3::X * 1.5, 0.0));
    w.deployables.push(unit("turret", Team::Ember, floor(1080.0, 700.0), 0.0));
    assert!(w.at_command(0));
    assert_eq!(w.viewable(Team::Ember), vec![1]);
    w.players[0].yaw = 0.0;
    w.players[0].pitch = 0.05;
    w.input.operate = 2;
    w.input.fire = true;
    let shots = w.players[0].shots;
    step(&mut w);
    let bullet = registry::registry().projectiles.index("bullet").unwrap() as u8;
    let shot = w.discs.iter().find(|d| d.owner == 0 && d.kind == bullet).expect("the turret fired for its operator");
    assert!(shot.vel.z < -100.0, "where they look: {}", shot.vel);
    assert_eq!(w.players[0].shots, shots, "their own weapon didn't");
    w.input.fire = false;
    w.input.operate = 0;
    let mark = Vec3::new(1000.0, 120.0, 650.0);
    w.input.waypoint = Some(mark);
    step(&mut w);
    assert_eq!(w.waypoints[Team::Ember.idx()], Some(mark));
}
