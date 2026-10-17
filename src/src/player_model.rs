//! Articulated third-person armor. Rigid parts per bone, posed procedurally
//! from state every client already has in its snapshot (velocity, ground,
//! ski, jet, aim, weapon cooldown, carried flag, death timer); a body going
//! down (dead, or tackled) goes limp instead, a ragdoll (`ragdoll`) that
//! lies as it lands. Render-only: the authoritative capsule, hitbox and
//! movement never read any of this.
//! Local frame: -Z forward, +Y up, the origin the player's position (the
//! collision sphere's centre), the soles of a standing body at
//! `gait::SOLE`. Bodies are drawn `STATURE` times the pose's size, the
//! soles on the ground under the sphere and the crown where it was, at the
//! top of the hit volume (`body_frame`).

use crate::drawlist::{EmitDraw, LitDraw, MeshId, ModelDraw};
use crate::sim::{weapon_reload, Player, Team};
use glam::{Mat4, Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Respawn delay the sim starts the death timer at; the collapse pose plays
/// over the first part of it.
const RESPAWN_DELAY: f32 = 3.4;
/// Bodies stay visible this long after death, then vanish until respawn.
pub(crate) const CORPSE_TIME: f32 = 2.6;
/// Stride cadence in radians per second for untracked poses. Tracked players
/// accumulate their stride phase at `cadence(speed)`, so faster runners take
/// quicker strides without the phase ever jumping.
pub(crate) const CADENCE: f32 = 9.0;

/// Stride rate in radians per second at a ground speed: half a turn a step,
/// at a runner's step rate (`gait::steps`), which is what the gait plants
/// its feet for.
pub fn cadence(speed: f32) -> f32 { TAU * 0.5 * crate::gait::steps(speed) }

/// Everything the pose depends on, lifted out of `Player` so the maths is a
/// pure function of its inputs.
#[derive(Clone, Copy, Debug)]
pub struct PoseInput {
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub jetting: bool,
    pub weapon: u8,
    /// Seconds since the last shot (large when idle).
    pub shot_age: f32,
    pub time: f32,
    pub seed: f32,
    /// Seconds since death, if dead.
    pub dead_for: Option<f32>,
    /// Landing squash, 0 to 1 (see `Motion::squash`).
    pub squash: f32,
    /// Weapon lowered for a switch, 0 to 1 (see `Motion::lower`).
    pub lower: f32,
    /// Stride phase in radians; advances with ground speed (see `Motion::stride`).
    pub stride: f32,
    /// What the hands are doing.
    pub hold: Hold,
    /// Fall direction and tilt for a downed body (see `Motion::fall_dir`).
    pub fall_dir: Vec2,
    pub fall_tilt: f32,
    /// The hands on a weapon's own grips (an armor with grip gloves holding
    /// an authored weapon), else None: the hands reach to fixed grips.
    pub grip: Option<GripHold>,
    /// Sitting in a vehicle's open seat: thighs forward, shins down.
    pub seated: bool,
    /// Football's throw, just made: 1 at the release, falling to 0 over
    /// `football::PASS_COOLDOWN`.
    pub throw: f32,
    /// Down from a tackle (alive, `dead_for` counting the fall): flat on
    /// the face, arms bent by the head to break the fall.
    pub tackled: bool,
    /// Eased weights (`Motion`): on the ground (1) or in the air (0),
    /// skiing, backpedalling (the stride run backward), a skier's hop
    /// (`hopping`) and a bound (a hop from a run, `gait::GaitIn::bound`).
    pub ground: f32,
    pub ski: f32,
    pub back: f32,
    pub hop: f32,
    pub bound: f32,
    /// The armor's mass for the gait: 0 light, 0.5 medium, 1 heavy.
    pub mass: f32,
    /// How much bigger than the pose the body is drawn on each axis
    /// (`body_size`): the gait runs at the speed in the pose's own units, so
    /// a planted foot stays put once drawn.
    pub size: Vec3,
    /// The blade's swing and guard (`Hold::Blade`).
    pub blade: peakrunner_core::sim::blade::State,
}

/// The armor's mass for the gait (`PoseInput::mass`).
pub fn mass_of(build: peakrunner_core::registry::Build) -> f32 {
    use peakrunner_core::registry::Build;
    match build { Build::Light => 0.0, Build::Medium => 0.5, Build::Heavy => 1.0 }
}

/// Whether a velocity (the body's frame) runs backward: a backpedal once it
/// turns past 110 degrees from the facing, back to a run inside 100, and
/// otherwise as `was` (so a strafe doesn't flicker between the two).
fn backpedal(local: Vec3, was: bool) -> bool {
    let speed = Vec3::new(local.x, 0.0, local.z).length();
    if speed < 1.0 { return was; }
    let ahead = -local.z / speed;
    if ahead < -0.34 { true } else if ahead > -0.17 { false } else { was }
}

/// Both hands on a weapon: each glove's frame and its wrist (where the
/// forearm meets the glove's cuff), left then right, in the aim frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripHold { pub gloves: [Mat4; 2], pub wrists: [Vec3; 2] }

/// What a player's hands hold, which decides how the arms are driven: gripping
/// a weapon (the hands reach to its grips), or free (the arm chain is posed
/// from joint angles, as later actions such as pointing or dancing will be).
/// Football is played empty-handed (since 2026-10-14): its `Carrier` tucks the
/// ball under the right arm and wears their team's flag on their back. In a
/// blade zone both hands hold the blade (`Blade`), posed by the swing
/// (`blade_look`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold { Weapon, Carrier, Free, Blade }

/// Where a Football carrier's right hand holds the ball against the ribs,
/// and which way the elbow goes (back and down, by the hip), in the chest's
/// frame; where a throw's follow-through reaches, forward and up. The arm
/// shows the throw while `PoseInput::throw` is over THROW_SHOWN.
const TUCK_HAND: Vec3 = Vec3::new(0.22, 0.24, -0.24);
const TUCK_ELBOW: Vec3 = Vec3::new(0.35, -1.0, 0.6);
const THROW_HAND: Vec3 = Vec3::new(0.30, 0.75, -0.45);
const THROW_ELBOW: Vec3 = Vec3::new(0.8, -1.0, 0.15);
const THROW_SHOWN: f32 = 0.35;

/// How far the upper body turns (radians, about Y) when both hands hold
/// something: the left shoulder forward.
const BLADE: f32 = -0.42;

/// Upper arm and forearm lengths (m).
pub(crate) const UPPER_ARM: f32 = 0.32;
pub(crate) const FOREARM: f32 = 0.30;

/// Forward kinematics for one arm: shoulder, elbow and wrist. `swing` raises
/// the arm forward about the shoulder (0 hangs it down, +pi/2 points it ahead),
/// `spread` lifts it out to the side, `bend` folds the elbow forward, and
/// `wrist` pitches the hand. Returns (elbow, wrist position, hand frame), all in
/// the frame `chest` maps into. `side` is -1 for the left arm, +1 for the right.
pub fn arm_chain(chest: Mat4, shoulder: Vec3, side: f32, swing: f32, spread: f32, bend: f32, wrist: f32) -> (Vec3, Vec3, Mat4) {
    let upper = chest * rot_x(swing) * Mat4::from_rotation_z(side * spread);
    let elbow = shoulder + upper.transform_vector3(Vec3::NEG_Y) * UPPER_ARM;
    let fore = upper * rot_x(bend);
    let hand = elbow + fore.transform_vector3(Vec3::NEG_Y) * FOREARM;
    let frame = Mat4::from_translation(hand) * Mat4::from_quat(Quat::from_mat4(&(fore * rot_x(wrist))).normalize());
    (elbow, hand, frame)
}

/// Two-bone reach: where the elbow goes for an arm from `shoulder` to
/// `target` with bones `upper` and `fore`, bending toward `pole` (a
/// direction). Both bones keep their lengths; out of reach, the arm points
/// straight at the target.
pub(crate) fn ik_elbow(shoulder: Vec3, target: Vec3, upper: f32, fore: f32, pole: Vec3) -> Vec3 {
    let d = target - shoulder;
    let dir = d.normalize_or(Vec3::NEG_Y);
    let len = d.length().clamp(1e-3, upper + fore - 1e-3);
    let cos = ((upper * upper + len * len - fore * fore) / (2.0 * upper * len)).clamp(-1.0, 1.0);
    let bend = (pole - dir * pole.dot(dir)).normalize_or(dir.any_orthonormal_vector());
    shoulder + dir * (upper * cos) + bend * (upper * (1.0 - cos * cos).max(0.0).sqrt())
}

/// A hand frame at `hand`, its -Y running along the forearm from `elbow`.
fn wrist_frame(elbow: Vec3, hand: Vec3) -> Mat4 {
    Mat4::from_translation(hand) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::NEG_Y, (hand - elbow).normalize_or(Vec3::NEG_Y)))
}

impl PoseInput {
    pub fn from_player(p: &Player, time: f32) -> Self {
        let reload = weapon_reload(p.weapon);
        let shot_age = if p.cooldown > 0.0 { (reload - p.cooldown).max(0.0) } else { 9.0 };
        // A tackled player falls into the collapse pose and stays down.
        let down = peakrunner_core::sim::football::TACKLE_STUN - p.stun;
        let dead_for = if !p.alive { Some((RESPAWN_DELAY - p.respawn).max(0.0)) }
            else if p.stun > 0.0 { Some(down.clamp(0.0, 0.8)) } else { None };
        PoseInput { vel: p.vel, yaw: p.yaw, pitch: p.pitch,
            jetting: p.jetting, weapon: crate::sim::arsenal::view_kind(p.weapon), shot_age, time,
            seed: seed_of(p.net_id), dead_for, squash: 0.0, lower: 0.0, stride: time * CADENCE, hold: Hold::Weapon,
            fall_dir: Vec2::new(0.0, -1.0), fall_tilt: FALL_TILT, grip: None, seated: p.riding > 0 && p.alive, throw: 0.0,
            tackled: p.alive && p.stun > 0.0, ground: if p.on_ground { 1.0 } else { 0.0 },
            ski: if p.skiing && p.on_ground { 1.0 } else { 0.0 },
            back: if backpedal(Mat4::from_rotation_y(-p.yaw).transform_vector3(p.vel), false) { 1.0 } else { 0.0 },
            hop: if hopping(p, 0.0) { 1.0 } else { 0.0 }, bound: 0.0,
            mass: mass_of(p.armor.def().build), size: body_size(p, armor_model(p.armor.def().build).is_some(), true), blade: p.blade }
    }
}

/// Short-lived animation cues derived from how a player's snapshot state
/// changes frame to frame: a hard landing and a weapon switch. Nothing here
/// is sent over the network; every client derives it the same way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    /// Seconds since the last hard landing, and how hard it was (0 to 1).
    pub land_age: f32,
    pub land_strength: f32,
    /// Seconds since the last weapon switch, and the weapon switched from.
    pub switch_age: f32,
    pub switch_from: u8,
    /// Stride phase, advanced only while on the ground (0 = untracked).
    pub stride: f32,
    /// How a downed body falls: the direction in the player's frame (x, z;
    /// -z is forward) and how far it may tip (radians), chosen when the
    /// player went down so the body never lies through a wall or pillar.
    pub fall_dir: Vec2,
    pub fall_tilt: f32,
    /// Eased gait weights (`PoseInput::ground`, `ski`, `back`, `hop`,
    /// `bound`): takeoff, landing, a ski starting or stopping, a turn into a
    /// backpedal and a hop blend over a few frames instead of snapping.
    pub ground: f32,
    pub ski: f32,
    pub back: f32,
    pub hop: f32,
    pub bound: f32,
    /// Who the body is for its looks and its gait's seed (`anim_who`; 0:
    /// untracked, the player's network ID).
    pub who: u32,
    /// Seconds since death, while dead (the tracker's own count).
    pub dead_for: Option<f32>,
}

/// The tracker's key for player `i`: its network ID; offline, where every
/// player's is 0, its slot. (All of an offline match's bodies used to share
/// one track: each frame every player advanced its stride, so the legs ran
/// several times too fast, and took each other's landings and hops.)
pub fn anim_id(i: usize, p: &Player) -> u64 {
    if p.net_id != 0 { p.net_id as u64 } else { 1 << 32 | i as u64 }
}

/// Who player `i` is for a face and a gait seed: its network ID, or offline
/// its slot counted from 1, so offline bodies don't all look and move alike.
fn anim_who(i: usize, p: &Player) -> u32 { if p.net_id != 0 { p.net_id } else { i as u32 + 1 } }

/// A body's gait seed from who it is (`anim_who`), kept small: the gait
/// adds it to the stride phase, and a server bot's ID (from 0xF000_0000)
/// made a seed so large that the stride vanished in its rounding, so the
/// legs of every bot online stood still while it slid along.
pub fn seed_of(who: u32) -> f32 { (who % 997) as f32 * 0.7 }

/// A full fall: flat on the ground.
const FALL_TILT: f32 = 1.42;
/// Seconds a body takes to go down.
const FALL_TIME: f32 = 0.55;
/// The ground under the collision sphere's centre (the root) is this far
/// below it (`terrain::PLAYER_RADIUS`).
const SPHERE_RADIUS: f32 = peakrunner_core::terrain::PLAYER_RADIUS;
/// How high a body lying down keeps its spine off the ground (m): face
/// down on its chest plate, more on its back, where the jetpack is.
const LIE_FRONT: f32 = 0.17;
const LIE_BACK: f32 = 0.34;
/// A tackled body: tipped all the way onto its face.
const PRONE_TILT: f32 = 1.5;

/// How far a downed body has gone over: 0 standing, 1 down.
pub fn fallen(dead_for: Option<f32>) -> f32 {
    dead_for.map_or(0.0, |t| { let k = (t / FALL_TIME).min(1.0); k * k * (3.0 - 2.0 * k) })
}
/// Feet to crown of a fallen body, and the room kept clear past it (m).
const BODY_LENGTH: f32 = 1.9 * STATURE;

impl Default for Motion {
    fn default() -> Self { Motion { land_age: 9.0, land_strength: 0.0, switch_age: 9.0, switch_from: 0, stride: 0.0,
        fall_dir: Vec2::new(0.0, -1.0), fall_tilt: FALL_TILT, ground: 1.0, ski: 0.0, back: 0.0, hop: 0.0, bound: 0.0, who: 0, dead_for: None } }
}

/// The longest a jump stays a hop (s): past it the legs hang (`gait`).
const HOP_AIR: f32 = 0.9;

/// A low hop at speed: off the ground without the jet for `air` seconds,
/// moving and not falling fast. Football's held jump hops from landing to
/// landing (its skiing); a jump from a run is one hop.
fn hopping(p: &Player, air: f32) -> bool {
    !p.on_ground && !p.jetting && air < HOP_AIR && p.vel.y > -9.0 && Vec2::new(p.vel.x, p.vel.z).length() > 3.0
}

/// Where a body going down should fall: the way it was knocked (its
/// velocity), or the other way if that side has more room, tipping only as
/// far as the room allows. `clear(from, to)` is how far a ray gets before
/// the map blocks it.
fn fall_for(p: &Player, clear: &dyn Fn(Vec3, Vec3) -> f32) -> (Vec2, f32) {
    let flat = Vec3::new(p.vel.x, 0.0, p.vel.z);
    let knock = if flat.length() > 1.0 { flat.normalize() } else { Mat4::from_rotation_y(p.yaw).transform_vector3(Vec3::NEG_Z) };
    let body = p.pos + Vec3::Y * 0.8;
    let reach = BODY_LENGTH + 0.5;
    let room = |d: Vec3| clear(body, body + d * reach);
    let (ahead, behind) = (room(knock), room(-knock));
    let (dir, space) = if behind > ahead + 0.3 { (-knock, behind) } else { (knock, ahead) };
    let tilt = if space >= reach - 0.01 { FALL_TILT } else { ((space - 0.35) / BODY_LENGTH).clamp(0.05, 1.0).asin().min(FALL_TILT) };
    let local = Mat4::from_rotation_y(-p.yaw).transform_vector3(dir);
    (Vec2::new(local.x, local.z).normalize_or(Vec2::new(0.0, -1.0)), tilt)
}

