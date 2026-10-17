//! The blade (`defs::Blade`): the side game a map's blade zones play
//! (`features::ZoneKind::Blade`; Tenfold's tower). Inside one, every hand
//! holds a blade and the jets are off. Fire swings it: a tap is a quick
//! swing, the three in turn (right to left, left to right, overhead); a
//! hold builds power for a heavier one; taps in quick succession chain into
//! combos (a third tap whirls the blade all the way round, a hold after one
//! tap lunges, a hold after two slams). The jet key guards. Every swing and
//! every second on guard costs stamina (the energy tank, which the jets
//! don't use here); a blocked hit costs stamina for its damage, and what
//! stamina can't pay comes off health and breaks the guard. Blades that
//! meet in mid-swing clash: both reel, nobody is hurt.
//!
//! What happens in a blade zone counts for nothing: a death there gives no
//! frag, takes no life from anyone's score and goes in no feed, and the
//! fallen are back `respawn` seconds later at one of the zone's own spawns.
//!
//! A swing's state is the player's (`State`, in snapshots), counted in
//! physics steps, so a client predicts its own blade exactly; whom it cuts,
//! and guards and clashes, are the server's.
use super::*;
use crate::defs::{Blade, Swing};

/// The swings (`State::swing`): none, the quick ones (right to left, left
/// to right, overhead), the power swing, and the combos' ends.
pub const NONE: u8 = 0;
pub const SLASH_LEFT: u8 = 1;
pub const SLASH_RIGHT: u8 = 2;
pub const CHOP: u8 = 3;
pub const POWER: u8 = 4;
pub const WHIRL: u8 = 5;
pub const LUNGE: u8 = 6;
pub const SLAM: u8 = 7;
pub const SWINGS: u8 = 8;

/// The latest blow a player took (`State::how`): a cut, a blow on their
/// guard, blades clashing, and a guard broken by a blow it couldn't pay for.
pub const CUT: u8 = 1;
pub const BLOCKED: u8 = 2;
pub const CLASH: u8 = 3;
pub const BROKEN: u8 = 4;

/// Where a press of fire is: none, just pressed (a tap if let go within
/// `tap`), tapped during a swing (it goes when the swing ends), charging a
/// power swing, and spent (it did its part; nothing more till it's let go).
const PRESS_NONE: u8 = 0;
const PRESS_NEW: u8 = 1;
const PRESS_TAPPED: u8 = 2;
const PRESS_CHARGE: u8 = 3;
const PRESS_SPENT: u8 = 4;

/// Walking speed kept while charging a power swing, during the heavy
/// swings, and while reeling.
const CHARGE_WALK: f32 = 0.75;
const HEAVY_WALK: f32 = 0.6;
const REEL_WALK: f32 = 0.5;
/// How far above or below the swinger (m) a body may stand and be reached.
const REACH_UP: f32 = 1.9;
/// The upward part (m/s) of a cut's push, and how hard blades that clash
/// throw their holders apart.
const PUSH_LIFT: f32 = 1.5;
const CLASH_PUSH: f32 = 5.0;

/// A player's blade: the swing and how far into it (steps), its charge (0
/// to 255) and the press of fire behind it, the combo (quick swings so far,
/// steps left to carry it on, the next quick swing's form, and the combo a
/// press joined), the guard, steps left reeling and before stamina comes
/// back, and the latest blow taken (a count clients watch, what it was and
/// whose blade). Put away (all but the blow) outside blade zones.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub swing: u8,
    pub t: u16,
    pub power: u8,
    pub held: u16,
    pub press: u8,
    pub chain: u8,
    pub window: u16,
    pub next: u8,
    pub link: u8,
    pub guard: bool,
    pub reel: u16,
    pub rest: u16,
    pub struck: u8,
    pub how: u8,
    pub by: u8,
    /// Who this swing has reached (bits by slot), and the blade zone (+ 1)
    /// its holder fell in, for the respawn. Server and offline state.
    #[serde(skip)]
    pub reached: u64,
    #[serde(skip)]
    pub fell_in: u8,
}

impl State {
    /// Everything but the latest blow, which clients count.
    fn put_away(&mut self) {
        *self = State { struck: self.struck, how: self.how, by: self.by, fell_in: self.fell_in, ..State::default() };
    }
    /// At death: all but the latest blow and the zone fallen in.
    pub(super) fn put_away_all(&mut self) { self.put_away(); }
    /// A swing's charge, 0 to 1.
    pub fn charge(&self) -> f32 { self.power as f32 / 255.0 }
    /// Charging a power swing.
    pub fn charging(&self) -> bool { self.press == PRESS_CHARGE }
}

/// Seconds as physics steps.
pub fn steps(seconds: f32) -> u16 { (seconds / STEP).round().clamp(0.0, u16::MAX as f32) as u16 }

/// A swing's numbers.
pub fn swing_def(kind: u8) -> Option<&'static Swing> {
    let b = crate::defs::blade();
    match kind {
        SLASH_LEFT | SLASH_RIGHT | CHOP => Some(&b.quick),
        POWER => Some(&b.power),
        WHIRL => Some(&b.whirl),
        LUNGE => Some(&b.lunge),
        SLAM => Some(&b.slam),
        _ => None,
    }
}

/// One of the quick swings.
pub fn quick(kind: u8) -> bool { matches!(kind, SLASH_LEFT | SLASH_RIGHT | CHOP) }

/// A swing at `charge` (a power swing's; 0 to 1): its damage, push and
/// stamina.
pub fn scaled(kind: u8, charge: f32) -> Option<(f32, f32, f32)> {
    let s = swing_def(kind)?;
    let b = crate::defs::blade();
    let k = if kind == POWER { charge.clamp(0.0, 1.0) } else { 0.0 };
    let more = 1.0 + (b.charged - 1.0) * k;
    Some((s.damage * more, s.push * more, s.stamina * (1.0 + (b.charged_cost - 1.0) * k)))
}

/// Where the blade is across the facing (radians, right positive) at share
/// `f` (0 to 1) of a swing's cut, and the half width it covers there: a
/// slash sweeps across, the whirl goes all the way round, and the chop,
/// the lunge's thrust and the slam strike ahead at once.
pub fn blade_angle(kind: u8, f: f32, arc: f32) -> (f32, f32) {
    let half = arc.to_radians() * 0.5;
    match kind {
        SLASH_LEFT | POWER => (half - 2.0 * half * f, 0.0),
        SLASH_RIGHT => (-half + 2.0 * half * f, 0.0),
        WHIRL => (std::f32::consts::TAU * f, 0.0),
        CHOP => (0.0, half / 3.0),
        _ => (0.0, half),
    }
}

/// The share of the cut a swing is at `t` steps in (None: not cutting).
fn cut_share(def: &Swing, t: u16) -> Option<f32> {
    let (a, b) = (steps(def.cut[0]), steps(def.cut[1]));
    let b = b.max(a + 1);
    (t > a && t <= b).then(|| (t - a) as f32 / (b - a) as f32)
}

/// Whether `angle` lies between `lo` and `hi` (radians, `lo <= hi`), round
/// the circle.
fn within(angle: f32, lo: f32, hi: f32) -> bool {
    let tau = std::f32::consts::TAU;
    [angle - tau, angle, angle + tau].iter().any(|a| *a >= lo && *a <= hi)
}

/// The bearing of `to` from a body at `from` facing `yaw`: radians, 0
/// straight ahead, right positive.
fn bearing(from: Vec3, yaw: f32, to: Vec3) -> f32 {
    let (fwd, right) = move_basis(yaw);
    let d = to - from;
    d.dot(right).atan2(d.dot(fwd))
}

impl World {
    /// The blade zone player `i` is in, while alive (`features::blade_zone`).
    pub fn blade_zone_of(&self, i: usize) -> Option<usize> {
        let p = self.players.get(i)?;
        if !p.alive || p.hidden() || p.riding > 0 { return None; }
        features::blade_zone(self.map, p.pos)
    }

    /// Player `i`'s stamina, 0 to 100 (the energy tank).
    pub fn stamina(&self, i: usize) -> f32 {
        let p = &self.players[i];
        100.0 * p.energy / loadout::energy_max(p.armor).max(1e-3)
    }

    fn spend_stamina(&mut self, i: usize, points: f32) {
        let p = &mut self.players[i];
        let max = loadout::energy_max(p.armor);
        p.energy = (p.energy - points * max / 100.0).clamp(0.0, max);
    }

    /// Outside a blade zone: the blade goes away.
    pub(super) fn sheathe(&mut self, i: usize) {
        if self.players[i].blade.swing != NONE || self.players[i].blade.press != PRESS_NONE || self.players[i].blade.guard
            || self.players[i].blade.chain != 0 || self.players[i].blade.reel != 0 {
            self.players[i].blade.put_away();
        }
    }

