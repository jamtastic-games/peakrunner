//! Behaviour traces (tests only). Fixed scenarios run through the shared
//! simulation, each simulated second reduced to one digest of everything a
//! player could notice: every player's movement, health, energy, weapon and
//! kit; flags, ball, capture points, equipment, deployables, loot and
//! projectiles; scores, clock and phase; events, feed and HUD text.
//!
//! `trace_golden.txt` holds the digests recorded before the moddable-engine
//! refactor (docs/modding.md, P0), recorded again for `arsenal1` (weapon
//! slots): the digest gained the plasma gun's rounds and the loadout, after
//! the old digest reproduced bit for bit on the new code; and for stations
//! that open only from their front (three scenarios stand beside one), and
//! with a tossed flag's velocity (`toss1`), after the rest reproduced with the
//! front rule switched off. The `arsenal` scenario, added after
//! `.20260930.5` shipped, pins that release's weapons and kit (plasma gun,
//! blaster, slots, field inventory, flag toss) the same way. Recorded again
//! on 2026-10-02 when the chaingun was renamed the Repeater: the feed's text
//! changed, and the old digests reproduced bit for bit under the old name
//! first. The `arsenal` scenario alone was recorded again the same day when
//! the three armors split the arsenal (light can't carry a field inventory,
//! heavy has five slots): it now starts in heavy armor; every other
//! scenario reproduced bit for bit. Again for the railgun's slug (300 m/s,
//! 126.67 damage, each shot empties the heavy's energy), which it fires;
//! every other scenario reproduced bit for bit. Again for the repeater's
//! rounds leaving its model's barrel end (`muzzle2`, `Weapon::muzzle`):
//! every scenario reproduced bit for bit with that muzzle switched off; and
//! again when every other weapon with a model got its barrel end, after the
//! repeater's digests reproduced with those switched off. The Ozarktic Blast
//! scenario alone was recorded again for its outpost tunnels (a new pack);
//! every other scenario reproduced bit for bit. Recorded again for
//! unmirrored ground (docs/map-pipeline.md, Terrain): new Frostline,
//! Cairnhold, Dustreach and Reefbreak packs (Reefbreak's side-on ship and
//! its flag bay too); the Old Holler, Tower Complex and Ozarktic Blast
//! scenarios reproduced bit for bit. The Reefbreak scenario alone was
//! recorded again for its ship fitted to an authored model (the flag mid-ship,
//! a new pack), and again for its hard-surface, three-level ship (the flag on
//! the entry platform, a stern low-gravity zone); every other scenario reproduced
//! bit for bit. Recorded again on 2026-10-09 when steering the jet began to
//! take lift (`steer1`) and bots plan and fly their climbs for it
//! (`bot_nav::lean_speed`), after every scenario reproduced bit for bit with
//! those switched off (the repair tool's rest, `repair1`, moves none); the
//! Football scenarios, on their own movement, reproduced with them on. Recorded
//! again the same day for the jump-boost (`jumpjet1`, `movement.jet_jump`;
//! at 0 it reproduced every scenario bit for bit) and two bot rules that
//! came with it (a climb starts from the ground, and a flyer takes off only
//! where its climb is clear); the hall that burns its holders (`spires3`)
//! runs on no traced map, and Football's five reproduced with all of it on.
//! Then once more for the railgun's faster slug (`rail2`, 600 m/s), which
//! moved only the `arsenal` scenario. Recorded again on 2026-10-10 when dead
//! bodies began to fall with their momentum and what they drop to ride them
//! down (`fall1`), after every scenario reproduced bit for bit with both
//! switched off. Recorded again on 2026-10-11 when Football was matched to
//! the classic rules and engine (`ball3`: a pass's elevation `pitch + 0.5
//! cos(pitch) - 0.25` from the middle of the body, power from any player
//! under the crosshair's body ray or in the yaw slice, a 1 s self-catch, the
//! catch pushed along the throw's heading, the engine's ball physics (a 0.2
//! x surface bounce, friction against the impact, rest under 1.28 m/s),
//! dropped throwers that fumble a carrier, an 88-degree look and ends
//! swapped at half time), which moved all six Football scenarios (a new ball
//! now lands and stops the engine's way); every scenario reproduced bit for
//! bit with the old rules first, and nothing else moved. Recorded again the
//! same day when discs began to burst where they meet water (`water3`, at
//! half damage and twice the push), which moved the two Old Holler CTF
//! scenarios, whose bots fire into its water; every scenario reproduced bit
//! for bit with the disc dragged in water as before. Four scenarios were
//! added the same day for the Rabbit and Team Rabbit modes (`rabbit1`), whose
//! flag joins the digest only in those modes; every existing scenario
//! reproduced bit for bit. The two Rabbit scenarios were recorded again on
//! 2026-10-11 for the classic Rabbit (`rabbit2`: hunters one side, points
//! per 15 s held over all holds, the rabbit's kills double, pings, whose
//! state joins the digest in Rabbit only); every other scenario, Team
//! Rabbit's included, reproduced bit for bit. The five Football scenarios
//! were recorded again on 2026-10-13 for bots that play football
//! (`football_bots`: catches in the air, open receivers, blocks, charges,
//! passes only a teammate takes first, football names); the lone one moved
//! through the names' draws on the match's random numbers, and every other
//! scenario reproduced bit for bit. Recorded again the same day when the
//! view models moved up and in (`muzzle3`: new `VM_ANCHOR_DISC`,
//! `VM_ANCHOR_BOLT` and `VM_ANCHOR_MORTAR`, the disc launcher's and
//! chaingun's barrel turns to match), which moved the eighteen scenarios
//! where someone shoots; the Football scenarios reproduced bit for bit.
//! The offline Highgoal Football scenario alone was recorded again on
//! 2026-10-14 when the feed's lines about the unnamed player became English
//! ("You pick up the ball", not "You picks up"; `World::verb`): only the
//! feed's text changed, and every digest reproduced bit for bit with the old
//! wording first. The offline Longfield Football scenario (12 players) alone
//! was recorded again the same day when catches were swept along the
//! ball's real path through the step (`pickup_along`, `ball4`): a catch
//! there took the speed the ball reached the catcher with, not the speed
//! after a bounce later in the step. The offline Longfield and Highgoal
//! Football scenarios and the Longfield server one with bots were recorded
//! again the same day for the "incoming" event a pass's receiver hears
//! (`Ball::pass_to`, `ball5`): with that one event left out, every digest
//! reproduced bit for bit. The same four Football scenarios were recorded
//! again on 2026-10-15 when a dropped ball (a fumble) stopped being
//! intercepted (`Ball::dropped`, `ball6`): whoever gets one picks it up, so
//! only the feed's words and the pickup's sound event changed, and with the
//! old wording every digest reproduced bit for bit. The same day, on the same
//! marker, catches reached 3% past the body (`football::CATCH_GROW`) and bot
//! carriers began reading defenders a quarter second late, only in front of
//! them, with a memory (`football_bots`, `fb_watch`): the four Football
//! scenarios moved again; with the old reach and exact reads every digest
//! reproduced, so the hits on bots judged where a person's game drew them
//! (`bump::seen_hit`; no scenario sends a claim) changed none. The same four
//! were recorded again on 2026-10-16 when a bot that catches in the air stopped
//! jetting the catch's push away (`football_bots`, `Brain::ride`): until it
//! lands, it jets only while its way lies within 60 degrees of the push, and
//! otherwise rides the push out; every other scenario reproduced bit for bit. Every later phase must reproduce them bit for bit until a phase
//! deliberately changes behaviour; only then record them again:
//!
//!   PEAKRUNNER_TRACE_BLESS=1 cargo test -p peakrunner-core --lib behaviour_traces
//!
//! `PEAKRUNNER_TRACE_DUMP=<dir>` also writes a readable line per second per
//! scenario, to diff two builds when a digest moves.
//!
//! Float results depend on the platform's maths library, so the digests are
//! compared only where they were recorded (macOS on Apple Silicon); other
//! platforms still run every scenario as a smoke test.
use super::*;
use crate::map_catalog::SupportedMode;
use std::fmt::Write as _;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/trace_golden.txt");
const CHECKED: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