/// A landing's squash and recovery, and a weapon switch's lower and raise.
const SQUASH_TIME: f32 = 0.38;
const SWITCH_TIME: f32 = 0.32;
/// Falls slower than this don't squash; airborne blips shorter than
/// `MIN_AIR` (snapshot jitter over bumps) aren't landings.
const SQUASH_FALL: f32 = 8.0;
const MIN_AIR: f32 = 0.15;

impl Motion {
    /// Squash amount: a quick dip over the first 0.08 s, then a smooth recovery.
    pub fn squash(&self) -> f32 {
        let t = self.land_age;
        if t >= SQUASH_TIME { return 0.0; }
        let dip = (t / 0.08).min(1.0);
        let rise = ((t - 0.08).max(0.0) / (SQUASH_TIME - 0.08)).min(1.0);
        let ease = |k: f32| k * k * (3.0 - 2.0 * k);
        self.land_strength * ease(dip) * (1.0 - ease(rise))
    }
    /// Weapon lowered: down and back up over `SWITCH_TIME`.
    pub fn lower(&self) -> f32 {
        if self.switch_age >= SWITCH_TIME { 0.0 } else { (self.switch_age / SWITCH_TIME * PI).sin() }
    }
    /// The weapon shown: the old one while lowering, the new one on the way up.
    pub fn shown_weapon(&self, current: u8) -> u8 {
        if self.switch_age < SWITCH_TIME * 0.5 { self.switch_from } else { current }
    }
}

struct Track { id: u64, alive: bool, down: bool, grounded: bool, air: f32, fall: f32, weapon: u8, motion: Motion,
    /// The body while it is down: limp, falling and lying as it lands.
    ragdoll: Option<crate::ragdoll::Ragdoll>,
    /// Running backward (`backpedal`), which `motion.back` eases toward.
    back: bool,
    /// This flight is a bound (a hop from a run, not from skis), the stride
    /// phase still to go to its landing, and how long the last hop flew
    /// (s), which times this one's.
    bounding: bool, bound_left: f32, last_air: f32,
    /// Seconds since death, while dead, and the body as it last lay.
    dead_age: f32, body: Option<Player> }

/// A body left down after its player is back in play (Football's, for
/// `football::BODY_LIES`): the player as they lay, their ragdoll, and its
/// age since the death.
struct Corpse { player: Player, ragdoll: crate::ragdoll::Ragdoll, age: f32, keep: f32 }

/// A hop's flight before any has been timed (s): about Football's.
const FIRST_HOP: f32 = 0.45;

/// A fresh track's gait weights, straight from the player's state.
fn settled(p: &Player) -> Motion {
    let i = PoseInput::from_player(p, 0.0);
    Motion { ground: i.ground, ski: i.ski, back: i.back, hop: i.hop, ..Motion::default() }
}

/// A player's frame as drawn: where they are, their facing and their
/// armor's bulk (`push_player`). With `kit`: an authored armor drawn.
fn body_frame(p: &Player, kit: bool, armed: bool) -> Mat4 {
    let size = body_size(p, kit, armed);
    // Grown from the soles, which stand on the ground under the sphere; a
    // rider from the pelvis, which stays on the seat.
    let feet = if p.riding > 0 { crate::gait::PELVIS_Y * (1.0 - size.y) } else { -SPHERE_RADIUS - size.y * crate::gait::SOLE };
    Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.yaw) * Mat4::from_translation(Vec3::Y * feet) * Mat4::from_scale(size)
}

/// Player `p`'s body frame as drawn (`body_frame`), moved to `at`: what is
/// fixed to a body is placed in it, in the pose's units.
pub(crate) fn drawn_frame(p: &Player, at: Vec3) -> Mat4 {
    Mat4::from_translation(at - p.pos) * body_frame(p, armor_model(p.armor.def().build).is_some(), true)
}

/// Where the barrel of player `p`'s held weapon ends as their body draws it
/// at `at` (its authored model's `muzzle`), standing as they stand; None
/// for a weapon without a model.
pub(crate) fn held_muzzle(p: &Player, at: Vec3) -> Option<Vec3> {
    let (_, model) = weapon_model(p.weapon)?;
    let end = model.point("muzzle")?;
    let kit = armor_model(p.armor.def().build);
    let grip = kit.and_then(|(_, kit)| grip_hold(kit, model, held_frame(model)));
    let input = PoseInput { grip, size: body_size(p, kit.is_some(), true), ..PoseInput::from_player(p, 0.0) };
    Some((drawn_frame(p, at) * pose(&input).aim * held_frame(model) * end).transform_point3(Vec3::ZERO))
}

/// How much bigger than the pose a body is drawn on each axis: `STATURE`,
/// and heavier armor broader, deeper and a little taller (authored armor
/// carries its own size, so less).
fn body_size(p: &Player, kit: bool, armed: bool) -> Vec3 {
    let bulk = match (kit, p.armor.def().build) {
        (true, peakrunner_core::registry::Build::Heavy) => Vec3::new(1.12, 1.06, 1.14),
        (true, peakrunner_core::registry::Build::Medium) => Vec3::new(1.05, 1.03, 1.06),
        (false, peakrunner_core::registry::Build::Heavy) if armed => Vec3::new(1.22, 1.1, 1.25),
        _ => Vec3::ONE,
    };
    bulk * STATURE
}

/// A player's ground speed in the pose's units (`PoseInput::size`), which
/// the stride's cadence follows.
fn gait_speed(p: &Player) -> f32 {
    let size = body_size(p, armor_model(p.armor.def().build).is_some(), true);
    let v = Mat4::from_rotation_y(-p.yaw).transform_vector3(p.vel);
    Vec2::new(v.x / size.x, v.z / size.z).length()
}

/// The top of a standing body's helmet in the pose (its shell; the light
/// armor's antenna rises past it).
const CROWN: f32 = 1.86;
/// How much bigger than the pose bodies are drawn, grown from the soles:
/// the soles on the ground under the collision sphere, `SPHERE_RADIUS`
/// below the player's position (the pose's own sit `gait::SOLE` below it),
/// and the crown where it was, at the top of the hit volume. The bodies were
/// drawn at the pose's size, floating 0.37 m over the ground.
pub(crate) const STATURE: f32 = (CROWN + SPHERE_RADIUS) / (CROWN - crate::gait::SOLE);

/// A ragdoll for player `p` going down now, from how they stood (`stride`:
/// their legs' place in the run), knocked toward `fall_dir` (their frame).
fn go_limp(p: &Player, stride: f32, fall_dir: Vec2, tackled: bool) -> crate::ragdoll::Ragdoll {
    let standing = PoseInput { dead_for: None, tackled: false, stride, hold: Hold::Free, ..PoseInput::from_player(p, 0.0) };
    // A tackle sends them onto their face, whichever way it came from.
    let dir = if tackled && fall_dir.y > 0.0 { -fall_dir } else { fall_dir };
    let knock = Mat4::from_rotation_y(p.yaw).transform_vector3(Vec3::new(dir.x, 0.0, dir.y));
    crate::ragdoll::Ragdoll::new(&pose(&standing), body_frame(p, armor_model(p.armor.def().build).is_some(), true), p.vel, knock, tackled)
}

/// Per-player animation memory for the client, keyed by `anim_id`.
#[derive(Default)]
pub struct AnimTracker { tracks: Vec<Track>, corpses: Vec<Corpse> }

impl AnimTracker {
    /// Observe this frame's players and age the cues by `dt`. `clear(from,
    /// to)` measures free space in the map, for where fallen bodies lie.
    /// `ground(p)`: the floor's height under `p`, for ragdolls.
    /// `keep`: how long a dead body stays down after its player respawns
    /// (s; 0: it goes at once).
    pub fn update(&mut self, players: &[Player], dt: f32, clear: &dyn Fn(Vec3, Vec3) -> f32, ground: &dyn Fn(Vec3) -> f32, keep: f32) {
        for c in &mut self.corpses {
            c.age += dt;
            let at = c.player.pos;
            c.ragdoll.step(dt, at, ground, clear);
        }
        self.corpses.retain(|c| c.age < c.keep);
        if keep <= 0.0 { self.corpses.clear(); }
        self.tracks.retain(|t| players.iter().enumerate().any(|(i, p)| anim_id(i, p) == t.id));
        for (i, p) in players.iter().enumerate() {
            let id = anim_id(i, p);
            let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) else {
                // First sight: nothing to animate yet.
                let motion = Motion { who: anim_who(i, p), ..settled(p) };
                self.tracks.push(Track { id, alive: p.alive, down: !p.alive || p.stun > 0.0,
                    grounded: p.on_ground, air: 0.0, fall: 0.0, weapon: p.weapon, back: motion.back > 0.5, motion, ragdoll: None,
                    bounding: false, bound_left: 0.0, last_air: FIRST_HOP, dead_age: 0.0, body: None });
                continue;
            };
            t.motion.land_age += dt;
            t.motion.switch_age += dt;
            // Leaving the ground: a hop from a run is a bound, its flight
            // carrying the stride on to the other foot's landing (a step
            // and more when the takeoff came just before one). A skier's
            // hop keeps the ski stance.
            if !p.on_ground && t.grounded {
                t.bounding = t.motion.ski < 0.5;
                let at = t.motion.stride + seed_of(t.motion.who);
                let next = ((at / PI).floor() + 1.0) * PI - at;
                t.bound_left = if next < 0.35 * PI { next + PI } else { next };
            }
            if p.on_ground {
                let stride = (t.motion.stride + dt * cadence(gait_speed(p))) % (TAU * 64.0);
                t.motion.stride = stride.max(1e-3);
            } else if t.bounding && hopping(p, t.air) {
                // Over the flight, as long as the last hop's.
                let step = (t.bound_left * dt / (t.last_air - t.air).max(0.05)).min(t.bound_left);
                t.motion.stride = ((t.motion.stride + step) % (TAU * 64.0)).max(1e-3);
                t.bound_left -= step;
            }
            // The gait's weights ease toward the state: a takeoff over a
            // few frames, a landing quicker; a ski or a backpedal starting
            // or stopping in a tenth of a second or so.
            let ease = |v: &mut f32, to: f32, rate: f32| *v += (to - *v) * (1.0 - (-dt * rate).exp());
            let land = if p.on_ground { 18.0 } else { 6.0 };
            ease(&mut t.motion.ground, if p.on_ground { 1.0 } else { 0.0 }, land);
            ease(&mut t.motion.ski, if p.skiing && p.on_ground { 1.0 } else { 0.0 }, 9.0);
            // A hop's stance holds through the touch before the next one.
            let hop = hopping(p, t.air);
            let (bound, tuck) = (hop && t.bounding, hop && !t.bounding);
            ease(&mut t.motion.hop, if tuck { 1.0 } else { 0.0 }, if tuck { 10.0 } else { 8.0 });
            ease(&mut t.motion.bound, if bound { 1.0 } else { 0.0 }, if bound { 10.0 } else { 8.0 });
            t.back = backpedal(Mat4::from_rotation_y(-p.yaw).transform_vector3(p.vel), t.back);
            ease(&mut t.motion.back, if t.back { 1.0 } else { 0.0 }, 10.0);
            let down = !p.alive || p.stun > 0.0;
            if p.alive != t.alive {
                // Back in play: the body stays down a while where it lies
                // (`keep`), its ragdoll still settling.
                if p.alive && keep > t.dead_age {
                    if let (Some(ragdoll), Some(body)) = (t.ragdoll.take(), t.body.take()) {
                        self.corpses.push(Corpse { player: body, ragdoll, age: t.dead_age, keep });
                    }
                }
                // Death or respawn: reset without a cue (a death picks where
                // the body falls).
                let stride = t.motion.stride;
                *t = Track { id, alive: p.alive, down, grounded: p.on_ground, air: 0.0, fall: 0.0,
                    weapon: p.weapon, motion: Motion { who: t.motion.who, ..settled(p) }, ragdoll: None, back: false,
                    bounding: false, bound_left: 0.0, last_air: FIRST_HOP, dead_age: 0.0, body: None };
                if !p.alive { t.motion.dead_for = Some(0.0); t.body = Some(p.clone()); }
                if down {
                    (t.motion.fall_dir, t.motion.fall_tilt) = fall_for(p, clear);
                    t.ragdoll = Some(go_limp(p, stride, t.motion.fall_dir, false));
                }
                continue;
            }
            if down && !t.down {
                // Tackled: fall the way the hit sent you, where there's room.
                (t.motion.fall_dir, t.motion.fall_tilt) = fall_for(p, clear);
                t.ragdoll = Some(go_limp(p, t.motion.stride, t.motion.fall_dir, p.alive));
            }
            if !down { t.ragdoll = None; }
            if let Some(r) = t.ragdoll.as_mut() { r.step(dt, p.pos, ground, clear); }
            if !p.alive {
                t.dead_age += dt;
                t.motion.dead_for = Some(t.dead_age);
                t.body = Some(p.clone());
            }
            t.down = down;
            if !p.on_ground {
                t.air += dt;
                t.fall = t.fall.max(-p.vel.y);
            } else if !t.grounded {
                // A bound's airtime times the next one.
                if t.bounding && t.air < HOP_AIR { t.last_air = t.air.max(0.15); }
                t.bounding = false;
                if t.air >= MIN_AIR && t.fall > SQUASH_FALL {
                    t.motion.land_age = 0.0;
                    t.motion.land_strength = ((t.fall - SQUASH_FALL) / 22.0).clamp(0.25, 1.0);
                }
                t.air = 0.0;
                t.fall = 0.0;
            }
            t.grounded = p.on_ground;
            if p.alive && p.weapon != t.weapon {
                t.motion.switch_from = t.weapon;
                t.motion.switch_age = 0.0;
                t.weapon = p.weapon;
            }
        }
    }

    /// Bodies left down after their players respawned: each as it lay, its
    /// ragdoll's points (sinking into the ground over the last
    /// `football::BODY_SINKS`) and its seconds since the death.
    pub fn corpses(&self) -> impl Iterator<Item = (&Player, [Vec3; crate::ragdoll::POINTS], f32)> + '_ {
        self.corpses.iter().map(|c| {
            let sink = ((c.age - (c.keep - peakrunner_core::sim::football::BODY_SINKS)) / peakrunner_core::sim::football::BODY_SINKS).clamp(0.0, 1.0);
            (&c.player, c.ragdoll.points().map(|q| q - Vec3::Y * sink * 0.6), c.age)
        })
    }

    /// Player `id`'s ragdoll's points, while they are down.
    pub fn ragdoll(&self, id: u64) -> Option<&[Vec3; crate::ragdoll::POINTS]> {
        self.tracks.iter().find(|t| t.id == id).and_then(|t| t.ragdoll.as_ref()).map(|r| r.points())
    }

    pub fn motion(&self, id: u64) -> Motion {
        self.tracks.iter().find(|t| t.id == id).map_or_else(Motion::default, |t| t.motion)
    }
}