    /// Player `i`'s blade this step in a blade zone, with fire and the
    /// guard (jet) key as held: the press, the swing, the guard, stamina.
    /// Returns the share of walking speed it leaves them. Predicted.
    pub(super) fn blade_step(&mut self, i: usize, fire: bool, guard: bool, dt: f32) -> f32 {
        let b = *crate::defs::blade();
        let tap = steps(b.tap);
        let st = &mut self.players[i].blade;
        st.rest = st.rest.saturating_sub(1);
        let pressed = fire && st.held == 0;
        let released = !fire && st.held > 0;
        let held_was = st.held;
        st.held = if fire { st.held.saturating_add(1) } else { 0 };
        if released { st.press = if st.press == PRESS_NEW && st.swing != NONE { PRESS_TAPPED } else if st.press == PRESS_TAPPED { PRESS_TAPPED } else { st.press }; }
        // Reeling: nothing but getting back the footing.
        if st.reel > 0 {
            st.reel -= 1;
            (st.swing, st.t, st.guard) = (NONE, 0, false);
            st.press = if fire { PRESS_SPENT } else { PRESS_NONE };
            return REEL_WALK;
        }
        // A press joins the combo running when it's made.
        if pressed {
            st.press = PRESS_NEW;
            st.link = if quick(st.swing) || (st.swing == NONE && st.window > 0) { st.chain } else { 0 };
        }
        if st.swing != NONE {
            st.t += 1;
            let def = swing_def(st.swing).copied();
            // A quick swing's follow-through gives way to the next blow of a
            // combo: a tap, or a hold that's past a tap.
            let next = st.press == PRESS_TAPPED || (st.press == PRESS_NEW && st.held > tap);
            let through = def.is_some_and(|d| quick(st.swing) && st.t > steps(d.cut[1]));
            if def.is_some_and(|d| st.t < steps(d.time)) && !(next && through) {
                return if matches!(st.swing, POWER | SLAM) { HEAVY_WALK } else { 1.0 };
            }
            // The swing is over: a quick one leaves the combo open.
            if quick(st.swing) { st.window = steps(b.combo); } else { (st.chain, st.window) = (0, 0); }
            (st.swing, st.t, st.power) = (NONE, 0, 0);
            if st.press == PRESS_NEW || st.press == PRESS_TAPPED { st.link = st.chain; }
        } else if st.window > 0 {
            st.window -= 1;
            if st.window == 0 { st.chain = 0; }
        }
        // On guard: no swings, and stamina drains. An empty tank drops it.
        if guard && self.stamina(i) > 0.0 {
            self.spend_stamina(i, b.guard_drain * dt);
            let st = &mut self.players[i].blade;
            st.guard = true;
            st.rest = steps(b.rest);
            st.press = if fire { PRESS_SPENT } else { PRESS_NONE };
            return b.guard_walk;
        }
        self.players[i].blade.guard = false;
        let st = self.players[i].blade;
        match st.press {
            // A tap: let go within `tap`, or tapped during the last swing.
            PRESS_TAPPED => { self.players[i].blade.press = PRESS_NONE; self.blade_tap(i); }
            PRESS_NEW if released && held_was <= tap => { self.players[i].blade.press = PRESS_NONE; self.blade_tap(i); }
            // Held past a tap: a combo's end, or a power swing charging.
            PRESS_NEW if st.held > tap => {
                if st.link >= 1 {
                    self.players[i].blade.press = PRESS_SPENT;
                    let kind = if st.link >= 2 { SLAM } else { LUNGE };
                    self.start_swing(i, kind, 0.0);
                } else {
                    self.players[i].blade.press = PRESS_CHARGE;
                }
            }
            PRESS_CHARGE if released => {
                self.players[i].blade.press = PRESS_NONE;
                let charge = (held_was.saturating_sub(tap) as f32 / steps(b.charge).max(1) as f32).min(1.0);
                self.start_swing(i, POWER, charge);
            }
            _ => {}
        }
        let st = &mut self.players[i].blade;
        if st.press == PRESS_CHARGE {
            let charge = (st.held.saturating_sub(tap) as f32 / steps(b.charge).max(1) as f32).min(1.0);
            st.power = (charge * 255.0).round() as u8;
            st.rest = steps(b.rest);
            return CHARGE_WALK;
        }
        if st.press == PRESS_SPENT && !fire { st.press = PRESS_NONE; }
        // Stamina comes back once the blade has rested.
        if st.swing == NONE && st.rest == 0 {
            let p = &mut self.players[i];
            let max = loadout::energy_max(p.armor);
            p.energy = (p.energy + b.regen * max / 100.0 * dt).min(max);
        }
        if matches!(self.players[i].blade.swing, POWER | SLAM) { HEAVY_WALK } else { 1.0 }
    }

    /// A tap of fire: the next quick swing, or a combo's third, the whirl.
    fn blade_tap(&mut self, i: usize) {
        let st = self.players[i].blade;
        if st.link >= 2 {
            self.start_swing(i, WHIRL, 0.0);
            return;
        }
        let kind = [SLASH_LEFT, SLASH_RIGHT, CHOP][st.next as usize % 3];
        if self.start_swing(i, kind, 0.0) {
            let st = &mut self.players[i].blade;
            st.next = (st.next + 1) % 3;
            st.chain = st.link + 1;
        }
    }

    /// Start swing `kind` (at `charge`, a power swing's) if there's the
    /// stamina for it; a lunge dashes ahead and a slam hops.
    fn start_swing(&mut self, i: usize, kind: u8, charge: f32) -> bool {
        let Some((_, _, cost)) = scaled(kind, charge) else { return false };
        let def = *swing_def(kind).expect("a swing");
        if self.stamina(i) + 1e-3 < cost {
            if i == self.player_id && !self.predicting { self.msg("OUT OF STAMINA", 0.8); }
            return false;
        }
        self.spend_stamina(i, cost);
        let rest = steps(crate::defs::blade().rest);
        let p = &mut self.players[i];
        p.blade.swing = kind;
        p.blade.t = 0;
        p.blade.power = (charge.clamp(0.0, 1.0) * 255.0).round() as u8;
        p.blade.reached = 0;
        p.blade.rest = rest;
        if !quick(kind) { (p.blade.chain, p.blade.window) = (0, 0); }
        let (fwd, _) = move_basis(p.yaw);
        if def.dash > 0.0 { p.vel += fwd * def.dash; }
        if def.hop > 0.0 && p.on_ground { p.vel.y = p.vel.y.max(def.hop); p.on_ground = false; }
        true
    }

    /// Whom player `i`'s swing reaches this step, and what comes of it: a
    /// cut, a blow on a guard (and maybe a guard broken), or a clash. The
    /// server's (and offline), after everyone has moved.
    pub(super) fn blade_cuts(&mut self, i: usize) {
        if self.predicting { return; }
        let st = self.players[i].blade;
        let Some(def) = swing_def(st.swing).copied() else { return };
        let Some(f) = cut_share(&def, st.t) else { return };
        let b = *crate::defs::blade();
        let prev = cut_share(&def, st.t - 1).unwrap_or(0.0);
        let (a0, w) = blade_angle(st.swing, prev, def.arc);
        let (a1, _) = blade_angle(st.swing, f, def.arc);
        let reach = b.reach + def.reach;
        let (me, yaw) = (self.players[i].pos, self.players[i].yaw);
        let (damage, push, _) = scaled(st.swing, st.charge()).expect("a swing");
        for j in 0..self.players.len() {
            if j == i || self.players[i].blade.swing == NONE || st.reached & (1 << j.min(63)) != 0 { continue; }
            if self.blade_zone_of(j).is_none() { continue; }
            let them = self.players[j].pos;
            let flat = Vec2::new(them.x - me.x, them.z - me.z).length();
            if flat > reach + bump::BODY_RADIUS || (them.y - me.y).abs() > REACH_UP { continue; }
            let margin = bump::BODY_RADIUS.atan2(flat.max(0.1));
            let at = bearing(me, yaw, them);
            if !within(at, a0.min(a1) - w - margin, a0.max(a1) + w + margin) { continue; }
            if !self.sight_clear(me + Vec3::Y * 0.9, them + Vec3::Y * 0.9) { continue; }
            self.players[i].blade.reached |= 1 << j.min(63);
            let away = Vec3::new(them.x - me.x, 0.0, them.z - me.z).normalize_or(move_basis(yaw).0);
            let theirs = self.players[j].blade;
            let facing_me = bearing(them, self.players[j].yaw, me);
            // Blades meeting: both swinging at each other this step.
            let clash = swing_def(theirs.swing).and_then(|d| {
                let g = cut_share(d, theirs.t)?;
                let (b0, bw) = blade_angle(theirs.swing, cut_share(d, theirs.t - 1).unwrap_or(0.0), d.arc);
                let (b1, _) = blade_angle(theirs.swing, g, d.arc);
                within(facing_me, b0.min(b1) - bw - margin, b0.max(b1) + bw + margin).then_some(())
            }).is_some();
            if clash {
                let reel = steps(b.reel * 0.5);
                for (k, other, dir) in [(i, j, -away), (j, i, away)] {
                    let p = &mut self.players[k];
                    p.blade.put_away();
                    p.blade.reel = reel;
                    p.vel += dir * CLASH_PUSH;
                    Self::struck(p, CLASH, other);
                }
                return;
            }
            if theirs.guard && facing_me.abs() <= b.guard_arc.to_radians() * 0.5 {
                // On guard: stamina pays for the blow; what it can't comes
                // off health and breaks the guard.
                let cost = damage * b.guard_cost;
                let have = self.stamina(j);
                self.spend_stamina(j, cost);
                let short = (cost - have).max(0.0) / b.guard_cost.max(1e-3);
                {
                    let p = &mut self.players[j];
                    p.vel += away * push * 0.4;
                    if short > 0.0 {
                        p.blade.put_away();
                        p.blade.reel = steps(b.reel);
                    }
                    Self::struck(p, if short > 0.0 { BROKEN } else { BLOCKED }, i);
                }
                // The blow glances off: the swing is spent.
                let p = &mut self.players[i];
                (p.blade.swing, p.blade.t, p.blade.chain, p.blade.window) = (NONE, 0, 0, 0);
                p.blade.reel = steps(b.reel * 0.3);
                if short > 0.0 { self.damage(j, short, "blade", Some(i), "Blade"); }
                return;
            }
            Self::struck(&mut self.players[j], CUT, i);
            self.players[j].vel += away * push + Vec3::Y * PUSH_LIFT;
            self.players[i].hits += 1;
            if i == self.player_id { self.hitmarker = 1.0; self.push_event("hit"); }
            self.damage(j, damage, "blade", Some(i), "Blade");
        }
    }

