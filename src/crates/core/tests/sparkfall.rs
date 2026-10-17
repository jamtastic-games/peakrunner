//! The Sparkfall example mod (src/examples/mods/sparkfall): it loads, keeps
//! the study's balance rules (§8.3: resistances 0.6 to 1.4, nothing kills a
//! full-health light armor in one hit but a head shot, caps on the heaviest),
//! and its classes, weapons, gadgets and fliers do what they say.
//! Its own test binary: activating a mod changes the process's definitions.
use glam::Vec3;
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{deploy::{DeployKind, Deployable}, loadout, vehicles::BUY_VEHICLE, ArmorClass, Team, World, STEP};
use peakrunner_core::terrain::{MapId, PLAYER_RADIUS};
use peakrunner_core::{catalog, mods, registry};

const MAP: MapId = MapId::OldHoller;

fn ground(x: f32, z: f32) -> Vec3 { Vec3::new(x, peakrunner_core::terrain::support_on(MAP, Vec3::new(x, 400.0, z)).0, z) }

fn step(w: &mut World) { w.tick(STEP); }

/// Player 0 in `armor` on open ground facing -z; an enemy `ahead` metres in front.
fn duel(mode: SupportedMode, armor: &str, ahead: f32) -> World {
    let mut w = World::new();
    w.set_map(MAP);
    w.set_mode(mode);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    let class = ArmorClass::by_id(armor).unwrap();
    let p = &mut w.players[0];
    (p.team, p.armor, p.pos, p.vel, p.yaw, p.pitch) = (Team::Ember, class, ground(1060.0, 700.0) + Vec3::Y * PLAYER_RADIUS, Vec3::ZERO, 0.0, 0.0);
    p.energy = loadout::energy_max(class);
    p.loadout = peakrunner_core::sim::arsenal::Loadout::kit(class);
    p.ammo = loadout::rounds_max(p);
    p.weapon = p.loadout.kinds().next().unwrap();
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.armor = ArmorClass::by_id("courier").unwrap();
    foe.pos = ground(1060.0, 700.0 - ahead) + Vec3::Y * PLAYER_RADIUS;
    w.players.push(foe);
    w
}

