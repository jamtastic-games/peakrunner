//! Status effects (`registry::Effect`): what a player has for a while, or
//! while a pack or ability holds it on. A player carries their effects and
//! the seconds each has left (`Player::effects`, in snapshots).
//!
//! What every client predicts, because it moves bodies or spends energy,
//! runs in `step_players` through `World::status_movement`: timers, movement
//! scales, energy gained or drained, regen, and what an effect blocks.
//! What only the server decides runs in `step_status`: damage and healing
//! over time. A shield pays damage from energy (`World::hurt`); firing or
//! being hit ends effects that `reveal`; stations cleanse those marked so.
use super::*;
use crate::registry::{Effect, Moves, Stack};
use super::hooks::HookEvent;

/// Most effects one player may carry at once.
pub const MAX_EFFECTS: usize = 8;

fn effect(id: u8) -> Option<&'static Effect> { crate::registry::registry().effects.get(id as usize) }

/// What a player's effects add up to for moving: movement multiples, the
/// regen multiple, energy a second, and what is blocked.
#[derive(Clone, Copy, Debug)]
pub struct Bearing { pub moves: Moves, pub regen: f32, pub energy: f32, pub no_fire: bool, pub no_jet: bool, pub no_flag: bool }

impl Default for Bearing {
    fn default() -> Self { Bearing { moves: Moves::default(), regen: 1.0, energy: 0.0, no_fire: false, no_jet: false, no_flag: false } }
}

/// What `effects` add up to.
pub fn bearing(effects: &[(u8, f32)]) -> Bearing {
    let mut b = Bearing::default();
    for (id, _) in effects {
        let Some(e) = effect(*id) else { continue };
        let m = &mut b.moves;
        m.jet *= e.movement.jet; m.side *= e.movement.side; m.air_cap *= e.movement.air_cap;
        m.walk *= e.movement.walk; m.jump *= e.movement.jump;
        b.regen *= e.regen_scale;
        b.energy += e.energy;
        b.no_fire |= e.blocks.fire;
        b.no_jet |= e.blocks.jet;
        b.no_flag |= e.blocks.flag;
    }
    b
}

/// How visible `effects` leave their bearer (1: plainly; the least of
/// theirs), whether they hide the name tag, and how near (m) an enemy must
/// be to see the shimmer of one nearly invisible.
pub fn visibility(effects: &[(u8, f32)]) -> f32 {
    effects.iter().filter_map(|(k, _)| effect(*k)).fold(1.0, |v, e| v.min(e.visibility))
}
pub fn hides_name(effects: &[(u8, f32)]) -> bool { effects.iter().any(|(k, _)| effect(*k).is_some_and(|e| e.hide_name)) }
pub fn shimmer(effects: &[(u8, f32)]) -> f32 { effects.iter().filter_map(|(k, _)| effect(*k)).fold(0.0, |s, e| s.max(e.shimmer)) }
/// Whether `effects` hide their bearer from sensors (and the minimap).
pub fn sensor_hidden(effects: &[(u8, f32)]) -> bool { effects.iter().any(|(k, _)| effect(*k).is_some_and(|e| e.sensor_hidden)) }

/// Below this visibility a player is left out of enemies' snapshots unless
/// one of them is within their shimmer range (`Match::snapshot_for`).
pub const UNSEEN: f32 = 0.5;

impl World {
    /// Give player `i` effect `id` (unless their armor is immune), as its
    /// `stack` says when they have it already. Returns whether it took.
    pub fn give_effect(&mut self, i: usize, id: u8) -> bool {
        let Some(e) = effect(id) else { return false };
        let Some(p) = self.players.get_mut(i) else { return false };
        if !p.alive || e.immune.iter().any(|a| a == p.armor.id()) { return false; }
        let full = p.effects.len() >= MAX_EFFECTS;
        match p.effects.iter_mut().find(|(k, _)| *k == id) {
            Some((_, left)) => match e.stack {
                Stack::Refresh => *left = left.max(e.seconds),
                Stack::Extend => *left += e.seconds,
                Stack::Keep => {}
            },
            None if !full => {
                p.effects.push((id, e.seconds));
                if self.hooks.live {
                    let name = crate::registry::registry().effects.id(id as usize).unwrap_or("").to_string();
                    self.hook(e.hook.as_deref(), HookEvent::Effect { player: i, effect: name });
                }
            }
            None => return false,
        }
        true
    }

    /// Effect `id` on player `i`, by its id, for a moment (a pack or an
    /// ability holding it on renews it every tick).
    pub(super) fn hold_effect(&mut self, i: usize, id: u8) {
        let Some(p) = self.players.get_mut(i) else { return };
        let Some(e) = effect(id) else { return };
        if !p.alive || e.immune.iter().any(|a| a == p.armor.id()) { return; }
        let full = p.effects.len() >= MAX_EFFECTS;
        match p.effects.iter_mut().find(|(k, _)| *k == id) {
            Some((_, left)) => *left = left.max(STEP * 2.0),
            None if !full => p.effects.push((id, STEP * 2.0)),
            None => {}
        }
    }

