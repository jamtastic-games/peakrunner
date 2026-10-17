//! QA-only, env-gated staging for screenshots, like `QA_FLYCAM`. Applied to the
//! client's view of the world every frame; the server never sees it. Absent the
//! env vars this does nothing.
//!
//! `QA_EQUIPMENT="3=120/0,5=250/200,gen0=down"`: set equipment `index=health/shield`;
//! `genN=down` destroys team N's generators so its circuit loses power.
//! `QA_PLAYERS="Ridge:0:1010,245,860;Echo:1:1040,245,870"`: stage extra players
//! (`name:team:x,y,z`, team 0 = Ember) for name-tag captures. A trailing `*` on
//! the name (`Budster*:0:...`) hands that player the enemy flag once
//! `QA_CARRY_AT` seconds (default 6) have passed, so the pickup announcement
//! fires through the normal state-change path before the 8 s capture. In
//! Football the starred player carries the ball.
//! `genN=boom` destroys team N's generators `QA_BOOM_AT` seconds in (default
//! 5): the hull drops to zero, the generator blast plays, and the wreck and
//! announcement follow through the normal paths. `QA_KIT=heal` shows the local
//! player repairing themselves (the repair tool's HUD and beam).
//! An optional fourth field stages a pose for animation captures: `dive`
//! (airborne, pitched down, firing), `land@T` (falling until `T` seconds,
//! then grounded: a hard landing) or `switch@T` (disc launcher until `T`,
//! then chaingun), `down@T` (tackled at `T`) or `dead@T` (killed at `T`). The
//! landing, switch and falls play through the client's normal frame-to-frame
//! animation tracker. `splash@T` treats `y` as a water
//! surface: the player skis along it 2 m up until `T` seconds, then drops
//! in at 22 m/s, so the entry splash plays through the normal effects path.
//! `skim` treats `y` as a skim-water surface (Reefbreak's sea) and skis the
//! player across it at 24 m/s, for spray and ripple captures.
//! `QA_BALL=x,y,z` rests a loose football there (Football only);
//! `QA_BALL=x,y,z,pass` makes it a pass on its way to you (`Ball::pass_to`).
//! `QA_RABBIT=1`: in Rabbit, the local player holds the flag (is the rabbit);
//! `QA_RABBIT=name` a staged player does, and `name:ping` with a ping running.
//! `QA_FULLMAP=1` opens the full map (the map key's overlay).
//! `QA_POINTS="Beacon:1010,120,860:drain;West:900,100,1000"` stages temporary
//! control points (`name:x,y,z[:drain][:rRADIUS]`) on any map, `QA_POINT_STATE="0=1/0.4/0/c"`
//! forces point `index=owner/progress/capturing[/c for contested]` (`-` = none),
//! and `QA_MODE=cnh` with `QA_SCORE=120,45` shows Capture & Hold on the client.
//! `QA_MODE=football` starts offline matches in Football on any map.
//! `QA_ROUNDS="1:1010,246,870:0,0,-300;0:..."` stages projectiles in flight
//! (`kind:x,y,z:vx,vy,vz`, kind 0 disc, 1 bullet, 2 grenade, 3 plasma). Each
//! loops along its velocity over 0.25 s so incoming fire can be captured.
//! `QA_BLASTS="0:1010,244,860@3;2:1016,244,866@3.2"` stages one explosion each
//! (`kind:x,y,z@seconds`) through the normal effects path, scorch included.
//! Duel servers (`launch_smoke` with `QA_LOCAL=1` and
//! `QA_VOTE_ROTATION='[{"playlist":"duel"}]'`): `QA_CROWD=N QA_DUELS=1`
//! pairs the crowd off into duels, `QA_DUEL=Rookie` (any bot archetype)
//! duels a bot once the board comes and `QA_DUEL=watch` watches the first
//! duel; `QA_VOTE_PANEL=1` opens the duel board.
//! `QA_HOST=<mode key>[,start]` opens the Host screen on that mode (`duel`)
//! and with `start` presses Host and join three seconds later (`app.rs`).
use glam::Vec3;
use peakrunner_core::sim::{Team, World};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
enum StagedPose { Stand, Dive, Land(f32), Switch(f32), Splash(f32), Skim, Run, Ski, Rise, Fall, Jet, Repair, Heal, Down(f32), Dead(f32) }

struct Overrides { equipment: Vec<(usize, f32, f32)>, down: Vec<u8>, boom: Vec<u8>, boom_at: f32, kit_heal: bool, players: Vec<(String, Team, Vec3, bool, StagedPose)>, started: std::time::Instant, carry_at: f32, rounds: Vec<(u8, Vec3, Vec3)>, blasts: Vec<(u8, Vec3, f32)> }

/// Marks QA-staged rounds so each frame replaces them rather than piling up.
const STAGED_ROUND_SPIN: f32 = -4242.0;

/// Whether any QA_ variable is set (read once), so normal play skips the
/// per-frame staging hooks and their environment lookups.
pub fn active() -> bool {
    static ACTIVE: OnceLock<bool> = OnceLock::new();
    // The browser has no environment (listing it panics there).
    *ACTIVE.get_or_init(|| !cfg!(target_arch = "wasm32") && std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("QA_")))
}

