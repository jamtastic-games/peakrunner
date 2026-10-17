//! Hooks (docs/modding.md §7): server scripts that definitions and modes
//! name. An armor, weapon, pack, deployable, item, effect or vehicle may
//! name a hook (`"hook": "rigger"` for `hooks/rigger.rhai` in its mod), a
//! trigger zone may name one, and a mode may list hooks that hear every
//! event. When something happens to or by a definition with a hook, the
//! engine reports a `HookEvent` to that hook and the mode's, and applies the
//! commands they answer (`rules::Cmd`) at once, as it does for rule sets.
//!
//! Like rules scripts, hooks run on servers only: a program that can run
//! them registers a `HookFactory` (`crates/script`); without one, or when
//! nothing names a hook, events aren't even made. A hook's commands never
//! raise further events (no recursion), and the hooks share a time budget
//! each step: past it, the rest of that step's events are dropped; timers
//! due take at most half of it, the rest waiting for the next step's.
use super::*;
use rules::Cmd;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Something that happened that a hook may answer.
#[derive(Clone, Debug, PartialEq)]
pub enum HookEvent {
    /// `player` spawned.
    Spawn { player: usize },
    /// `victim` died: by `killer` (None: the world), of `cause`.
    Death { victim: usize, killer: Option<usize>, cause: String },
    /// `victim` took `amount` (after armor) of damage kind `kind`.
    Damage { victim: usize, attacker: Option<usize>, amount: f32, kind: String },
    /// `player` fired `weapon` (an id).
    Fire { player: usize, weapon: String },
    /// `player` used their pack (`what`: its id) or class key (`ability`).
    Use { player: usize, what: String },
    /// `player` placed `deployable` at `at`.
    Deploy { player: usize, deployable: String, at: Vec3 },
    /// A `deployable` of `team` was destroyed at `at`.
    Destroyed { deployable: String, team: Team, at: Vec3 },
    /// `toucher`'s armor touched `touched`.
    Touch { toucher: usize, touched: usize },
    /// `player` came into or went out of trigger zone `zone` (its name).
    Enter { player: usize, zone: String },
    Leave { player: usize, zone: String },
    /// `player` took `item`.
    Pickup { player: usize, item: String },
    /// `player` bought at a station (`what`: a purchase code).
    Buy { player: usize, what: u16 },
    /// `player` was given `effect`.
    Effect { player: usize, effect: String },
    /// A timer the hook set ran out.
    Timer { name: String },
}

/// A hook: it reads the world and answers events with commands.
pub trait Hook {
    fn event(&mut self, world: &World, event: &HookEvent, out: &mut Vec<Cmd>);
}

/// Makes the hook named `name` (None: there's no such script, or it doesn't
/// compile; the server logs why).
pub type HookFactory = fn(&str) -> Option<Box<dyn Hook + Send>>;
static FACTORY: OnceLock<HookFactory> = OnceLock::new();

/// Let definitions' hooks run (a server, once at startup).
pub fn set_hook_factory(factory: HookFactory) { let _ = FACTORY.set(factory); }

/// Most time the hooks may take in one step.
pub const STEP_BUDGET: Duration = Duration::from_millis(4);
/// Most timers pending at once.
const MAX_TIMERS: usize = 64;

/// The hooks a match has loaded, its pending timers, and this step's spend.
#[derive(Default)]
pub struct HookSet {
    scripts: HashMap<String, Option<Box<dyn Hook + Send>>>,
    /// (hook, name, seconds left).
    timers: Vec<(String, String, f32)>,
    spent: Duration,
    running: bool,
    /// Whether anything names a hook in this mode (else no events are made).
    pub(super) live: bool,
}