/// Joint positions and frames in the local (feet) frame.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// Whole-body transform on top of the root: death tip-over, ski lean.
    pub body: Mat4,
    pub pelvis: Vec3,
    /// The pelvis's frame (upright but for a ragdoll's).
    pub pelvis_frame: Mat4,
    pub chest: Mat4,
    pub head: Mat4,
    /// Weapon frame: at the right grip, -Z along the aim, recoil applied.
    pub aim: Mat4,
    pub hips: [Vec3; 2],
    pub knees: [Vec3; 2],
    pub ankles: [Vec3; 2],
    /// Foot frames at the ankles: pitched by the ankle joint (toes down is
    /// positive), -Z along the foot.
    pub feet: [Mat4; 2],
    pub shoulders: [Vec3; 2],
    pub elbows: [Vec3; 2],
    pub hands: [Vec3; 2],
    /// Hand frames at the wrists (-Y along the forearm, -Z the palm's facing).
    pub wrists: [Mat4; 2],
    /// The grip gloves' frames when the hands are on a weapon's grips
    /// (`PoseInput::grip`), left then right.
    pub grips: Option<[Mat4; 2]>,
}

fn rot_x(a: f32) -> Mat4 { Mat4::from_rotation_x(a) }

/// Pure, deterministic pose from the inputs.
pub fn pose(i: &PoseInput) -> Pose {
    // The velocity in the pose's units: drawn `size` times bigger, the
    // feet keep pace with the ground.
    let local_vel = Mat4::from_rotation_y(-i.yaw).transform_vector3(i.vel) / Vec3::new(i.size.x, 1.0, i.size.z);
    let speed = Vec3::new(local_vel.x, 0.0, local_vel.z).length();
    let breathe = (i.time * 1.7 + i.seed).sin();
    let downed = i.dead_for.is_some();
    let squash = if downed { 0.0 } else { i.squash };

    // Going down: the feet come to the ground (the root is the collision
    // sphere's centre) as the body tips over them and settles there. A
    // tackle lays it on its face, turned the way it was hit; a death tips
    // it the way it was knocked, only as far as there's room.
    let fall = fallen(i.dead_for);
    // A tackle knocking someone backward still puts them on their face:
    // they pitch forward rather than spin round.
    let dir = if i.tackled && i.fall_dir.y > 0.0 { -i.fall_dir } else { i.fall_dir };
    let toward = Vec3::new(dir.x, 0.0, dir.y).normalize_or(Vec3::NEG_Z);
    // Knocked forward or to a side, a body lands on its face (a tackle
    // always does), turning to face the way it goes; knocked back, on its
    // back. It tips only as far as the room it chose allows.
    let prone = i.tackled || i.fall_dir.y <= 0.0;
    let room = if i.tackled { 1.0 } else { i.fall_tilt / FALL_TILT };
    let body = match i.dead_for {
        Some(_) => {
            let (face, tip, lie) = if prone { ((-toward.x).atan2(-toward.z), -PRONE_TILT, LIE_FRONT) }
                else { (toward.x.atan2(toward.z), PRONE_TILT, LIE_BACK) };
            Mat4::from_translation(Vec3::Y * ((lie * room + crate::gait::SOLE) * fall)) * Mat4::from_rotation_y(face * fall)
                * rot_x(tip * room * fall)
        }
        None => Mat4::IDENTITY,
    };

    // Standing, running, skiing and flying: the hips and the feet from the
    // gait (`gait`), the legs reaching the feet. Sitting and going down:
    // the legs from joint angles.
    let walking = !downed && !i.seated;
    let g = crate::gait::gait(&crate::gait::GaitIn { vel: local_vel, phase: i.stride + i.seed, ground: i.ground,
        ski: i.ski, back: i.back, hop: i.hop, bound: i.bound, mass: i.mass, jetting: i.jetting, time: i.time, seed: i.seed });
    let sitting = Mat4::from_translation(Vec3::new(0.0, crate::gait::PELVIS_Y, 0.0));
    // A landing's squash drops the hips, and the planted legs fold under them.
    let (pelvis_frame, lean, lean_side) = if walking {
        (Mat4::from_translation(Vec3::Y * (-0.2 * squash)) * g.pelvis, g.lean + 0.14 * squash, g.lean_side)
    } else if i.seated {
        // Leaning a little toward the handlebars.
        (sitting, 0.2, 0.0)
    } else { (sitting, 0.0, 0.0) };
    let pelvis = pelvis_frame.w_axis.truncate();
    // Holding a weapon two-handed, the upper body blades: the left
    // shoulder turns forward so the support hand reaches the fore-grip
    // (the head and the aim still face ahead). Running, the shoulders turn
    // a little with the hips' swivel.
    // Going down, the hands let go (the weapon isn't drawn).
    let hold = if downed { Hold::Free } else { i.hold };
    let twist = if !downed && hold == Hold::Weapon { BLADE } else { 0.0 } + if walking { g.twist } else { 0.0 };
    let upright = Mat4::from_translation(pelvis) * rot_x(-lean) * Mat4::from_rotation_z(-lean_side)
        * Mat4::from_scale(Vec3::new(1.0, 1.0 + 0.008 * breathe, 1.0));
    let chest = upright * Mat4::from_rotation_y(twist);
    let neck = chest.transform_point3(Vec3::new(0.0, 0.58, 0.0));
    let look = i.pitch.clamp(-1.1, 1.1);
    let head = if downed && prone {
        // Face down: turned to one side, chin up off the ground.
        Mat4::from_translation(neck) * Mat4::from_rotation_y(0.95 * fall) * rot_x(0.45 * fall)
    } else if downed {
        // On the back: lolled to one side.
        Mat4::from_translation(neck) * Mat4::from_rotation_y(0.7 * fall) * rot_x(-0.15 * fall)
    // The gaze stays level over a leaning body: the head tips only a little
    // with it.
    } else { Mat4::from_translation(neck) * rot_x(look * 0.55 - lean * 0.25) };

    // Legs: hip, knee and ankle. Walking, each reaches its foot with the
    // knee bending toward the gait's pole; otherwise from joint angles:
    // positive hip swings the leg forward (-Z), knee bend folds the shin
    // back, positive foot angle points the toes down.
    let mut hips = [Vec3::ZERO; 2];
    let mut knees = [Vec3::ZERO; 2];
    let mut ankles = [Vec3::ZERO; 2];
    let mut feet = [Mat4::IDENTITY; 2];
    for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let hip = pelvis_frame.transform_point3(Vec3::new(side * crate::gait::HIP.x, crate::gait::HIP.y, crate::gait::HIP.z));
        hips[s] = hip;
        if walking {
            let knee = ik_elbow(hip, g.ankles[s], crate::gait::THIGH, crate::gait::SHIN, g.poles[s]);
            // Out of reach, the shin points at its ankle at full length.
            let ankle = knee + (g.ankles[s] - knee).normalize_or(Vec3::NEG_Y) * crate::gait::SHIN;
            knees[s] = knee; ankles[s] = ankle;
            feet[s] = Mat4::from_translation(ankle) * Mat4::from_quat(Quat::from_mat4(&g.feet[s]).normalize());
            continue;
        }
        let (hip_a, knee_a, splay, toes) = if downed && prone {
            // Legs straight out behind, apart, the toes down in the ground
            // (a bent knee would lift the foot into the air).
            let k = fall;
            (k * (-0.04 + side * 0.03), 0.12 + k * if side < 0.0 { 0.16 } else { 0.06 }, 0.04 + 0.12 * k, -0.35 * k)
        } else if downed {
            // On the back: legs out, a little apart, one knee slightly up.
            let k = fall;
            (k * (0.05 + side * 0.04), 0.12 + k * if side < 0.0 { 0.35 } else { 0.08 }, 0.04 + 0.1 * k, 0.6 * k)
        } else {
            // Astride: thighs forward and a little apart, shins down to the pegs.
            (1.3, 1.45, 0.14, 0.25)
        };
        let thigh_dir = (rot_x(hip_a) * Mat4::from_rotation_z(side * splay)).transform_vector3(Vec3::NEG_Y);
        let knee = hip + thigh_dir * crate::gait::THIGH;
        let shin_dir = rot_x(hip_a - knee_a).transform_vector3(Vec3::NEG_Y);
        let ankle = knee + shin_dir * crate::gait::SHIN;
        knees[s] = knee; ankles[s] = ankle;
        feet[s] = Mat4::from_translation(ankle) * rot_x(-toes);
    }

    // Aim frame: shoulders follow the chest, the weapon follows yaw/pitch.
    let shoulders = [
        chest.transform_point3(Vec3::new(-0.30, 0.47, 0.0)),
        chest.transform_point3(Vec3::new(0.30, 0.47, 0.0)),
    ];
    let (back, rise, _) = crate::drawlist::recoil(i.weapon, i.shot_age, i.time);
    // The arms soak half the hips' rise and fall, so the weapon rides
    // steadier than the body carrying it.
    let soak = if walking { -0.5 * g.bob } else { 0.0 };
    // A weapon switch dips the gun down and in, then brings the new one up.
    let lower = if i.dead_for.is_some() { 0.0 } else { i.lower };
    let aim = if hold == Hold::Blade {
        // The blade's pose in the frame of the shoulders' middle, square to
        // the hips, tipped a little with the look.
        let mid = (shoulders[0] + shoulders[1]) * 0.5 + Vec3::Y * soak;
        Mat4::from_translation(mid) * rot_x(look * 0.4) * crate::blade_look::frame(crate::blade_look::pose(&i.blade, 0.0))
    } else {
        Mat4::from_translation(upright.transform_point3(Vec3::new(0.13, 0.30 + soak - 0.12 * lower, 0.08 * lower)))
            * rot_x(look * 0.9 + rise * 2.0 - 1.0 * lower)
            * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.30 + back * 2.5))
    };
    // Arms: three joints each. Reaching hands go to their places with both
    // bones at full length: a weapon's grips (its own, through `i.grip`,
    // else fixed ones), the elbows bending down and out; a Football
    // carrier's right hand on the ball at the ribs, the elbow back by the
    // hip; a throw's follow-through, forward and up. Free arms are posed
    // from joint angles.
    let out = |side: f32| chest.transform_vector3(Vec3::new(side * 0.8, -1.0, 0.15));
    let reach: [Option<(Vec3, Vec3)>; 2] = match hold {
        // Left on the fore-grip, right on the grip.
        Hold::Weapon => {
            let g = match i.grip {
                Some(g) => [aim.transform_point3(g.wrists[0]), aim.transform_point3(g.wrists[1])],
                None => [aim.transform_point3(Vec3::new(-0.06, -0.02, -0.34)), aim.transform_point3(Vec3::new(0.0, -0.09, 0.06))],
            };
            [Some((g[0], out(-1.0))), Some((g[1], out(1.0)))]
        }
        Hold::Carrier => [None, Some((chest.transform_point3(TUCK_HAND), chest.transform_vector3(TUCK_ELBOW)))],
        Hold::Free if i.throw > THROW_SHOWN && !downed => [None, Some((chest.transform_point3(THROW_HAND), chest.transform_vector3(THROW_ELBOW)))],
        Hold::Free => [None, None],
        // Both hands on the hilt, the right above the left.
        Hold::Blade => [Some((aim.transform_point3(crate::blade_look::LEFT_HAND), out(-1.0))),
            Some((aim.transform_point3(crate::blade_look::RIGHT_HAND), out(1.0)))],
    };
    // Free arms pump against the legs, harder the faster the run.
    let pump = if walking { g.stride * (0.25 + 0.45 * crate::gait::smoothstep(2.0, 10.0, speed)) } else { 0.0 };
    let mut hands = [Vec3::ZERO; 2];
    let mut elbows = [Vec3::ZERO; 2];
    let mut wrists = [Mat4::IDENTITY; 2];
    for s in 0..2 {
        let side = if s == 0 { -1.0 } else { 1.0 };
        match reach[s].filter(|_| !(downed && hold != Hold::Weapon)) {
            Some((target, pole)) => {
                elbows[s] = ik_elbow(shoulders[s], target, UPPER_ARM, FOREARM, pole);
                hands[s] = target;
                wrists[s] = wrist_frame(elbows[s], hands[s]);
            }
            None => {
                // Running: arms swing opposite the legs, elbows bent. In the
                // air: loose, a little out from the body, swaying. Dead: limp.
                let (swing, spread, bend) = if downed && prone {
                    // Flung out as they went down: the left arm ahead past the
                    // head, the right back along the side.
                    if side < 0.0 { (fall * 2.85, 0.12 + 0.25 * fall, 0.25) } else { (0.1, 0.12 + 0.18 * fall, 0.3) }
                } else if downed {
                    // On the back: arms out to the sides on the ground.
                    (0.15 * fall, 0.12 + 1.15 * fall, 0.35)
                } else {
                    // The right arm comes forward as the left leg does.
                    let run = (0.12 - side * g.arms * pump, 0.12, 0.4 + 1.0 * pump / 0.7);
                    let sway = (i.time * 1.2 + i.seed + side).sin();
                    let air = (0.15 + sway * 0.08, 0.28 + sway * 0.05, 0.35);
                    let a = 1.0 - i.ground;
                    (run.0 + (air.0 - run.0) * a, run.1 + (air.1 - run.1) * a, run.2 + (air.2 - run.2) * a)
                };
                let (elbow, hand, frame) = arm_chain(chest, shoulders[s], side, swing, spread, bend, 0.1);
                elbows[s] = elbow; hands[s] = hand; wrists[s] = frame;
            }
        }
    }
    let gloves = match (hold, i.grip) {
        (Hold::Weapon, Some(g)) => Some([aim * g.gloves[0], aim * g.gloves[1]]),
        _ => None,
    };
    Pose { body, pelvis, pelvis_frame, chest, head, aim, hips, knees, ankles, feet, shoulders, elbows, hands, wrists, grips: gloves }
}

/// How much detail a player is drawn with (`push_player`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod { Far, Near, Close }

/// Chin-to-crown height of the head (m); `character_mesh::head_mesh` is 1 tall.
const HEAD_SIZE: f32 = 0.235;

/// A person's own colours: skin, brows and eyes, fixed by their id.
struct Look { skin: Vec3, hair: Vec3, iris: Vec3 }

fn person_look(id: u32) -> Look {
    const SKIN: [Vec3; 7] = [Vec3::new(0.93, 0.78, 0.66), Vec3::new(0.87, 0.68, 0.55), Vec3::new(0.78, 0.57, 0.43),
        Vec3::new(0.66, 0.47, 0.33), Vec3::new(0.53, 0.36, 0.24), Vec3::new(0.40, 0.27, 0.18), Vec3::new(0.29, 0.19, 0.13)];
    const HAIR: [Vec3; 6] = [Vec3::new(0.05, 0.04, 0.035), Vec3::new(0.13, 0.085, 0.055), Vec3::new(0.25, 0.16, 0.09),
        Vec3::new(0.34, 0.14, 0.07), Vec3::new(0.55, 0.42, 0.24), Vec3::new(0.45, 0.44, 0.42)];
    const IRIS: [Vec3; 5] = [Vec3::new(0.24, 0.13, 0.06), Vec3::new(0.08, 0.05, 0.03), Vec3::new(0.2, 0.36, 0.56),
        Vec3::new(0.22, 0.35, 0.2), Vec3::new(0.36, 0.26, 0.1)];
    let h = id.wrapping_add(0x9e37_79b9).wrapping_mul(0x85eb_ca6b).rotate_right(13).wrapping_mul(0xc2b2_ae35);
    Look { skin: SKIN[(h % 7) as usize], hair: HAIR[((h >> 8) % 6) as usize], iris: IRIS[((h >> 16) % 5) as usize] }
}

