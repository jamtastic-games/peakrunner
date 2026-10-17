//! Quantized delta snapshots (marker `delta1`).
//!
//! A snapshot view is prepared once (`Prepared::new`): players and
//! projectiles become fixed-point integers, everything else postcard bytes
//! per item. `Prepared::encode` writes it bit-packed, in full or against a
//! baseline the client acknowledged: players and projectiles as small
//! differences (positions against where the baseline's velocity carried
//! them), other items copied, patched byte-wise or sent literally.
//! `Prepared::decode` rebuilds exactly the `Prepared` the server encoded, so
//! both ends hold identical baselines, and `to_snapshot` turns it into the
//! game's `Snapshot` (the canonical, quantized one).
//!
//! Precision:
//! - positions 1/1024 m (about 1 mm; exact in f32 within 16 km of the
//!   origin), velocities 1/256 m/s
//! - yaw and pitch 2π/65536 (0.0055°)
//! - health and energy 1/64, player timers (respawn, cooldown, throw clock,
//!   stun, coyote time) 1/256 s
//! - projectiles: position and velocity as players, life 1/256 s, spin
//!   1/16 rad (drawn only)
//!
//! Everything else (flags, ball, equipment, deployables, loot, effects, feed,
//! clocks, votes) is sent exactly. Other players are drawn 100 ms behind,
//! blended between snapshots, so millimetres never show. The recipient's own
//! player, which its prediction replays inputs from, is sent exactly in a
//! small per-peer trailer (`own_state`/`apply_own`), so quantization changes
//! neither the server's simulation nor a single prediction correction.
use std::io;
use glam::Vec3;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use peakrunner_core::sim::{arsenal::{Loadout, MAX_SLOTS}, loadout::Loot, packs::PackKind, ArmorClass, Disc, Explosion, Phase, Player, SmokePuff, Snapshot, Team, MAX_PLAYERS};
use peakrunner_core::{conditions::Conditions, map_catalog::SupportedMode, terrain::MapId, vote::Vote};
use peakrunner_discovery::wire::invalid;

const POS: f32 = 1024.0;
const VEL: f32 = 64.0;
const K_POS: u32 = 4;
const K_VEL: u32 = 4;
const VITAL: f32 = 64.0;
const TIMER: f32 = 256.0;
const TURN: f32 = 65536.0 / std::f32::consts::TAU;
const SPIN: f32 = 16.0;
/// Ticks between a snapshot and its baseline are clamped to this in
/// predictions (both ends alike), which keeps the arithmetic bounded.
const MAX_GAP: u64 = 1 << 16;
/// How far ahead in the baseline a projectile's match is looked for.
const WINDOW: usize = 32;
/// How far ahead an identical item is looked for (effects expire from the
/// front of their lists).
const COPY_WINDOW: usize = 256;
/// Decoding caps: no list may claim more items than this.
const MAX_ITEMS: usize = 4096;
const MAX_ITEM_BYTES: usize = 4096;
/// Item bytes one snapshot may decode to, however they are copied or
/// patched (a real one holds a few tens of KB at most).
const MAX_DECODED: usize = 128 * 1024;

fn q(x: f32, scale: f32) -> i32 { (x * scale).round() as i32 }
fn dq(v: i32, scale: f32) -> f32 { v as f32 / scale }
fn q3(v: Vec3, scale: f32) -> [i32; 3] { [q(v.x, scale), q(v.y, scale), q(v.z, scale)] }
fn dq3(v: [i32; 3], scale: f32) -> Vec3 { Vec3::new(dq(v[0], scale), dq(v[1], scale), dq(v[2], scale)) }
fn yaw_q(yaw: f32) -> u16 { (yaw.rem_euclid(std::f32::consts::TAU) * TURN).round() as i64 as u16 }
fn pitch_q(pitch: f32) -> i16 { q(pitch, TURN).clamp(i16::MIN as i32, i16::MAX as i32) as i16 }

// ---- Bit stream -----------------------------------------------------------

#[derive(Default)]
struct Writer { out: Vec<u8>, acc: u64, bits: u32 }
impl Writer {
    /// The low `n` bits of `v` (n <= 32).
    fn put(&mut self, v: u64, n: u32) {
        self.acc |= (v & ((1u64 << n) - 1)) << self.bits;
        self.bits += n;
        while self.bits >= 8 { self.out.push(self.acc as u8); self.acc >>= 8; self.bits -= 8; }
    }
    fn bit(&mut self, b: bool) { self.put(b as u64, 1) }
    /// Exp-Golomb of order `k`: small values in few bits, any u64 allowed.
    fn eg(&mut self, v: u64, k: u32) {
        let w = v as u128 + (1u128 << k);
        let len = 128 - w.leading_zeros();
        for _ in 0..len - 1 - k { self.put(0, 1); }
        self.put(1, 1);
        let mut rest = len - 1;
        while rest > 0 { let n = rest.min(32); rest -= n; self.put((w >> rest) as u64, n); }
    }
    fn se(&mut self, v: i64, k: u32) { self.eg(((v << 1) ^ (v >> 63)) as u64, k) }
    /// A difference that is usually zero: one bit when it is.
    fn change(&mut self, v: i64, k: u32) { self.bit(v != 0); if v != 0 { self.se(v, k); } }
    fn bytes(&mut self, b: &[u8]) { for x in b { self.put(*x as u64, 8); } }
    fn finish(mut self) -> Vec<u8> { if self.bits > 0 { self.out.push(self.acc as u8); } self.out }
}

struct Reader<'a> { data: &'a [u8], pos: usize, budget: usize }
impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, pos: 0, budget: MAX_DECODED } }
    /// Count decoded item bytes against the snapshot's budget.
    fn spend(&mut self, bytes: usize) -> io::Result<()> {
        self.budget = self.budget.checked_sub(bytes).ok_or_else(|| invalid("snapshot expansion limit exceeded"))?;
        Ok(())
    }
    fn left(&self) -> usize { self.data.len() * 8 - self.pos }
    fn get(&mut self, n: u32) -> io::Result<u64> {
        if n as usize > self.left() { return Err(invalid("truncated snapshot")); }
        let mut v = 0u64;
        for i in 0..n as usize {
            let at = self.pos + i;
            v |= (((self.data[at / 8] >> (at % 8)) & 1) as u64) << i;
        }
        self.pos += n as usize;
        Ok(v)
    }
    fn bit(&mut self) -> io::Result<bool> { Ok(self.get(1)? == 1) }
    fn eg(&mut self, k: u32) -> io::Result<u64> {
        let mut zeros = 0;
        while !self.bit()? { zeros += 1; if zeros + k > 64 { return Err(invalid("invalid snapshot number")); } }
        let len = zeros + k;
        let mut w = 1u128;
        let mut rest = len;
        while rest > 0 { let n = rest.min(32); rest -= n; w = (w << n) | self.get(n)? as u128; }
        u64::try_from(w - (1u128 << k)).map_err(|_| invalid("invalid snapshot number"))
    }
    fn se(&mut self, k: u32) -> io::Result<i64> { let v = self.eg(k)?; Ok((v >> 1) as i64 ^ -((v & 1) as i64)) }
    /// A number no larger than `max`.
    fn upto(&mut self, k: u32, max: usize) -> io::Result<usize> {
        let n = self.eg(k)?;
        if n > max as u64 { return Err(invalid("snapshot number out of range")); }
        Ok(n as usize)
    }
    /// A list length: every item costs at least a bit, so a count can't
    /// outrun the data (nor reserve memory it doesn't pay for).
    fn count(&mut self, k: u32, max: usize) -> io::Result<usize> {
        let n = self.upto(k, max)?;
        if n > self.left() { return Err(invalid("snapshot list too long")); }
        Ok(n)
    }
    /// `base` plus a signed difference.
    fn plus(&mut self, base: i64, k: u32) -> io::Result<i64> { base.checked_add(self.se(k)?).ok_or_else(|| invalid("snapshot value out of range")) }
    /// `base` plus a `Writer::change`.
    fn changed(&mut self, base: i64, k: u32) -> io::Result<i64> { if self.bit()? { self.plus(base, k) } else { Ok(base) } }
    fn bytes(&mut self, n: usize) -> io::Result<Vec<u8>> {
        if n * 8 > self.left() { return Err(invalid("truncated snapshot")); }
        (0..n).map(|_| self.get(8).map(|b| b as u8)).collect()
    }
}

fn narrow<T: TryFrom<i64>>(v: i64) -> io::Result<T> { T::try_from(v).map_err(|_| invalid("snapshot value out of range")) }
fn narrow_u<T: TryFrom<u64>>(v: u64) -> io::Result<T> { T::try_from(v).map_err(|_| invalid("snapshot value out of range")) }

// ---- Players --------------------------------------------------------------

