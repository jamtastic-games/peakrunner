//! Client-only presentation and input prediction. Outcomes always come from snapshots.
//!
//! The local player is predicted: one simulation step per command sent,
//! replayed on top of each acknowledged snapshot. Other players are drawn
//! INTERP_DELAY behind an estimate of the server's clock, between the two
//! snapshots around that time, so 20 Hz snapshots arriving with jitter still
//! give smooth motion; if none is new enough, they coast on their last
//! velocity for up to MAX_EXTRAPOLATE, and a respawn, teleport or new round
//! snaps instead of sliding. Vehicles others drive are drawn the same way;
//! the one you drive is predicted with you.
//!
//! Football draws the others as the classic engine did (`place_ahead`):
//! coasted on from the newest snapshot (`football::coast`) to where they'd
//! be at your own body's moment, so a straight run or a flight through the
//! air (no air control) is drawn about where it really is; a turn or a jet
//! the guess missed slides over when the next snapshot shows it. The loose
//! ball goes ahead the same way. Commands claim the snapshot and how far
//! on, so the server can hit a bot where you saw it (`sim::lagcomp`).
use std::collections::VecDeque;
use std::time::Instant;
use glam::Vec3;
use crate::sim::{Command, Phase, Snapshot, World, MAX_PLAYERS, STEP};

/// How far behind the server's clock other players are drawn (two snapshot
/// intervals: one late or lost snapshot still leaves a pair to blend).
const INTERP_DELAY: f64 = 0.1;
/// Longest a remote player coasts past the newest snapshot.
const MAX_EXTRAPOLATE: f64 = 0.1;
/// Snapshots of remote poses kept (one second at 20 Hz).
const HISTORY: usize = 20;
/// Longest a loose football flies on past the newest snapshot.
const MAX_BALL_AHEAD: f32 = 0.25;
/// Football: farthest past the newest snapshot the others are drawn (s).
const MAX_LEAD: f32 = 0.25;
/// Football: a wrong guess slides off with this time constant (s), mostly
/// gone in 0.15 s; farther off than SNAP (m), a respawn or teleport, it
/// snaps.
const SLIDE: f32 = 0.05;
const SNAP: f32 = 5.0;

/// Where one player slot was at one snapshot.
#[derive(Clone, Copy)]
struct Pose { net_id: u32, alive: bool, pos: Vec3, vel: Vec3, yaw: f32, pitch: f32 }
/// Where one vehicle was at one snapshot (by its place in the list, checked
/// against its kind and team).
#[derive(Clone, Copy)]
struct Ride { kind: u8, team: crate::sim::Team, pos: Vec3, vel: Vec3, yaw: f32, pitch: f32 }
struct Frame { time: f64, poses: Vec<Pose>, rides: Vec<Ride> }
/// Football: where a remote slot was drawn last frame, from which snapshot
/// (its time), and the correction still sliding off it (`place_ahead`).
#[derive(Clone, Copy)]
struct Drawn { net_id: u32, base: f64, pos: Vec3, yaw: f32, pitch: f32, slide: Vec3, turn: f32, tilt: f32 }

pub struct Online {
    pub tick: u64,
    pub round: u32,
    pub phase: Phase,
    pub latency_ms: f32,
    seq: u64,
    acc: f32,
    pending: VecDeque<(Command, Instant)>,
    /// Recent snapshots' remote poses, oldest first.
    history: VecDeque<Frame>,
    /// Estimated server time now (s): advanced by frame time, pulled gently
    /// toward each snapshot's time as it arrives.
    clock: f64,
    balance_notice_until: Option<Instant>,
    /// The newest snapshot's `phase_left` and its server time (s).
    phase_left: f32,
    phase_time: f64,
    /// When the player last asked to switch teams: a team change soon after
    /// is theirs, not the auto-balancer's.
    switch_asked: Option<Instant>,
    switch_notice: bool,
    /// Seconds the loose football has flown on since the newest snapshot.
    ball_flown: f32,
    /// Football: each remote slot as drawn last frame.
    drawn: Vec<Option<Drawn>>,
    /// Football: how far past the newest snapshot (s) the others were drawn
    /// last frame: your own body's moment.
    lead: f32,
}

/// A loose football between snapshots (`dt` on, `flown` so far): along its
/// flight under gravity, resting on the ground below it, for at most
/// MAX_BALL_AHEAD. Snapshots come 20 times a second, and a hard pass drawn
/// only where they put it jumped metres at a time; the next one puts it
/// where the server has it (bounces included).
fn advance_ball(ball: &mut crate::sim::football::Ball, map: peakrunner_core::terrain::MapId, dt: f32, flown: &mut f32) {
    if !ball.loose() || ball.vel == Vec3::ZERO || *flown >= MAX_BALL_AHEAD { return; }
    let dt = dt.min(MAX_BALL_AHEAD - *flown);
    *flown += dt;
    ball.vel.y -= peakrunner_core::defs::movement().gravity * dt;
    ball.pos += ball.vel * dt;
    let floor = peakrunner_core::terrain::support_on(map, ball.pos + Vec3::Y * 0.5).0 + crate::sim::football::BALL_RADIUS;
    if ball.pos.y < floor { ball.pos.y = floor; ball.vel.y = ball.vel.y.max(0.0); }
}

/// A snapshot from the server made safe to apply. Indices it names (flag
/// and ball carriers, projectile owners, point owners) must be in range and
/// numbers finite: bad references and bad effects are dropped, and a
/// snapshot whose players, flags, ball or equipment aren't finite is refused
/// (None). A hostile or broken server can't make the client panic.
fn sanitize(state: &Snapshot) -> Option<Snapshot> {
    let finite = |v: Vec3| v.is_finite();
    if state.players.len() != MAX_PLAYERS || state.acks.len() != MAX_PLAYERS { return None; }
    if !state.players.iter().all(|p| finite(p.pos) && finite(p.vel) && [p.yaw, p.pitch, p.health, p.energy, p.respawn,
        p.cooldown, p.throw_clock, p.stun].iter().all(|x| x.is_finite())) { return None; }
    if !state.flags.iter().all(|f| finite(f.pos) && finite(f.home) && f.drop_timer.is_finite()) { return None; }
    if !(finite(state.ball.pos) && finite(state.ball.vel)) { return None; }
    if !(finite(state.rabbit.pos) && finite(state.rabbit.vel) && finite(state.rabbit.home)
        && state.rabbit.drop_timer.is_finite() && state.rabbit.held.is_finite()) { return None; }
    if !state.equipment.iter().all(|e| finite(e.aim) && e.health.is_finite() && e.shield.is_finite() && e.cooldown.is_finite()) { return None; }
    let mut state = state.clone();
    let occupied = |i: &usize| state.players.get(*i).is_some_and(|p| p.net_id != 0);
    let carriers: Vec<Option<usize>> = state.flags.iter().map(|f| f.carrier.filter(occupied)).collect();
    for (f, carrier) in state.flags.iter_mut().zip(carriers) { f.carrier = carrier; }
    state.ball.carrier = state.ball.carrier.filter(occupied);
    state.rabbit.carrier = state.rabbit.carrier.filter(occupied);
    state.ball.thrown_by = state.ball.thrown_by.filter(|i| *i < MAX_PLAYERS);
    state.ball.pass_to = state.ball.pass_to.filter(|i| *i < MAX_PLAYERS);
    // Bodies down: finite, and a bounded few (one per death).
    state.ball.bodies.retain(|t| t.pos.is_finite() && t.life.is_finite());
    state.ball.bodies.truncate(MAX_PLAYERS * 4);
    for p in &mut state.players { p.repair_beam = p.repair_beam.filter(|b| b.is_finite()); }
    // Turrets' projectiles carry the owner MAX_PLAYERS (no player), a vent's
    // lava bombs the map's (`features::MAP_OWNER`).
    let map_owner = peakrunner_core::sim::features::MAP_OWNER;
    state.discs.retain(|d| (d.owner <= MAX_PLAYERS || d.owner == map_owner) && finite(d.pos) && finite(d.vel) && d.life.is_finite() && d.spin.is_finite());
    state.explosions.retain(|e| finite(e.pos) && e.age.is_finite() && e.max_r.is_finite());
    state.smoke.retain(|p| finite(p.pos) && p.age.is_finite());
    state.loot.retain(|l| finite(l.pos) && l.age.is_finite());
    state.deployables.retain(|d| finite(d.pos) && finite(d.aim) && d.yaw.is_finite() && d.health.is_finite() && d.cooldown.is_finite());
    state.points.retain(|p| finite(p.pos) && p.radius.is_finite() && p.progress.is_finite() && p.drain_radius.is_finite() && p.drain_rate.is_finite()
        && p.capture.is_finite() && p.beacon.is_none_or(f32::is_finite)
        && p.spires.len() <= peakrunner_core::control::MAX_SPIRES && p.spires.iter().all(|s| finite(*s)));
    for p in &mut state.points { p.owner = p.owner.filter(|t| *t < 2); p.capturing = p.capturing.filter(|t| *t < 2); }
    if !state.conditions.wind.iter().all(|w| w.is_finite()) { state.conditions.wind = [0.0; 2]; }
    if let Some(v) = &mut state.vote { if !v.left.is_finite() { v.left = 0.0; } }
    for x in [&mut state.time_left, &mut state.phase_left, &mut state.vote_wait] { if !x.is_finite() { *x = 0.0; } }
    Some(state)
}