/// FNV-1a, 64 bits: stable across builds and platforms.
struct Digest(u64);
impl Digest {
    fn new() -> Self { Self(0xcbf2_9ce4_8422_2325) }
    fn bytes(&mut self, b: &[u8]) { for &x in b { self.0 ^= x as u64; self.0 = self.0.wrapping_mul(0x0100_0000_01b3); } }
    fn u32(&mut self, v: u32) { self.bytes(&v.to_le_bytes()); }
    fn u64(&mut self, v: u64) { self.bytes(&v.to_le_bytes()); }
    fn f32(&mut self, v: f32) { self.u32(v.to_bits()); }
    fn v3(&mut self, v: Vec3) { self.f32(v.x); self.f32(v.y); self.f32(v.z); }
    fn flag(&mut self, v: bool) { self.bytes(&[v as u8]); }
    fn slot(&mut self, v: Option<usize>) { self.u64(v.map_or(u64::MAX, |v| v as u64)); }
    fn text(&mut self, s: &str) { self.u64(s.len() as u64); self.bytes(s.as_bytes()); }
}

fn team(t: Option<Team>) -> u64 { t.map_or(9, |t| t.idx() as u64) }

// The digest writes base content in its encodings from before the engine's
// classes (`classes1`), so the recorded traces still apply: a weapon as its
// family's old kind number (the grenade launcher and mortar were both 2, the
// laser and railgun both 3), rounds as the old five slots, loadouts as the
// old three-bit codes, death packs as their old four slots.

/// A weapon's old kind number: its family's place in `KIND_KEYS`.
fn legacy_weapon(k: u8) -> u8 {
    let r = crate::registry::registry();
    r.weapons.get(k as usize).and_then(|w| super::arsenal::KIND_KEYS.iter().position(|f| *f == w.family)).map_or(k, |f| f as u8)
}