/// A player as sent: quantized, with small enums as numbers.
#[derive(Clone, Debug, Default, PartialEq)]
struct PlayerQ {
    net_id: u32, name: String, glacier: bool, is_bot: bool, remote: bool,
    /// frags, losses, shots, hits
    counters: [u32; 4],
    /// alive, on_ground, skiing, jetting, repair_out (was the rifle), jump_prev,
    /// jet_prev (the jets held last tick, `jumpjet1`), repair_self (`repair2`)
    flags: u8,
    pos: [i32; 3], vel: [i32; 3], yaw: u16, pitch: i16,
    /// health, energy
    vitals: [i32; 2],
    /// respawn, cooldown, throw_clock, stun, coyote, suicide_in
    timers: [i32; 6],
    /// The weapon held and the armor worn (wire numbers, `classes1`).
    weapon: u8, armor: u8,
    /// Rounds per weapon, by wire number.
    ammo: Vec<u16>,
    pack: u8, throwables: [u8; 2], throw_held: u8, carrying: u8,
    /// The weapons carried, slot 1 first (`Loadout::raw`).
    loadout: [u8; MAX_SLOTS],
    beam: Option<[i32; 3]>,
    /// The trigger (`firing`): spin, charge and lock time, then burst
    /// left, fire mode, the weapon the mode is for, the lock's target, and
    /// fire held last tick; and a held beam's end.
    trigger_timers: [i32; 3],
    trigger: [u16; 4],
    fire_prev: bool,
    stream: Option<[i32; 3]>,
    /// Status effects: each one's wire number and seconds left.
    effects: Vec<(u8, i32)>,
    /// The pack and the class key (`packs`): pack clock, class key clock,
    /// strain and a borrowed armor's seconds; then what is switched on (1
    /// the pack, 2 the class key), pack uses, class key uses and the armor
    /// worn before (+ 1).
    kit_timers: [i32; 4],
    kit: [u8; 4],
    /// Credits of their own (`items`), whole; the vehicle ridden (+ 1).
    credits: u32,
    riding: u8,
    /// The blade (`blade1`, `sim::blade::State`): swing, steps into it,
    /// power, fire held, the press, combo, combo steps left, next form,
    /// the combo joined, guard, reeling and resting steps, and the latest
    /// blow (count, what, whose).
    blade: [u16; BLADE_FIELDS],
}

const BLADE_FIELDS: usize = 15;
fn blade_q(b: &peakrunner_core::sim::blade::State) -> [u16; BLADE_FIELDS] {
    [b.swing as u16, b.t, b.power as u16, b.held, b.press as u16, b.chain as u16, b.window, b.next as u16, b.link as u16,
     b.guard as u16, b.reel, b.rest, b.struck as u16, b.how as u16, b.by as u16]
}
fn blade_dq(q: &[u16; BLADE_FIELDS]) -> peakrunner_core::sim::blade::State {
    let b8 = |v: u16| v.min(255) as u8;
    peakrunner_core::sim::blade::State { swing: b8(q[0]), t: q[1], power: b8(q[2]), held: q[3], press: b8(q[4]), chain: b8(q[5]),
        window: q[6], next: b8(q[7]), link: b8(q[8]), guard: q[9] != 0, reel: q[10], rest: q[11], struck: b8(q[12]), how: b8(q[13]),
        by: b8(q[14]), ..Default::default() }
}

fn team_code(t: Option<Team>) -> u8 { match t { None => 0, Some(Team::Ember) => 1, Some(Team::Glacier) => 2 } }
fn code_team(c: u8) -> Option<Team> { match c { 1 => Some(Team::Ember), 2 => Some(Team::Glacier), _ => None } }
/// A carried pack: 0 none, else its wire number + 1.
fn pack_code(k: Option<PackKind>) -> u8 { k.map_or(0, |k| k.0.saturating_add(1)) }
fn code_pack(c: u8) -> Option<PackKind> { (c > 0).then(|| PackKind(c - 1)) }
/// Most rounds entries a player may carry (one per weapon).
const AMMO_MAX: usize = 64;

impl PlayerQ {
    fn new(p: &Player) -> Self {
        let (jump_prev, jet_prev, coyote) = p.wire_private();
        let flags = [p.alive, p.on_ground, p.skiing, p.jetting, p.repair_out, jump_prev, jet_prev, p.repair_self].iter().enumerate()
            .fold(0u8, |f, (i, b)| f | (*b as u8) << i);
        Self { net_id: p.net_id, name: p.name.clone(), glacier: p.team == Team::Glacier, is_bot: p.is_bot, remote: p.remote,
            counters: [p.frags, p.losses, p.shots, p.hits], flags,
            pos: q3(p.pos, POS), vel: q3(p.vel, VEL), yaw: yaw_q(p.yaw), pitch: pitch_q(p.pitch),
            vitals: [q(p.health, VITAL), q(p.energy, VITAL)],
            timers: [p.respawn, p.cooldown, p.throw_clock, p.stun, coyote, p.suicide_in].map(|t| q(t, TIMER)),
            weapon: p.weapon, armor: p.armor.0, ammo: p.ammo.iter().take(AMMO_MAX).copied().collect(), pack: pack_code(p.pack),
            throwables: p.throwables, throw_held: p.throw_held, carrying: team_code(p.carrying), loadout: p.loadout.raw(),
            beam: p.repair_beam.map(|b| q3(b, POS)),
            trigger_timers: [p.spin, p.charge, p.lock_time].map(|t| q(t, TIMER)),
            trigger: [p.burst_left as u16, p.fire_mode as u16, p.mode_of as u16, p.lock_target],
            fire_prev: p.fire_prev, stream: p.stream.map(|b| q3(b, POS)),
            effects: p.effects.iter().take(peakrunner_core::sim::status::MAX_EFFECTS).map(|(k, t)| (*k, q(*t, TIMER))).collect(),
            kit_timers: [p.pack_clock, p.ability_clock, p.strain, p.swap_clock].map(|t| q(t, TIMER)),
            kit: [p.pack_on as u8 | (p.ability_on as u8) << 1 | (p.footing as u8) << 2, p.pack_uses, p.ability_uses, p.swap_back],
            credits: if p.credits.is_finite() { p.credits.clamp(0.0, u32::MAX as f32) as u32 } else { 0 }, riding: p.riding,
            blade: blade_q(&p.blade) }
    }
    fn player(&self) -> Player {
        let mut p = Player::wire_blank();
        p.net_id = self.net_id; p.name = self.name.clone();
        p.team = if self.glacier { Team::Glacier } else { Team::Ember };
        p.is_bot = self.is_bot; p.remote = self.remote;
        [p.frags, p.losses, p.shots, p.hits] = self.counters;
        let flag = |i: u32| self.flags >> i & 1 == 1;
        (p.alive, p.on_ground, p.skiing, p.jetting, p.repair_out, p.repair_self) = (flag(0), flag(1), flag(2), flag(3), flag(4), flag(7));
        p.pos = dq3(self.pos, POS); p.vel = dq3(self.vel, VEL);
        p.yaw = self.yaw as f32 / TURN; p.pitch = self.pitch as f32 / TURN;
        p.health = dq(self.vitals[0], VITAL); p.energy = dq(self.vitals[1], VITAL);
        let [respawn, cooldown, throw_clock, stun, coyote, suicide_in] = self.timers.map(|t| dq(t, TIMER));
        (p.respawn, p.cooldown, p.throw_clock, p.stun, p.suicide_in) = (respawn, cooldown, throw_clock, stun, suicide_in);
        p.set_wire_private(flag(5), flag(6), coyote);
        p.weapon = self.weapon; p.armor = ArmorClass(self.armor);
        p.ammo = self.ammo.clone(); p.pack = code_pack(self.pack); p.throwables = self.throwables; p.throw_held = self.throw_held;
        p.carrying = code_team(self.carrying);
        p.loadout = Loadout::from_raw(self.loadout).unwrap_or_default();
        p.repair_beam = self.beam.map(|b| dq3(b, POS));
        [p.spin, p.charge, p.lock_time] = self.trigger_timers.map(|t| dq(t, TIMER));
        p.burst_left = self.trigger[0] as u8; p.fire_mode = self.trigger[1] as u8; p.mode_of = self.trigger[2] as u8;
        p.lock_target = self.trigger[3];
        p.fire_prev = self.fire_prev;
        p.stream = self.stream.map(|b| dq3(b, POS));
        p.effects = self.effects.iter().map(|(k, t)| (*k, dq(*t, TIMER))).collect();
        [p.pack_clock, p.ability_clock, p.strain, p.swap_clock] = self.kit_timers.map(|t| dq(t, TIMER));
        (p.pack_on, p.ability_on, p.footing) = (self.kit[0] & 1 != 0, self.kit[0] & 2 != 0, self.kit[0] & 4 != 0);
        [_, p.pack_uses, p.ability_uses, p.swap_back] = self.kit;
        p.credits = self.credits as f32;
        p.riding = self.riding;
        p.blade = blade_dq(&self.blade);
        p
    }
}

/// Where `pos` would be `dt` ticks on at `vel` (both quantized).
fn carry(pos: i32, vel: i32, dt: i64) -> i64 { pos as i64 + vel as i64 * dt * (POS / VEL) as i64 / 60 }

