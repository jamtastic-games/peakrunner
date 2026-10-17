//! What placed things do besides their bodies (`registry::Deployable`):
//! auras hold an effect on players near; teleporters take a player to their
//! partner; shocks hurt enemies who touch them; sensors sense enemies for
//! their team (turrets reach their sensed range, the HUD marks them, and an
//! alarm sounds when one comes in). Pads throw whoever stands on them in
//! `deploy::collide_deployables`, predicted with the movement.
//!
//! Auras, teleporters and shocks are the server's (their results arrive in
//! snapshots); sensing is worked out the same way on every machine from
//! what the snapshot holds, so the HUD needs nothing extra on the wire.
use super::*;
use crate::registry::{Behavior, Targets};
use deploy::{DeployKind, Deployable};

/// Speed (m/s) above which a motion sensor senses a player.
const MOVING: f32 = 2.0;

impl World {
    /// The enemies of `team` its sensors sense now (slots), each once.
    pub fn sensed(&self, team: Team) -> Vec<usize> {
        let mut out = Vec::new();
        let sensors: Vec<(&Deployable, &crate::registry::Sensor)> = self.deployables.iter()
            .filter(|e| e.team == team && e.health > 0.0)
            .filter_map(|e| e.kind.def().sensor.as_ref().map(|s| (e, s))).collect();
        if sensors.is_empty() { return out; }
        for (j, p) in self.players.iter().enumerate() {
            if !p.alive || p.hidden() || !self.hostile_team(team, j) || status::sensor_hidden(&p.effects) { continue; }
            if sensors.iter().any(|(e, s)| e.center().distance(p.pos) <= s.range && (!s.moving || p.vel.length() > MOVING)) { out.push(j); }
        }
        out
    }

    /// Whether `team` has a sensor standing (turrets then reach their
    /// sensed range).
    pub fn has_sensor(&self, team: Team) -> bool {
        self.deployables.iter().any(|e| e.team == team && e.health > 0.0 && e.kind.def().sensor.is_some())
    }

    /// Whether player `j` is an enemy of `team`.
    fn hostile_team(&self, team: Team, j: usize) -> bool { self.ffa() || self.players.get(j).is_some_and(|p| p.team != team) }

    /// The server's part each tick: auras, teleporters and shocks.
    pub(super) fn step_placed(&mut self, dt: f32) {
        for p in &mut self.players {
            p.teleport_clock = (p.teleport_clock - dt).max(0.0);
            p.shock_clock = (p.shock_clock - dt).max(0.0);
        }
        if self.predicting || self.deployables.is_empty() { return; }
        let effects = &crate::registry::registry().effects;
        for k in 0..self.deployables.len() {
            let e = self.deployables[k].clone();
            let d = e.kind.def();
            if e.health <= 0.0 { continue; }
            if let Some(a) = &d.aura {
                let Some(id) = effects.index(&a.effect) else { continue };
                for j in 0..self.players.len() {
                    let p = &self.players[j];
                    if !p.alive || p.pos.distance(e.center()) > a.range { continue; }
                    let enemy = self.hostile_team(e.team, j);
                    let take = match a.targets { Targets::Team => !enemy, Targets::Enemies => enemy, Targets::All => true };
                    if take { self.give_effect(j, id as u8); }
                }
            }
            if let Some(s) = &d.shock {
                for j in 0..self.players.len() {
                    let p = &self.players[j];
                    if !p.alive || p.shock_clock > 0.0 || !self.hostile_team(e.team, j) || !e.touches(p.pos, PLAYER_RADIUS) { continue; }
                    self.players[j].shock_clock = s.cooldown;
                    if let Some(id) = s.effect.as_deref().and_then(|x| effects.index(x)) { self.give_effect(j, id as u8); }
                    if s.damage > 0.0 {
                        self.hurt(j, s.damage, &s.damage_kind);
                        if self.players[j].health <= 0.0 && self.players[j].alive {
                            let owner = (e.owner as usize).checked_sub(1).filter(|&o| o < self.players.len());
                            self.kill(j, owner, d.name.as_str());
                        }
                    }
                }
            }
            if let Some(t) = &d.teleport {
                for j in 0..self.players.len() {
                    let p = &self.players[j];
                    if !p.alive || p.teleport_clock > 0.0 || p.team != e.team || (p.carrying.is_some() && !t.carrier)
                        || !e.touches(p.pos, PLAYER_RADIUS) { continue; }
                    let Some(to) = self.partner(k) else { continue };
                    let out = self.deployables[to].clone();
                    let half = deploy::half_extents(out.kind).unwrap_or(deploy::GATE_HALF);
                    let ahead = -out.front();
                    let p = &mut self.players[j];
                    let speed = Vec2::new(p.vel.x, p.vel.z).length();
                    p.pos = out.pos + ahead * (half.z + PLAYER_RADIUS + 0.6) + Vec3::Y * (PLAYER_RADIUS + 0.05);
                    p.vel = ahead * speed + Vec3::Y * p.vel.y.max(0.0);
                    p.teleport_clock = t.cooldown;
                    if j == self.player_id { self.push_event("deploy"); }
                }
            }
        }
    }

