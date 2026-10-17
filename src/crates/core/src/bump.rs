//! Player-to-player collision, in every mode: every body blocks every other,
//! teammates included, so defenders can
//! block flag carriers and teammates can't walk through each other. Hits are
//! swept across the physics step, so fast skiers can't pass through each other
//! between ticks. Teammates only ever push each other apart: no damage, no
//! tackles. Between enemies, outside Football a hit does light damage; in
//! Football the bodies swap velocities outright and the slower player loses
//! (`football::World::football_hit`), and nobody
//! tackles a player who is down (they still block). Server and offline only; client
//! prediction does not resolve hits against other players. Marker: `bump1`.
//!
//! In Football a person also hits a bot where their game drew it
//! (`seen_hit`, `ball6`): games draw the others coasted on from the newest
//! snapshot, a guess that misses a bot's turn until the next one; nobody is
//! there to feel a bot hit where it was drawn (`lagcomp`). Hits on people
//! are judged where they really are.
use super::*;

/// Bodies are cylinders the size of the base game's light armor collision
/// box (0.5 each side of the centre, 2.3 m tall).
pub(crate) const BODY_RADIUS: f32 = 0.5;
/// Centres closer than a body's full height can touch: a player dropping
/// onto another's head connects.
pub(crate) const BODY_HEIGHT: f32 = 2.3;
/// Speed along the line of impact passes from one body to the other almost
/// whole: a blocker stops a skier and is knocked away themselves.
const RESTITUTION: f32 = 0.8;
/// A body check sounds (and shakes the camera) only for a real hit, and at
/// most this often for the local player: bodies leaning on each other close
/// a little every step, which played a thump sixty times a second.
const BUMP_SOUND_DV: f32 = 3.0;
const BUMP_SOUND_GAP: f32 = 0.3;
/// Light damage outside football: nothing for a walking bump, a few points
/// for a running collision, at most 20 for a full-speed hit.
const BUMP_FREE: f32 = 8.0;
const BUMP_SCALE: f32 = 0.45;
const BUMP_MAX: f32 = 20.0;
/// A pair that hit, in the present or as one of them saw it, can't hit
/// again as seen for this long (s): a rewound view still shows the moments
/// before the hit, up to `lagcomp::MAX_REWIND_TICKS` back.
const SEEN_HIT_GAP: f32 = 0.3;

/// Damage to each player from the velocity change `dv` a hit gave them.
pub fn bump_damage(dv: f32) -> f32 {
    ((dv - BUMP_FREE).max(0.0) * BUMP_SCALE).min(BUMP_MAX).floor()
}

/// First moment in [0, 1] when bodies moving from `rel0` to `rel1` (b minus
/// a, horizontal) come within `reach`, if they do this tick. Catches fast
/// players who would otherwise pass through each other between ticks.
fn contact_time(rel0: Vec2, rel1: Vec2, reach: f32) -> Option<f32> {
    if rel0.length() < reach { return Some(0.0); }
    let d = rel1 - rel0;
    let (qa, qb, qc) = (d.dot(d), 2.0 * rel0.dot(d), rel0.dot(rel0) - reach * reach);
    if qa < 1e-9 { return None; }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 { return None; }
    let t = (-qb - disc.sqrt()) / (2.0 * qa);
    (0.0..=1.0).contains(&t).then_some(t)
}