fn put_player(w: &mut Writer, c: &PlayerQ, b: &PlayerQ, dt: i64) {
    let groups = [
        c.net_id != b.net_id || c.name != b.name || c.glacier != b.glacier || c.is_bot != b.is_bot || c.remote != b.remote,
        c.counters != b.counters,
        c.flags != b.flags,
        c.pos != b.pos || c.vel != b.vel,
        c.yaw != b.yaw || c.pitch != b.pitch,
        c.vitals != b.vitals,
        c.timers != b.timers,
        c.weapon != b.weapon || c.armor != b.armor || c.ammo != b.ammo || c.pack != b.pack || c.throwables != b.throwables
            || c.throw_held != b.throw_held || c.carrying != b.carrying || c.loadout != b.loadout,
        c.beam != b.beam,
        c.trigger_timers != b.trigger_timers || c.trigger != b.trigger || c.fire_prev != b.fire_prev || c.stream != b.stream,
        c.effects != b.effects,
        c.kit_timers != b.kit_timers || c.kit != b.kit || c.credits != b.credits || c.riding != b.riding,
        c.blade != b.blade,
    ];
    if !groups.contains(&true) { w.bit(false); return; }
    w.bit(true);
    for g in groups { w.bit(g); }
    if groups[0] {
        w.eg(c.net_id as u64, 4);
        w.eg(c.name.len() as u64, 3); w.bytes(c.name.as_bytes());
        w.bit(c.glacier); w.bit(c.is_bot); w.bit(c.remote);
    }
    if groups[1] { for i in 0..4 { w.change(c.counters[i] as i64 - b.counters[i] as i64, 0); } }
    if groups[2] { w.put(c.flags as u64, 8); }
    if groups[3] {
        for i in 0..3 { w.se(c.pos[i] as i64 - carry(b.pos[i], b.vel[i], dt), K_POS); }
        for i in 0..3 { w.se(c.vel[i] as i64 - b.vel[i] as i64, K_VEL); }
    }
    if groups[4] {
        w.change(c.yaw.wrapping_sub(b.yaw) as i16 as i64, 6);
        w.change(c.pitch as i64 - b.pitch as i64, 5);
    }
    if groups[5] { for i in 0..2 { w.change(c.vitals[i] as i64 - b.vitals[i] as i64, 4); } }
    if groups[6] { for i in 0..6 { w.change(c.timers[i] as i64 - b.timers[i] as i64, 4); } }
    if groups[7] {
        w.eg(c.weapon as u64, 1); w.eg(c.armor as u64, 0);
        w.eg(c.ammo.len() as u64, 2);
        for (i, n) in c.ammo.iter().enumerate() { w.change(*n as i64 - b.ammo.get(i).copied().unwrap_or(0) as i64, 2); }
        w.eg(c.pack as u64, 0);
        let slots = c.loadout.iter().take_while(|&&k| k != u8::MAX).count();
        w.eg(slots as u64, 1);
        for k in &c.loadout[..slots] { w.eg(*k as u64, 2); }
        for t in c.throwables { w.eg(t as u64, 1); }
        w.eg(c.throw_held as u64, 0); w.eg(c.carrying as u64, 0);
    }
    if groups[8] {
        w.bit(c.beam.is_some());
        if let Some(beam) = c.beam { for (b, p) in beam.iter().zip(c.pos) { w.se(*b as i64 - p as i64, 8); } }
    }
    if groups[9] {
        for i in 0..3 { w.change(c.trigger_timers[i] as i64 - b.trigger_timers[i] as i64, 4); }
        for v in c.trigger { w.eg(v as u64, 0); }
        w.bit(c.fire_prev);
        w.bit(c.stream.is_some());
        if let Some(s) = c.stream { for (b, p) in s.iter().zip(c.pos) { w.se(*b as i64 - p as i64, 8); } }
    }
    if groups[10] {
        w.eg(c.effects.len() as u64, 0);
        for (k, t) in &c.effects { w.eg(*k as u64, 2); w.se(*t as i64, 8); }
    }
    if groups[11] {
        for i in 0..4 { w.change(c.kit_timers[i] as i64 - b.kit_timers[i] as i64, 4); }
        for v in c.kit { w.eg(v as u64, 0); }
        w.eg(c.credits as u64, 6);
        w.eg(c.riding as u64, 0);
    }
    if groups[12] { for i in 0..BLADE_FIELDS { w.change(c.blade[i] as i64 - b.blade[i] as i64, 1); } }
}

fn get_player(r: &mut Reader, b: &PlayerQ, dt: i64) -> io::Result<PlayerQ> {
    let mut c = b.clone();
    if !r.bit()? { return Ok(c); }
    let mut groups = [false; 13];
    for g in &mut groups { *g = r.bit()?; }
    if groups[0] {
        c.net_id = narrow_u(r.eg(4)?)?;
        let len = r.upto(3, 255)?;
        c.name = String::from_utf8(r.bytes(len)?).map_err(|_| invalid("invalid player name"))?;
        c.glacier = r.bit()?; c.is_bot = r.bit()?; c.remote = r.bit()?;
    }
    if groups[1] { for i in 0..4 { c.counters[i] = narrow(r.changed(b.counters[i] as i64, 0)?)?; } }
    if groups[2] { c.flags = r.get(8)? as u8; }
    if groups[3] {
        for i in 0..3 { c.pos[i] = narrow(r.plus(carry(b.pos[i], b.vel[i], dt), K_POS)?)?; }
        for i in 0..3 { c.vel[i] = narrow(r.plus(b.vel[i] as i64, K_VEL)?)?; }
    }
    if groups[4] {
        c.yaw = b.yaw.wrapping_add(narrow::<i16>(r.changed(0, 6)?)? as u16);
        c.pitch = narrow(r.changed(b.pitch as i64, 5)?)?;
    }
    if groups[5] { for i in 0..2 { c.vitals[i] = narrow(r.changed(b.vitals[i] as i64, 4)?)?; } }
    if groups[6] { for i in 0..6 { c.timers[i] = narrow(r.changed(b.timers[i] as i64, 4)?)?; } }
    if groups[7] {
        c.weapon = narrow_u(r.eg(1)?)?; c.armor = narrow_u(r.eg(0)?)?;
        let n = r.upto(2, AMMO_MAX)?;
        c.ammo = (0..n).map(|i| narrow(r.changed(b.ammo.get(i).copied().unwrap_or(0) as i64, 2)?)).collect::<io::Result<_>>()?;
        c.pack = narrow_u(r.eg(0)?)?;
        let slots = r.upto(1, MAX_SLOTS)?;
        c.loadout = [u8::MAX; MAX_SLOTS];
        for k in &mut c.loadout[..slots] { *k = narrow_u(r.eg(2)?)?; }
        if Loadout::from_raw(c.loadout).is_none() { return Err(invalid("invalid loadout")); }
        for t in &mut c.throwables { *t = narrow_u(r.eg(1)?)?; }
        c.throw_held = narrow_u(r.eg(0)?)?; c.carrying = narrow_u(r.eg(0)?)?;
        if c.carrying > 2 { return Err(invalid("invalid flag carrier")); }
    }
    if groups[8] {
        c.beam = None;
        if r.bit()? {
            let mut beam = [0; 3];
            for (b, p) in beam.iter_mut().zip(c.pos) { *b = narrow(r.plus(p as i64, 8)?)?; }
            c.beam = Some(beam);
        }
    }
    if groups[9] {
        for i in 0..3 { c.trigger_timers[i] = narrow(r.changed(b.trigger_timers[i] as i64, 4)?)?; }
        for v in &mut c.trigger { *v = narrow_u(r.eg(0)?)?; }
        if c.trigger[0] > 255 || c.trigger[1] > 255 || c.trigger[2] > 255 { return Err(invalid("invalid trigger")); }
        c.fire_prev = r.bit()?;
        c.stream = None;
        if r.bit()? {
            let mut s = [0; 3];
            for (b, p) in s.iter_mut().zip(c.pos) { *b = narrow(r.plus(p as i64, 8)?)?; }
            c.stream = Some(s);
        }
    }
    if groups[10] {
        let n = r.upto(0, peakrunner_core::sim::status::MAX_EFFECTS)?;
        c.effects = (0..n).map(|_| -> io::Result<(u8, i32)> { Ok((narrow_u(r.eg(2)?)?, narrow(r.plus(0, 8)?)?)) }).collect::<io::Result<_>>()?;
    }
    if groups[11] {
        for i in 0..4 { c.kit_timers[i] = narrow(r.changed(b.kit_timers[i] as i64, 4)?)?; }
        for v in &mut c.kit { *v = narrow_u(r.eg(0)?)?; }
        c.credits = narrow_u(r.eg(6)?)?;
        c.riding = narrow_u(r.eg(0)?)?;
    }
    if groups[12] { for i in 0..BLADE_FIELDS { c.blade[i] = narrow(r.changed(b.blade[i] as i64, 1)?)?; } }
    Ok(c)
}

// ---- Projectiles ----------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
struct DiscQ { pos: [i32; 3], vel: [i32; 3], glacier: bool, owner: u32, life: i32, kind: u8, spin: i32 }
impl DiscQ {
    fn new(d: &Disc) -> Self {
        Self { pos: q3(d.pos, POS), vel: q3(d.vel, VEL), glacier: d.team == Team::Glacier, owner: u32::try_from(d.owner).unwrap_or(u32::MAX),
            life: q(d.life, TIMER), kind: d.kind, spin: q(d.spin, SPIN) }
    }
    fn disc(&self) -> Disc {
        Disc { pos: dq3(self.pos, POS), vel: dq3(self.vel, VEL), team: if self.glacier { Team::Glacier } else { Team::Ember },
            owner: self.owner as usize, life: dq(self.life, TIMER), kind: self.kind, spin: dq(self.spin, SPIN), target: 0, power: 1.0 }
    }
    /// This projectile carried on `dt` ticks, straight or `falling`:
    /// position, velocity, life, and spin if it spins on. Only a guess to
    /// code the difference from (the sim's gravity is 20 m/s² in 240 Hz
    /// steps; flying projectiles spin at 42 rad/s).
    fn carried(&self, dt: i64, falling: bool) -> ([i64; 3], [i64; 3], i64, i64) {
        let mut pos = [0, 1, 2].map(|i| carry(self.pos[i], self.vel[i], dt));
        let mut vel = self.vel.map(|v| v as i64);
        if falling {
            let steps = dt * 4;
            pos[1] -= steps * (steps + 1) * 16 / 90;
            vel[1] -= dt * 64 / 3;
        }
        (pos, vel, self.life as i64 - dt * 256 / 60, self.spin as i64 + dt * 672 / 60)
    }
}

