//! Vehicles (`registry::Vehicle`): fliers and ground vehicles bought at a
//! vehicle station (a deployable whose entry lists `vehicles`) and boarded
//! with the use key. A flier's pilot thrusts along the nose, sideways and
//! back with the movement keys, lifts with the jet key, and the nose turns
//! toward where they look. A ground vehicle keeps to the ground: forward and
//! back drive it, the sideways keys turn it, and its guns aim where its
//! riders look. Riders look and fire their seat's weapon; an exposed seat's
//! rider sits in the open, where shots and blasts reach them. Vehicles bump
//! each other and run over enemies on foot. Snapshots carry every vehicle
//! (`Craft`).
//!
//! The movement is predicted for the pilot: their client runs the same step
//! for the vehicle they drive. Boarding, buying, seat weapons, collisions'
//! damage and wrecks are the server's.
use super::*;

/// Most seats a vehicle may have.
pub const MAX_SEATS: usize = 6;
/// Most vehicles at once.
pub const MAX_VEHICLES: usize = 24;
/// How far from a vehicle's hull a player may board it (m).
const BOARD_REACH: f32 = 2.5;
/// A purchase that buys vehicle `v` at a vehicle station.
pub const BUY_VEHICLE: u16 = 0x500;
/// How high a step a ground vehicle drives up (m above its underside).
const STEP_UP: f32 = 0.6;
/// How far a ground vehicle keeps to falling ground instead of leaving it (m).
const HUG: f32 = 0.5;
/// Closing speed past which a collision hurts (m/s).
const RAM_SPEED: f32 = 10.0;

/// A vehicle in play.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Craft {
    /// Its entry's wire number.
    pub kind: u8,
    pub team: Team,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub hull: f32,
    pub energy: f32,
    /// Who sits in each seat (player slot + 1; 0 empty), the pilot's first.
    pub riders: [u8; MAX_SEATS],
    /// Seconds until each seat's weapon fires again.
    pub cooldowns: [f32; MAX_SEATS],
    /// Seconds it has stood with nobody aboard (the server's; not sent).
    #[serde(skip)]
    pub idle: f32,
}

impl Craft {
    pub fn def(&self) -> Option<&'static crate::registry::Vehicle> { crate::registry::registry().vehicles.get(self.kind as usize) }
    /// Where seat `k` is.
    pub fn seat_at(&self, k: usize) -> Vec3 {
        let at = self.def().and_then(|d| d.seat_list().get(k).map(|s| Vec3::from(s.at))).unwrap_or(Vec3::ZERO);
        let (fwd, right) = move_basis(self.yaw);
        self.pos + right * at.x + Vec3::Y * at.y + fwd * at.z
    }
    /// Its nose.
    pub fn nose(&self) -> Vec3 { look_dir(self.yaw, self.pitch) }
    fn half(&self) -> Vec3 { self.def().map_or(Vec3::ONE, |d| Vec3::from(d.size)) }
    /// A sphere round its hull (for shots and boarding).
    pub fn radius(&self) -> f32 { let h = self.half(); h.x.max(h.y).max(h.z) * 0.8 }
    pub fn rider(&self, k: usize) -> Option<usize> { (self.riders.get(k).copied().unwrap_or(0) as usize).checked_sub(1) }
    /// Whether it keeps to the ground.
    pub fn grounded(&self) -> bool { self.def().is_some_and(|d| d.drive == crate::registry::VehicleDrive::Ground) }
}

impl World {
    /// Player `i`'s vehicle and seat, if they ride one.
    pub fn riding(&self, i: usize) -> Option<(usize, usize)> {
        let p = self.players.get(i)?;
        let v = (p.riding as usize).checked_sub(1)?;
        let craft = self.vehicles.get(v)?;
        let seat = craft.riders.iter().position(|&r| r as usize == i + 1)?;
        Some((v, seat))
    }

