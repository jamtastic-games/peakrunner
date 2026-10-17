//! The rabbit flag, for Rabbit and Team Rabbit (docs/rabbit.md): one neutral
//! flag anyone may take. Its carrier is the rabbit.
//!
//! - **Rabbit** (the classic rules): everyone else hunts the rabbit. The
//!   hunters are one side and can't hurt each other (`World::foe`); the
//!   rabbit fights them all. The flag starts midway between the two bases
//!   (`rabbit_home`). Taking it scores a point, and a player scores another
//!   for every `HELD_POINT` seconds they have held it, all their holds added
//!   together; a hunter's kill of the rabbit scores one and the rabbit's
//!   kills two. Hunters see the rabbit for `PING_SECONDS` when it takes the
//!   flag, then again every `PING_EVERY` seconds that nobody hurt it
//!   (`RabbitFlag::ping`). The rabbit can't throw it. Dropped, it lies
//!   `flag_return` seconds, then goes home.
//! - **Team Rabbit** (two teams, each with a pool of lives): the first death
//!   drops the flag where it happened. A team holding it takes two of the
//!   enemy's lives a kill instead of one. Its carrier may throw it (Ctrl+F).
//!   Dropped and left `flag_return` seconds, it vanishes, and the next death
//!   drops it again.
//!
//! Team Rabbit's rabbit can't repair themselves. The engine moves the flag
//! here and reports grabs, held points and deaths to the rule set
//! (`rules::GameEvent`), which scores them. Markers `rabbit1`, `rabbit2`
//! (the classic Rabbit).
use super::*;

/// Seconds of holding the flag for each point (Rabbit), counted over all of
/// a player's holds in the round.
pub const HELD_POINT: f32 = 15.0;
/// Seconds hunters see where the rabbit is, when it takes the flag and at
/// each ping after (Rabbit).
pub const PING_SECONDS: f32 = 6.0;
/// Seconds from the end of one ping to the next try; it shows only if
/// nobody has hurt the rabbit in the last `PING_EVERY` seconds (Rabbit).
pub const PING_EVERY: f32 = 24.0;

/// The rabbit flag and its play state. Inactive outside the rabbit modes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RabbitFlag {
    /// A flag is on the field (Team Rabbit: from the first death until it
    /// vanishes).
    pub active: bool,
    pub pos: Vec3,
    /// A thrown flag's velocity while it flies; zero otherwise.
    pub vel: Vec3,
    /// Where it starts and goes back to (Rabbit).
    pub home: Vec3,
    pub carrier: Option<usize>,
    /// Seconds a dropped flag still lies before it goes home or vanishes.
    pub drop_timer: f32,
    /// Seconds the rabbit has held it toward their next point: their time
    /// held this round, less whole `HELD_POINT`s (Rabbit).
    pub held: f32,
    /// Seconds left of the ping that shows hunters the rabbit (Rabbit).
    pub ping: f32,
    /// Each player's time holding the flag this round, by `net_id`
    /// (`World::rabbit_key`; Rabbit), for the points and the scoreboard.
    pub times: Vec<(u32, f32)>,
    /// Who threw it, and seconds before they may take it again. Server only.
    #[serde(skip)]
    pub tossed_by: Option<usize>,
    #[serde(skip)]
    pub toss_guard: f32,
    /// Seconds to the next ping's try, and since someone else last hurt the
    /// rabbit (Rabbit). Server only.
    #[serde(skip)]
    pub ping_next: f32,
    #[serde(skip)]
    pub since_hurt: f32,
}

impl World {
    /// Rabbit or Team Rabbit is the mode in force.
    pub fn rabbit_mode(&self) -> bool { matches!(self.mode.rules(), crate::modes::Rules::Rabbit | crate::modes::Rules::TeamRabbit) }

    /// Whether player `i` carries the rabbit flag.
    pub fn is_rabbit(&self, i: usize) -> bool { self.rabbit.active && self.rabbit.carrier == Some(i) }

    /// Whose time held `RabbitFlag::times` keeps for player `i`: their
    /// `net_id`, or offline (where nobody has one) one for their slot.
    pub fn rabbit_key(&self, i: usize) -> u32 {
        self.players.get(i).map(|p| p.net_id).filter(|&id| id != 0).unwrap_or(u32::MAX - i as u32)
    }

