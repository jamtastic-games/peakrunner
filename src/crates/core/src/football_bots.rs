//! Football bots. At each think a bot reads the play and takes a job: carry
//! the ball, go for a loose one, block for the carrier, get open for a pass,
//! rush the enemy carrier, cover a receiver or stay deep. Every physics step
//! it plays that job (`football_inputs`): runs or hops to its spot, jumps
//! and jets to meet a ball in the air, spends its tank to win a hit, and
//! pounces on the player it's after. Server and offline only: nothing here
//! is sent or predicted.
//!
//! What they play by (`football.rs`): the faster body wins a hit, and at a
//! closing speed of 15 m/s the loser goes down; a ball touches anywhere on
//! the 2.3 m body; a pass's power comes from the player it's aimed at.
//!
//! A carrier reads the defenders as a person would (`fb_watch`, `fb_seen`):
//! a quarter second late, only those in front of it, and anyone it saw who
//! went out of view where they'd be at the speed it saw, for a while.

use super::*;
use super::football::JUMP_SPEED;

/// A bot's inputs for one step: keys right and forward, jump held, jump
/// pressed, fire, jet.
pub(super) type Inputs = (f32, f32, bool, bool, bool, bool);
const STILL: Inputs = (0.0, 0.0, false, false, false, false);

/// What a football bot is doing, from its last think.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Job {
    /// No ball in play: back to its side.
    #[default]
    Idle,
    /// Carrying the ball for the enemy end zone.
    Carry,
    /// Going for a loose ball: to catch it in the air or pick it up.
    Fetch,
    /// Clearing the way for the carrier: hits the enemy `mark`.
    Block,
    /// Getting open for a pass at `spot`.
    Receive,
    /// After `mark`: the enemy carrier, or whoever is about to catch.
    Rush,
    /// Marking the enemy receiver `mark` from between them and the ball
    /// (none: a zone at `spot`).
    Cover,
    /// Deep, between the play and our end zone.
    Safety,
}

/// A football bot's plan from its last think, and its jump in progress.
#[derive(Clone, Copy, Debug, Default)]
pub struct Brain {
    pub job: Job,
    /// Where it's going.
    pub spot: Vec3,
    /// The player it blocks, rushes or covers.
    pub mark: Option<usize>,
    /// The straight line to `spot` is blocked (a pillar): its route leads.
    pub blocked: bool,
    /// Seconds into a jump for a catch (0 = none).
    pub leap: f32,
    /// When it took the ball (world seconds), while it carries.
    pub since: f32,
    /// It took the ball in the air and hasn't landed since. The catch's push
    /// carries it as it would a person: while its way lies within 60 degrees
    /// of the push its run's jet adds to it, else it rides the push out with
    /// the jet off (there's no air control) and runs from where it lands.
    pub ride: bool,
}

/// The body catches a ball centred from 0.34 m below the feet to 0.34 over
/// its 2.3 m top, within 0.82 across (`football::catch_touch`). Bots plan
/// with a little margin.
const REACH_LOW: f32 = -0.25;
const REACH_HIGH: f32 = 2.45;
const REACH_ACROSS: f32 = 0.6;
/// Planning speeds (m/s): a run (`football::RUN_FORWARD`), and hopping
/// with the jet over longer distances.
const RUN: f32 = 11.0;
const TRAVEL: f32 = 15.0;
/// Farther than this a bot hops and jets; nearer, it runs.
const HOP_FROM: f32 = 22.0;
/// A ball this far over the standing reach is taken with a hop on the run;
/// higher, the bot stands under it and jumps with the jet.
const HOP_RISE: f32 = 0.6;
/// Seconds a bot takes to settle on its spot before a standing jump.
const SETTLE: f32 = 0.2;
/// A catch without a jet jump is worth waiting this much longer (s) for.
const EASIER: f32 = 0.5;
/// Planning steps through a ball's flight (s), and how far ahead.
const LOOK_STEP: f32 = 0.05;
const LOOK_AHEAD: f32 = 7.0;
/// Energy kept back while travelling: a carrier's for a burst, a rusher's
/// for the pounce, a receiver's for a catch.
const CARRY_RESERVE: f32 = 30.0;
const RUSH_RESERVE: f32 = 35.0;
const CATCH_RESERVE: f32 = 20.0;
/// Enemies this close to the carrier are threats blockers take on.
const THREAT_RANGE: f32 = 35.0;
/// An enemy reaching the carrier this soon (s) presses them to pass; until
/// then they run with it at least HOLD seconds, passing sooner only to a
/// receiver well up the field (GOOD_PASS: metres gained plus 6 per second
/// of margin over the enemy).
const PRESSED: f32 = 0.45;
const HOLD: f32 = 1.2;
const GOOD_PASS: f32 = 34.0;
/// A carrier's picture of the defenders (`World::fb_watch`): what it saw
/// REACTION_STEPS ago (0.25 s at 60 Hz; it acts on that), of those within
/// SIGHT degrees either side of the way it faced then; anyone it saw who
/// has since gone out of view, it pictures where they'd be at the speed it
/// last saw (it can't tell if they sped up, slowed or turned) for MEMORY
/// seconds.
const REACTION_STEPS: usize = 15;
const SIGHT: f32 = 65.0;
const MEMORY: f32 = 2.5;
/// Receivers look for space this far ahead of the ball and across.
const ROUTE_AHEAD: [f32; 4] = [10.0, 20.0, 32.0, 46.0];
const ROUTE_ACROSS: [f32; 5] = [-28.0, -14.0, 0.0, 14.0, 28.0];
/// A pass must reach a teammate this much sooner (s) than any enemy, and
/// no higher than this over their standing reach.
const PASS_RISE: f32 = 3.0;
const PASS_MARGIN: f32 = 0.45;
/// Throws a carrier tries: lead times on the receiver (s) and look pitches.
const PASS_LEADS: [f32; 3] = [0.0, 0.7, 1.4];
const PASS_PITCHES: [f32; 8] = [-0.2, -0.1, 0.0, 0.1, 0.22, 0.35, 0.5, 0.7];

/// Bots' names in Football.
pub const FOOTBALL_NAMES: [&str; 32] = [
    "Mike Ditza", "Joe Shameth", "Jerry Lice", "Barry Sandals", "Walter Paydirt",
    "Dick Butkiss", "Lawrence Tailor", "Joe Montanya", "Johnny Unitards", "Jim Brownie",
    "Dan Marinara", "Terry Brawdshaw", "Emmitt Smash", "Reggie Fight", "John Elbow",
    "Brett Favor", "Randy Mosh", "Bo Jacksaw", "Peyton Mansion", "Tom Bready",
    "Deion Sandwich", "Marshawn Lunch", "Earl Campfire", "Gale Sayerz", "Ronnie Lotto",
    "Roger Stopback", "Howie Lawn", "Troy Aikmen", "Larry Zonka", "Warren Moonbeam",
    "Fran Tarkentoast", "Mean Joe Spleen",
];

/// One player as anyone could have seen them at a moment
/// (`World::fb_glimpses`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Glimpse { net_id: u32, alive: bool, pos: Vec3, vel: Vec3, yaw: f32, stun: f32 }

impl Glimpse {
    fn of(p: &Player) -> Self { Self { net_id: p.net_id, alive: p.alive, pos: p.pos, vel: p.vel, yaw: p.yaw, stun: p.stun.max(0.0) } }
}

/// What a bot last saw of an enemy, and when (world seconds; `Player::bot_sight`).
#[derive(Clone, Copy, Debug)]
pub(super) struct Sighting { net_id: u32, pos: Vec3, vel: Vec3, stun: f32, at: f32 }

/// An enemy as a carrier pictures them now (`World::fb_seen`).
#[derive(Clone, Copy, Debug)]
struct Seen { pos: Vec3, vel: Vec3, stun: f32 }

/// Seconds for a player at `pos` moving at `vel` (down `stun` more
/// seconds) to reach `o` running on, and where: an intercept.
fn meet(pos: Vec3, vel: Vec3, stun: f32, o: &Player) -> (f32, Vec3) {
    let speed = flat(vel).length().max(RUN);
    let run = Vec3::new(o.vel.x, 0.0, o.vel.z);
    let mut t = flat(o.pos - pos).length() / speed;
    for _ in 0..4 { t = flat(o.pos + run * t.min(2.5) - pos).length() / speed; }
    (t + stun.max(0.0), o.pos + run * t.min(2.5))
}

/// A loose ball from now on: gravity and no drag, as `fly_ball` moves it
/// (bounces aside).
#[derive(Clone, Copy, Debug)]
pub(super) struct Flight {
    pub from: Vec3,
    pub vel: Vec3,
}

impl Flight {
    pub fn at(&self, t: f32) -> Vec3 {
        self.from + self.vel * t - Vec3::Y * (0.5 * crate::defs::movement().gravity * t * t)
    }
}

/// When and where a player can first take a loose ball: in the air (with
/// `rise` metres of jump over a standing reach) or where it comes down.
#[derive(Clone, Copy, Debug)]
pub(super) struct Catch {
    pub t: f32,
    pub at: Vec3,
    pub rise: f32,
    /// It comes down first: picked up off the ground.
    pub ground: bool,
}