    /// Whether player `i` rides in the open (an exposed seat), where shots
    /// and blasts reach them.
    pub fn rides_exposed(&self, i: usize) -> bool {
        self.riding(i).is_some_and(|(v, k)| self.vehicles[v].def().and_then(|d| d.seat_list().get(k).map(|s| s.exposed)).unwrap_or(false))
    }

    /// Every vehicle's step: its movement (for the pilot's client, just
    /// theirs) and bumps, riders carried; on the server, seat weapons,
    /// running over and wrecks too.
    pub(super) fn step_vehicles(&mut self, dt: f32) {
        for v in 0..self.vehicles.len() {
            let pilot = self.vehicles[v].rider(0);
            if self.predicting && pilot != Some(self.player_id) { continue; }
            if self.vehicles[v].grounded() { self.drive(v, pilot, dt); } else { self.fly(v, pilot, dt); }
            if self.vehicles.len() > 1 { self.bump(v); }
            if !self.predicting { self.run_over(v); }
            // Riders sit where their seats are.
            for k in 0..MAX_SEATS {
                let Some(i) = self.vehicles[v].rider(k) else { continue };
                let at = self.vehicles[v].seat_at(k);
                let vel = self.vehicles[v].vel;
                if let Some(p) = self.players.get_mut(i) { p.pos = at; p.vel = vel; p.on_ground = false; p.jetting = false; p.skiing = false; }
            }
        }
        if self.predicting { return; }
        for v in 0..self.vehicles.len() { self.fire_seats(v, dt); }
        // Wrecks, and vehicles left empty too long (`abandon`), which go quietly.
        let mut k = 0;
        while k < self.vehicles.len() {
            let c = &mut self.vehicles[k];
            c.idle = if c.riders.iter().all(|&r| r == 0) { c.idle + dt } else { 0.0 };
            let abandon = c.def().map_or(0.0, |d| d.abandon);
            if c.hull <= 0.0 { self.wreck(k, None); }
            else if abandon > 0.0 && c.idle >= abandon { self.scrap(k); }
            else { k += 1; }
        }
    }

    /// Vehicle `v` goes, empty and unharmed.
    fn scrap(&mut self, v: usize) {
        self.vehicles.remove(v);
        for p in &mut self.players { if p.riding as usize > v + 1 { p.riding -= 1; } }
    }

    /// One step of vehicle `v`'s flight, flown by `pilot` (or drifting
    /// down with nobody at the controls).
    fn fly(&mut self, v: usize, pilot: Option<usize>, dt: f32) {
        let Some(def) = self.vehicles[v].def() else { return };
        let f = &def.flight;
        let (wish_x, wish_z, lift, look) = match pilot.and_then(|i| self.input_of(i).map(|c| (c.move_x, c.move_z, c.jet, i))) {
            Some((x, z, jet, i)) => (x.clamp(-1.0, 1.0), z.clamp(-1.0, 1.0), jet, Some((self.players[i].yaw, self.players[i].pitch))),
            None => (0.0, 0.0, false, None),
        };
        let map = self.map;
        let c = &mut self.vehicles[v];
        // The nose turns toward where the pilot looks.
        if let Some((yaw, pitch)) = look {
            let most = f.turn.to_radians() * dt;
            let dy = (yaw - c.yaw).sin().atan2((yaw - c.yaw).cos());
            c.yaw += dy.clamp(-most, most);
            c.pitch += (pitch.clamp(-1.2, 1.2) - c.pitch).clamp(-most, most);
        }
        let (fwd, right) = move_basis(c.yaw);
        let nose = look_dir(c.yaw, c.pitch);
        let thrusting = pilot.is_some() && (wish_x != 0.0 || wish_z != 0.0 || lift) && c.energy > 0.0;
        let mut accel = Vec3::ZERO;
        if thrusting {
            accel += nose * if wish_z >= 0.0 { f.thrust * wish_z } else { f.reverse * wish_z };
            accel += right * f.strafe * wish_x;
            if lift { accel += Vec3::Y * f.lift; }
            c.energy = (c.energy - f.drain * dt).max(0.0);
        } else {
            c.energy = (c.energy + def.regen * dt).min(def.energy_max);
        }
        let hover = if pilot.is_some() { f.hover } else { 0.0 };
        accel -= Vec3::Y * crate::defs::movement().gravity * (1.0 - hover);
        c.vel += accel * dt;
        c.vel *= (-f.drag * dt).exp();
        let speed = c.vel.length();
        if speed > f.top_speed { c.vel *= f.top_speed / speed; }
        // Into the world, never through it.
        let half = Vec3::from(def.size);
        let from = c.pos;
        let to = from + c.vel * dt;
        let pillars = &self.pillars;
        let c = &mut self.vehicles[v];
        match obstacle_hit(map, pillars, from, to, half.y * 0.8) {
            Some(t) => { c.pos = from.lerp(to, (t - 0.05).max(0.0)); c.vel *= 0.3; }
            None => c.pos = to,
        }
        let floor = crate::terrain::support_on(map, c.pos + Vec3::Y * half.y).0 + half.y;
        if c.pos.y < floor {
            let impact = -c.vel.y;
            c.pos.y = floor;
            c.vel.y = c.vel.y.max(0.0);
            // Resting on the ground, it slows.
            c.vel.x *= (-3.0 * dt).exp();
            c.vel.z *= (-3.0 * dt).exp();
            if impact > 20.0 && !self.predicting { c.hull -= (impact - 20.0) * 5.0; }
        }
        let size = self.map_size();
        let c = &mut self.vehicles[v];
        c.pos.x = c.pos.x.clamp(6.0, size - 6.0);
        c.pos.z = c.pos.z.clamp(6.0, size - 6.0);
        let _ = fwd;
    }