/// Rounds in the old slots: disc, chaingun, launcher, rifle, plasma gun (the
/// player's armor's weapon of each family).
fn legacy_ammo(p: &Player) -> [u16; 5] {
    let mut out = [0; 5];
    for (f, family) in super::arsenal::KIND_KEYS.iter().take(5).enumerate() {
        out[f] = super::arsenal::family_weapon(p.armor, family).and_then(|k| p.ammo.get(k as usize).copied()).unwrap_or(0);
    }
    out
}

/// The old loadout code: three bits a slot (7 empty), six slots.
fn legacy_loadout(l: &super::arsenal::Loadout) -> u32 {
    (0..6).fold(0, |c, i| c | (l.slot(i).map_or(7, |k| legacy_weapon(k) as u32) << (3 * i)))
}

/// A death pack's old slots: discs, chaingun rounds, launcher rounds, 0.
fn legacy_loot(l: &super::loadout::Loot) -> [u16; 4] {
    let r = crate::registry::registry();
    let family = |name: &str| -> u16 { r.weapons.iter().filter(|(_, _, w)| w.family == name)
        .map(|(k, _, _)| l.ammo.get(k).copied().unwrap_or(0)).sum() };
    [family("disc"), family("chaingun"), family("launcher"), 0]
}

/// The HUD line with its weapon as the old kind number.
fn legacy_hud(hud: &str) -> String {
    let key = "\"weapon\":";
    let Some(at) = hud.find(key).map(|i| i + key.len()) else { return hud.to_string() };
    let end = hud[at..].find(',').map_or(hud.len(), |e| at + e);
    let weapon = hud[at..end].parse::<u8>().map_or_else(|_| hud[at..end].to_string(), |k| legacy_weapon(k).to_string());
    format!("{}{weapon}{}", &hud[..at], &hud[end..])
}

/// Put the person in heavy armor the way the scenario always did: by
/// assignment, keeping the light kit's weapons and rounds. Before the
/// classes those were armor-free (slot 2 was "the launcher"), so it meant
/// disc, chaingun and mortar with the light kit's numbers of rounds; the
/// same state now names the heavy armor's own weapons.
fn heavy_as_before(p: &mut Player) {
    let light = p.armor;
    let mut ammo = p.ammo.clone();
    for family in super::arsenal::KIND_KEYS {
        let (from, to) = (super::arsenal::family_weapon(light, family), super::arsenal::family_weapon(ArmorClass::Heavy, family));
        if let (Some(f), Some(t)) = (from, to) { ammo[t as usize] = p.ammo[f as usize]; if f != t { ammo[f as usize] = 0; } }
    }
    p.ammo = ammo;
    p.loadout = p.loadout.for_armor(ArmorClass::Heavy);
    p.armor = ArmorClass::Heavy;
}

/// A scripted input's old weapon kind as the player's armor carries it.
fn armed(w: &World, i: usize, kind: u8) -> u8 {
    super::arsenal::KIND_KEYS.get(kind as usize).and_then(|f| super::arsenal::family_weapon(w.players[i].armor, f)).unwrap_or(kind)
}

/// Everything observable in `w`, into `d`.
fn world(d: &mut Digest, w: &World) {
    d.text(w.map.key());
    d.text(w.mode.key());
    d.u32(w.score[0]); d.u32(w.score[1]);
    d.f32(w.time_left);
    d.f32(w.countdown);
    d.text(&format!("{:?}", w.state));
    d.text(&format!("{:?}", w.conditions));
    d.text(&w.message);
    d.u64(w.players.len() as u64);
    for p in &w.players {
        d.u32(p.net_id); d.text(&p.name); d.u64(p.team.idx() as u64);
        d.flag(p.alive); d.flag(p.is_bot);
        d.v3(p.pos); d.v3(p.vel); d.f32(p.yaw); d.f32(p.pitch);
        d.f32(p.health); d.f32(p.energy); d.f32(p.respawn);
        d.u64(team(p.carrying));
        d.flag(p.on_ground); d.flag(p.skiing); d.flag(p.jetting);
        d.f32(p.cooldown); d.bytes(&[legacy_weapon(p.weapon)]);
        d.text(&format!("{:?}", p.armor));
        for a in legacy_ammo(p) { d.bytes(&a.to_le_bytes()); }
        d.u32(legacy_loadout(&p.loadout)); d.text(&format!("{:?}", p.pack)); d.bytes(&p.throwables);
        d.f32(p.stun); d.f32(p.suicide_in);
        d.u32(p.frags); d.u32(p.losses); d.u32(p.shots); d.u32(p.hits);
    }
    for f in &w.flags { d.v3(f.pos); d.v3(f.home); d.slot(f.carrier); d.f32(f.drop_timer); d.v3(f.vel); }
    // Only in the rabbit modes, so every other scenario's digest stays as recorded.
    if w.rabbit_mode() {
        let r = &w.rabbit;
        d.flag(r.active); d.v3(r.pos); d.v3(r.vel); d.v3(r.home); d.slot(r.carrier); d.f32(r.drop_timer); d.f32(r.held);
        d.slot(r.tossed_by); d.f32(r.toss_guard);
        // The classic Rabbit's pings and time held (`rabbit2`).
        if w.mode.rules() == crate::modes::Rules::Rabbit {
            d.f32(r.ping); d.f32(r.ping_next); d.f32(r.since_hurt.min(1.0e6));
            for (id, t) in &r.times { d.u32(*id); d.f32(*t); }
        }
    }
    let b = &w.ball;
    d.flag(b.active); d.flag(b.in_play); d.v3(b.pos); d.v3(b.vel); d.slot(b.carrier); d.slot(b.thrown_by);
    d.u64(team(b.last_team)); d.f32(b.no_catch); d.f32(b.idle); d.f32(b.respawn); d.f32(b.regroup);
    d.bytes(&[b.next_side, b.half]); d.f32(b.oob_clock);
    for p in &w.points {
        d.text(&p.name); d.flag(p.active); d.u64(p.owner.map_or(9, u64::from)); d.f32(p.progress);
        d.u64(p.capturing.map_or(9, u64::from)); d.flag(p.contested);
    }
    for e in &w.equipment { d.f32(e.health); d.flag(e.powered); d.f32(e.cooldown); d.v3(e.aim); d.f32(e.shield); d.flag(e.offline); }
    for x in &w.deployables { d.text(&format!("{:?}", x.kind)); d.u64(x.team.idx() as u64); d.v3(x.pos); d.f32(x.yaw); d.f32(x.health); }
    for l in &w.loot { d.v3(l.pos); for a in legacy_loot(l) { d.bytes(&a.to_le_bytes()); } d.f32(l.age); }
    for x in &w.discs { d.v3(x.pos); d.v3(x.vel); d.u64(x.owner as u64); d.bytes(&[x.kind]); d.f32(x.life); }
    d.u64(w.explosions.len() as u64);
    d.u64(w.blast_serial);
    d.text(&serde_json::to_string(&w.feed).unwrap());
}

