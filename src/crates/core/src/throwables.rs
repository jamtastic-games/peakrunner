//! Hand grenades (G) and mines (M).
//! Both are thrown, not fired: hold the key to wind up (throw strength 0.3 to
//! 1.0 of full) and release to throw. A throw takes half a second.
//!
//! - A hand grenade bounces dully and explodes 2 s after the throw (radius
//!   10, damage 0.5).
//! - A mine bounces to rest, arms a moment later, and goes off when an enemy
//!   comes within 2.5 m, or when a blast nearby sets it off (radius 10,
//!   damage 0.65). A team can have MINE_TEAM_MAX mines out.
//!
//! They travel as `Disc`s (kinds GRENADE_KIND and MINE_KIND) so snapshots and
//! rendering carry them like every other projectile. A resting one has zero
//! velocity; a resting mine's `life` counts down from MINE_LIFE, and it is
//! armed once MINE_ARM of that has passed. Markers: `throw1`, `throw2` (full
//! momentum).
use super::*;

pub const GRENADE_KIND: u8 = 6;
pub const MINE_KIND: u8 = 7;

/// How far ahead of the eye a throw leaves the hand.
const RELEASE_AHEAD: f32 = 1.0;
pub const WIND_UP: f32 = 1.0;

/// What a player throws.
pub const THROW_NONE: u8 = 0;
pub const THROW_GRENADE: u8 = 1;
pub const THROW_MINE: u8 = 2;

/// Grenades and mines carried (light 5 grenades, heavy 8; 3 mines each).
pub fn max_throwables(class: ArmorClass) -> [u8; 2] {
    let a = class.def();
    [a.grenade.as_ref().map_or(0, |c| c.count), a.mine.as_ref().map_or(0, |c| c.count)]
}

/// What an armor's grenade (0) or mine (1) key throws: the throwable's entry,
/// and its projectile's entry and wire number.
pub fn thrown_by(class: ArmorClass, slot: usize) -> Option<(&'static crate::registry::Throwable, &'static crate::registry::Projectile, u8)> {
    let r = crate::registry::registry();
    let a = r.armors.get(class.0 as usize)?;
    let carry = if slot == 1 { a.mine.as_ref() } else { a.grenade.as_ref() }?;
    let t = r.throwables.by_id(&carry.throwable)?;
    let kind = r.projectiles.index(&t.projectile)?;
    Some((t, r.projectiles.get(kind)?, kind as u8))
}

/// Whether the mode takes an armor's grenade (0) or mine (1) out of play
/// (`ModeDef::banned`).
pub fn banned(def: &crate::modes::ModeDef, class: ArmorClass, slot: usize) -> bool {
    if def.banned.is_empty() { return false; }
    let Some(a) = crate::registry::registry().armors.get(class.0 as usize) else { return false };
    let carry = if slot == 1 { a.mine.as_ref() } else { a.grenade.as_ref() };
    carry.is_some_and(|c| def.banned.iter().any(|b| *b == c.throwable))
}

/// The projectile entry for wire number `kind`.
fn round(kind: u8) -> Option<&'static crate::registry::Projectile> { crate::registry::registry().projectiles.get(kind as usize) }

pub fn is_throwable(kind: u8) -> bool { round(kind).is_some_and(|r| r.class == crate::registry::ProjectileClass::Thrown) }

/// A mine or grenade at rest.
pub fn resting(d: &Disc) -> bool { is_throwable(d.kind) && d.vel == Vec3::ZERO }

/// What a resting thrown one waits for, if it is a mine.
fn trigger(d: &Disc) -> Option<crate::registry::Trigger> { round(d.kind).and_then(|r| r.trigger) }

/// A mine that is resting and armed.
pub fn armed(d: &Disc) -> bool { trigger(d).is_some_and(|t| resting(d) && d.life <= t.rest_life - t.arm) }