/// Where a new projectile's position is coded from: its owner's position.
fn origin(players: &[PlayerQ], owner: u32) -> [i32; 3] { players.get(owner as usize).map_or([0; 3], |p| p.pos) }

fn put_discs(w: &mut Writer, cur: &[DiscQ], base: &[DiscQ], players: &[PlayerQ], dt: i64) {
    w.eg(cur.len() as u64, 4);
    let mut cursor = 0;
    for d in cur {
        // Projectiles keep firing order, so a surviving one is further on
        // in the baseline: the same shooter's, where its flight put it.
        let found = (cursor..base.len().min(cursor + WINDOW)).find(|&j| {
            let b = &base[j];
            let (pos, ..) = b.carried(dt, false);
            b.glacier == d.glacier && b.owner == d.owner && b.kind == d.kind && (0..3).all(|i| (d.pos[i] as i64 - pos[i]).abs() < 4096)
        });
        match found {
            Some(j) => {
                let b = &base[j];
                // Straight or falling, whichever guessed better.
                let miss = |falling: bool| { let (pos, vel, ..) = b.carried(dt, falling);
                    (0..3).map(|i| (d.pos[i] as i64 - pos[i]).abs() + (d.vel[i] as i64 - vel[i]).abs()).sum::<i64>() };
                let falling = miss(true) < miss(false);
                let (pos, vel, life, spin) = b.carried(dt, falling);
                w.bit(true); w.eg((j - cursor) as u64, 0); w.bit(falling);
                for (x, p) in d.pos.iter().zip(pos) { w.se(*x as i64 - p, 1); }
                for (x, v) in d.vel.iter().zip(vel) { w.change(*x as i64 - v, 3); }
                w.se(d.life as i64 - life, 0);
                // Spinning on, or lying still.
                let spinning = (d.spin as i64 - spin).abs() < (d.spin as i64 - b.spin as i64).abs();
                w.bit(spinning);
                w.change(d.spin as i64 - if spinning { spin } else { b.spin as i64 }, 2);
                cursor = j + 1;
            }
            None => {
                w.bit(false);
                w.bit(d.glacier); w.eg(d.owner as u64, 2); w.eg(d.kind as u64, 1);
                let o = origin(players, d.owner);
                for (x, o) in d.pos.iter().zip(o) { w.se(*x as i64 - o as i64, 10); }
                for i in 0..3 { w.se(d.vel[i] as i64, 8); }
                w.se(d.life as i64, 8);
                w.se(d.spin as i64, 4);
            }
        }
    }
}

fn get_discs(r: &mut Reader, base: &[DiscQ], players: &[PlayerQ], dt: i64) -> io::Result<Vec<DiscQ>> {
    let n = r.count(4, MAX_ITEMS)?;
    let mut out = Vec::with_capacity(n.min(256));
    let mut cursor = 0;
    for _ in 0..n {
        if r.bit()? {
            let j = cursor + r.upto(0, WINDOW)?;
            let b = base.get(j).ok_or_else(|| invalid("invalid projectile reference"))?;
            let (pos, vel, life, spin) = b.carried(dt, r.bit()?);
            let mut d = b.clone();
            for (x, p) in d.pos.iter_mut().zip(pos) { *x = narrow(r.plus(p, 1)?)?; }
            for (x, v) in d.vel.iter_mut().zip(vel) { *x = narrow(r.changed(v, 3)?)?; }
            d.life = narrow(r.plus(life, 0)?)?;
            let spinning = r.bit()?;
            d.spin = narrow(r.changed(if spinning { spin } else { b.spin as i64 }, 2)?)?;
            out.push(d);
            cursor = j + 1;
        } else {
            let glacier = r.bit()?;
            let owner = narrow_u(r.eg(2)?)?;
            let kind = narrow_u(r.eg(1)?)?;
            let o = origin(players, owner);
            let mut d = DiscQ { glacier, owner, kind, ..DiscQ::default() };
            for (x, o) in d.pos.iter_mut().zip(o) { *x = narrow(r.plus(o as i64, 10)?)?; }
            for i in 0..3 { d.vel[i] = narrow(r.se(8)?)?; }
            d.life = narrow(r.se(8)?)?;
            d.spin = narrow(r.se(4)?)?;
            out.push(d);
        }
    }
    Ok(out)
}

// ---- Everything else: postcard items ------------------------------------

/// A byte patch: runs of equal bytes, then the differing ones.
fn put_patch(w: &mut Writer, c: &[u8], b: &[u8]) {
    let mut i = 0;
    while i < c.len() {
        let same = c[i..].iter().zip(&b[i..]).take_while(|(x, y)| x == y).count();
        w.eg(same as u64, 2); i += same;
        if i == c.len() { break; }
        let differ = c[i..].iter().zip(&b[i..]).take_while(|(x, y)| x != y).count();
        w.eg(differ as u64 - 1, 0); w.bytes(&c[i..i + differ]); i += differ;
    }
}
fn get_patch(r: &mut Reader, b: &[u8]) -> io::Result<Vec<u8>> {
    let mut c = b.to_vec();
    let mut i = 0;
    while i < c.len() {
        i += r.upto(2, c.len() - i)?;
        if i == c.len() { break; }
        let differ = r.upto(0, c.len() - i - 1)? + 1;
        c[i..i + differ].copy_from_slice(&r.bytes(differ)?);
        i += differ;
    }
    Ok(c)
}

/// A list of items: each one either copied from further on in the baseline,
/// patched from a nearby baseline item or the item before it (new effects
/// often resemble each other), or sent as it is.
fn put_items(w: &mut Writer, cur: &[Vec<u8>], base: &[Vec<u8>]) {
    w.eg(cur.len() as u64, 2);
    let mut cursor = 0;
    for (i, c) in cur.iter().enumerate() {
        if let Some(j) = (cursor..base.len().min(cursor + COPY_WINDOW)).find(|&j| base[j] == *c) {
            w.bit(true); w.eg((j - cursor) as u64, 0); cursor = j + 1; continue;
        }
        w.bit(false);
        let differ = |b: &Vec<u8>| if b.len() == c.len() { b.iter().zip(c).filter(|(x, y)| x != y).count() } else { usize::MAX };
        // Source 0 is the previous item, s + 1 the baseline's cursor + s.
        let previous = i.checked_sub(1).map(|p| (0, differ(&cur[p])));
        let near = previous.into_iter().chain((cursor..base.len().min(cursor + 4)).map(|j| (j - cursor + 1, differ(&base[j]))))
            .min_by_key(|(_, d)| *d).filter(|(_, d)| d.saturating_mul(2) <= c.len());
        match near {
            Some((0, _)) => { w.bit(true); w.eg(0, 0); put_patch(w, c, &cur[i - 1]); }
            Some((s, _)) => { w.bit(true); w.eg(s as u64, 0); put_patch(w, c, &base[cursor + s - 1]); cursor += s; }
            None => { w.bit(false); w.eg(c.len() as u64, 4); w.bytes(c); }
        }
    }
}
fn get_items(r: &mut Reader, base: &[Vec<u8>]) -> io::Result<Vec<Vec<u8>>> {
    let n = r.count(2, MAX_ITEMS)?;
    let mut out = Vec::with_capacity(n.min(256));
    let mut cursor = 0;
    for _ in 0..n {
        if r.bit()? {
            let j = cursor + r.upto(0, COPY_WINDOW)?;
            out.push(base.get(j).ok_or_else(|| invalid("invalid snapshot item reference"))?.clone());
            cursor = j + 1;
        } else if r.bit()? {
            let item = match r.upto(0, 4)? {
                0 => get_patch(r, out.last().ok_or_else(|| invalid("invalid snapshot item reference"))?)?,
                s => { let b = base.get(cursor + s - 1).ok_or_else(|| invalid("invalid snapshot item reference"))?; cursor += s; get_patch(r, b)? }
            };
            out.push(item);
        } else {
            let len = r.upto(4, MAX_ITEM_BYTES)?;
            out.push(r.bytes(len)?);
        }
        r.spend(out.last().map_or(0, Vec::len))?;
    }
    Ok(out)
}

fn items<T: Serialize>(list: &[T]) -> Vec<Vec<u8>> { list.iter().map(|x| postcard::to_allocvec(x).expect("snapshot items serialize")).collect() }
fn parse<T: DeserializeOwned>(bytes: &[u8]) -> io::Result<T> { postcard::from_bytes(bytes).map_err(|_| invalid("invalid snapshot item")) }
fn parse_all<T: DeserializeOwned>(list: &[Vec<u8>]) -> io::Result<Vec<T>> { list.iter().map(|b| parse(b)).collect() }

/// Effects and loot only age, so they are sent by the tick they appeared
/// (and smoke by where it appeared; it rises 0.35 m/s, as in `sim`): the
/// same one is then byte-identical in every snapshot and is copied, not
/// re-sent. Their ages come back to the tick (a sixtieth of a second).
fn born(tick: u64, age: f32) -> u64 { tick.saturating_sub((age * 60.0).round() as u64) }
fn age(tick: u64, born: u64) -> f32 { tick.saturating_sub(born) as f32 / 60.0 }
const SMOKE_RISE: f32 = 0.35;
const SMOKE: f32 = 64.0;
#[derive(Serialize, Deserialize)]
struct ExplosionW { born: u64, pos: Vec3, max_r: f32, kind: u8 }
/// A smoke puff by when and where it appeared (1/64 m).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SmokeQ { born: u64, pos: [i32; 3] }

