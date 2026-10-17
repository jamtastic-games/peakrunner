//! Lag compensation for the laser rifle (marker `lagcomp1`) and Football's
//! hits (`ball6`, `bump::World::seen_hit`).
//!
//! A client draws other players about 100 ms behind its estimate of the
//! server clock (`src/online.rs`), and its commands reach the server half a
//! round trip later. Without help, a laser aimed dead on a skier misses by
//! however far they moved in that time. So each command that holds fire
//! carries the server time of the view the client was drawing (`view`), and
//! the server keeps a short history of every player's position, one entry per
//! tick. A laser shot is tested against the other players where they were at
//! that time; the shooter, the world, deployables, equipment and mines stay
//! as they are now.
//!
//! The client only names a time, never a victim or a hit, and the server
//! clamps it: never ahead of now, never more than MAX_REWIND_TICKS back. A
//! victim who died, respawned or teleported since then is tested where they
//! are now. The cost: a target can be hit up to the rewind window after
//! ducking behind cover, as they looked to the shooter. Bots and offline play
//! send no view, so they shoot at the present. Projectiles (discs, chaingun,
//! grenades, mortar, railgun) are simulated forward on the server and are
//! not rewound.
//!
//! Football draws the others differently (`src/online.rs`): coasted on
//! from the newest snapshot (`football::coast`) to where they'd be by the
//! time the player's own body is, as the classic engine did, so people meet
//! about where they really meet and the server judges hits between people
//! at the present. A person's Football commands still name the snapshot
//! their drawing coasted on from (`view`) and how far (`ahead`), so a hit
//! on a bot (nobody to feel it) counts where it was drawn, even when the bot
//! turned since (`bump::seen_hit`): a drawing of no later than now and no
//! earlier than MAX_BEHIND_TICKS before it.
use super::*;
use std::collections::VecDeque;

/// Longest rewind in ticks (250 ms at 60 Hz): the client's 100 ms drawing
/// delay, a round trip of up to about 120 ms and a tick or two of input
/// buffering. Older claims are clamped to it, so a slower connection has to
/// lead a little again rather than shoot further into the past.
pub const MAX_REWIND_TICKS: u64 = 15;
/// A command's `view` counts sixteenths of a tick (about 1 ms); 0 means none.
pub const VIEW_SUBTICKS: u64 = 16;
/// Largest `view` a valid command may carry (far beyond any real tick count;
/// it keeps the conversion to seconds exact).
pub const MAX_VIEW: u64 = 1 << 52;

/// Largest `ahead` a valid command may carry (half a second); the server
/// coasts at most MAX_REWIND_TICKS.
pub const MAX_AHEAD: u16 = 30 * VIEW_SUBTICKS as u16;

/// Furthest behind the present a Football drawing counts (100 ms): an
/// honest one is drawn at the player's own body's moment, behind the server
/// by about a tick, a few more when frames come slowly (one frame's claim
/// goes with each of its commands) or a hitch holds commands back. Claimed
/// further back (an old view, coasted on little or not at all), a bot was
/// hit where it had been up to a quarter second before: a chaser 3 m behind
/// a carrier knocked the ball loose without ever touching it. Such a
/// drawing is coasted on to this.
pub const MAX_BEHIND_TICKS: u64 = 6;

/// `time_ticks` (a server time in ticks) as a command's `view`.
pub fn view_at(time_ticks: f64) -> u64 {
    if time_ticks.is_finite() && time_ticks > 0.0 { ((time_ticks * VIEW_SUBTICKS as f64).round() as u64).min(MAX_VIEW) } else { 0 }
}

#[derive(Clone, Copy, Debug)]
struct Sample { net_id: u32, alive: bool, pos: Vec3, vel: Vec3 }

impl Sample {
    fn of(p: &Player) -> Self { Self { net_id: p.net_id, alive: p.alive, pos: p.pos, vel: p.vel } }
}

/// Recent player positions, and the view each shooter claims this tick.
#[derive(Debug, Default)]
pub struct History {
    /// Oldest first; at most MAX_REWIND_TICKS + 1 ticks.
    frames: VecDeque<(u64, Vec<Sample>)>,
    /// The tick being simulated.
    now: u64,
    /// Per slot: the claimed view (`Command::view`), 0 for none, and how far
    /// past it the others were drawn (`Command::ahead`).
    views: Vec<(u64, u16)>,
}