/// `QA_NOHUD=gun`: no HUD or markers, but the weapon in hand (weapon pictures).
pub fn keep_gun() -> bool { std::env::var("QA_NOHUD").is_ok_and(|v| v == "gun") }

/// `QA_FULLMAP=1`: the full map open (`map_view`), for captures.
pub fn full_map() -> bool {
    static FULL: OnceLock<bool> = OnceLock::new();
    *FULL.get_or_init(|| active() && std::env::var_os("QA_FULLMAP").is_some())
}

/// `QA_NOHUD=1`: draw only the world (no HUD, markers or viewmodel), for
/// map screenshots.
pub fn no_hud() -> bool {
    static NO_HUD: OnceLock<bool> = OnceLock::new();
    *NO_HUD.get_or_init(|| active() && std::env::var_os("QA_NOHUD").is_some())
}

fn parse() -> Option<&'static Overrides> {
    static CELL: OnceLock<Option<Overrides>> = OnceLock::new();
    CELL.get_or_init(|| {
        let eq = std::env::var("QA_EQUIPMENT").ok();
        let pl = std::env::var("QA_PLAYERS").ok();
        let kit_heal = std::env::var("QA_KIT").is_ok_and(|v| v == "heal");
        let rd = std::env::var("QA_ROUNDS").ok();
        let bl = std::env::var("QA_BLASTS").ok();
        if eq.is_none() && pl.is_none() && !kit_heal && rd.is_none() && bl.is_none() { return None; }
        let boom_at = std::env::var("QA_BOOM_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(5.0);
        let carry_at = std::env::var("QA_CARRY_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(6.0);
        let mut o = Overrides { equipment: Vec::new(), down: Vec::new(), boom: Vec::new(), boom_at, kit_heal, players: Vec::new(), started: std::time::Instant::now(), carry_at, rounds: Vec::new(), blasts: Vec::new() };
        let vec3 = |s: &str| -> Option<Vec3> {
            let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
        };
        for item in rd.iter().flat_map(|s| s.split(';')) {
            let parts: Vec<_> = item.split(':').collect();
            if parts.len() != 3 { continue; }
            if let (Ok(kind), Some(pos), Some(vel)) = (parts[0].trim().parse(), vec3(parts[1]), vec3(parts[2])) {
                o.rounds.push((kind, pos, vel));
            }
        }
        for item in bl.iter().flat_map(|s| s.split(';')) {
            let Some((head, at)) = item.split_once('@') else { continue; };
            let Some((kind, xyz)) = head.split_once(':') else { continue; };
            if let (Ok(kind), Some(pos), Ok(at)) = (kind.trim().parse(), vec3(xyz), at.trim().parse()) {
                o.blasts.push((kind, pos, at));
            }
        }
        for item in eq.iter().flat_map(|s| s.split(',')) {
            let Some((key, value)) = item.split_once('=') else { continue; };
            if let Some(team) = key.strip_prefix("gen") {
                if let Ok(t) = team.parse() { if value == "boom" { o.boom.push(t); } else { o.down.push(t); } }
                continue;
            }
            let (Ok(i), Some((h, s))) = (key.trim().parse(), value.split_once('/')) else { continue; };
            if let (Ok(h), Ok(s)) = (h.parse(), s.parse()) { o.equipment.push((i, h, s)); }
        }
        for item in pl.iter().flat_map(|s| s.split(';')) {
            let parts: Vec<_> = item.split(':').collect();
            if parts.len() != 3 && parts.len() != 4 { continue; }
            let xyz: Vec<f32> = parts[2].split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if xyz.len() != 3 { continue; }
            let team = if parts[1] == "0" { Team::Ember } else { Team::Glacier };
            let (name, carries) = match parts[0].strip_suffix('*') { Some(n) => (n, true), None => (parts[0], false) };
            let at = |tag: &str| parts[3].strip_prefix(tag).and_then(|t| t.parse().ok());
            let pose = match parts.get(3) {
                None => StagedPose::Stand,
                Some(&"dive") => StagedPose::Dive,
                Some(&"run") => StagedPose::Run,
                Some(&"ski") => StagedPose::Ski,
                Some(&"rise") => StagedPose::Rise,
                Some(&"fall") => StagedPose::Fall,
                Some(&"jet") => StagedPose::Jet,
                Some(&"repair") => StagedPose::Repair,
                Some(&"heal") => StagedPose::Heal,
                Some(&"skim") => StagedPose::Skim,
                Some(_) => at("land@").map(StagedPose::Land).or_else(|| at("switch@").map(StagedPose::Switch))
                    .or_else(|| at("down@").map(StagedPose::Down)).or_else(|| at("dead@").map(StagedPose::Dead))
                    .or_else(|| at("splash@").map(StagedPose::Splash)).unwrap_or(StagedPose::Stand),
            };
            o.players.push((name.to_string(), team, Vec3::new(xyz[0], xyz[1], xyz[2]), carries, pose));
        }
        Some(o)
    }).as_ref()
}

pub fn apply(world: &mut World) {
    stage_ball(world);
    stage_rabbit(world);
    let Some(o) = parse() else { return; };
    let defs = peakrunner_core::equipment::definitions(world.map);
    for &team in &o.down {
        for (d, s) in defs.iter().zip(&mut world.equipment) {
            if d.kind == peakrunner_core::equipment::Kind::Generator && d.team == team { s.health = 0.0; }
        }
    }
    let since_boom = o.started.elapsed().as_secs_f32() - o.boom_at;
    if since_boom >= 0.0 {
        for &team in &o.boom {
            for (d, s) in defs.iter().zip(&mut world.equipment) {
                if d.kind != peakrunner_core::equipment::Kind::Generator || d.team != team { continue; }
                s.health = 0.0; s.offline = true;
                // Snapshots replace the explosion list, so keep the blast alive
                // at its true age until it has played out.
                if since_boom < 0.55 && !world.explosions.iter().any(|e| e.kind == 4) {
                    world.explosions.push(peakrunner_core::sim::Explosion { pos: d.pos() + Vec3::Y * 2.0, age: since_boom,
                        max_r: peakrunner_core::sim::GENERATOR_BLAST_RADIUS, kind: 4 });
                }
            }
        }
    }
    peakrunner_core::equipment::power(defs, &mut world.equipment);
    if o.kit_heal {
        if let Some(p) = world.players.get_mut(world.player_id) { p.repair_beam = Some(p.pos + Vec3::Y * 0.9); p.repair_out = true; p.repair_self = true; p.health = 55.0; }
    }
    for &(i, h, s) in &o.equipment {
        if let Some(state) = world.equipment.get_mut(i) { state.health = h; if state.powered { state.shield = s; } }
    }
    for &(kind, pos, at) in &o.blasts {
        let age = o.started.elapsed().as_secs_f32() - at;
        if (0.0..0.55).contains(&age) && !world.explosions.iter().any(|e| e.kind == kind && e.pos.distance(pos) < 0.1) {
            world.explosions.push(peakrunner_core::sim::Explosion { pos, age, max_r: 9.0, kind });
        }
    }
    if !o.rounds.is_empty() {
        world.discs.retain(|d| d.spin != STAGED_ROUND_SPIN);
        let phase = (o.started.elapsed().as_secs_f32() % 0.25) - 0.125;
        for &(kind, pos, vel) in &o.rounds {
            let life = match kind { 0 => 4.0, 1 => 0.9, 2 => 1.5, _ => 2.0 };
            world.discs.push(peakrunner_core::sim::Disc { pos: pos + vel * phase, vel, team: Team::Glacier,
                owner: 0, life, kind, spin: STAGED_ROUND_SPIN, target: 0, power: 1.0 });
        }
    }
    let Some(template) = world.players.get(world.player_id).cloned() else { return; };
    let carry_now = o.started.elapsed().as_secs_f32() >= o.carry_at;
    let now = o.started.elapsed().as_secs_f32();
    for (k, (name, team, pos, carries, pose)) in o.players.iter().enumerate() {
        let id = 60_000 + k as u32;
        let slot = match world.players.iter().position(|p| p.net_id == id) {
            Some(i) => i,
            None => { world.players.push(template.clone()); world.players.len() - 1 }
        };
        let p = &mut world.players[slot];
        p.net_id = id; p.name = name.clone(); p.team = *team; p.pos = *pos; p.vel = Vec3::ZERO; p.alive = true;
        p.on_ground = true; p.jetting = false; p.skiing = false;
        match *pose {
            StagedPose::Stand => {}
            StagedPose::Repair => {
                let fwd = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
                p.repair_beam = Some(p.pos + fwd * 5.0 + Vec3::Y * 0.4);
                p.repair_out = true;
            }
            // The repair tool on themselves: the glow, no stream.
            StagedPose::Heal => { p.repair_beam = Some(p.pos + Vec3::Y * 0.9); p.repair_out = true; p.repair_self = true; }
            // Movement poses: moving along the player's facing.
            StagedPose::Run | StagedPose::Ski | StagedPose::Rise | StagedPose::Fall | StagedPose::Jet => {
                let fwd = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
                let (speed, vy, ground, ski, jet) = match pose {
                    StagedPose::Run => (11.0, 0.0, true, false, false),
                    StagedPose::Ski => (34.0, 0.0, true, true, false),
                    StagedPose::Rise => (9.0, 6.0, false, false, false),
                    StagedPose::Fall => (9.0, -9.0, false, false, false),
                    _ => (14.0, 7.0, false, false, true),
                };
                p.vel = fwd * speed + Vec3::Y * vy; p.on_ground = ground; p.skiing = ski; p.jetting = jet;
            }
            StagedPose::Dive => {
                p.on_ground = false; p.vel = Vec3::new(0.0, -24.0, -12.0); p.pitch = -1.0; p.weapon = 0;
                p.cooldown = peakrunner_core::sim::weapon_reload(0) - 0.03;
            }
            StagedPose::Land(t) => if now < t { p.on_ground = false; p.vel = Vec3::new(0.0, -28.0, 0.0); },
            StagedPose::Switch(t) => p.weapon = if now < t { 0 } else { 1 },
            // Tackled, or killed, at `t` seconds: the fall plays through the
            // client's normal tracker (`y` is the standing centre).
            StagedPose::Down(t) => if now >= t { p.stun = (peakrunner_core::sim::football::TACKLE_STUN - (now - t)).max(0.05); },
            StagedPose::Dead(t) => if now >= t { p.alive = false; p.respawn = (3.4 - (now - t)).max(0.5); },
            // Skiing across skim water at 24 m/s (y is the water surface).
            StagedPose::Skim => {
                p.on_ground = true; p.skiing = true; p.vel = Vec3::new(24.0, 0.0, 0.0);
                p.pos.y += peakrunner_core::terrain::PLAYER_RADIUS;
            }
            StagedPose::Splash(t) => {
                p.on_ground = false; p.skiing = true; p.vel = Vec3::new(22.0, -2.0, 0.0);
                p.pos.y += if now < t { 2.0 } else { -0.6 };
            }
        }
        if *carries && carry_now && world.ball.active {
            // Football: the starred player carries the ball instead.
            world.ball.in_play = true; world.ball.carrier = Some(slot);
        } else if *carries && carry_now {
            let enemy = team.other();
            p.carrying = Some(enemy);
            let f = &mut world.flags[enemy.idx()];
            f.carrier = Some(slot); f.pos = *pos + Vec3::Y * 2.2;
        }
    }
    // Again, now a staged rabbit is back in the list after a snapshot.
    stage_rabbit(world);
}

/// `QA_RABBIT=1|name[:ping]`: in Rabbit, who holds the flag (client view).
fn stage_rabbit(world: &mut World) {
    let Ok(v) = std::env::var("QA_RABBIT") else { return };
    if !world.rabbit_mode() { return; }
    let (name, ping) = v.split_once(':').map_or((v.as_str(), false), |(n, p)| (n, p == "ping"));
    let holder = if name == "1" { Some(world.player_id) } else { world.players.iter().position(|p| p.name == name) };
    let Some(i) = holder else { return };
    world.rabbit.active = true; world.rabbit.carrier = Some(i);
    world.rabbit.pos = world.players[i].pos + Vec3::Y * 2.2;
    world.rabbit.ping = if ping { 3.0 } else { 0.0 };
}

/// `QA_BALL=x,y,z[,pass]`: in Football, a loose ball resting there, or a
/// pass to you there (client view).
fn stage_ball(world: &mut World) {
    static AT: OnceLock<Option<(Vec3, bool)>> = OnceLock::new();
    let at = AT.get_or_init(|| {
        let raw = std::env::var("QA_BALL").ok()?;
        let v: Vec<f32> = raw.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| (Vec3::new(v[0], v[1], v[2]), raw.trim().ends_with(",pass")))
    });
    if let (Some((at, pass)), true) = (at, world.ball.active) {
        world.ball.in_play = true; world.ball.carrier = None; world.ball.pos = *at; world.ball.vel = Vec3::ZERO;
        world.ball.pass_to = pass.then_some(world.player_id);
    }
}