impl World {
    /// Throws requested this tick: `Input::throw` with its strength.
    pub(super) fn step_throws(&mut self, dt: f32) {
        for p in &mut self.players { p.throw_clock = (p.throw_clock - dt).max(0.0); }
        if self.predicting || !self.mode.def().throwables { return; }
        for i in 0..self.players.len() {
            let (what, strength) = if self.players[i].is_bot { (THROW_NONE, 0.0) }
                else if self.network_inputs.is_empty() {
                    if i == self.player_id { (self.input.throw, self.input.throw_strength) } else { (THROW_NONE, 0.0) }
                } else { self.network_inputs.get(i).map_or((THROW_NONE, 0.0), |c| (c.throw, c.throw_strength)) };
            let p = &mut self.players[i];
            if !p.alive { p.throw_held = THROW_NONE; continue; }
            // A request during the pause waits for it to end (one held at a time).
            let (what, strength) = if what == THROW_NONE { (p.throw_held, 1.0) } else { (what, strength) };
            if what == THROW_NONE { continue; }
            if p.throw_clock > 0.0 { p.throw_held = what; continue; }
            p.throw_held = THROW_NONE;
            if let Err(why) = self.throw(i, what, strength) {
                if i == self.player_id && !why.is_empty() { self.msg(&why, 1.4); }
            }
        }
    }

    /// Player `i` throws what their grenade or mine key throws, wound up
    /// `strength` (0..1).
    pub(super) fn throw(&mut self, i: usize, what: u8, strength: f32) -> Result<(), String> {
        let p = &self.players[i];
        if !p.alive || p.throw_clock > 0.0 { return Err(String::new()); }
        let slot = if what == THROW_MINE { 1 } else { 0 };
        let Some((t, round, kind)) = thrown_by(p.armor, slot) else { return Err(String::new()) };
        if banned(self.mode.def(), p.armor, slot) { return Err(format!("NO {}S IN THIS MODE", t.name.to_uppercase())); }
        if p.throwables[slot] == 0 {
            return Err(format!("NO {}S LEFT", t.name.to_uppercase()));
        }
        let origin = self.release(i, t, round, kind, strength)?;
        let p = &mut self.players[i];
        p.throwables[slot] -= 1;
        p.throw_clock = t.time;
        if i == self.player_id { self.push_event("throw"); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push(("throw", origin)); }
        Ok(())
    }

    /// Player `i` throws throwable `id` (its wire number) at full strength,
    /// apart from what they carry (a pack's or class key's throw).
    pub(super) fn throw_kind(&mut self, i: usize, id: u8, strength: f32) -> Result<(), String> {
        let r = crate::registry::registry();
        let Some(t) = r.throwables.get(id as usize) else { return Err(String::new()) };
        let Some(kind) = r.projectiles.index(&t.projectile) else { return Err(String::new()) };
        let Some(round) = r.projectiles.get(kind) else { return Err(String::new()) };
        let origin = self.release(i, t, round, kind as u8, strength)?;
        if i == self.player_id { self.push_event("throw"); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push(("throw", origin)); }
        Ok(())
    }

    /// Throw `t` (projectile `round`, wire number `kind`) from player `i`'s
    /// hand, within its team and match limits; where it left the hand.
    fn release(&mut self, i: usize, t: &crate::registry::Throwable, round: &crate::registry::Projectile, kind: u8, strength: f32) -> Result<Vec3, String> {
        let p = &self.players[i];
        if t.team_max > 0 && self.discs.iter().filter(|d| d.kind == kind && d.team == p.team).count() >= t.team_max as usize {
            return Err(format!("YOUR TEAM HAS THE MAXIMUM OF {}S OUT", t.name.to_uppercase()));
        }
        let id = crate::registry::registry().throwables.iter().find(|(_, _, e)| std::ptr::eq(*e, t)).map_or(0, |(k, _, _)| k as u8);
        if t.match_limit > 0 && self.used(super::deploy::Used::Thrown, p.team, id) >= t.match_limit {
            return Err(format!("YOUR TEAM HAS THROWN ALL THE {}S IT MAY THIS ROUND", t.name.to_uppercase()));
        }
        // 0.3 + 0.7 x how far the throw was wound up.
        let strength = t.min_strength + (1.0 - t.min_strength) * if strength.is_finite() { strength.clamp(0.0, 1.0) } else { 1.0 };
        let dir = look_dir(p.yaw, p.pitch);
        let eye = p.pos + Vec3::Y * EYE;
        // From the chest, out in front, never through a wall.
        let want = eye + dir * RELEASE_AHEAD - Vec3::Y * 0.2;
        let origin = match obstacle_hit(self.map, &self.pillars, eye, want, SHOT_RADIUS) {
            Some(t) => eye.lerp(want, (t - 0.1).max(0.0)),
            None => want,
        };
        let speed = t.speed * strength;
        // A slight upward lift, like an arm's throw.
        let vel = (dir + Vec3::Y * 0.15).normalize() * speed + p.vel * t.inherit;
        let team = p.team;
        self.discs.push(Disc { pos: origin, vel, team, owner: i, life: round.life, kind, spin: 0.0, target: 0, power: 1.0 });
        if t.match_limit > 0 { self.count_use(super::deploy::Used::Thrown, team, id); }
        Ok(origin)
    }

