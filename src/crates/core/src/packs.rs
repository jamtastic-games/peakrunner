//! Packs (`registry::Pack`): what a player wears on the back, one at a time
//! (`Player::pack`). The deploy key uses it. A pack that places (every base
//! game pack) puts its deployable down where the player aims and is used up
//! (`deploy`). Others work whenever worn, are switched on and off, or are
//! used for an action (`registry::Action`, shared with armors' class keys).
//! While a pack works it holds its effects on and drains energy (its own
//! `drain`, plus `drain_regen` times its wearer's recharge); it may let its
//! wearer carry more rounds.
//!
//! An armor's class key (`registry::Ability`) works the same way, switched
//! or used, and builds strain that ebbs away; too much gives its strain
//! effect. Armors may also touch (mend teammates, drain enemies) and carry
//! mounts: weapons that fire by themselves and plates that stop shots from
//! the front.
//!
//! Switching a pack or the class key, energy, effects, strain and the parts
//! of an action that move a body are predicted by every client (they run in
//! `step_players`); firing, throwing, placing, touches and mounted weapons
//! are the server's.
use super::*;
use crate::registry::{Action, Pack};
pub use crate::registry::PackUse;

/// Most mounts an armor may have.
pub const MAX_MOUNTS: usize = 4;

/// Which pack a player wears: a pack entry's wire number. Its `Debug` form
/// is the entry's id, capitalized ("Wall").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackKind(pub u8);

#[allow(non_upper_case_globals)]
impl PackKind {
    /// The base game's packs (fixed wire numbers): each places its deployable.
    pub const Turret: PackKind = PackKind(0);
    pub const Wall: PackKind = PackKind(1);
    pub const Field: PackKind = PackKind(2);
    pub const Ammo: PackKind = PackKind(3);
    pub const Inventory: PackKind = PackKind(4);
    /// Every pack in the session.
    pub fn all() -> impl Iterator<Item = PackKind> { crate::registry::registry().packs.iter().map(|(k, _, _)| PackKind(k as u8)) }
    /// Its entry (the base wall pack's, for a number no entry has).
    pub fn def(self) -> &'static Pack {
        let r = crate::registry::registry();
        r.packs.get(self.0 as usize).or_else(|| r.packs.get(1)).expect("the base game has a wall pack")
    }
    /// Its id, as saved favorites name it.
    pub fn key(self) -> &'static str { crate::registry::registry().packs.id(self.0 as usize).unwrap_or("wall") }
    pub fn from_key(key: &str) -> Option<Self> { crate::registry::registry().packs.index(key).map(|k| PackKind(k as u8)) }
    pub fn name(self) -> &'static str { self.def().name.as_str() }
    /// The deployable it places, if it is one that places.
    pub fn deployable(self) -> Option<deploy::DeployKind> {
        let d = self.def();
        (d.use_ == PackUse::Place).then(|| d.deployable.as_deref().and_then(deploy::DeployKind::from_key)).flatten()
    }
    /// Whether an entry has this number.
    pub fn exists(self) -> bool { crate::registry::registry().packs.get(self.0 as usize).is_some() }
    /// Whether `armor` may wear it (`armors/<id>.packs`; none listed: any).
    pub fn fits(self, armor: ArmorClass) -> bool {
        let list = &armor.def().packs;
        list.is_empty() || list.iter().any(|p| p == self.key())
    }
}

impl std::fmt::Debug for PackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&super::loadout::capitalized(self.key())) }
}