/// A player as the bots plan with them: where they are, the ground under
/// them, how long until they land if they're in the air (they can't steer
/// there), their tank, the highest a jump on it lifts them, and how long
/// until they can move (down from a tackle).
#[derive(Clone, Copy, Debug)]
struct Body {
    pos: Vec3,
    vel: Vec3,
    feet: f32,
    land: f32,
    energy: f32,
    top: f32,
    busy: f32,
}

impl Body {
    /// Seconds to get within reach of `at` on the ground: from where it
    /// lands, if it's in the air.
    fn reach(&self, at: Vec3) -> f32 {
        let start = self.pos + Vec3::new(self.vel.x, 0.0, self.vel.z) * self.land.max(0.15);
        let d = (flat(at) - flat(start)).length() - REACH_ACROSS;
        let speed = if d > HOP_FROM { TRAVEL } else { RUN };
        self.busy.max(self.land) + d.max(0.0) / speed
    }

    /// Whether the ball at `b`, `t` seconds on, meets this body on its arc
    /// through the air (before it lands).
    fn in_flight(&self, b: Vec3, t: f32) -> bool {
        if t >= self.land { return false; }
        let g = crate::defs::movement().gravity;
        let feet = self.pos.y - PLAYER_RADIUS + self.vel.y * t - 0.5 * g * t * t;
        let h = b.y - feet;
        flat(b).distance(flat(self.pos + self.vel * t)) < REACH_ACROSS && (REACH_LOW..=REACH_HIGH).contains(&h)
    }

    /// The first moment this body can take the ball on flight `f`, after
    /// `reaction` seconds and no later than `until`: in reach in the air
    /// (an easy catch, standing or with a hop, if one comes within EASIER
    /// of a jet jump), or where it comes down.
    fn take(&self, f: &Flight, reaction: f32, until: f32) -> Option<Catch> {
        let mut hard: Option<Catch> = None;
        let mut t = 0.0;
        while t < LOOK_AHEAD && t <= until {
            if hard.is_some_and(|c| t > c.t + EASIER) { break; }
            let b = f.at(t);
            let h = b.y - self.feet;
            if h < REACH_LOW { break; }
            if self.in_flight(b, t) { return Some(Catch { t, at: b, rise: 0.0, ground: false }); }
            let rise = (h - REACH_HIGH).max(0.0);
            if rise <= self.top {
                let run = reaction + self.reach(b);
                if rise <= HOP_RISE {
                    if run <= t { return Some(Catch { t, at: b, rise, ground: false }); }
                } else if hard.is_none() {
                    // A standing jump: there, settled, then the climb.
                    let settle = if flat(b).distance(flat(self.pos)) > 1.0 { SETTLE } else { 0.0 };
                    let lift = rise_time(rise, self.energy).unwrap_or(f32::INFINITY);
                    if run + settle + lift <= t { hard = Some(Catch { t, at: b, rise, ground: false }); }
                }
            }
            t += LOOK_STEP;
        }
        if hard.is_some() { return hard; }
        if t > until { return None; }
        // It comes down first: picked up where it lands (it barely bounces).
        let land = f.at(t);
        let at = Vec3::new(land.x, self.feet + PLAYER_RADIUS, land.z);
        let t = t.max(reaction + self.reach(at));
        (t <= until).then_some(Catch { t, at, rise: 0.0, ground: true })
    }
}

/// The jump's lift (`classic_move`): its speed, the jet's net push while
/// the keys are let go, and how long `energy` keeps the jet on.
fn jump_terms(energy: f32) -> (f32, f32, f32, f32) {
    let g = crate::defs::movement().gravity;
    let armor = crate::defs::defs().armors.football;
    (JUMP_SPEED, g, armor.jet, (energy / (armor.drain - armor.regen)).max(0.0))
}

/// The highest a jump on `energy` lifts the body.
fn jump_top(energy: f32) -> f32 {
    let (v, g, jet, burn) = jump_terms(energy);
    // Rising all through the burn, then coasting up from its speed.
    let up = v + (jet - g) * burn;
    let at_burn = v * burn + 0.5 * (jet - g) * burn * burn;
    if up > 0.0 { at_burn + up * up / (2.0 * g) } else { v * v / (2.0 * g) }
}

/// How long a jump on `energy` takes to lift the body `r` metres.
fn rise_time(r: f32, energy: f32) -> Option<f32> {
    let (v, g, jet, burn) = jump_terms(energy);
    let a = jet - g;
    // While the jet burns: r = v t + a t² / 2.
    if burn > 0.0 {
        let t = (-v + (v * v + 2.0 * a * r).sqrt()) / a;
        if t <= burn { return Some(t); }
    }
    // Coasting after it: r = (v + jet b) t - g t² / 2 - jet b² / 2.
    let u = v + jet * burn;
    let disc = u * u - 2.0 * g * (r + 0.5 * jet * burn * burn);
    (disc >= 0.0).then(|| (u - disc.sqrt()) / g)
}

/// The yaw that faces from `from` to `to`.
fn yaw_to(from: Vec3, to: Vec3) -> f32 { (-(to.x - from.x)).atan2(-(to.z - from.z)) }

fn flat(v: Vec3) -> Vec2 { Vec2::new(v.x, v.z) }

impl World {
    fn fb_enemy(&self, team: Team, j: usize) -> bool {
        self.players.get(j).is_some_and(|o| o.alive && o.team != team)
    }

    /// Players who can play the ball: alive and on their feet.
    fn fb_up(&self, j: usize) -> bool {
        self.players.get(j).is_some_and(|o| o.alive && o.stun <= 0.0)
    }

    fn fb_body(&self, k: usize) -> Body {
        let p = &self.players[k];
        let (feet, land) = if p.on_ground { (p.pos.y - PLAYER_RADIUS, 0.0) } else {
            // Down to the ground under it on its arc.
            let floor = crate::terrain::support_on(self.map, p.pos).0;
            let (g, drop) = (crate::defs::movement().gravity, p.pos.y - PLAYER_RADIUS - floor);
            (floor, (p.vel.y + (p.vel.y * p.vel.y + 2.0 * g * drop.max(0.0)).sqrt()) / g)
        };
        Body { pos: p.pos, vel: p.vel, feet, land, energy: p.energy, top: jump_top(p.energy), busy: p.stun.max(0.0) }
    }

    /// The loose ball's flight, or None while it sits or rolls on the ground.
    fn fb_flight(&self) -> Option<Flight> {
        let b = &self.ball;
        if !b.loose() { return None; }
        let floor = crate::terrain::support_on(self.map, b.pos + Vec3::Y * 0.5).0;
        let grounded = b.pos.y - floor < football::BALL_RADIUS + 0.25 && b.vel.y.abs() < 2.0;
        (!grounded).then_some(Flight { from: b.pos, vel: b.vel })
    }

    /// The field's inside less `inset` metres (None on a map without one).
    fn fb_bounds(&self, inset: f32) -> Option<[f32; 4]> {
        football::field(self.map).and_then(|f| f.bounds).map(|[x0, z0, x1, z1]| [x0 + inset, z0 + inset, x1 - inset, z1 - inset])
    }

    fn fb_clamp(&self, p: Vec3) -> Vec3 {
        match self.fb_bounds(5.0) {
            Some([x0, z0, x1, z1]) => Vec3::new(p.x.clamp(x0, x1), p.y, p.z.clamp(z0, z1)),
            None => p,
        }
    }

    fn fb_inside(&self, p: Vec3) -> bool {
        self.fb_bounds(1.0).is_none_or(|[x0, z0, x1, z1]| p.x >= x0 && p.x <= x1 && p.z >= z0 && p.z <= z1)
    }

    /// Player `k`'s first take of the loose ball (None: can't play it).
    pub(super) fn fb_catch(&self, k: usize, reaction: f32) -> Option<Catch> {
        let b = &self.ball;
        if !self.fb_up(k) || (b.no_catch > 0.0 && b.thrown_by == Some(k)) { return None; }
        let body = self.fb_body(k);
        match self.fb_flight() {
            Some(f) => body.take(&f, reaction, f32::INFINITY),
            None => {
                let at = b.pos + Vec3::new(b.vel.x, 0.0, b.vel.z) * 0.2;
                Some(Catch { t: body.reach(at), at, rise: 0.0, ground: true })
            }
        }
    }

    /// Seconds for player `k` to reach player `c` running on, and where:
    /// an intercept.
    fn fb_meet(&self, k: usize, c: usize) -> (f32, Vec3) {
        let me = &self.players[k];
        meet(me.pos, me.vel, me.stun, &self.players[c])
    }