/// `QA_DRIVE="w@1-6;w,space@6-9;turn=90@3-5"`: holds keys (`w a s d`, `space`
/// to jump or ski, `jet`, `fire`, and `slot1` to `slot5` to raise what that
/// loadout slot carries) and turns (degrees a second, positive to the left)
/// over those seconds after the local player first stands in play (after
/// any round countdown: `waiting`), in place
/// of the keyboard and mouse, for recorded test runs (launch_smoke leaves the
/// window unfocused with it, so the real ones change nothing). True with a
/// script: the client then sends its inputs although unfocused.
#[cfg(not(target_arch = "wasm32"))]
pub fn drive(world: &mut World, dt: f32, waiting: bool) -> bool {
    struct Step { keys: Vec<String>, turn: f32, from: f32, to: f32 }
    static SCRIPT: OnceLock<Option<Vec<Step>>> = OnceLock::new();
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    let Some(script) = SCRIPT.get_or_init(|| {
        let raw = std::env::var("QA_DRIVE").ok()?;
        Some(raw.split(';').filter_map(|step| {
            let (what, span) = step.split_once('@')?;
            let (from, to) = span.split_once('-')?;
            let turn = what.strip_prefix("turn=").and_then(|d| d.trim().parse::<f32>().ok()).unwrap_or(0.0);
            let keys = if turn != 0.0 { Vec::new() } else { what.split(',').map(|k| k.trim().to_lowercase()).collect() };
            Some(Step { keys, turn, from: from.trim().parse().ok()?, to: to.trim().parse().ok()? })
        }).collect())
    }) else { return false };
    let standing = world.state == peakrunner_core::sim::MatchState::Playing && !waiting
        && world.players.get(world.player_id).is_some_and(|p| p.alive);
    if !standing && START.get().is_none() { return true; }
    let t = START.get_or_init(|| { eprintln!("QA_DRIVE: the local player stands in play; the script starts at {} ms", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())); std::time::Instant::now() }).elapsed().as_secs_f32();
    let (mut mx, mut mz, mut jump, mut jet, mut fire, mut turn) = (0.0, 0.0, false, false, false, 0.0);
    let mut slot = None;
    for step in script.iter().filter(|s| (s.from..s.to).contains(&t)) {
        turn += step.turn;
        for key in &step.keys {
            match key.as_str() {
                "w" => mz += 1.0, "s" => mz -= 1.0, "a" => mx -= 1.0, "d" => mx += 1.0,
                "space" => jump = true, "jet" => jet = true, "fire" => fire = true,
                k => if let Some(n) = k.strip_prefix("slot").and_then(|n| n.parse::<usize>().ok()) { slot = Some(n.saturating_sub(1)); },
            }
        }
    }
    world.input.move_x = mx;
    world.input.move_z = mz;
    world.input.jump = jump;
    world.input.jet = jet;
    // `QA_FIRE` still holds the trigger on its own.
    world.input.fire |= fire;
    if let Some(kind) = slot.and_then(|n| world.players.get(world.player_id).and_then(|p| p.loadout.slot(n))) { world.input.weapon = kind; }
    if let Some(p) = world.players.get_mut(world.player_id).filter(|p| p.alive) {
        p.yaw += turn.to_radians() * dt;
        // A line every quarter second (`QA_DRIVE_LOG`), to line clips up with.
        static LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(u32::MAX);
        let tick = (t * 4.0) as u32;
        if std::env::var_os("QA_DRIVE_LOG").is_some() && LOGGED.swap(tick, std::sync::atomic::Ordering::Relaxed) != tick {
            eprintln!("QA_DRIVE {t:5.2}s speed {:5.1} up {:5.1} ground {} ski {} at {:.1},{:.1},{:.1}",
                glam::Vec2::new(p.vel.x, p.vel.z).length(), p.vel.y, p.on_ground, p.skiing, p.pos.x, p.pos.y, p.pos.z);
        }
    }
    true
}