impl History {
    /// Start tick `now`, with each slot's claimed view.
    pub fn begin(&mut self, now: u64, views: impl IntoIterator<Item = u64>) {
        self.begin_claims(now, views.into_iter().map(|v| (v, 0)));
    }

    /// Start tick `now`, with each slot's claimed view and how far past it
    /// the others were drawn.
    pub fn begin_claims(&mut self, now: u64, claims: impl IntoIterator<Item = (u64, u16)>) {
        self.now = now;
        self.views.clear();
        self.views.extend(claims);
    }

    /// Keep where everyone is at the end of tick `tick`.
    pub fn record(&mut self, tick: u64, players: &[Player]) {
        if self.frames.back().is_some_and(|(t, _)| *t >= tick) { self.frames.clear(); }
        while self.frames.front().is_some_and(|(t, _)| t + MAX_REWIND_TICKS < tick) { self.frames.pop_front(); }
        // Full: reuse the oldest tick's allocation.
        let mut samples = if self.frames.len() as u64 > MAX_REWIND_TICKS {
            self.frames.pop_front().map(|(_, s)| s).unwrap_or_default()
        } else { Vec::new() };
        samples.clear();
        samples.extend(players.iter().map(Sample::of));
        self.frames.push_back((tick, samples));
    }

    /// Ticks recorded, oldest and newest.
    pub fn span(&self) -> Option<(u64, u64)> {
        Some((self.frames.front()?.0, self.frames.back()?.0))
    }

    /// How far back (ticks) `shooter`'s shot this tick is tested: its claim,
    /// never ahead of now and never more than MAX_REWIND_TICKS back, nor
    /// further than the history kept goes (begun a few ticks ago, it holds
    /// nothing older: `state` gave its oldest tick, and a drawing coasted
    /// on from there by the whole claim went past now).
    pub fn rewind(&self, shooter: usize) -> f64 {
        let view = self.views.get(shooter).map_or(0, |c| c.0);
        if view == 0 { return 0.0; }
        let kept = self.frames.front().map_or(0, |(t, _)| self.now.saturating_sub(*t));
        (self.now as f64 - view as f64 / VIEW_SUBTICKS as f64).clamp(0.0, MAX_REWIND_TICKS.min(kept) as f64)
    }

    /// Seconds past its view that `slot` drew the others, coasting them on
    /// (Football): never past now (its game drew the moment of its own body,
    /// which the server has reached), so at most its rewind, and never more
    /// than MAX_BEHIND_TICKS short of now. Claimed apart, a view of now and
    /// the most ahead coasted a bot a quarter second into its future, and an
    /// old view not coasted on hit it a quarter second in its past.
    pub fn ahead(&self, slot: usize) -> f32 {
        let claimed = self.views.get(slot).map_or(0, |c| c.1) as f32 / VIEW_SUBTICKS as f32;
        let rewind = self.rewind(slot) as f32;
        claimed.clamp((rewind - MAX_BEHIND_TICKS as f32).max(0.0), rewind) * STEP
    }

    /// Where player `slot` (now `current`) is tested for a shot rewound
    /// `rewind` ticks: between the two recorded ticks around that time, or
    /// where they are now if they died, respawned or teleported since.
    pub fn position(&self, slot: usize, rewind: f64, current: &Player) -> Vec3 {
        self.state(slot, rewind, current).map_or(current.pos, |(pos, _)| pos)
    }