    /// Every step in Football with bots: keep what everyone looked like
    /// REACTION_STEPS back, and let each bot see, as of then, the enemies in
    /// front of it then. The others it saw stay in its memory MEMORY seconds
    /// from their last sighting; the dead and the gone are forgotten.
    pub(super) fn fb_watch(&mut self) {
        if !self.players.iter().any(|p| p.is_bot) { self.fb_glimpses.clear(); return; }
        // Full: reuse the oldest step's allocation.
        let mut now = if self.fb_glimpses.len() > REACTION_STEPS { self.fb_glimpses.pop_front().map(|g| g.1).unwrap_or_default() } else { Vec::new() };
        now.clear();
        now.extend(self.players.iter().map(Glimpse::of));
        self.fb_glimpses.push_back((self.time, now));
        let Some((then, past)) = self.fb_glimpses.front() else { return };
        let (then, n, wide) = (*then, self.players.len(), SIGHT.to_radians().cos());
        for i in 0..n {
            if !self.players[i].is_bot { continue; }
            let mut sight = std::mem::take(&mut self.players[i].bot_sight);
            if !self.players[i].alive { sight.clear(); self.players[i].bot_sight = sight; continue; }
            sight.resize(n, None);
            let team = self.players[i].team;
            let me = past.get(i).filter(|g| g.alive && g.net_id == self.players[i].net_id);
            for (k, seen) in sight.iter_mut().enumerate() {
                let o = &self.players[k];
                let Some(g) = past.get(k).filter(|g| k != i && o.alive && o.team != team && g.alive && g.net_id == o.net_id) else { *seen = None; continue };
                let ahead = me.is_some_and(|m| {
                    let rel = flat(g.pos - m.pos);
                    rel.length() < 1e-3 || (rel / rel.length()).dot(flat(look_dir(m.yaw, 0.0))) >= wide
                });
                *seen = if ahead { Some(Sighting { net_id: g.net_id, pos: g.pos, vel: g.vel, stun: g.stun, at: then }) }
                    else { seen.filter(|s| s.net_id == g.net_id && then - s.at <= MEMORY) };
            }
            self.players[i].bot_sight = sight;
        }
    }

    /// Enemy `k` as bot `i` pictures them (None: never seen, or forgotten):
    /// where it saw them REACTION_STEPS ago, or, out of view since, where
    /// they'd be by then had they kept the speed it saw.
    fn fb_seen(&self, i: usize, k: usize) -> Option<Seen> {
        let s = self.players[i].bot_sight.get(k).copied().flatten()?;
        let then = self.fb_glimpses.front().map_or(s.at, |g| g.0);
        let gone = (then - s.at).max(0.0);
        Some(Seen { pos: s.pos + Vec3::new(s.vel.x, 0.0, s.vel.z) * gone, vel: s.vel, stun: (s.stun - gone).max(0.0) })
    }

    /// The enemies carrier `i` knows of, as it pictures them.
    fn fb_defenders(&self, i: usize) -> Vec<Seen> {
        let team = self.players[i].team;
        (0..self.players.len()).filter(|&k| self.fb_enemy(team, k)).filter_map(|k| self.fb_seen(i, k)).collect()
    }

    /// The ball changed hands or came loose: every bot reads the play again
    /// at once, as a player would look up.
    pub(super) fn bots_look_up(&mut self) {
        for p in self.players.iter_mut().filter(|p| p.is_bot) { p.bot_think = p.bot_think.min(0.05); }
    }

    /// Bot `i`'s think in Football: read the play, take a job, maybe pass.
    /// Returns where it's heading.
    pub(super) fn football_think(&mut self, i: usize, jx: f32, jz: f32) -> Vec3 {
        self.players[i].bot_pass = false;
        let team = self.players[i].team;
        let old = self.players[i].bot_fb;
        let mut brain = Brain { leap: old.leap, since: if old.job == Job::Carry { old.since } else { self.time }, ..Brain::default() };
        let (in_play, carrier) = (self.ball.in_play, self.ball.carrier);
        if !in_play {
            brain.spot = self.kickoff_spot(team) + Vec3::new(jx * 2.0, 0.0, jz * 2.0);
        } else if carrier == Some(i) {
            brain.job = Job::Carry;
            // Just taken off the ground, or still up after such a catch.
            brain.ride = !self.players[i].on_ground && (old.job != Job::Carry || old.ride);
            brain.spot = self.carry_think(i, self.time - brain.since);
        } else if let Some(c) = carrier.filter(|&c| c < self.players.len()) {
            let origin = self.players[c].pos;
            if self.players[c].team == team { self.offense(i, c, origin, &[c], &mut brain); }
            else { self.defense(i, c, origin, &[c], None, &mut brain); }
        } else {
            self.loose_think(i, &mut brain);
        }
        brain.spot = self.fb_clamp(brain.spot);
        let pos = self.players[i].pos;
        brain.blocked = crate::map_pack::on(self.map).is_some_and(|pack| {
            let chest = Vec3::Y * 1.2;
            let to = Vec3::new(brain.spot.x, pos.y, brain.spot.z);
            pack.sweep(pos + chest, to + chest, 0.35).is_some()
        });
        self.players[i].bot_fb = brain;
        brain.spot
    }

    /// A loose ball: each team's first to it goes for it, and a second goes
    /// too, or hits the enemy who'll get there first; the rest play as if
    /// the team that gets there first already had it.
    fn loose_think(&self, i: usize, brain: &mut Brain) {
        let team = self.players[i].team;
        let catches: Vec<Option<Catch>> = (0..self.players.len()).map(|k| self.fb_catch(k, 0.1)).collect();
        let order = |t: Team| {
            let mut v: Vec<(usize, Catch)> = catches.iter().enumerate()
                .filter_map(|(k, c)| c.filter(|_| self.players[k].team == t).map(|c| (k, c))).collect();
            v.sort_by(|a, b| a.1.t.total_cmp(&b.1.t).then(a.0.cmp(&b.0)));
            v
        };
        let (mine, theirs) = (order(team), order(team.other()));
        let best_theirs = theirs.first().map_or(f32::INFINITY, |(_, c)| c.t);
        let favored = mine.first().is_some_and(|(_, c)| c.t <= best_theirs);
        match mine.iter().position(|(k, _)| *k == i) {
            Some(0) => {
                brain.job = Job::Fetch;
                brain.spot = mine[0].1.at;
                return;
            }
            Some(1) if !favored && !theirs.is_empty() => {
                // Hit the enemy who gets there first as the ball arrives.
                let (k, c) = theirs[0];
                brain.job = Job::Rush;
                brain.mark = Some(k);
                brain.spot = c.at;
                return;
            }
            Some(1) if mine[1].1.t < best_theirs + 0.5 => {
                brain.job = Job::Fetch;
                brain.spot = mine[1].1.at;
                return;
            }
            _ => {}
        }
        let skip: Vec<usize> = mine.iter().take(1).chain(theirs.iter().take(1)).map(|(k, _)| *k).collect();
        if favored {
            let (k, c) = mine[0];
            self.offense(i, k, c.at, &skip, brain);
        } else if let Some(&(k, c)) = theirs.first() {
            // The second to the ball already goes for whoever gets there.
            self.defense(i, k, c.at, &skip, Some(0), brain);
        } else {
            brain.spot = self.ball.pos;
        }
    }

    /// Our side has (or is about to have) the ball, `c` at `origin`: the
    /// teammates nearest the ball take on the enemies closing in, the rest
    /// get open up the field.
    fn offense(&self, i: usize, c: usize, origin: Vec3, skip: &[usize], brain: &mut Brain) {
        let team = self.players[i].team;
        let near = |a: &usize, b: &usize| self.players[*a].pos.distance(origin).total_cmp(&self.players[*b].pos.distance(origin)).then(a.cmp(b));
        let mut mates: Vec<usize> = (0..self.players.len())
            .filter(|&k| self.players[k].is_bot && self.players[k].alive && self.players[k].team == team && !skip.contains(&k))
            .collect();
        mates.sort_by(near);
        let mut threats: Vec<usize> = (0..self.players.len())
            .filter(|&k| self.fb_enemy(team, k) && self.players[k].stun <= 0.0 && self.players[k].pos.distance(origin) < THREAT_RANGE)
            .collect();
        threats.sort_by(near);
        let blockers = threats.len().min(2).min(mates.len().saturating_sub(1));
        if let Some(k) = mates.iter().take(blockers).position(|&k| k == i) {
            brain.job = Job::Block;
            brain.mark = Some(threats[k]);
            let ep = self.players[threats[k]].pos;
            brain.spot = origin + (ep - origin).normalize_or_zero() * ((ep - origin).length() * 0.45).min(6.0);
            return;
        }
        brain.job = Job::Receive;
        let mut receivers = mates[blockers.min(mates.len())..].to_vec();
        receivers.sort();
        let mut taken: Vec<Vec3> = Vec::new();
        for r in receivers {
            let spot = self.open_spot(r, c, origin, &taken);
            if r == i { brain.spot = spot; return; }
            taken.push(spot);
        }
        brain.spot = origin;
    }