fn qa_points() -> Option<&'static Vec<peakrunner_core::control::Definition>> {
    static CELL: OnceLock<Option<Vec<peakrunner_core::control::Definition>>> = OnceLock::new();
    CELL.get_or_init(|| {
        let raw = std::env::var("QA_POINTS").ok()?;
        let defs: Vec<_> = raw.split(';').enumerate().filter_map(|(i, item)| {
            let parts: Vec<_> = item.split(':').collect();
            let xyz: Vec<f32> = parts.get(1)?.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if xyz.len() != 3 { return None; }
            let drain = parts.iter().any(|d| *d == "drain").then(|| peakrunner_core::control::Drain {
                radius: peakrunner_core::control::DEFAULT_DRAIN_RADIUS, rate: peakrunner_core::control::DEFAULT_DRAIN_RATE, airborne: false, damage: 0.0 });
            let radius = parts.iter().find_map(|p| p.strip_prefix('r').and_then(|r| r.parse().ok()))
                .unwrap_or(peakrunner_core::control::DEFAULT_RADIUS);
            Some(peakrunner_core::control::Definition { id: format!("qa-{i}"), name: parts[0].to_string(),
                pos: [xyz[0], xyz[1], xyz[2]], radius, ctf_active: true, drain, turret: None, capture: None, spires: Vec::new(), beacon: None, cook: None })
        }).collect();
        (!defs.is_empty()).then_some(defs)
    }).as_ref()
}

