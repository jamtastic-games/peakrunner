//! What lies on the ground (`loadout::Loot`) beyond the base game's death
//! drops of rounds and grenades: items (`registry::Item`), weapons and packs
//! that players drop (the drop keys) or leave when they die (the mode's
//! `death_drops`), for whoever runs over them. And a mode's economy
//! (`modes::Economy`): credits earned by the minute, by frags, captures and
//! returns, and spent on armors, weapons and packs at their `price`.
//! All of it is the server's; snapshots carry the loot and the credits.
use super::*;

/// What the drop key drops.
pub const DROP_NONE: u8 = 0;
pub const DROP_WEAPON: u8 = 1;
pub const DROP_PACK: u8 = 2;

/// Seconds before whoever dropped something can take it back.
pub const OWN_DROP_WAIT: f32 = 1.5;
/// Most loot lying about at once (the oldest goes first).
pub const LOOT_MAX: usize = 32;
/// A body moving slower than this on the ground has come to rest: its
/// drops lie on the ground at once.
const BODY_SETTLED: f32 = 1.0;

impl World {
    /// The drop keys this tick (`Input::drop`).
    pub(super) fn step_drops(&mut self) {
        if self.predicting || !self.mode.def().player_drops { return; }
        for i in 0..self.players.len() {
            let what = if self.players[i].is_bot { DROP_NONE }
                else if self.network_inputs.is_empty() { if i == self.player_id { self.input.drop } else { DROP_NONE } }
                else { self.network_inputs.get(i).map_or(DROP_NONE, |c| c.drop) };
            if what == DROP_NONE || !self.players[i].alive { continue; }
            match what {
                DROP_WEAPON => self.drop_weapon(i),
                DROP_PACK => self.drop_pack(i),
                _ => {}
            }
        }
    }

    /// Where something player `i` lets go of lands: on the ground a step
    /// ahead of them, `n` of a few spread around. A dead body's drops ride
    /// it down instead (`body_drop`).
    fn drop_spot(&self, i: usize, n: usize) -> Vec3 {
        let p = &self.players[i];
        let a = p.yaw + n as f32 * 2.1;
        let want = p.pos + Vec3::new(-a.sin(), 0.0, -a.cos()) * 1.4;
        let at = if obstacle_hit(self.map, &self.pillars, p.pos, want, 0.2).is_some() { p.pos } else { want };
        Vec3::new(at.x, crate::terrain::support_on(self.map, at + Vec3::Y * 0.5).0, at.z)
    }

    /// Where a dead player `i`'s drop starts: riding the body while it is
    /// still falling (carried, `owner` the body), else on the ground under
    /// it, `n` of a few spread around.
    pub(super) fn body_drop(&self, i: usize, n: usize) -> (Vec3, bool, u8) {
        let p = &self.players[i];
        let owner = (i + 1).min(u8::MAX as usize) as u8;
        let floor = crate::terrain::support_on(self.map, p.pos + Vec3::Y * 0.5).0;
        let resting = p.on_ground && p.vel.length() < BODY_SETTLED && p.pos.y - PLAYER_RADIUS - floor < 0.3;
        if resting { (self.settle_spot(i, p.pos, n), false, owner) } else { (p.pos - Vec3::Y * PLAYER_RADIUS, true, owner) }
    }

    /// On the ground by body `i` at `at`: the first drop under it, the
    /// rest spread a little around, never through a wall.
    fn settle_spot(&self, i: usize, at: Vec3, n: usize) -> Vec3 {
        let a = self.players[i].yaw + n as f32 * 2.1;
        let want = if n == 0 { at } else { at + Vec3::new(-a.sin(), 0.0, -a.cos()) * 0.8 };
        let at = if obstacle_hit(self.map, &self.pillars, at, want, 0.2).is_some() { at } else { want };
        Vec3::new(at.x, crate::terrain::support_on(self.map, at + Vec3::Y * 0.5).0, at.z)
    }

    /// Drops riding a falling body move with it; once it lies still (or is
    /// gone: respawned, or its player left), they come to rest on the
    /// ground there.
    pub(super) fn step_carried_loot(&mut self) {
        let mut settled: Vec<usize> = Vec::new();
        for k in 0..self.loot.len() {
            let Some(i) = self.loot[k].rider_of() else { continue };
            // A slot its player left is emptied (`Match::vacate`): no body.
            let body = self.players.get(i).filter(|p| !p.alive && !(p.net_id == 0 && p.remote) && !p.hidden());
            match body {
                Some(p) if !(p.on_ground && p.vel == Vec3::ZERO) => self.loot[k].pos = p.pos - Vec3::Y * PLAYER_RADIUS,
                _ => {
                    let n = settled.iter().filter(|&&j| j == i).count();
                    settled.push(i);
                    let at = self.loot[k].pos + Vec3::Y * PLAYER_RADIUS;
                    self.loot[k].pos = if i < self.players.len() { self.settle_spot(i, at, n) } else { at };
                    self.loot[k].carried = false;
                }
            }
        }
    }