    /// A grenade or mine meeting a surface at `contact` with outward
    /// `normal`: a dull bounce, coming to rest on a floor.
    pub(super) fn bounce_throwable(d: &mut Disc, contact: Vec3, normal: Vec3) {
        let Some(r) = round(d.kind) else { return };
        let inward = d.vel.dot(normal);
        if inward < 0.0 { d.vel -= normal * inward * (1.0 + r.elasticity); }
        let along = d.vel - normal * d.vel.dot(normal);
        d.vel = normal * d.vel.dot(normal) + along * r.slide;
        d.pos = contact + normal * 0.12;
        if normal.y > 0.6 && d.vel.length() < r.rest_speed {
            d.vel = Vec3::ZERO;
            if let Some(t) = r.trigger { d.life = t.rest_life; }
        }
    }

    /// Armed mines with an enemy in reach: (position, owner, team).
    pub(super) fn tripped_mines(&self) -> Vec<usize> {
        self.discs.iter().enumerate().filter(|(_, d)| armed(d) && self.players.iter().enumerate().any(|(j, p)| {
            p.alive && self.hostile(d.owner, d.team, j) && trigger(d).is_some_and(|t| player_blast_distance(d.pos, p.pos) <= t.radius)
        })).map(|(k, _)| k).collect()
    }

    /// Mines near a blast go off too, any team's (a blast clears a minefield).
    pub(super) fn sympathetic_mines(&self, pos: Vec3) -> Vec<usize> {
        self.discs.iter().enumerate()
            .filter(|(_, d)| trigger(d).is_some_and(|t| resting(d) && d.pos.distance(pos) <= t.sympathy))
            .map(|(k, _)| k).collect()
    }

    /// Resting mines (any team's) that a shot from `a` to `b` passes
    /// through: shooting a mine sets it off, so players can clear their own.
    pub(super) fn shot_mines(&self, a: Vec3, b: Vec3) -> Vec<usize> {
        self.discs.iter().enumerate()
            .filter(|(_, d)| trigger(d).is_some_and(|t| resting(d) && super::segment_sphere(a, b, d.pos, t.shot_radius).is_some()))
            .map(|(k, _)| k).collect()
    }