#[test]
fn sparkfall_loads_balances_and_plays() {
    catalog::add_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples"));
    mods::activate_home(&["sparkfall".to_string()]).unwrap();
    let r = registry::registry();
    let ctf = SupportedMode::parse("sparkfall-ctf").expect("its mode");
    assert!(SupportedMode::parse("sparkfall-tdm").is_some());
    let classes = ["courier", "longshot", "veil", "striker", "bombard", "rigger", "warden", "bulwark", "siegeframe"];
    for c in classes { assert!(ctf.def().offers_armor(ArmorClass::by_id(c).unwrap()), "{c}"); }
    assert!(!ctf.def().offers_armor(ArmorClass::Light), "only Sparkfall's classes");

    // Balance (study §8.3).
    for c in classes {
        let a = ArmorClass::by_id(c).unwrap().def();
        for (kind, share) in &a.resist { assert!((0.6..=1.4).contains(share), "{c} resists {kind} {share}"); }
    }
    assert_eq!(ArmorClass::by_id("siegeframe").unwrap().def().team_cap, 1, "one Siegeframe a team");
    let lightest = classes.iter().map(|c| ArmorClass::by_id(c).unwrap().def().damage_taken).fold(0.0f32, f32::max);
    for (_, id, w) in r.weapons.iter() {
        let Some(round) = r.projectiles.by_id(&w.projectile) else { continue };
        let scale = if w.fire == registry::Fire::Charge { w.charge_scale } else { 1.0 };
        let direct = round.damage * round.pellets.max(1) as f32 * scale * lightest;
        let burst = round.blast.as_deref().and_then(|b| r.blasts.by_id(b)).map_or(0.0, |b| b.max_damage) * lightest;
        // Bases' weapons are the base game's own; the rule is for Sparkfall's.
        if registry::base().weapons.index(id).is_some() { continue; }
        assert!(direct < 100.0 && burst < 100.0, "{id}: {direct} direct, {burst} burst against a full-health light armor");
    }
    for (_, id, t) in r.throwables.iter() {
        if ["hand-grenade", "mine"].contains(&id) { continue; }
        let round = r.projectiles.by_id(&t.projectile).unwrap();
        let burst = round.blast.as_deref().and_then(|b| r.blasts.by_id(b)).map_or(0.0, |b| b.max_damage) * lightest;
        assert!(burst < 100.0, "{id}: {burst}");
    }

    // The Courier's Overdrive: past two uses within a minute or so, each
    // more one hurts.
    let mut w = duel(ctf, "courier", 200.0);
    let overstrained = r.effects.index("overstrained").unwrap() as u8;
    w.input.ability = true;
    step(&mut w);
    assert!(w.has_effect(0, r.effects.index("overdrive").unwrap() as u8));
    for _ in 0..2 {
        assert!(!w.has_effect(0, overstrained));
        w.players[0].ability_clock = 0.0;
        w.players[0].energy = 75.0;
        w.input.ability = true;
        step(&mut w);
    }
    assert!(w.has_effect(0, overstrained), "strain: {}", w.players[0].strain);

    // The Tether pulls whoever it hooks toward its shooter.
    let mut w = duel(ctf, "veil", 25.0);
    w.players[0].weapon = r.weapons.index("tether").unwrap() as u8;
    w.input.weapon = w.players[0].weapon;
    w.players[0].pitch = 0.0;
    let foe_z = w.players[1].pos.z;
    w.input.fire = true;
    let mut pulled = false;
    for _ in 0..20 {
        step(&mut w);
        w.input.fire = false;
        if w.players[1].vel.z > 10.0 { pulled = true; break; }
    }
    assert!(pulled, "dragged toward the shooter: {} (was at {foe_z})", w.players[1].vel);

    // Its push mode (X) shoves them away.
    let mut w = duel(ctf, "veil", 25.0);
    let tether = r.weapons.index("tether").unwrap() as u8;
    w.players[0].weapon = tether;
    w.input.weapon = tether;
    w.players[0].fire_mode = 1;
    w.players[0].mode_of = tether;
    w.players[0].pitch = 0.0;
    w.input.fire = true;
    let mut pushed = false;
    for _ in 0..20 {
        step(&mut w);
        w.input.fire = false;
        if w.players[1].vel.z < -10.0 { pushed = true; break; }
    }
    assert!(pushed, "shoved away from the shooter: {}", w.players[1].vel);
    let start = w.players[1].pos;
    for _ in 0..(1.0 / STEP) as usize { step(&mut w); }
    let moved = (w.players[1].pos - start).length();
    assert!(moved > 15.0, "a push carries them well off: {moved:.1} m in a second");

    // The Spark rifle's bolt leaves static on whoever it catches.
    let mut w = duel(ctf, "rigger", 20.0);
    w.players[0].weapon = r.weapons.index("spark-rifle").unwrap() as u8;
    w.input.weapon = w.players[0].weapon;
    w.players[0].pitch = -0.02;
    w.input.fire = true;
    let static_fx = r.effects.index("static").unwrap() as u8;
    let mut charged = false;
    for _ in 0..40 { step(&mut w); if w.has_effect(1, static_fx) { charged = true; break; } }
    assert!(charged, "static");

    // The Rigger's medkit lands as a repair kit.
    let mut w = duel(ctf, "rigger", 200.0);
    w.players[0].pitch = -0.4;
    w.input.ability = true;
    let kit = r.items.index("repair-kit").unwrap() as u8 + 1;
    let mut landed = false;
    for _ in 0..(5.0 / STEP) as usize { step(&mut w); if w.loot.iter().any(|l| l.item == kit) { landed = true; break; } }
    assert!(landed, "a repair kit on the ground");

    // A tripwire lands and becomes a sensor.
    let mut w = duel(ctf, "longshot", 200.0);
    w.players[0].pitch = -0.4;
    w.input.throw = peakrunner_core::sim::throwables::THROW_MINE;
    w.input.throw_strength = 0.3;
    let wire = DeployKind::from_key("tripwire").unwrap();
    let mut placed = false;
    for _ in 0..(5.0 / STEP) as usize { step(&mut w); if w.deployables.iter().any(|d| d.kind == wire) { placed = true; break; } }
    assert!(placed, "a tripwire standing");

    // The Multitool's claim turns an enemy unit after five seconds of beam.
    let mut w = duel(ctf, "rigger", 200.0);
    let tool = r.weapons.index("multitool").unwrap() as u8;
    w.players[0].weapon = tool;
    w.input.weapon = tool;
    w.players[0].fire_mode = 1;
    w.players[0].mode_of = tool;
    let eye = ground(1060.0, 692.0);
    w.deployables.push(Deployable { owner: 0, kind: DeployKind::from_key("eye").unwrap(), team: Team::Glacier, pos: eye, yaw: 0.0,
        health: 70.0, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
    let target = eye + Vec3::Y * 0.9 - (w.players[0].pos + Vec3::Y * peakrunner_core::terrain::EYE);
    w.players[0].pitch = (target.y / target.length()).asin();
    w.input.fire = true;
    for _ in 0..(5.5 / STEP) as usize { w.players[0].energy = 90.0; step(&mut w); }
    assert_eq!(w.deployables.last().unwrap().team, Team::Ember, "claimed");

    // The Bulwark's plate stops most of what comes from ahead.
    let w = duel(ctf, "bulwark", 30.0);
    assert!(w.blocked_share(0, Vec3::Z) < 0.5 && w.blocked_share(0, -Vec3::Z) == 1.0);

    // A flier bay hands out the mod's skiff, with its pilot at the controls.
    let mut w = duel(ctf, "rigger", 200.0);
    let bay = DeployKind::from_key("flier-bay").unwrap();
    let at = w.players[0].pos - Vec3::Y * PLAYER_RADIUS;
    w.deployables.push(Deployable { owner: 0, kind: bay, team: Team::Ember, pos: at, yaw: 0.0, health: 600.0, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
    w.input.buy = BUY_VEHICLE + r.vehicles.index("sparkfall-skiff").unwrap() as u16;
    step(&mut w);
    assert_eq!(w.riding(0), Some((0, 0)));
    assert!(w.credits_of(0) < 800.0, "paid from the team purse");
}