/// The drawn view (`Online::view`) a command claims: while firing, for
/// the laser, and always in Football, for body checks (`sim::lagcomp`).
fn claimed_view(view: u64, active: bool, fire: bool, football: bool) -> u64 {
    if active && (fire || football) { view } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loose_ball_flies_on_between_snapshots_for_a_moment_and_rests_on_the_ground() {
        use crate::sim::football::Ball;
        use peakrunner_core::terrain::MapId;
        let map = MapId::Longfield;
        let floor = peakrunner_core::terrain::support_on(map, Vec3::new(1024.0, 120.0, 1060.0)).0 + crate::sim::football::BALL_RADIUS;
        let thrown = Ball { active: true, in_play: true, pos: Vec3::new(1024.0, floor + 2.0, 1060.0), vel: Vec3::new(0.0, 5.0, -30.0), ..Ball::default() };
        let mut ball = thrown.clone();
        let mut flown = 0.0;
        for _ in 0..3 { advance_ball(&mut ball, map, 1.0 / 60.0, &mut flown); }
        let t = 3.0 / 60.0;
        let g = peakrunner_core::defs::movement().gravity;
        assert!((ball.pos.z - (thrown.pos.z - 30.0 * t)).abs() < 1e-3, "on along its flight: {}", ball.pos);
        assert!(ball.vel.y < 5.0 && (ball.vel.y - (5.0 - g * t)).abs() < 1e-3, "under gravity: {}", ball.vel);
        // At most MAX_BALL_AHEAD past the snapshot, never through the ground.
        for _ in 0..120 { advance_ball(&mut ball, map, 1.0 / 60.0, &mut flown); }
        assert!((flown - MAX_BALL_AHEAD).abs() < 1e-4 && (thrown.pos.z - ball.pos.z - 30.0 * MAX_BALL_AHEAD).abs() < 0.01, "{flown} {}", ball.pos);
        assert!(ball.pos.y >= floor - 1e-3);
        // A carried ball stays with its carrier.
        let mut held = Ball { carrier: Some(0), ..thrown.clone() };
        let mut flown = 0.0;
        advance_ball(&mut held, map, 0.1, &mut flown);
        assert_eq!(held.pos, thrown.pos);
    }
    use crate::{sim::Match, terrain::MapId};

    /// A pass to you, thrown on the server by the thrower's fire input
    /// (from either team, as the balanced teams fall): your snapshot names
    /// you (`Ball::pass_to`) and your game hears "incoming"; the thrower's
    /// game doesn't.
    #[test]
    fn a_pass_thrown_to_you_on_the_server_sounds_incoming() {
        use crate::sim::football::Ball;
        let map = MapId::Longfield;
        let mut server = Match::new(map);
        server.rotate_to_mode(map, peakrunner_core::map_catalog::SupportedMode::Football);
        let me = server.join(1, "Me").unwrap();
        let mate = server.join(2, "Mate").unwrap();
        server.phase = Phase::Playing;
        server.step(&[]);
        let floor = peakrunner_core::terrain::support_on(map, Vec3::new(1024.0, 120.0, 1060.0)).0 + 1.2;
        let (from, to) = (Vec3::new(1024.0, floor, 1060.0), Vec3::new(1024.0, floor, 1030.0));
        for (i, at) in [(mate, from), (me, to)] {
            let p = &mut server.world.players[i];
            p.alive = true; p.pos = at; p.vel = Vec3::ZERO; p.yaw = 0.0; p.cooldown = 0.0;
        }
        server.world.ball = Ball { active: true, in_play: true, carrier: Some(mate), pos: from, ..Ball::default() };
        let (mut mine, mut theirs) = ((World::new(), Online::default()), (World::new(), Online::default()));
        mine.1.receive(&mut mine.0, &server.snapshot(), 1);
        theirs.1.receive(&mut theirs.0, &server.snapshot(), 2);
        let look = to + Vec3::Y * 0.4 - (from + Vec3::Y * 2.0);
        let mut commands = vec![None; mate + 1];
        commands[mate] = Some(Command { seq: 1, yaw: 0.0, pitch: (look.y / look.length()).asin(), fire: true, ..Command::default() });
        server.step(&commands);
        assert_eq!(server.world.ball.pass_to, Some(me), "the server names the receiver: {:?}", server.world.ball);
        mine.0.events.clear(); theirs.0.events.clear();
        mine.1.receive(&mut mine.0, &server.snapshot(), 1);
        theirs.1.receive(&mut theirs.0, &server.snapshot(), 2);
        assert_eq!(mine.0.ball.pass_to, Some(mine.0.player_id));
        assert!(mine.0.events.split(',').any(|e| e == "incoming"), "{}", mine.0.events);
        assert!(!theirs.0.events.split(',').any(|e| e == "incoming"), "{}", theirs.0.events);
    }

    /// An enemy running into your carrier on the server knocks the ball
    /// loose: through the snapshots your game announces FUMBLE and grunts,
    /// and when they come up with it there's no INTERCEPTS (`ball6`).
    #[test]
    fn a_fumble_on_the_server_is_announced_and_its_recovery_is_no_interception() {
        use crate::sim::football::Ball;
        let map = MapId::Longfield;
        let mut server = Match::new(map);
        server.rotate_to_mode(map, peakrunner_core::map_catalog::SupportedMode::Football);
        let me = server.join(1, "Me").unwrap();
        let them = server.join(2, "Them").unwrap();
        server.phase = Phase::Playing;
        server.step(&[]);
        let floor = peakrunner_core::terrain::support_on(map, Vec3::new(1024.0, 120.0, 1060.0)).0 + 1.2;
        let (at, from) = (Vec3::new(1024.0, floor, 1060.0), Vec3::new(1024.0, floor, 1061.3));
        for (i, team, pos, vel) in [(me, crate::sim::Team::Ember, at, Vec3::ZERO), (them, crate::sim::Team::Glacier, from, Vec3::new(0.0, 0.0, -12.0))] {
            let p = &mut server.world.players[i];
            p.alive = true; p.team = team; p.pos = pos; p.vel = vel; p.stun = 0.0;
        }
        server.world.ball = Ball { active: true, in_play: true, carrier: Some(me), pos: at, ..Ball::default() };
        let (mut world, mut online, mut watch) = (World::new(), Online::default(), crate::football_hud::FootballWatch::default());
        online.receive(&mut world, &server.snapshot(), 1);
        watch.update(&world);
        for _ in 0..12 { if server.world.ball.loose() { break; } server.step(&[]); }
        assert!(server.world.ball.loose() && server.world.ball.fumbles == 1, "knocked loose on the server: {:?}", server.world.ball);
        world.events.clear();
        online.receive(&mut world, &server.snapshot(), 1);
        assert_eq!(watch.update(&world), ["FUMBLE"]);
        assert!(world.events.split(',').any(|e| e == "pain") && !world.events.split(',').any(|e| e == "pass"), "{}", world.events);
        // They take it while it still flies.
        let p = server.world.players[them].pos;
        server.world.ball.pos = p;
        server.world.ball.vel = Vec3::new(0.0, 0.0, -8.0);
        server.world.ball.no_catch = 0.0;
        server.step(&[]);
        assert!(server.world.ball.carried_by(them), "{:?}", server.world.ball);
        online.receive(&mut world, &server.snapshot(), 1);
        assert!(watch.update(&world).is_empty(), "no INTERCEPTS for a fumble");
    }

    #[test]
    fn capture_cues_follow_team_and_do_not_replay_or_sound_on_reset() {
        let mut server=Match::new(MapId::Valley);
        server.join(1,"A").unwrap();server.join(2,"B").unwrap();server.tick=1;
        let mut world=World::new();let mut online=Online::default();
        online.receive(&mut world,&server.snapshot(),1);
        for (score,expected) in [([1,0],"capture_win,"),([1,1],"capture_loss,"),([0,0],"")] {
            world.events.clear();server.tick+=3;server.world.score=score;
            let state=server.snapshot();online.receive(&mut world,&state,1);
            assert_eq!(world.events,expected);
            world.events.clear();online.receive(&mut world,&state,1);
            assert!(world.events.is_empty());
        }
    }

    #[test]
    fn holding_points_and_frags_never_play_the_capture_sting() {
        use peakrunner_core::map_catalog::SupportedMode;
        for mode in [SupportedMode::CaptureAndHold, SupportedMode::TeamDeathmatch] {
            let mut server=Match::new(MapId::Valley);
            server.world.set_mode(mode);
            server.join(1,"A").unwrap();server.join(2,"B").unwrap();server.tick=1;
            let mut world=World::new();let mut online=Online::default();
            online.receive(&mut world,&server.snapshot(),1);
            for score in [[5,0],[10,3],[15,6]] {
                world.events.clear();server.tick+=3;server.world.score=score;
                online.receive(&mut world,&server.snapshot(),1);
                assert!(!world.events.contains("capture"), "{mode:?}: {}", world.events);
            }
        }
    }

    #[test]
    fn remote_sounds_keep_positions_and_repeated_snapshots_are_silent() {
        let mut server = Match::new(MapId::Valley);
        server.join(1, "A").unwrap(); server.join(2, "B").unwrap();
        server.tick = 1;
        let mut world = World::new(); let mut online = Online::default();
        online.receive(&mut world, &server.snapshot(), 1);
        server.tick += 3;
        server.world.players[1].pos = server.world.players[0].pos + glam::Vec3::X * 40.0;
        server.world.players[1].shots += 1;
        server.world.blast_serial += 1;
        let position = server.world.players[1].pos;
        server.world.explosions.push(crate::sim::Explosion { pos:position, age:0.0, max_r:9.0, kind:0 });
        let state = server.snapshot();
        online.receive(&mut world, &state, 1);
        assert!(world.spatial_sounds.contains(&("boom", position)));
        assert!(world.spatial_sounds.contains(&("disc", position)));
        assert!(!world.events.contains("boom"));
        world.spatial_sounds.clear();
        online.receive(&mut world, &state, 1);
        assert!(world.spatial_sounds.is_empty());
    }

    #[test]
    fn team_transfer_discards_old_prediction_and_announces_redeployment() {
        let mut server = Match::new(MapId::Valley);
        for id in 1..=3 { server.join(id, "Skier").unwrap(); }
        server.step(&[]);
        let mut world = World::new();
        let mut online = Online::default();
        online.receive(&mut world, &server.snapshot(), 3);
        online.pending.push_back((Command { seq: 100, move_z: 1.0, ..Command::default() }, Instant::now()));
        server.leave(1);
        server.step(&[]);
        let snapshot = server.snapshot();
        online.receive(&mut world, &snapshot, 3);
        assert!(online.pending.is_empty());
        assert_eq!(world.players[2].team, snapshot.players[2].team);
        assert_eq!(world.players[2].pos, snapshot.players[2].pos);
        assert!(world.message.starts_with("AUTO-BALANCED"));
        server.step(&[]);
        online.receive(&mut world, &server.snapshot(), 3);
        assert!(world.message.starts_with("AUTO-BALANCED"));
    }

    #[test]
    fn a_voluntary_switch_says_so_and_a_spectator_follows_the_match() {
        let mut server = Match::new(MapId::Valley);
        for id in 1..=3 { server.join(id, "Skier").unwrap(); }
        server.join_spectator(9, "Watcher").unwrap();
        server.step(&[]);
        let mut world = World::new(); let mut online = Online::default();
        let me = server.world.players.iter().position(|p| p.net_id == 1).unwrap();
        online.receive(&mut world, &server.snapshot(), 1);
        online.expect_switch();
        server.switch_team(me).unwrap();
        server.step(&[]);
        online.receive(&mut world, &server.snapshot(), 1);
        assert!(world.message.starts_with("SWITCHED TEAMS"), "{}", world.message);
        // A spectator's world: every player remote, nothing pending.
        let mut watcher = World::new(); let mut view = Online::default();
        view.receive(&mut watcher, &server.snapshot(), 9);
        assert!(watcher.spectating && watcher.player_id == MAX_PLAYERS);
        assert_eq!(view.tick, server.tick);
        assert!(view.pending.is_empty());
        // Becoming a player again snaps to the new slot.
        server.tick += SWITCH_WAIT;
        let slot = server.unspectate(9, 8).unwrap();
        server.step(&[]);
        view.receive(&mut watcher, &server.snapshot(), 9);
        assert!(!watcher.spectating && watcher.player_id == slot);
        assert_eq!(watcher.players.len(), MAX_PLAYERS);
    }
    const SWITCH_WAIT: u64 = peakrunner_core::sim::teams::SWITCH_COOLDOWN;

    /// Draws `frames` at 144 fps while snapshots `0..count` (from `make`)
    /// arrive every 50 ms with up to 15 ms of jitter. Returns the other
    /// player's drawn position each frame, with the render time.
    fn draw_remote(make: impl Fn(u64) -> Snapshot, count: u64) -> Vec<(f64, glam::Vec3)> {
        let mut world = World::new(); let mut online = Online::default();
        let mut rng = 0x2545_f491u64;
        let mut jitter = || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; (rng % 1000) as f64 / 1000.0 * 0.015 };
        let arrivals: Vec<f64> = (0..count).map(|k| k as f64 * 0.05 + jitter()).collect();
        let dt = 1.0 / 144.0;
        let (mut now, mut next, mut drawn) = (0.0, 0, Vec::new());
        while now < count as f64 * 0.05 {
            while next < count as usize && arrivals[next] <= now { online.receive(&mut world, &make(next as u64), 1); next += 1; }
            online.animate(&mut world, dt as f32);
            if let Some(p) = world.players.iter().find(|p| p.net_id == 2) { drawn.push((online.clock - INTERP_DELAY, p.pos)); }
            now += dt;
        }
        drawn
    }

    #[test]
    fn remote_players_interpolate_smoothly_and_snap_on_teleport() {
        let mut server = Match::new(MapId::Valley);
        server.join(1, "Me").unwrap(); let other = server.join(2, "Other").unwrap();
        server.step(&[]);
        let base = server.snapshot();
        let (start, v, t0) = (base.players[other].pos, glam::Vec3::new(30.0, 0.0, 12.0), base.tick as f64 * STEP as f64);
        let at = |k: u64, jump: glam::Vec3| {
            let mut s = base.clone(); s.tick = base.tick + 3 * k;
            s.players[other].pos = start + v * (3 * k) as f32 * STEP + jump; s.players[other].vel = v; s
        };
        // Steady flight: drawn on the true path about 100 ms back, moving the
        // same distance every frame (no 20 Hz steps, no easing lag).
        let drawn = draw_remote(|k| at(k, glam::Vec3::ZERO), 60);
        let step = v.length() / 144.0;
        for pair in drawn[144..].windows(2) {
            let (t, p) = pair[1];
            let truth = start + v * (t - t0) as f32;
            assert!(p.distance(truth) < 0.05, "off the path by {} at {t}", p.distance(truth));
            let moved = pair[0].1.distance(p);
            assert!((moved - step).abs() < step * 0.5, "frame step {moved} vs {step}");
        }
        // A teleport 200 m away halfway: never drawn sliding between the two.
        let jump = glam::Vec3::new(200.0, 0.0, 0.0);
        let drawn = draw_remote(|k| at(k, if k >= 30 { jump } else { glam::Vec3::ZERO }), 60);
        for (t, p) in &drawn[144..] {
            let path = start + v * (t - t0) as f32;
            assert!(p.distance(path) < 5.0 || p.distance(path + jump) < 5.0, "drawn mid-slide at {t}: {p}");
        }
        assert!(drawn.last().unwrap().1.distance(start + v * (drawn.last().unwrap().0 - t0) as f32 + jump) < 1.0);
    }

    #[test]
    fn vehicles_others_drive_are_drawn_smoothly_too() {
        use peakrunner_core::sim::vehicles::{Craft, MAX_SEATS};
        let mut server = Match::new(MapId::Valley);
        server.join(1, "Me").unwrap(); let other = server.join(2, "Other").unwrap();
        server.step(&[]);
        let base = server.snapshot();
        let (start, v, t0) = (glam::Vec3::new(300.0, 80.0, 300.0), glam::Vec3::new(60.0, 0.0, -20.0), base.tick as f64 * STEP as f64);
        let mut riders = [0u8; MAX_SEATS];
        riders[0] = other as u8 + 1;
        let craft = Craft { kind: 0, team: base.players[other].team, pos: start, vel: v, yaw: 0.0, pitch: 0.0, hull: 100.0, energy: 100.0,
            riders, cooldowns: [0.0; MAX_SEATS], idle: 0.0 };
        let at = |k: u64| {
            let mut s = base.clone(); s.tick = base.tick + 3 * k;
            let mut c = craft.clone(); c.pos = start + v * (3 * k) as f32 * STEP; s.vehicles = vec![c]; s
        };
        let mut world = World::new(); let mut online = Online::default();
        let dt = 1.0 / 144.0;
        let (mut now, mut next, mut drawn) = (0.0, 0u64, Vec::new());
        while now < 60.0 * 0.05 {
            while next < 60 && next as f64 * 0.05 <= now { online.receive(&mut world, &at(next), 1); next += 1; }
            online.animate(&mut world, dt as f32);
            if let Some(c) = world.vehicles.first() { drawn.push((online.clock - INTERP_DELAY, c.pos)); }
            now += dt;
        }
        // On the true path about 100 ms back, the same distance every frame.
        let step = v.length() / 144.0;
        for pair in drawn[144..].windows(2) {
            let (t, p) = pair[1];
            assert!(p.distance(start + v * (t - t0) as f32) < 0.1, "off the path at {t}");
            assert!((pair[0].1.distance(p) - step).abs() < step * 0.5, "a 20 Hz step");
        }
    }

    #[test]
    fn the_view_sent_with_a_shot_is_the_time_remote_players_are_drawn_at() {
        let mut server = Match::new(MapId::Valley);
        server.join(1, "Me").unwrap(); let other = server.join(2, "Other").unwrap();
        server.step(&[]);
        let mut world = World::new(); let mut online = Online::default();
        assert_eq!(online.view(), 0, "nothing drawn yet: no claim");
        let (start, v) = (server.world.players[other].pos, glam::Vec3::new(25.0, 0.0, 0.0));
        let base = server.snapshot();
        for k in 0..20u64 {
            let mut s = base.clone(); s.tick = base.tick + 3 * k;
            s.players[other].pos = start + v * (3 * k) as f32 * STEP; s.players[other].vel = v;
            online.receive(&mut world, &s, 1);
            for _ in 0..3 { online.animate(&mut world, STEP); }
        }
        // The claimed time, on the true path, is where the other player is drawn.
        let ticks = online.view() as f64 / crate::sim::lagcomp::VIEW_SUBTICKS as f64 - base.tick as f64;
        let claimed = start + v * (ticks as f32 * STEP);
        assert!(world.players[other].pos.distance(claimed) < 0.05, "drawn {} vs claimed {claimed}", world.players[other].pos);
    }

    /// Football on a server with you and one other person, and a client
    /// that has received its first snapshot with `pending` commands not yet
    /// answered (your body is predicted through them).
    fn football_client(pending: u64) -> (Match, usize, World, Online) {
        let map = MapId::Longfield;
        let mut server = Match::new(map);
        server.rotate_to_mode(map, peakrunner_core::map_catalog::SupportedMode::Football);
        server.join(1, "Me").unwrap();
        let other = server.join(2, "Other").unwrap();
        server.phase = Phase::Playing;
        server.step(&[]);
        let (world, mut online) = (World::new(), Online::default());
        for seq in 1..=pending { online.pending.push_back((Command { seq, ..Command::default() }, Instant::now())); }
        (server, other, world, online)
    }

    /// The other player's place in `s`: running along the turf at `vel`.
    fn running(s: &mut Snapshot, other: usize, at: glam::Vec3, vel: glam::Vec3) {
        let floor = peakrunner_core::terrain::support_on(MapId::Longfield, at + glam::Vec3::Y * 20.0).0 + peakrunner_core::terrain::PLAYER_RADIUS;
        (s.players[other].pos, s.players[other].vel) = (glam::Vec3::new(at.x, floor, at.z), vel);
    }

    /// In Football the others are drawn ahead with you, coasted on from the
    /// newest snapshot to your own body's moment; a turn the guess missed
    /// slides over instead of jumping; commands claim that snapshot and how
    /// far on.
    #[test]
    fn football_others_are_drawn_ahead_with_you_and_slide_over_a_missed_turn() {
        let (server, other, mut world, mut online) = football_client(7);
        let lead = 6.0 * STEP;
        let (at, run) = (glam::Vec3::new(1024.0, 0.0, 1000.0), glam::Vec3::new(0.0, 0.0, 12.0));
        let mut s = server.snapshot();
        running(&mut s, other, at, run);
        online.receive(&mut world, &s, 1);
        let ahead = s.players[other].pos + run * lead;
        assert!(world.players[other].pos.distance(ahead) < 0.01, "drawn {} vs {ahead}", world.players[other].pos);
        let (view, claimed) = online.claim(true);
        assert_eq!((view, claimed), (crate::sim::lagcomp::view_at(s.tick as f64), 6 * crate::sim::lagcomp::VIEW_SUBTICKS as u16));
        assert_eq!(online.claim(false), (online.view(), 0), "other modes claim the delayed clock");
        // The next snapshot: it cut sideways. Drawn where it was at first,
        // then over to the new guess within a few frames.
        let before = world.players[other].pos;
        let cut = glam::Vec3::new(20.0, 0.0, 0.0);
        let mut next = s.clone();
        next.tick += 3;
        running(&mut next, other, at + run * (3.0 * STEP), cut);
        online.receive(&mut world, &next, 1);
        assert!(world.players[other].pos.distance(before) < 0.01, "no jump: {} vs {before}", world.players[other].pos);
        let mut last = world.players[other].pos;
        // A 2 m miss: at most 0.6 m a frame, most of it within 0.15 s.
        for _ in 0..16 {
            online.animate(&mut world, STEP);
            assert!(world.players[other].pos.distance(last) < 0.6, "a slide, frame by frame: {}", world.players[other].pos.distance(last));
            last = world.players[other].pos;
        }
        let guess = next.players[other].pos + cut * lead;
        assert!(last.distance(guess) < 0.05, "over to the new guess: {last} vs {guess}");
    }

    /// Six bots a side playing Football against you over a 100 ms round
    /// trip (50 ms each way, snapshots at 20 Hz): how far each moving bot is
    /// drawn from where the server has it at your own body's moment (the
    /// tick your newest command is applied), drawn ahead (`place_ahead`) and
    /// as other modes draw (`place_delayed`).
    #[test]
    fn football_bots_are_drawn_near_where_they_really_are() {
        use std::collections::BTreeMap;
        let map = MapId::Longfield;
        let mut server = Match::new(map);
        server.bots = 12;
        server.rotate_to_mode(map, peakrunner_core::map_catalog::SupportedMode::Football);
        server.join(1, "Me").unwrap();
        server.phase = Phase::Playing;
        let me = (0..server.world.players.len()).find(|&i| server.world.players[i].net_id == 1).unwrap();
        let (mut world, mut online) = (World::new(), Online::default());
        const ONE_WAY: u64 = 3;
        let (mut to_server, mut to_client): (VecDeque<(u64, Command)>, VecDeque<(u64, Snapshot)>) = (VecDeque::new(), VecDeque::new());
        // Drawn at (tick): per bot slot, ahead and delayed.
        let mut drawn: BTreeMap<u64, Vec<(usize, glam::Vec3, glam::Vec3)>> = BTreeMap::new();
        let (mut ahead, mut delayed) = (Vec::new(), Vec::new());
        let mut seq = 0;
        for _ in 0..(40.0 / STEP) as u64 {
            let mut list = vec![None; MAX_PLAYERS];
            while to_server.front().is_some_and(|(due, _)| *due <= server.tick + 1) { list[me] = to_server.pop_front().map(|c| c.1); }
            server.step(&list);
            let now = server.tick;
            if let Some(seen) = drawn.remove(&now) {
                for (k, a, d) in seen {
                    let truth = server.world.players[k].pos;
                    if server.world.players[k].vel.length() > 3.0 { ahead.push(a.distance(truth)); delayed.push(d.distance(truth)); }
                }
            }
            if now % 3 == 0 { to_client.push_back((now + ONE_WAY, server.snapshot())); }
            while to_client.front().is_some_and(|(due, _)| *due <= now) {
                let (_, snapshot) = to_client.pop_front().unwrap();
                online.receive(&mut world, &snapshot, 1);
            }
            // A frame: one command sent (applied ONE_WAY ticks on, with those
            // still waiting), then the others drawn for it.
            seq += 1;
            let command = Command { seq, ..Command::default() };
            to_server.push_back((now + ONE_WAY, command));
            online.pending.push_back((command, Instant::now()));
            online.animate(&mut world, STEP);
            if online.history.is_empty() || !world.football() { continue; }
            // Your body is drawn a step back from the newest command's tick.
            let at = now + ONE_WAY - 1;
            let mut old = World::new();
            old.players = world.players.clone();
            online.place_delayed(&mut old);
            let bots = (0..world.players.len()).filter(|&k| k != me && world.players[k].is_bot && world.players[k].alive);
            drawn.insert(at, bots.map(|k| (k, world.players[k].pos, old.players[k].pos)).collect());
        }
        let stats = |v: &mut Vec<f32>| { v.sort_by(f32::total_cmp); (v.iter().sum::<f32>() / v.len() as f32, v[v.len() / 2], v[v.len() * 95 / 100]) };
        let (a, d) = (stats(&mut ahead), stats(&mut delayed));
        println!("{} samples of moving bots: drawn ahead off by {:.2} m on average (median {:.2}, 95% within {:.2}); drawn delayed {:.2} m (median {:.2}, 95% within {:.2})", ahead.len(), a.0, a.1, a.2, d.0, d.1, d.2);
        assert!(ahead.len() > 1000, "bots moved");
        // Measured 2026-10-15: ahead 0.13 m on average (median 0.05, 95%
        // within 0.40), delayed 2.06 m (median 1.83, 95% within 3.78).
        assert!(a.1 < 0.15 && a.2 < 0.8 && a.0 < d.0 * 0.25, "ahead is close: {a:?} vs delayed {d:?}");
    }

    /// Honest Football claims (`claim`: the newest snapshot and how far past
    /// it the others were drawn) are near the present at the server, against
    /// the tick each command is applied, over a 100 ms round trip with
    /// snapshots at 20 Hz. One frame's claim goes with each of its commands,
    /// so the fewer the frames, the further behind its last ones. Measured
    /// 2026-10-16, at most: 2 ticks at 60 frames a second, 4 at 30, 6 at 20,
    /// 8 at 15 and 7 at 12. From 20 frames a second up the server's bound
    /// (`lagcomp::MAX_BEHIND_TICKS`) leaves them be; below, a frame's last
    /// commands are drawn up to 2 ticks further on than they were.
    #[test]
    fn honest_football_claims_are_within_the_servers_bound() {
        use peakrunner_core::sim::lagcomp::{MAX_BEHIND_TICKS, VIEW_SUBTICKS};
        for batch in [1u64, 2, 3, 4, 5] {
            let map = MapId::Longfield;
            let mut server = Match::new(map);
            server.bots = 4;
            server.rotate_to_mode(map, peakrunner_core::map_catalog::SupportedMode::Football);
            server.join(1, "Me").unwrap();
            server.phase = Phase::Playing;
            let me = (0..server.world.players.len()).find(|&i| server.world.players[i].net_id == 1).unwrap();
            let (mut world, mut online) = (World::new(), Online::default());
            const ONE_WAY: u64 = 3;
            let (mut to_server, mut to_client): (VecDeque<(u64, Command)>, VecDeque<(u64, Snapshot)>) = (VecDeque::new(), VecDeque::new());
            let (mut seq, mut gaps) = (0, Vec::new());
            for _ in 0..(20.0 / STEP) as u64 {
                // One command a tick, as the server's input buffer takes them.
                let mut list = vec![None; MAX_PLAYERS];
                if to_server.front().is_some_and(|(due, _)| *due <= server.tick + 1) { list[me] = to_server.pop_front().map(|c| c.1); }
                server.step(&list);
                if let Some(c) = list[me].filter(|c| c.view != 0) {
                    gaps.push(server.world.lag.rewind(me) - c.ahead as f64 / VIEW_SUBTICKS as f64);
                }
                let now = server.tick;
                if now % 3 == 0 { to_client.push_back((now + ONE_WAY, server.snapshot())); }
                while to_client.front().is_some_and(|(due, _)| *due <= now) {
                    let (_, snapshot) = to_client.pop_front().unwrap();
                    online.receive(&mut world, &snapshot, 1);
                }
                if now % batch != 0 { continue; }
                // A frame: its commands sent together, each with its claim.
                let (view, ahead) = online.claim(world.football());
                for _ in 0..batch {
                    seq += 1;
                    let command = Command { seq, view, ahead, ..Command::default() };
                    to_server.push_back((now + ONE_WAY, command));
                    online.pending.push_back((command, Instant::now()));
                }
                online.animate(&mut world, batch as f32 * STEP);
            }
            gaps.sort_by(f64::total_cmp);
            let worst = *gaps.last().expect("claims");
            println!("{batch} command(s) a frame: {} claims, behind the present by {:.2} ticks at the median, {worst:.2} at most", gaps.len(), gaps[gaps.len() / 2]);
            assert!(gaps.len() > 500, "claims applied");
            let bound = MAX_BEHIND_TICKS as f64 + if batch <= 3 { 0.0 } else { 2.0 };
            assert!(worst <= bound, "{batch} a frame: {worst:.2} ticks behind");
        }
    }

    /// The loose ball goes ahead with the others, on its flight.
    #[test]
    fn football_ball_is_drawn_ahead_with_you() {
        let (server, _, mut world, mut online) = football_client(7);
        let mut s = server.snapshot();
        let from = glam::Vec3::new(1024.0, 112.0, 1000.0);
        s.ball = crate::sim::football::Ball { active: true, in_play: true, pos: from, vel: glam::Vec3::new(0.0, 4.0, 20.0), ..Default::default() };
        online.receive(&mut world, &s, 1);
        let lead = 6.0 * STEP;
        assert!((world.ball.pos.z - (from.z + 20.0 * lead)).abs() < 0.01, "ahead along its flight: {}", world.ball.pos);
        let flown = world.ball.pos;
        online.animate(&mut world, STEP);
        assert_eq!(world.ball.pos, flown, "and held there until your body moves on");
    }

    #[test]
    fn football_commands_claim_their_view_without_firing() {
        assert_eq!(claimed_view(77, true, true, false), 77, "a shot claims it");
        assert_eq!(claimed_view(77, true, false, false), 0, "no shot, no claim");
        assert_eq!(claimed_view(77, true, false, true), 77, "Football always claims it, for body checks");
        assert_eq!(claimed_view(77, false, true, true), 0, "nor while not playing");
    }

    #[test]
    fn hostile_snapshots_are_cleaned_or_refused_and_never_panic() {
        use crate::sim::{Disc, Explosion, Team};
        let mut server = Match::new(MapId::Valley);
        server.join(1, "Me").unwrap(); server.join(2, "Other").unwrap();
        server.step(&[]);
        server.world.ball.active = true;
        let mut world = World::new(); let mut online = Online::default();
        online.receive(&mut world, &server.snapshot(), 1);
        let good = online.tick;
        let mut bad = server.snapshot(); bad.tick += 3;
        bad.flags[0].carrier = Some(999); bad.flags[1].carrier = Some(20);
        bad.ball.carrier = Some(400); bad.ball.thrown_by = Some(usize::MAX); bad.ball.pass_to = Some(usize::MAX); bad.ball.in_play = true;
        let disc = |pos: glam::Vec3, owner: usize| Disc { pos, vel: glam::Vec3::X, team: Team::Ember, owner, life: 1.0, kind: 0, spin: 0.0, target: 0, power: 1.0 };
        let map_owner = peakrunner_core::sim::features::MAP_OWNER;
        bad.discs = vec![disc(glam::Vec3::ONE, 999), disc(glam::Vec3::ONE, map_owner + 1), disc(glam::Vec3::NAN, 0),
            disc(glam::Vec3::ONE, 1), disc(glam::Vec3::ONE, MAX_PLAYERS), disc(glam::Vec3::ONE, map_owner)];
        bad.explosions.push(Explosion { pos: glam::Vec3::INFINITY, age: 0.0, max_r: 9.0, kind: 0 });
        bad.points.push(peakrunner_core::control::Point { name: "X".into(), pos: glam::Vec3::ZERO, radius: 5.0, drain_radius: 0.0,
            drain_rate: 0.0, active: true, owner: Some(200), progress: 0.5, capturing: Some(7), contested: false,
            turret: None, turret_at: glam::Vec3::ZERO, turret_aim: glam::Vec3::ZERO, turret_cooldown: 0.0, turret_range: 0.0,
            ..Default::default() });
        bad.time_left = f32::NAN;
        online.receive(&mut world, &bad, 1);
        for _ in 0..30 { online.animate(&mut world, 1.0 / 60.0); }
        assert_eq!(online.tick, bad.tick, "cleaned and applied");
        assert!(world.flags.iter().all(|f| f.carrier.is_none()) && world.ball.carrier.is_none() && world.ball.thrown_by.is_none() && world.ball.pass_to.is_none());
        assert_eq!(world.discs.iter().map(|d| d.owner).collect::<Vec<_>>(), [1, MAX_PLAYERS, map_owner], "turret bolts and lava bombs are drawn");
        assert!(world.explosions.is_empty());
        assert_eq!((world.points[0].owner, world.points[0].capturing), (None, None));
        assert!(world.time_left.is_finite());
        // Non-finite players can't be cleaned: the whole snapshot is refused.
        let mut worse = server.snapshot(); worse.tick = bad.tick + 3;
        worse.players[0].pos = glam::Vec3::NAN;
        online.receive(&mut world, &worse, 1);
        let mut short = server.snapshot(); short.tick = bad.tick + 6; short.players.truncate(3);
        online.receive(&mut world, &short, 1);
        assert_eq!(online.tick, bad.tick);
        assert!(world.players.iter().all(|p| p.pos.is_finite()));
        assert!(good < online.tick);
    }

    #[test]
    fn the_countdown_runs_down_smoothly_between_snapshots() {
        let mut server = Match::new(MapId::Valley);
        server.join(1, "A").unwrap(); server.join(2, "B").unwrap();
        server.step(&[]);
        let mut world = World::new(); let mut online = Online::default();
        online.receive(&mut world, &server.snapshot(), 1);
        assert_eq!(online.countdown_left(), Some(crate::sim::ROUND_COUNTDOWN));
        // Frames at 144 fps, a snapshot every third tick: the shown value
        // falls every frame, never jumps back, and tracks the server's.
        let mut shown = Vec::new();
        for tick in 1..=120 {
            server.step(&[]);
            if tick % 3 == 0 { online.receive(&mut world, &server.snapshot(), 1); }
            for _ in 0..2 { online.animate(&mut world, STEP / 2.0); shown.push(online.countdown_left().unwrap()); }
            assert!((shown.last().unwrap() - server.phase_left).abs() < 0.1, "{} vs {}", shown.last().unwrap(), server.phase_left);
        }
        for pair in shown.windows(2) { assert!(pair[1] <= pair[0] + 1e-4, "went back up: {pair:?}"); }
        server.phase = Phase::Playing; server.tick += 3;
        online.receive(&mut world, &server.snapshot(), 1);
        assert_eq!(online.countdown_left(), None);
    }

    #[test]
    fn play_starts_from_the_countdown_without_a_rubber_band() {
        let mut server = Match::new(MapId::Valley);
        server.join(1, "A").unwrap(); server.join(2, "B").unwrap();
        server.step(&[]);
        assert_eq!(server.phase, Phase::Countdown);
        let mut client = World::new();
        let mut online = Online::default();
        online.receive(&mut client, &server.snapshot(), 1);
        let mut snapshots = VecDeque::new();
        let mut played = false;
        let ticks = (crate::sim::ROUND_COUNTDOWN / STEP) as u64 + 90;
        for seq in 1..=ticks {
            // As `advance`: every command is sent, predicted only in play.
            let command = Command { seq, move_z: 1.0, jet: seq % 40 < 20, ..Command::default() };
            online.pending.push_back((command, Instant::now()));
            if matches!(online.phase, Phase::Playing | Phase::Waiting) { client.predict_command(command); }
            server.step(&[Some(command)]);
            if seq % 3 == 0 { snapshots.push_back((seq + 6, server.snapshot())); }
            if snapshots.front().is_some_and(|(at, _)| *at <= seq) {
                let (_, snapshot) = snapshots.pop_front().unwrap();
                let before = client.players[0].pos;
                let first_play = snapshot.phase == Phase::Playing && !played;
                online.receive(&mut client, &snapshot, 1);
                if snapshot.phase == Phase::Playing {
                    played = true;
                    // Everything sent is now predicted: exactly where the server has us.
                    assert!(client.players[0].pos.distance(server.world.players[0].pos) < 0.001, "diverged at {seq}");
                    assert!(online.pending.len() <= 9, "prediction kept across the start");
                }
                if first_play {
                    // Only forward, by the few ticks in flight, and smoothed.
                    let jump = client.players[0].pos - before;
                    assert!(jump.length() < 2.0 && client.net_camera_offset != glam::Vec3::ZERO, "start jump {jump}");
                }
            }
        }
        assert!(played);
    }

    #[test]
    fn delayed_snapshots_reconcile_pending_inputs_without_replaying_audio() {
        let mut server = Match::new(MapId::Valley);
        server.join(1, "A").unwrap(); server.join(2, "B").unwrap();
        server.phase = Phase::Playing;
        let mut client = World::new();
        let mut online = Online::default();
        server.step(&[]);
        online.receive(&mut client, &server.snapshot(), 1);
        let mut commands = Vec::new();
        let mut snapshots = VecDeque::new();
        for seq in 1..=180 {
            let command = Command { seq, yaw: 0.0, move_x: if seq < 90 { -1.0 } else { 1.0 },
                move_z: 1.0, jet: seq < 100, jump: true, ..Command::default() };
            commands.push(command);
            online.pending.push_back((command, Instant::now()));
            client.predict_command(command);
            server.step(&[Some(command)]);
            if seq % 3 == 0 { snapshots.push_back((seq + 6, server.snapshot())); }
            if snapshots.front().is_some_and(|(at, _)| *at <= seq) {
                let (_, snapshot) = snapshots.pop_front().unwrap();
                client.events.clear();
                online.receive(&mut client, &snapshot, 1);
                assert!(!client.events.contains("disc"));
                assert!(client.players[0].pos.distance(server.world.players[0].pos) < 0.001,
                    "reconciliation changed the predicted route at {seq}");
                assert!(online.pending.len() <= 9);
                assert_eq!(client.players[0].health, snapshot.players[0].health);
            }
        }
        assert!(client.players[0].pos.is_finite());
        let tick = online.tick;
        let mut stale = server.snapshot(); stale.tick = tick - 1;
        stale.players[0].health = 1.0;
        online.receive(&mut client, &stale, 1);
        assert_eq!(online.tick, tick);
        assert_ne!(client.players[0].health, 1.0);
    }
}
impl Default for Online {
    fn default() -> Self { Self { tick: 0, round: 0, phase: Phase::Waiting, latency_ms: 0.0,
        seq: 0, acc: 0.0, pending: VecDeque::new(), history: VecDeque::new(), clock: 0.0, balance_notice_until: None,
        phase_left: 0.0, phase_time: 0.0, switch_asked: None, switch_notice: false, ball_flown: 0.0, drawn: Vec::new(), lead: 0.0 } }
}
impl Online {
    /// The player asked to switch teams (the next team change is theirs).
    pub fn expect_switch(&mut self) { self.switch_asked = Some(Instant::now()); }