    /// Full grenades and mines for the player's armor.
    pub(crate) fn restock_throwables(p: &mut Player) {
        p.throwables = max_throwables(p.armor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player alone on Old Holler, on open ground at their spawn.
    fn ready_world() -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(1);
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        let floor = crate::terrain::support_on(MapId::OldHoller, spot + Vec3::Y * 200.0).0;
        w.players[0].pos = Vec3::new(spot.x, floor + PLAYER_RADIUS, spot.z);
        for _ in 0..30 { w.physics_step(); }
        w
    }

    /// Another idle human on `team`, parked out of the way.
    fn add(w: &mut World, team: Team) -> usize {
        let mut p = w.players[0].clone();
        p.team = team;
        p.pos += Vec3::new(0.0, 0.0, 60.0);
        w.players.push(p);
        w.players.len() - 1
    }

    fn throw_one(w: &mut World, what: u8) {
        w.input.throw = what;
        w.input.throw_strength = 1.0;
        w.physics_step();
        w.input.throw = THROW_NONE;
    }

    #[test]
    fn a_skier_throws_ahead_of_themselves() {
        // Coasting at 60 m/s high over the map, looking where they're going.
        let mut w = ready_world();
        let dir = look_dir(w.players[0].yaw, 0.0);
        w.players[0].pitch = 0.0;
        w.players[0].pos += Vec3::Y * 80.0;
        w.players[0].vel = dir * 60.0;
        w.players[0].on_ground = false;
        throw_one(&mut w, THROW_GRENADE);
        for _ in 0..(0.5 / STEP) as usize { w.physics_step(); }
        let g = w.discs.iter().find(|d| d.kind == GRENADE_KIND).expect("grenade in flight");
        let lead = (g.pos - w.players[0].pos).dot(dir);
        // Full momentum plus the throw: about 20 m/s faster than the thrower,
        // so roughly 10 m ahead after half a second. At half momentum it fell
        // 10 m/s behind.
        assert!(lead > 7.0, "the grenade leads by {lead:.1} m");
    }

    /// A grenade thrown at a deployed wall bounces off its face. The old bounce
    /// probed only the map for a normal, found none on a deployable and used
    /// the ground's: the grenade came to rest stuck to the wall in mid-air,
    /// touching its face (0.30 m out), where a mine would have armed.
    #[test]
    fn grenades_bounce_off_deployed_walls() {
        let mut results = Vec::new();
        for angle_deg in [15.0f32, 30.0, 60.0, 90.0] {
            for speed in [15.0f32, 30.0] {
                let mut w = ready_world();
                let p = w.players[0].pos;
                let floor = crate::terrain::support_on(w.map, p + Vec3::Y * 2.0).0;
                // A wall 6 m along +X, its broad faces looking along X.
                w.deployables.push(crate::sim::deploy::Deployable { owner: 0, kind: crate::sim::deploy::DeployKind::Wall,
                    team: Team::Glacier, pos: Vec3::new(p.x + 6.0, floor, p.z), yaw: std::f32::consts::FRAC_PI_2,
                    health: 1000.0, cooldown: 0.0, aim: Vec3::Z, age: 0.0 });
                let a = angle_deg.to_radians();
                // Aimed to meet the face (0.3 m out, with the grenade's radius) mid-panel.
                let start = Vec3::new(p.x + 3.0, floor + 1.4, p.z - 2.7 * a.cos() / a.sin());
                w.discs.push(Disc { pos: start, vel: Vec3::new(a.sin(), 0.1, a.cos()) * speed,
                    team: Team::Ember, owner: 0, life: 10.0, kind: GRENADE_KIND, spin: 0.0, target: 0, power: 1.0 });
                w.players[0].pos += Vec3::new(-40.0, 0.0, 0.0);
                for _ in 0..(1.0 / STEP) as usize { w.physics_step(); }
                let d = w.discs.iter().find(|d| d.kind == GRENADE_KIND).expect("grenade");
                results.push((angle_deg, speed, d.pos.x - (p.x + 6.0)));
            }
        }
        // Bounced clear of the face, on the near side (never stuck to it at
        // -0.30, never through it).
        let stuck: Vec<_> = results.iter().filter(|r| r.2 > -0.5).collect();
        assert!(stuck.is_empty(), "stuck to or through the wall: {stuck:?} (all {results:?})");
    }

    /// Grenades and mines glancing off Highgoal's walls bounce off them,
    /// however shallow the angle (the shared bounce also serves the football,
    /// which used to go through).
    #[test]
    fn throwables_glancing_off_walls_stay_inside() {
        let map = MapId::Highgoal;
        let pack = crate::map_pack::on(map).unwrap();
        let [x0, z0, x1, z1] = crate::sim::football::field(map).unwrap().bounds.unwrap();
        let centre = Vec2::new((x0 + x1) / 2.0, (z0 + z1) / 2.0);
        let floor_at = |p: Vec2| crate::terrain::support_on(map, Vec3::new(p.x, 200.0, p.y)).0;
        let (mut tried, mut escaped) = (0, Vec::new());
        for out in [Vec2::X, -Vec2::X] {
            let along = Vec2::new(-out.y, out.x);
            let y = floor_at(centre) + 1.5;
            let from = Vec3::new(centre.x, y, centre.y);
            let (t, _) = pack.sweep(from, from + Vec3::new(out.x, 0.0, out.y) * 200.0, 0.12).expect("a side wall");
            let wall = centre + out * (t * 200.0 + 0.12);
            for kind in [GRENADE_KIND, MINE_KIND] {
                for angle_deg in [3.0f32, 6.0, 10.0, 16.0, 25.0, 45.0, 90.0] {
                    for speed in [20.0f32, 40.0, 80.0] {
                        let mut w = World::new();
                        w.set_map(map); w.start_match(true); w.players.truncate(1);
                        w.players[0].pos = Vec3::new(centre.x, 400.0, centre.y);
                        let a = angle_deg.to_radians();
                        let dir = out * a.sin() + along * a.cos();
                        let start = wall - out * 3.0 - along * 10.0 * a.cos();
                        w.discs.push(Disc { pos: Vec3::new(start.x, floor_at(start) + 1.0, start.y),
                            vel: Vec3::new(dir.x * speed, 2.0, dir.y * speed), team: Team::Ember, owner: 0,
                            life: 10.0, kind, spin: 0.0, target: 0, power: 1.0 });
                        tried += 1;
                        let hit_at = 3.0 / a.sin().max(1e-3) / speed;
                        for _ in 0..((hit_at + 0.4) / STEP) as usize { w.physics_step(); }
                        let Some(d) = w.discs.iter().find(|d| d.kind == kind) else { continue };
                        let past = (Vec2::new(d.pos.x, d.pos.z) - wall).dot(out);
                        if past > 0.0 { escaped.push(format!("kind {kind} {angle_deg}° {speed} m/s: {:.2} m past", past)); }
                    }
                }
            }
        }
        assert!(tried > 50 && escaped.is_empty(), "{} of {tried} went through: {escaped:?}", escaped.len());
    }

    #[test]
    fn grenades_bounce_then_explode_on_their_fuse() {
        let mut w = ready_world();
        let before = w.players[0].throwables[0];
        w.players[0].pitch = -0.3;
        throw_one(&mut w, THROW_GRENADE);
        assert_eq!(w.players[0].throwables[0], before - 1);
        assert!(w.discs.iter().any(|d| d.kind == GRENADE_KIND));
        // No second throw inside the throw time.
        throw_one(&mut w, THROW_GRENADE);
        assert_eq!(w.players[0].throwables[0], before - 1, "throw time");
        let serial = w.blast_serial;
        let mut steps = 0;
        while w.discs.iter().any(|d| d.kind == GRENADE_KIND) && steps < 400 { w.physics_step(); steps += 1; }
        let seconds = steps as f32 * STEP;
        assert!(w.blast_serial > serial, "it exploded");
        assert!((seconds - crate::defs::throws().grenade_fuse).abs() < 0.2, "on the fuse, not contact: {seconds}");
    }

    #[test]
    fn a_tap_throws_short_and_a_wind_up_throws_far() {
        let land = |strength: f32| {
            let mut w = ready_world();
            w.input.throw = THROW_MINE;
            w.input.throw_strength = strength;
            w.physics_step();
            w.input.throw = THROW_NONE;
            let start = w.players[0].pos;
            for _ in 0..600 { w.physics_step(); if w.discs.iter().any(resting) { break; } }
            let d = w.discs.iter().find(|d| d.kind == MINE_KIND).expect("mine");
            assert!(resting(d), "it comes to rest");
            Vec3::new(d.pos.x - start.x, 0.0, d.pos.z - start.z).length()
        };
        let (short, far) = (land(0.0), land(1.0));
        assert!(far > short * 2.0, "tap {short} m, full {far} m");
    }

    #[test]
    fn own_mines_can_be_cleared_by_shooting_them_or_a_blast() {
        let place = |w: &mut World, dx: f32| {
            let at = w.players[0].pos + Vec3::new(dx, 0.0, 0.0);
            let floor = crate::terrain::support_on(w.map, at + Vec3::Y * 2.0).0;
            w.discs.push(Disc { pos: Vec3::new(at.x, floor + 0.12, at.z), vel: Vec3::ZERO, team: Team::Ember, owner: 0,
                life: crate::defs::throws().mine_life, kind: MINE_KIND, spin: 0.0, target: 0, power: 1.0 });
            w.discs.last().unwrap().pos
        };
        // A teammate's chaingun round aimed at it.
        let mut w = ready_world();
        let mine = place(&mut w, 12.0);
        let from = mine + Vec3::new(-6.0, 1.5, 0.0);
        w.discs.push(Disc { pos: from, vel: (mine - from).normalize() * 200.0, team: Team::Ember, owner: 0,
            life: 1.0, kind: 1, spin: 0.0, target: 0, power: 1.0 });
        for _ in 0..6 { w.physics_step(); }
        assert!(!w.discs.iter().any(|d| d.kind == MINE_KIND), "shot mine goes off");
        // A friendly blast nearby.
        let mut w = ready_world();
        let mine = place(&mut w, 12.0);
        w.explode(mine + Vec3::new(1.5, 0.5, 0.0), 0, 2, Team::Ember);
        assert!(!w.discs.iter().any(|d| d.kind == MINE_KIND), "friendly blast sets it off");
    }

    #[test]
    fn mines_arm_and_trip_on_enemies_but_not_teammates() {
        let mut w = ready_world();
        let foe = add(&mut w, Team::Glacier);
        let mate = add(&mut w, Team::Ember);
        let at = w.players[0].pos + Vec3::new(20.0, 0.0, 0.0);
        let floor = crate::terrain::support_on(w.map, at + Vec3::Y * 2.0).0;
        w.discs.push(Disc { pos: Vec3::new(at.x, floor + 0.12, at.z), vel: Vec3::ZERO, team: Team::Ember, owner: 0,
            life: crate::defs::throws().mine_life, kind: MINE_KIND, spin: 0.0, target: 0, power: 1.0 });
        let mine = w.discs.last().unwrap().pos;
        for k in [foe, mate] { w.players[k].pos = mine + Vec3::new(40.0, 5.0, 0.0); }
        // A teammate standing on it: nothing.
        w.players[mate].pos = Vec3::new(mine.x, floor + PLAYER_RADIUS + 0.02, mine.z);
        for _ in 0..90 { w.physics_step(); }
        assert!(w.discs.iter().any(|d| d.kind == MINE_KIND), "teammates don't trip it");
        w.players[mate].pos = mine + Vec3::new(40.0, 5.0, 0.0);
        // An enemy walking up does.
        let health = w.players[foe].health;
        w.players[foe].pos = Vec3::new(mine.x + 2.0, floor + PLAYER_RADIUS + 0.02, mine.z);
        w.players[foe].vel = Vec3::ZERO;
        w.physics_step();
        w.physics_step();
        assert!(!w.discs.iter().any(|d| d.kind == MINE_KIND), "tripped");
        assert!(w.players[foe].health < health - 40.0 || !w.players[foe].alive, "it hurts: {}", w.players[foe].health);
    }

    #[test]
    fn unarmed_mines_wait_and_blasts_set_them_off() {
        let mut w = ready_world();
        let foe = add(&mut w, Team::Glacier);
        let at = w.players[0].pos + Vec3::new(20.0, 0.0, 0.0);
        let floor = crate::terrain::support_on(w.map, at + Vec3::Y * 2.0).0;
        let mine = Vec3::new(at.x, floor + 0.12, at.z);
        // Just settled: not armed yet, so an enemy on top is safe for a moment.
        w.discs.push(Disc { pos: mine, vel: Vec3::ZERO, team: Team::Ember, owner: 0, life: crate::defs::throws().mine_life, kind: MINE_KIND, spin: 0.0, target: 0, power: 1.0 });
        assert!(!armed(&w.discs[0]));
        // An enemy blast beside it sets it off.
        w.players[foe].pos = mine + Vec3::new(60.0, 5.0, 0.0);
        let before = w.blast_serial;
        w.explode(mine + Vec3::new(2.0, 0.5, 0.0), foe, 0, Team::Glacier);
        w.physics_step();
        assert!(!w.discs.iter().any(|d| d.kind == MINE_KIND));
        assert!(w.blast_serial >= before + 2, "the disc and the mine");
    }

    #[test]
    fn one_press_throws_once_however_many_ticks_run() {
        let mut w = ready_world();
        w.input.throw = THROW_GRENADE;
        w.input.throw_strength = 0.5;
        // Several ticks in one frame, then well past the throw time.
        w.tick(STEP * 4.5);
        for _ in 0..60 { w.tick(STEP); }
        assert_eq!(w.input.throw, THROW_NONE, "the intent is used up");
        assert_eq!(w.players[0].throwables[0], max_throwables(ArmorClass::Light)[0] - 1);
    }

    #[test]
    fn carrying_limits_and_team_mine_cap() {
        let mut w = ready_world();
        assert_eq!(w.players[0].throwables, max_throwables(ArmorClass::Light));
        w.players[0].throwables = [0, 0];
        assert_eq!(w.throw(0, THROW_GRENADE, 1.0), Err("NO GRENADES LEFT".to_string()));
        assert_eq!(w.throw(0, THROW_MINE, 1.0), Err("NO MINES LEFT".to_string()));
        w.players[0].throwables = [0, 3];
        for _ in 0..crate::defs::throws().mine_team_max {
            w.discs.push(Disc { pos: Vec3::ZERO, vel: Vec3::ZERO, team: Team::Ember, owner: 0, life: crate::defs::throws().mine_life, kind: MINE_KIND, spin: 0.0, target: 0, power: 1.0 });
        }
        assert_eq!(w.throw(0, THROW_MINE, 1.0), Err("YOUR TEAM HAS THE MAXIMUM OF MINES OUT".to_string()));
        assert_eq!(max_throwables(ArmorClass::Heavy), [8, 3]);
    }
}