    /// The best open spot for receiver `r` with the ball (`c`'s) at
    /// `origin`: up the field, away from enemies and out of their passing
    /// lanes, spread from the spots `taken` by teammates, not far to run.
    fn open_spot(&self, r: usize, c: usize, origin: Vec3, taken: &[Vec3]) -> Vec3 {
        let me = &self.players[r];
        let team = me.team;
        let goal = self.end_zone(team.other());
        let fwd = flat(goal - origin).normalize_or(Vec2::Y);
        let across = Vec2::new(-fwd.y, fwd.x);
        let to_goal = flat(goal - origin).length();
        let enemies: Vec<Vec2> = (0..self.players.len()).filter(|&k| k != c && self.fb_enemy(team, k) && self.players[k].stun <= 0.0)
            .map(|k| flat(self.players[k].pos)).collect();
        let previous = (me.bot_fb.job == Job::Receive).then_some(me.bot_fb.spot);
        let mut best = (f32::NEG_INFINITY, origin);
        for ahead in ROUTE_AHEAD {
            let ahead = ahead.min(to_goal + 4.0);
            for side in ROUTE_ACROSS {
                let at = flat(origin) + fwd * ahead + across * side;
                let spot = self.fb_clamp(Vec3::new(at.x, origin.y, at.y));
                let s = flat(spot);
                let gained = to_goal - flat(goal).distance(s);
                let open = enemies.iter().map(|e| e.distance(s)).fold(25.0f32, f32::min);
                // How near an enemy stands to the throw's line.
                let (a, d) = (flat(origin), s - flat(origin));
                let lane = enemies.iter().map(|e| {
                    let t = ((*e - a).dot(d) / d.length_squared().max(1e-3)).clamp(0.0, 1.0);
                    (a + d * t).distance(*e)
                }).fold(10.0f32, f32::min);
                let run = flat(me.pos).distance(s) / RUN;
                let crowd: f32 = taken.iter().map(|q| (14.0 - flat(*q).distance(s)).max(0.0)).sum::<f32>()
                    + (8.0 - flat(origin).distance(s)).max(0.0);
                let zone = if flat(goal).distance(s) < self.end_zone_radius() { 8.0 } else { 0.0 };
                let stay = if previous.is_some_and(|p| flat(p).distance(s) < 6.0) { 3.0 } else { 0.0 };
                let score = gained * 0.5 + open * 0.8 + lane - run * 2.0 - crowd * 1.5 + zone + stay;
                if score > best.0 { best = (score, spot); }
            }
        }
        best.1
    }

    /// The other side has (or is about to have) the ball, `c` at `origin`:
    /// the quickest to them rush (two, one in a small side, or `rush`), one
    /// stays deep, the rest mark receivers from between them and the ball.
    fn defense(&self, i: usize, c: usize, origin: Vec3, skip: &[usize], rush: Option<usize>, brain: &mut Brain) {
        let team = self.players[i].team;
        let own = self.end_zone(team);
        let mut ours: Vec<(usize, f32)> = (0..self.players.len())
            .filter(|&k| self.players[k].is_bot && self.players[k].alive && self.players[k].team == team && !skip.contains(&k))
            .map(|k| (k, self.fb_meet(k, c).0)).collect();
        ours.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        let rushers = rush.unwrap_or(if ours.len() >= 3 { 2 } else { 1 });
        if ours.iter().take(rushers).any(|(k, _)| *k == i) {
            brain.job = Job::Rush;
            brain.mark = Some(c);
            brain.spot = if self.ball.carried_by(c) { self.fb_meet(i, c).1 } else { origin };
            return;
        }
        let mut rest: Vec<usize> = ours.iter().skip(rushers).map(|(k, _)| *k).collect();
        // One deep when three or more are left: a defender by role first,
        // else whoever is nearest our end zone.
        if rest.len() >= 3 {
            let key = |k: usize| (self.players[k].bot_role != BotRole::Defense, self.players[k].pos.distance(own));
            let deep = rest.iter().copied().min_by(|&a, &b| {
                let (ka, kb) = (key(a), key(b));
                ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(a.cmp(&b))
            }).unwrap();
            if deep == i {
                brain.job = Job::Safety;
                let back = (origin - own).length();
                brain.spot = own + (origin - own).normalize_or_zero() * (back * 0.45).clamp(25.0, 50.0);
                return;
            }
            rest.retain(|&k| k != deep);
        }
        // The receivers nearest our end zone first, each to the nearest free
        // marker.
        let mut marks: Vec<usize> = (0..self.players.len()).filter(|&k| k != c && self.fb_enemy(team, k)).collect();
        marks.sort_by(|&a, &b| self.players[a].pos.distance(own).total_cmp(&self.players[b].pos.distance(own)).then(a.cmp(&b)));
        for m in marks {
            if rest.is_empty() { break; }
            let mp = self.players[m].pos;
            let at = (0..rest.len()).min_by(|&a, &b| self.players[rest[a]].pos.distance(mp)
                .total_cmp(&self.players[rest[b]].pos.distance(mp)).then(rest[a].cmp(&rest[b]))).unwrap();
            if rest.remove(at) == i {
                brain.job = Job::Cover;
                brain.mark = Some(m);
                brain.spot = self.cover_spot(m, origin, own);
                return;
            }
        }
        // More markers than receivers: a zone between the ball and home.
        brain.job = Job::Cover;
        brain.spot = origin.lerp(own, 0.35);
    }

    /// Where a marker stands: a few metres from their receiver toward the
    /// ball, and a little toward home.
    fn cover_spot(&self, m: usize, origin: Vec3, own: Vec3) -> Vec3 {
        let o = &self.players[m];
        let at = o.pos + Vec3::new(o.vel.x, 0.0, o.vel.z) * 0.3;
        at + (origin - at).normalize_or_zero() * 3.0 + (own - at).normalize_or_zero() * 2.0
    }

    /// The carrier's think, `held` seconds into its carry: pass when pressed
    /// or a teammate is open well up the field; else pick the way through.
    fn carry_think(&mut self, i: usize, held: f32) -> Vec3 {
        let (pos, team) = (self.players[i].pos, self.players[i].team);
        let goal = self.end_zone(team.other());
        let threat = self.carry_threat(i);
        let near_goal = flat(goal - pos).length() < 25.0;
        let pressed = threat < PRESSED;
        if ((pressed && held > 0.25) || threat < 0.2 || held > HOLD) && (!near_goal || threat < 0.4) {
            if let Some((yaw, pitch)) = self.pass_choice(i, pressed) {
                // The throw as this bot's hand makes it: a wobble that can
                // change who sets the power, so it's checked again.
                let err = self.players[i].bot_profile.aim_error * 0.5;
                let (yaw, pitch) = (yaw + (self.rng() - 0.5) * 2.0 * err, (pitch + (self.rng() - 0.5) * 2.0 * err).clamp(-football::LOOK_LIMIT, football::LOOK_LIMIT));
                if self.pass_safe(i, yaw, pitch).is_some() {
                    let p = &mut self.players[i];
                    (p.yaw, p.pitch, p.bot_pass) = (yaw, pitch, true);
                    return pos;
                }
            }
        }
        // A raised goal: for its run-up spot, where the leap takes over
        // (`bot_leap_inputs`).
        let target = if self.raised_goal(team.other()) { self.leap_spot(goal) } else { goal };
        if flat(target - pos).length() < 30.0 { return target; }
        let was = self.players[i].bot_fb;
        let keep = (was.job == Job::Carry).then(|| (was.spot - pos).normalize_or_zero());
        pos + self.carry_heading(i, target, keep) * 30.0
    }

    /// Seconds before the nearest enemy the carrier knows of could reach
    /// it, as it pictures them (`fb_seen`).
    fn carry_threat(&self, i: usize) -> f32 {
        self.fb_defenders(i).iter().filter(|e| e.stun <= 0.0)
            .map(|e| meet(e.pos, e.vel, e.stun, &self.players[i]).0).fold(f32::INFINITY, f32::min)
    }

    /// The carrier's way: toward `goal` (the end zone), turned up to 75 degrees
    /// either side for the line enemies it knows of take longest to cut off,
    /// in bounds, and keeping to the way it was going (`keep`) unless
    /// another is clearly better.
    fn carry_heading(&self, i: usize, goal: Vec3, keep: Option<Vec3>) -> Vec3 {
        let p = &self.players[i];
        let base = yaw_to(p.pos, goal);
        let speed = flat(p.vel).length().max(football::CARRIER_FORWARD);
        let enemies: Vec<(Vec2, f32)> = self.fb_defenders(i).iter().map(|e| (flat(e.pos), e.stun)).collect();
        let mut best = (f32::NEG_INFINITY, look_dir(base, 0.0));
        for step in -5i32..=5 {
            let turn = step as f32 * 15f32.to_radians();
            let dir = look_dir(base + turn, 0.0);
            // When the first enemy could get to where this line takes it.
            let mut caught = 2.5f32;
            'ahead: for k in 1..=10 {
                let t = k as f32 * 0.25;
                let at = p.pos + dir * speed * t;
                if !self.fb_inside(at) { caught = caught.min(t * 0.6); break; }
                for (e, stun) in &enemies {
                    if e.distance(flat(at)) <= RUN * (t - stun).max(0.0) + 1.5 { caught = caught.min(t); break 'ahead; }
                }
            }
            let stay = keep.map_or(0.0, |k| if k.dot(dir) > 0.97 { 0.35 } else { 0.0 });
            let score = turn.cos() + caught * 1.4 + stay;
            if score > best.0 { best = (score, dir); }
        }
        best.1
    }