/// One readable line for `PEAKRUNNER_TRACE_DUMP`.
fn describe(w: &World) -> String {
    let mut s = format!("score {:?} time {:.3} ball {:?} {:?} flags", w.score, w.time_left, w.ball.pos, w.ball.carrier);
    for f in &w.flags { let _ = write!(s, " {:?}/{:?}", f.pos, f.carrier); }
    for p in &w.points { let _ = write!(s, " {}:{:?}:{:.3}", p.name, p.owner, p.progress); }
    if w.rabbit_mode() {
        let r = &w.rabbit;
        let _ = write!(s, " rabbit {} {:?}/{:?} held {:.2}", r.active, r.pos, r.carrier, r.held);
    }
    for (i, p) in w.players.iter().enumerate().filter(|(_, p)| p.net_id != 0 || !p.remote) {
        let _ = write!(s, " | {i} {:?} {} {:?} {:?} h{:.4} e{:.4}", p.team, p.alive, p.pos, p.vel, p.health, p.energy);
        if w.rabbit_mode() { let _ = write!(s, " f{}", p.frags); }
    }
    s
}

struct Run { name: String, digests: Vec<u64>, lines: Vec<String> }

impl Run {
    fn new(name: String) -> Self { Self { name, digests: Vec::new(), lines: Vec::new() } }
    fn second(&mut self, w: &World, extra: impl FnOnce(&mut Digest)) {
        let mut d = Digest::new();
        world(&mut d, w);
        extra(&mut d);
        self.digests.push(d.0);
        self.lines.push(describe(w));
    }
}

fn map(key: &str) -> MapId { MapId::parse(key).unwrap_or_else(|| panic!("map {key}")) }

/// Bots route over the map's graph from the first tick, not once a
/// background build happens to land.
fn graph_ready(m: MapId) { let _ = crate::bot_nav::graph(m); }

/// The scripted person: a twelve-second loop of running, skiing, jetting,
/// strafing, firing each weapon and turning, plus a thrown grenade and a
/// Ctrl+K along the way.
fn scripted(step: usize) -> (Input, f32) {
    let mut i = Input::default();
    let phase = (step / 60) % 12;
    match phase {
        0 | 1 => i.move_z = 1.0,
        2 | 3 => { i.move_z = 1.0; i.jump = true; }
        4 => { i.move_z = 1.0; i.jet = true; }
        5 => { i.move_x = 1.0; i.jet = true; }
        6 | 7 => { i.move_z = 1.0; i.fire = step % 30 == 0; i.weapon = 0; }
        8 => { i.fire = true; i.weapon = 1; }
        9 => { i.fire = step % 20 == 0; i.weapon = 2; i.jump = true; }
        10 => { i.move_x = -1.0; i.move_z = 0.5; }
        _ => { i.move_z = -1.0; i.jump = step % 40 < 20; }
    }
    if step == 1500 { i.throw = throwables::THROW_GRENADE; i.throw_strength = 0.8; }
    if step == 3300 { i.suicide = true; }
    let look = if phase == 10 { 3.0 } else if phase == 5 { -1.5 } else { 0.0 };
    (i, look)
}

fn command(step: usize, seq: u64, yaw: f32) -> Command {
    let (i, look) = scripted(step);
    Command { seq, move_x: i.move_x, move_z: i.move_z, yaw: yaw - look * 0.004 * step as f32 % 100.0,
        pitch: -0.1, jump: i.jump, jet: i.jet, fire: i.fire, weapon: i.weapon, throw: i.throw,
        throw_strength: i.throw_strength, suicide: i.suicide, ..Command::default() }
}