    /// Player `i`'s input this tick (a person's; bots have none).
    pub(super) fn input_of(&self, i: usize) -> Option<&Input> {
        if self.players.get(i)?.is_bot { return None; }
        if self.network_inputs.is_empty() { (i == self.player_id).then_some(&self.input) } else { self.network_inputs.get(i) }
    }

    /// Whether player `i` stands at one of their team's command stations.
    pub fn at_command(&self, i: usize) -> bool {
        let Some(p) = self.players.get(i).filter(|p| p.alive) else { return false };
        self.deployables.iter().any(|e| e.team == p.team && e.health > 0.0 && e.kind.behavior() == Behavior::Command
            && e.center().distance(p.pos) <= deploy::AMMO_REACH + 0.5)
    }

    /// What a command station looks through for `team`: its cameras and
    /// turrets (deployable indices).
    pub fn viewable(&self, team: Team) -> Vec<usize> {
        self.deployables.iter().enumerate()
            .filter(|(_, e)| e.team == team && e.health > 0.0 && (e.kind.def().camera || e.kind.behavior() == Behavior::Turret))
            .map(|(k, _)| k).collect()
    }

    /// The team turret player `i` operates from a command station, if any.
    pub fn operated_by(&self, i: usize) -> Option<usize> {
        let k = (self.input_of(i)?.operate as usize).checked_sub(1)?;
        let e = self.deployables.get(k)?;
        (e.team == self.players[i].team && e.kind.behavior() == Behavior::Turret && self.at_command(i)).then_some(k)
    }

    /// A turret operated from a command station: it aims where its operator
    /// looks and fires while they hold fire (`deploy::step_deployables`).
    /// Whether turret `k` is operated.
    pub(super) fn operate_turret(&mut self, k: usize) -> bool {
        let Some(i) = (0..self.players.len()).find(|&i| self.operated_by(i) == Some(k)) else { return false };
        let Some(profile) = self.deployables[k].kind.turret() else { return false };
        let p = &self.players[i];
        let aim = look_dir(p.yaw, p.pitch);
        let fire = self.input_of(i).is_some_and(|c| c.fire);
        let e = &mut self.deployables[k];
        e.aim = aim;
        if fire && profile.fires && e.cooldown <= 0.0 && self.discs.len() < 256 {
            let muzzle = e.center() + aim * (deploy::TURRET_RADIUS + 0.4);
            let team = e.team;
            e.cooldown = profile.cooldown;
            // The operator's own shot: they're credited with what it does.
            self.discs.push(Disc { pos: muzzle, vel: aim * profile.speed, team, owner: i, life: profile.life,
                kind: profile.projectile, spin: 0.0, target: 0, power: 1.0 });
        }
        true
    }