/// Number of QA-staged control points, if any are staged.
pub fn staged_point_count() -> Option<usize> { qa_points().map(|d| d.len()) }

/// `QA_MODE=football`: offline Football on any map (end zones fall back to the
/// flag stands) before a stadium exists, for captures.
pub fn football_staged() -> bool { std::env::var("QA_MODE").is_ok_and(|m| m == "football") }

/// Put QA-staged control points into a world (offline start and client view).
pub fn stage_points(world: &mut World) {
    if let Some(defs) = qa_points() { world.set_control_points(defs.clone()); }
}

/// Client-view staging for control points, mode and score; see the module docs.
pub fn apply_points(world: &mut World) {
    let Some(defs) = qa_points() else { apply_point_state(world); return; };
    if world.points.len() != defs.len() { world.set_control_points(defs.clone()); }
    if std::env::var("QA_MODE").is_ok_and(|m| m == "cnh") {
        world.mode = peakrunner_core::map_catalog::SupportedMode::CaptureAndHold;
        for p in &mut world.points { p.active = true; }
        if let Some(score) = std::env::var("QA_SCORE").ok().and_then(|s| {
            let v: Vec<u32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 2).then(|| [v[0], v[1]]) }) { world.score = score; }
    }
    apply_point_state(world);
}

/// `QA_POINT_STATE="0=1/0.4/0/c"`: owner / progress / capturing / contested
/// for a point, staged or from the map.
fn apply_point_state(world: &mut World) {
    for item in std::env::var("QA_POINT_STATE").unwrap_or_default().split(',') {
        let Some((i, v)) = item.split_once('=') else { continue; };
        let Ok(i) = i.trim().parse::<usize>() else { continue; };
        let Some(p) = world.points.get_mut(i) else { continue; };
        let f: Vec<&str> = v.split('/').collect();
        let team = |s: &str| s.parse::<u8>().ok().filter(|t| *t < 2);
        p.owner = f.first().and_then(|s| team(s));
        p.progress = f.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        p.capturing = f.get(2).and_then(|s| team(s));
        p.contested = f.get(3) == Some(&"c");
    }
}