/// An offline match as the menu starts it: the scripted person and the
/// offline bots, stepped through `World::tick`.
fn offline(key: &str, mode: SupportedMode, seconds: usize, alone: bool, heavy: bool) -> Run {
    offline_staged(key, mode, seconds, alone, heavy, false)
}

/// With `captures`, the person is set down on the enemy flag and then on
/// their own stand three times (grab, capture, the winning capture and the
/// end of the match), with a Ctrl+K drop in between.
fn offline_staged(key: &str, mode: SupportedMode, seconds: usize, alone: bool, heavy: bool, captures: bool) -> Run {
    let m = map(key);
    if !alone { graph_ready(m); }
    let mut w = World::new();
    w.reseed(0x5EED_0001);
    w.set_map(m);
    w.set_mode(mode);
    w.start_match(true);
    if alone { w.players.truncate(1); }
    if heavy { heavy_as_before(&mut w.players[0]); }
    let mut run = Run::new(format!("offline {} {key}{}{}{}", mode.key(), if alone { " alone" } else { "" },
        if heavy { " heavy" } else { "" }, if captures { " captures" } else { "" }));
    let mut events = String::new();
    for step in 0..seconds * 60 {
        if captures && w.players[0].alive {
            let at = match step % 600 { 100 | 400 => Some(w.flags[1].pos), 200 => Some(w.flags[0].home), _ => None };
            if let Some(at) = at { w.players[0].pos = at + Vec3::Y * 1.2; w.players[0].vel = Vec3::ZERO; }
        }
        let (mut input, look) = scripted(step);
        input.weapon = armed(&w, 0, input.weapon);
        if captures && step % 600 == 450 { input.suicide = true; }
        let jump_prev = w.input.jump_prev;
        w.input = Input { jump_prev, ..input };
        if look != 0.0 { w.add_look(look, 0.0); }
        w.tick(STEP);
        events.push_str(&w.events);
        events.push('|');
        if step % 60 == 59 {
            let hud = w.hud_json();
            run.second(&w, |d| { d.text(&events); d.text(&legacy_hud(&hud)); });
            events.clear();
        }
    }
    run
}

/// A match whose clock runs out: the end of the round (the rule set's
/// verdict), and on a server the intermission and the next round.
fn clock_out(key: &str, mode: SupportedMode, server: bool) -> Run {
    let m = map(key);
    graph_ready(m);
    let mut run = Run::new(format!("clock {} {key}{}", mode.key(), if server { " server" } else { "" }));
    if server {
        let mut g = Match::new(m);
        g.world.set_mode(mode);
        g.world.reseed(0x5EED_0004);
        g.bots = 6;
        let none = vec![None; MAX_PLAYERS];
        let mut events = String::new();
        for second in 0..45 {
            if second == 15 { g.world.time_left = 3.0; }
            for _ in 0..60 { g.step(&none); events.push_str(&g.world.events); events.push('|'); }
            let (phase, left, round) = (g.phase, g.phase_left, g.round);
            let hud = g.world.hud_json();
            run.second(&g.world, |d| { d.text(&events); d.text(&legacy_hud(&hud)); d.text(&format!("{phase:?}")); d.f32(left); d.u32(round); });
            events.clear();
        }
    } else {
        let mut w = World::new();
        w.reseed(0x5EED_0005);
        w.set_map(m);
        w.set_mode(mode);
        w.start_match(true);
        w.time_left = 4.0;
        let mut events = String::new();
        for step in 0..8 * 60 {
            let (mut input, _) = scripted(step);
            input.weapon = armed(&w, 0, input.weapon);
            let jump_prev = w.input.jump_prev;
            w.input = Input { jump_prev, ..input };
            w.tick(STEP);
            events.push_str(&w.events);
            events.push('|');
            if step % 60 == 59 {
                let hud = w.hud_json();
                run.second(&w, |d| { d.text(&events); d.text(&legacy_hud(&hud)); });
                events.clear();
            }
        }
    }
    run
}

/// A dedicated server's match: bots only, through countdown, play, and
/// (with `rotate`) a rotation to another map and mode partway.
fn server(key: &str, mode: SupportedMode, bots: usize, seconds: usize, rotate: Option<(usize, &str, SupportedMode)>) -> Run {
    let m = map(key);
    graph_ready(m);
    if let Some((_, k, _)) = rotate { graph_ready(map(k)); }
    let mut g = Match::new(m);
    g.world.set_mode(mode);
    g.world.reseed(0x5EED_0002);
    g.bots = bots;
    let none = vec![None; MAX_PLAYERS];
    let mut run = Run::new(format!("server {} {key} bots{bots}{}", mode.key(),
        rotate.map_or(String::new(), |(s, k, m)| format!(" then {} {k} at {s}s", m.key()))));
    let mut events = String::new();
    for second in 0..seconds {
        if let Some((at, k, mode)) = rotate { if second == at { g.rotate_to_mode(map(k), mode); } }
        for _ in 0..60 {
            g.step(&none);
            events.push_str(&g.world.events);
            events.push('|');
        }
        let (phase, left, round, tick) = (g.phase, g.phase_left, g.round, g.tick);
        run.second(&g.world, |d| {
            d.text(&events); d.text(&format!("{phase:?}")); d.f32(left); d.u32(round); d.u64(tick);
        });
        events.clear();
    }
    run
}