    pub fn receive(&mut self, world: &mut World, state: &Snapshot, id: u32) {
        if state.tick <= self.tick { return; }
        let Some(state) = sanitize(state) else {
            log::warn!("snapshot refused: tick {} has out-of-range values", state.tick);
            return;
        };
        let state = &state;
        let was_online = self.tick > 0;
        let old_map = world.map;
        let old_blast = world.blast_serial;
        let old = world.players.clone();
        let old_score = world.score;
        let old_flags = world.flags.clone();
        let old_ball = world.ball.clone();
        let old_rabbit = world.rabbit.clone();
        let was = old.get(world.player_id).cloned();
        let input = world.input.clone();
        self.ball_flown = 0.0;
        if !world.apply_snapshot(state, id) {
            if world.apply_spectator_snapshot(state, id) {
                // Watching: nothing to predict or acknowledge.
                self.pending.clear();
                let new_round = state.round != self.round;
                self.tick = state.tick; self.round = state.round; self.phase = state.phase;
                self.remember(state, new_round || state.map != old_map);
                if was_online && !new_round && state.blast_serial > old_blast {
                    let count = (state.blast_serial - old_blast).min(state.explosions.len() as u64) as usize;
                    for e in state.explosions.iter().rev().take(count) { world.spatial_sounds.push(("boom", e.pos)); }
                }
                self.place_remotes(world, 0.0);
                world.input = input;
                return;
            }
            log::warn!("snapshot rejected: tick {} map {:?} me {} present {} equipment {}", state.tick, state.map, id,
                state.players.iter().any(|p| p.net_id == id), state.equipment.len());
            return;
        }
        let me = world.player_id;
        let new_round = state.round != self.round;
        let team_changed = was.as_ref().is_some_and(|p| p.net_id == id && p.team != world.players[me].team);
        let ack = state.acks[me];
        while self.pending.front().is_some_and(|(c, _)| c.seq <= ack) {
            if let Some((_, sent)) = self.pending.pop_front() { self.latency_ms = sent.elapsed().as_secs_f32() * 1000.0; }
        }
        if new_round || team_changed { self.pending.clear(); }
        self.tick = state.tick; self.round = state.round; self.phase = state.phase;
        self.remember(state, new_round || state.map != old_map);
        self.phase_left = state.phase_left; self.phase_time = state.tick as f64 * STEP as f64;
        let current = world.players[me].clone();
        if !new_round && was_online {
            if state.blast_serial > old_blast {
                let count = (state.blast_serial - old_blast).min(state.explosions.len() as u64) as usize;
                for e in state.explosions.iter().rev().take(count) {
                    world.spatial_sounds.push(("boom", e.pos));
                }
            }
            // Flag captures and touchdowns only: Capture & Hold and deathmatch
            // scores tick up constantly (held points, frags), which played the
            // capture sting over and over.
            let scored = state.ball.active || state.mode.rules() == peakrunner_core::modes::Rules::Ctf;
            if old_score != state.score && scored {
                for team in 0..2 {
                    // Score decreases on reset are not captures; duplicate/stale
                    // snapshots are already rejected above.
                    let (win, loss) = if state.ball.active { ("touchdown_win,", "touchdown_loss,") }
                        else { ("capture_win,", "capture_loss,") };
                    for _ in 0..state.score[team].saturating_sub(old_score[team]).min(3) {
                        world.events.push_str(if team == current.team.idx() { win } else { loss });
                    }
                }
            }
            else if old_flags.iter().zip(&state.flags).any(|(a, b)| a.carrier != b.carrier) {
                world.events.push_str("flag,");
            }
            // The rabbit flag: taken, dropped or thrown, then home or gone.
            let r = &state.rabbit;
            if r.carrier.is_some() && r.carrier != old_rabbit.carrier { world.events.push_str("flag,"); }
            else if old_rabbit.carrier.is_some() && r.carrier.is_none() && r.active { world.events.push_str("drop,"); }
            else if old_rabbit.active && old_rabbit.carrier.is_none() && old_rabbit.drop_timer > 0.0
                && (!r.active || (r.drop_timer <= 0.0 && r.pos.distance(old_rabbit.pos) > 3.0)) { world.events.push_str("return,"); }
            let (events, spatial) = crate::football_hud::network_events(&old_ball, &state.ball, &old, &state.players, me, current.pos);
            for e in events { world.events.push_str(e); world.events.push(','); }
            world.spatial_sounds.extend(spatial);
            for (i, p) in state.players.iter().enumerate() {
                if i != me && p.net_id != 0 && p.pos.distance(current.pos) < 120.0 {
                    if old.get(i).is_some_and(|o| o.net_id == p.net_id && p.shots > o.shots) {
                        world.spatial_sounds.push((match p.weapon { 0 => "disc", 1 => "chain", _ => "grenade" }, p.pos));
                    }
                }
            }
        }
        if let Some(prev) = &was {
            if prev.net_id == id && !new_round {
                if current.hits > prev.hits { world.hitmarker = 1.0; world.events.push_str("hit,"); }
                if current.health < prev.health { world.damage_flash = 0.6; world.events.push_str("pain,"); }
                if prev.alive && !current.alive { world.events.push_str("death,"); }
            }
        }
        let events = world.events.clone();
        let trauma = world.trauma;
        if matches!(self.phase, Phase::Playing | Phase::Waiting) {
            for (command, _) in &self.pending { world.predict_command(*command); }
        }
        world.events = events;
        world.trauma = trauma;
        if let Some(prev) = was.filter(|p| p.net_id == id && p.alive == current.alive && !new_round && !team_changed) {
            let correction = prev.pos - world.players[me].pos;
            world.net_camera_offset = if correction.length() < 2.0 {
                (world.net_camera_offset + correction).clamp_length_max(2.0)
            } else { glam::Vec3::ZERO };
            // Keep the between-steps origin in the corrected frame, so the
            // correction is smoothed once (net_camera_offset), not twice.
            world.interp_from -= correction;
            // Mouse look is immediate, including between simulation steps.
            world.players[me].yaw = prev.yaw;
            world.players[me].pitch = prev.pitch;
        } else {
            world.net_camera_offset = glam::Vec3::ZERO;
            // Respawn, new round or team change: nothing to blend from.
            world.interp_from = world.players[me].pos;
        }
        self.place_remotes(world, 0.0);
        world.input = input;
        if team_changed {
            self.balance_notice_until = Some(Instant::now() + std::time::Duration::from_secs(4));
            self.switch_notice = self.switch_asked.take().is_some_and(|t| t.elapsed().as_secs_f32() < 5.0);
        }
        if self.balance_notice_until.is_some_and(|until| until > Instant::now()) {
            world.message = if !self.switch_notice { "AUTO-BALANCED — REDEPLOYED TO THE OTHER TEAM".into() }
                else if world.players[me].team == crate::sim::Team::Ember { "SWITCHED TEAMS — NOW PLAYING FOR EMBER".into() }
                else { "SWITCHED TEAMS — NOW PLAYING FOR GLACIER".into() };
        }
    }