/// How much of an armor's bare plates take its team's color
/// (`ModelDraw::plates`): all of them, so a body's team shows from any side
/// and at any range, the way the classic told teams apart by their armor.
pub const TEAM_PLATES: f32 = 1.0;

pub(crate) fn team_color(team: Team) -> Vec3 {
    if team == Team::Ember { Vec3::new(0.74, 0.20, 0.12) } else { Vec3::new(0.12, 0.51, 0.65) }
}

/// Carried-flag cloth colour: the colour of the flag being carried.
/// The rabbit flag's gold cloth, the same on the ground and on the rabbit.
pub const RABBIT_FLAG: Vec3 = Vec3::new(0.96, 0.78, 0.24);

/// What a body wears: its armor's paint, how much of the bare plates take
/// it (`ModelDraw::plates`) and how brightly the paint lights itself
/// (`ModelDraw::shine`), its jet flame, and whether the rabbit flag rides
/// on its back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Livery { pub color: Vec3, pub plates: f32, pub shine: f32, pub jet: [f32; 4], pub rabbit: bool,
    /// The lighter trim on gear built from parts and first-person hands.
    pub accent: Vec3 }

/// The hunters' armor where a mode has no teams: the art's own steel, with
/// steel where the team paint goes.
pub const HUNTER_STEEL: Vec3 = Vec3::new(0.46, 0.48, 0.51);
/// How brightly the rabbit's gold lights itself, so it shows in any shade.
pub const RABBIT_SHINE: f32 = 0.45;
/// Trim and shots that name no side (`Livery::accent`, `drawlist::side_accent`).
pub const NEUTRAL_ACCENT: Vec3 = Vec3::new(0.80, 0.82, 0.85);

/// A team's trim, lighter than its paint.
pub fn team_accent(team: Team) -> Vec3 {
    if team == Team::Ember { Vec3::new(0.86, 0.28, 0.18) } else { Vec3::new(0.22, 0.70, 0.86) }
}

impl Livery {
    /// A team's: its color on every plate, its flame.
    pub fn team(team: Team) -> Self {
        let jet = if team == Team::Ember { [1.0, 0.42, 0.12, 0.55] } else { [0.3, 0.85, 1.0, 0.55] };
        Livery { color: team_color(team), plates: TEAM_PLATES, shine: 0.0, jet, rabbit: false, accent: team_accent(team) }
    }
}

/// Player `i`'s livery. Teams wear their color; where the mode has no
/// teams (Rabbit, free-for-all) a body's hidden team means nothing, so
/// everyone wears the hunters' steel (the art as drawn), and the rabbit
/// the flag's gold on every plate, lit so they can't hide (since 2026-10-14).
pub fn livery(world: &peakrunner_core::sim::World, i: usize) -> Livery {
    let Some(p) = world.players.get(i) else { return Livery::team(Team::Ember) };
    if !world.ffa() { return Livery::team(p.team); }
    if world.is_rabbit(i) {
        return Livery { color: RABBIT_FLAG, plates: 1.0, shine: RABBIT_SHINE, jet: [1.0, 0.8, 0.3, 0.6], rabbit: true, accent: RABBIT_FLAG };
    }
    Livery { color: HUNTER_STEEL, plates: 0.0, shine: 0.0, jet: [1.0, 0.84, 0.62, 0.5], rabbit: false, accent: NEUTRAL_ACCENT }
}

fn flag_color(flag: Team) -> Vec3 {
    if flag == Team::Ember { Vec3::new(0.89, 0.29, 0.20) } else { Vec3::new(0.24, 0.78, 0.88) }
}