/// Two people on a server; each second the first one's client applies the
/// latest snapshot and predicts half a second of its own scripted commands
/// (the online movement path, Football's included).
fn prediction(key: &str, mode: SupportedMode, seconds: usize) -> Run {
    let m = map(key);
    let mut g = Match::new(m);
    g.world.set_mode(mode);
    g.world.reseed(0x5EED_0003);
    let a = g.join(11, "Runner").unwrap();
    let b = g.join(22, "Other").unwrap();
    let mut client = World::new();
    let mut run = Run::new(format!("predict {} {key}", mode.key()));
    let mut commands = vec![None; MAX_PLAYERS];
    let mut seq = 0u64;
    let mut step = 0usize;
    for _ in 0..seconds {
        for _ in 0..60 {
            seq += 1;
            step += 1;
            let mut c = command(step, seq, 0.3);
            c.weapon = armed(&g.world, a, c.weapon);
            commands[a] = Some(c);
            commands[b] = Some(Command { seq, move_z: 1.0, jet: step % 90 < 30, fire: step % 45 == 0, yaw: 2.0, ..Command::default() });
            g.step(&commands);
        }
        let snap = g.snapshot_for(a);
        assert!(client.apply_snapshot(&snap, 11), "{} applies", run.name);
        let mut predicted = Vec::new();
        for k in 0..30 {
            let mut c = command(step + k, seq + k as u64 + 1, 0.3);
            c.weapon = armed(&client, client.player_id, c.weapon);
            client.predict_command(c);
            let p = &client.players[client.player_id];
            predicted.push((p.pos, p.vel, p.energy, p.on_ground, p.skiing, p.jetting, p.weapon));
        }
        run.second(&g.world, |d| {
            d.text(&format!("{:?}", g.phase));
            for (pos, vel, energy, ground, ski, jet, weapon) in predicted {
                d.v3(pos); d.v3(vel); d.f32(energy); d.flag(ground); d.flag(ski); d.flag(jet); d.bytes(&[legacy_weapon(weapon)]);
            }
        });
    }
    run
}