    fn struck(p: &mut Player, how: u8, by: usize) {
        p.blade.struck = p.blade.struck.wrapping_add(1);
        p.blade.how = how;
        p.blade.by = by.min(255) as u8;
    }

    /// A spawn point of the lobby's tower for player `i`, drawn at random
    /// from all of them, both teams' lists (one on every floor, and the
    /// plaza's), but none someone stands on.
    pub(super) fn lobby_spawn(&mut self, i: usize) -> Option<(Vec3, f32)> {
        let mut points = crate::terrain::spawn_points_on(self.map, true);
        points.extend(crate::terrain::spawn_points_on(self.map, false));
        let free: Vec<(Vec3, f32)> = points.iter().copied()
            .filter(|(at, _)| self.players.iter().enumerate().all(|(j, p)| j == i || !p.alive || p.pos.distance(*at) > 2.0)).collect();
        let pool = if free.is_empty() { points } else { free };
        if pool.is_empty() { return None; }
        let k = ((self.rng() * pool.len() as f32) as usize).min(pool.len() - 1);
        Some(pool[k])
    }

    /// A blade zone's spawn for player `i`, fallen in zone `zone`: the one
    /// farthest from everyone alive.
    pub(super) fn blade_spawn(&self, i: usize, zone: usize) -> Option<(Vec3, f32)> {
        let z = features::zones(self.map).get(zone)?;
        z.spawns.iter().map(|s| (Vec3::new(s[0], s[1], s[2]), s[3].to_radians()))
            .max_by(|a, b| {
                let room = |at: Vec3| self.players.iter().enumerate().filter(|(j, p)| *j != i && p.alive && !p.hidden())
                    .map(|(_, p)| p.pos.distance(at)).fold(f32::INFINITY, f32::min);
                room(a.0).total_cmp(&room(b.0))
            })
    }
}

/// Sparring bots' names in the duel lobby's tower: half from one far-off
/// galaxy, half from another's starship, mixed into one parody name each.
pub const SPARRING_NAMES: [&str; 26] = [
    "Jean Luc Skywalker", "Luke Q", "Data the Dog", "Jabba the Klingon", "Obi Wan Kirk", "Darth Spock",
    "Khan Solo", "Princess Uhura", "Boba Worf", "Captain Vader", "Lando Sulu", "Chewie Riker", "Yoda McCoy",
    "Qui Gon Chekov", "Wesley Kenobi", "Jar Jar Sisko", "Seven of Ewoks", "R2 Borg", "Padme Janeway",
    "Grand Moff Quark", "Ahsoka Tuvok", "Greedo the Ferengi", "Tribble Hutt", "Scotty Windu",
    "Admiral Ackbar Troi", "Emperor Picard",
];

/// A sparring bot's plan (server and offline only): its floor (the blade
/// zone + 1), fire presses queued (steps held, then steps let go), how long
/// until it next decides and keeps its guard up, the enemy swing it last
/// answered, which way it circles, where it wanders, and how stuck it is.
#[derive(Clone, Debug, Default)]
pub struct Spar {
    pub home: u8,
    presses: Vec<(u16, u16)>,
    wait: f32,
    guard: f32,
    answered: (usize, u64),
    side: f32,
    wander: Option<Vec3>,
    stuck: f32,
    dodge: f32,
    jumped: bool,
    outside: f32,
    /// How quick it is to decide (s) and how likely to guard a swing it sees.
    pace: f32,
    wary: f32,
}

/// The farthest a sparring bot looks for someone to fight on its floor (m).
const SPAR_SIGHT: f32 = 40.0;
/// Stamina below which it backs off to get its wind.
const WINDED: f32 = 24.0;

impl World {
    /// Sparring: a map with blade zones in a mode without weapons (the duel
    /// lobby's tower), where bots fight with the blade on their floors.
    pub fn sparring(&self) -> bool {
        features::has_blade_zones(self.map) && self.mode.def().weapons.as_ref().is_some_and(|w| w.is_empty())
    }

    /// A sparring bot's floor (the blade zone it belongs to): two bots a
    /// floor from the ground up, by the order they're in.
    pub(super) fn spar_home(&mut self, i: usize) -> Option<usize> {
        if !self.players[i].is_bot || !self.sparring() { return None; }
        if self.players[i].spar.home == 0 {
            let rank = self.players.iter().take(i).filter(|p| p.is_bot && p.net_id != 0).count();
            let blade: Vec<usize> = features::zones(self.map).iter().enumerate().filter(|(_, z)| z.kind == features::ZoneKind::Blade).map(|(k, _)| k).collect();
            let s = &mut self.players[i].spar;
            s.home = (blade[(rank / 2) % blade.len().max(1)] + 1) as u8;
            let r = (self.rng(), self.rng());
            let s = &mut self.players[i].spar;
            (s.pace, s.wary, s.side) = (0.45 + 0.5 * r.0, 0.35 + 0.45 * r.1, if rank % 2 == 0 { 1.0 } else { -1.0 });
        }
        Some(self.players[i].spar.home as usize - 1)
    }