/// Parts for one player at a level of detail: far players keep the animated
/// silhouette, near ones get the full armor, and close ones the face.
/// `hold`: a weapon, Football's ball (with the flag for the carrier), a blade, or free hands.
/// `models`: where authored models go, and the bone palettes of the skinned
/// ones (`DrawFrame::bones`); `None` draws the built-in parts only.
/// `ragdoll`: the body's ragdoll while it is down (`AnimTracker::ragdoll`).
/// `livery`: what the body wears (`livery`; the rabbit flag on its back).
#[allow(clippy::too_many_arguments)]
pub fn push_player(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, models: Option<(&mut Vec<ModelDraw>, &mut Vec<Mat4>)>, p: &Player, time: f32, lod: Lod, motion: Motion, hold: Hold,
    ragdoll: Option<&[Vec3; crate::ragdoll::POINTS]>, livery: Livery) {
    let (mut models, bones) = match models { Some((m, b)) => (Some(m), Some(b)), None => (None, None) };
    let base = PoseInput::from_player(p, time);
    let stride = if motion.stride > 0.0 { motion.stride } else { base.stride };
    let armed = hold == Hold::Weapon;
    // The weapon in the hands, and the built-in model it's drawn with
    // (`View::kind`) when it has no authored one.
    let held = motion.shown_weapon(p.weapon);
    let shown = crate::sim::arsenal::view_kind(held);
    // An armor with grip gloves holding an authored weapon (not the repair
    // tool): the hands on its own grips.
    let grip = if models.is_some() && !p.repair_out && hold == Hold::Weapon {
        armor_model(p.armor.def().build).and_then(|(_, kit)| weapon_model(held).and_then(|(_, w)| grip_hold(kit, w, held_frame(w))))
    } else { None };
    // Football's throw: the follow-through over the pass's cooldown.
    let throw = if hold == Hold::Free && p.alive { (p.cooldown / peakrunner_core::sim::football::PASS_COOLDOWN).clamp(0.0, 1.0) } else { 0.0 };
    // A tracked player's gait weights are eased (`Motion`); an untracked
    // one's come straight from the state.
    let (ground, ski, back, hop, bound) = if motion.stride > 0.0 { (motion.ground, motion.ski, motion.back, motion.hop, motion.bound) }
        else { (base.ground, base.ski, base.back, base.hop, base.bound) };
    // Who the body is: its face and its gait's seed.
    let who = if motion.who != 0 { motion.who } else { p.net_id };
    // A tracked body counts its own time down (the respawn timer only says
    // it for the default delay; Football's is a second).
    let dead_for = if base.dead_for.is_some() && !p.alive { motion.dead_for.or(base.dead_for) } else { base.dead_for };
    let input = PoseInput { squash: motion.squash(), lower: motion.lower(), stride, hold, throw, dead_for,
        fall_dir: motion.fall_dir, fall_tilt: motion.fall_tilt, grip, ground, ski, back, hop, bound, seed: seed_of(who), ..base };
    if matches!(input.dead_for, Some(t) if t > CORPSE_TIME) { return; }
    let carrier = hold == Hold::Carrier;
    let build = p.armor.def().build;
    // Authored armor for the body's size, when there is one.
    let kit = models.as_ref().and_then(|_| armor_model(build));
    let input = PoseInput { size: body_size(p, kit.is_some(), armed), ..input };
    // Heavy armor is bigger all round: broader, deeper, a little taller.
    // Authored armor carries its own size, so the frame grows less.
    let frame = body_frame(p, kit.is_some(), armed);
    // Down with a ragdoll: as it lies; else the posed body.
    let pose = match ragdoll.filter(|_| input.dead_for.is_some()) {
        Some(points) => crate::ragdoll::pose_of(points, frame),
        None => pose(&input),
    };
    let root = frame * pose.body;
    let detailed = lod != Lod::Far;
    let team = livery.color;
    let plate = team;
    let plate_dark = team * 0.72;
    let alloy = Vec3::new(0.50, 0.56, 0.62);
    let steel = Vec3::new(0.36, 0.40, 0.45);
    let suit = Vec3::new(0.085, 0.09, 0.10);
    let trim = Vec3::new(0.17, 0.20, 0.24);
    let leather = Vec3::new(0.12, 0.10, 0.09);
    let webbing = Vec3::new(0.21, 0.22, 0.17);
    let light = Vec3::new(0.32, 0.88, 1.0);
    let look = person_look(who);
    // Dead: lights out. Down (dead, or tackled): the hands have let go.
    let dead = !p.alive;
    let downed = input.dead_for.is_some();
    let visor = if dead { 0.0 } else { 0.7 };

    let mut part = |m: Mat4, mesh: MeshId, color: Vec3, glow: f32, mode: f32| {
        lit.push(LitDraw { mesh, model: root * m, color, emit: glow, mode });
    };
    let at = |pos: Vec3, size: Vec3| Mat4::from_translation(pos) * Mat4::from_scale(size);
    // A box along the limb from `a` to `b` (arms: its sides have no set facing).
    let seg = |a: Vec3, b: Vec3, w: f32, d: f32| {
        let axis = b - a;
        Mat4::from_translation((a + b) * 0.5)
            * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, axis.normalize_or(Vec3::Y)))
            * Mat4::from_scale(Vec3::new(w, axis.length() + 0.05, d))
    };
    // A stretch `t0..t1` of the leg from `a` to `b`, `w` wide and `d` deep,
    // shifted `dz` toward the back (negative: to the front of the leg).
    let leg = |a: Vec3, b: Vec3, t0: f32, t1: f32, w: f32, d: f32, dz: f32| {
        let (p0, p1) = (a.lerp(b, t0), a.lerp(b, t1));
        let y = (p1 - p0).normalize_or(Vec3::NEG_Y);
        let z = (Vec3::Z - y * y.z).normalize_or(Vec3::X);
        let x = y.cross(z);
        Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), ((p0 + p1) * 0.5 + z * dz).extend(1.0))
            * Mat4::from_scale(Vec3::new(w, (p1 - p0).length() + 0.04, d))
    };

    // A person in plate over a dark undersuit: a team-coloured cuirass,
    // pauldrons, thigh plates and vambraces; steel greaves and knee cops; an
    // open-faced helmet with the HUD visor raised, so the face shows; and the
    // twin-tube jetpack. The plates, helmet and pack are at every LOD: they
    // make a player readable at range.
    let c = pose.chest;
    let side_of = |s: usize| if s == 0 { -1.0f32 } else { 1.0 };
    let h = pose.head;
    let head = h * Mat4::from_translation(Vec3::new(0.0, 0.155, -0.005)) * Mat4::from_scale(Vec3::splat(HEAD_SIZE));
    let shell = h * Mat4::from_translation(Vec3::new(0.0, 0.20, 0.012)) * rot_x(0.30);
    let pack = c * Mat4::from_translation(Vec3::new(0.0, 0.30, 0.25));
    if let (Some((id, kit)), Some(out)) = (kit, models.as_deref_mut()) {
        // The undersuit, face, neck and gloves; the armor is the model's.
        // The suit is slimmer than under the built-in plates, so the
        // armor's pieces stand clear of it. An armor that carries its own
        // undersuit (point `suit`), a closed helmet (point `visor`) or
        // gloves (parts `glove_l/_r`) leaves out the built-in ones.
        let (own_suit, closed) = (kit.point("suit").is_some(), kit.point("visor").is_some());
        let own_gloves = kit.parts.iter().any(|q| q.name == "glove_r");
        if !own_suit {
            part(c * at(Vec3::new(0.0, 0.26, 0.02), Vec3::new(0.32, 0.6, 0.21)), MeshId::Capsule, suit, 0.0, 0.0);
            part(pose.pelvis_frame * at(Vec3::new(0.0, -0.06, 0.0), Vec3::new(0.30, 0.22, 0.21)), MeshId::Capsule, suit, 0.0, 0.0);
        }
        if !closed { part(head, MeshId::Head, look.skin, 0.0, 3.0); }
        part(h * at(Vec3::new(0.0, 0.03, 0.01), Vec3::new(0.12, 0.16, 0.12)), MeshId::Capsule, suit, 0.0, 0.0);
        for s in 0..2 {
            if !own_suit {
                part(seg(pose.shoulders[s], pose.elbows[s], 0.10, 0.10), MeshId::Capsule, suit, 0.0, 0.0);
                part(seg(pose.elbows[s], pose.hands[s], 0.085, 0.085), MeshId::Capsule, suit, 0.0, 0.0);
                let (hip, knee, ankle) = (pose.hips[s], pose.knees[s], pose.ankles[s]);
                part(leg(hip, knee, -0.02, 1.02, 0.15, 0.16, 0.0), MeshId::Capsule, suit, 0.0, 0.0);
                part(leg(knee, ankle, 0.0, 1.0, 0.11, 0.12, 0.0), MeshId::Capsule, suit, 0.0, 0.0);
            }
            if !own_gloves {
                part(pose.wrists[s] * at(Vec3::new(0.0, -0.06, 0.0), Vec3::new(0.095, 0.12, 0.11)), MeshId::Bevel, leather, 0.0, 0.0);
            }
        }
        let lights = if dead { 0.0 } else { 1.0 };
        let jets = if dead { 0.0 } else if p.jetting { 1.8 } else { 0.35 };
        push_armor(out, bones.expect("bones with models"), id, kit, &pose, root, livery, lights, jets);
    } else {
    // Torso: undersuit, cuirass, the hips.
    part(c * at(Vec3::new(0.0, 0.26, 0.02), Vec3::new(0.40, 0.66, 0.27)), MeshId::Capsule, suit, 0.0, 0.0);
    part(c * at(Vec3::new(0.0, 0.39, -0.015), Vec3::new(0.54, 0.36, 0.33)), MeshId::Armor, plate, 0.0, 0.0);
    part(pose.pelvis_frame * at(Vec3::new(0.0, -0.06, 0.0), Vec3::new(0.34, 0.24, 0.25)), MeshId::Capsule, suit, 0.0, 0.0);
    // Pauldrons: a dome over each shoulder with a lame below it.
    for s in 0..2 {
        let side = side_of(s);
        let shoulder = c * Mat4::from_translation(Vec3::new(side * 0.34, 0.50, 0.0)) * Mat4::from_rotation_z(-side * 0.35);
        part(shoulder * Mat4::from_scale(Vec3::new(0.14, 0.115, 0.16)), MeshId::Dome, plate, 0.0, 0.0);
        part(shoulder * at(Vec3::new(side * 0.03, -0.03, 0.0), Vec3::new(0.22, 0.045, 0.27)), MeshId::Armor, plate_dark, 0.0, 0.0);
    }
    // Head: the face under an open helmet, the visor raised on the brow.
    part(head, MeshId::Head, look.skin, 0.0, 3.0);
    part(shell * Mat4::from_scale(Vec3::new(0.118, 0.115, 0.132)), MeshId::Dome, plate, 0.0, 0.0);
    // Arms and legs.
    for s in 0..2 {
        if detailed {
            part(seg(pose.shoulders[s], pose.elbows[s], 0.13, 0.13), MeshId::Capsule, suit, 0.0, 0.0);
            part(seg(pose.elbows[s], pose.hands[s], 0.11, 0.11), MeshId::Capsule, suit, 0.0, 0.0);
            let (e, hand) = (pose.elbows[s], pose.hands[s]);
            part(seg(e.lerp(hand, 0.18), e.lerp(hand, 0.86), 0.14, 0.145), MeshId::Armor, plate, 0.0, 0.0);
        } else {
            part(seg(pose.shoulders[s], pose.hands[s], 0.15, 0.16), MeshId::Capsule, suit, 0.0, 0.0);
        }
        let (hip, knee, ankle) = (pose.hips[s], pose.knees[s], pose.ankles[s]);
        part(leg(hip, knee, 0.05, 0.9, 0.21, if detailed { 0.12 } else { 0.21 }, if detailed { -0.045 } else { 0.0 }), MeshId::Armor, plate, 0.0, 0.0);
        part(leg(knee, ankle, 0.1, 0.92, 0.16, if detailed { 0.1 } else { 0.17 }, if detailed { -0.035 } else { 0.0 }), MeshId::Armor, alloy * 0.85, 0.0, 0.0);
        part(pose.feet[s] * at(Vec3::new(0.0, -0.075, -0.05), Vec3::new(0.17, 0.16, 0.29)), MeshId::Armor, leather, 0.0, 0.0);
    }
    // Jetpack: a housing over twin tubes.
    part(pack * Mat4::from_scale(Vec3::new(0.44, 0.50, 0.22)), MeshId::Armor, trim, 0.0, 0.0);
    }

    // Carried flag on the back at every LOD, so carriers read from range:
    // a pole from the pack and a waving cloth, larger than the armor.
    // The football's carrier wears their own team's flag the same way.
    let carried = if livery.rabbit { Some(RABBIT_FLAG) } else { p.carrying.map(flag_color).or((hold == Hold::Carrier).then(|| flag_color(p.team))) };
    if let (Some(cloth), false) = (carried, dead) {
        // The ball carrier's flag stands taller and larger, so the carrier
        // shows across the field.
        let big = if hold == Hold::Carrier { 1.45 } else { 1.0 };
        let pole_base = pack * Mat4::from_translation(Vec3::new(-0.16, 0.2, 0.08)) * Mat4::from_scale(Vec3::splat(big));
        part(pole_base * at(Vec3::new(0.0, 0.58, 0.0), Vec3::new(0.04, 1.2, 0.04)), MeshId::Cube, Vec3::splat(0.08), 0.0, 0.0);
        let drift = Vec3::new(p.vel.x, 0.0, p.vel.z).length().min(40.0) / 40.0;
        let strips = if detailed { 5 } else { 2 };
        for k in 0..strips {
            let u = k as f32 / strips as f32;
            let wave = (time * 7.0 - u * 3.0 + input.seed).sin() * (0.05 + 0.12 * u);
            part(pole_base * Mat4::from_translation(Vec3::new(0.0, 0.88 - u * 0.12 * drift, 0.10 + u * 0.66))
                * Mat4::from_rotation_y(wave) * Mat4::from_scale(Vec3::new(0.025, 0.52, 0.68 / strips as f32 + 0.02)),
                MeshId::Cube, cloth, 0.35, 0.0);
        }
        if detailed {
            part(pole_base * at(Vec3::new(0.0, 1.20, 0.0), Vec3::splat(0.10)), MeshId::Sphere, Vec3::new(0.92, 0.74, 0.32), 0.3, 0.0);
        }
    }
    // The blade (`Hold::Blade`): its glow, and the smear of a cut.
    let blade = |lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>| {
        let held = crate::blade_look::frame(crate::blade_look::pose(&p.blade, 0.0));
        let base = root * pose.aim * held.inverse();
        let trail: Vec<Mat4> = crate::blade_look::trail(&p.blade, 0.0, 6).into_iter().map(|q| base * crate::blade_look::frame(q)).collect();
        let heat = if p.blade.swing == peakrunner_core::sim::blade::POWER || p.blade.charging() { p.blade.charge() } else { 0.0 };
        crate::blade_look::push(lit, emit, root * pose.aim, crate::blade_look::color(who), heat, &trail, false);
    };
    if !detailed {
        if downed {}
        else if hold == Hold::Blade { blade(lit, emit); }
        else if carrier { lit.extend(crate::drawlist::ball_draws(root * tucked_ball(&pose))); }
        // (Empty hands hold nothing far off too: Football's players, a
        // blade's holder out of sight.)
        else if p.repair_out && armed { push_repair_tool(lit, root * pose.aim, time, p.repair_beam.is_some()); }
        else if armed && !push_weapon_model(models.as_deref_mut(), root * pose.aim, held, team, &input) {
            push_held_weapon(lit, root * pose.aim, shown, team, &input, false);
        }
        return;
    }
    if kit.is_none() {

    // Near LOD: the undersuit under the plates, the belt kit, the helmet's
    // guards and visor, gloves, knees, boots and the jetpack's tubes.
    part(c * at(Vec3::new(0.0, 0.40, -0.182), Vec3::new(0.10, 0.25, 0.03)), MeshId::Bevel, trim, 0.0, 0.0);
    part(c * at(Vec3::new(0.0, 0.44, -0.198), Vec3::new(0.05, 0.035, 0.02)), MeshId::Cube, light, visor * 0.7, 0.0);
    for k in 0..3 {
        let k = k as f32;
        part(c * at(Vec3::new(0.0, 0.175 - 0.07 * k, -0.105 + 0.006 * k), Vec3::new(0.34 - 0.025 * k, 0.062, 0.2)),
            MeshId::Bevel, if k == 1.0 { trim } else { plate_dark }, 0.0, 0.0);
    }
    part(c * at(Vec3::new(0.0, 0.57, 0.01), Vec3::new(0.22, 0.055, 0.2)), MeshId::Armor, plate_dark, 0.0, 0.0);
    part(c * at(Vec3::new(0.0, -0.02, 0.0), Vec3::new(0.38, 0.075, 0.28)), MeshId::Bevel, leather, 0.0, 0.0);
    part(c * at(Vec3::new(0.0, -0.02, -0.145), Vec3::new(0.07, 0.05, 0.02)), MeshId::Cube, alloy, 0.0, 0.0);
    part(c * at(Vec3::new(0.0, -0.14, -0.12), Vec3::new(0.21, 0.15, 0.05)), MeshId::Armor, plate_dark, 0.0, 0.0);
    for s in 0..2 {
        let side = side_of(s);
        part(c * at(Vec3::new(side * 0.155, -0.03, -0.11), Vec3::new(0.08, 0.09, 0.06)), MeshId::Bevel, webbing, 0.0, 0.0);
        part(c * Mat4::from_translation(Vec3::new(side * 0.19, -0.12, 0.0)) * Mat4::from_rotation_z(side * 0.12)
            * Mat4::from_scale(Vec3::new(0.06, 0.17, 0.20)), MeshId::Armor, plate_dark, 0.0, 0.0);
        let shoulder = c * Mat4::from_translation(Vec3::new(side * 0.34, 0.50, 0.0)) * Mat4::from_rotation_z(-side * 0.35);
        part(shoulder * at(Vec3::new(side * 0.05, -0.075, 0.0), Vec3::new(0.19, 0.04, 0.24)), MeshId::Armor, trim, 0.0, 0.0);
        // Arms: elbow cop and glove.
        part(at(pose.elbows[s], Vec3::splat(0.068)), MeshId::Sphere, steel, 0.0, 0.0);
        part(pose.wrists[s] * at(Vec3::new(0.0, -0.06, 0.0), Vec3::new(0.095, 0.12, 0.11)), MeshId::Bevel, leather, 0.0, 0.0);
        part(pose.wrists[s] * at(Vec3::new(0.0, -0.045, -0.05), Vec3::new(0.10, 0.06, 0.03)), MeshId::Cube, steel, 0.0, 0.0);
        // Legs: the undersuit, knee cop, toe cap and sole.
        let (hip, knee, ankle) = (pose.hips[s], pose.knees[s], pose.ankles[s]);
        part(leg(hip, knee, -0.02, 1.02, 0.19, 0.2, 0.0), MeshId::Capsule, suit, 0.0, 0.0);
        part(leg(knee, ankle, 0.0, 1.0, 0.14, 0.15, 0.0), MeshId::Capsule, suit, 0.0, 0.0);
        part(leg(knee, ankle, -0.1, 0.1, 0.15, 0.09, -0.075), MeshId::Armor, steel, 0.0, 0.0);
        part(pose.feet[s] * at(Vec3::new(0.0, -0.1, -0.17), Vec3::new(0.175, 0.09, 0.08)), MeshId::Bevel, steel, 0.0, 0.0);
        part(pose.feet[s] * at(Vec3::new(0.0, -0.158, -0.055), Vec3::new(0.18, 0.028, 0.31)), MeshId::Cube, trim, 0.0, 0.0);
        // Helmet: cheek guards over the ears.
        part(h * Mat4::from_translation(Vec3::new(side * 0.103, 0.155, 0.025)) * rot_x(-0.25)
            * Mat4::from_scale(Vec3::new(0.028, 0.085, 0.10)), MeshId::Armor, plate_dark, 0.0, 0.0);
        // Jetpack: tube, nozzle with a glowing throat while jetting, fin.
        part(pack * at(Vec3::new(side * 0.15, -0.30, 0.03), Vec3::new(0.15, 0.44, 0.15)), MeshId::Capsule, alloy, 0.0, 0.0);
        let nozzle = pack * Mat4::from_translation(Vec3::new(side * 0.15, -0.52, 0.03));
        part(nozzle * Mat4::from_scale(Vec3::new(0.12, 0.10, 0.12)), MeshId::Disc, trim, 0.0, 0.0);
        part(nozzle * Mat4::from_translation(Vec3::new(0.0, -0.055, 0.0)) * Mat4::from_scale(Vec3::new(0.09, 0.01, 0.09)),
            MeshId::Disc, light, if p.jetting { 1.8 } else { 0.1 }, 0.0);
        part(pack * at(Vec3::new(side * 0.235, 0.08, 0.03), Vec3::new(0.03, 0.26, 0.12)), MeshId::Bevel, plate_dark, 0.0, 0.0);
    }
    part(pack * at(Vec3::new(0.0, 0.02, 0.115), Vec3::new(0.14, 0.38, 0.03)), MeshId::Bevel, plate, 0.0, 0.0);
    // Helmet: neck and neck guard, the brow band with the raised HUD visor
    // glowing on it, and a low crest.
    part(h * at(Vec3::new(0.0, 0.03, 0.01), Vec3::new(0.13, 0.16, 0.13)), MeshId::Capsule, suit, 0.0, 0.0);
    part(h * Mat4::from_translation(Vec3::new(0.0, 0.13, 0.10)) * rot_x(0.35) * Mat4::from_scale(Vec3::new(0.20, 0.07, 0.06)),
        MeshId::Armor, trim, 0.0, 0.0);
    part(h * Mat4::from_translation(Vec3::new(0.0, 0.232, -0.108)) * rot_x(0.3) * Mat4::from_scale(Vec3::new(0.19, 0.028, 0.024)),
        MeshId::Bevel, plate_dark, 0.0, 0.0);
    part(h * Mat4::from_translation(Vec3::new(0.0, 0.258, -0.102)) * rot_x(0.95) * Mat4::from_scale(Vec3::new(0.16, 0.04, 0.007)),
        MeshId::Cube, light, visor, 0.0);
    part(shell * at(Vec3::new(0.0, 0.106, 0.0), Vec3::new(0.022, 0.028, 0.15)), MeshId::Cube, plate_dark, 0.0, 0.0);
    }

    // Close up: eyes, brows and lips on the face (none behind a closed helmet).
    let face_shows = kit.is_none_or(|(_, k)| k.point("visor").is_none());
    if lod == Lod::Close && face_shows {
        use crate::character_mesh::face_point;
        for s in 0..2 {
            let side = side_of(s);
            let socket = face_point(side * 0.14, 0.028);
            part(head * at(socket + Vec3::new(0.0, 0.0, -0.012), Vec3::new(0.06, 0.04, 0.03)), MeshId::Sphere,
                Vec3::new(0.93, 0.91, 0.87), 0.0, 0.0);
            part(head * at(socket + Vec3::new(side * 0.004, 0.0, -0.041), Vec3::new(0.024, 0.024, 0.006)), MeshId::Sphere,
                if dead { look.skin * 0.6 } else { look.iris }, 0.0, 0.0);
            let brow = face_point(side * 0.14, 0.1);
            part(head * Mat4::from_translation(brow + Vec3::new(0.0, 0.0, -0.014)) * Mat4::from_rotation_z(-side * 0.1)
                * Mat4::from_scale(Vec3::new(0.15, 0.034, 0.035)), MeshId::Cube, look.hair, 0.0, 0.0);
        }
        let lips = face_point(0.0, -0.265);
        part(head * at(lips + Vec3::new(0.0, 0.0, 0.008), Vec3::new(0.085, 0.02, 0.02)), MeshId::Sphere,
            look.skin * Vec3::new(0.78, 0.56, 0.52), 0.0, 3.0);
    }

    if downed {}
    else if hold == Hold::Blade { blade(lit, emit); }
    else if carrier { lit.extend(crate::drawlist::ball_draws(root * tucked_ball(&pose))); }
    else if p.repair_out && armed { push_repair_tool(lit, root * pose.aim, time, p.repair_beam.is_some()); }
    else if armed && !push_weapon_model(models.as_deref_mut(), root * pose.aim, held, team, &input) {
        push_held_weapon(lit, root * pose.aim, shown, team, &input, true);
    }

    // Jet flame from each nozzle: the armor's (points `nozzle_*`, in the
    // chest frame, where the flame leaves), else the built-in pack's two.
    if p.jetting && !dead {
        let col = livery.jet;
        let nozzles: Vec<Mat4> = match kit {
            Some((_, k)) if k.points.iter().any(|(n, _)| n.starts_with("nozzle")) => k.points.iter()
                .filter(|(n, _)| n.starts_with("nozzle")).map(|(_, m)| c * *m * Mat4::from_translation(Vec3::new(0.0, -0.23, 0.0)))
                .collect(),
            _ => [-1.0f32, 1.0].iter().map(|&side| pack * Mat4::from_translation(Vec3::new(side * 0.15, -0.80, 0.02))).collect(),
        };
        for (k, nozzle) in nozzles.into_iter().enumerate() {
            let side = if k % 2 == 0 { -1.0f32 } else { 1.0 } * (1 + k / 2) as f32;
            let flick = 1.0 + 0.18 * (time * 43.0 + side * 3.0 + input.seed).sin();
            let base = root * nozzle;
            emit.push(EmitDraw { mesh: MeshId::Sphere,
                model: base * Mat4::from_scale(Vec3::new(0.12, 0.42 * flick, 0.12)), color: col });
            emit.push(EmitDraw { mesh: MeshId::Sphere,
                model: base * Mat4::from_translation(Vec3::new(0.0, 0.2, 0.0)) * Mat4::from_scale(Vec3::new(0.07, 0.2, 0.07)),
                color: [1.0, 0.95, 0.85, 0.7] });
        }
    }

}

/// The authored armor for a body size (`assets/models/armor/<size>/`).
pub(crate) fn armor_model(build: peakrunner_core::registry::Build) -> Option<(u16, &'static crate::model::Model)> {
    use peakrunner_core::registry::Build;
    crate::model::get(match build { Build::Light => "armor/small", Build::Medium => "armor/medium", Build::Heavy => "armor/large" })
}

/// Rest lengths of the limb pieces' bones (m): what an authored piece is
/// made to; a posed bone of another length stretches it.
const THIGH: f32 = 0.44;
const SHIN: f32 = 0.44;

/// A limb bone's frame: origin at the joint `a`, +Y back up the limb from
/// `b`, X as near `side` as it can be (the body's right), Z behind.
pub(crate) fn limb_frame(a: Vec3, b: Vec3, side: Vec3, rest: f32) -> Mat4 {
    let along = a - b;
    let y = along.normalize_or(Vec3::Y);
    let x = (side - y * side.dot(y)).normalize_or(Vec3::X);
    let z = x.cross(y);
    let stretch = (along.length() / rest).clamp(0.8, 1.25);
    Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), a.extend(1.0)) * Mat4::from_scale(Vec3::new(1.0, stretch, 1.0))
}