    /// Where player `i` lets go of something: a step ahead of the living
    /// (`drop_spot`), with a dead body (`body_drop`, `n` of its drops).
    fn letting_go(&self, i: usize, n: usize) -> (Vec3, bool) {
        if self.players[i].alive { (self.drop_spot(i, n - 1), false) } else { let (at, carried, _) = self.body_drop(i, n); (at, carried) }
    }

    fn put_loot(&mut self, l: loadout::Loot) {
        self.loot.push(l);
        if self.loot.len() > LOOT_MAX { self.loot.remove(0); }
    }

    /// Player `i` drops the weapon in their hand, with its rounds.
    pub(super) fn drop_weapon(&mut self, i: usize) {
        let w = self.players[i].weapon;
        if !self.players[i].loadout.carries(w) { return; }
        let (pos, carried) = self.letting_go(i, 1);
        let p = &mut self.players[i];
        p.loadout.remove(w);
        let mut ammo = vec![0u16; (w as usize + 1).max(p.ammo.len())];
        if let Some(n) = p.ammo.get_mut(w as usize) { ammo[w as usize] = *n; *n = 0; }
        p.weapon = p.loadout.kinds().next().unwrap_or(w);
        let owner = (i + 1).min(u8::MAX as usize) as u8;
        self.put_loot(loadout::Loot { pos, ammo, weapon: w + 1, owner, carried, ..Default::default() });
        if i == self.player_id { self.push_event("throw"); }
    }

    /// Player `i` drops the pack they wear.
    pub(super) fn drop_pack(&mut self, i: usize) {
        let Some(kind) = self.players[i].pack else { return };
        let (pos, carried) = self.letting_go(i, 2);
        let p = &mut self.players[i];
        p.pack = None;
        p.pack_on = false;
        let owner = (i + 1).min(u8::MAX as usize) as u8;
        self.put_loot(loadout::Loot { pos, pack: kind.0 + 1, owner, carried, ..Default::default() });
        if i == self.player_id { self.push_event("throw"); }
    }

    /// What player `i`'s death leaves besides its rounds (`modes::DeathDrops`),
    /// before their pack and weapons are cleared.
    pub(super) fn death_drops(&mut self, i: usize) {
        if self.predicting { return; }
        let d = self.mode.def().death_drops.clone();
        if d.pack && self.players[i].pack.is_some() { self.drop_pack(i); }
        if d.weapon && !self.players[i].loadout.is_empty() { self.drop_weapon(i); }
        let items = &crate::registry::registry().items;
        for (n, id) in d.items.iter().enumerate() {
            let Some(k) = items.index(id) else { continue };
            let (pos, carried, owner) = self.body_drop(i, n + 3);
            self.put_loot(loadout::Loot { pos, item: k as u8 + 1, carried, owner, ..Default::default() });
        }
    }

    /// Player `i` runs over loot `k`: they take the item, weapon and pack
    /// it holds where they can (its rounds and grenades are taken in
    /// `step_loot`). Whether they took anything.
    pub(super) fn take_extras(&mut self, i: usize, k: usize) -> bool {
        let l = &self.loot[k];
        let (item, weapon, pack) = (l.item, l.weapon, l.pack);
        let mut took = false;
        if item > 0 && self.take_item(i, item - 1) { self.loot[k].item = 0; took = true; }
        if weapon > 0 {
            let w = weapon - 1;
            let p = &self.players[i];
            let fits = arsenal::carries(p.armor, w) && self.mode.def().allows(w) && !p.loadout.carries(w);
            if fits {
                let slots = arsenal::slots(self.players[i].armor);
                if self.players[i].loadout.take(w, slots) {
                    let max = loadout::rounds_max(&self.players[i]).get(w as usize).copied().unwrap_or(0);
                    let rounds = self.loot[k].ammo.get(w as usize).copied().unwrap_or(0);
                    let p = &mut self.players[i];
                    if p.ammo.len() <= w as usize { p.ammo.resize(w as usize + 1, 0); }
                    p.ammo[w as usize] = p.ammo[w as usize].saturating_add(rounds).min(max);
                    if let Some(n) = self.loot[k].ammo.get_mut(w as usize) { *n = 0; }
                    self.loot[k].weapon = 0;
                    took = true;
                }
            }
        }
        if pack > 0 && self.players[i].pack.is_none() {
            let kind = packs::PackKind(pack - 1);
            if kind.exists() && (self.mode.def().deployables || kind.deployable().is_none()) {
                let p = &mut self.players[i];
                p.pack = Some(kind);
                (p.pack_on, p.pack_clock, p.pack_uses) = (false, 0.0, 0);
                self.loot[k].pack = 0;
                took = true;
            }
        }
        took
    }