    /// Player `slot`'s position and velocity `rewind` ticks back, between
    /// the two recorded ticks around that time; None for the present (no
    /// rewind or no history), or if they died, respawned or teleported since.
    pub fn state(&self, slot: usize, rewind: f64, current: &Player) -> Option<(Vec3, Vec3)> {
        if rewind <= 0.0 || self.frames.is_empty() { return None; }
        let at = self.now as f64 - rewind;
        let now = (self.now, Sample::of(current));
        let mut track = self.frames.iter().filter_map(|(t, s)| s.get(slot).map(|s| (*t, *s)))
            .filter(|(t, _)| *t < self.now).chain(std::iter::once(now)).peekable();
        // The newest recorded tick at or before `at` (or the oldest kept).
        let mut from = track.next().expect("the present is always there");
        while let Some(next) = track.peek().filter(|(t, _)| *t as f64 <= at).copied() { from = next; track.next(); }
        let rest: Vec<(u64, Sample)> = track.collect();
        // Continuous from then to now: the same player, alive throughout, and
        // no step farther than their speed allows.
        let mut prev = from;
        for &(t, s) in &rest {
            let span = (t - prev.0) as f32 * STEP;
            let reach = prev.1.vel.length().max(s.vel.length()) * span * 1.5 + 2.0;
            if s.net_id != prev.1.net_id || !s.alive || !prev.1.alive || s.pos.distance(prev.1.pos) > reach { return None; }
            prev = (t, s);
        }
        let Some(&(t1, to)) = rest.first() else { return Some((from.1.pos, from.1.vel)) };
        let k = ((at - from.0 as f64) / (t1 - from.0) as f64).clamp(0.0, 1.0) as f32;
        Some((from.1.pos.lerp(to.pos, k), from.1.vel.lerp(to.vel, k)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::rifles::{RIFLE_SLOT};

    fn player(pos: Vec3, vel: Vec3) -> Player {
        let mut p = make_player(Team::Glacier, false, pos, 0.0, BotRole::Offense);
        p.net_id = 7; p.vel = vel; p
    }

    #[test]
    fn history_is_bounded_evicts_the_oldest_and_resets_on_a_clock_jump() {
        let mut h = History::default();
        let mut players = vec![player(Vec3::ZERO, Vec3::ZERO)];
        for tick in 1..=100 {
            players[0].pos.x = tick as f32;
            h.record(tick, &players);
            assert!(h.frames.len() as u64 <= MAX_REWIND_TICKS + 1);
        }
        assert_eq!(h.span(), Some((100 - MAX_REWIND_TICKS, 100)));
        // A gap (nobody connected for a while) drops what is too old.
        h.record(200, &players);
        assert_eq!(h.span(), Some((200, 200)));
        // A clock that runs backwards (a fresh match) starts over.
        h.record(150, &players);
        assert_eq!(h.span(), Some((150, 150)));
    }

    #[test]
    fn rewind_is_clamped_to_the_cap_and_never_into_the_future() {
        let mut h = History::default();
        let now = 1000;
        let claim = |ticks_ago: f64| view_at(now as f64 - ticks_ago);
        let claims = [0, claim(6.5), claim(40.0), claim(-5.0), claim(MAX_REWIND_TICKS as f64)];
        h.begin(now, claims);
        assert_eq!(h.rewind(1), 0.0, "no history kept: nothing to rewind to");
        for tick in now - 20..now { h.record(tick, &[player(Vec3::ZERO, Vec3::ZERO)]); }
        h.begin(now, claims);
        assert_eq!(h.rewind(0), 0.0, "no claim: the present");
        assert!((h.rewind(1) - 6.5).abs() < 1e-9, "sub-tick claims: {}", h.rewind(1));
        assert_eq!(h.rewind(2), MAX_REWIND_TICKS as f64, "older claims are clamped");
        assert_eq!(h.rewind(3), 0.0, "future claims are the present");
        assert_eq!(h.rewind(4), MAX_REWIND_TICKS as f64);
        assert_eq!(h.rewind(99), 0.0, "an unknown slot claims nothing");
        assert_eq!((view_at(f64::NAN), view_at(-3.0), view_at(1e300)), (0, 0, MAX_VIEW));
    }

    #[test]
    fn positions_interpolate_between_ticks_and_ignore_a_respawn() {
        let mut h = History::default();
        let v = Vec3::new(30.0, 0.0, 0.0);
        let mut players = vec![player(Vec3::ZERO, v)];
        for tick in 1..=20u64 {
            players[0].pos = v * tick as f32 * STEP;
            h.record(tick, &players);
        }
        players[0].pos = v * 21.0 * STEP;
        h.begin(21, [0]);
        let at = h.position(0, 4.25, &players[0]);
        assert!(at.distance(v * 16.75 * STEP) < 1e-4, "{at}");
        assert_eq!(h.position(0, 0.0, &players[0]), players[0].pos);
        // Beyond what is kept: the oldest kept tick.
        assert!(h.position(0, 30.0, &players[0]).distance(v * (20 - MAX_REWIND_TICKS) as f32 * STEP) < 1e-4);
        // Respawned since (a jump no speed explains): tested where they are now.
        let mut back = players[0].clone();
        back.pos = Vec3::new(500.0, 0.0, 0.0);
        assert_eq!(h.position(0, 4.25, &back), back.pos);
        // Someone else in the slot now: the same.
        let mut other = players[0].clone();
        other.net_id = 8;
        assert_eq!(h.position(0, 4.25, &other), other.pos);
    }

    /// A light-armor shooter with a laser, and a Glacier target 60 m in
    /// front of them skiing sideways at 30 m/s, in a two-player match.
    fn range() -> (Match, usize, usize, Vec3) {
        let mut game = Match::new(crate::terrain::MapId::OldHoller);
        let a = game.join(1, "Shooter").unwrap();
        let b = game.join(2, "Target").unwrap();
        assert_ne!(game.world.players[a].team, game.world.players[b].team);
        game.phase = Phase::Playing;
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        let floor = crate::terrain::support_on(crate::terrain::MapId::OldHoller, spot + Vec3::Y * 200.0).0;
        let s = &mut game.world.players[a];
        s.pos = Vec3::new(spot.x, floor + 40.0, spot.z); s.vel = Vec3::ZERO;
        s.armor = ArmorClass::Light; s.loadout.push(RIFLE_SLOT); s.energy = crate::defs::movement().energy_max; s.yaw = 0.0; s.pitch = 0.0;
        let start = s.pos + Vec3::new(-20.0, 0.0, -60.0);
        (game, a, b, start)
    }

    const SPEED: Vec3 = Vec3::new(30.0, 0.0, 0.0);

    /// One tick with the target flown along a straight line (placed ahead of
    /// physics, so where it ends tick T is exactly `start + SPEED * T * STEP`)
    /// and the shooter held in the air.
    fn fly(game: &mut Match, a: usize, b: usize, start: Vec3, command: Command) {
        let hold = game.world.players[a].pos;
        let mut list = vec![None; MAX_PLAYERS];
        list[a] = Some(command);
        let t = game.tick as f32 * STEP;
        let target = &mut game.world.players[b];
        target.pos = start + SPEED * t; target.vel = SPEED; target.health = 100.0;
        let shooter = &mut game.world.players[a];
        shooter.pos = hold; shooter.vel = Vec3::ZERO; shooter.energy = crate::defs::movement().energy_max;
        game.step(&list);
    }

    /// The shooter fires on the next tick, aimed dead on the target's chest
    /// where it was drawn `rewind_ticks` before that tick, sending that view
    /// or none. The target's health and the shooter's hits after.
    fn shoot(rewind_ticks: f64, send_view: bool) -> (f32, u32) {
        let (mut game, a, b, start) = range();
        let mut seq = 0;
        let mut next = |c: Command| { seq += 1; Command { seq, weapon: RIFLE_SLOT, ..c } };
        for _ in 0..60 { fly(&mut game, a, b, start, next(Command::default())); }
        game.world.players[a].cooldown = 0.0;
        let fire_tick = (game.tick + 1) as f64;
        // Recorded tick T holds the path at T; the present (the target hasn't
        // moved yet this tick when the shooter, in a lower slot, fires) is the
        // path at the last tick.
        let drawn = if rewind_ticks == 0.0 { game.world.players[b].pos }
            else { start + SPEED * ((fire_tick - rewind_ticks) as f32 * STEP) };
        let d = (drawn + Vec3::Y * 0.9) - (game.world.players[a].pos + Vec3::Y * EYE);
        let (yaw, pitch) = ((-d.x).atan2(-d.z), (d.y / d.length()).asin());
        let view = if send_view { view_at(fire_tick - rewind_ticks) } else { 0 };
        fly(&mut game, a, b, start, next(Command { yaw, pitch, fire: true, view, ..Command::default() }));
        (game.world.players[b].health, game.world.players[a].hits)
    }

    #[test]
    fn a_shot_at_the_drawn_position_hits_with_rewind_and_misses_without() {
        let (health, hits) = shoot(12.3, true);
        assert!(health <= 100.0 - crate::defs::weapons().laser.damage * 0.5 && hits == 1, "rewound shot hits: {health} {hits}");
        let (health, hits) = shoot(12.3, false);
        assert!(health == 100.0 && hits == 0, "the same aim without a view misses: {health}");
        // At the present, the view changes nothing.
        let (health, hits) = shoot(0.0, false);
        assert!(health < 100.0 && hits == 1, "a shot at the present hits");
    }

    #[test]
    fn claims_beyond_the_cap_are_clamped_not_trusted() {
        // Aimed at where the target was 30 ticks ago and claiming it: the
        // server only goes back MAX_REWIND_TICKS, so it misses.
        let (health, _) = shoot(30.0, true);
        assert_eq!(health, 100.0);
        // At exactly the cap it still counts.
        let (health, _) = shoot(MAX_REWIND_TICKS as f64, true);
        assert!(health < 100.0);
    }

    /// Football on a server: a person chasing a bot carrier that runs up
    /// the field at 12 m/s and, 6 ticks before the last, cuts sideways at
    /// 25. The person runs on where it would be had it gone straight, 10 m
    /// back, then 0.6 m back on the last tick, whose command claims a
    /// drawing coasted on from the snapshot `claim.0` ticks before that tick
    /// by `claim.1` ticks (None: no claim). Whether the bot was tackled.
    fn tackled(claim: Option<(f64, f64)>) -> bool {
        let map = crate::terrain::MapId::Longfield;
        let mut game = Match::new(map);
        game.rotate_to_mode(map, crate::map_catalog::SupportedMode::Football);
        let a = game.join(1, "Chaser").unwrap();
        // One a side: the person on one, a bot on the other.
        game.bots = 2;
        game.maintain_bots();
        game.phase = Phase::Playing;
        game.step(&[]);
        let b = (0..game.world.players.len()).find(|&k| game.world.players[k].is_bot && game.world.players[k].net_id != 0).expect("a bot");
        let floor = crate::terrain::support_on(map, Vec3::new(1024.0, 120.0, 990.0)).0 + 1.2;
        let start = Vec3::new(1024.0, floor, 990.0);
        let (run, side, cut, last) = (Vec3::Z * 12.0, Vec3::X * 25.0, 24u32, 30u32);
        let straight = |k: u32| start + run * (k as f32 * STEP);
        let path = |k: u32| if k <= cut { (straight(k), run) } else { (straight(cut) + side * ((k - cut) as f32 * STEP), side) };
        for k in 0..=last {
            let (at, vel) = path(k);
            let gap = if k == last { 0.6 } else { 10.0 };
            for (i, team, pos, vel) in [(a, Team::Ember, straight(k) - Vec3::Z * gap, Vec3::Z * 30.0), (b, Team::Glacier, at, vel)] {
                let p = &mut game.world.players[i];
                (p.alive, p.team, p.pos, p.vel, p.stun, p.on_ground) = (true, team, pos, vel, 0.0, true);
            }
            game.world.ball = crate::sim::football::Ball { active: true, in_play: true, carrier: Some(b), pos: at, ..Default::default() };
            let mut command = Command { seq: k as u64 + 1, ..Command::default() };
            if let Some((back, ahead)) = claim.filter(|_| k == last) {
                command.view = view_at((game.tick + 1) as f64 - back);
                command.ahead = (ahead * VIEW_SUBTICKS as f64) as u16;
            }
            let mut list = vec![None; MAX_PLAYERS];
            list[a] = Some(command);
            game.step(&list);
        }
        game.world.players[b].stun > 0.0
    }

    #[test]
    fn a_person_tackles_a_bot_where_their_game_drew_it() {
        assert!(tackled(Some((9.0, 9.0))), "drawn coasting on from before its cut: they touched");
        assert!(!tackled(None), "on the server they never met");
        assert!(!tackled(Some((3.0, 3.0))), "drawn from after the cut: it wasn't there");
    }

    /// A drawing is coasted on at most to the present: claiming a snapshot
    /// 2 ticks back drawn 15 ahead is drawn 2 ahead.
    #[test]
    fn a_drawing_is_never_coasted_past_now() {
        let mut h = History::default();
        let players = vec![player(Vec3::ZERO, Vec3::ZERO)];
        for tick in 80..100 { h.record(tick, &players); }
        let ahead = |ticks: u64| (ticks * VIEW_SUBTICKS) as u16;
        let claims = [(view_at(98.0), ahead(15)), (view_at(91.0), ahead(9)), (view_at(80.0), ahead(15)), (0, MAX_AHEAD)];
        h.begin_claims(100, claims);
        assert_eq!((h.ahead(0), h.ahead(1)), (2.0 * STEP, 9.0 * STEP));
        assert_eq!(h.ahead(2), MAX_REWIND_TICKS as f32 * STEP, "no further back than the rewind goes");
        assert_eq!(h.ahead(3), 0.0, "no view, nothing drawn ahead");
        // A history begun two ticks ago: nothing older to coast on from, so
        // no more than two ticks ahead (its oldest tick, coasted the whole
        // claim, was drawn 13 ticks into the future).
        let mut young = History::default();
        for tick in 98..100 { young.record(tick, &players); }
        young.begin_claims(100, claims);
        assert_eq!((young.rewind(2), young.ahead(2)), (2.0, 2.0 * STEP));
        assert_eq!(young.ahead(1), 2.0 * STEP);
    }

    /// Nor further behind it than MAX_BEHIND_TICKS: an old view coasted on
    /// little or not at all is coasted on to that.
    #[test]
    fn a_drawing_is_never_further_behind_than_the_bound() {
        let mut h = History::default();
        let players = vec![player(Vec3::ZERO, Vec3::ZERO)];
        for tick in 80..100 { h.record(tick, &players); }
        let ahead = |ticks: u64| (ticks * VIEW_SUBTICKS) as u16;
        h.begin_claims(100, [(view_at(85.0), 0), (view_at(85.0), ahead(5)), (view_at(85.0), ahead(12)), (view_at(97.0), 0)]);
        let least = (MAX_REWIND_TICKS - MAX_BEHIND_TICKS) as f32 * STEP;
        assert_eq!((h.ahead(0), h.ahead(1)), (least, least), "15 back, coasted 0 or 5: drawn 6 behind");
        assert_eq!(h.ahead(2), 12.0 * STEP, "3 behind: as claimed");
        assert_eq!(h.ahead(3), 0.0, "a view 3 back, not coasted: as claimed");
    }

    #[test]
    fn bots_and_offline_play_use_no_rewind() {
        let mut game = Match::new(crate::terrain::MapId::OldHoller);
        game.bots = 4;
        let me = game.join(1, "Person").unwrap();
        for _ in 0..30 { game.step(&[]); }
        let view = view_at(game.tick as f64 - 5.0);
        let mut seq = 0;
        let mut all = |game: &mut Match| { seq += 1; game.step(&vec![Some(Command { seq, view, ..Command::default() }); MAX_PLAYERS]); };
        all(&mut game);
        let bots: Vec<usize> = (0..MAX_PLAYERS).filter(|&i| game.world.players[i].is_bot && game.world.players[i].net_id != 0).collect();
        assert!(!bots.is_empty());
        assert!(game.world.lag.rewind(me) > 5.0, "a person's claim counts: {}", game.world.lag.rewind(me));
        for bot in bots { assert_eq!(game.world.lag.rewind(bot), 0.0, "a bot claims nothing"); }
        // Offline worlds keep no history and no views.
        let w = World::new();
        assert_eq!((w.lag.rewind(0), w.lag.span()), (0.0, None));
    }

    #[test]
    fn malformed_views_are_refused() {
        let ok = Command { seq: 1, view: view_at(1234.5), ..Command::default() };
        assert!(ok.valid());
        assert!(!Command { view: MAX_VIEW + 1, ..ok }.valid());
        assert!(!Command { view: u64::MAX, ..ok }.valid());
    }
}