/// A posed bone's frame by its skin joint's name (`bone_<bone>`, the bones
/// the armor's pieces ride: `pelvis`, `chest`, `head`, and `upper_arm`,
/// `forearm`, `thigh`, `shin`, `foot` and `hand` with `_l` or `_r`).
fn bone_frame(pose: &Pose, name: &str) -> Option<Mat4> {
    let side = pose.chest.transform_vector3(Vec3::X).normalize_or(Vec3::X);
    let hip_side = pose.pelvis_frame.transform_vector3(Vec3::X).normalize_or(Vec3::X);
    let name = name.strip_prefix("bone_")?;
    Some(match name {
        "pelvis" => pose.pelvis_frame,
        "chest" => pose.chest,
        "head" => pose.head,
        _ => {
            let s = if name.ends_with("_l") { 0 } else if name.ends_with("_r") { 1 } else { return None };
            match &name[..name.len() - 2] {
                "upper_arm" => limb_frame(pose.shoulders[s], pose.elbows[s], side, UPPER_ARM),
                "forearm" => limb_frame(pose.elbows[s], pose.hands[s], side, FOREARM),
                "thigh" => limb_frame(pose.hips[s], pose.knees[s], hip_side, THIGH),
                "shin" => limb_frame(pose.knees[s], pose.ankles[s], hip_side, SHIN),
                "foot" => pose.feet[s],
                "hand" => pose.grips.map_or(pose.wrists[s], |g| g[s]),
                _ => return None,
            }
        }
    })
}

/// The armor's pieces on their bones. Each piece is a part named for its
/// bone, made in that bone's frame (`docs/model-pipeline.md`); a skinned
/// part (the undersuit) bends with the bones its skin names, its palette in
/// `bones`.
fn push_armor(out: &mut Vec<ModelDraw>, bones: &mut Vec<Mat4>, id: u16, kit: &crate::model::Model, pose: &Pose, root: Mat4, livery: Livery, lights: f32, jets: f32) {
    let team = livery.color;
    let side = pose.chest.transform_vector3(Vec3::X).normalize_or(Vec3::X);
    let hip_side = pose.pelvis_frame.transform_vector3(Vec3::X).normalize_or(Vec3::X);
    let mut palette = None;
    for (k, part) in kit.parts.iter().enumerate() {
        if part.skinned {
            let Some(skin) = &kit.skin else { continue };
            let at = *palette.get_or_insert_with(|| {
                let at = (bones.len() / crate::model::MAX_BONES) as u32;
                bones.extend(skin.joints.iter().zip(&skin.inverse_bind)
                    .map(|(name, inverse)| bone_frame(pose, name).unwrap_or(Mat4::IDENTITY) * *inverse));
                bones.resize((at as usize + 1) * crate::model::MAX_BONES, Mat4::IDENTITY);
                at
            });
            out.push(ModelDraw { id, part: k as u16, model: root, team, glow: lights, paint: false, plates: livery.plates, shine: livery.shine, skin: at });
            continue;
        }
        let (bone, glow) = match part.name.as_str() {
            "helmet" => (pose.head, lights),
            "torso" => (pose.chest, lights),
            "pack" => (pose.chest, jets),
            "pelvis" => (pose.pelvis_frame, lights),
            name => {
                let s = if name.ends_with("_l") { 0 } else if name.ends_with("_r") { 1 } else { continue };
                match &name[..name.len() - 2] {
                    "upper_arm" => (limb_frame(pose.shoulders[s], pose.elbows[s], side, UPPER_ARM), lights),
                    "forearm" => (limb_frame(pose.elbows[s], pose.hands[s], side, FOREARM), lights),
                    "thigh" => (limb_frame(pose.hips[s], pose.knees[s], hip_side, THIGH), lights),
                    "shin" => (limb_frame(pose.knees[s], pose.ankles[s], hip_side, SHIN), lights),
                    "boot" => (pose.feet[s], lights),
                    // On a weapon's grips the grip gloves stand in for the hands.
                    "glove" if pose.grips.is_none() => (pose.wrists[s], lights),
                    "grip" => match pose.grips { Some(g) => (g[s], lights), None => continue },
                    _ => continue,
                }
            }
        };
        out.push(ModelDraw { id, part: k as u16, model: root * bone, team, glow, paint: false, plates: livery.plates, shine: livery.shine, skin: crate::drawlist::RIGID });
    }
}

/// The held weapon as an authored model (`assets/models/weapons/<id>/`),
/// its right-hand grip in the hand; false when the weapon has none.
fn push_weapon_model(models: Option<&mut Vec<ModelDraw>>, aim: Mat4, weapon: u8, team: Vec3, input: &PoseInput) -> bool {
    let Some(out) = models else { return false };
    let Some((id, model)) = weapon_model(weapon) else { return false };
    let frame = aim * held_frame(model);
    let glow = if input.dead_for.is_some() { 0.0 } else { 1.0 };
    crate::model::push(out, id, model, frame, team, glow, &|_| None);
    true
}

/// Where an authored weapon sits in the aim frame (`pose`): its right-hand
/// grip in the hand, a little oversized so it reads against the armor at
/// range.
fn held_frame(model: &crate::model::Model) -> Mat4 {
    let grip = model.point("grip_r").unwrap_or(Mat4::IDENTITY);
    Mat4::from_translation(Vec3::new(0.0, -0.09, 0.06)) * Mat4::from_scale(Vec3::splat(1.15)) * grip.inverse()
}

/// How far left of a weapon's `grip_l` (its centre line) the support hand's
/// palm sits, in the weapon model's units.
pub(crate) const SUPPORT_OUT: f32 = 0.056;

/// The hands on an authored weapon placed at `frame`: the armor's grip
/// gloves (`grip_l`, `grip_r`) at the weapon's grips and their wrists
/// (points `wrist_grip_l/_r`), in `frame`'s parent; None when either is
/// missing.
pub(crate) fn grip_hold(kit: &crate::model::Model, weapon: &crate::model::Model, frame: Mat4) -> Option<GripHold> {
    if !["grip_l", "grip_r"].iter().all(|n| kit.parts.iter().any(|q| q.name == *n)) { return None; }
    let (wl, wr) = (kit.point("wrist_grip_l")?, kit.point("wrist_grip_r")?);
    let right = frame * weapon.point("grip_r").unwrap_or(Mat4::IDENTITY);
    let left = frame * Mat4::from_translation(weapon.point("grip_l")?.transform_point3(Vec3::ZERO) + Vec3::new(-SUPPORT_OUT, 0.0, 0.0));
    let wrist = |glove: Mat4, w: Mat4| (glove * w).transform_point3(Vec3::ZERO);
    Some(GripHold { gloves: [left, right], wrists: [wrist(left, wl), wrist(right, wr)] })
}

/// The authored model of a weapon (by its definition's id), if it has one.
pub fn weapon_model(weapon: u8) -> Option<(u16, &'static crate::model::Model)> {
    let id = peakrunner_core::registry::registry().weapons.id(weapon as usize)?;
    crate::model::get(&format!("weapons/{id}"))
}

/// The ball tucked in a carrier's right hand, in the pose's frame: the
/// case's base (`drawlist::ball_draws`) under the hand, turned with the
/// chest and pressed a little in toward the ribs.
fn tucked_ball(pose: &Pose) -> Mat4 {
    let (_, turn, _) = pose.chest.to_scale_rotation_translation();
    let centre = pose.hands[1] + turn * Vec3::new(-0.05, 0.0, 0.04);
    Mat4::from_translation(centre) * Mat4::from_quat(turn) * Mat4::from_translation(Vec3::new(0.0, -0.15, 0.0))
}


/// The repair tool: a compact emitter with twin prongs, a green coil and a
/// glowing canister, held in the aim frame (-Z forward). Shared by the
/// first-person view (`drawlist`) and other players' models. Resting on an
/// empty tank (not `running`), its glow dims and the coil stops.
pub fn repair_tool_parts(frame: Mat4, time: f32, running: bool) -> Vec<LitDraw> {
    let dark = Vec3::new(0.12, 0.13, 0.15);
    let steel = Vec3::new(0.55, 0.6, 0.64);
    let green = Vec3::new(0.35, 1.0, 0.45);
    let pulse = if running { 0.9 + 0.5 * (time * 11.0).sin().abs() } else { 0.2 };
    let time = if running { time } else { 0.0 };
    let at = |pos: Vec3, size: Vec3| Mat4::from_translation(pos) * Mat4::from_scale(size);
    let mut out = vec![
        LitDraw { mesh: MeshId::Bevel, model: frame * at(Vec3::new(0.0, 0.0, -0.08), Vec3::new(0.11, 0.13, 0.32)), color: dark, emit: 0.0, mode: 0.0 },
        LitDraw { mesh: MeshId::Bevel, model: frame * Mat4::from_translation(Vec3::new(0.0, -0.11, 0.02)) * Mat4::from_rotation_x(0.35)
            * Mat4::from_scale(Vec3::new(0.07, 0.16, 0.08)), color: dark, emit: 0.0, mode: 0.0 },
        LitDraw { mesh: MeshId::Sphere, model: frame * at(Vec3::new(0.0, 0.09, 0.0), Vec3::new(0.06, 0.05, 0.1)), color: green, emit: 0.9 * pulse, mode: 0.0 },
        LitDraw { mesh: MeshId::Disc, model: frame * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.26)) * Mat4::from_rotation_x(FRAC_PI_2)
            * Mat4::from_rotation_y(time * 9.0) * Mat4::from_scale(Vec3::new(0.075, 0.02, 0.075)), color: green, emit: 1.2 * pulse, mode: 0.0 },
    ];
    for s in [-1.0f32, 1.0] {
        out.push(LitDraw { mesh: MeshId::Cube, model: frame * at(Vec3::new(s * 0.04, 0.0, -0.33), Vec3::new(0.022, 0.022, 0.16)),
            color: steel, emit: 0.0, mode: 0.0 });
    }
    out
}

fn push_repair_tool(lit: &mut Vec<LitDraw>, grip: Mat4, time: f32, running: bool) {
    lit.extend(repair_tool_parts(grip * Mat4::from_scale(Vec3::splat(1.3)), time, running));
}

/// Third-person weapon in the aim frame, echoing each first-person model.
fn push_held_weapon(lit: &mut Vec<LitDraw>, grip: Mat4, weapon: u8, team: Vec3, input: &PoseInput, detailed: bool) {
    // Slightly oversized so the weapon reads against the armor at range.
    let grip = grip * Mat4::from_scale(Vec3::splat(1.3));
    let mut put = |mesh, model: Mat4, color: Vec3, glow: f32| lit.push(LitDraw { mesh, model: grip * model, color, emit: glow, mode: 0.0 });
    let box_at = |at: Vec3, size: Vec3| Mat4::from_translation(at) * Mat4::from_scale(size);
    let along_z = |at: Vec3, r: f32, len: f32| Mat4::from_translation(at) * Mat4::from_rotation_x(FRAC_PI_2)
        * Mat4::from_scale(Vec3::new(r, len, r));
    let gun = Vec3::new(0.24, 0.27, 0.30);
    let dark = Vec3::new(0.06, 0.07, 0.09);
    let steel = Vec3::new(0.44, 0.48, 0.52);
    let flash = input.shot_age < 0.06;
    if !detailed {
        // Far LOD: body and barrel in the weapon's key colour.
        let body = match weapon { 0 => Vec3::new(0.64, 0.68, 0.72), 1 | crate::sim::arsenal::PLASMA => gun,
            crate::sim::arsenal::BLASTER => Vec3::new(0.78, 0.80, 0.84), _ => Vec3::new(0.30, 0.34, 0.19) };
        put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.0, 0.04), Vec3::new(0.17, 0.15, 0.40)), body, 0.0);
        put(MeshId::Cube, box_at(Vec3::new(0.0, 0.02, -0.30), Vec3::new(0.09, 0.09, 0.34)), steel, 0.0);
        return;
    }
    match weapon {
        0 => {
            let silver = Vec3::new(0.64, 0.68, 0.72);
            let cyan = Vec3::new(0.05, 0.74, 0.95);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.0, 0.02), Vec3::new(0.17, 0.15, 0.44)), silver, 0.0);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.10, 0.10), Vec3::new(0.07, 0.12, 0.10)), dark, 0.0);
            for side in [-1.0, 1.0] {
                put(MeshId::Bevel, box_at(Vec3::new(side * 0.068, 0.055, -0.14), Vec3::new(0.04, 0.06, 0.60)), Vec3::new(0.84, 0.87, 0.9), 0.0);
                put(MeshId::Cube, box_at(Vec3::new(side * 0.052, 0.06, -0.14), Vec3::new(0.006, 0.02, 0.44)), cyan, 0.8);
            }
            if detailed {
                put(MeshId::Cube, box_at(Vec3::new(0.088, -0.02, 0.02), Vec3::new(0.004, 0.035, 0.30)), team, 0.35);
                let charge = (1.0 - input.shot_age / 0.75).clamp(0.0, 1.0);
                if charge < 1.0 {
                    put(MeshId::Disc, Mat4::from_translation(Vec3::new(0.0, 0.07, -0.05)) * Mat4::from_scale(Vec3::new(0.07, 0.01, 0.07)), cyan, 0.6);
                }
            }
            if flash {
                put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.05, -0.48), Vec3::new(0.08, 0.04, 0.12)), Vec3::new(0.35, 0.85, 1.0), 1.4);
            }
        }
        1 => {
            // Six-barrel cluster that spins while firing.
            let spin = if input.shot_age < 0.4 { input.time * 30.0 } else { 0.0 };
            put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.0, 0.08), Vec3::new(0.16, 0.15, 0.30)), gun, 0.0);
            put(MeshId::Disc, along_z(Vec3::new(0.0, 0.01, 0.25), 0.07, 0.10), dark, 0.0);
            put(MeshId::Disc, Mat4::from_translation(Vec3::new(-0.11, -0.04, 0.06)) * Mat4::from_rotation_z(FRAC_PI_2)
                * Mat4::from_scale(Vec3::new(0.08, 0.07, 0.08)), Vec3::new(0.22, 0.25, 0.19), 0.0);
            put(MeshId::Disc, along_z(Vec3::new(0.0, 0.01, -0.10), 0.075, 0.04), steel, 0.0);
            put(MeshId::Disc, along_z(Vec3::new(0.0, 0.01, -0.44), 0.075, 0.03), steel, 0.0);
            let barrels = if detailed { 6 } else { 3 };
            for k in 0..barrels {
                let a = spin + k as f32 * TAU / barrels as f32;
                let o = Vec3::new(a.cos(), a.sin(), 0.0) * 0.045;
                put(MeshId::Disc, along_z(Vec3::new(0.0, 0.01, -0.27) + o, 0.017, 0.44), Vec3::new(0.32, 0.35, 0.38), 0.0);
            }
            put(MeshId::Cube, box_at(Vec3::new(0.082, 0.0, 0.08), Vec3::new(0.006, 0.04, 0.24)), team, 0.35);
            if flash {
                put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.01, -0.54), Vec3::new(0.09, 0.09, 0.05)), Vec3::new(1.0, 0.8, 0.35), 1.6);
            }
        }
        crate::sim::arsenal::PLASMA => {
            let copper = Vec3::new(0.66, 0.38, 0.18);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.0, 0.10), Vec3::new(0.17, 0.16, 0.30)), gun, 0.0);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.10, 0.16), Vec3::new(0.07, 0.12, 0.10)), dark, 0.0);
            put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.02, -0.08), Vec3::splat(0.06)), crate::drawlist::PLASMA_GLOW, 1.0);
            put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.30), 0.07, 0.32), gun, 0.0);
            for k in 0..3 { put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.20 - k as f32 * 0.09), 0.095, 0.02), copper, 0.0); }
            put(MeshId::Cube, box_at(Vec3::new(0.087, -0.01, 0.10), Vec3::new(0.004, 0.04, 0.22)), team, 0.35);
            if flash { put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.02, -0.52), Vec3::splat(0.1)), crate::drawlist::PLASMA_GLOW, 1.8); }
        }
        crate::sim::arsenal::BLASTER => {
            let pale = Vec3::new(0.78, 0.80, 0.84);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.0, 0.06), Vec3::new(0.12, 0.13, 0.36)), pale, 0.0);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.11, 0.14), Vec3::new(0.06, 0.14, 0.09)), dark, 0.0);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.01, -0.22), Vec3::new(0.08, 0.085, 0.24)), pale, 0.0);
            put(MeshId::Cube, box_at(Vec3::new(0.066, 0.0, 0.0), Vec3::new(0.02, 0.05, 0.22)), crate::drawlist::BLASTER_GLOW, 0.7);
            put(MeshId::Cube, box_at(Vec3::new(-0.063, -0.01, 0.06), Vec3::new(0.004, 0.035, 0.22)), team, 0.35);
            if flash { put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.01, -0.42), Vec3::splat(0.07)), crate::drawlist::BLASTER_GLOW, 1.8); }
        }
        _ => {
            let olive = Vec3::new(0.30, 0.34, 0.19);
            let amber = Vec3::new(1.0, 0.62, 0.18);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, 0.0, 0.10), Vec3::new(0.17, 0.16, 0.26)), gun, 0.0);
            put(MeshId::Bevel, box_at(Vec3::new(0.0, -0.03, 0.27), Vec3::new(0.08, 0.12, 0.14)), olive, 0.0);
            put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.28), 0.07, 0.34), gun, 0.0);
            put(MeshId::Disc, along_z(Vec3::new(0.0, 0.02, -0.45), 0.08, 0.03), steel, 0.0);
            put(MeshId::Disc, Mat4::from_translation(Vec3::new(0.0, -0.03, -0.06)) * Mat4::from_rotation_x(FRAC_PI_2)
                * Mat4::from_scale(Vec3::new(0.10, 0.15, 0.10)), olive, 0.0);
            let ready = input.shot_age > weapon_reload(2);
            put(MeshId::Cube, box_at(Vec3::new(0.0, 0.09, 0.02), Vec3::new(0.03, 0.02, 0.03)), amber, if ready { 0.8 } else { 0.05 });
            if detailed {
                put(MeshId::Cube, box_at(Vec3::new(0.087, -0.01, 0.10), Vec3::new(0.004, 0.04, 0.2)), team, 0.35);
            }
            if flash {
                put(MeshId::Sphere, box_at(Vec3::new(0.0, 0.02, -0.52), Vec3::new(0.09, 0.09, 0.14)), Vec3::new(1.0, 0.6, 0.2), 1.6);
            }
        }
    }
}