impl HookSet {
    /// Ready for `mode`: forget loaded hooks and timers; live when a factory
    /// is installed and the mode or any definition names a hook.
    pub(super) fn reset(&mut self, mode: crate::map_catalog::SupportedMode) {
        let r = crate::registry::registry();
        let named = !mode.def().hooks.is_empty()
            || r.armors.iter().any(|(_, _, a)| a.hook.is_some()) || r.weapons.iter().any(|(_, _, w)| w.hook.is_some())
            || r.packs.iter().any(|(_, _, p)| p.hook.is_some()) || r.deployables.iter().any(|(_, _, d)| d.hook.is_some())
            || r.items.iter().any(|(_, _, t)| t.hook.is_some()) || r.effects.iter().any(|(_, _, e)| e.hook.is_some())
            || r.vehicles.iter().any(|(_, _, v)| v.hook.is_some());
        *self = HookSet { live: FACTORY.get().is_some() && named, ..Default::default() };
    }
}

impl World {
    /// Report `event` to hook `own` (a definition's) and the mode's hooks,
    /// and apply what they answer. Never during prediction, nor while a
    /// hook's own commands are being applied.
    pub(crate) fn hook(&mut self, own: Option<&str>, event: HookEvent) {
        if !self.hooks.live || self.predicting || self.hooks.running { return; }
        if self.hooks.spent > STEP_BUDGET { return; }
        let mode = &self.mode.def().hooks;
        let names: Vec<String> = own.into_iter().map(String::from).chain(mode.iter().cloned()).collect();
        for name in names { self.run_hook(&name, &event); }
    }

    fn run_hook(&mut self, name: &str, event: &HookEvent) {
        // Checked before every call: one event can reach several hooks.
        if self.hooks.spent > STEP_BUDGET { return; }
        // All of it counts: loading the script the first time, the call,
        // and applying what it answers.
        let started = Instant::now();
        let mut script = match self.hooks.scripts.remove(name) {
            Some(s) => s,
            None => match FACTORY.get() { Some(factory) => factory(name), None => return },
        };
        let mut out = Vec::new();
        if let Some(s) = script.as_mut() { s.event(self, event, &mut out); }
        self.hooks.scripts.insert(name.to_string(), script);
        self.hooks.running = true;
        for cmd in out {
            // A timer belongs to the hook that set it.
            if let Cmd::Timer { name: t, seconds, .. } = cmd {
                if self.hooks.timers.len() < MAX_TIMERS { self.hooks.timers.push((name.to_string(), t, seconds)); }
                continue;
            }
            self.apply_rule(cmd);
        }
        self.hooks.running = false;
        self.hooks.spent += started.elapsed();
    }

    /// Each step: a fresh budget, and timers that ran out call their hooks,
    /// in order, while half of it lasts (up to MAX_TIMERS at once used to
    /// run past it all); the rest stay due for the next step, and the other
    /// half is the events'. A timer set now, even for no time, waits a step
    /// too.
    pub(super) fn step_hooks(&mut self, dt: f32) {
        if !self.hooks.live || self.predicting { return; }
        self.hooks.spent = Duration::ZERO;
        if self.hooks.timers.is_empty() { return; }
        for t in &mut self.hooks.timers { t.2 -= dt; }
        let due = self.hooks.timers.iter().filter(|t| t.2 <= 0.0).count();
        for _ in 0..due {
            if self.hooks.spent > STEP_BUDGET / 2 { break; }
            let Some(k) = self.hooks.timers.iter().position(|t| t.2 <= 0.0) else { break };
            let (hook, name, _) = self.hooks.timers.remove(k);
            self.run_hook(&hook, &HookEvent::Timer { name });
        }
    }

    /// The hook a player's armor names.
    pub(super) fn armor_hook(&self, i: usize) -> Option<&'static str> {
        self.players.get(i).and_then(|p| p.armor.def().hook.as_deref())
    }
}