    /// The carrier's best pass, if a teammate takes it well before any enemy
    /// can: the look (yaw, pitch). Under `pressure` any safe pass does;
    /// otherwise only one that gains ground.
    fn pass_choice(&self, i: usize, pressure: bool) -> Option<(f32, f32)> {
        let (pos, team) = (self.players[i].pos, self.players[i].team);
        let goal = self.end_zone(team.other());
        let here = flat(goal - pos).length();
        let eye = pos + Vec3::Y * EYE;
        let bodies: Vec<Option<Body>> = (0..self.players.len()).map(|k| (k != i && self.fb_up(k)).then(|| self.fb_body(k))).collect();
        let mut best: Option<(f32, f32, f32)> = None;
        for j in 0..self.players.len() {
            if self.players[j].team != team || bodies[j].is_none() { continue; }
            let o = &self.players[j];
            for lead in PASS_LEADS {
                let yaw = yaw_to(eye, o.pos + Vec3::new(o.vel.x, 0.0, o.vel.z) * lead);
                for pitch in PASS_PITCHES {
                    let Some((mate, margin, _)) = self.throw_outcome(i, yaw, pitch, &bodies) else { continue };
                    if margin < PASS_MARGIN { continue; }
                    let gained = here - flat(goal - mate.at).length();
                    let value = gained + margin.min(2.0) * 6.0 - if mate.ground { 15.0 } else { 0.0 };
                    if best.is_none_or(|b| value > b.0) { best = Some((value, yaw, pitch)); }
                }
            }
        }
        let (value, yaw, pitch) = best?;
        (if pressure { value > -3.0 } else { value > GOOD_PASS }).then_some((yaw, pitch))
    }

    /// A throw by carrier `i` looking (yaw, pitch), if a teammate takes it
    /// in bounds: their catch and how much sooner (s) than any enemy.
    fn pass_safe(&self, i: usize, yaw: f32, pitch: f32) -> Option<(Catch, f32)> {
        let bodies: Vec<Option<Body>> = (0..self.players.len()).map(|k| (k != i && self.fb_up(k)).then(|| self.fb_body(k))).collect();
        self.throw_outcome(i, yaw, pitch, &bodies).filter(|(_, margin, _)| *margin >= PASS_MARGIN * 0.5).map(|(c, m, _)| (c, m))
    }

    /// Who takes carrier `i`'s throw looking (yaw, pitch) first: the
    /// teammate's catch (in bounds), how much sooner than any enemy, and the
    /// throw's velocity.
    fn throw_outcome(&self, i: usize, yaw: f32, pitch: f32, bodies: &[Option<Body>]) -> Option<(Catch, f32, Vec3)> {
        let team = self.players[i].team;
        let (_, from, vel) = self.pass_throw(i, yaw, pitch);
        let f = Flight { from, vel };
        let side = |want: bool, reaction: f32, until: f32| bodies.iter().enumerate()
            .filter(|(k, _)| (self.players[*k].team == team) == want)
            .filter_map(|(_, b)| b.as_ref().and_then(|b| b.take(&f, reaction, until)))
            .min_by(|a, b| a.t.total_cmp(&b.t));
        let mate = side(true, 0.15, 4.0)?;
        if !self.fb_inside(mate.at) || mate.rise > PASS_RISE { return None; }
        let enemy = side(false, 0.05, mate.t + 2.0).map_or(f32::INFINITY, |c| c.t);
        Some((mate, enemy - mate.t, vel))
    }

    /// Bot `i`'s inputs this step in Football (None: its route leads).
    pub(super) fn football_inputs(&mut self, i: usize, dt: f32) -> Option<Inputs> {
        let p = &self.players[i];
        if !p.is_bot || !self.football() || !p.alive || p.stun > 0.0 { return None; }
        let (brain, pass, pouncing, aloft) = (p.bot_fb, p.bot_pass, p.bot_pounce > 0.0, !p.on_ground);
        if pass && self.ball.carried_by(i) { return Some((0.0, 0.0, false, false, true, false)); }
        if brain.leap > 0.0 { return Some(self.catch_flight(i, dt)); }
        if brain.ride {
            if !aloft {
                self.players[i].bot_fb.ride = false;
            } else if flat(p.vel).normalize_or_zero().dot(flat(brain.spot - p.pos).normalize_or_zero()) < 0.5 {
                // Its way lies more than 60 degrees off the push: the jet
                // would only take speed off it, so ride it out.
                return Some(STILL);
            }
            // Going its way: its run's jet adds to the push.
        }
        if brain.blocked && brain.job != Job::Fetch { self.players[i].bot_pounce = 0.0; return None; }
        if pouncing {
            if let Some(inputs) = brain.mark.and_then(|m| self.pounce_on(i, m, dt)) { return Some(inputs); }
        }
        let face = self.ball.pos;
        Some(match brain.job {
            Job::Idle => self.fb_drive(i, brain.spot, false, 60.0, None),
            Job::Carry => self.carry_inputs(i, brain),
            Job::Fetch => self.fetch_inputs(i, brain, dt),
            Job::Rush => match brain.mark.filter(|&m| self.fb_up(m)) {
                None => self.fb_drive(i, brain.spot, true, RUSH_RESERVE, None),
                Some(m) => match self.pounce_on(i, m, dt) {
                    Some(inputs) => inputs,
                    // On an intercept course with a carrier, else to their catch.
                    None => {
                        let to = if self.ball.carried_by(m) { self.fb_meet(i, m).1 } else { brain.spot };
                        self.fb_drive(i, to, true, RUSH_RESERVE, None)
                    }
                },
            },
            Job::Block => match brain.mark.filter(|&m| self.fb_up(m)) {
                None => self.fb_drive(i, brain.spot, true, RUSH_RESERVE, None),
                Some(m) => {
                    let (me, them) = (self.players[i].pos, self.players[m].pos);
                    // Between them and the ball, or close: take them on.
                    if me.distance(them) < 5.0 || (me.distance(them) < 14.0 && me.distance(face) < them.distance(face)) {
                        match self.pounce_on(i, m, dt) {
                            Some(inputs) => inputs,
                            None => {
                                let o = &self.players[m];
                                let at = o.pos + Vec3::new(o.vel.x, 0.0, o.vel.z) * 0.25;
                                self.players[i].yaw = yaw_to(me, at);
                                (0.0, 1.0, false, false, false, false)
                            }
                        }
                    } else {
                        self.fb_drive(i, brain.spot, true, RUSH_RESERVE, Some(them))
                    }
                }
            },
            Job::Receive => self.fb_drive(i, brain.spot, true, CATCH_RESERVE, Some(face)),
            Job::Cover => {
                let own = self.end_zone(self.players[i].team);
                let to = match brain.mark.filter(|&m| self.players[m].alive) {
                    Some(m) => self.fb_clamp(self.cover_spot(m, face, own)),
                    None => brain.spot,
                };
                self.fb_drive(i, to, true, CATCH_RESERVE, Some(face))
            }
            Job::Safety => self.fb_drive(i, brain.spot, true, CATCH_RESERVE, Some(face)),
        })
    }

    /// Head for `to`: hop with the jet from afar (keeping `reserve` energy),
    /// run nearer, brake into it, and stand facing `face` once there.
    fn fb_drive(&mut self, i: usize, to: Vec3, travel: bool, reserve: f32, face: Option<Vec3>) -> Inputs {
        let p = &mut self.players[i];
        let d = flat(to - p.pos);
        let dist = d.length();
        if dist < 1.2 {
            if let Some(f) = face { p.yaw = yaw_to(p.pos, f); }
            return STILL;
        }
        p.yaw = yaw_to(p.pos, to);
        // The ground pulls toward the run at 40 m/s²: coast into the spot.
        let closing = flat(p.vel).dot(d / dist);
        if dist < HOP_FROM && closing > 0.0 && dist < closing * closing / 72.0 + 0.6 { return STILL; }
        let hop = travel && dist > HOP_FROM;
        let jet = hop && !p.on_ground && p.energy > reserve + 5.0;
        (0.0, 1.0, hop, false, false, jet)
    }

    /// The carrier's run: its chosen way, sprinting (hopping with the jet,
    /// a burst's worth kept back) while nobody it knows of is ahead within
    /// 25 m, else on foot; and a burst into a hit it sees coming, so it's the
    /// faster body when they meet.
    fn carry_inputs(&mut self, i: usize, brain: Brain) -> Inputs {
        let (pos, vel) = (self.players[i].pos, self.players[i].vel);
        let dir = flat(brain.spot - pos).normalize_or_zero();
        let enemies: Vec<Seen> = self.fb_defenders(i).into_iter().filter(|e| e.stun <= 0.0).collect();
        // A hit coming from ahead; or, just after the pickup, from anywhere
        // (a scramble for a loose ball): break out of it.
        let fresh = self.time - brain.since < 0.6;
        let coming = enemies.iter().any(|o| {
            let rel = flat(o.pos - pos);
            let closing = -flat(o.vel - vel).dot(rel.normalize_or_zero());
            rel.length() < 5.0 && closing > if fresh { 2.0 } else { 6.0 } && (fresh || rel.normalize_or_zero().dot(dir) > 0.2)
        });
        let p = &self.players[i];
        if coming && p.energy > 15.0 {
            // A burst: jump, then jet along the run.
            return (0.0, 1.0, true, false, false, !p.on_ground || p.coyote <= 0.0);
        }
        let clear = !enemies.iter().any(|o| {
            let rel = flat(o.pos - pos);
            rel.length() < 25.0 && rel.normalize_or_zero().dot(dir) > 0.5
        });
        self.fb_drive(i, brain.spot, clear, CARRY_RESERVE, None)
    }