    /// One step of ground vehicle `v`, driven by `pilot` (or rolling to a
    /// stop with nobody at the controls): full gravity, held on any slope it
    /// may climb, its tracks gripping sideways; forward and back push it
    /// along the ground, the sideways keys turn it. Structures and pillars
    /// stop it; the ground is its floor.
    fn drive(&mut self, v: usize, pilot: Option<usize>, dt: f32) {
        let Some(def) = self.vehicles[v].def() else { return };
        let f = &def.flight;
        let (wish_x, wish_z) = match pilot.and_then(|i| self.input_of(i)) {
            Some(c) => (c.move_x.clamp(-1.0, 1.0), c.move_z.clamp(-1.0, 1.0)),
            None => (0.0, 0.0),
        };
        let (map, size) = (self.map, self.map_size());
        let half = Vec3::from(def.size);
        let gravity = crate::defs::movement().gravity;
        // The floor under it: terrain, or a deck no higher than a step.
        let floor = |pos: Vec3| crate::terrain::support_on(map, pos + Vec3::Y * (STEP_UP + PLAYER_RADIUS - half.y));
        let c = &mut self.vehicles[v];
        let (ground, normal) = floor(c.pos);
        let on_ground = c.pos.y - half.y <= ground + 0.05;
        let up = if on_ground { normal.normalize_or(Vec3::Y) } else { Vec3::Y };
        if pilot.is_some() { c.yaw = (c.yaw - wish_x * f.turn.to_radians() * dt).rem_euclid(std::f32::consts::TAU); }
        let (fwd, _) = move_basis(c.yaw);
        let ahead = (fwd - up * fwd.dot(up)).normalize_or(fwd);
        let side = ahead.cross(up).normalize_or(Vec3::X);
        let steep = up.y < f.climb.to_radians().cos();
        // Held by its brakes on a slope it can climb; sliding down a steeper one.
        let mut accel = if on_ground && !steep { -up * gravity } else { Vec3::NEG_Y * gravity };
        let fuelled = f.drain <= 0.0 || c.energy > 0.0;
        let driven = on_ground && wish_z != 0.0 && fuelled;
        if driven {
            let push = if wish_z > 0.0 { f.thrust * wish_z } else { f.reverse * wish_z };
            if !(steep && ahead.y * push > 0.0) { accel += ahead * push; }
            c.energy = (c.energy - f.drain * dt).max(0.0);
        } else {
            c.energy = (c.energy + def.regen * dt).min(def.energy_max);
        }
        c.vel += accel * dt;
        if on_ground {
            let into = c.vel.dot(up);
            if into < 0.0 { c.vel -= up * into; }
            // Its tracks hold sideways, and every way while it isn't driven.
            let hold = 1.0 - (-f.grip * dt).exp();
            if driven { c.vel -= side * c.vel.dot(side) * hold; } else { c.vel -= c.vel * hold; }
        }
        c.vel *= (-f.drag * dt).exp();
        let speed = c.vel.length();
        if speed > f.top_speed { c.vel *= f.top_speed / speed; }
        // Walls: its body swept at the nose and the tail, above a step.
        let from = c.pos;
        let to = from + c.vel * dt;
        let r = half.x.min(half.y).max(0.3) * 0.9;
        let lift = Vec3::Y * (STEP_UP + r - half.y + 0.05);
        let reach = (half.z - r).max(0.0);
        let mut t = 1.0f32;
        let mut wall = None;
        for end in [ahead * reach, -ahead * reach] {
            if let Some((u, n)) = crate::map_pack::on(map).and_then(|p| p.sweep(from + lift + end, to + lift + end, r)) {
                if u < t && n.y < 0.7 { t = u; wall = Some(n); }
            }
            if let Some(u) = pillar_hit(map, &self.pillars, from + lift + end, to + lift + end, r) {
                if u < t { t = u; wall = Some(-(to - from).normalize_or(Vec3::Z)); }
            }
        }
        let c = &mut self.vehicles[v];
        if let Some(n) = wall {
            c.pos = from.lerp(to, (t - 0.05).max(0.0));
            let n = Vec3::new(n.x, 0.0, n.z).normalize_or(Vec3::Z);
            let into = c.vel.dot(n);
            if into < 0.0 { c.vel -= n * into; }
        } else {
            c.pos = to;
        }
        // Onto the floor; over a crest it keeps to the ground unless it leaves it fast.
        let (ground, normal) = floor(c.pos);
        let gap = c.pos.y - half.y - ground;
        if gap < 0.0 || (on_ground && gap < HUG && c.vel.y <= 0.5) {
            let impact = -c.vel.y;
            c.pos.y = ground + half.y;
            let into = c.vel.dot(normal);
            if into < 0.0 { c.vel -= normal * into; }
            if gap < 0.0 && impact > 20.0 && !self.predicting { c.hull -= (impact - 20.0) * 5.0; }
            let ahead = (fwd - normal * fwd.dot(normal)).normalize_or(fwd);
            c.pitch = ahead.y.clamp(-1.0, 1.0).asin();
        } else {
            c.pitch *= (-2.0 * dt).exp();
        }
        c.pos.x = c.pos.x.clamp(6.0, size - 6.0);
        c.pos.z = c.pos.z.clamp(6.0, size - 6.0);
    }