    /// Item `id` for player `i`, if it gives them anything.
    pub(super) fn take_item(&mut self, i: usize, id: u8) -> bool {
        let Some(t) = crate::registry::registry().items.get(id as usize) else { return false };
        let max = loadout::rounds_max(&self.players[i]);
        let energy_max = loadout::energy_max(self.players[i].armor);
        let throws = throwables::max_throwables(self.players[i].armor);
        let p = &mut self.players[i];
        let mut gave = false;
        if t.health > 0.0 && p.health < 100.0 { p.health = (p.health + t.health).min(100.0); gave = true; }
        if t.energy > 0.0 && p.energy < energy_max { p.energy = (p.energy + t.energy).min(energy_max); gave = true; }
        if t.rounds > 0.0 {
            for w in p.loadout.kinds().collect::<Vec<_>>() {
                let full = max.get(w as usize).copied().unwrap_or(0);
                if full == 0 { continue; }
                if p.ammo.len() <= w as usize { p.ammo.resize(w as usize + 1, 0); }
                let have = p.ammo[w as usize];
                if have < full { p.ammo[w as usize] = (have + (full as f32 * t.rounds).ceil() as u16).min(full); gave = true; }
            }
        }
        for (slot, n) in [(0, t.grenades), (1, t.mines)] {
            if n > 0 && p.throwables[slot] < throws[slot] { p.throwables[slot] = (p.throwables[slot] + n).min(throws[slot]); gave = true; }
        }
        if let Some(e) = t.effect.as_deref().and_then(|e| crate::registry::registry().effects.index(e)) {
            gave |= self.give_effect(i, e as u8);
        }
        if gave && i == self.player_id { self.push_event("buy"); }
        if gave && self.hooks.live {
            let id_s = crate::registry::registry().items.id(id as usize).unwrap_or("").to_string();
            self.hook(t.hook.as_deref(), super::hooks::HookEvent::Pickup { player: i, item: id_s });
        }
        gave
    }

    /// What a round does to player `idx` it hit directly besides damage
    /// (`registry::Projectile`): its effect, and a drag toward (or away
    /// from) its shooter. Server only.
    pub(super) fn hit_extras(&mut self, idx: usize, owner: usize, round: &crate::registry::Projectile) {
        if self.predicting || !self.players.get(idx).is_some_and(|p| p.alive) { return; }
        if let Some(e) = round.effect.as_deref().and_then(|e| crate::registry::registry().effects.index(e)) { self.give_effect(idx, e as u8); }
        if round.pull != 0.0 && owner < self.players.len() && owner != idx && self.players[idx].riding == 0 {
            let to = (self.players[owner].pos - self.players[idx].pos).normalize_or_zero();
            let p = &mut self.players[idx];
            p.vel += to * round.pull;
            // A push (negative) also lifts them off their feet, so the
            // ground's grip doesn't swallow it.
            if round.pull < 0.0 { p.vel.y += -round.pull * 0.4; }
            p.on_ground = false;
        }
    }

    /// Drag player `i` toward `at` at `speed` (a grapple). Server only.
    pub(super) fn drag(&mut self, i: usize, at: Vec3, speed: f32) {
        if self.predicting || !self.players.get(i).is_some_and(|p| p.alive && p.riding == 0) { return; }
        let p = &mut self.players[i];
        let to = (at - p.pos).normalize_or_zero();
        p.vel += to * speed + Vec3::Y * speed * 0.15;
        p.on_ground = false;
        p.skiing = false;
    }