    /// After a loose ball: to where it's first in reach. Once its flight
    /// passes over the bot, it stands and lets it come, or jumps (with the
    /// jet for more than a hop) when the climb takes as long as the ball
    /// has left; with no air control the jump goes straight up from a
    /// standstill.
    fn fetch_inputs(&mut self, i: usize, brain: Brain, dt: f32) -> Inputs {
        let Some(f) = self.fb_flight() else {
            let b = &self.ball;
            let at = b.pos + Vec3::new(b.vel.x, 0.0, b.vel.z) * 0.2;
            return self.fb_drive(i, at, true, 0.0, None);
        };
        let body = self.fb_body(i);
        if let Some((tc, rise)) = self.overhead(&body, &f) {
            let p = &mut self.players[i];
            p.yaw = yaw_to(body.pos, f.from);
            if rise <= 0.0 { return STILL; }
            if p.on_ground && rise_time(rise + 0.1, body.energy).is_some_and(|need| tc <= need + dt) {
                p.bot_fb.leap = dt;
                return (0.0, 0.0, true, true, false, rise > HOP_RISE);
            }
            // Settle under it until then: a running jump would carry past.
            return STILL;
        }
        let Some(c) = body.take(&f, 0.0, f32::INFINITY) else { return self.fb_drive(i, brain.spot, true, 0.0, None) };
        let late = body.reach(c.at) > c.t - 0.3;
        self.fb_drive(i, c.at, c.ground || late, if c.ground || late { 0.0 } else { CATCH_RESERVE }, Some(f.from))
    }

    /// Whether the ball's flight comes over `body` where it stands: when it
    /// passes nearest (s) and how far over the standing reach it is then
    /// (none or less: it comes to the body).
    fn overhead(&self, body: &Body, f: &Flight) -> Option<(f32, f32)> {
        let (mut best, mut t) = ((f32::INFINITY, 0.0f32), 0.0);
        while t < 1.5 {
            let d = flat(f.at(t)).distance(flat(body.pos));
            if d < best.0 { best = (d, t); }
            t += 0.02;
        }
        let (across, tc) = best;
        if across > REACH_ACROSS + 0.1 || tc <= 0.0 { return None; }
        let h = f.at(tc).y - body.feet;
        (h >= REACH_LOW && h - REACH_HIGH <= body.top).then_some((tc, h - REACH_HIGH))
    }

    /// In a jump for a catch: keys let go (the jet lifts straight up), the
    /// jet held while the body would pass under the ball, until the ball is
    /// past, taken or down.
    fn catch_flight(&mut self, i: usize, dt: f32) -> Inputs {
        let flight = self.fb_flight();
        let p = &self.players[i];
        let (pos, vel, leap, on_ground) = (p.pos, p.vel, p.bot_fb.leap, p.on_ground);
        let Some(f) = flight.filter(|_| leap <= 2.0 && !(leap > 0.25 && on_ground)) else {
            self.players[i].bot_fb.leap = 0.0;
            return STILL;
        };
        // When the ball passes nearest, at the body's present drift.
        let (mut best, mut t) = ((f32::INFINITY, 0.0f32), 0.0);
        while t < 1.2 {
            let d = flat(f.at(t)).distance(flat(pos + vel * t));
            if d < best.0 { best = (d, t); }
            t += 0.02;
        }
        let (across, tc) = best;
        let g = crate::defs::movement().gravity;
        let feet = pos.y - PLAYER_RADIUS + vel.y * tc - 0.5 * g * tc * tc;
        let jet = across < 1.2 && tc > 0.0 && feet + REACH_HIGH < f.at(tc).y;
        self.players[i].bot_fb.leap = leap + dt;
        (0.0, 0.0, false, false, false, jet)
    }

    /// A charge at enemy `m`: from 3.5 m to 5-8 m (by its aggression) on
    /// the ground with jet for it,
    /// a jump, then the jet along the keys, turned each step toward where
    /// they'll be. The jet's push makes the charger the faster body by far,
    /// so the hit takes them down (15 m/s closing) even from behind. Ends on
    /// landing, an empty tank, or after 1.2 s.
    fn pounce_on(&mut self, i: usize, m: usize, dt: f32) -> Option<Inputs> {
        let team = self.players[i].team;
        if !self.fb_up(m) || self.players[m].team == team { self.players[i].bot_pounce = 0.0; return None; }
        let (p, o) = (&self.players[i], &self.players[m]);
        let rel = flat(o.pos - p.pos);
        let d = rel.length();
        let closing = -flat(o.vel - p.vel).dot(rel / d.max(0.1));
        let t = p.bot_pounce;
        if t <= 0.0 {
            // From the ground, in range, with a tank for it.
            let reach = 5.0 + 3.0 * p.bot_profile.aggression;
            if !(p.on_ground && (3.5..reach).contains(&d) && p.energy >= 35.0 && (o.pos.y - p.pos.y).abs() < 3.0) { return None; }
        } else if t > 1.2 || (t > 0.3 && p.on_ground) || p.energy < 1.0 {
            self.players[i].bot_pounce = 0.0;
            return None;
        }
        let eta = d / closing.max(12.0);
        let lead = o.pos + Vec3::new(o.vel.x, 0.0, o.vel.z) * eta;
        let p = &mut self.players[i];
        p.bot_pounce = t + dt;
        p.yaw = yaw_to(p.pos, lead);
        Some((0.0, 1.0, t <= 0.0, t <= 0.0, false, t > 0.0))
    }

    /// A bot's name in the mode in force: in Football, one of
    /// FOOTBALL_NAMES nobody else has; otherwise its personality's
    /// ("Rider", "Ace 2").
    pub(super) fn bot_name(&mut self, i: usize) -> String {
        // Sparring in the duel lobby's tower: names from two galaxies.
        if self.sparring() {
            let start = (self.rng() * super::blade::SPARRING_NAMES.len() as f32) as usize;
            let taken = |n: &str| self.players.iter().enumerate().any(|(j, o)| j != i && o.name == n);
            let names = &super::blade::SPARRING_NAMES;
            if let Some(name) = (0..names.len()).map(|k| names[(start + k) % names.len()]).find(|n| !taken(n)) {
                return name.into();
            }
        }
        if self.football() {
            let start = (self.rng() * FOOTBALL_NAMES.len() as f32) as usize;
            let taken = |n: &str| self.players.iter().enumerate().any(|(j, o)| j != i && o.name == n);
            if let Some(name) = (0..FOOTBALL_NAMES.len()).map(|k| FOOTBALL_NAMES[(start + k) % FOOTBALL_NAMES.len()]).find(|n| !taken(n)) {
                return name.into();
            }
        }
        let profile = self.players[i].bot_profile;
        let same = self.players.iter().enumerate()
            .filter(|(j, o)| *j < i && o.is_bot && o.bot_profile.name == profile.name).count();
        if same == 0 { profile.name.into() } else { format!("{} {}", profile.name, same + 1) }
    }
}

#[cfg(test)]
pub(crate) mod report {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// What happened in a bots-only Football match, read from its plays.
    #[derive(Debug, Default)]
    pub(crate) struct Tally {
        pub passes: u32,
        pub completions: u32,
        pub interceptions: u32,
        pub pickups: u32,
        pub knocked_loose: u32,
        pub tackles: u32,
        pub carrier_tackles: u32,
        pub air_catches: u32,
        pub touchdowns: u32,
        pub score: [u32; 2],
        pub resets: u32,
        /// Tackles by the winner's and loser's jobs.
        pub hits: std::collections::BTreeMap<String, u32>,
        /// Possessions and their seconds in all.
        pub possessions: u32,
        pub held: f32,
        pub seconds: f32,
        pub step_ms: f32,
        pub worst_ms: f32,
    }