impl World {
    /// Resolve hits between enemies, sweeping each pair from `before` (the
    /// positions at the start of this physics step) to now.
    pub(super) fn step_bumps(&mut self, before: &[Vec3]) {
        if self.predicting { return; }
        let n = self.players.len();
        let football = self.football();
        let now = self.time;
        self.recent_hits.retain(|h| now - h.2 < SEEN_HIT_GAP);
        for a in 0..n {
            for b in a + 1..n {
                let (pa, pb) = (&self.players[a], &self.players[b]);
                if !pa.alive || !pb.alive || pa.remote || pb.remote { continue; }
                // Teammates, and anyone touching a player who is down, only
                // push apart: the game rules below are for enemy hits.
                let enemies = self.hostile(a, pa.team, b);
                let plays = enemies && !(football && (pa.stun > 0.0 || pb.stun > 0.0));
                let (a1, b1) = (pa.pos, pb.pos);
                // Sweep from the start of the step unless someone teleported.
                let (a0, b0) = match (before.get(a), before.get(b)) {
                    (Some(&a0), Some(&b0)) if a0.distance(a1) < 10.0 && b0.distance(b1) < 10.0 => (a0, b0),
                    _ => (a1, b1),
                };
                let flat = |v: Vec3| Vec2::new(v.x, v.z);
                let contact = contact_time(flat(b0 - a0), flat(b1 - a1), BODY_RADIUS * 2.0)
                    .map(|t| (t, a0.lerp(a1, t), b0.lerp(b1, t)))
                    .filter(|(_, at_a, at_b)| (at_b.y - at_a.y).abs() < BODY_HEIGHT);
                let Some((t, at_a, at_b)) = contact else {
                    if football && plays { self.seen_hit(a, b, before); }
                    continue;
                };
                // A pass-through is caught where the bodies first touched, but
                // only back along a clear line: the straight path between the
                // step's ends can cut a corner the real move slid around.
                if t > 0.0 {
                    for (i, at) in [(a, at_a), (b, at_b)] {
                        let now = self.players[i].pos;
                        if obstacle_hit(self.map, &self.pillars, now, at, PLAYER_RADIUS * 0.5).is_none() { self.players[i].pos = at; }
                    }
                }
                let d = at_b - at_a;
                let flat = Vec2::new(d.x, d.z);
                let h = flat.length();
                let normal = if h > 1e-3 { Vec3::new(flat.x / h, 0.0, flat.y / h) } else { Vec3::X };
                // Separate, never through a wall: whoever can't move stays put.
                let push = normal * ((BODY_RADIUS * 2.0 - h) * 0.5);
                for (i, dir) in [(a, -1.0f32), (b, 1.0)] {
                    let from = self.players[i].pos;
                    let to = from + push * dir;
                    if obstacle_hit(self.map, &self.pillars, from, to, PLAYER_RADIUS * 0.5).is_none() {
                        self.players[i].pos = to;
                    }
                }
                // What each one's armor does by touch (`packs`).
                self.touch(a, b);
                self.touch(b, a);
                let (va, vb) = (self.players[a].vel, self.players[b].vel);
                let closing = (va - vb).dot(normal);
                if closing <= 0.0 { continue; }
                // Heavier armor is harder to shove: the exchange is shared by mass.
                let (ma, mb) = if football { (1.0, 1.0) } else {
                    (loadout::mass_scale(self.players[a].armor), loadout::mass_scale(self.players[b].armor)) };
                let impulse = closing * (1.0 + RESTITUTION) / (1.0 / ma + 1.0 / mb);
                let (dva, dvb) = (impulse / ma, impulse / mb);
                let dv = dva.max(dvb);
                if football && plays {
                    // In Football the two bodies swap
                    // velocities outright, so a tackler stops dead and the
                    // player hit flies on at the tackler's speed.
                    self.players[a].vel = vb;
                    self.players[b].vel = va;
                } else {
                    self.players[a].vel -= normal * dva;
                    self.players[b].vel += normal * dvb;
                }
                let at = (self.players[a].pos + self.players[b].pos) * 0.5;
                self.bump_sound(a, b, dv, at);
                if !plays { continue; }
                if football {
                    self.recent_hits.push((a, b, now));
                    self.football_hit(a, b, va, vb);
                    continue;
                }
                for (i, other, dv) in [(a, b, dva), (b, a, dvb)] {
                    let damage = bump_damage(dv);
                    if damage <= 0.0 || !self.players[i].alive { continue; }
                    self.hurt(i, damage, "impact");
                    if self.players[i].health <= 0.0 { self.kill(i, Some(other), "Body check"); }
                }
            }
        }
    }

    /// A hit's thump and camera shake for the local player, or a thump
    /// where it happened offline; only a real hit (`dv`) sounds.
    fn bump_sound(&mut self, a: usize, b: usize, dv: f32, at: Vec3) {
        if dv < BUMP_SOUND_DV { return; }
        if a == self.player_id || b == self.player_id {
            if self.time - self.bump_heard >= BUMP_SOUND_GAP {
                self.bump_heard = self.time;
                self.push_event("bump");
            }
            self.trauma = (self.trauma + (dv / 40.0).min(0.5)).min(1.0);
        } else if self.network_inputs.is_empty() {
            self.spatial_sounds.push(("bump", at));
        }
    }