/// Smoke expires from the front and is added at the back (a trail puff per
/// grenade in flight per tick, up to 256): the survivors are one run of the
/// baseline. A new puff is where a grenade in this snapshot was when it
/// appeared, so it is coded from that grenade flown back, taking the
/// grenades in turn; failing that, from the puff before it.
fn put_smoke(w: &mut Writer, cur: &[SmokeQ], base: &[SmokeQ], discs: &[DiscQ], tick: u64) {
    w.eg(cur.len() as u64, 4);
    let dropped = cur.first().and_then(|c| base.iter().position(|b| b == c)).unwrap_or(base.len());
    let kept = cur.iter().zip(&base[dropped..]).take_while(|(c, b)| c == b).count();
    w.eg(dropped as u64, 2); w.eg(kept as u64, 4);
    let grenades: Vec<&DiscQ> = discs.iter().filter(|d| peakrunner_core::registry::registry().smokes(d.kind)).collect();
    let mut previous = if kept > 0 { cur[kept - 1] } else { SmokeQ::default() };
    let mut next = 0;
    for c in &cur[kept..] {
        w.se(c.born as i64 - previous.born as i64, 1);
        let from = |g: &DiscQ| trail(g, tick, c.born);
        let off = |p: [i64; 3]| (0..3).map(|i| (c.pos[i] as i64 - p[i]).abs()).max().unwrap_or(0);
        let best = (0..grenades.len()).min_by_key(|&i| (off(from(grenades[i])), (i + grenades.len() - next) % grenades.len()))
            .filter(|&i| off(from(grenades[i])) < 256);
        w.bit(best.is_some());
        match best {
            Some(i) => {
                w.se(i as i64 - next as i64, 0);
                let p = from(grenades[i]);
                for (x, p) in c.pos.iter().zip(p) { w.se(*x as i64 - p, 3); }
                next = (i + 1) % grenades.len();
            }
            None => for k in 0..3 { w.se(c.pos[k] as i64 - previous.pos[k] as i64, 7); },
        }
        previous = *c;
    }
}
fn get_smoke(r: &mut Reader, base: &[SmokeQ], discs: &[DiscQ], tick: u64) -> io::Result<Vec<SmokeQ>> {
    // Survivors cost nothing; every new puff costs bits.
    let n = r.upto(4, MAX_ITEMS)?;
    let dropped = r.upto(2, base.len())?;
    let kept = r.upto(4, (base.len() - dropped).min(n))?;
    if n - kept > r.left() { return Err(invalid("snapshot list too long")); }
    let grenades: Vec<&DiscQ> = discs.iter().filter(|d| peakrunner_core::registry::registry().smokes(d.kind)).collect();
    let mut out = base[dropped..dropped + kept].to_vec();
    let mut previous = out.last().copied().unwrap_or_default();
    let mut next = 0i64;
    for _ in kept..n {
        let born = narrow_u(r.plus(previous.born as i64, 1)? as u64)?;
        let mut pos = [0; 3];
        if r.bit()? {
            let i = r.plus(next, 0)?;
            let g = usize::try_from(i).ok().and_then(|i| grenades.get(i)).ok_or_else(|| invalid("invalid smoke reference"))?;
            let p = trail(g, tick, born);
            for k in 0..3 { pos[k] = narrow(r.plus(p[k], 3)?)?; }
            next = (i + 1) % grenades.len() as i64;
        } else {
            for (x, p) in pos.iter_mut().zip(previous.pos) { *x = narrow(r.plus(p as i64, 7)?)?; }
        }
        previous = SmokeQ { born, pos };
        out.push(previous);
    }
    Ok(out)
}
/// Where a smoking round `g` (`Registry::smokes`; the base game's grenade
/// shell) was at tick `born`, in smoke units.
fn trail(g: &DiscQ, tick: u64, born: u64) -> [i64; 3] {
    let back = tick.saturating_sub(born).min(MAX_GAP) as i64;
    [0, 1, 2].map(|i| (g.pos[i] as i64 * SMOKE as i64 / POS as i64) - g.vel[i] as i64 * back * SMOKE as i64 / VEL as i64 / 60)
}

#[derive(Serialize, Deserialize)]
struct LootW { born: u64, pos: Vec3, ammo: Vec<u16>, grenades: u8, item: u8, weapon: u8, pack: u8, owner: u8, carried: bool }

/// Match-wide state, sent as one item.
#[derive(Serialize, Deserialize)]
struct Misc {
    blast_serial: u64, round: u32, phase: Phase, phase_left: f32, map: MapId, score: [u32; 2], time_left: f32,
    mode: SupportedMode, conditions: Conditions, vote: Option<Vote>, vote_wait: f32,
    /// The viewer's team's waypoint, marks and credits.
    waypoint: Option<Vec3>,
    marked: Vec<u8>,
    marked_spots: Vec<Vec3>,
    credits: f32,
    /// The map's moveables' openness and items' clocks.
    moveables: Vec<f32>,
    map_items: Vec<f32>,
}

/// Item lists, in wire order.
const LISTS: usize = 9;

// ---- Prepared snapshots ---------------------------------------------------

/// A snapshot in wire form. Built once per view on the server
/// (`new`), or decoded on the client; the two are identical.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prepared {
    pub tick: u64,
    players: Vec<PlayerQ>,
    acks: Vec<u64>,
    pings: Vec<(u32, u32)>,
    discs: Vec<DiscQ>,
    /// Match state, both flags, the ball, the rabbit flag.
    fixed: Vec<Vec<u8>>,
    smoke: Vec<SmokeQ>,
    /// Feed, equipment, control points, deployables, loot, explosions, vote choices, spectators, vehicles.
    lists: Vec<Vec<Vec<u8>>>,
}

impl Prepared {
    pub fn new(s: &Snapshot) -> Self {
        let misc = Misc { blast_serial: s.blast_serial, round: s.round, phase: s.phase, phase_left: s.phase_left, map: s.map,
            score: s.score, time_left: s.time_left, mode: s.mode, conditions: s.conditions, vote: s.vote.clone(), vote_wait: s.vote_wait,
            waypoint: s.waypoint, marked: s.marked.clone(), marked_spots: s.marked_spots.clone(), credits: s.credits, moveables: s.moveables.clone(), map_items: s.map_items.clone() };
        let fixed = vec![postcard::to_allocvec(&misc).expect("snapshot state serializes"),
            postcard::to_allocvec(&s.flags[0]).expect("flag"), postcard::to_allocvec(&s.flags[1]).expect("flag"),
            postcard::to_allocvec(&s.ball).expect("ball"), postcard::to_allocvec(&s.rabbit).expect("rabbit flag")];
        let t = s.tick;
        let loot: Vec<_> = s.loot.iter().map(|l| LootW { born: born(t, l.age), pos: l.pos, ammo: l.ammo.clone(), grenades: l.grenades,
            item: l.item, weapon: l.weapon, pack: l.pack, owner: l.owner, carried: l.carried }).collect();
        let explosions: Vec<_> = s.explosions.iter().map(|e| ExplosionW { born: born(t, e.age), pos: e.pos, max_r: e.max_r, kind: e.kind }).collect();
        let smoke = s.smoke.iter().map(|p| SmokeQ { born: born(t, p.age), pos: q3(p.pos - Vec3::Y * SMOKE_RISE * p.age, SMOKE) }).collect();
        let lists = vec![items(&s.feed), items(&s.equipment), items(&s.points), items(&s.deployables), items(&loot),
            items(&explosions), items(&s.vote_choices), items(&s.spectators), items(&s.vehicles)];
        Self { tick: s.tick, players: s.players.iter().map(PlayerQ::new).collect(), acks: s.acks.clone(), pings: s.pings.clone(),
            discs: s.discs.iter().map(DiscQ::new).collect(), smoke, fixed, lists }
    }

    /// The game's snapshot. Fails only on corrupt items.
    pub fn to_snapshot(&self) -> io::Result<Snapshot> {
        let misc: Misc = parse(&self.fixed[0])?;
        let l = &self.lists;
        let t = self.tick;
        let loot = parse_all::<LootW>(&l[4])?.into_iter().map(|w| Loot { pos: w.pos, ammo: w.ammo, age: age(t, w.born), grenades: w.grenades,
            item: w.item, weapon: w.weapon, pack: w.pack, owner: w.owner, carried: w.carried }).collect();
        let explosions = parse_all::<ExplosionW>(&l[5])?.into_iter().map(|w| Explosion { pos: w.pos, age: age(t, w.born), max_r: w.max_r, kind: w.kind }).collect();
        let smoke = self.smoke.iter().map(|w| { let a = age(t, w.born); SmokePuff { pos: dq3(w.pos, SMOKE) + Vec3::Y * SMOKE_RISE * a, age: a } }).collect();
        Ok(Snapshot { pings: self.pings.clone(), feed: parse_all(&l[0])?, equipment: parse_all(&l[1])?, blast_serial: misc.blast_serial,
            tick: self.tick, round: misc.round, phase: misc.phase, phase_left: misc.phase_left, map: misc.map,
            players: self.players.iter().map(PlayerQ::player).collect(), acks: self.acks.clone(),
            discs: self.discs.iter().map(DiscQ::disc).collect(), explosions, smoke,
            flags: [parse(&self.fixed[1])?, parse(&self.fixed[2])?], score: misc.score, time_left: misc.time_left, mode: misc.mode,
            points: parse_all(&l[2])?, ball: parse(&self.fixed[3])?, rabbit: parse(&self.fixed[4])?, deployables: parse_all(&l[3])?, loot,
            conditions: misc.conditions, vote: misc.vote, vote_choices: parse_all(&l[6])?, vote_wait: misc.vote_wait, spectators: parse_all(&l[7])?,
            waypoint: misc.waypoint, marked: misc.marked, marked_spots: misc.marked_spots, credits: misc.credits, moveables: misc.moveables, map_items: misc.map_items, vehicles: parse_all(&l[8])? })
    }