/// Part counts for tests and budgets.
#[cfg(test)]
pub fn part_count(p: &Player, time: f32, lod: Lod) -> (usize, usize) {
    let (mut lit, mut emit) = (Vec::new(), Vec::new());
    push_player(&mut lit, &mut emit, None, p, time, lod, Motion::default(), Hold::Weapon, None, Livery::team(p.team));
    (lit.len(), emit.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::World;

    /// Teams wear their colors; where the mode has no teams nobody wears a
    /// team's, and the rabbit wears bright gold.
    #[test]
    fn rabbit_and_free_for_all_wear_no_team_colors() {
        use peakrunner_core::map_catalog::SupportedMode;
        let mut w = World::new();
        w.set_mode(SupportedMode::Ctf);
        w.start_match(true);
        let (ember, glacier) = (w.players.iter().position(|p| p.team == Team::Ember).unwrap(), w.players.iter().position(|p| p.team == Team::Glacier).unwrap());
        assert_eq!(livery(&w, ember), Livery::team(Team::Ember));
        assert_eq!(livery(&w, glacier).color, team_color(Team::Glacier));
        for mode in [SupportedMode::Rabbit, SupportedMode::Deathmatch] {
            w.set_mode(mode);
            w.rabbit.active = mode == SupportedMode::Rabbit;
            w.rabbit.carrier = w.rabbit.active.then_some(ember);
            for i in [ember, glacier] {
                let l = livery(&w, i);
                if w.is_rabbit(i) {
                    assert_eq!((l.color, l.plates, l.rabbit), (RABBIT_FLAG, 1.0, true), "{mode:?}: the rabbit in gold all over");
                    assert!(l.shine > 0.0, "lit, so they can't hide");
                } else {
                    assert_eq!((l.color, l.plates, l.shine, l.rabbit), (HUNTER_STEEL, 0.0, 0.0, false), "{mode:?}: player {i} in the hunters' steel");
                }
            }
        }
        // Team Rabbit has teams: they keep their colors.
        w.set_mode(SupportedMode::TeamRabbit);
        assert_eq!(livery(&w, glacier).color, team_color(Team::Glacier));
    }

    /// A body stays down after its player respawns for as long as asked
    /// (Football's `BODY_LIES`), sinking at the end, and not at all without.
    #[test]
    fn a_body_stays_down_after_its_player_is_back_only_where_asked() {
        const DT: f32 = 1.0 / 60.0;
        for keep in [0.0, 4.0] {
            let mut t = AnimTracker::default();
            let mut p = runner();
            p.net_id = 9;
            let step = |t: &mut AnimTracker, p: &Player| t.update(std::slice::from_ref(p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, keep);
            for _ in 0..10 { step(&mut t, &p); }
            p.alive = false; p.respawn = 1.0;
            for _ in 0..60 { step(&mut t, &p); }
            assert!(t.motion(anim_id(0, &p)).dead_for.is_some_and(|d| (d - 1.0).abs() < 0.05), "the track counts its own time down");
            p.alive = true; p.pos += Vec3::new(0.0, 0.0, -50.0);
            step(&mut t, &p);
            assert_eq!(t.corpses().count(), if keep > 0.0 { 1 } else { 0 }, "keep {keep}");
            for _ in 0..(5.0 / DT) as usize { step(&mut t, &p); }
            assert_eq!(t.corpses().count(), 0, "gone once its time is up");
        }
    }

    fn runner() -> Player {
        let mut world = World::new();
        world.start_match(true);
        let mut p = world.players[0].clone();
        p.pos = Vec3::ZERO;
        p.alive = true;
        p
    }

    fn input(time: f32) -> PoseInput {
        PoseInput { vel: Vec3::new(0.0, 0.0, -8.0), yaw: 0.3, pitch: 0.1,
            jetting: false, weapon: 0, shot_age: 9.0, time, seed: 1.4, dead_for: None, squash: 0.0, lower: 0.0,
            stride: time * CADENCE, hold: Hold::Weapon, fall_dir: Vec2::new(0.0, -1.0), fall_tilt: FALL_TILT, grip: None, seated: false, tackled: false, throw: 0.0,
            ground: 1.0, ski: 0.0, back: 0.0, hop: 0.0, bound: 0.0, mass: 0.5, size: Vec3::ONE, blade: Default::default() }
    }

    #[test]
    fn two_bone_reach_keeps_both_bones_and_bends_toward_the_pole() {
        let shoulder = Vec3::new(0.3, 1.42, 0.0);
        for target in [Vec3::new(0.1, 1.2, -0.45), Vec3::new(0.0, 1.3, -0.45), Vec3::new(0.3, 1.0, -0.2)] {
            let pole = Vec3::new(0.8, -1.0, 0.15);
            let elbow = ik_elbow(shoulder, target, UPPER_ARM, FOREARM, pole);
            assert!((elbow.distance(shoulder) - UPPER_ARM).abs() < 1e-4, "upper arm {target:?}");
            assert!((elbow.distance(target) - FOREARM).abs() < 1e-4, "forearm {target:?}");
            let mid = (shoulder + target) * 0.5;
            assert!((elbow - mid).dot(pole) > 0.0, "the elbow bends toward the pole for {target:?}");
        }
        // Out of reach: all but straight at the target (a millimetre short of
        // full stretch, so the elbow stays defined).
        let far = shoulder + Vec3::new(0.0, 0.0, -2.0);
        let elbow = ik_elbow(shoulder, far, UPPER_ARM, FOREARM, Vec3::NEG_Y);
        assert!((elbow - (shoulder + Vec3::new(0.0, 0.0, -UPPER_ARM))).length() < 0.03);
    }

    #[test]
    fn a_two_handed_hold_blades_the_chest_but_not_the_head() {
        let held = pose(&input(0.4));
        let free = pose(&PoseInput { hold: Hold::Free, ..input(0.4) });
        // The left shoulder comes forward (-Z), the right goes back.
        assert!(held.shoulders[0].z < free.shoulders[0].z - 0.05);
        assert!(held.shoulders[1].z > free.shoulders[1].z + 0.05);
        assert!(held.head.x_axis.truncate().distance(free.head.x_axis.truncate()) < 1e-4, "the head still faces ahead");
    }

    #[test]
    fn pose_is_deterministic_and_finite() {
        let a = pose(&input(1.23));
        let b = pose(&input(1.23));
        assert_eq!(a.aim, b.aim);
        assert_eq!(a.knees, b.knees);
        for state in 0..5 {
            let mut i = input(0.7);
            match state { 1 => i.ski = 1.0, 2 => { i.jetting = true; i.ground = 0.0 }
                3 => i.ground = 0.0, 4 => i.dead_for = Some(0.4), _ => {} }
            let p = pose(&i);
            for v in p.knees.iter().chain(&p.ankles).chain(&p.elbows).chain(&p.hands) {
                assert!(v.is_finite(), "state {state}");
            }
            assert!(p.aim.is_finite() && p.body.is_finite());
        }
    }

    #[test]
    fn bones_keep_their_lengths() {
        for t in [0.0, 0.3, 0.9, 2.2] {
            let p = pose(&input(t));
            for s in 0..2 {
                assert!((p.hips[s].distance(p.knees[s]) - 0.44).abs() < 1e-4);
                assert!((p.knees[s].distance(p.ankles[s]) - 0.44).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn animation_phase_is_continuous() {
        // Small time steps and a speed change mid-stride move joints only a
        // little: amplitude follows speed, the phase never jumps.
        let mut first = input(0.0);
        first.vel = Vec3::new(0.0, 0.0, -2.0);
        let mut prev = pose(&first);
        for step in 1..600 {
            let t = step as f32 / 120.0;
            let mut i = input(t);
            i.vel = Vec3::new(0.0, 0.0, -2.0 - (step as f32 / 60.0));
            let next = pose(&i);
            for s in 0..2 {
                assert!(prev.ankles[s].distance(next.ankles[s]) < 0.12, "jump at {t}");
                assert!(prev.hands[s].distance(next.hands[s]) < 0.05, "hand jump at {t}");
            }
            prev = next;
        }
    }

    #[test]
    fn legs_run_on_the_ground_ski_together_and_dangle_in_the_air() {
        let mut i = input(0.1);
        let run = pose(&i);
        assert!((run.ankles[0].z - run.ankles[1].z).abs() > 0.05, "legs should be apart mid-stride");
        // Skiing, the feet slide together, staggered, flat on the ground
        // under bent knees, whatever the stride's phase.
        i.ski = 1.0;
        i.vel = Vec3::new(0.0, 0.0, -30.0);
        let ski = pose(&i);
        let other = pose(&PoseInput { stride: i.stride + 1.9, ..i });
        for s in 0..2 {
            assert!(ski.ankles[s].distance(other.ankles[s]) < 1e-3, "a skier's feet don't run");
            assert!((ski.ankles[s].y - ski.ankles[1 - s].y).abs() < 1e-3, "both skis on the ground");
            assert!(ski.hips[s].distance(ski.ankles[s]) < 0.8, "knees bent over the skis");
        }
        // In the air the legs hang: both feet below the knees, toes down.
        i.ground = 0.0; i.ski = 0.0; i.jetting = true;
        let fly = pose(&i);
        for s in 0..2 {
            assert!(fly.ankles[s].y < fly.knees[s].y - 0.25, "dangling shin");
            let toe = fly.feet[s].transform_vector3(Vec3::NEG_Z);
            assert!(toe.y < -0.4, "toes hang down: {toe}");
        }
        // Free arms swing opposite the legs on the run; gripping hands hold.
        i = input(0.1);
        i.hold = Hold::Free;
        let free = pose(&i);
        assert!((free.hands[0].z - free.hands[1].z).abs() > 0.05, "arms swing");
        // Football is empty-handed: the carrier's left arm swings free and
        // the right tucks the ball at the ribs, the elbow down by the hip.
        i.hold = Hold::Carrier;
        let carrier = pose(&i);
        assert!(carrier.hands[0].distance(free.hands[0]) < 1e-5, "the free arm swings");
        assert!(carrier.hands[1].distance(carrier.chest.transform_point3(TUCK_HAND)) < 1e-4, "the ball at the ribs");
        assert!(carrier.elbows[1].y < carrier.hands[1].y, "the elbow down by the hip");
        // A throw's follow-through reaches forward and up, then the arm swings again.
        i.hold = Hold::Free;
        i.throw = 0.9;
        let thrown = pose(&i);
        let (hand, shoulder) = (thrown.hands[1], thrown.shoulders[1]);
        assert!(hand.y > shoulder.y && hand.z < shoulder.z - 0.4, "the throw reaches out and up: {hand} from {shoulder}");
        i.throw = 0.1;
        assert!(pose(&i).hands[1].distance(free.hands[1]) < 1e-5);
    }

    #[test]
    fn lod_part_counts_are_bounded() {
        let mut p = runner();
        for (ski, jet, ground, carry) in [(false, false, true, false), (true, false, true, true), (false, true, false, false)] {
            p.skiing = ski; p.jetting = jet; p.on_ground = ground;
            p.carrying = if carry { Some(Team::Glacier) } else { None };
            for weapon in 0..3 {
                p.weapon = weapon;
                let (close, close_emit) = part_count(&p, 0.4, Lod::Close);
                let (near, near_emit) = part_count(&p, 0.4, Lod::Near);
                let (far, far_emit) = part_count(&p, 0.4, Lod::Far);
                println!("parts: close {close}, near {near}, far {far} (ski {ski} jet {jet} carry {carry} weapon {weapon})");
                assert!(close <= 104, "close LOD {close}");
                assert!(near <= 96, "near LOD {near}");
                assert!(far <= 26, "far LOD {far}");
                assert!(far < near && near < close);
                assert!(close_emit <= 4 && near_emit <= 4 && far_emit == 0);
            }
        }
    }

    #[test]
    fn corpses_collapse_then_vanish() {
        let mut p = runner();
        p.alive = false;
        p.respawn = RESPAWN_DELAY - 0.3;
        assert!(part_count(&p, 0.0, Lod::Near).0 > 0);
        p.respawn = RESPAWN_DELAY - CORPSE_TIME - 0.1;
        assert_eq!(part_count(&p, 0.0, Lod::Near).0, 0);
        let lying = pose(&PoseInput { dead_for: Some(1.0), ..input(0.0) });
        let head_y = lying.body.transform_point3(lying.head.transform_point3(Vec3::ZERO)).y;
        assert!(head_y < 0.8, "a settled corpse lies down, head at {head_y}");
    }

    const DT: f32 = 1.0 / 60.0;

    /// Feed the tracker `frames` of one player's state; count landing cues.
    fn feed(t: &mut AnimTracker, p: &mut Player, frames: usize, on_ground: bool, vy: f32) -> usize {
        let mut landings = 0;
        for _ in 0..frames {
            p.on_ground = on_ground;
            p.vel.y = vy;
            t.update(std::slice::from_ref(p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
            if t.motion(anim_id(0, &p)).land_age == 0.0 { landings += 1; }
        }
        landings
    }

    #[test]
    fn a_hard_landing_squashes_once_and_jitter_does_not() {
        let mut t = AnimTracker::default();
        let mut p = runner();
        feed(&mut t, &mut p, 10, true, 0.0);
        // A one-frame airborne blip over a bump (snapshot jitter): no squash.
        assert_eq!(feed(&mut t, &mut p, 1, false, -2.0) + feed(&mut t, &mut p, 5, true, 0.0), 0);
        // A slow step down: no squash either.
        assert_eq!(feed(&mut t, &mut p, 30, false, -5.0) + feed(&mut t, &mut p, 5, true, 0.0), 0);
        // A real fall: exactly one squash, scaled by impact speed.
        assert_eq!(feed(&mut t, &mut p, 60, false, -24.0) + feed(&mut t, &mut p, 60, true, 0.0), 1);
        let mut t2 = AnimTracker::default();
        let mut q = runner();
        feed(&mut t2, &mut q, 5, true, 0.0);
        feed(&mut t2, &mut q, 60, false, -12.0);
        feed(&mut t2, &mut q, 1, true, 0.0);
        let soft = t2.motion(anim_id(0, &q)).land_strength;
        feed(&mut t, &mut p, 60, false, -30.0);
        feed(&mut t, &mut p, 1, true, 0.0);
        assert!(t.motion(anim_id(0, &p)).land_strength > soft, "harder landings squash more");
    }

    #[test]
    fn a_hop_from_a_run_is_a_bound_and_a_skiers_hop_keeps_the_tuck() {
        // Football's held jump: hops of about 0.45 s from landing to
        // landing at 12 m/s. Each flight carries the stride on to the other
        // foot's landing with the legs split; a hop off a ski slide stays
        // tucked.
        let chain = |skiing: bool| {
            let mut t = AnimTracker::default();
            let mut p = runner();
            p.vel = Vec3::new(0.0, 0.0, -12.0);
            let (mut landings, mut split) = (Vec::new(), 0.0f32);
            let feed = |t: &mut AnimTracker, p: &mut Player, on_ground: bool, vy: f32| {
                (p.on_ground, p.skiing, p.vel.y) = (on_ground, skiing && on_ground, vy);
                t.update(std::slice::from_ref(p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
            };
            for _ in 0..30 { feed(&mut t, &mut p, true, 0.0); }
            // A skier: a slide, then one hop off it.
            for hop in 0..if skiing { 1 } else { 6 } {
                for k in 0..27 {
                    feed(&mut t, &mut p, false, 4.4 - 8.8 * k as f32 / 26.0);
                    if (hop > 0 || skiing) && k == 13 {
                        let m = t.motion(anim_id(0, &p));
                        let pose = pose(&PoseInput { stride: m.stride, ground: m.ground, ski: m.ski, back: m.back, hop: m.hop, bound: m.bound,
                            seed: seed_of(m.who), ..PoseInput::from_player(&p, 0.0) });
                        split = split.max((pose.ankles[0].z - pose.ankles[1].z).abs());
                    }
                }
                for _ in 0..2 { feed(&mut t, &mut p, true, 0.0); }
                let m = t.motion(anim_id(0, &p));
                if hop > 0 { landings.push((m.stride + seed_of(m.who)) / PI); }
            }
            (landings, split)
        };
        let (landings, split) = chain(false);
        for w in landings.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!((a - a.round()).abs() < 0.2 && (b - b.round()).abs() < 0.2, "each bound lands on a footfall: {landings:?}");
            assert!((b.round() - a.round()) as i32 % 2 != 0, "the feet take turns: {landings:?}");
        }
        assert!(split > 0.45, "a bound's legs are split mid-flight: {split}");
        let (_, tucked) = chain(true);
        assert!(tucked < 0.3, "a skier's hop keeps the feet together: {tucked}");
    }

    #[test]
    fn server_bots_ids_still_stride() {
        // Server bots' network IDs start at 0xF000_0000: a seed made from
        // one must stay small enough for the stride to move the legs.
        let mut p = runner();
        p.net_id = peakrunner_core::sim::BOT_ID_BASE + 5;
        p.on_ground = true;
        p.vel = Vec3::new(0.0, 0.0, -9.0);
        let mut t = AnimTracker::default();
        let mut ankles = Vec::new();
        for frame in 0..40 {
            t.update(std::slice::from_ref(&p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
            if frame % 8 == 7 {
                let m = t.motion(anim_id(0, &p));
                let pose = pose(&PoseInput { stride: m.stride, seed: seed_of(m.who), ground: 1.0, hop: 0.0, ski: 0.0, ..PoseInput::from_player(&p, 0.0) });
                ankles.push(pose.ankles[0]);
            }
        }
        let spread = ankles.iter().map(|a| a.distance(ankles[0])).fold(0.0, f32::max);
        assert!(spread > 0.2, "the legs move through the stride: {spread}");
    }

    #[test]
    fn offline_bodies_keep_their_own_stride_and_looks() {
        // Offline every network ID is 0. Each body still has its own track:
        // a runner's stride advances at its own cadence, not once for every
        // player on the field, and each has its own face and gait seed.
        let mut world = World::new();
        world.start_match(true);
        let mut players: Vec<Player> = world.players.iter().take(8).cloned().collect();
        assert!(players.len() >= 4 && players.iter().all(|p| p.net_id == 0), "an offline roster");
        for (k, p) in players.iter_mut().enumerate() {
            (p.alive, p.on_ground, p.skiing, p.stun, p.yaw) = (true, true, false, 0.0, 0.0);
            p.vel = if k == 0 { Vec3::new(0.0, 0.0, -11.0) } else { Vec3::ZERO };
        }
        let mut t = AnimTracker::default();
        let frames = 60;
        for _ in 0..=frames { t.update(&players, DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0); }
        for (k, p) in players.iter().enumerate() {
            let m = t.motion(anim_id(k, p));
            let want = frames as f32 * DT * cadence(gait_speed(p));
            assert!((m.stride - want).abs() < 1e-3 * want, "player {k}: stride {} for {want}", m.stride);
            assert_eq!(m.who, k as u32 + 1, "player {k}'s looks are their own");
        }
    }

    #[test]
    fn squash_and_switch_curves_are_continuous_and_settle() {
        let m = |land_age: f32, switch_age: f32| Motion { land_age, land_strength: 1.0, switch_age, switch_from: 1, ..Motion::default() };
        let mut prev = (m(0.0, 0.0).squash(), m(0.0, 0.0).lower());
        let mut peak = 0.0f32;
        // Sampled finely: no step between neighbouring instants (the dip is
        // quick, 0.08 s, but smooth).
        for k in 1..=480 {
            let a = k as f32 / 480.0;
            let now = (m(a, a).squash(), m(a, a).lower());
            assert!((now.0 - prev.0).abs() < 0.05 && (now.1 - prev.1).abs() < 0.05, "jump at {a}: {prev:?} -> {now:?}");
            peak = peak.max(now.0);
            prev = now;
        }
        assert!(peak > 0.9, "a full-strength landing reaches a full squash");
        assert_eq!(m(1.0, 1.0).squash(), 0.0);
        assert_eq!(m(1.0, 1.0).lower(), 0.0);
        // The posed hips sink during the squash.
        let flat = pose(&input(0.0)).pelvis.y;
        let sunk = pose(&PoseInput { squash: 1.0, ..input(0.0) }).pelvis.y;
        assert!(sunk < flat - 0.15);
    }

    #[test]
    fn a_weapon_switch_plays_once_and_swaps_the_model_midway() {
        let mut t = AnimTracker::default();
        let mut p = runner();
        p.weapon = 0;
        t.update(std::slice::from_ref(&p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
        // First sight never plays a switch.
        assert!(t.motion(anim_id(0, &p)).switch_age > SWITCH_TIME);
        p.weapon = 1;
        let mut starts = 0;
        let mut shown = Vec::new();
        for _ in 0..40 {
            t.update(std::slice::from_ref(&p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
            let m = t.motion(anim_id(0, &p));
            if m.switch_age == 0.0 { starts += 1; }
            shown.push(m.shown_weapon(p.weapon));
        }
        assert_eq!(starts, 1, "one switch, one animation");
        assert_eq!(shown.first(), Some(&0), "the old weapon goes down first");
        assert_eq!(shown.last(), Some(&1), "the new weapon comes up");
        // Respawning with a different weapon is a reset, not a switch.
        p.alive = false;
        t.update(std::slice::from_ref(&p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
        p.alive = true;
        p.weapon = 2;
        t.update(std::slice::from_ref(&p), DT, &|a: Vec3, b: Vec3| a.distance(b), &|_: Vec3| -1000.0, 0.0);
        assert!(t.motion(anim_id(0, &p)).switch_age > SWITCH_TIME);
    }

    /// The lowest and highest points of a body's authored armor as drawn
    /// (skinned parts bent by their palettes), in the world.
    fn armor_extent(p: &Player, motion: Motion) -> Option<(f32, f32)> {
        let (id, _) = armor_model(p.armor.def().build)?;
        let (mut lit, mut emit, mut models, mut bones) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        push_player(&mut lit, &mut emit, Some((&mut models, &mut bones)), p, 0.0, Lod::Close, motion, Hold::Weapon, None, Livery::team(p.team));
        let kit = crate::model::by_id(id)?;
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for d in models.iter().filter(|d| d.id == id) {
            let part = &kit.parts[d.part as usize];
            for &k in &kit.indices[part.start as usize..(part.start + part.count) as usize] {
                let v = &kit.vertices[k as usize * crate::model::VERTEX_FLOATS..][..crate::model::VERTEX_FLOATS];
                let at = Vec3::new(v[0], v[1], v[2]);
                let posed = if d.skin == crate::drawlist::RIGID { at } else {
                    let palette = &bones[d.skin as usize * crate::model::MAX_BONES..];
                    (0..4).map(|j| palette[v[12 + j] as usize].transform_point3(at) * v[16 + j]).sum()
                };
                let y = d.model.transform_point3(posed).y;
                (low, high) = (low.min(y), high.max(y));
            }
        }
        Some((low, high))
    }

    /// Standing in each armor, the boots are on the ground under the
    /// collision sphere and the head inside the hit volume's head zone (from
    /// 0.35 m under the eye to the hit sphere's top, 0.9 m up, radius 0.97).
    /// Running, no boot sinks into the ground.
    #[test]
    fn standing_bodies_reach_the_ground_and_keep_their_heads_in_the_hit_zone() {
        let r = peakrunner_core::terrain::PLAYER_RADIUS;
        let eye = peakrunner_core::terrain::EYE;
        for size in ["light", "medium", "heavy"] {
            let mut p = runner();
            p.armor = crate::sim::ArmorClass::by_id(size).unwrap();
            p.vel = Vec3::ZERO;
            p.on_ground = true;
            let Some((low, high)) = armor_extent(&p, settled(&p)) else { continue };
            let input = PoseInput { size: body_size(&p, true, true), ..PoseInput::from_player(&p, 0.0) };
            let head = (body_frame(&p, true, true) * pose(&input).head).transform_point3(Vec3::new(0.0, 0.155, 0.0));
            println!("{size}: boots {low:.3}, top {high:.3}, head {:.3} (ground {:.3})", head.y, -r);
            assert!((low + r).abs() < 0.03, "{size}: the boots are {:.3} m off the ground", low + r);
            assert!(head.y > eye - 0.35 && head.y < 0.9 + 0.97 - 0.05, "{size}: the head's middle {:.3} leaves the head zone", head.y);
            // The light armor's antenna rises a little past the hit volume.
            assert!(high < 0.9 + 0.97 + 0.18, "{size}: the top {high:.3} rises well past the hit volume");
            p.vel = Vec3::new(0.0, 0.0, -9.0);
            let mut motion = settled(&p);
            for k in 0..24 {
                motion.stride = 0.01 + k as f32 / 24.0 * TAU;
                let (low, _) = armor_extent(&p, motion).unwrap();
                assert!(low > -r - 0.035, "{size}: a running boot sinks {:.3} m into the ground", -r - low);
            }
        }
    }

    /// Drawn at full stature, a running body's planted toe stays where it
    /// came down in the world, in every armor, forward and strafing.
    #[test]
    fn drawn_feet_stay_planted_on_the_ground() {
        let ground = -peakrunner_core::terrain::PLAYER_RADIUS;
        for (size, vel) in [("light", Vec3::new(0.0, 0.0, -9.0)), ("heavy", Vec3::new(0.0, 0.0, -6.0)), ("medium", Vec3::new(5.0, 0.0, -5.0))] {
            let mut p = runner();
            p.armor = crate::sim::ArmorClass::by_id(size).unwrap();
            p.yaw = 0.4;
            p.vel = Mat4::from_rotation_y(p.yaw).transform_vector3(vel);
            p.on_ground = true;
            let mut motion = settled(&p);
            motion.stride = 0.01;
            let dt = 1.0 / 240.0;
            let (mut prev, mut planted) = ([None::<Vec3>; 2], 0);
            for _ in 0..240 {
                p.pos += p.vel * dt;
                motion.stride += dt * cadence(gait_speed(&p));
                let input = PoseInput { stride: motion.stride, ground: 1.0, ski: 0.0, back: 0.0, hop: 0.0, size: body_size(&p, true, true),
                    ..PoseInput::from_player(&p, 0.0) };
                let (frame, pose) = (body_frame(&p, true, true), pose(&input));
                for s in 0..2 {
                    let toe = (frame * pose.feet[s]).transform_point3(crate::gait::TOE);
                    let down = (toe.y - p.pos.y - ground).abs() < 2e-5;
                    if let (true, Some(was)) = (down, prev[s]) {
                        let slide = Vec2::new(toe.x - was.x, toe.z - was.z).length();
                        assert!(slide < 0.004, "{size}: a planted toe slid {slide:.4} m in a frame");
                        planted += 1;
                    }
                    prev[s] = down.then_some(toe);
                }
            }
            assert!(planted > 30, "{size}: only {planted} planted frames");
        }
    }

    #[test]
    fn a_downed_body_falls_the_way_it_was_knocked_and_never_through_a_wall() {
        let mut p = runner();
        p.yaw = 0.0;
        // Knocked toward +x with open ground: a full fall toward +x.
        p.vel = Vec3::new(12.0, 0.0, 0.0);
        let open = |a: Vec3, b: Vec3| a.distance(b);
        let (dir, tilt) = fall_for(&p, &open);
        assert!(dir.x > 0.9 && (tilt - FALL_TILT).abs() < 1e-5, "{dir} {tilt}");
        // A pillar 0.8 m away on that side, open behind: fall the other way.
        let pillar = |a: Vec3, b: Vec3| if b.x > a.x { 0.8 } else { a.distance(b) };
        let (dir, tilt) = fall_for(&p, &pillar);
        assert!(dir.x < -0.9 && (tilt - FALL_TILT).abs() < 1e-5, "{dir} {tilt}");
        // Boxed in both ways: tip only as far as fits.
        let boxed = |_: Vec3, _: Vec3| 1.0;
        let (_, tilt) = fall_for(&p, &boxed);
        assert!(tilt < 0.5, "{tilt}");
        let mut i = input(0.1);
        i.dead_for = Some(1.0);
        i.fall_dir = Vec2::new(1.0, 0.0);
        i.fall_tilt = tilt;
        let head = pose(&i).body.transform_point3(Vec3::new(0.0, 1.8, 0.0));
        assert!(head.x.abs() < 1.0 - 0.35 + 0.1, "the head stays inside the room: {head}");
    }
}

