//! The trigger (`registry::Weapon::fire`): how a held weapon fires. Single
//! shots (every base game weapon), a barrel that spins up, a charge held and
//! released, bursts, and beams held on; fire modes (the fire-mode key cycles
//! a weapon's `modes`); locks for homing rounds; firing again to set off a
//! weapon's rounds still out; and the marks beams leave on what they hit
//! (a held marking beam keeps one, on what it touches), which the marking
//! team sees and its turrets and homing rounds use.
//!
//! The trigger's state (spin, charge, burst, mode, lock, a held beam's end)
//! is the player's, in snapshots, so a client predicts its own shots exactly
//! as the server fires them.
use super::*;
use crate::registry::{Fire, Weapon};

/// A mark a beam left: on a player (it follows them) or a spot, for the
/// marking team, for `life` more seconds; `by` the holder of the marking
/// stream that keeps it (a held beam has one mark, moved as it sweeps).
#[derive(Clone, Debug, PartialEq)]
pub struct Mark { pub team: Team, pub player: Option<usize>, pub pos: Vec3, pub life: f32, pub by: Option<usize> }
/// Most spot marks a snapshot carries for a team (the newest).
const SPOTS_MAX: usize = 16;

/// Turn `vel` toward `want` (a direction) by at most `max` radians, keeping
/// its speed.
pub(super) fn turn_toward(vel: Vec3, want: Vec3, max: f32) -> Vec3 {
    let speed = vel.length();
    let (from, to) = (vel.normalize_or_zero(), want.normalize_or_zero());
    if speed <= 0.0 || from == Vec3::ZERO || to == Vec3::ZERO { return vel; }
    let angle = from.dot(to).clamp(-1.0, 1.0).acos();
    if angle <= max { return to * speed; }
    let axis = from.cross(to).try_normalize().unwrap_or(Vec3::Y);
    glam::Quat::from_axis_angle(axis, max) * from * speed
}