/// Loadout staging for captures (client view only): `QA_DEPLOY="turret:0:x,y,z,yaw;wall:1:...;field:0:...;ammo:0:..."`
/// places deployables (team 0 = Ember, yaw in radians); `QA_HEAVY="Name,Name"`
/// puts those staged players in heavy armor (`QA_HEAVY=me` the local player);
/// `QA_MORTAR="x,y,z"` shows a mortar shell hanging there;
/// `QA_MORTAR_BLAST="x,y,z,age"` a mortar explosion that age (seconds) old.
/// `QA_THROWN="mine:0:x,y,z;grenade:1:x,y,z"` shows resting mines (armed)
/// and grenades; `QA_SHOP=1` opens the inventory screen (see app.rs);
/// `QA_EMPTY=1` empties the local player's weapons.
/// `QA_CONDITIONS="night,rain"` forces deathmatch conditions on the client
/// (time dawn/day/dusk/night, weather clear/overcast/rain/storm/snow/fog;
/// a view change only).
pub fn apply_loadout(world: &mut World) {
    use peakrunner_core::sim::deploy::{DeployKind, Deployable};
    if let Ok(raw) = std::env::var("QA_CONDITIONS") {
        use peakrunner_core::conditions::{TimeOfDay, Weather};
        let mut c = world.conditions;
        for word in raw.split(',') {
            match word.trim() {
                "dawn" => c.time = TimeOfDay::Dawn, "day" => c.time = TimeOfDay::Day,
                "dusk" => c.time = TimeOfDay::Dusk, "night" => c.time = TimeOfDay::Night,
                "clear" => c.weather = Weather::Clear, "overcast" => c.weather = Weather::Overcast,
                "rain" => c.weather = Weather::Rain, "storm" => c.weather = Weather::Storm,
                "snow" => c.weather = Weather::Snow, "fog" => c.weather = Weather::Fog,
                _ => {}
            }
        }
        if c.wind == [0.0, 0.0] { c.wind = [3.0, 1.5]; }
        world.conditions = c;
    }
    if let Ok(raw) = std::env::var("QA_DEPLOY") {
        world.deployables = raw.split(';').filter_map(|item| {
            let parts: Vec<_> = item.split(':').collect();
            let kind = match *parts.first()? { "turret" => DeployKind::Turret, "wall" => DeployKind::Wall, "field" => DeployKind::Field,
                "ammo" => DeployKind::Ammo, "inventory" => DeployKind::Inventory, _ => return None };
            let team = if *parts.get(1)? == "0" { Team::Ember } else { Team::Glacier };
            let v: Vec<f32> = parts.get(2)?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if v.len() != 4 { return None; }
            Some(Deployable { owner: 0, kind, team, pos: Vec3::new(v[0], v[1], v[2]), yaw: v[3],
                health: peakrunner_core::sim::deploy::max_health(kind), cooldown: 0.0, aim: Vec3::new(-v[3].sin(), 0.0, -v[3].cos()), age: 0.0 })
        }).collect();
    }
    if let Ok(raw) = std::env::var("QA_HEAVY") {
        for name in raw.split(',') {
            let slot = if name == "me" { Some(world.player_id) } else { world.players.iter().position(|p| p.name == name) };
            if let Some(p) = slot.and_then(|i| world.players.get_mut(i)) { p.armor = peakrunner_core::sim::ArmorClass::Heavy; }
        }
    }
    if let Some(at) = std::env::var("QA_MORTAR").ok().and_then(|s| {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2])) }) {
        if !world.discs.iter().any(|d| d.kind == peakrunner_core::sim::loadout::MORTAR_KIND) {
            world.discs.push(peakrunner_core::sim::Disc { pos: at, vel: Vec3::ZERO, team: Team::Glacier, owner: 0,
                life: peakrunner_core::defs::weapons().mortar.life, kind: peakrunner_core::sim::loadout::MORTAR_KIND, spin: 0.0, target: 0, power: 1.0 });
        }
    }
    if let Some(v) = std::env::var("QA_MORTAR_BLAST").ok().map(|s| s.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>())
        .filter(|v| v.len() == 4) {
        use peakrunner_core::sim::loadout::MORTAR_KIND;
        world.explosions.retain(|e| e.kind != MORTAR_KIND);
        world.explosions.push(peakrunner_core::sim::Explosion { pos: Vec3::new(v[0], v[1], v[2]), age: v[3],
            max_r: peakrunner_core::defs::defs().blasts.mortar.radius, kind: MORTAR_KIND });
    }
    // `QA_FLAG_DOWN="team:x,y,z"`: that team's flag (0 Ember) lies dropped there.
    if let Some((team, at)) = std::env::var("QA_FLAG_DOWN").ok().and_then(|raw| {
        let (t, xyz) = raw.split_once(':')?;
        let v: Vec<f32> = xyz.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 3).then(|| (t == "1", Vec3::new(v[0], v[1], v[2])))
    }) {
        if let Some(f) = world.flags.iter_mut().find(|f| (f.team == Team::Glacier) == team) {
            f.carrier = None;
            f.pos = at;
            f.drop_timer = 30.0;
        }
    }
    // `QA_RIFLE=1` buys and raises the local player's rifle (`=heavy` buys
    // heavy armor first, for the railgun); `QA_FIRE=1`
    // (app.rs) holds the trigger; `QA_ZOOM=1` looks through the sight (`=2`
    // at its second level); `QA_THIRD=1` uses the third-person view;
    // `QA_AIM="yaw,pitch"` or `"x,y,z,yaw,pitch"` (degrees) holds the local
    // view there (the elevation gauge, a sight on a staged player); the
    // place only after 3 s, once the spawn station has sold what was asked.
    // The local match is served, so the rifle is bought through the server
    // (the spawn stands at the inventory station) rather than granted.
    if let Ok(kind) = std::env::var("QA_RIFLE") {
        use peakrunner_core::sim::{arsenal::{family_weapon, Loadout, CHAINGUN, DISC, RIFLE}, loadout::Equip, ArmorClass};
        let armor = if kind == "heavy" { ArmorClass::Heavy } else { ArmorClass::Light };
        let rifle = family_weapon(armor, "rifle").unwrap_or(RIFLE);
        if let Some(p) = world.players.get(world.player_id) {
            if !p.loadout.carries(rifle) || p.armor != armor {
                world.input.equip = Some(Equip { armor: if armor == ArmorClass::Heavy { armor.0 + 1 } else { 0 },
                    weapons: Loadout::of(&[rifle, DISC, CHAINGUN]), pack: 0 });
            }
        }
        world.input.weapon = rifle;
    }
    // `QA_LOADOUT=plasma,blaster,disc` (weapon families, `arsenal::KIND_KEYS`
    // plus `drain` and `marker`; an armor id first, `heavy,` or `medium,`, for
    // that armor) equips those weapons at the station and holds the first.
    if let Ok(list) = std::env::var("QA_LOADOUT") {
        use peakrunner_core::sim::{arsenal::{family_weapon, Loadout}, loadout::Equip, ArmorClass};
        let armor = list.split(',').next().and_then(ArmorClass::by_id).unwrap_or(ArmorClass::Light);
        let kinds: Vec<u8> = list.split(',').filter_map(|f| family_weapon(armor, f)).collect();
        // Staged in the local view too (offline that's the match itself).
        if let Some(p) = world.players.get_mut(world.player_id) {
            if p.loadout != Loadout::of(&kinds) {
                world.input.equip = Some(Equip { armor: armor.0 + 1, weapons: Loadout::of(&kinds), pack: 0 });
                p.loadout = Loadout::of(&kinds);
                p.armor = armor;
            }
        }
        if let Some(&first) = kinds.first() {
            world.input.weapon = first;
            if let Some(p) = world.players.get_mut(world.player_id) { p.weapon = first; }
        }
    }
    if let Ok(level) = std::env::var("QA_ZOOM") { world.zoomed = true; world.zoom_level = u8::from(level == "2"); }
    if let Ok(raw) = std::env::var("QA_AIM") {
        static FIRST: OnceLock<std::time::Instant> = OnceLock::new();
        let settled = FIRST.get_or_init(std::time::Instant::now).elapsed().as_secs_f32() > 3.0;
        let v: Vec<f32> = raw.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let Some(p) = world.players.get_mut(world.player_id) {
            if v.len() == 5 && settled { p.pos = Vec3::new(v[0], v[1], v[2]); p.vel = Vec3::ZERO; }
            if let [.., yaw, pitch] = v[..] { p.yaw = yaw.to_radians(); p.pitch = pitch.to_radians(); }
        }
    }
    // `QA_VEHICLES="tank:0:x,g,z,yaw[:Name|me];cycle:1:x,y,z,yaw"`: vehicles
    // standing there (team 0 Ember, yaw in radians, `g` for y: on the ground,
    // tilted with it), each with that player at the controls.
    if let Ok(raw) = std::env::var("QA_VEHICLES") {
        use peakrunner_core::sim::vehicles::{Craft, MAX_SEATS};
        let r = peakrunner_core::registry::registry();
        for p in &mut world.players { p.riding = 0; }
        let mut crafts = Vec::new();
        for item in raw.split(';') {
            let parts: Vec<_> = item.split(':').collect();
            let (Some(kind), Some(team), Some(at)) = (parts.first().and_then(|k| r.vehicles.index(k)), parts.get(1), parts.get(2)) else { continue };
            let Some(def) = r.vehicles.get(kind) else { continue };
            let v: Vec<&str> = at.split(',').map(str::trim).collect();
            let num = |k: usize| v.get(k).and_then(|x| x.parse::<f32>().ok());
            let (Some(x), Some(z), Some(yaw)) = (num(0), num(2), num(3)) else { continue };
            let ground = |dx: f32, dz: f32| peakrunner_core::terrain::support_on(world.map, Vec3::new(x + dx, 2000.0, z + dz)).0;
            let (fwd, h) = (Vec3::new(-yaw.sin(), 0.0, -yaw.cos()), def.size);
            let (y, pitch) = match num(1) {
                Some(y) => (y, 0.0),
                None => (ground(0.0, 0.0) + h[1], ((ground(fwd.x * h[2], fwd.z * h[2]) - ground(-fwd.x * h[2], -fwd.z * h[2])) / (2.0 * h[2])).atan()),
            };
            let team = if *team == "0" { Team::Ember } else { Team::Glacier };
            let rider = parts.get(3).and_then(|n| if *n == "me" { Some(world.player_id) } else { world.players.iter().position(|p| p.name == *n) });
            let mut riders = [0u8; MAX_SEATS];
            if let Some(i) = rider { riders[0] = i as u8 + 1; }
            crafts.push(Craft { kind: kind as u8, team, pos: Vec3::new(x, y, z), vel: Vec3::ZERO, yaw, pitch, hull: def.hull, energy: def.energy_max,
                riders, cooldowns: [0.0; MAX_SEATS], idle: 0.0 });
        }
        for (v, c) in crafts.iter().enumerate() {
            if let Some(i) = c.rider(0) {
                let at = c.seat_at(0);
                let me = i == world.player_id;
                if let Some(p) = world.players.get_mut(i) {
                    p.riding = v as u8 + 1; p.pos = at; p.vel = Vec3::ZERO; p.on_ground = false;
                    // A staged rider looks along the nose; your own look stays yours.
                    if !me { p.yaw = c.yaw; p.pitch = 0.0; }
                }
            }
        }
        world.vehicles = crafts;
    }
    if std::env::var_os("QA_THIRD").is_some() { world.third_person = true; }
    if std::env::var_os("QA_EMPTY").is_some() {
        if let Some(p) = world.players.get_mut(world.player_id) { p.ammo.iter_mut().for_each(|n| *n = 0); }
    }
    if let Ok(raw) = std::env::var("QA_THROWN") {
        use peakrunner_core::sim::throwables::{GRENADE_KIND, MINE_KIND, is_throwable};
        world.discs.retain(|d| !is_throwable(d.kind));
        for item in raw.split(';') {
            let parts: Vec<_> = item.split(':').collect();
            let Some(kind) = parts.first().and_then(|k| match *k { "mine" => Some(MINE_KIND), "grenade" => Some(GRENADE_KIND), _ => None }) else { continue };
            let team = if parts.get(1) == Some(&"0") { Team::Ember } else { Team::Glacier };
            let v: Vec<f32> = parts.get(2).map_or(Vec::new(), |p| p.split(',').filter_map(|x| x.trim().parse().ok()).collect());
            if v.len() != 3 { continue; }
            world.discs.push(peakrunner_core::sim::Disc { pos: Vec3::new(v[0], v[1], v[2]), vel: Vec3::ZERO, team, owner: 0,
                life: if kind == MINE_KIND { peakrunner_core::defs::throws().mine_life - 5.0 } else { 1.0 }, kind, spin: 0.0, target: 0, power: 1.0 });
        }
    }
}