    /// Seconds before play in the server's round countdown (None outside
    /// one): the newest snapshot's, run down smoothly by the estimated server
    /// clock between snapshots.
    pub fn countdown_left(&self) -> Option<f32> {
        (self.phase == Phase::Countdown).then(|| (self.phase_left - (self.clock - self.phase_time) as f32).clamp(0.0, self.phase_left))
    }

    pub fn advance(&mut self, world: &mut World, dt: f32, active: bool, station: bool, session: &peakrunner_net::Session) -> bool {
        let dt = dt.clamp(0.0, 0.1);
        world.time += dt;
        world.hitmarker = (world.hitmarker - dt * 3.5).max(0.0);
        world.damage_flash = (world.damage_flash - dt * 2.8).max(0.0);
        world.trauma = (world.trauma - dt * 1.6).max(0.0);
        world.net_camera_offset *= (-dt * 18.0).exp();
        let mut input = world.input.clone();
        if active {
            let limit = world.look_limit();
            if let Some(p) = world.players.get_mut(world.player_id) {
                p.yaw -= input.look_stick_x * 1.8 * dt;
                p.pitch = (p.pitch - input.look_stick_y * 1.4 * dt).clamp(-limit, limit);
            }
        }
        self.acc = (self.acc + dt).min(STEP * 5.0);
        world.steps_last_frame = 0;
        // The time of the other players on screen (last frame's), sent while
        // firing so the server can test a laser shot where they were drawn,
        // and always in Football, for body checks (`sim::lagcomp`).
        let football = world.football();
        let (view, ahead) = self.claim(football);
        while self.acc >= STEP {
            world.steps_last_frame += 1;
            self.acc -= STEP;
            let Some(p) = world.players.get(world.player_id) else { return false; };
            self.seq += 1;
            let command = Command { seq: self.seq,
                move_x: if active { input.move_x } else { 0.0 }, move_z: if active { input.move_z } else { 0.0 },
                yaw: p.yaw.rem_euclid(std::f32::consts::TAU), pitch: p.pitch,
                jump: active && input.jump, jet: active && input.jet, fire: active && input.fire,
                interact:(active || station) && input.interact, repair: active && input.repair, buy: if active || station { input.buy } else { 0 },
                equip: if active || station { input.equip } else { None }, toss: active && input.toss, mode: active && input.mode, ability: active && input.ability, operate: if active { input.operate } else { 0 }, waypoint: input.waypoint.filter(|_| active), drop: if active { input.drop } else { 0 }, board: active && input.board, deploy: active && input.deploy, deploy_turn: input.deploy_turn, suicide: active && input.suicide,
                throw: if active { input.throw } else { 0 }, throw_strength: if active { input.throw_strength } else { 0.0 }, weapon: input.weapon,
                view: claimed_view(view, active, input.fire, football), ahead: if active && football { ahead } else { 0 } };
            // Acknowledgements running 2 s behind: keep playing on the newest
            // inputs (the old ones can't be replayed usefully any more). A dead
            // link is caught by the session's own 5 s snapshot timeout.
            if self.pending.len() >= 120 {
                log::warn!("inputs unacknowledged for {:.1} s ({} queued; last snapshot tick {}, phase {:?}); dropping the oldest",
                    self.pending.front().map_or(0.0, |(_, t)| t.elapsed().as_secs_f32()), self.pending.len(), self.tick, self.phase);
                while self.pending.len() >= 60 { self.pending.pop_front(); }
            }
            if !session.send_input(command) {
                // The network thread is behind; this input is dropped, not the match.
                log::warn!("input queue full; input {} dropped", command.seq);
                input.clear_once();
                continue;
            }
            // One-shot intents go in this command only.
            input.clear_once();
            // A spectator's inputs only keep the session alive.
            if world.spectating { continue; }
            self.pending.push_back((command, Instant::now()));
            world.mark_step();
            if matches!(self.phase, Phase::Playing | Phase::Waiting) { world.predict_command(command); }
        }
        world.interpolate_local(self.acc / STEP);
        world.input = input;
        self.animate(world, dt);
        true
    }