/// The weapons and kit of `arsenal1` and `toss1` on Old Holler: the person
/// equips a heavy setup with a field inventory at their own station (light
/// armor can't carry one), fires the plasma gun and the blaster at two
/// enemies standing still, glances blaster bolts off the ground, fires the
/// railgun, puts the field inventory down and swaps a weapon at it, fires
/// the mortar, changes to light armor at the station, fires the laser and
/// the grenade launcher, then grabs the enemy flag, tosses it home and runs
/// after it. (Re-recorded when the three armors split the arsenal: it began
/// in light armor.)
fn arsenal() -> Run {
    use super::arsenal::{Loadout, BLASTER, DISC, LAUNCHER, MORTAR, PLASMA, RAILGUN, RIFLE};
    use super::deploy::DeployKind;
    use super::loadout::{buy_drop, buy_take, Equip, BUY_LIGHT, BUY_NONE};
    let m = map("old-holler");
    let mut w = World::new();
    w.reseed(0x5EED_0006);
    w.set_map(m);
    w.set_mode(SupportedMode::Ctf);
    w.start_match(true);
    w.players.truncate(1);
    let me = w.players[0].team;
    let foe_team = if me == Team::Ember { Team::Glacier } else { Team::Ember };
    let station = crate::equipment::definitions(m).iter()
        .find(|d| d.kind == crate::equipment::Kind::Inventory && d.team as usize == me.idx()).expect("a station").pos();
    let ground = |x: f32, z: f32| Vec3::new(x, crate::terrain::support_on(m, Vec3::new(x, 400.0, z)).0 + PLAYER_RADIUS, z);
    let range = ground(1060.0, 700.0);
    const FOES: [(f32, f32); 2] = [(-1.5, -10.0), (2.0, -25.0)];
    let mut run = Run::new("arsenal ctf old-holler".into());
    let mut events = String::new();
    // The weapons that hit someone, each hit credited to the last one fired.
    let (mut hit_with, mut last) = (std::collections::BTreeSet::new(), DISC);
    for step in 0..25 * 60 {
        let mut input = Input::default();
        let mut aim_at = None;
        let mut put = None;
        match step {
            0 => {
                put = Some(station);
                input.equip = Some(Equip { armor: ArmorClass::Heavy.0 + 1, weapons: Loadout::of(&[PLASMA, BLASTER, RAILGUN, MORTAR]),
                    pack: super::packs::PackKind::Inventory.0 + 1 });
            }
            30 | 840 => {
                put = Some(range);
                if step == 30 {
                    for _ in FOES {
                        let mut foe = w.players[0].clone();
                        foe.team = foe_team;
                        w.players.push(foe);
                    }
                }
            }
            60..240 => { input.weapon = PLASMA; input.fire = step % 36 == 0; aim_at = Some(1); }
            240..360 => { input.weapon = BLASTER; input.fire = true; aim_at = Some(2); }
            360..420 => { input.weapon = BLASTER; input.fire = true; w.players[0].pitch = -0.5; }
            420..540 => { input.weapon = RIFLE; input.fire = step % 70 == 0; aim_at = Some(2); }
            540 => {
                // Down a few metres off to the side, clear of the targets
                // and the toss, at the first heading with room.
                w.players[0].pos = range;
                w.players[0].vel = Vec3::ZERO;
                w.players[0].pitch = -0.3;
                let yaw = (0..16).map(|k| std::f32::consts::FRAC_PI_2 + k as f32 * 0.39)
                    .find(|&y| { w.players[0].yaw = y; w.place(0, DeployKind::Inventory, 0.0).is_ok() });
                w.players[0].yaw = yaw.expect("room for the field inventory");
                input.deploy = true;
            }
            600 | 601 => {
                if let Some(kiosk) = w.deployables.iter().find(|d| d.kind == DeployKind::Inventory) {
                    let front = kiosk.pos + kiosk.front() * 1.2;
                    put = Some(ground(front.x, front.z));
                }
                input.buy = if step == 600 { buy_drop(armed(&w, 0, RIFLE)) } else { buy_take(DISC) };
            }
            660..720 => { input.weapon = DISC; input.fire = step % 60 == 0; aim_at = Some(2); }
            // Fire held: one mortar shell.
            720..780 => { input.weapon = LAUNCHER; input.fire = true; aim_at = Some(2); }
            780..784 => {
                put = Some(station);
                input.buy = [BUY_LIGHT, buy_drop(BLASTER), buy_take(armed(&w, 0, RIFLE)), BUY_NONE][step - 780];
            }
            // Fire held: laser shots, then grenades once it has reloaded.
            900..990 => { input.weapon = RIFLE; input.fire = true; aim_at = Some(2); }
            990..1080 => { input.weapon = LAUNCHER; input.fire = true; aim_at = Some(2); }
            1080 => put = Some(w.flags[foe_team.idx()].pos + Vec3::Y * 1.2),
            1110 => put = Some(range),
            // Away from the two enemies, then after it (the thrower can't
            // catch it for a moment).
            1140.. => {
                w.players[0].yaw = std::f32::consts::PI;
                w.players[0].pitch = 0.2;
                input.toss = step == 1140;
                input.move_z = if step > 1150 { 1.0 } else { 0.0 };
            }
            _ => {}
        }
        if let Some(at) = put { w.players[0].pos = at; w.players[0].vel = Vec3::ZERO; }
        // The enemies stand still, like targets on a range.
        for (k, &(dx, dz)) in FOES.iter().enumerate() {
            if let Some(f) = w.players.get_mut(k + 1).filter(|f| f.alive) { f.pos = ground(range.x + dx, range.z + dz); f.vel = Vec3::ZERO; }
        }
        if let Some(t) = aim_at {
            let d = (w.players[t].pos + Vec3::Y * 0.9) - (w.players[0].pos + Vec3::Y * EYE);
            w.players[0].yaw = (-d.x).atan2(-d.z);
            w.players[0].pitch = (d.y / d.length()).asin();
        }
        input.weapon = armed(&w, 0, input.weapon);
        if input.fire { last = legacy_weapon(input.weapon); }
        let jump_prev = w.input.jump_prev;
        w.input = Input { jump_prev, ..input };
        let hits = w.players[0].hits;
        w.tick(STEP);
        events.push_str(&w.events);
        events.push('|');
        // The script does what it says.
        let p = &w.players[0];
        let kit = || p.loadout.kinds().map(legacy_weapon).collect::<Vec<_>>();
        match step {
            0 => assert_eq!((p.armor, kit(), p.pack), (ArmorClass::Heavy, vec![PLASMA, BLASTER, RIFLE, LAUNCHER],
                Some(super::packs::PackKind::Inventory)), "equipped"),
            540 => assert!(w.deployables.iter().any(|d| d.kind == DeployKind::Inventory), "field inventory down"),
            601 => assert_eq!(kit(), [PLASMA, BLASTER, LAUNCHER, DISC], "swapped at the field inventory"),
            783 => assert_eq!((p.armor, kit()), (ArmorClass::Light, vec![PLASMA, LAUNCHER, RIFLE]), "light at the station"),
            1139 => assert_eq!(p.carrying, Some(foe_team), "holding the flag"),
            1140 => assert!(p.carrying.is_none() && w.flags[foe_team.idx()].vel.length() > 10.0, "tossed"),
            1175 => assert!(w.flags[foe_team.idx()].pos.distance(p.pos) > 6.0, "a good way off"),
            _ => {}
        }
        if p.hits > hits && last != DISC { hit_with.insert(last); }
        if step % 60 == 59 {
            let hud = w.hud_json();
            run.second(&w, |d| { d.text(&events); d.text(&legacy_hud(&hud)); });
            events.clear();
        }
    }
    assert_eq!(hit_with.into_iter().collect::<Vec<_>>(), [LAUNCHER, RIFLE, PLASMA, BLASTER], "both launchers, both rifles, the plasma gun and blaster hit");
    run
}