/// `QA_PLACING="wall:turn_deg:yaw_deg:pitch_deg:x,y,z"`: the local player
/// stands at x,y,z facing yaw/pitch, lining up that pack turned turn_deg (the
/// deploy hologram and deployer). Returns the pack and turn (radians).
pub fn placing(world: &mut World) -> Option<(peakrunner_core::sim::deploy::DeployKind, f32)> {
    use peakrunner_core::sim::deploy::DeployKind;
    let raw = std::env::var("QA_PLACING").ok()?;
    let parts: Vec<_> = raw.split(':').collect();
    let kind = match *parts.first()? { "turret" => DeployKind::Turret, "wall" => DeployKind::Wall, "field" => DeployKind::Field,
        "ammo" => DeployKind::Ammo, "inventory" => DeployKind::Inventory, _ => return None };
    let num = |k: usize| parts.get(k).and_then(|v| v.trim().parse::<f32>().ok());
    let (turn, yaw, pitch) = (num(1)?.to_radians(), num(2)?.to_radians(), num(3)?.to_radians());
    let at: Vec<f32> = parts.get(4)?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if at.len() != 3 { return None; }
    if let Some(p) = world.players.get_mut(world.player_id) {
        p.pos = Vec3::new(at[0], at[1], at[2]);
        p.vel = Vec3::ZERO;
        p.yaw = yaw;
        p.pitch = pitch;
    }
    Some((kind, turn))
}