    /// What a command claims about the others on screen (last frame's):
    /// in Football the snapshot they coasted on from and how far past it, in
    /// sixteenths of a tick (`place_ahead`); elsewhere the delayed time
    /// they're drawn at (`view`).
    fn claim(&self, football: bool) -> (u64, u16) {
        let Some(newest) = self.history.back() else { return (0, 0) };
        if !football { return (self.view(), 0); }
        let subticks = crate::sim::lagcomp::VIEW_SUBTICKS as f32;
        (crate::sim::lagcomp::view_at(newest.time / STEP as f64), (self.lead / STEP * subticks).round() as u16)
    }

    /// The server time other players are drawn at, as a command's `view`
    /// (0 before any snapshot).
    fn view(&self) -> u64 {
        if self.history.is_empty() { return 0; }
        crate::sim::lagcomp::view_at((self.clock - INTERP_DELAY) / STEP as f64)
    }

    /// Everything drawn between snapshots: remote players at the delayed
    /// clock, and projectiles and effects extrapolated (visuals only; hits
    /// are never resolved locally).
    fn animate(&mut self, world: &mut World, dt: f32) {
        self.clock += dt as f64;
        let ahead = self.place_remotes(world, dt);
        for d in &mut world.discs { d.pos += d.vel * dt; d.spin += dt * 18.0; d.life -= dt; }
        for e in &mut world.explosions { e.age += dt; }
        for p in &mut world.smoke { p.age += dt; }
        // In Football the ball went ahead with the others (`place_ahead`).
        if !ahead { advance_ball(&mut world.ball, world.map, dt, &mut self.ball_flown); }
        for f in &mut world.flags {
            if let Some(carrier) = f.carrier.and_then(|i| world.players.get(i)) { f.pos = carrier.pos + Vec3::Y * 2.2; }
        }
    }