    /// Thrown rounds come to rest that become a deployable or an item
    /// (`registry::Projectile::becomes`, `drops`) do so.
    pub(super) fn settle_thrown(&mut self) {
        let r = crate::registry::registry();
        let mut k = 0;
        while k < self.discs.len() {
            let d = &self.discs[k];
            let round = r.projectiles.get(d.kind as usize);
            let settles = d.vel == Vec3::ZERO && round.is_some_and(|x| x.becomes.is_some() || x.drops.is_some());
            if !settles { k += 1; continue; }
            let d = self.discs.remove(k);
            let round = round.unwrap();
            let floor = crate::terrain::support_on(self.map, d.pos + Vec3::Y * 0.3).0;
            let at = Vec3::new(d.pos.x, floor, d.pos.z);
            if let Some(kind) = round.becomes.as_deref().and_then(deploy::DeployKind::from_key) {
                let yaw = self.players.get(d.owner).map_or(0.0, |p| p.yaw);
                self.deployables.push(deploy::Deployable { owner: (d.owner + 1).min(255) as u8, kind, team: d.team, pos: at, yaw,
                    health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
            }
            if let Some(item) = round.drops.as_deref().and_then(|t| r.items.index(t)) {
                self.put_loot(loadout::Loot { pos: at, item: item as u8 + 1, ..Default::default() });
            }
        }
    }

    // ---- The economy -------------------------------------------------------

    /// Player `i`'s credits to spend: their team's where the economy is
    /// shared, else their own (0 without an economy).
    pub fn credits_of(&self, i: usize) -> f32 {
        let Some(e) = self.mode.def().economy.as_ref() else { return 0.0 };
        let Some(p) = self.players.get(i) else { return 0.0 };
        if e.shared { self.credits[p.team.idx()] } else { p.credits }
    }

    /// Credits earned by player `i` (their team's where shared).
    pub(super) fn earn(&mut self, i: usize, n: u32) {
        let Some(e) = self.mode.def().economy.as_ref() else { return };
        if n == 0 || i >= self.players.len() || self.predicting { return; }
        let max = e.max as f32;
        if e.shared { let t = self.players[i].team.idx(); self.credits[t] = (self.credits[t] + n as f32).min(max); }
        else { self.players[i].credits = (self.players[i].credits + n as f32).min(max); }
    }

    /// Player `i` pays `n` credits: false (and nothing paid) when they
    /// can't. Always true without an economy.
    pub(super) fn pay(&mut self, i: usize, n: u32) -> bool {
        let Some(e) = self.mode.def().economy.as_ref() else { return true };
        if n == 0 { return true; }
        if self.credits_of(i) < n as f32 { return false; }
        if e.shared { let t = self.players[i].team.idx(); self.credits[t] -= n as f32; } else { self.players[i].credits -= n as f32; }
        true
    }

    /// What a setup costs: its armor, weapons and pack.
    pub fn setup_cost(&self, s: &Setup) -> u32 {
        let r = crate::registry::registry();
        let armor = s.armor.map_or(0, |a| a.def().price);
        let weapons: u32 = s.weapons.iter().flat_map(|l| l.kinds()).filter_map(|k| r.weapons.get(k as usize)).map(|w| w.price).sum();
        armor + weapons + s.pack.map_or(0, |k| k.def().price)
    }

    /// Income by the minute, and the credits a frag, a capture or a return
    /// earns (`modes::Economy`).
    pub(super) fn step_economy(&mut self, dt: f32) {
        if self.predicting { return; }
        let Some(e) = self.mode.def().economy.as_ref() else { return };
        if e.per_minute == 0 { return; }
        let gain = e.per_minute as f32 * dt / 60.0;
        let max = e.max as f32;
        if e.shared { for c in &mut self.credits { *c = (*c + gain).min(max); } }
        else { for p in &mut self.players { p.credits = (p.credits + gain).min(max); } }
    }

    /// The credits a player or team starts with.
    pub fn starting_credits(&self) -> f32 { self.mode.def().economy.as_ref().map_or(0.0, |e| e.start as f32) }

    /// Everyone back to the mode's starting credits (a new match or round).
    pub fn reset_credits(&mut self) {
        let start = self.starting_credits();
        self.credits = [start; 2];
        for p in &mut self.players { p.credits = start; }
    }

    pub(super) fn earn_for(&mut self, i: usize, what: Earned) {
        let Some(e) = self.mode.def().economy.as_ref() else { return };
        let n = match what { Earned::Frag => e.frag, Earned::Capture => e.capture, Earned::Return => e.returned };
        self.earn(i, n);
    }
}

/// What earns credits.
#[derive(Clone, Copy, Debug)]
pub(super) enum Earned { Frag, Capture, Return }