impl World {
    /// Player `i`'s pack this tick, with the deploy key `pressed`: switch a
    /// toggled one, use a used one, and while it works hold its effects on
    /// and drain its energy.
    pub(super) fn step_pack(&mut self, i: usize, pressed: bool, dt: f32) {
        let p = &mut self.players[i];
        p.pack_clock = (p.pack_clock - dt).max(0.0);
        let Some(kind) = p.pack.filter(|_| p.alive) else { p.pack_on = false; return };
        let def = kind.def();
        let works = match def.use_ {
            PackUse::Place => false,
            PackUse::Always => true,
            PackUse::Toggle => {
                if pressed { p.pack_on = !p.pack_on && p.energy >= def.min_energy; }
                p.pack_on
            }
            PackUse::Use => {
                let spent = def.charges > 0 && p.pack_uses >= def.charges;
                if pressed && p.pack_clock <= 0.0 && !spent && p.energy >= def.energy {
                    p.energy -= def.energy;
                    p.pack_clock = def.cooldown;
                    p.pack_uses = p.pack_uses.saturating_add(1);
                    if let Some(a) = &def.action { self.act(i, a); }
                    if self.hooks.live { self.hook(def.hook.as_deref(), super::hooks::HookEvent::Use { player: i, what: kind.key().to_string() }); }
                }
                false
            }
        };
        if !works { return; }
        // Its own drain, plus a share of what the wearer's armor recharges
        // (a shield or jammer outruns the recharge, so the tank empties
        // slowly even at rest).
        let drain = if def.drain_regen > 0.0 { def.drain + def.drain_regen * self.armor_of(i).regen } else { def.drain };
        let p = &mut self.players[i];
        if p.energy < def.min_energy.max(drain * dt) && drain > 0.0 {
            // Out of energy: it stops (a switched one switches off).
            p.pack_on = false;
            return;
        }
        p.energy = (p.energy - drain * dt).max(0.0);
        let effects = &crate::registry::registry().effects;
        for e in &def.effects {
            if let Some(id) = effects.index(e) { self.hold_effect(i, id as u8); }
        }
    }

    /// Player `i`'s class key this tick, with the key `pressed`: switch it,
    /// or use it; strain ebbs, and too much gives the strain effect.
    pub(super) fn step_ability(&mut self, i: usize, pressed: bool, dt: f32) {
        let p = &mut self.players[i];
        p.ability_clock = (p.ability_clock - dt).max(0.0);
        let Some(a) = p.armor.def().ability.as_ref().filter(|_| p.alive) else { p.ability_on = false; return };
        p.strain = (p.strain - a.strain_decay * dt).max(0.0);
        match a.use_ {
            PackUse::Toggle => {
                if pressed { p.ability_on = !p.ability_on && p.energy >= a.min_energy; }
                if p.ability_on && a.drain > 0.0 && p.energy < a.min_energy.max(a.drain * dt) { p.ability_on = false; }
                if p.ability_on {
                    p.energy = (p.energy - a.drain * dt).max(0.0);
                    p.strain += a.strain * dt;
                    let effects = &crate::registry::registry().effects;
                    for e in &a.effects {
                        if let Some(id) = effects.index(e) { self.hold_effect(i, id as u8); }
                    }
                }
            }
            PackUse::Use => {
                let spent = a.charges > 0 && p.ability_uses >= a.charges;
                if pressed && p.ability_clock <= 0.0 && !spent && p.energy >= a.energy {
                    p.energy -= a.energy;
                    p.ability_clock = a.cooldown;
                    p.ability_uses = p.ability_uses.saturating_add(1);
                    p.strain += a.strain;
                    if let Some(x) = &a.action { self.act(i, x); }
                    if self.hooks.live { self.hook(self.armor_hook(i), super::hooks::HookEvent::Use { player: i, what: "ability".into() }); }
                }
            }
            PackUse::Place | PackUse::Always => {}
        }
        let p = &self.players[i];
        if a.strain_limit > 0.0 && p.strain >= a.strain_limit {
            if let Some(e) = a.strain_effect.as_deref().and_then(|e| crate::registry::registry().effects.index(e)) { self.give_effect(i, e as u8); }
        }
    }

    /// Effects that firing or a hit ended switch off the pack or class key
    /// holding them on.
    pub(super) fn switch_off_revealed(&mut self, i: usize) {
        let r = crate::registry::registry();
        let reveals = |ids: &[String]| ids.iter().any(|e| r.effects.by_id(e).is_some_and(|e| e.reveal));
        let p = &mut self.players[i];
        if p.pack_on && p.pack.is_some_and(|k| reveals(&k.def().effects)) { p.pack_on = false; }
        if p.ability_on && p.armor.def().ability.as_ref().is_some_and(|a| reveals(&a.effects)) { p.ability_on = false; }
    }