    /// Football: enemies `a` and `b` (both up) didn't touch this step, but
    /// one of them, a person, may have touched the other, a bot, where their
    /// game drew it: their body now against the bot as their command claims
    /// it was drawn, its `lagcomp` state at the claimed view coasted on as
    /// far as they coasted it (`football::coast`). Then it's a hit, judged
    /// as they saw it (the bot's drawn speed), and the bodies swap velocities
    /// as in any Football hit; neither is moved. Bots claim no view, and
    /// people are hit only where they are. Each pair hits once in
    /// SEEN_HIT_GAP, so the moments before a hit, still on someone's screen,
    /// never hit again.
    fn seen_hit(&mut self, a: usize, b: usize, before: &[Vec3]) {
        if self.recent_hits.iter().any(|&(x, y, _)| (x, y) == (a, b) || (x, y) == (b, a)) { return; }
        let flat = |v: Vec3| Vec2::new(v.x, v.z);
        for (me, other) in [(a, b), (b, a)] {
            if !self.players[other].is_bot { continue; }
            let (rewind, ahead) = (self.lag.rewind(me), self.lag.ahead(me));
            let Some((from, fv)) = self.lag.state(other, rewind, &self.players[other]) else { continue };
            // As drawn this step, and a step before.
            let (o1, ov) = football::coast(self.map, from, fv, ahead);
            let o0 = if ahead >= STEP { football::coast(self.map, from, fv, ahead - STEP).0 }
                else { self.lag.state(other, rewind + 1.0, &self.players[other]).map_or(o1, |s| s.0) };
            let m1 = self.players[me].pos;
            let m0 = before.get(me).copied().filter(|p| p.distance(m1) < 10.0).unwrap_or(m1);
            let Some(t) = contact_time(flat(o0 - m0), flat(o1 - m1), BODY_RADIUS * 2.0) else { continue };
            let (at_m, at_o) = (m0.lerp(m1, t), o0.lerp(o1, t));
            if (at_o.y - at_m.y).abs() >= BODY_HEIGHT { continue; }
            let d = flat(at_o - at_m);
            let normal = if d.length() > 1e-3 { d / d.length() } else { Vec2::X };
            let mv = self.players[me].vel;
            let closing = flat(mv - ov).dot(normal);
            if closing <= 0.0 { continue; }
            self.players[me].vel = ov;
            self.players[other].vel = mv;
            self.recent_hits.push((a, b, self.time));
            self.bump_sound(me, other, closing * (1.0 + RESTITUTION) * 0.5, (m1 + o1) * 0.5);
            self.football_hit(me, other, mv, ov);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two bodies leaning on each other close a little every step; that
    /// used to thump sixty times a second. Only a real hit sounds, once.
    #[test]
    fn leaning_is_quiet_and_a_hit_thumps_once() {
        let (mut w, a, b) = pair();
        let steps = |w: &mut World, n: usize, closing: f32| -> usize {
            let mut bumps = 0;
            for _ in 0..n {
                w.players[b].pos = w.players[a].pos + Vec3::new(0.95, 0.0, 0.0);
                w.players[a].vel = Vec3::new(closing / 2.0, 0.0, 0.0);
                w.players[b].vel = Vec3::new(-closing / 2.0, 0.0, 0.0);
                let before = w.players.iter().map(|p| p.pos).collect::<Vec<_>>();
                w.step_bumps(&before);
                bumps += w.events.matches("bump").count();
                w.events.clear();
                w.time += STEP;
            }
            bumps
        };
        assert_eq!(steps(&mut w, 60, 3.0), 0, "leaning into each other");
        // Hard hits a step apart: one thump, then quiet for a moment.
        assert_eq!(steps(&mut w, 5, 20.0), 1, "one thump for a burst of hits");
        w.time += 0.5;
        assert_eq!(steps(&mut w, 1, 20.0), 1, "and another hit later thumps again");
    }

    fn pair() -> (World, usize, usize) {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_match(true);
        w.players.truncate(1);
        let mut enemy = w.players[0].clone();
        enemy.team = Team::Glacier;
        enemy.is_bot = false;
        w.players.push(enemy);
        (w, 0, 1)
    }

    #[test]
    fn a_blocker_stops_a_fast_carrier_and_both_take_light_damage() {
        let (mut w, a, b) = pair();
        let at = w.players[a].pos;
        w.players[b].pos = at + Vec3::new(0.8, 0.0, 0.0);
        w.players[a].vel = Vec3::new(40.0, 0.0, 0.0);
        w.players[b].vel = Vec3::ZERO;
        w.step_bumps(&w.players.iter().map(|p| p.pos).collect::<Vec<_>>());
        assert!(w.players[a].vel.x < 6.0, "the runner is stopped: {}", w.players[a].vel.x);
        assert!(w.players[b].vel.x > 30.0, "the blocker is knocked on");
        let taken = 100.0 - w.players[a].health;
        assert!(taken > 0.0 && taken <= BUMP_MAX, "{taken}");
        assert_eq!(w.players[a].health, w.players[b].health);
        assert!(w.players[a].pos.distance(w.players[b].pos) >= BODY_RADIUS * 2.0 - 1e-3);
    }

    #[test]
    fn walking_bumps_do_no_damage_and_teammates_block_without_harm() {
        assert_eq!(bump_damage(5.0), 0.0);
        assert!(bump_damage(20.0) > 0.0 && bump_damage(20.0) <= 6.0);
        assert_eq!(bump_damage(200.0), BUMP_MAX);
        let (mut w, a, b) = pair();
        w.players[b].team = Team::Ember;
        let at = w.players[a].pos;
        w.players[b].pos = at + Vec3::new(0.5, 0.0, 0.0);
        w.players[a].vel = Vec3::new(30.0, 0.0, 0.0);
        w.step_bumps(&w.players.iter().map(|p| p.pos).collect::<Vec<_>>());
        assert!(w.players[a].vel.x < 6.0, "a teammate blocks the runner: {}", w.players[a].vel.x);
        assert!(w.players[a].pos.distance(w.players[b].pos) >= 2.0 * BODY_RADIUS - 1e-3, "pushed apart");
        assert_eq!((w.players[a].health, w.players[b].health), (100.0, 100.0), "no friendly damage");
    }

    #[test]
    fn separating_players_are_left_alone_and_a_fatal_hit_credits_the_other() {
        let (mut w, a, b) = pair();
        let at = w.players[a].pos;
        w.players[b].pos = at + Vec3::new(0.8, 0.0, 0.0);
        w.players[a].vel = Vec3::new(-5.0, 0.0, 0.0);
        w.step_bumps(&w.players.iter().map(|p| p.pos).collect::<Vec<_>>());
        assert_eq!(w.players[a].vel.x, -5.0);
        w.players[a].health = 3.0;
        w.players[b].pos = w.players[a].pos + Vec3::new(0.8, 0.0, 0.0);
        w.players[a].vel = Vec3::new(60.0, 0.0, 0.0);
        w.step_bumps(&w.players.iter().map(|p| p.pos).collect::<Vec<_>>());
        assert!(!w.players[a].alive);
        assert_eq!(w.players[b].frags, 1);
    }

    #[test]
    fn fast_players_cannot_pass_through_each_other_between_ticks() {
        for closing in [30.0f32, 60.0, 90.0, 120.0] {
            for k in 0..12 {
                let (mut w, a, b) = pair();
                let at = w.players[a].pos;
                // Start just outside contact, far enough to cross in one step.
                let gap = 2.0 * BODY_RADIUS + (k + 1) as f32 / 13.0 * closing * STEP;
                w.players[b].pos = at + Vec3::new(0.0, 0.0, gap);
                w.players[a].vel = Vec3::new(0.0, 0.0, closing / 2.0);
                w.players[b].vel = Vec3::new(0.0, 0.0, -closing / 2.0);
                let before: Vec<Vec3> = w.players.iter().map(|p| p.pos).collect();
                for i in [a, b] { let v = w.players[i].vel; w.players[i].pos += v * STEP; }
                w.step_bumps(&before);
                assert!(w.players[a].vel.z < closing / 2.0 - 1.0, "{closing} m/s, gap {gap}: passed through");
                assert!(w.players[b].pos.z > w.players[a].pos.z, "bodies stay in order");
            }
        }
    }

    /// Football: person `a` chasing `b` up the field at `speed` m/s. b ran
    /// at 12 m/s until 6 ticks ago, then cut sideways at 25; a runs on where
    /// b would be had it gone straight, `gap` metres behind. a's command
    /// claims its drawing coasted on from the snapshot `claim.0` ticks back
    /// by `claim.1` ticks (None: no claim). b is a bot if `bot`, and
    /// carries the ball if `carrying`. The world after this step's hits,
    /// and where everyone started the step.
    fn chase(gap: f32, claim: Option<(f64, f64)>, bot: bool, speed: f32, carrying: bool) -> (World, Vec<Vec3>) {
        let (mut w, a, b) = pair();
        w.set_mode(crate::map_catalog::SupportedMode::Football);
        w.players[b].is_bot = bot;
        let start = w.players[a].pos;
        let (now, cut) = (100u64, 94u64);
        let (run, side) = (Vec3::Z * 12.0, Vec3::X * 25.0);
        let straight = |t: u64| start + run * (t as f32 * STEP);
        let path = |t: u64| if t <= cut { (straight(t), run) } else { (straight(cut) + side * ((t - cut) as f32 * STEP), side) };
        for t in now - 16..now {
            (w.players[b].pos, w.players[b].vel) = path(t);
            (w.players[a].pos, w.players[a].vel) = (straight(t) - Vec3::Z * 10.0, Vec3::Z * speed);
            w.lag.record(t, &w.players);
        }
        let mut claims = vec![(0, 0); w.players.len()];
        if let Some((back, ahead)) = claim {
            claims[a] = (lagcomp::view_at(now as f64 - back), (ahead * lagcomp::VIEW_SUBTICKS as f64) as u16);
        }
        w.lag.begin_claims(now, claims);
        (w.players[b].pos, w.players[b].vel) = path(now);
        (w.players[a].pos, w.players[a].vel) = (straight(now) - Vec3::Z * gap, Vec3::Z * speed);
        w.ball = football::Ball { active: true, in_play: true, carrier: carrying.then_some(b), ..Default::default() };
        let before: Vec<Vec3> = w.players.iter().map(|p| p.pos - p.vel * STEP).collect();
        w.step_bumps(&before);
        (w, before)
    }

    /// The tackler's game coasted the carrier on from a snapshot 9 ticks
    /// back, before its cut: on their screen it ran straight into them. A
    /// bot is tackled there, judged by the speeds drawn; on the server they
    /// never met, so a person isn't, and nor is a bot without the claim, or
    /// with one from after the cut (the game drew the cut then).
    #[test]
    fn a_bot_is_tackled_where_the_tacklers_game_drew_it() {
        let (a, b) = (0, 1);
        let (w, _) = chase(0.6, Some((9.0, 9.0)), true, 30.0, true);
        assert!(w.players[b].stun > 0.0 && !w.ball.carried_by(b), "tackled where it was drawn");
        assert!(w.players[a].vel.z < 12.5 && w.players[b].vel.z > 29.5, "the bodies swap velocities: {} {}", w.players[a].vel.z, w.players[b].vel.z);
        assert!(w.players[a].pos.distance(w.players[b].pos) > 2.0, "and neither is moved");
        let (w, _) = chase(0.6, Some((9.0, 9.0)), false, 30.0, true);
        assert!(w.players[b].stun == 0.0 && w.ball.carried_by(b), "a person is hit only where they are");
        let (w, _) = chase(0.6, None, true, 30.0, true);
        assert!(w.players[b].stun == 0.0 && w.ball.carried_by(b), "no claim: no hit");
        assert_eq!((w.players[a].vel.z, w.players[b].vel.x), (30.0, 25.0));
        let (w, _) = chase(0.6, Some((3.0, 3.0)), true, 30.0, true);
        assert!(w.players[b].stun == 0.0, "drawn from after the cut: it wasn't there");
    }

    /// A drawing is never coasted past the present: a claim of the last
    /// tick drawn 15 ahead put a bot carrier running at someone 3 m in front
    /// right on them: a hit a quarter second before it could happen, which
    /// the faster tackler won, the carrier knocked down and the ball loose.
    #[test]
    fn a_bot_is_never_hit_where_it_would_be_later() {
        let (mut w, a, b) = pair();
        w.set_mode(crate::map_catalog::SupportedMode::Football);
        w.players[b].is_bot = true;
        let (start, now, run) = (w.players[a].pos, 100u64, Vec3::Z * 12.0);
        let at = |t: u64| start + run * (t as f32 * STEP);
        let ahead = at(now) + Vec3::Z * 3.0;
        for t in now - 16..now {
            (w.players[b].pos, w.players[b].vel) = (at(t), run);
            (w.players[a].pos, w.players[a].vel) = (ahead, Vec3::Z * -20.0);
            w.lag.record(t, &w.players);
        }
        let mut claims = vec![(0, 0); w.players.len()];
        claims[a] = (lagcomp::view_at(now as f64 - 1.0), (15 * lagcomp::VIEW_SUBTICKS) as u16);
        w.lag.begin_claims(now, claims);
        (w.players[b].pos, w.players[b].vel) = (at(now), run);
        (w.players[a].pos, w.players[a].vel) = (ahead, Vec3::Z * -20.0);
        w.ball = football::Ball { active: true, in_play: true, carrier: Some(b), ..Default::default() };
        let before: Vec<Vec3> = w.players.iter().map(|p| p.pos - p.vel * STEP).collect();
        w.step_bumps(&before);
        assert!(w.players[b].stun == 0.0 && w.ball.carried_by(b), "not tackled");
        assert_eq!((w.players[a].vel.z, w.players[b].vel.z), (-20.0, 12.0), "no hit at all");
    }

    /// Nor where it was long before: a chaser holding 3 m behind a bot
    /// carrier, claiming the view of a quarter second ago coasted on not at
    /// all, met the carrier where it had been then and knocked the ball
    /// loose without ever touching it. Drawn no more than
    /// `lagcomp::MAX_BEHIND_TICKS` back, the carrier is still ahead.
    #[test]
    fn a_bot_is_never_hit_where_it_was_long_before() {
        let (mut w, a, b) = pair();
        w.set_mode(crate::map_catalog::SupportedMode::Football);
        w.players[b].is_bot = true;
        let (start, now, run) = (w.players[a].pos, 100u64, Vec3::Z * 12.0);
        let at = |t: u64| start + run * (t as f32 * STEP);
        let behind = at(now) - Vec3::Z * 3.0;
        for t in now - 16..now {
            (w.players[b].pos, w.players[b].vel) = (at(t), run);
            (w.players[a].pos, w.players[a].vel) = (behind, Vec3::Z * 20.0);
            w.lag.record(t, &w.players);
        }
        let mut claims = vec![(0, 0); w.players.len()];
        claims[a] = (lagcomp::view_at(now as f64 - 15.0), 0);
        w.lag.begin_claims(now, claims);
        (w.players[b].pos, w.players[b].vel) = (at(now), run);
        (w.players[a].pos, w.players[a].vel) = (behind, Vec3::Z * 20.0);
        w.ball = football::Ball { active: true, in_play: true, carrier: Some(b), ..Default::default() };
        let before: Vec<Vec3> = w.players.iter().map(|p| p.pos - p.vel * STEP).collect();
        w.step_bumps(&before);
        assert!(w.players[b].stun == 0.0 && w.ball.carried_by(b), "not tackled");
        assert_eq!((w.players[a].vel.z, w.players[b].vel.z), (20.0, 12.0), "no hit at all");
    }

    /// A slower chase that only knocks: the bodies swap velocities once; the
    /// same drawing, still on the tackler's screen next step, hits again
    /// only after SEEN_HIT_GAP.
    #[test]
    fn a_seen_hit_happens_once() {
        let (mut w, before) = chase(0.6, Some((9.0, 9.0)), true, 20.0, false);
        let (a, b) = (0, 1);
        assert_eq!((w.players[a].vel.z, w.players[b].vel.z), (12.0, 20.0), "a knock swaps velocities");
        (w.players[a].vel, w.players[b].vel) = (Vec3::Z * 20.0, Vec3::Z * 12.0);
        w.step_bumps(&before);
        assert_eq!((w.players[a].vel.z, w.players[b].vel.z), (20.0, 12.0), "not twice");
        w.time += SEEN_HIT_GAP + STEP;
        w.step_bumps(&before);
        assert_eq!((w.players[a].vel.z, w.players[b].vel.z), (12.0, 20.0), "a new hit later");
    }

    #[test]
    fn football_teammates_block_but_never_tackle() {
        let (mut w, a, b) = pair();
        w.set_mode(crate::map_catalog::SupportedMode::Football);
        w.ball = football::Ball { active: true, in_play: true, carrier: Some(a), ..Default::default() };
        w.players[b].team = w.players[a].team;
        let at = w.players[a].pos;
        w.players[b].pos = at + Vec3::new(0.9, 0.0, 0.0);
        w.players[b].vel = Vec3::new(-30.0, 0.0, 0.0);
        w.step_bumps(&w.players.iter().map(|p| p.pos).collect::<Vec<_>>());
        assert!(w.players[b].vel.x > -10.0, "blocked");
        assert!(w.ball.carried_by(a), "no fumble from a teammate");
        assert_eq!((w.players[a].stun, w.players[b].stun), (0.0, 0.0), "no tackle");
    }
}