    /// Seconds player `i` has held Rabbit's flag this round.
    pub fn rabbit_time(&self, i: usize) -> f32 {
        let id = self.rabbit_key(i);
        self.rabbit.times.iter().find(|(n, _)| *n == id).map_or(0.0, |(_, t)| *t)
    }

    /// Someone else hurt player `i`: if they are Rabbit's rabbit, the next
    /// ping waits until nobody has for `PING_EVERY` seconds.
    pub(super) fn rabbit_hurt(&mut self, i: usize, by: Option<usize>) {
        if self.mode.rules() == crate::modes::Rules::Rabbit && self.is_rabbit(i) && by.is_some_and(|a| a != i) {
            self.rabbit.since_hurt = 0.0;
        }
    }

    /// Where Rabbit's flag starts: midway between the two flag stands, on
    /// the highest floor there.
    pub fn rabbit_home(&self) -> Vec3 {
        let mid = (self.flags[0].home + self.flags[1].home) * 0.5;
        let high = Vec3::new(mid.x, self.flags[0].home.y.max(self.flags[1].home.y) + 80.0, mid.z);
        let floor = crate::terrain::support_on(self.map, high).0;
        Vec3::new(mid.x, floor + 0.4, mid.z)
    }

    /// The round's flag: Rabbit's on its stand; Team Rabbit's waits for the
    /// first death; none in other modes.
    pub(super) fn reset_rabbit(&mut self) {
        self.rabbit = RabbitFlag::default();
        if self.mode.rules() == crate::modes::Rules::Rabbit {
            let home = self.rabbit_home();
            self.rabbit = RabbitFlag { active: true, pos: home, home, ..RabbitFlag::default() };
        }
    }