impl World {
    /// The weapon player `i` holds, in its current fire mode.
    pub fn weapon_def(&self, i: usize) -> Option<&'static Weapon> {
        let p = self.players.get(i)?;
        let w = crate::registry::registry().weapons.get(p.weapon as usize)?;
        let mode = p.fire_mode as usize;
        Some(if mode > 0 && p.mode_of == p.weapon { w.mode_defs.get(mode - 1).unwrap_or(w) } else { w })
    }

    /// The fire-mode key: the held weapon's next mode, round to its first.
    pub(super) fn next_fire_mode(&mut self, i: usize) {
        let Some(w) = crate::registry::registry().weapons.get(self.players[i].weapon as usize) else { return };
        if w.mode_defs.is_empty() { return; }
        let p = &mut self.players[i];
        p.fire_mode = if p.mode_of == p.weapon { (p.fire_mode + 1) % (w.mode_defs.len() as u8 + 1) } else { 1 };
        (p.mode_of, p.spin, p.charge, p.burst_left) = (p.weapon, 0.0, 0.0, 0);
        if i == self.player_id {
            let name = self.weapon_def(i).map_or(String::new(), |m| m.name.to_uppercase());
            self.msg(&name, 1.0);
        }
    }

    /// Player `i`'s trigger this tick, fire `held` (and not repairing).
    pub(super) fn trigger(&mut self, i: usize, held: bool, is_local: bool, dt: f32) {
        let p = &self.players[i];
        let press = held && !p.fire_prev;
        let release = !held && p.fire_prev;
        let ready = p.cooldown <= 0.0 && p.alive;
        self.players[i].fire_prev = held;
        // A new weapon in hand starts in its first mode, unspun and uncharged.
        if self.players[i].mode_of != self.players[i].weapon {
            let p = &mut self.players[i];
            (p.mode_of, p.fire_mode, p.spin, p.charge, p.burst_left) = (p.weapon, 0, 0.0, 0.0, 0);
        }
        if self.football() {
            if held && ready { self.pass(i); }
            return;
        }
        if !self.armed(i) { self.players[i].stream = None; return; }
        let Some(w) = self.weapon_def(i) else { return };
        self.track_lock(i, w, dt);
        // Firing again sets off what this weapon has out.
        if w.remote && press && self.detonate(i) { return; }
        match w.fire {
            Fire::Single => if held && ready { self.fire_once(i, is_local, 1.0) },
            Fire::Spin => {
                let p = &mut self.players[i];
                p.spin = if held { p.spin + dt / w.spin_up.max(1e-3) } else { p.spin - dt / w.spin_down.max(1e-3) }.clamp(0.0, 1.0);
                if held && ready && self.players[i].spin >= 1.0 { self.fire_once(i, is_local, 1.0); }
            }
            Fire::Burst => {
                if press && ready && self.players[i].burst_left == 0 { self.players[i].burst_left = w.burst; }
                if self.players[i].burst_left > 0 && ready {
                    if self.pay_shot(i) {
                        self.shoot_with(i, 1.0);
                        let p = &mut self.players[i];
                        p.burst_left -= 1;
                        p.cooldown = if p.burst_left > 0 { w.burst_gap } else { w.reload };
                    } else {
                        self.players[i].burst_left = 0;
                        self.dry_click(i, is_local);
                    }
                }
            }
            Fire::Charge => {
                if held && ready {
                    self.players[i].charge += dt;
                    if w.overload > 0.0 && self.players[i].charge > w.charge + w.overload {
                        // Held too long: it bites the hand that holds it.
                        self.players[i].charge = 0.0;
                        self.players[i].cooldown = w.reload;
                        if !self.predicting {
                            let kind = self.held(i).map_or("blast", |(_, r, _)| r.damage_kind.as_str());
                            self.hurt(i, w.overload_damage, kind);
                            if i == self.player_id { self.msg("OVERLOADED", 1.2); }
                            if self.players[i].health <= 0.0 { self.kill(i, None, &w.name); }
                        }
                    }
                }
                if release && self.players[i].charge > 0.0 {
                    let fraction = (self.players[i].charge / w.charge.max(1e-3)).min(1.0);
                    self.players[i].charge = 0.0;
                    if ready { self.fire_once(i, is_local, 1.0 + (w.charge_scale - 1.0) * fraction); }
                }
            }
            Fire::Sustained => {
                let cost = w.energy * dt;
                if held && self.players[i].alive && self.players[i].energy >= cost.max(1e-6) {
                    self.players[i].energy -= cost;
                    self.reveal(i);
                    self.stream_tick(i, dt);
                } else {
                    self.players[i].stream = None;
                }
            }
        }
    }

    /// A shot if it can be paid for, else (now and then) the dry click.
    fn fire_once(&mut self, i: usize, is_local: bool, power: f32) {
        if self.pay_shot(i) {
            self.shoot_with(i, power);
            if self.hooks.live {
                if let Some(w) = self.weapon_def(i) {
                    let id = crate::registry::registry().weapons.id(self.players[i].weapon as usize).unwrap_or("").to_string();
                    self.hook(w.hook.as_deref(), super::hooks::HookEvent::Fire { player: i, weapon: id });
                }
            }
        } else { self.dry_click(i, is_local); }
    }

    fn dry_click(&mut self, _i: usize, is_local: bool) {
        // Empty: a click every so often. It never touches the weapon's
        // cooldown, which the viewmodel animates from.
        if is_local && self.dry_clock <= 0.0 { self.dry_clock = 0.35; self.push_event("dry"); }
    }

    /// Set off player `i`'s rounds of their held weapon still out; false
    /// when none are.
    fn detonate(&mut self, i: usize) -> bool {
        let Some((_, _, kind)) = self.held(i) else { return false };
        let mine: Vec<usize> = self.discs.iter().enumerate().filter(|(_, d)| d.owner == i && d.kind == kind).map(|(k, _)| k).collect();
        if mine.is_empty() || self.predicting { return !mine.is_empty(); }
        for k in mine.into_iter().rev() {
            let d = self.discs.remove(k);
            self.explode_direct(d.pos, d.owner, d.kind, d.team, None);
        }
        true
    }

    /// Lock-on for weapons that lock: the enemy nearest the crosshair
    /// within the weapon's cone and range, in sight, held there `lock`
    /// seconds.
    fn track_lock(&mut self, i: usize, w: &Weapon, dt: f32) {
        if w.lock <= 0.0 { self.players[i].lock_target = 0; return; }
        let p = &self.players[i];
        let eye = p.pos + Vec3::Y * EYE;
        let dir = look_dir(p.yaw, p.pitch);
        let team = p.team;
        let cone = w.lock_cone.to_radians().cos();
        let mut best: Option<(f32, usize)> = None;
        for (j, q) in self.players.iter().enumerate() {
            if !q.alive || !self.hostile(i, team, j) || self.sensor_hidden(j) { continue; }
            let to = q.pos + Vec3::Y * 0.9 - eye;
            let d = to.length();
            if d > w.lock_range || d < 1.0 { continue; }
            let c = to.dot(dir) / d;
            if c < cone || !self.sight_clear(eye, q.pos + Vec3::Y * 0.9) { continue; }
            if best.is_none_or(|(b, _)| c > b) { best = Some((c, j)); }
        }
        let p = &mut self.players[i];
        match best {
            Some((_, j)) if p.lock_target == j as u16 + 1 => p.lock_time += dt,
            Some((_, j)) => { p.lock_target = j as u16 + 1; p.lock_time = 0.0; }
            None => { p.lock_target = 0; p.lock_time = 0.0; }
        }
    }

    /// Whether player `i` holds a finished lock, and on whom.
    pub(super) fn locked(&self, i: usize) -> Option<usize> {
        let p = self.players.get(i)?;
        let w = self.weapon_def(i)?;
        (w.lock > 0.0 && p.lock_target > 0 && p.lock_time >= w.lock).then(|| p.lock_target as usize - 1)
    }

    /// What a homing round fired by player `i` along `dir` from `from` flies
    /// at: their lock, else the nearest fitting target in its cone and range.
    pub(super) fn seek_target(&self, i: usize, seek: &crate::registry::Seek, from: Vec3, dir: Vec3) -> u16 {
        use crate::registry::SeekTarget;
        if let Some(j) = self.locked(i) { return j as u16 + 1; }
        let team = self.players[i].team;
        let cone = seek.cone.to_radians().cos();
        let mut best: Option<(f32, usize)> = None;
        for (j, q) in self.players.iter().enumerate() {
            if !q.alive || !self.hostile(i, team, j) || self.sensor_hidden(j) { continue; }
            let fits = seek.targets.iter().any(|t| match t {
                SeekTarget::Players => true,
                SeekTarget::Jetting => q.jetting,
                SeekTarget::Marked => self.marks.iter().any(|m| m.team == team && m.player == Some(j)),
            });
            if !fits { continue; }
            let to = q.pos + Vec3::Y * 0.9 - from;
            let d = to.length();
            if d > seek.range || d < 1.0 || to.dot(dir) / d < cone { continue; }
            if best.is_none_or(|(b, _)| d < b) { best = Some((d, j)); }
        }
        best.map_or(0, |(_, j)| j as u16 + 1)
    }

    /// A held beam this tick: from the eye along the crosshair to the first
    /// thing it meets; enemies lose health and energy, teammates and the
    /// team's equipment and deployables are mended.
    fn stream_tick(&mut self, i: usize, dt: f32) {
        let Some((w, round, _)) = self.held(i) else { return };
        let Some(s) = round.stream else { return };
        let p = &self.players[i];
        let eye = p.pos + Vec3::Y * EYE;
        let dir = look_dir(p.yaw, p.pitch);
        let team = p.team;
        let end = eye + dir * s.range;
        let mut t = obstacle_hit(self.map, &self.pillars, eye, end, 0.0).unwrap_or(1.0);
        if let Some((u, _)) = self.moveable_hit(eye, end, 0.0) { t = t.min(u); }
        let (mut player, mut deploy, mut equipment) = (None, None, None);
        if let Some((u, k)) = self.deployable_hit(eye, end, 0.0, None) { if u < t { t = u; deploy = Some(k); } }
        for (k, (d, st)) in crate::equipment::definitions(self.map).iter().zip(&self.equipment).enumerate() {
            if st.health <= 0.0 && d.team as usize != team.idx() { continue; }
            if let Some(u) = segment_sphere(eye, end, d.pos(), d.radius) { if u < t { t = u; equipment = Some(k); deploy = None; } }
        }
        for (j, q) in self.players.iter().enumerate() {
            if j == i || !q.alive { continue; }
            if let Some(u) = segment_sphere(eye, end, q.pos + Vec3::Y * 0.9, PLAYER_RADIUS + 0.45) {
                if u < t { t = u; player = Some(j); deploy = None; equipment = None; }
            }
        }
        let hit = eye.lerp(end, t);
        self.players[i].stream = Some(hit);
        if self.predicting { return; }
        // A marking stream keeps its holder's one mark on what it touches:
        // an enemy (followed), else the spot; it stays `marks` seconds after
        // letting go. Open sky marks nothing.
        if round.marks > 0.0 && t < 1.0 {
            let foe = player.filter(|&j| self.hostile(i, team, j));
            let mark = Mark { team, player: foe, pos: hit, life: round.marks, by: Some(i) };
            match self.marks.iter_mut().find(|m| m.by == Some(i)) { Some(m) => *m = mark, None => self.marks.push(mark) }
        }
        let ours = |team2: Team| team2 == team && !self.ffa();
        if let Some(j) = player {
            if ours(self.players[j].team) {
                let q = &mut self.players[j];
                q.health = (q.health + s.heal * dt).min(100.0);
            } else {
                // A stream that does no harm (the target marker's) leaves a
                // cloak on.
                self.hit_by = Some(i);
                if s.damage > 0.0 { self.hurt(j, s.damage * dt, &round.damage_kind); }
                self.hit_by = None;
                // The stream's effect, held on while it's on them.
                if let Some(e) = round.effect.as_deref().and_then(|e| crate::registry::registry().effects.index(e)) { self.give_effect(j, e as u8); }
                let q = &mut self.players[j];
                q.energy = (q.energy - s.drain * dt).max(0.0);
                if self.players[j].health <= 0.0 && self.players[j].alive {
                    self.kill(j, Some(i), &w.name);
                    if i == self.player_id { self.kills += 1; self.msg("FRAG", 1.1); }
                }
            }
        }
        if let Some(k) = deploy {
            let e = &mut self.deployables[k];
            if e.team == team { e.health = (e.health + s.repair * dt).min(deploy::max_health(e.kind)); }
            else { e.health -= s.damage * dt; }
            // Held long enough on an enemy unit, the stream claims it.
            if s.claim > 0.0 && e.team != team {
                let p = &mut self.players[i];
                if p.claim_on != k as u16 + 1 { p.claim_on = k as u16 + 1; p.claim_time = 0.0; }
                p.claim_time += dt;
                if p.claim_time >= s.claim {
                    let e = &mut self.deployables[k];
                    e.team = team;
                    e.owner = (i + 1).min(255) as u8;
                    self.players[i].claim_time = 0.0;
                    if i == self.player_id { self.msg("CLAIMED", 1.4); }
                }
            } else { self.players[i].claim_on = 0; }
        } else { self.players[i].claim_on = 0; }
        if let Some(k) = equipment {
            let defs = crate::equipment::definitions(self.map);
            if defs[k].team as usize == team.idx() { self.equipment[k].repair(&defs[k], s.repair * dt); }
            else if self.equipment[k].damage(&defs[k], s.damage * dt, true) { self.equipment_destroyed(&defs[k]); }
        }
    }

    /// Marks age; those on players follow them; the dead lose theirs.
    pub(super) fn step_marks(&mut self, dt: f32) {
        if self.predicting { return; }
        for m in &mut self.marks { m.life -= dt; }
        let players = &self.players;
        self.marks.retain(|m| m.life > 0.0 && m.player.is_none_or(|j| players.get(j).is_some_and(|p| p.alive)));
        for m in &mut self.marks { if let Some(j) = m.player { m.pos = self.players[j].pos + Vec3::Y * 0.9; } }
    }

    /// Whether `team` has a mark on player `j`.
    pub fn marked(&self, team: Team, j: usize) -> bool { self.marks.iter().any(|m| m.team == team && m.player == Some(j)) }
    /// The spots `team` has marked (not on players), the newest
    /// `SPOTS_MAX`.
    pub fn marked_spots(&self, team: Team) -> Vec<Vec3> {
        let spots: Vec<Vec3> = self.marks.iter().filter(|m| m.team == team && m.player.is_none()).map(|m| m.pos).collect();
        spots[spots.len().saturating_sub(SPOTS_MAX)..].to_vec()
    }

    /// The players `team` has marked, by slot, each once.
    pub fn marked_by(&self, team: Team) -> Vec<u8> {
        let mut out: Vec<u8> = self.marks.iter().filter(|m| m.team == team).filter_map(|m| m.player).map(|j| j as u8).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}