    /// A server's bots-only Football match on `map` with `bots` bots, for
    /// `minutes` of play after the warmup.
    pub(crate) fn play(map: MapId, bots: usize, minutes: f32) -> Tally {
        // The route graph from the first step, as on a server that prepared
        // it: built in the background, it would land whenever, or already be
        // there from another test, and the play would differ run to run.
        let _ = crate::bot_nav::graph(map);
        let mut game = Match::new(map);
        game.bots = bots;
        game.rotate_to_mode(map, SupportedMode::Football);
        let none = vec![None; MAX_PLAYERS];
        let mut t = Tally::default();
        while game.phase != Phase::Playing { game.step(&none); }
        let steps = (minutes * 60.0 / STEP) as usize;
        let started = std::time::Instant::now();
        let mut ran = 0;
        let mut holder = String::new();
        for _ in 0..steps {
            if game.phase != Phase::Playing { break; }
            let tick = std::time::Instant::now();
            game.step(&none);
            t.worst_ms = t.worst_ms.max(tick.elapsed().as_secs_f32() * 1000.0);
            ran += 1;
            let w = &mut game.world;
            // Off the ground: the feet well above the floor under them.
            let up = |w: &World, c: usize| w.players[c].pos.y - crate::terrain::support_on(w.map, w.players[c].pos).0 > PLAYER_RADIUS + 0.5;
            let plays: Vec<_> = w.feed.drain(..).collect();
            for e in plays {
                let crate::feed::Entry::Play { text } = e else { continue };
                // Offline the unnamed player's lines say "You pass", "You catch".
                if text.contains(" passes toward ") || text.contains(" pass toward ") || text.ends_with(" throws the ball") || text.ends_with(" throw the ball") { t.passes += 1; }
                if text.ends_with(" catches the ball") || text.ends_with(" catch the ball") {
                    t.completions += 1;
                    if w.ball.carrier.is_some_and(|c| up(w, c)) { t.air_catches += 1; }
                }
                if text.ends_with(" intercepts the ball!") || text.ends_with(" intercept the ball!") {
                    t.interceptions += 1;
                    if w.ball.carrier.is_some_and(|c| up(w, c)) { t.air_catches += 1; }
                }
                if text.ends_with(" picks up the ball") || text.ends_with(" pick up the ball") { t.pickups += 1; }
                if text.contains(" knocks the ball loose from ") || text.contains(" knock the ball loose from ") { t.knocked_loose += 1; }
                let tackled = text.contains(" tackles ") || text.starts_with("You tackle ");
                if let Some((a, b)) = text.split_once(" tackles ").or_else(|| text.split_once(" tackle "))
                    .or_else(|| text.split_once(" knocks the ball loose from ")).or_else(|| text.split_once(" knock the ball loose from ")) {
                    let job = |n: &str| w.players.iter().find(|p| p.name == n).map_or("?".into(), |p| format!("{:?}", p.bot_fb.job));
                    let key = format!("{} {} {}", job(a), if tackled { "tackles" } else { "knocks" }, if b == holder { "Carrier".into() } else { job(b) });
                    *t.hits.entry(key).or_default() += 1;
                }
                if tackled {
                    t.tackles += 1;
                    if !holder.is_empty() && (text.ends_with(&format!(" tackles {holder}")) || text.ends_with(&format!(" tackle {holder}"))) { t.carrier_tackles += 1; }
                }
                if text.contains(" was reset") { t.resets += 1; }
            }
            let now = w.ball.carrier.map_or(String::new(), |c| w.players[c].name.clone());
            if !now.is_empty() { t.held += STEP; if now != holder { t.possessions += 1; } }
            holder = now;
        }
        t.seconds = ran as f32 * STEP;
        t.step_ms = started.elapsed().as_secs_f32() * 1000.0 / ran.max(1) as f32;
        t.score = game.world.score;
        t.touchdowns = t.score[0] + t.score[1];
        t
    }

    /// Each pass: who the bots expected to take it and when, and what
    /// happened. `PEAKRUNNER_FB_MAP`, `PEAKRUNNER_FB_PASSES` (20).
    #[test]
    #[ignore]
    fn football_pass_trace() {
        let key = std::env::var("PEAKRUNNER_FB_MAP").unwrap_or_else(|_| "longfield".into());
        let map = crate::catalog::find(&key).expect("a map key");
        let want = std::env::var("PEAKRUNNER_FB_PASSES").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
        let mut game = Match::new(map);
        game.bots = 16;
        game.rotate_to_mode(map, SupportedMode::Football);
        let none = vec![None; MAX_PLAYERS];
        while game.phase != Phase::Playing { game.step(&none); }
        let (mut seen, mut open): (usize, Option<(f32, String, String)>) = (0, None);
        for _ in 0..(20.0 * 60.0 / STEP) as usize {
            game.step(&none);
            let w = &mut game.world;
            let plays: Vec<_> = w.feed.drain(..).collect();
            for e in plays {
                let crate::feed::Entry::Play { text } = e else { continue };
                if text.contains(" passes toward ") || text.ends_with(" throws the ball") {
                    let Some(thrower) = w.ball.thrown_by else { continue };
                    println!("  release yaw {:.3} pitch {:.2}", w.players[thrower].yaw, w.players[thrower].pitch);
                    let team = w.players[thrower].team;
                    let all: Vec<(usize, Catch)> = (0..w.players.len()).filter_map(|k| w.fb_catch(k, 0.0).map(|c| (k, c))).collect();
                    let best = |mine: bool| all.iter().filter(|(k, _)| (w.players[*k].team == team) == mine).min_by(|a, b| a.1.t.total_cmp(&b.1.t))
                        .map_or("none".to_string(), |(k, c)| format!("{} {:.2}s{}{}", w.players[*k].name, c.t, if c.ground { " ground" } else { "" },
                            if c.rise > 0.0 { format!(" rise {:.1}", c.rise) } else { String::new() }));
                    open = Some((w.time, format!("{text} | v {:.1} | mate {} | enemy {}", w.ball.vel.length(), best(true), best(false)), w.players[thrower].name.clone()));
                } else if let Some((t0, plan, _)) = &open {
                    if [" catches the ball", " catch the ball", " intercepts the ball!", " intercept the ball!", " picks up the ball", " pick up the ball"].iter().any(|s| text.ends_with(s)) || text.contains("reset") {
                        let c = w.ball.carrier.map(|c| (w.players[c].pos.y - crate::terrain::support_on(w.map, w.players[c].pos).0 - PLAYER_RADIUS, w.players[c].bot_fb.job));
                        println!("{:6.1} {plan}\n       -> {:.2}s {text} {c:?}", t0, w.time - t0);
                        open = None;
                        seen += 1;
                    }
                }
            }
            if seen >= want { break; }
        }
    }

    /// Every possession for a while: who, how long, how far toward the end
    /// zone, and the play that ended it. `PEAKRUNNER_FB_MAP`.
    #[test]
    #[ignore]
    fn football_possession_trace() {
        let key = std::env::var("PEAKRUNNER_FB_MAP").unwrap_or_else(|_| "longfield".into());
        let map = crate::catalog::find(&key).expect("a map key");
        let mut game = Match::new(map);
        game.bots = 16;
        game.rotate_to_mode(map, SupportedMode::Football);
        let none = vec![None; MAX_PLAYERS];
        while game.phase != Phase::Playing { game.step(&none); }
        let mut open: Option<(usize, f32, f32)> = None;
        let mut last = String::new();
        for _ in 0..(90.0 / STEP) as usize {
            game.step(&none);
            let w = &mut game.world;
            for e in w.feed.drain(..) { if let crate::feed::Entry::Play { text } = e { last = text; } }
            let now = w.ball.carrier;
            let to_goal = |w: &World, c: usize| flat(w.end_zone(w.players[c].team.other()) - w.players[c].pos).length();
            match (open, now) {
                (Some((c, _, _)), Some(n)) if c == n => {}
                (Some((c, t0, d0)), _) => {
                    let d = if w.players[c].alive { to_goal(w, c) } else { d0 };
                    println!("{:6.1} {:16} {:4.1}s {:+6.1} m  left {:5.1} speed {:4.1} -> {last}", t0, w.players[c].name, w.time - t0, d0 - d, d, flat(w.players[c].vel).length());
                    open = now.map(|n| (n, w.time, to_goal(w, n)));
                }
                (None, Some(n)) => open = Some((n, w.time, to_goal(w, n))),
                (None, None) => {}
            }
        }
    }