    /// A sparring bot's controls this step: (right, ahead, jump held, jump
    /// pressed, fire, guard), and it turns to face its foe. It fights whoever
    /// is nearest on its floor (people first), keeps at a blade's length,
    /// circles, swings (quick, combos, power against a guard, the odd lunge),
    /// guards what it sees coming, backs off when winded, steps round molten
    /// pits and hops what it walks into.
    pub(super) fn spar_wish(&mut self, i: usize, dt: f32) -> (f32, f32, bool, bool, bool, bool) {
        let b = *crate::defs::blade();
        let home = self.spar_home(i);
        let zone = self.blade_zone_of(i).or(home);
        let p = &self.players[i];
        let (pos, vel, on_ground, yaw) = (p.pos, p.vel, p.on_ground, p.yaw);
        // Pushed out of the tower: back to its floor after a while.
        if self.blade_zone_of(i).is_none() {
            self.players[i].spar.outside += dt;
            if self.players[i].spar.outside > 3.0 {
                if let Some((at, yaw)) = home.and_then(|h| self.blade_spawn(i, h)) {
                    let p = &mut self.players[i];
                    (p.pos, p.vel, p.yaw, p.spar.outside) = (at, Vec3::ZERO, yaw, 0.0);
                }
            }
        } else { self.players[i].spar.outside = 0.0; }
        // Its foe: the nearest on the same floor, people first.
        let foe = (0..self.players.len()).filter(|&j| j != i && zone.is_some() && self.blade_zone_of(j) == zone)
            .map(|j| { let o = &self.players[j]; (j, o.pos.distance(pos) * if o.is_bot { 1.0 } else { 0.6 }) })
            .filter(|&(_, d)| d < SPAR_SIGHT).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(j, _)| j);
        let stamina = self.stamina(i);
        let mut dir = Vec3::ZERO;
        let mut guard = false;
        // Molten metal close by: no hops (a slam's, or over what it's stuck on).
        let molten_near = (0..8).any(|k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_4;
            [1.2f32, 2.6].iter().any(|r| crate::water::lava_burn(self.map, pos + Vec3::new(a.sin(), 0.0, a.cos()) * *r).is_some())
        });
        {
            let s = &mut self.players[i].spar;
            s.wait -= dt;
            s.guard -= dt;
            s.dodge -= dt;
        }
        match foe {
            Some(j) => {
                let o = self.players[j].clone();
                let to = Vec3::new(o.pos.x - pos.x, 0.0, o.pos.z - pos.z);
                let d = to.length();
                let toward = to.normalize_or(Vec3::NEG_Z);
                // Face it, a little behind its moves.
                let want = (-toward.x).atan2(-toward.z);
                let turn = ((want - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).clamp(-8.0 * dt, 8.0 * dt);
                self.players[i].yaw = yaw + turn;
                let side = Vec3::new(-toward.z, 0.0, toward.x) * self.players[i].spar.side;
                let reach = b.reach + 0.1;
                // Its swing coming: guard it, as often as this bot is wary.
                let theirs = o.blade;
                let coming = swing_def(theirs.swing).is_some_and(|def| theirs.t <= steps(def.cut[1]))
                    && bearing(o.pos, o.yaw, pos).abs() < 1.2 && d < reach + 1.5;
                // (Each swing once: known by the step it started on.)
                let started = self.steps.wrapping_sub(theirs.t as u64);
                if coming && self.players[i].spar.answered != (j, started) {
                    let roll = self.rng();
                    let s = &mut self.players[i].spar;
                    s.answered = (j, started);
                    if roll < s.wary && stamina > 8.0 { s.guard = 0.45; s.presses.clear(); }
                }
                guard = self.players[i].spar.guard > 0.0 && stamina > 4.0;
                // Molten metal between them: round the floor (the shorter way
                // about its zone's middle) until the way across is clear,
                // instead of standing at the rim.
                let across = (1..d.min(SPAR_SIGHT) as i32).any(|k| crate::water::lava_burn(self.map, pos + toward * k as f32).is_some());
                let round = zone.and_then(|z| features::zones(self.map).get(z)).filter(|_| across).map(|z| {
                    let c = Vec3::new(z.at[0], 0.0, z.at[2]);
                    let (mine, theirs) = (pos - c, o.pos - c);
                    let turn = mine.x.atan2(mine.z);
                    let way = ((theirs.x.atan2(theirs.z) - turn + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).signum();
                    Vec3::new(turn.cos(), 0.0, -turn.sin()) * way
                });
                if stamina < WINDED && !guard {
                    // Winded: back off and get its wind.
                    dir = -toward * 0.8 + side * 0.4;
                } else if let Some(round) = round {
                    dir = round;
                } else if d > reach * 0.8 {
                    dir = toward + side * 0.15;
                } else if d < 1.1 {
                    dir = -toward * 0.6 + side * 0.5;
                } else {
                    dir = side * 0.55;
                }
                // Swing when it's in reach and has decided.
                let ready = self.players[i].spar.wait <= 0.0 && self.players[i].spar.presses.is_empty() && !guard;
                if ready && stamina >= WINDED * 0.8 {
                    let roll = self.rng();
                    let presses: Vec<(u16, u16)> = if d <= reach {
                        if o.blade.guard && roll < 0.6 { vec![(steps(0.5 + 0.4 * self.rng()), 3)] }        // power through a guard
                        else if roll < 0.18 { vec![(3, 9), (3, 9), (3, 6)] }                                     // a whirl
                        else if roll < 0.3 { vec![(steps(0.35 + 0.5 * self.rng()), 3)] }                         // power
                        else if roll < 0.4 && !molten_near { vec![(3, 8), (3, 10), (24, 3)] }                    // a slam (it hops)
                        else { vec![(3, 4)] }                                                                    // quick
                    } else if d < reach + 3.0 && roll < 0.08 * dt * 60.0 && stamina > 45.0
                        && [1.0f32, 2.0, 3.0, 4.0, 5.0].iter().all(|k| crate::water::lava_burn(self.map, pos + toward * *k).is_none()) {
                        vec![(3, 10), (24, 3)]                                                                   // a lunge from range, its dash clear of molten metal
                    } else { Vec::new() };
                    if !presses.is_empty() {
                        let pace = self.players[i].spar.pace;
                        let wait = pace + 0.35 * self.rng();
                        let s = &mut self.players[i].spar;
                        s.presses = presses;
                        s.wait = wait;
                    }
                }
                if self.rng() < 0.4 * dt { self.players[i].spar.side *= -1.0; }
            }
            None => {
                // Nobody on its floor: wander it, ready.
                let c = zone.and_then(|z| features::zones(self.map).get(z)).map(|z| Vec3::new(z.at[0], 0.0, z.at[2]));
                let arrived = self.players[i].spar.wander.is_none_or(|w| Vec3::new(w.x - pos.x, 0.0, w.z - pos.z).length() < 1.5);
                if arrived {
                    if let Some(c) = c {
                        let (a, r) = (self.rng() * std::f32::consts::TAU, 7.0 + 8.0 * self.rng());
                        let at = Vec3::new(c.x + r * a.sin(), pos.y, c.z + r * a.cos());
                        self.players[i].spar.wander = crate::water::lava_burn(self.map, at).is_none().then_some(at);
                    }
                }
                // Only somewhere it can walk straight to: never across molten
                // metal, which it would stand at the rim of.
                let map = self.map;
                if self.players[i].spar.wander.is_some_and(|w| {
                    let to = Vec3::new(w.x - pos.x, 0.0, w.z - pos.z);
                    (1..to.length() as i32 + 1).any(|k| crate::water::lava_burn(map, pos + to.normalize_or_zero() * k as f32).is_some())
                }) {
                    self.players[i].spar.wander = None;
                }
                if let Some(w) = self.players[i].spar.wander {
                    let to = Vec3::new(w.x - pos.x, 0.0, w.z - pos.z);
                    dir = to.normalize_or_zero() * 0.6;
                    if to.length_squared() > 0.25 { self.players[i].yaw = (-to.x).atan2(-to.z); }
                }
            }
        }
        // Stuck against something: hop it, then sidestep a moment.
        let speed = Vec2::new(vel.x, vel.z).length();
        let s = &mut self.players[i].spar;
        if dir.length_squared() > 0.1 && speed < 1.0 && on_ground { s.stuck += dt; } else { s.stuck = 0.0; }
        let mut jump = false;
        if s.stuck > 0.35 {
            jump = !molten_near;
            s.stuck = 0.0;
            s.dodge = 0.5;
            s.side *= -1.0;
        }
        if s.dodge > 0.0 && dir != Vec3::ZERO {
            dir = Vec3::new(-dir.z, 0.0, dir.x) * s.side + dir * 0.3;
        }
        // Never into molten metal: the first way round that's clear for as
        // far as a run takes to stop (and its momentum too), else stand.
        let map = self.map;
        let clear = |v: Vec3| [0.8f32, 1.6, 2.4, 3.2, 4.0].iter().all(|k| crate::water::lava_burn(map, pos + v * *k).is_none());
        if dir != Vec3::ZERO {
            let len = dir.length().min(1.0);
            let unit = dir / dir.length();
            dir = [0.0f32, 0.5, -0.5, 1.0, -1.0, 1.6, -1.6, 2.2, -2.2, 3.1].iter().map(|a| glam::Quat::from_rotation_y(*a) * unit)
                .find(|v| clear(*v)).map_or(Vec3::ZERO, |v| v * len);
        }
        let moving = Vec3::new(vel.x, 0.0, vel.z);
        if moving.length() > 2.0 && !clear(moving.normalize()) && dir.dot(moving) > 0.0 {
            // Heading at it: turn away.
            dir = -moving.normalize();
        }
        // A step from a molten edge: off it, a meter's margin.
        let near: Vec3 = (0..8).map(|k| { let a = k as f32 * std::f32::consts::FRAC_PI_4; Vec3::new(a.sin(), 0.0, a.cos()) })
            .filter(|v| crate::water::lava_burn(map, pos + *v * 1.1).is_some() || crate::water::lava_burn(map, pos + *v * 1.7).is_some()).sum();
        if near != Vec3::ZERO { dir = (dir * 0.3 - near.normalize()).normalize_or_zero(); }
        let s = &mut self.players[i].spar;
        let jump_edge = jump && !s.jumped;
        s.jumped = jump;
        // The fire presses queued: held so many steps, then let go.
        let fire = match s.presses.first_mut() {
            Some((down, _)) if *down > 0 => { *down -= 1; true }
            Some((_, up)) if *up > 0 => { *up -= 1; false }
            Some(_) => { s.presses.remove(0); false }
            None => false,
        };
        let (fwd, right) = move_basis(self.players[i].yaw);
        (dir.dot(right).clamp(-1.0, 1.0), dir.dot(fwd).clamp(-1.0, 1.0), jump, jump_edge, fire, guard)
    }
}