    /// Keep this snapshot's remote poses; `reset` (a new round or map)
    /// forgets older ones so nothing blends across the change.
    fn remember(&mut self, state: &Snapshot, reset: bool) {
        let time = state.tick as f64 * STEP as f64;
        if reset { self.history.clear(); }
        // Pull the clock toward the snapshot's time: gently, to average out
        // arrival jitter, or at once when far off (joining, a long stall).
        let error = time - self.clock;
        if self.history.is_empty() || error.abs() > 0.25 { self.clock = time; } else { self.clock += error * 0.1; }
        self.history.push_back(Frame { time, poses: state.players.iter().map(|p| Pose { net_id: p.net_id, alive: p.alive,
            pos: p.pos, vel: p.vel, yaw: p.yaw, pitch: p.pitch }).collect(),
            rides: state.vehicles.iter().map(|c| Ride { kind: c.kind, team: c.team, pos: c.pos, vel: c.vel, yaw: c.yaw, pitch: c.pitch }).collect() });
        while self.history.len() > HISTORY { self.history.pop_front(); }
    }

    /// Put every remote player where they were INTERP_DELAY ago, or in
    /// Football (playing, not watching) ahead with you (`place_ahead`,
    /// true). `dt`: seconds since the last frame.
    fn place_remotes(&mut self, world: &mut World, dt: f32) -> bool {
        if world.football() && !world.spectating && self.place_ahead(world, dt) { return true; }
        self.place_delayed(world);
        false
    }