    /// Player `a` touching player `b` (`bump`): what `a`'s armor's touch
    /// does, at most once a cooldown per pair. Server only.
    pub(super) fn touch(&mut self, a: usize, b: usize) {
        if self.predicting || !self.players[a].alive || !self.players[b].alive { return; }
        let Some(t) = self.players[a].armor.def().touch.as_ref() else { return };
        let pair = (a as u8, b as u8);
        if self.touches.iter().any(|(x, y, _)| (*x, *y) == pair) { return; }
        self.touches.push((pair.0, pair.1, t.cooldown));
        if self.hooks.live { self.hook(self.armor_hook(a), super::hooks::HookEvent::Touch { toucher: a, touched: b }); }
        self.hit_by = Some(a);
        let effects = &crate::registry::registry().effects;
        if !self.hostile(a, self.players[a].team, b) {
            let max = loadout::energy_max(self.players[b].armor);
            let q = &mut self.players[b];
            q.health = (q.health + t.heal).min(100.0);
            q.energy = (q.energy + t.energy).min(max);
            if let Some(e) = t.ally_effect.as_deref().and_then(|e| effects.index(e)) { self.give_effect(b, e as u8); }
            self.hit_by = None;
            return;
        }
        if t.steal_energy > 0.0 {
            let took = self.players[b].energy * t.steal_energy;
            self.players[b].energy -= took;
            let max = loadout::energy_max(self.players[a].armor);
            self.players[a].energy = (self.players[a].energy + took).min(max);
        }
        if let Some(e) = t.effect.as_deref().and_then(|e| effects.index(e)) { self.give_effect(b, e as u8); }
        if t.damage > 0.0 {
            self.hurt(b, t.damage, &t.damage_kind);
            if self.players[b].health <= 0.0 && self.players[b].alive {
                let name = self.players[a].armor.def().name.as_str();
                self.kill(b, Some(a), name);
            }
        }
        self.hit_by = None;
    }

    /// Touch cooldowns run down.
    pub(super) fn step_touches(&mut self, dt: f32) {
        if self.touches.is_empty() { return; }
        for t in &mut self.touches { t.2 -= dt; }
        self.touches.retain(|t| t.2 > 0.0);
    }

    /// The share of a hit travelling along `dir` that reaches player `i`
    /// past the plates on their front (1: none in the way).
    pub fn blocked_share(&self, i: usize, dir: Vec3) -> f32 {
        let Some(p) = self.players.get(i) else { return 1.0 };
        let mounts = &p.armor.def().mounts;
        if mounts.is_empty() { return 1.0; }
        let facing = Vec3::new(-p.yaw.sin(), 0.0, -p.yaw.cos());
        let toward = -Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        mounts.values().filter_map(|m| m.blocker)
            .filter(|b| toward.dot(facing) >= b.arc.to_radians().cos())
            .fold(1.0, |share, b| share.min(b.share))
    }

    /// Where mount `at` sits on player `i`.
    pub fn mount_point(&self, i: usize, at: crate::registry::MountAt) -> Vec3 {
        let p = &self.players[i];
        let (fwd, right) = move_basis(p.yaw);
        match at {
            crate::registry::MountAt::Shoulder => p.pos + Vec3::Y * (EYE - 0.15) + right * 0.45,
            crate::registry::MountAt::Back => p.pos + Vec3::Y * 1.1 - fwd * 0.45,
            crate::registry::MountAt::Front => p.pos + Vec3::Y * 0.9 + fwd * 0.55,
            crate::registry::MountAt::Belt => p.pos + Vec3::Y * 0.6 + right * 0.4,
        }
    }

