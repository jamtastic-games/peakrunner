//! Vehicles from the stock library (docs/mod-format.md): bought at a vehicle
//! pad with the buyer at the controls, flown by the movement and jet keys
//! toward where the pilot looks (the pilot's prediction matches the
//! server), its seat's gun fired, left and boarded again, falling when
//! nobody flies it, and wrecked with its riders inside. A tank keeps to the
//! ground, turns with the sideways keys and is predicted the same way; a
//! cycle's rider sits in the open; vehicles bump each other and run over
//! enemies on foot.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{deploy::{DeployKind, Deployable}, vehicles::BUY_VEHICLE, Command, Disc, Team, World, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::registry;

fn step(w: &mut World) { w.tick(STEP); }

/// Player 0 on a vehicle pad on open ground, in a skiff they just bought.
fn flying() -> World { bought("skiff") }

/// Player 0 on a vehicle pad on open ground, in a `vehicle` they just bought.
fn bought(vehicle: &str) -> World {
    let map = MapId::OldHoller;
    let mut w = World::new();
    w.set_map(map);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    let ground = Vec3::new(1060.0, peakrunner_core::terrain::support_on(map, Vec3::new(1060.0, 400.0, 700.0)).0, 700.0);
    let kind = DeployKind::from_key("vehicle-pad").unwrap();
    w.deployables.push(Deployable { owner: 0, kind, team: Team::Ember, pos: ground, yaw: 0.0, health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
    w.players[0].pos = ground + Vec3::Y * (0.3 + PLAYER_RADIUS);
    w.players[0].yaw = 0.0;
    w.players[0].pitch = 0.0;
    let kind = registry::registry().vehicles.index(vehicle).unwrap() as u16;
    w.input.buy = BUY_VEHICLE + kind;
    step(&mut w);
    w.input.buy = 0;
    w
}

/// Where vehicle `v`'s underside would rest on the ground under it.
fn resting(w: &World, v: usize) -> f32 {
    let c = &w.vehicles[v];
    let half = c.def().unwrap().size[1];
    peakrunner_core::terrain::support_on(w.map, c.pos + Vec3::Y * (1.2 - half)).0 + half
}

/// A second player, an enemy on foot, `ahead` metres in front of player 0's vehicle.
fn enemy_ahead(w: &mut World, ahead: f32) -> usize {
    let mut p = w.players[0].clone();
    p.team = Team::Glacier;
    p.riding = 0;
    p.net_id += 1;
    let c = &w.vehicles[0];
    let at = c.pos + Vec3::new(-c.yaw.sin(), 0.0, -c.yaw.cos()) * ahead;
    p.pos = Vec3::new(at.x, peakrunner_core::terrain::support_on(w.map, at + Vec3::Y * 3.0).0, at.z);
    p.vel = Vec3::ZERO;
    w.players.push(p);
    w.players.len() - 1
}

#[test]
fn vehicles_fly_fire_and_wreck() {
    let mut w = flying();
    assert_eq!(w.vehicles.len(), 1, "bought");
    assert_eq!(w.riding(0), Some((0, 0)), "at the controls");
    // Forward along the nose (yaw 0: toward -z), and up with the jet key.
    let start = w.vehicles[0].pos;
    w.input.move_z = 1.0;
    w.input.jet = true;
    for _ in 0..90 { step(&mut w); }
    let c = &w.vehicles[0];
    assert!(c.pos.z < start.z - 15.0 && c.pos.y > start.y + 3.0, "flew: {} from {start}", c.pos);
    assert!(w.players[0].pos.distance(c.seat_at(0)) < 0.01, "the pilot rides in the seat");
    // Its gun fires along the pilot's view.
    w.input.move_z = 0.0;
    w.input.jet = false;
    w.input.fire = true;
    step(&mut w);
    w.input.fire = false;
    let bolt = registry::registry().projectiles.index("blaster-bolt").unwrap() as u8;
    assert!(w.discs.iter().any(|d| d.owner == 0 && d.kind == bolt), "the seat's blaster");
    // Out, and it drops with nobody flying it; back in.
    w.input.board = true;
    step(&mut w);
    assert_eq!(w.riding(0), None);
    let high = w.vehicles[0].pos.y;
    for _ in 0..90 { step(&mut w); }
    assert!(w.vehicles[0].pos.y < high - 1.0, "falls unflown: {} from {high}", w.vehicles[0].pos.y);
    w.players[0].pos = w.vehicles[0].pos + Vec3::new(2.0, 0.0, 0.0);
    w.input.board = true;
    step(&mut w);
    assert_eq!(w.riding(0), Some((0, 0)), "boarded");
    // Wrecked: its pilot dies with it.
    w.damage_vehicle(0, 10_000.0, None);
    assert!(w.vehicles.is_empty() && !w.players[0].alive);
}

#[test]
fn the_pilot_predicts_the_flight_the_server_flies() {
    let run = |predict: bool| {
        let mut w = flying();
        let c0 = w.vehicles[0].clone();
        for tick in 0..120u64 {
            let cmd = Command { seq: tick + 1, move_z: if tick < 60 { 1.0 } else { -0.5 }, move_x: if tick % 40 < 20 { 0.6 } else { -0.4 },
                jet: tick > 30 && tick < 90, yaw: 0.002 * tick as f32, pitch: 0.1, weapon: w.players[0].weapon, ..Command::default() };
            if predict { w.predict_command(cmd); }
            else {
                w.input = cmd.input();
                w.players[0].yaw = cmd.yaw;
                w.players[0].pitch = cmd.pitch;
                step(&mut w);
            }
        }
        let c = &w.vehicles[0];
        assert!(c.pos.distance(c0.pos) > 5.0);
        (c.pos.to_array().map(f32::to_bits), c.vel.to_array().map(f32::to_bits), c.yaw.to_bits(), c.energy.to_bits())
    };
    assert_eq!(run(true), run(false));
}

#[test]
fn tanks_keep_to_the_ground_turn_with_the_keys_and_hold_on_slopes() {
    let mut w = bought("tank");
    assert_eq!(w.riding(0), Some((0, 0)), "at the controls");
    assert!(w.vehicles[0].grounded());
    // Bought over the pad, it settles onto the ground and stays put.
    for _ in 0..120 { step(&mut w); }
    let parked = w.vehicles[0].pos;
    assert!((parked.y - resting(&w, 0)).abs() < 0.05, "on the ground: {} vs {}", parked.y, resting(&w, 0));
    for _ in 0..120 { step(&mut w); }
    assert!(w.vehicles[0].pos.distance(parked) < 0.05, "held by its brakes: {} from {parked}", w.vehicles[0].pos);
    // The jet key lifts nothing.
    w.input.jet = true;
    for _ in 0..60 { step(&mut w); }
    w.input.jet = false;
    assert!((w.vehicles[0].pos.y - resting(&w, 0)).abs() < 0.05, "no lift");
    // Forward along its heading (yaw 0: toward -z), on the ground all the way.
    let start = w.vehicles[0].pos;
    w.input.move_z = 1.0;
    for _ in 0..180 {
        step(&mut w);
        assert!((w.vehicles[0].pos.y - resting(&w, 0)).abs() < 0.6, "keeps to the ground at {}", w.vehicles[0].pos);
    }
    let c = &w.vehicles[0];
    let speed = Vec3::new(c.vel.x, 0.0, c.vel.z).length();
    assert!(c.pos.z < start.z - 20.0 && (c.pos.x - start.x).abs() < 3.0, "drove ahead: {} from {start}", c.pos);
    assert!(speed <= c.def().unwrap().flight.top_speed + 0.01 && speed > 10.0, "near its top speed: {speed}");
    // The sideways keys turn it; where the driver looks doesn't.
    w.input.move_z = 0.0;
    w.input.move_x = 1.0;
    w.players[0].yaw = 2.0;
    let yaw = w.vehicles[0].yaw;
    for _ in 0..60 { step(&mut w); }
    w.input.move_x = 0.0;
    let turned = (yaw - w.vehicles[0].yaw).rem_euclid(std::f32::consts::TAU).to_degrees();
    assert!((turned - 55.0).abs() < 1.0, "turned right {turned} degrees in a second");
    // The cannon fires where the driver looks, not along the hull.
    w.players[0].yaw = 1.0;
    w.players[0].pitch = 0.05;
    w.input.fire = true;
    step(&mut w);
    w.input.fire = false;
    let shell = registry::registry().projectiles.index("tank-shell").unwrap() as u8;
    let d = w.discs.iter().find(|d| d.owner == 0 && d.kind == shell).expect("the cannon fired");
    let aim = Vec3::new(-(1.0f32).sin(), 0.0, -(1.0f32).cos());
    assert!(Vec3::new(d.vel.x, 0.0, d.vel.z).normalize().dot(aim) > 0.99, "along the view");
    // Unpiloted, it rolls to a stop on the ground.
    w.input.board = true;
    step(&mut w);
    assert_eq!(w.riding(0), None);
    for _ in 0..240 { step(&mut w); }
    assert!(w.vehicles[0].vel.length() < 0.2 && (w.vehicles[0].pos.y - resting(&w, 0)).abs() < 0.05, "stopped on the ground");
}

#[test]
fn the_driver_predicts_the_drive_the_server_drives() {
    let run = |predict: bool| {
        let mut w = bought("tank");
        for _ in 0..60 { step(&mut w); }
        let c0 = w.vehicles[0].clone();
        for tick in 0..240u64 {
            let cmd = Command { seq: tick + 1, move_z: if tick < 150 { 1.0 } else { -0.6 }, move_x: if tick % 50 < 25 { 0.8 } else { -0.5 },
                jet: tick % 7 == 0, yaw: 0.004 * tick as f32, pitch: 0.05, weapon: w.players[0].weapon, ..Command::default() };
            if predict { w.predict_command(cmd); }
            else {
                w.input = cmd.input();
                w.players[0].yaw = cmd.yaw;
                w.players[0].pitch = cmd.pitch;
                step(&mut w);
            }
        }
        let c = &w.vehicles[0];
        assert!(c.pos.distance(c0.pos) > 10.0);
        (c.pos.to_array().map(f32::to_bits), c.vel.to_array().map(f32::to_bits), c.yaw.to_bits(), c.pitch.to_bits(), c.energy.to_bits())
    };
    assert_eq!(run(true), run(false));
}

#[test]
fn a_cycle_rider_sits_in_the_open_and_a_tank_crew_inside() {
    let bullet = registry::registry().projectiles.index("bullet").unwrap() as u8;
    for (vehicle, exposed) in [("cycle", true), ("tank", false)] {
        let mut w = bought(vehicle);
        for _ in 0..30 { step(&mut w); }
        assert_eq!(w.rides_exposed(0), exposed, "{vehicle}");
        let e = enemy_ahead(&mut w, 30.0);
        // A round from the enemy at the rider's chest, or the tank's middle.
        let chest = if exposed { w.players[0].pos + Vec3::Y * 1.45 } else { w.vehicles[0].pos };
        let from = chest + (w.players[e].pos + Vec3::Y * 1.5 - chest).normalize() * 12.0;
        let (health, hull) = (w.players[0].health, w.vehicles[0].hull);
        w.discs.push(Disc { pos: from, vel: (chest - from).normalize() * 420.0, team: Team::Glacier, owner: e, life: 1.0, kind: bullet, spin: 0.0, target: 0, power: 1.0 });
        for _ in 0..3 { step(&mut w); }
        if exposed { assert!(w.players[0].health < health, "{vehicle}: the rider was hit"); }
        else { assert!(w.players[0].health == health && w.vehicles[0].hull < hull, "{vehicle}: the hull took it"); }
    }
}

#[test]
fn vehicles_bump_and_run_over_enemies_on_foot() {
    // Two tanks in one place push apart.
    let mut w = bought("tank");
    for _ in 0..30 { step(&mut w); }
    let mut other = w.vehicles[0].clone();
    other.riders = [0; 6];
    other.pos.x += 1.0;
    w.vehicles.push(other);
    step(&mut w);
    let (a, b) = (&w.vehicles[0], &w.vehicles[1]);
    assert!(a.pos.distance(b.pos) >= a.radius() + b.radius() - 0.01, "apart: {}", a.pos.distance(b.pos));
    // A tank at full speed into an enemy on foot: shoved and hurt.
    let mut w = bought("tank");
    for _ in 0..30 { step(&mut w); }
    let e = enemy_ahead(&mut w, 4.5);
    let health = w.players[e].health;
    let c = &mut w.vehicles[0];
    c.vel = Vec3::new(-c.yaw.sin(), 0.0, -c.yaw.cos()) * 20.0;
    w.input.move_z = 1.0;
    for _ in 0..20 { step(&mut w); }
    assert!(w.players[e].health < health - 40.0 || !w.players[e].alive, "run over: {} of {health}", w.players[e].health);
    // A teammate is only shoved.
    let mut w = bought("tank");
    for _ in 0..30 { step(&mut w); }
    let f = enemy_ahead(&mut w, 4.5);
    w.players[f].team = Team::Ember;
    let (health, at) = (w.players[f].health, w.players[f].pos);
    let c = &mut w.vehicles[0];
    c.vel = Vec3::new(-c.yaw.sin(), 0.0, -c.yaw.cos()) * 20.0;
    for _ in 0..20 { step(&mut w); }
    assert!(w.players[f].health == health && w.players[f].pos.distance(at) > 1.0, "shoved, unhurt");
}


#[test]
fn a_vehicle_left_empty_goes_after_its_abandon_time() {
    let mut w = bought("cycle");
    let abandon = w.vehicles[0].def().unwrap().abandon;
    assert!(abandon > 0.0);
    w.input.board = true;
    step(&mut w);
    w.input.board = false;
    let steps = (abandon / STEP) as usize;
    for _ in 0..steps - 30 { step(&mut w); }
    assert_eq!(w.vehicles.len(), 1, "still there just before");
    for _ in 0..60 { step(&mut w); }
    assert!(w.vehicles.is_empty(), "gone once left empty {abandon} s");
}

/// Getting out checks the whole body, beside the vehicle and over it: on
/// the right, else the left, else on top; with no room anywhere the rider
/// stays in. (Only the feet were checked, and the top not at all.)
#[test]
fn a_rider_gets_out_where_the_body_fits() {
    use peakrunner_core::terrain::Pillar;
    // Out with walls (columns) in the way, from where the vehicle is now;
    // where the rider ends up, from the vehicle's middle, if out.
    let leave = |walls: &[f32]| -> Option<Vec3> {
        let mut w = flying();
        let c = w.vehicles[0].clone();
        let reach = c.def().unwrap().size[0] + 1.4;
        w.pillars = walls.iter().map(|&side| Pillar { x: c.pos.x + side * reach, z: c.pos.z, r: 1.0, h: 60.0 }).collect();
        if walls.contains(&0.0) { w.pillars.push(Pillar { x: c.pos.x, z: c.pos.z, r: 20.0, h: 60.0 }); }
        w.input.board = true;
        step(&mut w);
        w.riding(0).is_none().then(|| w.players[0].pos - w.vehicles[0].pos)
    };
    let out = leave(&[]).expect("out");
    assert!(out.x > 1.0 && out.y.abs() < 1.0, "on the right: {out}");
    let out = leave(&[1.0]).expect("out");
    assert!(out.x < -1.0 && out.y.abs() < 1.0, "a wall on the right: on the left, {out}");
    let out = leave(&[1.0, -1.0]).expect("out");
    assert!(out.x.abs() < 0.5 && out.y > 1.0, "walls both sides: on top, {out}");
    assert_eq!(leave(&[0.0]), None, "boxed in: stays in");
}

/// Landed, every stock vehicle lets its rider out: a cycle's middle rests
/// lower above the ground than a body's radius, so an exit checked from
/// there started inside the ground and was refused every way.
#[test]
fn a_landed_vehicle_lets_its_rider_out() {
    for vehicle in ["skiff", "transport", "cycle", "tank"] {
        let mut w = bought(vehicle);
        for _ in 0..600 { step(&mut w); }
        let rest = resting(&w, 0);
        assert!((w.vehicles[0].pos.y - rest).abs() < 0.05, "{vehicle} landed: {} vs {rest}", w.vehicles[0].pos.y);
        w.input.board = true;
        step(&mut w);
        assert_eq!(w.riding(0), None, "{vehicle}: out");
        let out = w.players[0].pos - w.vehicles[0].pos;
        assert!(out.with_y(0.0).length() > 1.0, "{vehicle}: beside it, not dropped on top: {out}");
        let ground = peakrunner_core::terrain::support_on(w.map, w.players[0].pos + Vec3::Y).0;
        assert!(w.players[0].pos.y >= ground + PLAYER_RADIUS - 0.05, "{vehicle}: on the ground, not in it");
    }
}

/// What would stop a body walking stops one getting out too: an enemy's
/// force field on the right sends the rider out on the left (their own
/// team's lets them through), and so does a vehicle parked there. (Only
/// the map was checked: riders got out inside a field or a hull.)
#[test]
fn a_rider_gets_out_clear_of_deployables_and_vehicles() {
    let leave = |field: Option<Team>, parked: bool| -> Vec3 {
        let mut w = flying();
        let c = w.vehicles[0].clone();
        let right = c.pos + Vec3::X * (c.def().unwrap().size[0] + 1.4);
        if let Some(team) = field {
            let kind = DeployKind::from_key("field").unwrap();
            let ground = peakrunner_core::terrain::support_on(w.map, right + Vec3::Y * 5.0).0;
            w.deployables.push(Deployable { owner: 0, kind, team, pos: right.with_y(ground), yaw: 0.0, health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
        }
        if parked {
            let mut other = c.clone();
            other.pos = right;
            other.riders = Default::default();
            w.vehicles.push(other);
        }
        w.input.board = true;
        step(&mut w);
        assert_eq!(w.riding(0), None, "out");
        w.players[0].pos - w.vehicles[0].pos
    };
    assert!(leave(None, false).x > 1.0, "nothing there: on the right");
    assert!(leave(Some(Team::Glacier), false).x < -1.0, "an enemy field on the right: on the left");
    assert!(leave(Some(Team::Ember), false).x > 1.0, "their own team's field lets them through");
    assert!(leave(None, true).x < -1.0, "a vehicle parked on the right: on the left");
}

/// A rider is never set down inside what their vehicle is already in: a
/// cycle nosed into a transport lets them out on the other side, and one
/// landed in an enemy's force field keeps them in (the overlap at the
/// start used to excuse the thing for every way out). Online, the refusal
/// is a notice the server sends: the rider's own game never runs it.
#[test]
fn a_rider_is_never_set_down_inside_what_their_vehicle_is_in() {
    let mut w = bought("cycle");
    let c = w.vehicles[0].clone();
    let mut transport = bought("transport").vehicles[0].clone();
    transport.riders = Default::default();
    // Lengthwise on the cycle's right, just clear of the bump (the hulls'
    // round bounds) yet near enough that the rider's body starts within
    // reach of the transport's nose.
    (transport.pos, transport.yaw, transport.vel) = (c.pos + Vec3::X * 4.3, std::f32::consts::FRAC_PI_2, Vec3::ZERO);
    w.vehicles.push(transport);
    w.input.board = true;
    step(&mut w);
    assert_eq!(w.riding(0), None, "out");
    let out = w.players[0].pos - w.vehicles[0].pos;
    assert!(out.x < -1.0, "the transport fills the right: on the left, {out}");

    let mut w = bought("cycle");
    for _ in 0..600 { step(&mut w); }
    let c = w.vehicles[0].clone();
    let kind = DeployKind::from_key("field").unwrap();
    let ground = peakrunner_core::terrain::support_on(w.map, c.pos + Vec3::Y).0;
    w.deployables.push(Deployable { owner: 0, kind, team: Team::Glacier, pos: c.pos.with_y(ground), yaw: 0.0, health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
    (w.players[0].remote, w.players[0].net_id) = (true, 5);
    w.input.board = true;
    step(&mut w);
    assert!(w.riding(0).is_some(), "landed in the field: stays in, not out at {}", w.players[0].pos - w.vehicles[0].pos);
    assert_eq!(w.notices, [(0, "NO ROOM TO GET OUT".to_string())], "told as a notice");
}