/// The blade's numbers in force, for clients drawing it.
pub fn tuning() -> &'static Blade { crate::defs::blade() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    fn tower() -> MapId { crate::sim::duel::lobby_map().expect("Tenfold Tower is installed (src/assets/maps/tenfold)") }

    /// The duel lobby on the tower, with `people` joined.
    fn lobby(people: usize) -> Match {
        let mut m = Match::new(tower());
        m.world.set_mode(SupportedMode::DuelLobby);
        m.open = true;
        for i in 0..people { m.join(1 + i as u32, &format!("P{i}")).unwrap(); }
        m
    }

    /// The top of floor `k` (1 to 10), from its zone.
    fn floor(m: &Match, k: usize) -> f32 {
        let z = &features::zones(m.world.map)[k - 1];
        z.at[1] - z.size[1] + 0.3
    }

    /// The tower's axis.
    fn axis(m: &Match) -> Vec3 { Vec3::from(features::zones(m.world.map)[0].at) }

    /// Player `i` standing on floor `k` at `r` m from the axis, `a` degrees
    /// round from the stair's landing.
    fn place(m: &mut Match, i: usize, k: usize, r: f32, a: f32) {
        let (c, y) = (axis(m), floor(m, k));
        let a = a.to_radians();
        let at = Vec3::new(c.x + r * a.sin(), y + 1.5, c.z + r * a.cos());
        let ground = crate::terrain::support_on(m.world.map, at).0;
        let p = &mut m.world.players[i];
        (p.pos, p.vel, p.alive, p.health) = (Vec3::new(at.x, ground + PLAYER_RADIUS, at.z), Vec3::ZERO, true, 100.0);
        p.energy = loadout::energy_max(p.armor);
        p.on_ground = true;
    }

    fn yaw_to(from: Vec3, to: Vec3) -> f32 { let d = to - from; (-d.x).atan2(-d.z) }

    /// One step with each player's fire, guard (jet) and facing.
    fn step(m: &mut Match, seq: &mut u64, input: &[(bool, bool, Option<f32>)]) {
        *seq += 1;
        let commands: Vec<Option<Command>> = (0..m.world.players.len()).map(|i| {
            let (fire, jet, yaw) = input.get(i).copied().unwrap_or((false, false, None));
            let p = &m.world.players[i];
            Some(Command { seq: *seq, fire, jet, yaw: yaw.unwrap_or(p.yaw), pitch: 0.0, ..Default::default() })
        }).collect();
        m.step(&commands);
    }

    fn steps(m: &mut Match, seq: &mut u64, n: usize, input: &[(bool, bool, Option<f32>)]) { for _ in 0..n { step(m, seq, input); } }

    /// Player 0 taps fire: pressed for a few steps, then let go.
    fn tap(m: &mut Match, seq: &mut u64) {
        steps(m, seq, 3, &[(true, false, None)]);
        step(m, seq, &[(false, false, None)]);
    }

    #[test]
    fn the_tower_has_ten_blade_floors_and_spawns_on_each() {
        let m = lobby(0);
        let map = m.world.map;
        let zones = features::zones(map);
        assert_eq!(zones.len(), 10);
        assert!(crate::map_catalog::maps_for(SupportedMode::Ctf).iter().all(|&x| x != map), "no rotation offers the lobby");
        assert!(crate::map_catalog::supports(map, SupportedMode::DuelLobby));
        for (k, z) in zones.iter().enumerate() {
            assert_eq!(z.kind, features::ZoneKind::Blade);
            assert_eq!(z.spawns.len(), 4, "floor {}", k + 1);
            for s in &z.spawns {
                let at = Vec3::new(s[0], s[1], s[2]);
                assert_eq!(features::blade_zone(map, at), Some(k), "floor {} spawn {at} is in its own zone", k + 1);
                let ground = crate::terrain::support_on(map, at - Vec3::Y * 0.6).0;
                assert!((at.y - ground - 1.2).abs() < 0.05, "floor {} spawn {at} stands 1.2 m over its floor ({ground})", k + 1);
                assert!(crate::water::lava_burn(map, at).is_none() && crate::water::at(map, &[], at - Vec3::Y).is_none(), "floor {} spawn {at} is dry", k + 1);
                assert!(crate::terrain::segment_hit(map, at, at + Vec3::Y * 1.8, PLAYER_RADIUS).is_none(), "floor {} spawn {at} has headroom", k + 1);
                for d in 0..8 {
                    let a = d as f32 * std::f32::consts::FRAC_PI_4;
                    let to = at + Vec3::new(a.sin(), 0.3, a.cos()) * 1.2;
                    assert!(crate::terrain::segment_hit(map, at + Vec3::Y * 0.3, to, PLAYER_RADIUS).is_none(), "floor {} spawn {at} has room round it", k + 1);
                }
            }
        }
        // People join on every floor, and on the quiet plaza.
        for team in crate::map_pack::on(map).unwrap().manifest.spawn_points.iter() {
            let mut floors: Vec<usize> = team.iter().filter_map(|s| features::blade_zone(map, Vec3::new(s[0], s[1], s[2]))).collect();
            floors.sort();
            assert_eq!(floors, (0..10).collect::<Vec<_>>(), "one on every floor");
            assert!(team.iter().any(|s| features::blade_zone(map, Vec3::new(s[0], s[1], s[2])).is_none()), "and the plaza");
        }
    }

    /// Walking the spiral with the real movement: from the ground floor's
    /// landing round the core to the crown.
    #[test]
    fn the_spiral_climbs_from_the_ground_to_the_crown() {
        let mut m = lobby(1);
        place(&mut m, 0, 1, 3.8, 0.0);
        let c = axis(&m);
        let mut seq = 0;
        let mut highest = 0.0f32;
        for _ in 0..(90.0 / STEP) as usize {
            let p = m.world.players[0].pos;
            let a = (p.x - c.x).atan2(p.z - c.z) + 0.5;
            let target = Vec3::new(c.x + 3.8 * a.sin(), p.y, c.z + 3.8 * a.cos());
            seq += 1;
            let command = Command { seq, move_z: 1.0, yaw: yaw_to(p, target), ..Default::default() };
            m.step(&[Some(command)]);
            highest = highest.max(m.world.players[0].pos.y);
            if features::blade_zone(m.world.map, m.world.players[0].pos) == Some(9) { break; }
        }
        let p = &m.world.players[0];
        assert_eq!(features::blade_zone(m.world.map, p.pos), Some(9), "reached the crown: at {} (highest {highest})", p.pos);
        assert!(p.alive);
    }

    #[test]
    fn inside_the_jets_are_off_and_the_jet_key_guards() {
        let mut m = lobby(1);
        let mut seq = 0;
        place(&mut m, 0, 2, 12.0, 80.0);
        let y = m.world.players[0].pos.y;
        let full = m.world.stamina(0);
        steps(&mut m, &mut seq, 60, &[(false, true, None)]);
        let p = &m.world.players[0];
        assert!(p.blade.guard && !p.jetting && p.pos.y < y + 0.1, "on guard, not flying ({} from {y})", p.pos.y);
        let drained = full - m.world.stamina(0);
        assert!((drained - crate::defs::blade().guard_drain).abs() < 1.5, "a second on guard costs its drain: {drained}");
        // Outside on the plaza, the jets fly and the blade is away.
        let c = axis(&m);
        let ground = floor(&m, 1);
        m.world.players[0].pos = Vec3::new(c.x + 30.0, ground + 0.6, c.z);
        let start = m.world.players[0].pos.y;
        steps(&mut m, &mut seq, 30, &[(false, true, None)]);
        let p = &m.world.players[0];
        assert!(p.pos.y > start + 1.0 && p.blade == State { struck: p.blade.struck, how: p.blade.how, by: p.blade.by, ..State::default() },
            "jets outside: {} from {start}, blade {:?}", p.pos.y, p.blade);
    }

    #[test]
    fn taps_swing_in_turn_and_quick_taps_chain_into_a_whirl() {
        let mut m = lobby(1);
        let mut seq = 0;
        place(&mut m, 0, 10, 14.0, 90.0);
        let mut kinds = Vec::new();
        for _ in 0..3 {
            tap(&mut m, &mut seq);
            kinds.push(m.world.players[0].blade.swing);
            steps(&mut m, &mut seq, 90, &[]);
        }
        assert_eq!(kinds, [SLASH_LEFT, SLASH_RIGHT, CHOP], "a different swing each time");
        assert_eq!(m.world.players[0].blade.chain, 0, "the combo lapsed between them");
        // Quick taps, each made while the last swing is still going: each
        // new swing starts as the last one's cut is through.
        let mut kinds = Vec::new();
        tap(&mut m, &mut seq);
        kinds.push(m.world.players[0].blade.swing);
        for _ in 0..2 {
            steps(&mut m, &mut seq, 4, &[]);
            tap(&mut m, &mut seq);
            let mut waited = 0;
            while m.world.players[0].blade.t != 0 || m.world.players[0].blade.swing == NONE {
                step(&mut m, &mut seq, &[]);
                waited += 1;
                assert!(waited < 60, "the next swing came: {:?}", m.world.players[0].blade);
            }
            kinds.push(m.world.players[0].blade.swing);
        }
        assert_eq!(kinds[2], WHIRL, "the third quick tap whirls: {kinds:?}");
        assert!(quick(kinds[0]) && quick(kinds[1]));
    }

    #[test]
    fn holds_build_power_and_end_combos() {
        let mut m = lobby(1);
        let mut seq = 0;
        place(&mut m, 0, 10, 14.0, 90.0);
        steps(&mut m, &mut seq, 60, &[(true, false, None)]);
        let b = m.world.players[0].blade;
        assert!(b.charging() && b.swing == NONE && b.charge() > 0.5, "charging: {b:?}");
        step(&mut m, &mut seq, &[]);
        let b = m.world.players[0].blade;
        assert_eq!(b.swing, POWER);
        assert!(b.charge() > 0.5);
        steps(&mut m, &mut seq, 120, &[]);
        // A tap, then a hold: the lunge, dashing ahead.
        tap(&mut m, &mut seq);
        steps(&mut m, &mut seq, 4, &[]);
        steps(&mut m, &mut seq, 20, &[(true, false, None)]);
        assert_eq!(m.world.players[0].blade.swing, LUNGE);
        assert!(Vec2::new(m.world.players[0].vel.x, m.world.players[0].vel.z).length() > 8.0, "a lunge dashes");
        steps(&mut m, &mut seq, 120, &[]);
        // Two taps, then a hold: the slam.
        tap(&mut m, &mut seq);
        steps(&mut m, &mut seq, 4, &[]);
        tap(&mut m, &mut seq);
        steps(&mut m, &mut seq, 12, &[]);
        steps(&mut m, &mut seq, 24, &[(true, false, None)]);
        assert_eq!(m.world.players[0].blade.swing, SLAM, "{:?}", m.world.players[0].blade);
    }

    #[test]
    fn swings_cost_stamina_and_an_empty_tank_swings_nothing() {
        let mut m = lobby(1);
        let mut seq = 0;
        place(&mut m, 0, 10, 14.0, 90.0);
        let full = m.world.stamina(0);
        tap(&mut m, &mut seq);
        assert!((full - m.world.stamina(0) - crate::defs::blade().quick.stamina).abs() < 0.5);
        steps(&mut m, &mut seq, 120, &[]);
        assert!(m.world.stamina(0) > full - 0.5, "it comes back after a rest");
        m.world.players[0].energy = 0.0;
        tap(&mut m, &mut seq);
        assert_eq!(m.world.players[0].blade.swing, NONE, "no stamina, no swing");
    }

    /// Two on the crown's open deck, 1.8 m apart, facing each other.
    fn facing(m: &mut Match) -> (f32, f32) {
        place(m, 0, 10, 14.0, 90.0);
        place(m, 1, 10, 14.0, 97.4);
        let (a, b) = (m.world.players[0].pos, m.world.players[1].pos);
        (yaw_to(a, b), yaw_to(b, a))
    }

    #[test]
    fn a_cut_hurts_a_guard_pays_in_stamina_and_a_broken_guard_in_health() {
        let mut m = lobby(2);
        let mut seq = 0;
        let (ya, yb) = facing(&mut m);
        steps(&mut m, &mut seq, 2, &[(false, false, Some(ya)), (false, false, Some(yb))]);
        let taken = loadout::damage_taken(m.world.players[1].armor);
        let quick = crate::defs::blade().quick.damage;
        let swing = |m: &mut Match, seq: &mut u64, guard: bool| {
            steps(m, seq, 3, &[(true, false, Some(ya)), (false, guard, Some(yb))]);
            steps(m, seq, 30, &[(false, false, Some(ya)), (false, guard, Some(yb))]);
        };
        swing(&mut m, &mut seq, false);
        let hurt = 100.0 - m.world.players[1].health;
        assert!((hurt - quick * taken).abs() < 0.5, "a cut: {hurt}");
        assert_eq!(m.world.players[1].blade.how, CUT);
        assert!(m.world.players[0].hits >= 1);
        // On guard: the health stays, the stamina pays.
        steps(&mut m, &mut seq, 90, &[(false, false, Some(ya)), (false, false, Some(yb))]);
        let (health, stamina) = (m.world.players[1].health, m.world.stamina(1));
        swing(&mut m, &mut seq, true);
        assert_eq!(m.world.players[1].health, health, "the guard took it");
        assert_eq!(m.world.players[1].blade.how, BLOCKED);
        let paid = stamina - m.world.stamina(1);
        assert!(paid > quick * crate::defs::blade().guard_cost * 0.9, "the guard paid {paid}");
        // An almost empty tank: the blow breaks the guard and the rest is health.
        steps(&mut m, &mut seq, 90, &[(false, false, Some(ya)), (false, false, Some(yb))]);
        let max = loadout::energy_max(m.world.players[1].armor);
        m.world.players[1].energy = max * 0.02;
        let health = m.world.players[1].health;
        swing(&mut m, &mut seq, true);
        let b = &m.world.players[1];
        assert_eq!(b.blade.how, BROKEN);
        assert!(b.health < health - 4.0, "health paid the rest: {} from {health}", b.health);
    }

    #[test]
    fn blades_that_meet_clash() {
        let mut m = lobby(2);
        let mut seq = 0;
        let (ya, yb) = facing(&mut m);
        steps(&mut m, &mut seq, 2, &[(false, false, Some(ya)), (false, false, Some(yb))]);
        steps(&mut m, &mut seq, 3, &[(true, false, Some(ya)), (true, false, Some(yb))]);
        steps(&mut m, &mut seq, 30, &[(false, false, Some(ya)), (false, false, Some(yb))]);
        assert!(m.world.players.iter().take(2).all(|p| p.health == 100.0 && p.blade.how == CLASH), "{:?}",
            m.world.players.iter().take(2).map(|p| (p.health, p.blade.how)).collect::<Vec<_>>());
    }

    /// People in the lobby come in at random: over many spawns, on every
    /// floor and outside.
    #[test]
    fn lobby_spawns_are_random_over_every_floor_and_outside() {
        let mut m = lobby(1);
        let mut seen = std::collections::BTreeSet::new();
        let mut outside = 0;
        for _ in 0..400 {
            m.world.respawn(0);
            match m.world.blade_zone_of(0) { Some(z) => { seen.insert(z); } None => outside += 1 }
        }
        assert_eq!(seen.len(), 10, "every floor: {seen:?}");
        assert!(outside > 40, "and outside: {outside} of 400");
    }

    #[test]
    fn a_death_in_the_tower_counts_for_nothing_and_comes_back_at_a_random_spawn() {
        let mut m = lobby(2);
        let mut seq = 0;
        let (ya, yb) = facing(&mut m);
        steps(&mut m, &mut seq, 2, &[(false, false, Some(ya)), (false, false, Some(yb))]);
        let feed = m.world.feed.len();
        m.world.players[1].health = 5.0;
        steps(&mut m, &mut seq, 3, &[(true, false, Some(ya)), (false, false, Some(yb))]);
        steps(&mut m, &mut seq, 30, &[(false, false, Some(ya))]);
        let (a, b) = (&m.world.players[0], &m.world.players[1]);
        assert!(!b.alive, "cut down");
        assert_eq!((a.frags, b.losses, m.world.feed.len(), m.world.score), (0, 0, feed, [0, 0]), "counts for nothing");
        let respawn = crate::defs::blade().respawn;
        steps(&mut m, &mut seq, ((respawn - 0.6) / STEP) as usize, &[]);
        assert!(!m.world.players[1].alive, "not before {respawn} s");
        steps(&mut m, &mut seq, (1.0 / STEP) as usize, &[]);
        let b = &m.world.players[1];
        assert!(b.alive);
        let points = crate::map_pack::on(m.world.map).unwrap().manifest.spawn_points.concat();
        assert!(points.iter().any(|s| Vec3::new(s[0], s[1], s[2]).with_y(0.0).distance(b.pos.with_y(0.0)) < 0.5), "back at a spawn point: {}", b.pos);
    }

    /// Outside the tower's blade zones the lobby still scores nothing: a fall
    /// or Ctrl+K on the plaza, or a bump, used to count a loss and a frag in
    /// the feed.
    #[test]
    fn a_death_outside_the_tower_counts_for_nothing_too() {
        let mut m = lobby(2);
        m.step(&[]);
        let plaza = crate::map_pack::on(m.world.map).unwrap().manifest.spawn_points.concat().into_iter()
            .map(|s| Vec3::new(s[0], s[1], s[2])).find(|&s| features::blade_zone(m.world.map, s).is_none()).expect("a plaza spawn");
        let feed = m.world.feed.len();
        for (i, (killer, cause)) in [(None, "Fall"), (Some(0), "Bump")].into_iter().enumerate() {
            m.world.players[i].pos = plaza;
            m.world.kill(i, killer, cause);
            assert!(!m.world.players[i].alive);
            assert_eq!(m.world.players[i].respawn, SupportedMode::DuelLobby.def().respawn, "the lobby's own respawn, not a blade zone's");
        }
        let (a, b) = (&m.world.players[0], &m.world.players[1]);
        assert_eq!((a.frags, a.losses, b.losses, m.world.feed.len(), m.world.score), (0, 0, 0, feed, [0, 0]), "counts for nothing");
    }

    /// Sparring bots in the lobby's tower: named from two galaxies, two to a
    /// floor from the ground up, fighting whoever is on their floor and
    /// never walking into molten metal.
    #[test]
    fn sparring_bots_fight_on_their_floors() {
        let mut m = lobby(1);
        m.bots = 12;
        let mut seq = 0;
        place(&mut m, 0, 1, 11.0, 150.0);
        steps(&mut m, &mut seq, 2, &[]);
        let bots: Vec<usize> = (0..m.world.players.len()).filter(|&j| m.world.players[j].is_bot && m.world.players[j].net_id != 0).collect();
        assert_eq!(bots.len(), 11, "the lobby's bots, one making room for the person");
        for &j in &bots {
            let p = &m.world.players[j];
            assert!(SPARRING_NAMES.contains(&p.name.as_str()) && crate::names::validate(&p.name) == Some(p.name.as_str()), "{}", p.name);
        }
        let floors = |m: &Match| bots.iter().filter_map(|&j| m.world.blade_zone_of(j)).collect::<Vec<_>>();
        let mut on = floors(&m);
        on.sort();
        assert_eq!(on, [0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5], "two to a floor from the ground up");
        let (mut hurt, mut molten, mut away) = (0.0f32, 0, 0);
        for _ in 0..(40.0 / STEP) as usize {
            let before = m.world.players[0].health;
            step(&mut m, &mut seq, &[]);
            let p = &m.world.players[0];
            if p.alive { hurt += (before - p.health).max(0.0); } else { hurt += before.max(0.0); place(&mut m, 0, 1, 11.0, 150.0); }
            for &j in &bots {
                let b = &m.world.players[j];
                if b.alive && crate::water::lava_burn(m.world.map, b.pos).is_some() { molten += 1; }
                if b.alive && m.world.blade_zone_of(j).is_none() { away += 1; }
            }
        }
        assert!(hurt > 60.0, "the ground floor's bots fought the person there: {hurt} damage");
        // Then the foundry, between its pits: its bot gives chase round them.
        let at = |m: &mut Match| place(m, 0, 6, 11.6, 124.0);
        at(&mut m);
        let mut hurt = 0.0f32;
        for _ in 0..(60.0 / STEP) as usize {
            let before = m.world.players[0].health;
            step(&mut m, &mut seq, &[]);
            hurt += (before - m.world.players[0].health.max(0.0)).max(0.0);
            if !m.world.players[0].alive { at(&mut m); }
            for &j in &bots {
                let b = &m.world.players[j];
                if b.alive && crate::water::lava_burn(m.world.map, b.pos).is_some() { molten += 1; }
            }
        }
        assert!(hurt > 40.0, "the foundry's bot found the person round its pits: {hurt} damage");
        assert_eq!(molten, 0, "no bot stood in molten metal");
        assert!(away < 60, "bots keep to the tower: {away} steps out");
    }

    #[test]
    fn molten_pits_kill_water_slows_and_neither_reaches_the_floor_below() {
        let mut m = lobby(1);
        let mut seq = 0;
        // The foundry's middle pit (floor 6).
        place(&mut m, 0, 6, 11.6, 180.0);
        steps(&mut m, &mut seq, 30, &[]);
        assert!(!m.world.players[0].alive, "the molten pit kills");
        steps(&mut m, &mut seq, (crate::defs::blade().respawn / STEP) as usize + 10, &[]);
        // Under it on floor 5, nothing lifts you onto it.
        place(&mut m, 0, 5, 7.0, 180.0);
        let y = m.world.players[0].pos.y;
        steps(&mut m, &mut seq, 60, &[]);
        assert!((m.world.players[0].pos.y - y).abs() < 0.2 && m.world.players[0].alive, "floor 5 stays floor 5");
        // The greenhouse's channel slows a walk.
        // Straight along the floor, against the channel's current (which
        // runs the way the angle grows).
        let walk = |m: &mut Match, seq: &mut u64, r: f32, a: f32| {
            place(m, 0, 5, r, a);
            let from = m.world.players[0].pos;
            let back = (a - 90.0).to_radians();
            let yaw = yaw_to(from, from + Vec3::new(back.sin(), 0.0, back.cos()));
            for _ in 0..45 {
                *seq += 1;
                m.step(&[Some(Command { seq: *seq, move_z: 1.0, yaw, ..Default::default() })]);
            }
            m.world.players[0].pos.with_y(0.0).distance(from.with_y(0.0))
        };
        let wading = walk(&mut m, &mut seq, 11.6, 140.0);
        let dry = walk(&mut m, &mut seq, 7.2, 70.0);
        assert!(wading < dry * 0.85, "wading {wading} m, dry {dry} m");
        // A wader jumps back out onto the bank.
        place(&mut m, 0, 5, 11.6, 140.0);
        let c = axis(&m);
        let bank = Vec3::new(c.x + 8.0 * 140f32.to_radians().sin(), 0.0, c.z + 8.0 * 140f32.to_radians().cos());
        for k in 0..90 {
            seq += 1;
            let p = m.world.players[0].pos;
            m.step(&[Some(Command { seq, move_z: 1.0, jump: k % 20 < 2, yaw: yaw_to(p, bank.with_y(p.y)), ..Default::default() })]);
        }
        let out = m.world.players[0].pos.y - floor(&m, 5);
        assert!(out > 0.3, "out of the water and up on the floor ({out})");
    }
}