    /// Bit-packed, in full or as a delta against `base`.
    pub fn encode(&self, base: Option<&Prepared>) -> Vec<u8> {
        let mut w = Writer::default();
        let dt = base.map_or(0, |b| self.tick.saturating_sub(b.tick).min(MAX_GAP) as i64);
        let blank = PlayerQ::default();
        w.eg(self.players.len() as u64, 0);
        for (i, c) in self.players.iter().enumerate() {
            put_player(&mut w, c, base.and_then(|b| b.players.get(i)).unwrap_or(&blank), dt);
        }
        w.eg(self.acks.len() as u64, 0);
        for (i, a) in self.acks.iter().enumerate() {
            w.se(a.wrapping_sub(base.and_then(|b| b.acks.get(i).copied()).unwrap_or(0)) as i64, 3);
        }
        w.eg(self.pings.len() as u64, 2);
        for (i, (id, rtt)) in self.pings.iter().enumerate() {
            match base.and_then(|b| b.pings.get(i)).filter(|(b, _)| b == id) {
                Some((_, b)) => { w.bit(true); w.se(*rtt as i64 - *b as i64, 2); }
                None => { w.bit(false); w.eg(*id as u64, 4); w.eg(*rtt as u64, 5); }
            }
        }
        put_discs(&mut w, &self.discs, base.map_or(&[], |b| &b.discs), &self.players, dt);
        put_items(&mut w, &self.fixed, base.map_or(&[], |b| &b.fixed));
        put_smoke(&mut w, &self.smoke, base.map_or(&[], |b| &b.smoke), &self.discs, self.tick);
        for (i, list) in self.lists.iter().enumerate() { put_items(&mut w, list, base.map_or(&[], |b| &b.lists[i])); }
        w.finish()
    }

    /// The `Prepared` that `encode` was given, from its bytes and the same `base`.
    pub fn decode(tick: u64, bytes: &[u8], base: Option<&Prepared>) -> io::Result<Self> {
        let mut r = Reader::new(bytes);
        let dt = base.map_or(0, |b| tick.saturating_sub(b.tick).min(MAX_GAP) as i64);
        let blank = PlayerQ::default();
        if r.count(0, MAX_PLAYERS)? != MAX_PLAYERS { return Err(invalid("invalid snapshot player count")); }
        let players = (0..MAX_PLAYERS).map(|i| get_player(&mut r, base.and_then(|b| b.players.get(i)).unwrap_or(&blank), dt))
            .collect::<io::Result<Vec<_>>>()?;
        if r.count(0, MAX_PLAYERS)? != MAX_PLAYERS { return Err(invalid("invalid snapshot ack count")); }
        let acks = (0..MAX_PLAYERS).map(|i| Ok(base.and_then(|b| b.acks.get(i).copied()).unwrap_or(0).wrapping_add(r.se(3)? as u64)))
            .collect::<io::Result<Vec<_>>>()?;
        let n = r.count(2, MAX_ITEMS)?;
        let mut pings = Vec::with_capacity(n.min(64));
        for i in 0..n {
            if r.bit()? {
                let (id, b) = base.and_then(|b| b.pings.get(i)).copied().ok_or_else(|| invalid("invalid ping reference"))?;
                pings.push((id, narrow(r.plus(b as i64, 2)?)?));
            } else {
                pings.push((narrow_u(r.eg(4)?)?, narrow_u(r.eg(5)?)?));
            }
        }
        let discs = get_discs(&mut r, base.map_or(&[], |b| &b.discs), &players, dt)?;
        let fixed = get_items(&mut r, base.map_or(&[], |b| &b.fixed))?;
        if fixed.len() != 5 { return Err(invalid("invalid snapshot state")); }
        let smoke = get_smoke(&mut r, base.map_or(&[], |b| &b.smoke), &discs, tick)?;
        let lists = (0..LISTS).map(|i| get_items(&mut r, base.map_or(&[], |b| &b.lists[i]))).collect::<io::Result<Vec<_>>>()?;
        Ok(Self { tick, players, acks, pings, discs, smoke, fixed, lists })
    }
}

// ---- The recipient's own player, exactly ---------------------------------

/// The floats a client's prediction starts from, in wire order.
fn own_floats(p: &Player) -> [f32; 15] {
    [p.pos.x, p.pos.y, p.pos.z, p.vel.x, p.vel.y, p.vel.z, p.yaw, p.pitch, p.health, p.energy,
        p.respawn, p.cooldown, p.throw_clock, p.stun, p.wire_private().2]
}
fn set_own_floats(p: &mut Player, f: [f32; 15]) {
    p.pos = Vec3::new(f[0], f[1], f[2]); p.vel = Vec3::new(f[3], f[4], f[5]);
    (p.yaw, p.pitch, p.health, p.energy, p.respawn, p.cooldown, p.throw_clock, p.stun) = (f[6], f[7], f[8], f[9], f[10], f[11], f[12], f[13]);
    let (jump_prev, jet_prev, _) = p.wire_private();
    p.set_wire_private(jump_prev, jet_prev, f[14]);
}
/// Floats in an order where neighbours differ by one (-0.0 below +0.0).
fn ordered(x: f32) -> i64 {
    let bits = x.to_bits();
    if bits >> 31 == 1 { -((bits & 0x7fff_ffff) as i64) - 1 } else { bits as i64 }
}
fn unordered(o: i64) -> io::Result<f32> {
    let magnitude = if o < 0 { -(o + 1) } else { o };
    if magnitude > 0x7fff_ffff { return Err(invalid("invalid own state")); }
    Ok(f32::from_bits(magnitude as u32 | if o < 0 { 0x8000_0000 } else { 0 }))
}

/// Trailer for the player in `slot`: how its exact floats differ from the
/// quantized ones in `prepared` (usually by a few units in the last place).
pub fn own_state(prepared: &Prepared, slot: usize, exact: &Player) -> Vec<u8> {
    let mut w = Writer::default();
    w.eg(slot as u64, 2);
    let quantized = own_floats(&prepared.players[slot].player());
    for (x, q) in own_floats(exact).iter().zip(quantized) { w.se(ordered(*x) - ordered(q), 3); }
    w.finish()
}