    /// Vehicle `v` against the others: pushed out of any it overlaps, losing
    /// the speed it closed at (shared by mass); on the server, a hard bump
    /// hurts both.
    fn bump(&mut self, v: usize) {
        for u in 0..self.vehicles.len() {
            if u == v { continue; }
            let (a, b) = (&self.vehicles[v], &self.vehicles[u]);
            let gap = a.pos - b.pos;
            let reach = a.radius() + b.radius();
            let dist = gap.length();
            if dist >= reach || dist < 1.0e-4 { continue; }
            let n = gap / dist;
            let (ma, mb) = (a.def().map_or(1.0, |d| d.mass), b.def().map_or(1.0, |d| d.mass));
            let (ram_a, ram_b) = (a.def().map_or(0.0, |d| d.ram), b.def().map_or(0.0, |d| d.ram));
            let closing = -(a.vel - b.vel).dot(n);
            let share = mb / (ma + mb);
            let a = &mut self.vehicles[v];
            a.pos += n * (reach - dist);
            if closing > 0.0 { a.vel += n * closing * share; }
            if self.predicting || closing <= 0.0 { continue; }
            self.vehicles[u].vel -= n * closing * (1.0 - share);
            if closing > RAM_SPEED {
                // Each takes what the other rams with, and a knock by the
                // other's mass; a wreck goes at the end of the step.
                let hard = closing - RAM_SPEED;
                self.vehicles[v].hull -= hard * (ram_b + 2.0 * mb / ma);
                self.vehicles[u].hull -= hard * (ram_a + 2.0 * ma / mb);
            }
        }
    }