    /// Mounted weapons fire by themselves at the nearest enemy in sight
    /// (turret targeting). Server only.
    pub(super) fn step_mounts(&mut self, dt: f32) {
        if self.predicting { return; }
        let r = crate::registry::registry();
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive || p.armor.def().mounts.is_empty() { continue; }
            let team = p.team;
            for (k, m) in p.armor.def().mounts.values().enumerate().take(MAX_MOUNTS) {
                let clock = &mut self.players[i].mount_clocks[k];
                *clock = (*clock - dt).max(0.0);
                if *clock > 0.0 { continue; }
                let Some(weapon) = m.weapon.as_deref().and_then(|w| r.weapons.by_id(w)) else { continue };
                let Some(kind) = r.projectiles.index(&weapon.projectile) else { continue };
                let Some(round) = r.projectiles.get(kind) else { continue };
                if matches!(round.class, crate::registry::ProjectileClass::Beam | crate::registry::ProjectileClass::Stream) { continue; }
                let profile = crate::equipment::TurretProfile { range: m.range, sensed_range: m.range, speed: round.speed, life: round.life,
                    cooldown: m.cooldown.unwrap_or(weapon.reload), leads: true, fires: true, projectile: kind as u8, jetting_only: false };
                let origin = self.mount_point(i, m.at);
                let own = team.idx() as u8;
                let candidates: Vec<crate::equipment::Candidate> = self.players.iter().enumerate()
                    .filter(|(j, q)| *j != i && q.alive && !self.sensor_hidden(*j))
                    .map(|(index, q)| crate::equipment::Candidate { index, team: if self.hostile(i, team, index) { own ^ 1 } else { own }, pos: q.pos, vel: q.vel, jetting: q.jetting })
                    .collect();
                let clear = |a: Vec3, b: Vec3| {
                    obstacle_hit(self.map, &self.pillars, a, b, 0.0).is_none()
                        && self.deployables.iter().all(|e| !e.blocks(Some(team)) || e.hit(a, b, 0.0).is_none())
                };
                let Some(hit) = crate::equipment::acquire_target(origin, 0.2, own, &profile, false, candidates, clear) else { continue };
                if self.discs.len() >= 256 { continue; }
                self.discs.push(Disc { pos: hit.muzzle, vel: hit.aim * round.speed + self.players[i].vel * round.inherit, team, owner: i,
                    life: round.life, kind: kind as u8, spin: 0.0, target: 0, power: 1.0 });
                self.players[i].mount_clocks[k] = profile.cooldown;
                if self.network_inputs.is_empty() && i != self.player_id { self.spatial_sounds.push((weapon.sound.as_str(), hit.muzzle)); }
                else if i == self.player_id { self.push_event(&weapon.sound); }
            }
        }
    }

    /// A bot's presses for its pack and class key by their hints
    /// (`registry::BotUse`): in a fight (an enemy `seen`), defending (by
    /// its flag), or always. Switched ones are switched to match; used ones
    /// are used when ready; a pack that places is put down defending.
    pub(super) fn bot_uses(&mut self, i: usize, seen: bool, defending: bool) {
        use crate::registry::BotUse;
        let wants = |hint: Option<BotUse>| match hint { Some(BotUse::Combat) => seen, Some(BotUse::Defense) => defending, Some(BotUse::Always) => true, None => false };
        let p = &self.players[i];
        let mut press = p.bot_press;
        if let Some(k) = p.pack {
            let d = k.def();
            let want = wants(d.bot);
            match d.use_ {
                PackUse::Toggle if want != p.pack_on && d.bot.is_some() => press |= 1,
                PackUse::Use if want && p.pack_clock <= 0.0 => press |= 1,
                PackUse::Place if want => press |= 4,
                _ => {}
            }
        }
        if let Some(a) = p.armor.def().ability.as_ref() {
            let want = wants(a.bot);
            match a.use_ {
                PackUse::Toggle if want != p.ability_on && a.bot.is_some() => press |= 2,
                PackUse::Use if want && p.ability_clock <= 0.0 => press |= 2,
                _ => {}
            }
        }
        self.players[i].bot_press = press;
    }

    /// Do `action` for player `i` (a pack's or a class key's): the parts
    /// that move a body or change what they wear here (predicted); firing,
    /// throwing and placing on the server.
    pub(super) fn act(&mut self, i: usize, a: &Action) {
        let r = crate::registry::registry();
        if let Some([forward, up]) = a.impulse {
            let p = &mut self.players[i];
            let mass = loadout::mass_scale(p.armor).max(0.1);
            let ahead = look_dir(p.yaw, p.pitch.max(0.0));
            p.vel += ahead * (forward / mass) + Vec3::Y * (up / mass);
            p.on_ground = false;
            p.skiing = false;
            p.coyote = 0.0;
        }
        if let Some(e) = a.effect.as_deref().and_then(|e| r.effects.index(e)) { self.give_effect(i, e as u8); }
        if let Some(class) = a.armor.as_deref().and_then(ArmorClass::by_id) {
            let p = &mut self.players[i];
            if p.swap_back == 0 { p.swap_back = p.armor.0 + 1; }
            p.armor = class;
            p.swap_clock = a.seconds;
        }
        if self.predicting { return; }
        if let Some(w) = a.fire.as_deref().and_then(|w| r.weapons.index(w)) { self.fire_from(i, w as u8); }
        if let Some(t) = a.throw.as_deref().and_then(|t| r.throwables.index(t)) {
            let _ = self.throw_kind(i, t as u8, 1.0);
        }
        if let Some(kind) = a.recall.as_deref().and_then(deploy::DeployKind::from_key) { self.recall(i, kind); }
        if let Some(kind) = a.place.as_deref().and_then(deploy::DeployKind::from_key) {
            match self.place(i, kind, 0.0) {
                Ok(e) => { self.count_use(deploy::Used::Placed, e.team, e.kind.0); self.deployables.push(e); }
                Err(why) => if i == self.player_id { self.msg(why, 1.6); },
            }
        }
    }

    /// A borrowed armor's time runs out: back to the one worn before.
    pub(super) fn step_swap(&mut self, i: usize, dt: f32) {
        let p = &mut self.players[i];
        if p.swap_back == 0 { return; }
        p.swap_clock -= dt;
        if p.swap_clock <= 0.0 || !p.alive {
            p.armor = ArmorClass(p.swap_back - 1);
            p.swap_back = 0;
            p.swap_clock = 0.0;
            p.energy = p.energy.min(loadout::energy_max(p.armor));
        }
    }

    /// Fire weapon `w`'s projectile for player `i` from the shoulder at the
    /// crosshair, apart from what they hold (a pack's or class key's shot).
    pub(super) fn fire_from(&mut self, i: usize, w: u8) {
        let r = crate::registry::registry();
        let Some(weapon) = r.weapons.get(w as usize) else { return };
        let Some(kind) = r.projectiles.index(&weapon.projectile) else { return };
        let Some(round) = r.projectiles.get(kind) else { return };
        if matches!(round.class, crate::registry::ProjectileClass::Beam | crate::registry::ProjectileClass::Stream) { return; }
        let p = &self.players[i];
        let dir = look_dir(p.yaw, p.pitch);
        let (right, _) = view_basis(dir);
        let eye = p.pos + Vec3::Y * EYE;
        let want = eye + right * 0.45 + Vec3::Y * 0.25;
        let origin = match obstacle_hit(self.map, &self.pillars, eye, want, SHOT_RADIUS) {
            Some(t) => eye.lerp(want, (t - 0.1).max(0.0)),
            None => want,
        };
        let shot = (eye + dir * weapon.aim - origin).normalize_or(dir);
        let target = round.seek.as_ref().map_or(0, |s| self.seek_target(i, s, origin, shot));
        let p = &self.players[i];
        let vel = shot * round.speed + p.vel * round.inherit;
        self.discs.push(Disc { pos: origin, vel, team: p.team, owner: i, life: round.life, kind: kind as u8, spin: 0.0, target, power: 1.0 });
        if i == self.player_id { self.push_event(&weapon.sound); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push((weapon.sound.as_str(), origin)); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One player in `armor` alone in Old Holler's CTF with a full tank,
    /// wearing `pack` switched on.
    fn wearing(armor: &str, pack: &str) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(1);
        let class = ArmorClass::by_id(armor).unwrap();
        let p = &mut w.players[0];
        (p.armor, p.pack, p.pack_on) = (class, PackKind::from_key(pack), true);
        p.energy = loadout::energy_max(class);
        w
    }

    #[test]
    fn a_shield_or_jammer_outruns_the_recharge_even_at_rest() {
        for armor in ["light", "medium", "heavy"] {
            for (pack, share) in [("shield", 1.125), ("jammer", 1.3125)] {
                let mut w = wearing(armor, pack);
                let regen = w.armor_of(0).regen;
                let start = w.players[0].energy;
                for _ in 0..600 { w.step_players(STEP); }
                let lost = start - w.players[0].energy;
                assert!((lost - (share - 1.0) * regen * 10.0).abs() < 0.05, "{armor} {pack}: lost {lost} in 10 s");
                assert!(w.players[0].pack_on && w.players[0].effects.len() == 1, "{armor} {pack}: still on");
                // Left on, it empties the tank down to its minimum and
                // switches off.
                let mut on = 10.0;
                while w.players[0].pack_on && on < 300.0 { w.step_players(STEP); on += STEP; }
                let empty = (start - 2.0) / ((share - 1.0) * regen);
                assert!((on - empty).abs() < 0.5, "{armor} {pack}: off after {on} s, not {empty}");
            }
        }
    }

    #[test]
    fn a_shield_pays_the_same_for_a_hit_whatever_the_armor() {
        for armor in ["light", "medium", "heavy"] {
            let mut w = wearing(armor, "shield");
            w.step_players(STEP);
            let (health, energy) = (w.players[0].health, w.players[0].energy);
            w.hurt(0, 20.0, "blast");
            assert_eq!(w.players[0].health, health, "{armor}: the shield took it all");
            assert!((energy - w.players[0].energy - 10.0).abs() < 1e-3, "{armor}: half an energy a point, {}", energy - w.players[0].energy);
        }
    }
}