    /// Football: every remote player, and the loose ball, where they'd be at
    /// your own body's moment, `lead` past the newest snapshot: the pending
    /// commands the server hasn't answered yet (your body is predicted
    /// through them) less the step it's drawn back between steps. Each is
    /// coasted on from that snapshot (`football::coast`); when a new one
    /// moves the guess, the body slides over from where it was drawn rather
    /// than jumping. False with no snapshot yet.
    fn place_ahead(&mut self, world: &mut World, dt: f32) -> bool {
        let Some(newest) = self.history.back() else { return false };
        let lead = (((self.pending.len() as f32 - 1.0).max(0.0) * STEP + self.acc).min(MAX_LEAD)).max(0.0);
        self.lead = lead;
        let (me, map, fade) = (world.player_id, world.map, (-dt / SLIDE).exp());
        self.drawn.resize(world.players.len(), None);
        for (i, p) in world.players.iter_mut().enumerate() {
            let pose = newest.poses.get(i).copied().filter(|o| o.net_id == p.net_id && p.net_id != 0);
            let Some(pose) = pose.filter(|_| i != me) else { self.drawn[i] = None; continue };
            let target = if pose.alive { crate::sim::football::coast(map, pose.pos, pose.vel, lead).0 } else { pose.pos };
            let was = self.drawn[i].filter(|d| d.net_id == pose.net_id);
            let (mut slide, mut turn, mut tilt) = was.map_or((Vec3::ZERO, 0.0, 0.0), |d| (d.slide, d.turn, d.tilt));
            if let Some(d) = was.filter(|d| d.base != newest.time) {
                // A new snapshot: carry on from where it was drawn.
                let off = d.pos - target;
                (slide, turn, tilt) = if off.length() < SNAP && pose.alive {
                    let t = d.yaw - pose.yaw;
                    (off, t.sin().atan2(t.cos()), d.pitch - pose.pitch)
                } else { (Vec3::ZERO, 0.0, 0.0) };
            }
            (slide, turn, tilt) = (slide * fade, turn * fade, tilt * fade);
            (p.pos, p.yaw, p.pitch) = (target + slide, pose.yaw + turn, pose.pitch + tilt);
            self.drawn[i] = Some(Drawn { net_id: pose.net_id, base: newest.time, pos: p.pos, yaw: p.yaw, pitch: p.pitch, slide, turn, tilt });
        }
        let flown = self.ball_flown;
        advance_ball(&mut world.ball, map, (lead - flown).max(0.0), &mut self.ball_flown);
        true
    }