    /// Vehicle `v` runs into players on foot: it shoves them aside, and an
    /// enemy it hits faster than `RAM_SPEED` takes its `ram` damage.
    fn run_over(&mut self, v: usize) {
        let c = &self.vehicles[v];
        let Some(def) = c.def() else { return };
        let (pos, vel, team, reach, ram, pilot) = (c.pos, c.vel, c.team, c.radius() + PLAYER_RADIUS, def.ram, c.rider(0));
        let name = def.name.clone();
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive || p.riding > 0 { continue; }
            let body = p.pos + Vec3::Y * 0.9;
            let gap = body - pos;
            let dist = gap.length();
            if dist >= reach || dist < 1.0e-4 { continue; }
            let n = Vec3::new(gap.x, 0.0, gap.z).normalize_or(Vec3::X);
            let closing = (vel - p.vel).dot(n);
            let enemy = match pilot { Some(k) => self.hostile(k, team, i), None => self.ffa() || p.team != team };
            let p = &mut self.players[i];
            p.pos += n * (reach - dist);
            if closing > 0.0 { p.vel += n * closing + Vec3::Y * closing * 0.3; p.on_ground = false; }
            if enemy && ram > 0.0 && closing > RAM_SPEED { self.damage(i, (closing - RAM_SPEED) * ram, "impact", pilot, &name); }
        }
    }

    /// Riders fire their seats' weapons along their view.
    fn fire_seats(&mut self, v: usize, dt: f32) {
        let r = crate::registry::registry();
        let Some(def) = self.vehicles[v].def() else { return };
        for (k, seat) in def.seat_list().into_iter().enumerate().take(MAX_SEATS) {
            let c = &mut self.vehicles[v];
            c.cooldowns[k] = (c.cooldowns[k] - dt).max(0.0);
            let (Some(i), Some(weapon)) = (c.rider(k), seat.weapon.as_deref().and_then(|w| r.weapons.by_id(w))) else { continue };
            if c.cooldowns[k] > 0.0 || !self.input_of(i).is_some_and(|x| x.fire) { continue; }
            let Some(kind) = r.projectiles.index(&weapon.projectile) else { continue };
            let Some(round) = r.projectiles.get(kind) else { continue };
            if matches!(round.class, crate::registry::ProjectileClass::Beam | crate::registry::ProjectileClass::Stream) { continue; }
            let c = &self.vehicles[v];
            if weapon.energy > 0.0 && c.energy < weapon.energy { continue; }
            let aim = look_dir(self.players[i].yaw, self.players[i].pitch);
            let origin = c.seat_at(k) + aim * (c.radius() + 0.4);
            let (team, vel) = (c.team, aim * round.speed + c.vel * round.inherit);
            if self.discs.len() >= 256 { continue; }
            self.discs.push(Disc { pos: origin, vel, team, owner: i, life: round.life, kind: kind as u8, spin: 0.0, target: 0, power: 1.0 });
            let c = &mut self.vehicles[v];
            c.cooldowns[k] = weapon.reload;
            c.energy -= weapon.energy;
            self.players[i].shots += 1;
            if i == self.player_id { self.push_event(&weapon.sound); }
            else if self.network_inputs.is_empty() { self.spatial_sounds.push((weapon.sound.as_str(), origin)); }
        }
    }

    /// Damage vehicle `v` by `amount` for `attacker`; at none left, it's
    /// wrecked.
    pub fn damage_vehicle(&mut self, v: usize, amount: f32, attacker: Option<usize>) {
        if self.predicting || !amount.is_finite() || amount <= 0.0 { return; }
        let Some(c) = self.vehicles.get_mut(v) else { return };
        c.hull -= amount;
        if c.hull <= 0.0 { self.wreck(v, attacker); }
    }

    /// Vehicle `v` bursts: its riders die with it (credited to `attacker`).
    fn wreck(&mut self, v: usize, attacker: Option<usize>) {
        let c = self.vehicles.remove(v);
        for p in &mut self.players { if p.riding as usize > v + 1 { p.riding -= 1; } else if p.riding as usize == v + 1 { p.riding = 0; } }
        let name = c.def().map_or("Vehicle", |d| d.name.as_str());
        for k in 0..MAX_SEATS {
            if let Some(i) = c.rider(k) {
                if self.players.get(i).is_some_and(|p| p.alive) { self.kill(i, attacker.filter(|&a| a != i), name); }
            }
        }
        self.blast_serial += 1;
        self.explosions.push(Explosion { pos: c.pos, age: 0.0, max_r: 8.0, kind: 4 });
        self.spatial_sounds.push(("boom", c.pos));
    }

    /// The first vehicle but `skip` (the shooter's own) a sphere of
    /// `radius` meets along `a` to `b`, against its body's box turned with
    /// it: fraction, vehicle and outward normal.
    pub fn vehicle_hit(&self, a: Vec3, b: Vec3, radius: f32, skip: Option<usize>) -> Option<(f32, usize, Vec3)> {
        self.vehicles.iter().enumerate().filter(|&(v, _)| Some(v) != skip).filter_map(|(v, c)| {
            let half = c.half() + Vec3::splat(radius);
            let (into, out) = (glam::Quat::from_rotation_y(-c.yaw), glam::Quat::from_rotation_y(c.yaw));
            let (la, lb) = (into * (a - c.pos), into * (b - c.pos));
            let t = segment_box(la, lb, -half, half)?;
            let p = la.lerp(lb, t) / half;
            let face = if p.x.abs() >= p.y.abs() && p.x.abs() >= p.z.abs() { Vec3::X * p.x.signum() }
                else if p.y.abs() >= p.z.abs() { Vec3::Y * p.y.signum() } else { Vec3::Z * p.z.signum() };
            Some((t, v, out * face))
        }).min_by(|x, y| x.0.total_cmp(&y.0))
    }

    /// Boarding and leaving (`Input::board`), and buying vehicles.
    pub(super) fn step_boarding(&mut self) {
        if self.predicting { return; }
        for i in 0..self.players.len() {
            let Some(c) = self.input_of(i) else { continue };
            let (board, buy) = (c.board, c.buy);
            if !self.players[i].alive { continue; }
            if buy >= BUY_VEHICLE && buy < BUY_VEHICLE + 0x100 { self.buy_vehicle(i, (buy - BUY_VEHICLE) as u8); }
            if !board { continue; }
            if let Some((v, k)) = self.riding(i) { self.leave(i, v, k); } else { self.board(i); }
        }
    }

    /// Player `i` takes the nearest free seat in reach, the pilot's first.
    fn board(&mut self, i: usize) {
        let p = &self.players[i];
        let Some(v) = (0..self.vehicles.len()).filter(|&v| {
            let c = &self.vehicles[v];
            p.pos.distance(c.pos) <= c.radius() + BOARD_REACH
        }).min_by(|&a, &b| p.pos.distance(self.vehicles[a].pos).total_cmp(&p.pos.distance(self.vehicles[b].pos))) else { return };
        let c = &self.vehicles[v];
        let Some(def) = c.def() else { return };
        let empty = c.riders.iter().all(|&r| r == 0);
        if !empty && c.team != p.team && !self.ffa() { return; }
        if p.carrying.is_some() && !def.carries_flag {
            self.tell(i, "NO FLAG IN THAT ONE", 1.4);
            return;
        }
        let seats = def.seat_list().len().min(MAX_SEATS);
        let Some(k) = (0..seats).find(|&k| c.riders[k] == 0) else { return };
        let team = p.team;
        let c = &mut self.vehicles[v];
        c.riders[k] = (i + 1) as u8;
        if empty { c.team = team; }
        let p = &mut self.players[i];
        p.riding = (v + 1) as u8;
        p.repair_beam = None;
        p.repair_out = false;
        p.stream = None;
        if i == self.player_id { self.push_event("deploy"); }
    }

    /// Player `i` gets out of vehicle `v`'s seat `k`: beside it on the right,
    /// else the left, else on top, wherever the whole body gets to from the
    /// vehicle's middle (the feet clear of the ground under it: a cycle's
    /// middle rests lower than a body's radius). With no room anywhere
    /// (wedged under a bridge) they stay in. Only the feet were checked, and
    /// the top not at all, which could leave someone inside a wall or a low
    /// roof.
    fn leave(&mut self, i: usize, v: usize, k: usize) {
        let c = &self.vehicles[v];
        let (_, right) = move_basis(c.yaw);
        let half = c.half();
        let ground = crate::terrain::support_on(self.map, c.pos + Vec3::Y * half.y).0;
        let from = c.pos.with_y(c.pos.y.max(ground + PLAYER_RADIUS + 0.05));
        let vel = c.vel;
        // Beside it, on the ground there (a step up, as walking takes, or
        // a cycle on a slope had no way out on the uphill side).
        let beside = |to: Vec3| to.with_y(to.y.max(crate::terrain::support_on(self.map, to + Vec3::Y * 0.6).0 + PLAYER_RADIUS + 0.05));
        let team = self.players[i].team;
        let spots = [beside(from + right * (half.x + 1.4)), beside(from - right * (half.x + 1.4)), c.pos + Vec3::Y * (half.y + 1.2)];
        let Some(at) = spots.into_iter().find(|&to| self.body_gets_to(from, to, team, v)) else {
            self.tell(i, "NO ROOM TO GET OUT", 1.4);
            return;
        };
        self.vehicles[v].riders[k] = 0;
        let p = &mut self.players[i];
        p.riding = 0;
        p.pos = at;
        p.vel = vel;
    }

    /// Whether a walking body of `team` gets from `from` (in vehicle `own`)
    /// to `to` and stands there: its spheres against the map
    /// (`MapPack::body_sweep`, and none inside it at `to`), the terrain and
    /// pillars at the feet, the map's moveables, the deployables that block
    /// it (a field or a wall it couldn't walk through; not a pad, a floor to
    /// stand on) and the other vehicles. A sphere already inside one of
    /// those where it starts (a vehicle flown into a field, or nosed into
    /// another's hull) may leave through it, but is never set down in it.
    fn body_gets_to(&self, from: Vec3, to: Vec3, team: Team, own: usize) -> bool {
        const BODY: [f32; 4] = [0.0, 0.5, 1.0, crate::terrain::EYE];
        let r = PLAYER_RADIUS;
        let spheres = || BODY.into_iter().map(|o| (from + Vec3::Y * o, to + Vec3::Y * o));
        let clear_of_vehicle = |c: &Craft| {
            let (half, into) = (c.half() + Vec3::splat(r), glam::Quat::from_rotation_y(-c.yaw));
            let inside = |p: Vec3| (into * (p - c.pos)).abs().cmple(half).all();
            spheres().all(|(a, b)| !inside(b) && (inside(a) || segment_box(into * (a - c.pos), into * (b - c.pos), -half, half).is_none()))
        };
        obstacle_hit(self.map, &self.pillars, from, to, r).is_none()
            && crate::map_pack::on(self.map).is_none_or(|pack| pack.body_sweep(from, to).is_none() && !pack.body_stuck(to))
            && spheres().all(|(a, b)| self.moveable_hit(a, b, r).is_none())
            && self.deployables.iter().filter(|e| e.blocks(Some(team)) && e.kind.behavior() != crate::registry::Behavior::Pad)
                .all(|e| spheres().all(|(a, b)| !e.touches(b, r) && (e.touches(a, r) || e.hit(a, b, r).is_none())))
            && self.vehicles.iter().enumerate().filter(|&(k, _)| k != own).all(|(_, c)| clear_of_vehicle(c))
    }

    /// Whether player `i` stands at a vehicle station of their team, and
    /// which (deployable index).
    pub fn vehicle_station(&self, i: usize) -> Option<usize> {
        let p = self.players.get(i).filter(|p| p.alive)?;
        self.deployables.iter().position(|e| e.team == p.team && e.health > 0.0 && !e.kind.def().vehicles.is_empty()
            && Vec2::new(p.pos.x - e.pos.x, p.pos.z - e.pos.z).length() <= deploy::half_extents(e.kind).map_or(2.0, |h| h.x.max(h.z)) + 1.5)
    }

    /// Player `i` buys vehicle `kind` at the station they stand at: it
    /// appears over the station, facing where they look, with them at the
    /// controls.
    fn buy_vehicle(&mut self, i: usize, kind: u8) {
        let r = crate::registry::registry();
        let (Some(s), Some(def)) = (self.vehicle_station(i), r.vehicles.get(kind as usize)) else { return };
        let Some(id) = r.vehicles.id(kind as usize) else { return };
        let e = self.deployables[s].clone();
        if !e.kind.def().vehicles.iter().any(|v| v == id) { return; }
        let team = self.players[i].team;
        let out = self.vehicles.iter().filter(|c| c.team == team && c.kind == kind).count();
        let full = def.team_limit > 0 && out >= def.team_limit as usize;
        if full || self.vehicles.len() >= MAX_VEHICLES {
            self.tell(i, "YOUR TEAM HAS THE MOST OF THOSE IT MAY", 1.6);
            return;
        }
        if !self.pay(i, def.price) {
            self.tell(i, "NOT ENOUGH CREDITS", 1.6);
            return;
        }
        let top = e.center().y + deploy::half_extents(e.kind).map_or(0.2, |h| h.y);
        let craft = Craft { kind, team, pos: Vec3::new(e.pos.x, top + def.size[1] + 1.5, e.pos.z), vel: Vec3::ZERO,
            yaw: self.players[i].yaw, pitch: 0.0, hull: def.hull, energy: def.energy_max, riders: [0; MAX_SEATS], cooldowns: [0.0; MAX_SEATS], idle: 0.0 };
        self.vehicles.push(craft);
        let v = self.vehicles.len() - 1;
        self.vehicles[v].riders[0] = (i + 1) as u8;
        self.players[i].riding = (v + 1) as u8;
        if i == self.player_id { self.push_event("buy"); }
    }

    /// A rider who died or left the match frees their seat.
    pub(super) fn unseat(&mut self, i: usize) {
        if let Some((v, k)) = self.riding(i) { self.vehicles[v].riders[k] = 0; }
        if let Some(p) = self.players.get_mut(i) { p.riding = 0; }
    }

    /// Blast damage to vehicles within `radius` of `pos`.
    pub(super) fn blast_vehicles(&mut self, pos: Vec3, radius: f32, damage: f32, owner: usize) {
        if self.vehicles.is_empty() || self.predicting { return; }
        let hits: Vec<(usize, f32)> = self.vehicles.iter().enumerate()
            .map(|(v, c)| (v, (c.pos.distance(pos) - c.radius()).max(0.0)))
            .filter(|(_, d)| *d < radius).map(|(v, d)| (v, damage * (1.0 - d / radius))).collect();
        let attacker = (owner < self.players.len()).then_some(owner);
        // Highest index first: a wreck removes its vehicle.
        for (v, dmg) in hits.into_iter().rev() { self.damage_vehicle(v, dmg, attacker); }
    }
}