#[cfg(test)]
mod tower_load {
    use super::*;
    /// 28 people walking and swinging through the tower's floors: the
    /// server's tick time (release: `cargo test --release ... -- --ignored`).
    #[test]
    #[ignore = "a timing probe"]
    fn twenty_eight_in_the_tower() {
        for map in [crate::sim::duel::lobby_map().unwrap(), MapId::OldHoller] {
            let mut m = Match::new(map);
            m.world.set_mode(crate::map_catalog::SupportedMode::DuelLobby);
            m.open = true;
            for i in 0..28 { m.join(1 + i, &format!("P{i}")).unwrap(); }
            let zones = features::zones(map);
            for i in 0..28usize {
                let p = &mut m.world.players[i];
                if let Some(z) = zones.get(i % 10) {
                    let a = i as f32 * 1.3;
                    p.pos = Vec3::new(z.at[0] + 12.0 * a.sin(), z.at[1] - z.size[1] + 1.0, z.at[2] + 12.0 * a.cos());
                }
            }
            let mut worst = 0.0f64;
            let start = std::time::Instant::now();
            let ticks = 1200u64;
            for k in 0..ticks {
                let commands: Vec<Option<Command>> = (0..28).map(|i| Some(Command { seq: k + 1, move_z: 1.0,
                    yaw: (k as f32 * 0.01 + i as f32).sin() * 3.0, fire: (k + i as u64) % 40 < 3, jet: (k + i as u64) % 90 < 20, ..Default::default() })).collect();
                let t = std::time::Instant::now();
                m.step(&commands);
                worst = worst.max(t.elapsed().as_secs_f64());
            }
            let mean = start.elapsed().as_secs_f64() / ticks as f64;
            println!("{}: mean {:.0} us, worst {:.0} us a tick", map.key(), mean * 1e6, worst * 1e6);
        }
    }
}