/// Put the exact own state from `own_state` into a decoded snapshot.
pub fn apply_own(snapshot: &mut Snapshot, trailer: &[u8]) -> io::Result<()> {
    let mut r = Reader::new(trailer);
    let slot = r.upto(2, MAX_PLAYERS - 1)?;
    let p = snapshot.players.get_mut(slot).ok_or_else(|| invalid("invalid own slot"))?;
    let mut f = own_floats(p);
    for x in &mut f { *x = unordered(r.plus(ordered(*x), 3)?)?; }
    set_own_floats(p, f);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::sim::{Command, Match, World, MatchState};
    use peakrunner_core::terrain::MapId;

    fn bytes(s: &Snapshot) -> Vec<u8> { postcard::to_allocvec(s).unwrap() }
    /// The snapshot a client sees: quantized as the wire quantizes it.
    fn canonical(s: &Snapshot) -> Snapshot { Prepared::new(s).to_snapshot().unwrap() }
    fn roundtrip(cur: &Snapshot, base: Option<&Snapshot>) -> (Snapshot, usize) {
        let (c, b) = (Prepared::new(cur), base.map(Prepared::new));
        let body = c.encode(b.as_ref());
        let back = Prepared::decode(cur.tick, &body, b.as_ref()).unwrap();
        assert_eq!(back, c, "decoded wire form differs");
        (back.to_snapshot().unwrap(), body.len())
    }

    #[test]
    fn bits_and_numbers_round_trip() {
        let mut w = Writer::default();
        let values: Vec<(u64, u32)> = [0u64, 1, 2, 3, 255, 256, 65535, 1 << 40, u64::MAX - 5, u64::MAX].iter()
            .flat_map(|v| [0u32, 1, 4, 10].map(|k| (*v, k))).collect();
        for (v, k) in &values { w.eg(*v, *k); }
        let signed = [0i64, -1, 1, -1000, 1000, i32::MIN as i64, i32::MAX as i64, i64::MIN / 4, i64::MAX / 4];
        for v in signed { w.se(v, 3); }
        w.put(0b10110, 5); w.bytes(b"peak");
        let data = w.finish();
        let mut r = Reader::new(&data);
        for (v, k) in &values { assert_eq!(r.eg(*k).unwrap(), *v, "eg {v} k {k}"); }
        for v in signed { assert_eq!(r.se(3).unwrap(), v); }
        assert_eq!(r.get(5).unwrap(), 0b10110);
        assert_eq!(r.bytes(4).unwrap(), b"peak");
        assert!(r.get(9).is_err(), "reads past the end fail");
        // A run of zero bits is not a number.
        assert!(Reader::new(&[0; 16]).eg(0).is_err());
    }

    #[test]
    fn quantization_round_trips_within_precision_and_bounds() {
        for x in [0.0f32, 1e-4, -1e-4, 0.4999 / 1024.0, 1234.5678, -3999.123, 16_383.999, -16_000.0, 1.0e5] {
            let back = dq(q(x, POS), POS);
            assert!((back - x).abs() <= 0.5 / POS + x.abs() * f32::EPSILON, "{x} -> {back}");
            assert_eq!(dq(q(back, POS), POS), back, "idempotent at {x}");
        }
        // Out of range and non-finite values clamp instead of wrapping.
        assert_eq!(q(f32::INFINITY, POS), i32::MAX);
        assert_eq!(q(-1e30, POS), i32::MIN);
        assert_eq!(q(f32::NAN, POS), 0);
        assert!(dq(i32::MAX, POS).is_finite());
        for yaw in [0.0f32, 0.01, 3.0, 6.2, std::f32::consts::TAU - 1e-4, -0.5, 12.0, 100.0] {
            let back = yaw_q(yaw) as f32 / TURN;
            let err = (back - yaw).sin().atan2((back - yaw).cos()).abs();
            assert!(err <= 0.5 / TURN + 1e-6, "yaw {yaw}: {err}");
            assert_eq!(yaw_q(back), yaw_q(yaw));
        }
        for pitch in [-1.52f32, -0.08, 0.0, 0.7, 1.52] {
            let back = pitch_q(pitch) as f32 / TURN;
            assert!((back - pitch).abs() <= 0.5 / TURN + 1e-6);
        }
        assert!((0.5 / TURN).to_degrees() < 0.01, "angles to under 0.01°");
    }

    #[test]
    fn own_state_restores_exact_floats_including_odd_ones() {
        let mut m = Match::new(MapId::OldHoller);
        let slot = m.join(1, "Exact").unwrap();
        let p = &mut m.world.players[slot];
        p.pos = Vec3::new(1234.567, -0.000_123, 987.654); p.vel = Vec3::new(-0.0, 33.333_3, f32::MIN_POSITIVE);
        p.energy = 57.123_456; p.health = 0.333_333; p.cooldown = 1e-9; p.yaw = -0.3; p.stun = -0.0;
        p.set_wire_private(true, true, 0.139_999_9);
        let exact = m.snapshot();
        let prepared = Prepared::new(&exact);
        let mut decoded = prepared.to_snapshot().unwrap();
        assert_ne!(decoded.players[slot].pos, exact.players[slot].pos, "quantized");
        apply_own(&mut decoded, &own_state(&prepared, slot, &exact.players[slot])).unwrap();
        let (a, b) = (&decoded.players[slot], &exact.players[slot]);
        assert_eq!(own_floats(a).map(f32::to_bits), own_floats(b).map(f32::to_bits));
        assert_eq!((a.wire_private().0, a.wire_private().1), (b.wire_private().0, b.wire_private().1));
        assert!(apply_own(&mut decoded, &[0xff; 3]).is_err());
    }

    /// A team's marks (the target marker's), on players and spots, reach
    /// only that team, across the wire, and show on its players' screens.
    #[test]
    fn a_team_s_marks_reach_only_that_team() {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..4 { m.join(100 + i, &format!("Player {i}")).unwrap(); }
        let (ember, glacier) = ((0..4).find(|&i| m.world.players[i].team == Team::Ember).unwrap(),
            (0..4).find(|&i| m.world.players[i].team == Team::Glacier).unwrap());
        m.world.marks.push(peakrunner_core::sim::firing::Mark { team: Team::Ember, player: Some(glacier), pos: Vec3::ZERO, life: 5.0, by: None });
        let spot = Vec3::new(10.0, 20.0, 30.0);
        m.world.marks.push(peakrunner_core::sim::firing::Mark { team: Team::Ember, player: None, pos: spot, life: 5.0, by: Some(ember) });
        let seen = |slot: usize| Prepared::decode(m.tick, &Prepared::new(&m.snapshot_for(slot)).encode(None), None).unwrap().to_snapshot().unwrap();
        assert_eq!((seen(ember).marked, seen(ember).marked_spots), (vec![glacier as u8], vec![spot]));
        assert!(seen(glacier).marked.is_empty() && seen(glacier).marked_spots.is_empty(), "never to the other team");
        let mut client = peakrunner_core::sim::World::new();
        assert!(client.apply_snapshot(&seen(ember), m.world.players[ember].net_id));
        assert!(client.marked(Team::Ember, glacier));
        assert_eq!(client.marked_spots(Team::Ember), [spot]);
    }

    /// A busy match: 28 players moving, jetting and firing every weapon.
    fn busy(seq: u64, i: usize) -> Command {
        Command { seq, move_z: 1.0, move_x: if i.is_multiple_of(2) { -0.5 } else { 0.5 }, yaw: i as f32 + seq as f32 * 0.01,
            jet: seq % 240 < 100, jump: seq % 50 < 3, fire: true, weapon: ((seq / 120) % 3) as u8, ..Command::default() }
    }

    #[test]
    fn deltas_decode_to_the_full_snapshot_for_real_matches() {
        for map in [MapId::OldHoller, MapId::Longfield] {
            let mut m = Match::new(map);
            if map == MapId::Longfield { m.world.set_mode(SupportedMode::Football); }
            for i in 0..MAX_PLAYERS { m.join(100 + i as u32, &format!("Player {i}")).unwrap(); }
            let mut history: Vec<Snapshot> = Vec::new();
            for seq in 1..=900u64 {
                let commands: Vec<Option<Command>> = (0..MAX_PLAYERS).map(|i| Some(busy(seq, i))).collect();
                m.step(&commands);
                if seq == 450 { m.leave(3); m.chat(5, "halfway"); m.join(999, "Late Joiner"); }
                // Spectators come and go; a map change (rotation or vote)
                // leaves every baseline from the old map behind.
                if seq == 540 { m.leave(7); m.join_spectator(2000, "Watcher").unwrap(); m.queue_spectator(2000, true); }
                if seq == 600 { m.spectate(9).unwrap(); }
                if seq == 690 { m.rotate_to_mode(MapId::Reefbreak, SupportedMode::TeamDeathmatch); }
                if seq % 3 != 0 { continue; }
                let mut s = m.snapshot_for(seq as usize % MAX_PLAYERS);
                s.pings = (0..MAX_PLAYERS as u32).map(|i| (100 + i, 20 + (seq as u32 * 7 + i) % 30)).collect();
                let full = canonical(&s);
                assert_eq!(bytes(&roundtrip(&s, None).0), bytes(&full));
                // Against the previous snapshot, and older ones (a late ack).
                for back in [1, 2, 7, 30] {
                    if let Some(base) = history.len().checked_sub(back).map(|i| &history[i]) {
                        assert_eq!(bytes(&roundtrip(&s, Some(base)).0), bytes(&full), "{map:?} tick {} against {back} back", s.tick);
                    }
                }
                history.push(s);
            }
        }
    }

    #[test]
    fn deltas_decode_to_the_full_snapshot_for_random_states() {
        struct Rng(u64);
        impl Rng {
            fn u(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
            fn f(&mut self) -> f32 { (self.u() % 2_000_000) as f32 / 1000.0 - 1000.0 }
            fn v(&mut self) -> Vec3 { Vec3::new(self.f(), self.f(), self.f()) }
        }
        let mut rng = Rng(0x5eed_1234);
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..20 { m.join(1 + i, &format!("R{i}")).unwrap(); }
        let template = m.snapshot();
        let random = |g: &mut Rng| {
            let mut s = template.clone();
            s.tick = 1000 + g.u() % 100;
            for p in &mut s.players {
                if g.u().is_multiple_of(4) { continue; }
                p.pos = g.v(); p.vel = g.v() * 0.2;
                p.yaw = g.f(); p.pitch = g.f() / 700.0; p.health = g.f().abs() / 10.0; p.energy = g.f().abs() / 10.0;
                p.cooldown = g.f().abs() / 1000.0; p.frags = (g.u() % 50) as u32; p.alive = !g.u().is_multiple_of(5);
                p.carrying = [None, Some(Team::Ember), Some(Team::Glacier)][g.u() as usize % 3];
                p.repair_beam = g.u().is_multiple_of(3).then(|| g.v());
                p.repair_out = p.repair_beam.is_some() || g.u().is_multiple_of(4);
                p.repair_self = p.repair_beam.is_some() && g.u().is_multiple_of(2);
                let (jump_prev, _, coyote) = p.wire_private();
                p.set_wire_private(jump_prev, g.u().is_multiple_of(3), coyote);
                p.pack = [None, Some(PackKind::Wall), Some(PackKind::Ammo)][g.u() as usize % 3];
                p.armor = if g.u().is_multiple_of(2) { ArmorClass::Heavy } else { ArmorClass::Light };
                p.ammo = (0..8).map(|_| (g.u() % 200) as u16).collect();
                if g.u().is_multiple_of(7) { p.name = format!("N{}", g.u() % 1000); p.net_id = (g.u() % 5000) as u32; }
                if g.u().is_multiple_of(3) {
                    let b = &mut p.blade;
                    (b.swing, b.t, b.power, b.held, b.press) = ((g.u() % 8) as u8, (g.u() % 60) as u16, (g.u() % 256) as u8, (g.u() % 900) as u16, (g.u() % 5) as u8);
                    (b.chain, b.window, b.next, b.link, b.guard) = ((g.u() % 3) as u8, (g.u() % 30) as u16, (g.u() % 3) as u8, (g.u() % 3) as u8, g.u().is_multiple_of(2));
                    (b.reel, b.rest, b.struck, b.how, b.by) = ((g.u() % 40) as u16, (g.u() % 50) as u16, (g.u() % 256) as u8, (g.u() % 5) as u8, (g.u() % 28) as u8);
                }
            }
            s.acks = (0..MAX_PLAYERS).map(|_| g.u() % 100_000).collect();
            s.discs = (0..g.u() % 60).map(|i| Disc { pos: g.v(), vel: g.v() * 0.3,
                team: if i % 2 == 0 { Team::Ember } else { Team::Glacier }, owner: (g.u() % 30) as usize, life: g.f().abs() / 100.0,
                kind: (g.u() % 9) as u8, spin: g.f(), target: 0, power: 1.0 }).collect();
            s.explosions = (0..g.u() % 10).map(|_| Explosion { pos: g.v(), age: g.f().abs() / 1000.0, max_r: 9.0, kind: 2 }).collect();
            s.smoke = (0..g.u() % 40).map(|_| SmokePuff { pos: g.v(), age: g.f().abs() / 1000.0 }).collect();
            // Loot at rest, and riding a falling body (`fall1`).
            s.loot = (0..g.u() % 6).map(|_| Loot { pos: g.v(), ammo: vec![(g.u() % 9) as u16; 3], age: (g.u() % 600) as f32 / 60.0,
                grenades: (g.u() % 3) as u8, owner: (g.u() % 20) as u8, carried: g.u().is_multiple_of(2), ..Default::default() }).collect();
            s.pings = (0..g.u() % 10).map(|i| (i as u32 + (g.u() % 3) as u32, (g.u() % 400) as u32)).collect();
            s.time_left = g.f(); s.score = [(g.u() % 4) as u32, (g.u() % 4) as u32];
            // The rabbit flag: loose, or carried (`rabbit1`).
            s.rabbit = peakrunner_core::sim::rabbit::RabbitFlag { active: g.u().is_multiple_of(2), pos: g.v(), vel: g.v(), home: g.v(),
                carrier: g.u().is_multiple_of(2).then(|| (g.u() % 8) as usize), drop_timer: g.f().abs() / 100.0, held: g.f().abs() / 1000.0, ..Default::default() };
            s
        };
        for _ in 0..300 {
            let base = random(&mut rng);
            let mut cur = random(&mut rng);
            // Often a small step from the baseline, as in play.
            if rng.u().is_multiple_of(2) {
                cur = base.clone(); cur.tick += 3;
                for p in &mut cur.players { p.pos += p.vel * 0.05; p.energy -= 0.3; }
                for d in &mut cur.discs { d.pos += d.vel * 0.05; d.life -= 0.05; }
                if !cur.discs.is_empty() { cur.discs.remove(0); }
            }
            let full = canonical(&cur);
            assert_eq!(bytes(&roundtrip(&cur, Some(&base)).0), bytes(&full));
            assert_eq!(bytes(&roundtrip(&cur, None).0), bytes(&full));
        }
    }

    #[test]
    fn corrupt_and_hostile_deltas_are_rejected_without_panics() {
        let mut m = Match::new(MapId::OldHoller);
        for i in 0..MAX_PLAYERS { m.join(100 + i as u32, &format!("Player {i}")).unwrap(); }
        let base = Prepared::new(&m.snapshot());
        for seq in 1..=30 { m.step(&(0..MAX_PLAYERS).map(|i| Some(busy(seq, i))).collect::<Vec<_>>()); }
        let cur = Prepared::new(&m.snapshot());
        let body = cur.encode(Some(&base));
        let mut rng = 0x0bad_5eed_u64;
        let mut next = move || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; rng };
        let mut failures = 0;
        for trial in 0..3000 {
            let mut bad = body.clone();
            match trial % 3 {
                0 => { let i = next() as usize % bad.len(); bad[i] ^= 1 << (next() % 8); }
                1 => bad.truncate(next() as usize % bad.len()),
                _ => bad = (0..next() % 400).map(|_| next() as u8).collect(),
            }
            // Must return, never panic; whatever decodes is well-formed.
            match Prepared::decode(cur.tick, &bad, Some(&base)).and_then(|p| p.to_snapshot()) {
                Ok(s) => assert!(s.players.len() == MAX_PLAYERS && s.acks.len() == MAX_PLAYERS
                    && s.players.iter().all(|p| p.pos.is_finite() && p.vel.is_finite())),
                Err(_) => failures += 1,
            }
        }
        assert!(failures > 1000, "most corruption is detected: {failures}");
        // Patching the previous item again and again can't blow up memory.
        let mut w = Writer::default();
        w.eg(MAX_ITEMS as u64, 2);
        w.bit(false); w.bit(false); w.eg(MAX_ITEM_BYTES as u64, 4); w.bytes(&[7; MAX_ITEM_BYTES]);
        for _ in 1..MAX_ITEMS { w.bit(false); w.bit(true); w.eg(0, 0); w.eg(MAX_ITEM_BYTES as u64, 2); }
        let bomb = w.finish();
        assert!(bomb.len() < 20 * 1024);
        assert!(get_items(&mut Reader::new(&bomb), &[]).is_err(), "16 MB from under 20 KB");
        // A delta needs its own baseline: against no baseline or another one
        // it fails or decodes to something else, never to garbage that panics.
        let _ = Prepared::decode(cur.tick, &body, None);
        assert!(Prepared::decode(cur.tick, &[], None).is_err());
    }

    /// Prediction replays inputs from the recipient's own player; with the
    /// exact trailer it starts from the same bits as without quantization,
    /// so corrections are exactly those of an unquantized snapshot.
    #[test]
    fn local_prediction_corrections_are_unchanged_by_quantization() {
        let mut server = Match::new(MapId::OldHoller);
        let me = server.join(1, "Me").unwrap();
        for i in 0..6 { server.join(10 + i, "Other").unwrap(); }
        server.phase = Phase::Playing;
        server.step(&[]);
        let (mut exact, mut wire, mut rough) = (World::new(), World::new(), World::new());
        let mut pending: Vec<Command> = Vec::new();
        let (mut worst, mut samples, mut quantized_only) = (0.0f32, 0, 0.0f32);
        for seq in 1..=600u64 {
            let c = Command { seq, move_z: 1.0, move_x: if (seq / 90) % 2 == 0 { 0.6 } else { -0.6 }, yaw: (seq as f32 * 0.015).rem_euclid(std::f32::consts::TAU),
                jet: (seq / 40) % 3 == 0, jump: seq % 50 < 3, fire: seq % 7 == 0, ..Command::default() };
            let mut commands = vec![None; MAX_PLAYERS];
            commands[me] = Some(c);
            for (i, c) in commands.iter_mut().enumerate().take(7).skip(1) { *c = Some(busy(seq, i)); }
            server.step(&commands);
            pending.push(c);
            if seq % 3 != 0 { continue; }
            // The client hears about this snapshot while 6 inputs are in flight.
            let s = server.snapshot_for(me);
            let prepared = Prepared::new(&s);
            let mut sent = prepared.to_snapshot().unwrap();
            let plain = sent.clone();
            apply_own(&mut sent, &own_state(&prepared, me, &s.players[me])).unwrap();
            let acked = s.acks[me];
            pending.retain(|c| c.seq > acked);
            let replay = |w: &mut World, snap: &Snapshot| {
                w.apply_snapshot(snap, 1);
                if w.state == MatchState::Playing { for c in &pending { w.predict_command(*c); } }
                w.players[w.player_id].pos
            };
            let a = replay(&mut exact, &s);
            let b = replay(&mut wire, &sent);
            worst = worst.max(a.distance(b)); samples += 1;
            quantized_only = quantized_only.max(replay(&mut rough, &plain).distance(a));
        }
        assert!(samples > 150);
        assert_eq!(worst, 0.0, "prediction from the wire differs from prediction from the exact state");
        eprintln!("without the own-state trailer, prediction would be off by up to {quantized_only} m");
    }

    #[test]
    fn every_snapshot_and_player_field_is_carried() {
        // A new field must be added to the codec (and this list).
        let m = Match::new(MapId::OldHoller);
        let keys = |v: serde_json::Value| { let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect(); k.sort(); k.join(",") };
        assert_eq!(keys(serde_json::to_value(m.snapshot()).unwrap()),
            "acks,ball,blast_serial,conditions,credits,deployables,discs,equipment,explosions,feed,flags,loot,map,map_items,marked,marked_spots,mode,moveables,phase,phase_left,pings,\
players,points,rabbit,round,score,smoke,spectators,tick,time_left,vehicles,vote,vote_choices,vote_wait,waypoint");
        assert_eq!(keys(serde_json::to_value(Player::wire_blank()).unwrap()),
            "ability_clock,ability_on,ability_uses,alive,ammo,armor,blade,bot_goal,bot_role,bot_think,burst_left,carrying,charge,cooldown,coyote,credits,effects,energy,fire_mode,\
fire_prev,footing,frags,health,hits,is_bot,jet_prev,jetting,jump_prev,loadout,lock_target,lock_time,losses,mode_of,name,net_id,on_ground,pack,pack_clock,pack_on,\
pack_uses,pitch,pos,remote,repair_beam,repair_out,repair_self,respawn,riding,shots,skiing,spin,strain,stream,stun,suicide_in,swap_back,swap_clock,team,throw_clock,throw_held,\
throwables,vel,weapon,yaw");
        // A projectile's homing target and power are the server's alone:
        // clients never simulate hits or homing.
        let disc = Disc { pos: Vec3::ZERO, vel: Vec3::ZERO, team: Team::Ember, owner: 0, life: 0.0, kind: 0, spin: 0.0, target: 0, power: 1.0 };
        assert_eq!(keys(serde_json::to_value(disc).unwrap()), "kind,life,owner,pos,power,spin,target,team,vel");
    }
}