    /// Every remote player where they were INTERP_DELAY ago.
    fn place_delayed(&self, world: &mut World) {
        let render = self.clock - INTERP_DELAY;
        let me = world.player_id;
        for (i, p) in world.players.iter_mut().enumerate() {
            if i == me || p.net_id == 0 { continue; }
            let pose = |f: &Frame| f.poses.get(i).copied().filter(|o| o.net_id == p.net_id);
            // The newest pose at or before `render`, and the one after it.
            let before = self.history.iter().rev().find(|f| f.time <= render).and_then(|f| pose(f).map(|o| (f.time, o)));
            let after = self.history.iter().find(|f| f.time > render).and_then(|f| pose(f).map(|o| (f.time, o)));
            let (pos, yaw, pitch) = match (before, after) {
                (Some((t0, a)), Some((t1, b))) => {
                    // Continuous only if alive throughout and no farther apart
                    // than their speed allows (else a respawn or teleport).
                    let span = (t1 - t0) as f32;
                    let reach = a.vel.length().max(b.vel.length()) * span * 1.5 + 2.0;
                    if a.alive != b.alive || a.pos.distance(b.pos) > reach {
                        let o = if render - t0 < (t1 - t0) / 2.0 { a } else { b };
                        (o.pos, o.yaw, o.pitch)
                    } else {
                        let k = ((render - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
                        let turn = (b.yaw - a.yaw).sin().atan2((b.yaw - a.yaw).cos());
                        (a.pos.lerp(b.pos, k), a.yaw + turn * k, a.pitch + (b.pitch - a.pitch) * k)
                    }
                }
                // Past the newest snapshot: coast briefly on its velocity.
                (Some((t0, a)), None) => {
                    let ahead = if a.alive { (render - t0).min(MAX_EXTRAPOLATE) as f32 } else { 0.0 };
                    (a.pos + a.vel * ahead, a.yaw, a.pitch)
                }
                (None, Some((_, b))) => (b.pos, b.yaw, b.pitch),
                (None, None) => continue,
            };
            p.pos = pos; p.yaw = yaw; p.pitch = pitch;
        }
        // Vehicles others drive, the same way.
        for (v, c) in world.vehicles.iter_mut().enumerate() {
            if c.rider(0) == Some(me) { continue; }
            let ride = |f: &Frame| f.rides.get(v).copied().filter(|r| r.kind == c.kind && r.team == c.team);
            let before = self.history.iter().rev().find(|f| f.time <= render).and_then(|f| ride(f).map(|r| (f.time, r)));
            let after = self.history.iter().find(|f| f.time > render).and_then(|f| ride(f).map(|r| (f.time, r)));
            let (pos, yaw, pitch) = match (before, after) {
                (Some((t0, a)), Some((t1, b))) => {
                    let span = (t1 - t0) as f32;
                    if a.pos.distance(b.pos) > a.vel.length().max(b.vel.length()) * span * 1.5 + 2.0 {
                        let r = if render - t0 < (t1 - t0) / 2.0 { a } else { b };
                        (r.pos, r.yaw, r.pitch)
                    } else {
                        let k = ((render - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
                        let turn = (b.yaw - a.yaw).sin().atan2((b.yaw - a.yaw).cos());
                        (a.pos.lerp(b.pos, k), a.yaw + turn * k, a.pitch + (b.pitch - a.pitch) * k)
                    }
                }
                (Some((t0, a)), None) => (a.pos + a.vel * (render - t0).min(MAX_EXTRAPOLATE) as f32, a.yaw, a.pitch),
                _ => continue,
            };
            c.pos = pos; c.yaw = yaw; c.pitch = pitch;
        }
    }
}