#[cfg(test)]
mod tower_traps {
    use super::*;
    use crate::map_pack::closest_on;

    /// How far the body at `pos` (its four spheres) overlaps the map at most.
    fn overlap(pack: &crate::map_pack::MapPack, pos: Vec3) -> f32 {
        let mut worst = 0.0f32;
        for t in pack.triangles() {
            for o in [0.0f32, 0.5, 1.0, crate::terrain::EYE] {
                let s = pos + Vec3::Y * o;
                worst = worst.max(PLAYER_RADIUS - closest_on(s, *t).distance(s));
            }
        }
        worst
    }

    /// Nowhere in the tower holds a body for good: wanderers walk, turn and
    /// hop at random on every floor for 40 s each, and one that stalls tries
    /// eight ways out, jumping each way; a spot it can't leave 1.5 m from is
    /// a trap, and so is ending up inside the round wall (no jets indoors to
    /// get out). Prints each trap (floor, radius, angle from the landing,
    /// height over the floor) and fails on any (release:
    /// `cargo test --release -p peakrunner-core --lib tower_traps -- --ignored --nocapture`;
    /// `PEAKRUNNER_TRAP_WANDERERS` a floor, default 24, and
    /// `PEAKRUNNER_TRAP_SEED`). docs/tenfold.md "Traps".
    #[test]
    #[ignore = "a search of the tower for places a body gets stuck (minutes)"]
    fn nobody_gets_stuck_in_the_tower() {
        let map = crate::sim::duel::lobby_map().expect("Tenfold Tower is installed");
        let pack = crate::map_pack::on(map).unwrap();
        let zones = features::zones(map);
        let axis = Vec3::from(zones[0].at);
        let floor = |k: usize| zones[k - 1].at[1] - zones[k - 1].size[1] + 0.3;
        let env = |name: &str, default: u32| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
        let wanderers = env("PEAKRUNNER_TRAP_WANDERERS", 24) as usize;
        let mut rng = env("PEAKRUNNER_TRAP_SEED", 0x9e37_79b9).max(1);
        let mut rand = move || { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; rng as f32 / u32::MAX as f32 };
        let mut traps: Vec<String> = Vec::new();
        for k in 1..=10usize {
            for w in 0..wanderers {
                let mut m = Match::new(map);
                m.world.set_mode(crate::map_catalog::SupportedMode::DuelLobby);
                m.open = true;
                m.join(1, "Wanderer").unwrap();
                // Somewhere on the floor (or on something on it), clear of everything.
                let place = |m: &mut Match, rand: &mut dyn FnMut() -> f32| loop {
                    let (r, a) = (5.6 + 12.0 * rand(), rand() * std::f32::consts::TAU);
                    let at = Vec3::new(axis.x + r * a.sin(), floor(k) + 6.0, axis.z + r * a.cos());
                    let ground = crate::terrain::support_on(map, at).0;
                    if ground < floor(k) - 1.5 || ground > floor(k) + 4.0 { continue; }
                    let at = Vec3::new(at.x, ground + PLAYER_RADIUS + 0.05, at.z);
                    if overlap(pack, at) > 0.0 { continue; }
                    let p = &mut m.world.players[0];
                    (p.pos, p.vel, p.alive, p.health, p.on_ground) = (at, Vec3::ZERO, true, 100.0, false);
                    break;
                };
                place(&mut m, &mut rand);
                let (mut heading, mut turn_in) = (rand() * std::f32::consts::TAU, 60u32);
                let mut track: std::collections::VecDeque<Vec3> = std::collections::VecDeque::new();
                // Trying to get out: from where, for how long, which way first, how far it got.
                let mut escape: Option<(Vec3, u32, f32, f32)> = None;
                for seq in 1..=(60 * 40u64) {
                    let p = &m.world.players[0];
                    if !p.alive || p.pos.y < floor(k) - 3.0 || p.pos.y > floor(k) + 7.0 {
                        place(&mut m, &mut rand);
                        track.clear();
                        escape = None;
                        continue;
                    }
                    let (pos, d) = (p.pos, p.pos - axis);
                    let (r, a, h) = (d.with_y(0.0).length(), d.x.atan2(d.z).to_degrees().rem_euclid(360.0), pos.y - PLAYER_RADIUS - floor(k));
                    // Inside the round wall (r 18 to 19), but for the ground floor's doors.
                    let doorway = k == 1 && [90.0f32, 180.0, 270.0].iter().any(|door| ((a - door + 540.0) % 360.0 - 180.0).abs() < 10.0);
                    if r > 18.0 - PLAYER_RADIUS + 0.08 && r < 19.0 + PLAYER_RADIUS - 0.08 && !doorway && k < 10 {
                        traps.push(format!("floor {k} wanderer {w}: in the wall at r {r:.2} a {a:.1} h {h:.2}"));
                        place(&mut m, &mut rand);
                        track.clear();
                        escape = None;
                        continue;
                    }
                    let mut c = Command { seq, move_z: 1.0, ..Default::default() };
                    match escape.as_mut() {
                        None => {
                            turn_in = turn_in.saturating_sub(1);
                            if turn_in == 0 { heading = rand() * std::f32::consts::TAU; turn_in = 60 + (rand() * 120.0) as u32; }
                            c.yaw = heading;
                            c.jump = rand() < 1.0 / 90.0;
                            track.push_back(pos);
                            if track.len() > 90 {
                                let old = track.pop_front().unwrap();
                                if old.with_y(0.0).distance(pos.with_y(0.0)) < 0.6 && (old.y - pos.y).abs() < 0.6 {
                                    escape = Some((pos, 0, rand() * std::f32::consts::TAU, 0.0));
                                }
                            }
                        }
                        Some((from, t, first, far)) => {
                            *far = far.max(from.with_y(0.0).distance(pos.with_y(0.0))).max(if (from.y - pos.y).abs() > 1.0 { 9.0 } else { 0.0 });
                            // Eight ways round, half a second each, jumping twice each way.
                            c.yaw = *first + (*t / 30) as f32 * std::f32::consts::FRAC_PI_4;
                            c.jump = *t % 30 < 3 || (15..18).contains(&(*t % 30));
                            *t += 1;
                            if *t >= 240 {
                                if *far < 1.5 {
                                    traps.push(format!("floor {k} wanderer {w}: stuck at r {r:.2} a {a:.1} h {h:.2} grounded {}", p.on_ground));
                                    place(&mut m, &mut rand);
                                }
                                track.clear();
                                escape = None;
                            }
                        }
                    }
                    m.step(&[Some(c)]);
                }
            }
        }
        for t in &traps { println!("{t}"); }
        assert!(traps.is_empty(), "{} traps", traps.len());
    }