    /// Waypoints set at command stations this tick.
    pub(super) fn step_waypoints(&mut self) {
        if self.predicting { return; }
        for i in 0..self.players.len() {
            let Some(at) = self.input_of(i).and_then(|c| c.waypoint) else { continue };
            if !at.is_finite() || !self.at_command(i) { continue; }
            let size = self.map_size();
            let at = Vec3::new(at.x.clamp(0.0, size), at.y.clamp(-100.0, 2000.0), at.z.clamp(0.0, size));
            self.waypoints[self.players[i].team.idx()] = Some(at);
            if i == self.player_id { self.push_event("point"); }
        }
    }

    /// The teleporter linked to deployable `k`: its team's units of its kind
    /// pair up in the order they were placed (first with second, third with
    /// fourth); an odd one out has none.
    pub fn partner(&self, k: usize) -> Option<usize> {
        let e = self.deployables.get(k)?;
        let same: Vec<usize> = self.deployables.iter().enumerate()
            .filter(|(_, x)| x.team == e.team && x.kind == e.kind).map(|(n, _)| n).collect();
        let at = same.iter().position(|&n| n == k)?;
        same.get(at ^ 1).copied()
    }

    /// Recall (`registry::Action::recall`) for player `i`: go to the `kind`
    /// they placed (it's used up; a flag carrier drops the flag first), or
    /// put one down where they aim. Server only.
    pub(super) fn recall(&mut self, i: usize, kind: DeployKind) {
        let owner = (i + 1).min(u8::MAX as usize) as u8;
        if let Some(k) = self.deployables.iter().position(|e| e.owner == owner && e.kind == kind) {
            let e = self.deployables.remove(k);
            if self.players[i].carrying.is_some() { self.drop_carried_flag(i); }
            let p = &mut self.players[i];
            p.pos = e.pos + Vec3::Y * (PLAYER_RADIUS + 0.1);
            p.vel = Vec3::ZERO;
            if i == self.player_id { self.push_event("deploy"); }
            return;
        }
        match self.place(i, kind, 0.0) {
            Ok(e) => { self.count_use(deploy::Used::Placed, e.team, e.kind.0); self.deployables.push(e); }
            Err(why) => if i == self.player_id { self.msg(why, 1.6); },
        }
    }

    /// The local player's alarms: a message and a cue when an enemy comes
    /// within one of their team's alarm sensors. Every machine, for its own
    /// player.
    pub(super) fn step_alarms(&mut self) {
        let Some(me) = self.players.get(self.player_id) else { return };
        let team = me.team;
        if !self.deployables.iter().any(|e| e.team == team && e.kind.def().sensor.as_ref().is_some_and(|s| s.alarm)) {
            self.alarmed = 0;
            return;
        }
        let mut tripped = 0u64;
        for (k, e) in self.deployables.iter().enumerate().filter(|(_, e)| e.team == team).take(64) {
            let Some(s) = e.kind.def().sensor.as_ref().filter(|s| s.alarm) else { continue };
            let near = self.players.iter().enumerate().any(|(j, p)| p.alive && !p.hidden() && self.hostile_team(team, j)
                && !status::sensor_hidden(&p.effects) && p.pos.distance(e.center()) <= s.range && (!s.moving || p.vel.length() > MOVING));
            if near { tripped |= 1 << k; }
        }
        if tripped & !self.alarmed != 0 {
            self.msg("SENSOR ALARM", 1.6);
            self.push_event("alarm");
        }
        self.alarmed = tripped;
    }
}

/// Where a deployable's look and its behavior's default meet: the base look
/// it's drawn with.
pub fn look(kind: DeployKind) -> crate::registry::DeployLook {
    use crate::registry::DeployLook as L;
    let d = kind.def();
    d.look.unwrap_or(match d.behavior {
        Behavior::Turret => L::Turret, Behavior::Wall => L::Wall, Behavior::Field => L::Field, Behavior::Ammo => L::Ammo,
        Behavior::Inventory => L::Inventory, Behavior::Post if d.sensor.is_some() => L::Sensor,
        Behavior::Post if d.camera => L::Camera, Behavior::Post => L::Pylon, Behavior::Pad => L::Pad, Behavior::Gate => L::Gate,
        Behavior::Command => L::Command,
    })
}