fn scenarios() -> Vec<Run> {
    let jobs: Vec<Box<dyn FnOnce() -> Run + Send>> = vec![
        Box::new(|| offline("old-holler", SupportedMode::Ctf, 40, true, false)),
        Box::new(|| offline("frostline", SupportedMode::Ctf, 40, true, true)),
        Box::new(|| offline("longfield", SupportedMode::Football, 40, true, false)),
        Box::new(|| offline("old-holler", SupportedMode::Ctf, 120, false, false)),
        Box::new(|| offline_staged("dustreach", SupportedMode::Ctf, 40, false, false, true)),
        Box::new(|| offline("reefbreak", SupportedMode::Ctf, 60, false, false)),
        Box::new(|| offline("ozarktic-blast", SupportedMode::Ctf, 60, false, false)),
        Box::new(|| offline("tower-complex", SupportedMode::CaptureAndHold, 90, false, false)),
        Box::new(|| offline("dustreach", SupportedMode::TeamDeathmatch, 90, false, false)),
        Box::new(|| offline("cairnhold", SupportedMode::Deathmatch, 90, false, false)),
        Box::new(|| offline("longfield", SupportedMode::Football, 120, false, false)),
        Box::new(|| offline("highgoal", SupportedMode::Football, 90, false, false)),
        Box::new(|| server("old-holler", SupportedMode::Ctf, 12, 150, None)),
        Box::new(|| server("longfield", SupportedMode::Football, 8, 150, None)),
        Box::new(|| server("frostline", SupportedMode::CaptureAndHold, 8, 120, None)),
        Box::new(|| server("cairnhold", SupportedMode::TeamDeathmatch, 10, 150, Some((70, "highgoal", SupportedMode::Football)))),
        Box::new(|| prediction("old-holler", SupportedMode::Ctf, 40)),
        Box::new(|| clock_out("old-holler", SupportedMode::Ctf, false)),
        Box::new(|| clock_out("dustreach", SupportedMode::TeamDeathmatch, false)),
        Box::new(|| clock_out("cairnhold", SupportedMode::Deathmatch, false)),
        Box::new(|| clock_out("frostline", SupportedMode::Ctf, true)),
        Box::new(|| prediction("longfield", SupportedMode::Football, 50)),
        Box::new(arsenal),
        Box::new(|| offline("cairnhold", SupportedMode::Rabbit, 90, false, false)),
        Box::new(|| offline("dustreach", SupportedMode::TeamRabbit, 90, false, false)),
        Box::new(|| server("frostline", SupportedMode::TeamRabbit, 10, 150, None)),
        Box::new(|| clock_out("cairnhold", SupportedMode::Rabbit, false)),
    ];
    std::thread::scope(|s| {
        let handles: Vec<_> = jobs.into_iter().map(|job| s.spawn(job)).collect();
        handles.into_iter().map(|h| h.join().expect("scenario panicked")).collect()
    })
}

fn render(runs: &[Run]) -> String {
    let mut out = String::from("# Behaviour trace digests (crates/core/src/trace.rs). One line per scenario,\n# one digest per simulated second. Recorded on macOS, Apple Silicon.\n");
    for r in runs {
        let _ = write!(out, "{}:", r.name);
        for d in &r.digests { let _ = write!(out, " {d:016x}"); }
        out.push('\n');
    }
    out
}

#[test]
fn behaviour_traces_match_the_recorded_digests() {
    let runs = scenarios();
    if let Ok(dir) = std::env::var("PEAKRUNNER_TRACE_DUMP") {
        std::fs::create_dir_all(&dir).unwrap();
        for r in &runs {
            let file = r.name.replace(' ', "_");
            let body: String = r.lines.iter().enumerate().map(|(i, l)| format!("{:4} {:016x} {l}\n", i + 1, r.digests[i])).collect();
            std::fs::write(format!("{dir}/{file}.txt"), body).unwrap();
        }
    }
    let now = render(&runs);
    if std::env::var_os("PEAKRUNNER_TRACE_BLESS").is_some() {
        std::fs::write(GOLDEN, &now).unwrap();
        return;
    }
    if !CHECKED { return; }
    let golden = std::fs::read_to_string(GOLDEN).expect("trace_golden.txt (record it with PEAKRUNNER_TRACE_BLESS=1)");
    let recorded: std::collections::HashMap<&str, Vec<&str>> = golden.lines().filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(':')).map(|(n, d)| (n, d.split_whitespace().collect())).collect();
    let mut problems = Vec::new();
    for r in &runs {
        let Some(want) = recorded.get(r.name.as_str()) else { problems.push(format!("{}: not recorded", r.name)); continue; };
        let got: Vec<String> = r.digests.iter().map(|d| format!("{d:016x}")).collect();
        if let Some(i) = (0..want.len().max(got.len())).find(|&i| want.get(i).copied() != got.get(i).map(String::as_str)) {
            problems.push(format!("{}: first differs at second {} ({})", r.name, i + 1, r.lines.get(i).map_or("", String::as_str)));
        }
    }
    assert!(problems.is_empty(), "behaviour changed:\n{}", problems.join("\n"));
}