    /// `PEAKRUNNER_FB_MAP` (longfield), `PEAKRUNNER_FB_BOTS` (16),
    /// `PEAKRUNNER_FB_MINUTES` (5):
    /// `cargo test --release -p peakrunner-core --lib football_bot_report -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn football_bot_report() {
        let key = std::env::var("PEAKRUNNER_FB_MAP").unwrap_or_else(|_| "longfield".into());
        let map = crate::catalog::find(&key).expect("a map key");
        let bots = std::env::var("PEAKRUNNER_FB_BOTS").ok().and_then(|v| v.parse().ok()).unwrap_or(16);
        let minutes = std::env::var("PEAKRUNNER_FB_MINUTES").ok().and_then(|v| v.parse().ok()).unwrap_or(5.0);
        let t = play(map, bots, minutes);
        println!("{key}, {bots} bots, {minutes} min: {t:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// Longfield with the local player out of the way and one bot of their
    /// team standing at `at`.
    fn field_with_bot(at: Vec3) -> World {
        let mut w = World::new();
        w.set_map(MapId::Longfield);
        w.set_mode(SupportedMode::Football);
        w.start_match(true);
        let bot = (1..w.players.len()).find(|&k| w.players[k].is_bot && w.players[k].team == w.players[0].team).unwrap();
        w.players.swap(1, bot);
        w.players.truncate(2);
        w.players[0].pos = Vec3::new(1100.0, 100.0 + PLAYER_RADIUS, 1200.0);
        w.players[1].pos = at;
        w.players[1].vel = Vec3::ZERO;
        w.players[1].on_ground = true;
        w.countdown = 0.0;
        w
    }

    /// A ball thrown by the local player from `back` metres behind the bot
    /// to pass `over` metres above its feet `flight` seconds later.
    fn lob(w: &mut World, back: f32, over: f32, flight: f32) {
        let bot = w.players[1].pos;
        let from = bot + Vec3::new(0.0, 1.0, back);
        let feet = bot.y - PLAYER_RADIUS;
        let g = crate::defs::movement().gravity;
        let vel = Vec3::new(0.0, (feet + over - from.y + 0.5 * g * flight * flight) / flight, -back / flight);
        w.ball = football::Ball { active: true, in_play: true, pos: from, vel, thrown_by: Some(0), last_team: Some(w.players[0].team), ..football::Ball::default() };
    }

    /// Eight a side for four minutes on Longfield: passes caught (some in
    /// the air), interceptions, carriers tackled or knocked loose, blockers
    /// taking defenders down, and touchdowns.
    #[test]
    fn bots_play_football_eight_a_side() {
        let t = report::play(MapId::Longfield, 16, 4.0);
        println!("{t:?}");
        let blocks: u32 = t.hits.iter().filter(|(k, _)| k.starts_with("Block tackles")).map(|(_, n)| n).sum();
        let carriers: u32 = t.hits.iter().filter(|(k, _)| k.ends_with("Carrier")).map(|(_, n)| n).sum();
        assert!(t.completions >= 15 && t.completions * 3 >= t.passes, "passes caught: {} of {}", t.completions, t.passes);
        assert!(t.air_catches >= 3, "catches in the air: {}", t.air_catches);
        assert!(t.interceptions >= 2, "interceptions: {}", t.interceptions);
        assert!(carriers >= 5 && t.carrier_tackles >= 1, "carriers stopped: {carriers}, tackled {}", t.carrier_tackles);
        assert!(blocks >= 1, "blocks: {:?}", t.hits);
        assert!(t.touchdowns >= 1, "touchdowns: {:?}", t.score);
    }

    #[test]
    fn football_bots_go_by_football_names_and_back() {
        let mut game = Match::new(MapId::Longfield);
        game.bots = 16;
        game.rotate_to_mode(MapId::Longfield, SupportedMode::Football);
        game.step(&vec![None; MAX_PLAYERS]);
        let names = |g: &Match| g.world.players.iter().filter(|p| p.is_bot && p.net_id != 0).map(|p| p.name.clone()).collect::<Vec<_>>();
        let football = names(&game);
        assert_eq!(football.len(), 16);
        assert!(football.iter().all(|n| FOOTBALL_NAMES.contains(&n.as_str()) && crate::names::validate(n) == Some(n.as_str())), "{football:?}");
        let unique: std::collections::BTreeSet<_> = football.iter().collect();
        assert_eq!(unique.len(), 16, "no two alike: {football:?}");
        // Other modes keep their personalities' names.
        game.rotate_to_mode(MapId::Cairnhold, SupportedMode::Ctf);
        assert!(names(&game).iter().all(|n| !FOOTBALL_NAMES.contains(&n.as_str())), "{:?}", names(&game));
        game.rotate_to_mode(MapId::Highgoal, SupportedMode::Football);
        assert!(names(&game).iter().all(|n| FOOTBALL_NAMES.contains(&n.as_str())), "{:?}", names(&game));
    }

    /// A bot carrier facing up the field (-z) sees a defender a quarter
    /// second late and only in front of it; one who went behind it, it
    /// pictures at the speed it last saw them for MEMORY seconds, then
    /// forgets.
    #[test]
    fn a_carrier_sees_defenders_late_in_front_and_remembers_them_behind() {
        let at = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1000.0);
        let mut w = field_with_bot(at);
        let (e, bot) = (0, 1);
        w.players[e].team = w.players[bot].team.other();
        w.players[bot].yaw = 0.0;
        let watch = |w: &mut World, n: usize| for _ in 0..n {
            let v = w.players[e].vel;
            w.players[e].pos += v * STEP;
            w.time += STEP;
            w.fb_watch();
        };
        // Behind it from the start: never seen.
        (w.players[e].pos, w.players[e].vel) = (at + Vec3::Z * 10.0, Vec3::ZERO);
        watch(&mut w, 30);
        assert!(w.fb_seen(bot, e).is_none() && w.carry_threat(bot).is_infinite(), "unseen behind");
        // Steps in front, running at it: noticed REACTION_STEPS later, where
        // it was then.
        (w.players[e].pos, w.players[e].vel) = (at - Vec3::Z * 12.0, Vec3::Z * 8.0);
        watch(&mut w, REACTION_STEPS);
        assert!(w.fb_seen(bot, e).is_none(), "not yet");
        let first = at.z - 12.0 + 8.0 * STEP;
        watch(&mut w, 1);
        let seen = w.fb_seen(bot, e).expect("seen a quarter second late");
        assert!((seen.pos.z - first).abs() < 1e-3, "where it was then: {} vs {first}", seen.pos.z);
        assert!(w.players[e].pos.z - seen.pos.z > 1.9, "while it's 2 m closer now");
        assert!(w.carry_threat(bot).is_finite());
        // Goes behind it and turns at it fast: pictured at the old speed.
        watch(&mut w, 30);
        let last = w.fb_seen(bot, e).unwrap().pos.z;
        (w.players[e].pos, w.players[e].vel) = (at + Vec3::Z * 30.0, Vec3::Z * -20.0);
        watch(&mut w, REACTION_STEPS + 30);
        let pictured = w.fb_seen(bot, e).expect("remembered");
        assert_eq!(pictured.vel, Vec3::Z * 8.0, "the speed it last saw");
        // It saw the last REACTION_STEPS in front before the turn, then 30 more.
        let on = last + 8.0 * (REACTION_STEPS + 30) as f32 * STEP;
        assert!((pictured.pos.z - on).abs() < 0.02, "on at that speed: {} vs {on}", pictured.pos.z);
        // And forgotten after MEMORY.
        w.players[e].vel = Vec3::ZERO;
        watch(&mut w, (MEMORY / STEP) as usize);
        assert!(w.fb_seen(bot, e).is_none(), "forgotten");
    }

    #[test]
    fn a_bot_that_catches_in_the_air_rides_the_push_to_the_landing() {
        // A fast pass caught at the top of a jump: the catch's push along the
        // throw stays with the bot, jet off, until it lands.
        let mut w = field_with_bot(Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1000.0));
        lob(&mut w, 60.0, 3.2, 1.2);
        let mut push = None;
        for _ in 0..(4.0 / STEP) as usize {
            w.physics_step();
            let p = &w.players[1];
            match push {
                None if w.ball.carried_by(1) => {
                    assert!(!p.on_ground, "caught in the air");
                    assert!(p.vel.z < -7.0, "pushed along the throw: {}", p.vel);
                    push = Some(p.vel.z);
                }
                Some(z) if !p.on_ground => {
                    assert!(!p.jetting && (p.vel.z - z).abs() < 1e-4, "rides it out: {} vs {z}", p.vel);
                }
                Some(_) => return,
                None => {}
            }
        }
        panic!("caught {:?} and landed", push);
    }

    #[test]
    fn a_bot_that_catches_a_pass_its_way_in_the_air_jets_on_with_the_push() {
        // A pass thrown toward the end zone the bot attacks, caught at the
        // top of its jump: the push carries it on and its run's jet adds to
        // it, so it never loses speed along the throw before it lands.
        let mut w = field_with_bot(Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1020.0));
        let bot = w.players[1].pos;
        let goal = w.end_zone(w.players[1].team.other());
        let dir = Vec3::new(goal.x - bot.x, 0.0, goal.z - bot.z).normalize();
        let (from, flight, g) = (bot - dir * 40.0 + Vec3::Y, 0.85, crate::defs::movement().gravity);
        let vel = dir * (40.0 / flight) + Vec3::Y * ((bot.y - PLAYER_RADIUS + 3.2 - from.y + 0.5 * g * flight * flight) / flight);
        w.ball = football::Ball { active: true, in_play: true, pos: from, vel, thrown_by: Some(0), facing: (-dir.x).atan2(-dir.z),
            last_team: Some(w.players[1].team), ..football::Ball::default() };
        let (mut push, mut fastest) = (None, 0.0f32);
        for _ in 0..(4.0 / STEP) as usize {
            w.physics_step();
            let p = &w.players[1];
            let along = p.vel.dot(dir);
            match push {
                None if w.ball.carried_by(1) => {
                    assert!(!p.on_ground && along > 7.0, "caught in the air and pushed: {}", p.vel);
                    push = Some(along);
                }
                Some(z) if !p.on_ground => {
                    assert!(along >= z - 1e-3, "never slower along the throw: {along} vs {z}");
                    fastest = fastest.max(along);
                }
                Some(z) => { assert!(fastest > z + 2.0, "jetted on: {fastest} vs {z}"); return; }
                None => {}
            }
        }
        panic!("caught {push:?} and landed");
    }

    #[test]
    fn a_bot_jumps_and_jets_to_catch_a_ball_over_its_head() {
        for over in [1.0f32, 3.2, 4.5, 7.0] {
            let mut w = field_with_bot(Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1000.0));
            lob(&mut w, 30.0, over, 1.4);
            let mut top = 0.0f32;
            for _ in 0..(3.0 / STEP) as usize {
                w.physics_step();
                top = top.max(w.players[1].pos.y - 100.0 - PLAYER_RADIUS);
                if w.ball.carrier.is_some() { break; }
            }
            println!("over {over}: carrier {:?}, rose {top:.2} m", w.ball.carrier);
            assert_eq!(w.ball.carrier, Some(1), "a ball {over} m up is caught");
        }
    }
}