    /// The tower's sparring bots never stick: twenty of them, two a floor,
    /// spar for ten minutes. One that stays within a meter for 20 s with
    /// nobody within 4 m (so not just trading blows), or ends up inside the
    /// round wall, fails (release:
    /// `cargo test --release -p peakrunner-core --lib sparring_bots_never_stick -- --ignored --nocapture`).
    #[test]
    #[ignore = "ten minutes of the tower's bots (about a minute)"]
    fn sparring_bots_never_stick() {
        let map = crate::sim::duel::lobby_map().expect("Tenfold Tower is installed");
        let zones = features::zones(map);
        let axis = Vec3::from(zones[0].at);
        // The crown's wall is a parapet to stand on, not a wall to be in.
        let crown = zones[9].at[1] - zones[9].size[1] + 0.3 - axis.y;
        let mut m = Match::new(map);
        m.world.set_mode(crate::map_catalog::SupportedMode::DuelLobby);
        m.open = true;
        m.bots = 20;
        let window = (20.0 / STEP) as usize;
        let mut since: Vec<Option<(Vec3, usize)>> = vec![None; m.world.players.len()];
        let mut problems = Vec::new();
        for step in 0..(600.0 / STEP) as usize {
            m.step(&[]);
            let w = &m.world;
            for (j, b) in w.players.iter().enumerate() {
                if !b.is_bot || b.net_id == 0 || !b.alive { since[j] = None; continue; }
                let d = b.pos - axis;
                let (r, a) = (d.with_y(0.0).length(), d.x.atan2(d.z).to_degrees().rem_euclid(360.0));
                let doorway = [90.0f32, 180.0, 270.0].iter().any(|door| ((a - door + 540.0) % 360.0 - 180.0).abs() < 10.0);
                if r > 18.0 - PLAYER_RADIUS + 0.08 && r < 19.0 + PLAYER_RADIUS - 0.08 && !doorway && d.y > -1.0 && d.y < crown - 0.5 {
                    problems.push(format!("{} in the wall at {step}: r {r:.2} a {a:.1} y {:.2}", b.name, d.y));
                }
                let engaged = w.players.iter().enumerate().any(|(o, q)| o != j && q.alive && q.net_id != 0 && q.pos.distance(b.pos) < 4.0);
                match since[j] {
                    Some((at, start)) if b.pos.distance(at) < 1.0 && !engaged => {
                        if step - start > window {
                            let others: Vec<String> = w.players.iter().enumerate().filter(|(o, q)| *o != j && q.alive && q.net_id != 0 && w.blade_zone_of(*o) == w.blade_zone_of(j))
                                .map(|(_, q)| { let e = q.pos - axis; format!("{} r {:.1} a {:.1}", q.name, e.with_y(0.0).length(), e.x.atan2(e.z).to_degrees().rem_euclid(360.0)) }).collect();
                            problems.push(format!("{} stuck at {step}: r {r:.2} a {a:.1} y {:.2} zone {:?} vel {:?} stamina {:.0} with {others:?}", b.name, d.y, w.blade_zone_of(j), b.vel, w.stamina(j)));
                            since[j] = None;
                        }
                    }
                    _ => since[j] = Some((b.pos, step)),
                }
            }
        }
        let bots = m.world.players.iter().filter(|p| p.is_bot && p.net_id != 0).count();
        assert_eq!(bots, 20, "the lobby's bots");
        for p in &problems { println!("{p}"); }
        assert!(problems.is_empty(), "{} problems", problems.len());
    }
}