    /// The flag, once per physics step: it follows the rabbit (who scores
    /// for holding it, in Rabbit), flies when thrown, lies when dropped until
    /// it goes home or vanishes, and is taken by the first player to touch
    /// it. `before`: everyone's position at the start of the step.
    pub(super) fn step_rabbit(&mut self, before: &[Vec3], dt: f32) {
        if !self.rabbit.active || self.predicting { return; }
        if let Some(c) = self.rabbit.carrier {
            match self.players.get(c).filter(|p| p.alive) {
                Some(p) => {
                    self.rabbit.pos = p.pos + Vec3::Y * 2.2;
                    if self.mode.rules() == crate::modes::Rules::Rabbit {
                        self.step_ping(dt);
                        if self.state == MatchState::Playing && self.countdown <= 0.0 { self.step_held(c, dt); }
                    }
                    return;
                }
                None => { let pos = self.rabbit.pos; self.drop_rabbit(pos); }
            }
        }
        self.rabbit.toss_guard = (self.rabbit.toss_guard - dt).max(0.0);
        if self.rabbit.vel != Vec3::ZERO {
            // A thrown flag flies until it meets the map, then lies there.
            let vel = self.rabbit.vel - Vec3::Y * crate::defs::movement().gravity * dt;
            let next = self.rabbit.pos + vel * dt;
            let far = self.map_size() - 1.0;
            let hit = obstacle_hit(self.map, &self.pillars, self.rabbit.pos, next, 0.15)
                .or_else(|| (next.x < 1.0 || next.z < 1.0 || next.x > far || next.z > far).then_some(0.0));
            match hit {
                Some(t) => { self.rabbit.pos = self.rabbit.pos.lerp(next, t); self.rabbit.vel = Vec3::ZERO; }
                None => { self.rabbit.pos = next; self.rabbit.vel = vel; }
            }
        }
        if self.rabbit.drop_timer > 0.0 {
            self.rabbit.drop_timer -= dt;
            if self.rabbit.vel == Vec3::ZERO {
                let pos = self.rabbit.pos;
                self.rabbit.pos.y = crate::terrain::support_on(self.map, pos + Vec3::Y * PLAYER_RADIUS).0 + 0.4;
            }
            if self.rabbit.drop_timer <= 0.0 {
                self.rabbit.drop_timer = 0.0;
                if self.mode.rules() == crate::modes::Rules::Rabbit {
                    self.rabbit.pos = self.rabbit.home;
                    self.rabbit.vel = Vec3::ZERO;
                } else {
                    self.rabbit.active = false;
                }
                self.push_event("return");
                return;
            }
        }
        // The first living player whose body touches the pole takes it.
        let top = flag_top(self.map, self.rabbit.pos);
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive || p.stun > 0.0 { continue; }
            if self.rabbit.tossed_by == Some(i) && self.rabbit.toss_guard > 0.0 { continue; }
            // An effect may stop its bearer taking a flag (a cloak).
            if !p.effects.is_empty() && status::bearing(&p.effects).no_flag { continue; }
            let from = before.get(i).copied().unwrap_or(p.pos);
            if touches_flag(self.rabbit.pos, top, from, p.pos, FLAG_GRAB_REACH) {
                let held = self.rabbit_time(i) % HELD_POINT;
                let rabbit = self.mode.rules() == crate::modes::Rules::Rabbit;
                let r = &mut self.rabbit;
                (r.carrier, r.drop_timer, r.vel, r.held, r.tossed_by) = (Some(i), 0.0, Vec3::ZERO, if rabbit { held } else { 0.0 }, None);
                // Hunters see the new rabbit at once.
                if rabbit { (r.ping, r.ping_next, r.since_hurt) = (PING_SECONDS, PING_SECONDS + PING_EVERY, f32::INFINITY); }
                self.push_event("flag");
                self.rule_event(rules::GameEvent::RabbitGrab { player: i });
                return;
            }
        }
    }

    /// The flag falls where `pos` is, on the floor under it.
    fn drop_rabbit(&mut self, pos: Vec3) {
        let y = crate::terrain::support_on(self.map, pos).0 + 0.4;
        let flag_return = self.mode.def().flag_return;
        let r = &mut self.rabbit;
        (r.carrier, r.pos, r.vel, r.drop_timer, r.held, r.ping, r.ping_next) = (None, Vec3::new(pos.x, y, pos.z), Vec3::ZERO, flag_return, 0.0, 0.0, 0.0);
        self.push_event("drop");
    }

    /// Rabbit: the rabbit's time held, and a point each time it passes
    /// another whole `HELD_POINT` (all their holds this round together).
    fn step_held(&mut self, c: usize, dt: f32) {
        let id = self.rabbit_key(c);
        let times = &mut self.rabbit.times;
        let k = match times.iter().position(|(n, _)| *n == id) { Some(k) => k, None => { times.push((id, 0.0)); times.len() - 1 } };
        let before = times[k].1;
        times[k].1 += dt;
        let after = times[k].1;
        self.rabbit.held = after % HELD_POINT;
        for _ in 0..((after / HELD_POINT).floor() - (before / HELD_POINT).floor()).max(0.0) as u32 {
            self.rule_event(rules::GameEvent::RabbitHeld { player: c });
        }
    }

    /// Rabbit: the ping that shows hunters the rabbit. It lasts
    /// `PING_SECONDS`; `PING_EVERY` seconds after one ends the next tries,
    /// and shows only if nobody has hurt the rabbit for that long (else it
    /// tries again as long after).
    fn step_ping(&mut self, dt: f32) {
        let r = &mut self.rabbit;
        r.since_hurt += dt;
        r.ping = (r.ping - dt).max(0.0);
        r.ping_next -= dt;
        if r.ping_next <= 0.0 {
            if r.since_hurt < PING_EVERY { r.ping_next += PING_EVERY; }
            else { r.ping = PING_SECONDS; r.ping_next += PING_SECONDS + PING_EVERY; }
        }
    }

    /// Player `i` died (`kill`): the rabbit drops the flag, and in Team
    /// Rabbit a death with no flag on the field drops a new one there. The
    /// rule set has already counted the death.
    pub(super) fn rabbit_death(&mut self, i: usize) {
        if !self.rabbit_mode() || self.predicting { return; }
        let pos = self.players[i].pos;
        if self.is_rabbit(i) {
            self.drop_rabbit(pos);
        } else if !self.rabbit.active && self.mode.rules() == crate::modes::Rules::TeamRabbit {
            self.rabbit.active = true;
            self.drop_rabbit(pos);
        }
    }

    /// Team Rabbit: the rabbit throws the flag ahead along their aim, as a
    /// CTF carrier throws theirs (`toss_flag`). Rabbit's can't be thrown.
    pub(super) fn toss_rabbit(&mut self, i: usize) {
        if self.mode.rules() != crate::modes::Rules::TeamRabbit || !self.is_rabbit(i) || !self.players[i].alive { return; }
        let p = &self.players[i];
        let dir = look_dir(p.yaw, p.pitch.max(-0.35));
        let (pos, vel) = (p.pos + Vec3::Y * 1.4 + dir * 0.6, p.vel + dir * TOSS_SPEED + Vec3::Y * TOSS_LIFT);
        let flag_return = self.mode.def().flag_return;
        let r = &mut self.rabbit;
        (r.carrier, r.pos, r.vel, r.drop_timer, r.tossed_by, r.toss_guard, r.held) = (None, pos, vel, flag_return, Some(i), TOSS_GUARD, 0.0);
        if i == self.player_id { self.push_event("throw"); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push(("throw", pos)); }
        self.push_event("drop");
    }

    /// Where a bot heads in the rabbit modes: a loose flag, else a hostile
    /// rabbit. None for the rabbit itself (it runs, `rabbit_flight`, or in
    /// Team Rabbit fights on), with no flag, or when the bot's team has it,
    /// and it fights as in the deathmatch modes.
    pub(super) fn rabbit_goal(&self, i: usize) -> Option<Vec3> {
        if !self.rabbit_mode() || !self.rabbit.active || self.is_rabbit(i) { return None; }
        let me = &self.players[i];
        match self.rabbit.carrier {
            None => Some(self.rabbit.pos),
            Some(c) if self.hostile(i, me.team, c) => self.players.get(c).map(|p| p.pos),
            Some(_) => None,
        }
    }

    /// Where a bot holding Rabbit's flag runs: of the map's spawn points and
    /// flag stands, the one farthest from its nearest chaser. None outside
    /// Rabbit (Team Rabbit's carrier fights beside its team) or with nobody
    /// to run from.
    pub(super) fn rabbit_flight(&self, i: usize) -> Option<Vec3> {
        if self.mode.rules() != crate::modes::Rules::Rabbit || !self.is_rabbit(i) { return None; }
        let team = self.players[i].team;
        let chasers: Vec<Vec3> = self.players.iter().enumerate()
            .filter(|&(j, o)| o.alive && self.hostile(i, team, j)).map(|(_, o)| o.pos).collect();
        if chasers.is_empty() { return None; }
        let spots = crate::terrain::spawn_points_on(self.map, true).into_iter()
            .chain(crate::terrain::spawn_points_on(self.map, false)).map(|(p, _)| p)
            .chain(self.flags.iter().map(|f| f.home));
        let safety = |p: Vec3| chasers.iter().map(|c| c.distance(p)).fold(f32::INFINITY, f32::min);
        spots.max_by(|a, b| safety(*a).total_cmp(&safety(*b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    fn arena(mode: SupportedMode) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(mode);
        w.start_match(true);
        w.state = MatchState::Playing;
        w.countdown = 0.0;
        w
    }

    /// Steps the flag with nobody moving.
    fn step(w: &mut World, seconds: f32) {
        for _ in 0..(seconds / STEP).round() as usize {
            let before: Vec<Vec3> = w.players.iter().map(|p| p.pos).collect();
            w.step_rabbit(&before, STEP);
        }
    }

    fn rabbit() -> SupportedMode { SupportedMode::parse("rabbit").expect("rabbit") }
    fn team_rabbit() -> SupportedMode { SupportedMode::parse("team_rabbit").expect("team_rabbit") }

    #[test]
    fn the_rabbit_takes_the_flag_scores_for_holding_it_and_drops_it_on_death() {
        let mut w = arena(rabbit());
        assert!(w.rabbit.active && w.rabbit.carrier.is_none());
        let home = w.rabbit.home;
        assert!(home.distance(w.rabbit_home()) < 1e-3);
        // Player 0 walks onto it: one point for the grab.
        let frags = w.players[0].frags;
        w.players[0].pos = home + Vec3::Y * PLAYER_RADIUS;
        step(&mut w, STEP);
        assert!(w.is_rabbit(0));
        assert_eq!(w.players[0].frags, frags + 1, "a point for taking it");
        // Holding on: a point every HELD_POINT seconds.
        step(&mut w, HELD_POINT * 2.0 + 0.1);
        assert_eq!(w.players[0].frags, frags + 3);
        // Killed: the flag lies where they fell, then goes home.
        let at = w.players[0].pos;
        w.kill(0, None, "Fall");
        assert!(w.rabbit.carrier.is_none() && Vec2::new(w.rabbit.pos.x - at.x, w.rabbit.pos.z - at.z).length() < 0.01);
        let wait = w.mode.def().flag_return + 0.1;
        step(&mut w, wait);
        assert!(w.rabbit.pos.distance(home) < 1e-3, "back on its stand");
    }

    #[test]
    fn the_rabbit_cannot_throw_the_flag_but_a_team_rabbit_carrier_can() {
        let mut w = arena(rabbit());
        w.rabbit.carrier = Some(0);
        w.toss_rabbit(0);
        assert_eq!(w.rabbit.carrier, Some(0), "Rabbit's flag stays in hand");
        let mut w = arena(team_rabbit());
        w.rabbit = RabbitFlag { active: true, carrier: Some(0), pos: w.players[0].pos, ..RabbitFlag::default() };
        w.toss_rabbit(0);
        assert!(w.rabbit.carrier.is_none() && w.rabbit.vel.length() > 10.0, "thrown");
        // The thrower can't take it straight back.
        w.rabbit.pos = w.players[0].pos;
        w.rabbit.vel = Vec3::ZERO;
        step(&mut w, STEP);
        assert!(w.rabbit.carrier.is_none());
    }

    #[test]
    fn team_rabbit_drops_a_flag_at_the_first_death_and_it_vanishes_if_left() {
        let mut w = arena(team_rabbit());
        assert!(!w.rabbit.active, "no flag before anyone dies");
        let at = w.players[1].pos;
        w.kill(1, None, "Fall");
        assert!(w.rabbit.active && Vec2::new(w.rabbit.pos.x - at.x, w.rabbit.pos.z - at.z).length() < 0.01);
        // Nobody takes it: it vanishes, and the next death drops another.
        for p in &mut w.players { p.pos += Vec3::new(400.0, 0.0, 0.0); }
        let wait = w.mode.def().flag_return + 0.1;
        step(&mut w, wait);
        assert!(!w.rabbit.active);
        let next = w.players[2].pos;
        w.kill(2, None, "Fall");
        assert!(w.rabbit.active && Vec2::new(w.rabbit.pos.x - next.x, w.rabbit.pos.z - next.z).length() < 0.01);
    }

    #[test]
    fn hunters_are_one_side_against_the_rabbit() {
        let mut w = arena(rabbit());
        let hostile = |w: &World, a: usize, b: usize| w.hostile(a, w.players[a].team, b);
        // Nobody holds the flag: nobody is anyone's enemy.
        assert!(!hostile(&w, 1, 2) && !w.foe(1, 2));
        w.rabbit.carrier = Some(0);
        assert!(hostile(&w, 0, 1) && hostile(&w, 1, 0) && w.foe(2, 0), "the rabbit against everyone");
        let other = (2..w.players.len()).find(|&j| w.players[j].team != w.players[1].team).unwrap();
        assert!(!hostile(&w, 1, other) && !w.foe(1, other), "hunters can't hurt each other, whatever their team");
        // A hunter's blast hurts the rabbit, never a hunter beside it.
        let at = Vec3::new(1000.0, 300.0, 1000.0);
        for (k, d) in [(0, 0.0f32), (other, 1.5)] { w.players[k].pos = at + Vec3::X * d; w.players[k].health = 100.0; }
        w.explode(at + Vec3::Y * 0.5, 1, 0, w.players[1].team);
        assert!(w.players[0].health < 100.0 && w.players[other].health == 100.0, "{} {}", w.players[0].health, w.players[other].health);
        // Team Rabbit goes by team.
        let w = arena(team_rabbit());
        let enemy = (0..w.players.len()).find(|&j| w.players[j].team != w.players[0].team).unwrap();
        let mate = (1..w.players.len()).find(|&j| w.players[j].team == w.players[0].team).unwrap();
        assert!(w.foe(0, enemy) && !w.foe(0, mate));
    }

    #[test]
    fn time_held_adds_up_over_every_hold() {
        let mut w = arena(rabbit());
        w.players[0].pos = w.rabbit.home + Vec3::Y * PLAYER_RADIUS;
        step(&mut w, STEP);
        assert!(w.is_rabbit(0));
        let first = w.players[0].frags;
        step(&mut w, 10.0);
        assert_eq!(w.players[0].frags, first, "10 s: no point yet");
        w.kill(0, None, "Fall");
        // Up again and onto the flag where it lies: a point for the grab,
        // and five more seconds finish the first hold's point.
        (w.players[0].alive, w.players[0].health) = (true, 100.0);
        w.players[0].pos = w.rabbit.pos + Vec3::Y * PLAYER_RADIUS;
        step(&mut w, STEP);
        assert!(w.is_rabbit(0));
        assert_eq!(w.players[0].frags, first + 1);
        assert!((w.rabbit.held - 10.0).abs() < 0.1, "picks up where it left off: {}", w.rabbit.held);
        step(&mut w, 5.2);
        assert_eq!(w.players[0].frags, first + 2, "the second hold finishes the point");
        assert!((w.rabbit_time(0) - 15.2).abs() < 0.1 && w.rabbit_time(1) == 0.0, "{}", w.rabbit_time(0));
    }

    #[test]
    fn hunters_see_the_rabbit_in_pings_that_wait_while_it_is_hurt() {
        let mut w = arena(rabbit());
        w.players[0].pos = w.rabbit.home + Vec3::Y * PLAYER_RADIUS;
        step(&mut w, STEP);
        assert!(w.is_rabbit(0) && w.rabbit.ping > PING_SECONDS - 0.1, "shown as it takes the flag");
        step(&mut w, PING_SECONDS + 1.0);
        assert_eq!(w.rabbit.ping, 0.0, "then hidden");
        // Unhurt, the next ping comes PING_EVERY after the first ended.
        step(&mut w, PING_EVERY - 1.2);
        assert_eq!(w.rabbit.ping, 0.0);
        step(&mut w, 0.4);
        assert!(w.rabbit.ping > 0.0, "shown again");
        // Hurt by a hunter while it waits: that ping is skipped, and the next
        // shows once nobody has hurt it for PING_EVERY.
        step(&mut w, PING_SECONDS + 1.0);
        w.damage(0, 1.0, "blast", Some(1), "Disc launcher");
        step(&mut w, PING_EVERY);
        assert_eq!(w.rabbit.ping, 0.0, "skipped: it was hurt");
        step(&mut w, PING_EVERY);
        assert!(w.rabbit.ping > 0.0, "shown once it was left alone");
        // Dropped, no ping.
        w.kill(0, None, "Fall");
        assert_eq!(w.rabbit.ping, 0.0);
    }

    #[test]
    fn rabbit_leaves_out_the_mortar_and_mines_and_lets_the_rabbit_mend() {
        let mut w = arena(rabbit());
        let r = crate::registry::registry();
        let (mortar, disc) = (r.weapons.index("mortar").unwrap() as u8, r.weapons.index("disc").unwrap() as u8);
        assert!(!w.mode.def().allows(mortar) && w.mode.def().allows(disc));
        assert!(arena(team_rabbit()).mode.def().allows(mortar), "Team Rabbit keeps it");
        // A mine won't throw; a grenade will.
        w.players[0].armor = ArmorClass::Light;
        w.players[0].throwables = crate::sim::throwables::max_throwables(ArmorClass::Light);
        let mines = w.players[0].throwables[1];
        assert!(mines > 0);
        assert!(w.throw(0, crate::sim::throwables::THROW_MINE, 1.0).is_err());
        assert_eq!(w.players[0].throwables[1], mines);
        // The rabbit repairs themselves, as anyone may; Team Rabbit's can't.
        for mode in [rabbit(), team_rabbit()] {
            let mut w = arena(mode);
            w.rabbit = RabbitFlag { active: true, carrier: Some(0), pos: w.players[0].pos, ..RabbitFlag::default() };
            w.players[0].pos = Vec3::new(1000.0, 300.0, 1000.0);
            (w.players[0].health, w.players[0].energy, w.input.repair) = (30.0, crate::defs::movement().energy_max, true);
            for _ in 0..60 { w.step_repair(STEP); }
            assert_eq!(w.players[0].health > 30.0, mode == rabbit(), "{}: {}", mode.key(), w.players[0].health);
        }
    }

    #[test]
    fn bots_go_for_a_loose_flag_and_hunt_an_enemy_rabbit() {
        let mut w = arena(team_rabbit());
        assert_eq!(w.rabbit_goal(0), None, "no flag yet");
        w.rabbit = RabbitFlag { active: true, pos: Vec3::new(900.0, 100.0, 900.0), ..RabbitFlag::default() };
        assert_eq!(w.rabbit_goal(0), Some(w.rabbit.pos));
        let enemy = (0..w.players.len()).find(|&j| w.players[j].team != w.players[0].team).unwrap();
        let mate = (1..w.players.len()).find(|&j| w.players[j].team == w.players[0].team).unwrap();
        w.rabbit.carrier = Some(enemy);
        assert_eq!(w.rabbit_goal(0), Some(w.players[enemy].pos));
        w.rabbit.carrier = Some(mate);
        assert_eq!(w.rabbit_goal(0), None, "their own team has it");
    }
}

#[cfg(test)]
mod scoring {
    use super::*;
    use crate::map_catalog::SupportedMode;

    fn arena(mode: &str) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(SupportedMode::parse(mode).expect("mode"));
        w.start_match(true);
        (w.state, w.countdown) = (MatchState::Playing, 0.0);
        w
    }

    #[test]
    fn rabbit_scores_the_classic_way_and_ends_at_the_limit() {
        let mut w = arena("rabbit");
        let limit = w.mode.def().score_limit;
        assert_eq!(limit, 1200, "the classic's default: the clock decides");
        assert_eq!(w.mode.def().flag_return, 45.0);
        // Taking the flag: a point.
        w.rabbit.carrier = Some(0);
        w.rule_event(rules::GameEvent::RabbitGrab { player: 0 });
        assert_eq!(w.players[0].frags, 1);
        // Player 0 is the one at the screen, unnamed offline: "You".
        let line = if w.is_you(0) { "You are the rabbit".to_string() } else { format!("{} is the rabbit", w.display_name(0)) };
        assert!(w.feed.iter().any(|e| e.line().contains(&line)), "{line}");
        // The rabbit's kills count two (the engine counts one).
        w.kill(1, Some(0), "Disc launcher");
        assert_eq!(w.players[0].frags, 3);
        // One hunter killing another (they can't hurt each other) scores nothing.
        w.kill(2, Some(3), "Disc launcher");
        assert_eq!(w.players[3].frags, 0);
        // A hunter's kill of the rabbit: one, counted before the flag drops.
        w.kill(0, Some(3), "Disc launcher");
        assert_eq!(w.players[3].frags, 1);
        assert!(w.rabbit.carrier.is_none());
        // A suicide costs nothing.
        w.kill(3, Some(3), "Disc launcher");
        assert_eq!(w.players[3].frags, 1);
        // Held points up to the limit: the match is over.
        w.players[4].frags = limit - 1;
        w.rule_event(rules::GameEvent::RabbitHeld { player: 4 });
        assert_eq!(w.state, MatchState::Ended);
    }

    #[test]
    fn team_rabbit_takes_a_life_a_death_and_two_with_the_flag() {
        let mut w = arena("team_rabbit");
        let limit = w.mode.def().score_limit;
        assert_eq!(limit, 50);
        let a = 0;
        let them = (0..w.players.len()).filter(|&j| w.players[j].team != w.players[a].team).collect::<Vec<_>>();
        let (b, c) = (them[0], them[1]);
        let (mine, theirs) = (w.players[a].team.idx(), w.players[b].team.idx());
        // Killing an enemy: one of their lives. Falling: one of your own.
        w.kill(b, Some(a), "Disc launcher");
        assert_eq!(w.score[mine], 1);
        w.kill(a, None, "Fall");
        assert_eq!(w.score[theirs], 1);
        // With the flag in your team's hands, a kill takes two.
        w.players[a].alive = true;
        w.players[a].health = 100.0;
        w.rabbit = RabbitFlag { active: true, carrier: Some(a), pos: w.players[a].pos, ..RabbitFlag::default() };
        w.kill(c, Some(a), "Disc launcher");
        assert_eq!(w.score[mine], 3);
        // The last life ends it.
        w.score[mine] = limit - 1;
        w.players[b].alive = true;
        w.kill(b, Some(a), "Disc launcher");
        assert_eq!((w.score[mine], w.state), (limit, MatchState::Ended));
    }
}

#[cfg(test)]
mod bots {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// An offline match of bots (the person stands still), stepped through
    /// `World::tick` for `seconds`, reporting each frame to `watch`.
    fn play(mode: &str, map: MapId, seconds: usize, mut watch: impl FnMut(&World)) -> World {
        let _ = crate::bot_nav::graph(map);
        let mut w = World::new();
        w.reseed(0x5EED_0002);
        w.set_map(map);
        w.set_mode(SupportedMode::parse(mode).expect("mode"));
        w.start_match(true);
        for _ in 0..seconds * 60 {
            w.input = Input::default();
            w.tick(STEP);
            watch(&w);
        }
        w
    }

    #[test]
    fn rabbit_bots_take_the_flag_and_hold_it_for_points() {
        let (mut carried, mut held_points, mut was) = (false, 0, (None, 0.0));
        play("rabbit", MapId::Cairnhold, 120, |w| {
            let r = &w.rabbit;
            carried |= r.carrier.is_some_and(|c| c != w.player_id && w.players[c].is_bot);
            // A held point takes HELD_POINT off the time held by the same rabbit.
            if r.carrier.is_some() && r.carrier == was.0 && r.held < was.1 { held_points += 1; }
            was = (r.carrier, r.held);
        });
        assert!(carried, "a bot took the flag");
        assert!(held_points >= 2, "bots held it for points ({held_points})");
    }

    #[test]
    fn team_rabbit_bots_raise_the_flag_carry_it_and_take_lives() {
        let (mut fell, mut carried) = (false, false);
        let w = play("team_rabbit", MapId::Frostline, 150, |w| {
            fell |= w.rabbit.active;
            carried |= w.rabbit.carrier.is_some_and(|c| c != w.player_id && w.players[c].is_bot);
        });
        assert!(fell, "a death brought the flag");
        assert!(carried, "a bot took it");
        assert!(w.score[0] + w.score[1] >= 5, "deaths took lives: {:?}", w.score);
    }
}

#[cfg(test)]
mod homes {
    use super::*;

    /// Rabbit's flag starts on dry, open footing on every map that plays it:
    /// not in water or lava, under no roof, above the ground.
    #[test]
    fn every_rabbit_map_starts_the_flag_on_dry_open_ground() {
        for m in crate::map_catalog::maps_for(crate::map_catalog::SupportedMode::parse("rabbit").unwrap()) {
            let mut w = World::new();
            w.set_map(m);
            let h = w.rabbit_home();
            assert!(crate::water::at(m, &[], h + Vec3::Y * 0.5).is_none(), "{}: in water at {h:?}", m.key());
            assert!(crate::water::skim_surface(m, h.x, h.z).is_none_or(|s| s < h.y), "{}: under skim water", m.key());
            assert!(crate::map_pack::on(m).and_then(|p| p.lava_cell(h.x, h.z)).is_none(), "{}: on lava", m.key());
            assert!(obstacle_hit(m, &[], h + Vec3::Y * 0.5, h + Vec3::Y * 40.0, 0.2).is_none(), "{}: roofed", m.key());
            assert!(h.y >= crate::terrain::height_on(m, h.x, h.z), "{}: under the ground", m.key());
        }
    }
}