    /// Whether player `i` has effect `id`.
    pub fn has_effect(&self, i: usize, id: u8) -> bool { self.players.get(i).is_some_and(|p| p.effects.iter().any(|(k, _)| *k == id)) }

    /// Whether sensors, turrets and homing rounds can't pick player `i` out.
    pub fn sensor_hidden(&self, i: usize) -> bool { self.players.get(i).is_some_and(|p| sensor_hidden(&p.effects)) }

    /// Firing or being hit ends player `i`'s effects that reveal them.
    pub(super) fn reveal(&mut self, i: usize) {
        let Some(p) = self.players.get_mut(i) else { return };
        if p.effects.is_empty() { return; }
        p.effects.retain(|(k, _)| !effect(*k).is_some_and(|e| e.reveal));
        self.switch_off_revealed(i);
    }

    /// A station ends what it cleanses.
    pub fn cleanse(p: &mut Player) { p.effects.retain(|(k, _)| !effect(*k).is_some_and(|e| e.cleansed)); }

    /// Damage `amount` (after armor) of `kind` to player `i`, less what their
    /// shields pay from energy instead. A shield's `absorb` prices a point
    /// of the hit before armor, and `armor` is the share of a hit the armor
    /// lets through, so heavier armor pays more for each point it takes.
    pub(super) fn shielded(&mut self, i: usize, amount: f32, kind: &str, armor: f32) -> f32 {
        let p = &mut self.players[i];
        if p.effects.is_empty() || amount <= 0.0 { return amount; }
        let mut amount = amount;
        for (id, _) in &p.effects {
            let Some(e) = effect(*id) else { continue };
            if e.absorb <= 0.0 || !(e.absorb_kinds.is_empty() || e.absorb_kinds.iter().any(|k| k == kind)) { continue; }
            let price = if armor > 0.0 { e.absorb / armor } else { e.absorb };
            let taken = amount.min(p.energy / price);
            p.energy -= taken * price;
            amount -= taken;
        }
        amount
    }

    /// The server's part each tick: damage and healing over time.
    pub(super) fn step_status(&mut self, dt: f32) {
        if self.predicting { return; }
        for i in 0..self.players.len() {
            if !self.players[i].alive || self.players[i].effects.is_empty() { continue; }
            let effects = self.players[i].effects.clone();
            for (id, _) in effects {
                let Some(e) = effect(id) else { continue };
                if e.heal > 0.0 { let p = &mut self.players[i]; p.health = (p.health + e.heal * dt).min(100.0); }
                if e.damage > 0.0 {
                    self.hurt(i, e.damage * dt, &e.damage_kind);
                    if self.players[i].health <= 0.0 && self.players[i].alive { self.kill(i, None, &e.name); break; }
                }
            }
        }
    }

    /// Player `i`'s effects this tick, predicted with their movement: the
    /// timers run down and the expired go; what's left, added up.
    pub(super) fn status_movement(&mut self, i: usize, dt: f32) -> Bearing {
        let p = &mut self.players[i];
        if p.effects.is_empty() { return Bearing::default(); }
        for (_, left) in &mut p.effects { *left -= dt; }
        p.effects.retain(|(_, left)| *left > 0.0);
        bearing(&p.effects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_add_up_stack_and_run_out() {
        let r = crate::registry::registry();
        let shield = r.effects.index("shielded").unwrap() as u8;
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(1);
        assert!(w.give_effect(0, shield));
        assert!(w.give_effect(0, shield), "again: refreshed, not doubled");
        assert_eq!(w.players[0].effects.len(), 1);
        // A shield pays damage from energy: half an energy a point of the
        // hit before armor.
        w.players[0].energy = 30.0;
        let left = w.shielded(0, 10.0, "blast", 1.0);
        assert_eq!(left, 0.0);
        assert!((w.players[0].energy - 25.0).abs() < 1e-4, "{}", w.players[0].energy);
        // Armor that lets half a hit through: 5 points it takes were a hit of 10.
        let left = w.shielded(0, 5.0, "blast", 0.5);
        assert_eq!(left, 0.0);
        assert!((w.players[0].energy - 20.0).abs() < 1e-4, "{}", w.players[0].energy);
        let left = w.shielded(0, 50.0, "blast", 1.0);
        assert!((left - 10.0).abs() < 1e-4 && w.players[0].energy.abs() < 1e-4, "out of energy, the rest gets through");
        // It runs out.
        for _ in 0..30 { w.status_movement(0, STEP); }
        assert!(w.players[0].effects.is_empty());
    }
}