impl World {
    /// Apply a hook's command, checking it: a bad player, id or number does
    /// nothing.
    pub(super) fn apply_hook_cmd(&mut self, cmd: Cmd) {
        let r = crate::registry::registry();
        let alive = |w: &World, i: usize| w.players.get(i).is_some_and(|p| p.alive);
        let near = |w: &World, v: Vec3| v.is_finite() && v.x >= 0.0 && v.z >= 0.0 && v.x <= w.map_size() && v.z <= w.map_size() && v.y.abs() < 5000.0;
        match cmd {
            Cmd::Damage { player, amount, kind, attacker } => {
                if r.damage.index(&kind).is_some() { self.damage(player, amount, &kind, attacker, "Hook"); }
            }
            Cmd::Heal { player, amount } => if alive(self, player) && amount.is_finite() && amount > 0.0 {
                let p = &mut self.players[player];
                p.health = (p.health + amount).min(100.0);
            },
            Cmd::GiveEffect { player, effect } => if let Some(e) = r.effects.index(&effect) { self.give_effect(player, e as u8); },
            Cmd::ClearEffect { player, effect } => if let (Some(e), Some(p)) = (r.effects.index(&effect), self.players.get_mut(player)) {
                p.effects.retain(|(k, _)| *k as usize != e);
            },
            Cmd::Impulse { player, vel } => if alive(self, player) && vel.is_finite() && vel.length() <= 200.0 && self.players[player].riding == 0 {
                let p = &mut self.players[player];
                p.vel += vel;
                if vel.y > 0.0 { p.on_ground = false; p.coyote = 0.0; }
            },
            Cmd::Teleport { player, to } => if alive(self, player) && near(self, to) && self.players[player].riding == 0 {
                if self.players[player].carrying.is_some() { self.drop_carried_flag(player); }
                let p = &mut self.players[player];
                p.pos = to;
                p.vel = Vec3::ZERO;
            },
            Cmd::SetArmor { player, armor } => if let (Some(class), true) = (ArmorClass::by_id(&armor), alive(self, player)) {
                let p = &mut self.players[player];
                p.armor = class;
                p.energy = p.energy.min(loadout::energy_max(class));
                self.equip(player);
            },
            Cmd::GiveRounds { player, weapon, rounds } => if let (Some(w), true) = (r.weapons.index(&weapon), alive(self, player)) {
                let max = loadout::rounds_max(&self.players[player]).get(w).copied().unwrap_or(0);
                let p = &mut self.players[player];
                if p.ammo.len() <= w { p.ammo.resize(w + 1, 0); }
                p.ammo[w] = (p.ammo[w] as i32 + rounds).clamp(0, max as i32) as u16;
            },
            Cmd::SetEnergy { player, energy } => if alive(self, player) && energy.is_finite() {
                let max = loadout::energy_max(self.players[player].armor);
                self.players[player].energy = energy.clamp(0.0, max);
            },
            Cmd::Credits { player, amount } => if player < self.players.len() {
                if amount >= 0 { self.earn(player, amount as u32); } else { let take = self.credits_of(player).min(amount.unsigned_abs() as f32) as u32; let _ = self.pay(player, take); }
            },
            Cmd::TeamFeed { team, text } => if team < 2 {
                let team = if team == 1 { Team::Glacier } else { Team::Ember };
                crate::feed::push(&mut self.feed, crate::feed::Entry::TeamChat { team, sender: "Game".into(), text });
            },
            Cmd::Waypoint { team, at } => if team < 2 && near(self, at) { self.waypoints[team] = Some(at); },
            Cmd::Place { team, deployable, at, yaw } => if let (Some(kind), true) = (deploy::DeployKind::from_key(&deployable), team < 2 && near(self, at) && yaw.is_finite()) {
                if self.deployables.len() < 256 {
                    let team = if team == 1 { Team::Glacier } else { Team::Ember };
                    self.deployables.push(deploy::Deployable { owner: 0, kind, team, pos: at, yaw, health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
                }
            },
            Cmd::Projectile { owner, projectile, at, vel } => if let Some(k) = r.projectiles.index(&projectile) {
                let Some(round) = r.projectiles.get(k) else { return };
                if !near(self, at) || !vel.is_finite() || vel.length() > 5000.0 || self.discs.len() >= 256 { return; }
                let owner = owner.filter(|&o| o < self.players.len());
                let team = owner.map_or(Team::Ember, |o| self.players[o].team);
                self.discs.push(Disc { pos: at, vel, team, owner: owner.unwrap_or(MAX_PLAYERS), life: round.life, kind: k as u8, spin: 0.0, target: 0, power: 1.0 });
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hook taking 2 ms a call, counting its calls.
    struct Slow(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    impl Hook for Slow {
        fn event(&mut self, _: &World, _: &HookEvent, _: &mut Vec<Cmd>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Timers due together run while half the step's budget lasts (the rest
    /// is the events'), the others on the steps after: all of them used to
    /// run at once, past the budget.
    #[test]
    fn timers_due_together_keep_to_the_budget() {
        use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
        let calls = Arc::new(AtomicUsize::new(0));
        let mut w = World::new();
        w.hooks.live = true;
        w.hooks.scripts.insert("slow".into(), Some(Box::new(Slow(calls.clone()))));
        for k in 0..6 { w.hooks.timers.push(("slow".into(), format!("t{k}"), 0.01)); }
        w.step_hooks(0.02);
        let first = calls.load(Ordering::Relaxed);
        assert!((1..=2).contains(&first), "{first} of 6 within 2 ms");
        assert_eq!(w.hooks.timers.len(), 6 - first, "the rest wait");
        for _ in 0..10 { w.step_hooks(0.02); }
        assert_eq!((calls.load(Ordering::Relaxed), w.hooks.timers.len()), (6, 0), "all in the end, once each");
    }

    /// Junk commands (bad players, ids, numbers and places) do nothing
    /// harmful: no panic, nothing out of range.
    #[test]
    fn junk_commands_are_checked() {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);
        let mut seed = 0x1234_5678u64;
        let mut next = |n: u64| { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed % n };
        let numbers = [0.0, -5.0, 25.0, 1e9, f32::NAN, f32::INFINITY, -f32::INFINITY, 3.5];
        let ids = ["shielded", "", "Bad Id", "disc", "light", "turret", "x".repeat(40).leak() as &str, "blast"];
        for _ in 0..4000 {
            let player = next(40) as usize;
            let n = numbers[next(8) as usize];
            let id = ids[next(8) as usize].to_string();
            let v = Vec3::new(numbers[next(8) as usize], numbers[next(8) as usize], numbers[next(8) as usize]);
            let cmd = match next(14) {
                0 => Cmd::Damage { player, amount: n, kind: id, attacker: Some(next(40) as usize) },
                1 => Cmd::Heal { player, amount: n },
                2 => Cmd::GiveEffect { player, effect: id },
                3 => Cmd::ClearEffect { player, effect: id },
                4 => Cmd::Impulse { player, vel: v },
                5 => Cmd::Teleport { player, to: v },
                6 => Cmd::SetArmor { player, armor: id },
                7 => Cmd::GiveRounds { player, weapon: id, rounds: next(20000) as i32 - 10000 },
                8 => Cmd::SetEnergy { player, energy: n },
                9 => Cmd::Credits { player, amount: next(2000) as i32 - 1000 },
                10 => Cmd::Waypoint { team: next(4) as usize, at: v },
                11 => Cmd::Place { team: next(4) as usize, deployable: id, at: v, yaw: n },
                12 => Cmd::Projectile { owner: Some(next(40) as usize), projectile: id, at: v, vel: v },
                _ => Cmd::TeamFeed { team: next(4) as usize, text: id },
            };
            w.apply_rule(cmd);
        }
        for p in &w.players {
            assert!(p.pos.is_finite() && p.vel.is_finite() && p.health.is_finite() && p.energy.is_finite());
            assert!(p.health <= 100.0 && p.energy >= 0.0);
        }
        assert!(w.deployables.iter().all(|d| d.pos.is_finite() && d.yaw.is_finite()));
        assert!(w.discs.iter().all(|d| d.pos.is_finite() && d.vel.is_finite()) && w.discs.len() <= 256);
    }
}
