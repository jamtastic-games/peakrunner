//! Football: a ball mode with no weapons: one ball, passes, fumbles, tackles
//! and touchdowns. The server owns every outcome; snapshots carry the ball whole,
//! so clients draw it and announce plays by diffing state.
//!
//! Stadium maps declare the field in their manifest (`football`: end zones,
//! kickoff spots, zone radius), and only they offer the mode. Without one (tests),
//! each team's end zone is its own flag stand and a new ball appears part of the
//! way from the receiving team's end zone to the other. Marker: `ball2`.
#[cfg(test)]
use crate::map_catalog::SupportedMode;
use super::*;
use crate::feed::Entry;


// Classic movement: Football's own player physics, stepped on a fixed tick.
// Football only; CTF keeps the approved movement bit for bit.

/// The classic movement tick (31.25 Hz).
const CLASSIC_TICK: f32 = 0.032;
/// Jump impulse 50 on mass 9.
pub const JUMP_SPEED: f32 = 50.0 / 9.0;
/// You can jump for this long after touching a jumpable surface (256 ms),
/// once per touch. Holding jump re-jumps at every landing: that is skiing,
/// and the only way to keep speed on the ground.
pub const JUMP_WINDOW: f32 = 0.256;
/// A surface at least this upright can be jumped from.
const JUMP_SURFACE_MIN_DOT: f32 = 0.2;
/// On the ground your velocity is pulled toward your run speed at up to
/// ground force / mass (360 / 9), times a traction of 3 times the surface's
/// uprightness (capped at 1). Only after touching the ground: there is no
/// air control at all.
const GROUND_ACCEL: f32 = 40.0;
const GROUND_TRACTION: f32 = 3.0;
/// Run speeds: 11 forward, 10 backward and sideways, and the ball carrier's
/// slower 8, 7 and 7.
pub const RUN_FORWARD: f32 = 11.0;
pub const RUN_SIDE: f32 = 10.0;
pub const CARRIER_FORWARD: f32 = 8.0;
pub const CARRIER_SIDE: f32 = 7.0;
/// In the air with a movement key held, the jet splits: up to this share
/// pushes along the keys and only the rest lifts, less than gravity. It
/// shrinks toward 0 as your speed along the keys nears JET_FORWARD_VELOCITY
/// (in practice never).
const JET_SIDE_FACTOR: f32 = 0.8;
const JET_FORWARD_VELOCITY: f32 = 250.0;
/// A player at rest on the ground slower than this stops dead (MINSPEED).
const CRAWL_STOP: f32 = 0.75;
/// Hitting any surface faster than 25 m/s hurts 0.005 of armor per m/s,
/// against a maximum armor of 0.66.
const MIN_DAMAGE_SPEED: f32 = 25.0;
const IMPACT_DAMAGE: f32 = 0.005 / 0.66 * 100.0;

/// One player's controls for `classic_move`.
pub(super) struct Controls { pub wish_x: f32, pub wish_z: f32, pub jump: bool, pub jet: bool, pub carrier: bool }

/// One step of classic movement for player `p`. Returns the damage from
/// hitting a surface too fast (the caller applies it).
pub(super) fn classic_move(map: MapId, water: &[crate::water::Volume], p: &mut Player, c: Controls,
    edge: f32, dt: f32) -> f32 {
    let (fwd, right) = move_basis(p.yaw);
    let (ahead, other) = if c.carrier { (CARRIER_FORWARD, CARRIER_SIDE) } else { (RUN_FORWARD, RUN_SIDE) };
    let (wet, volume) = crate::water::immersion(map, water, p.pos);
    let along = c.wish_z.clamp(-1.0, 1.0) * if c.wish_z > 0.0 { ahead } else { other };
    let run = (fwd * along + right * c.wish_x.clamp(-1.0, 1.0) * other).clamp_length_max(ahead)
        * (1.0 - crate::water::WADE_SLOW * wet);
    let dir = run.normalize_or_zero();
    // Friction applies on the tick after a collision.
    let touched = p.on_ground;
    let (floor, face) = crate::terrain::support_on(map, p.pos);
    let nrm = if crate::map_pack::on(map).is_some() { face } else { crate::terrain::normal_on(map, p.pos.x, p.pos.z) };

    // Jump: straight up by the surface's uprightness, plus a push along the
    // keys when they lead away from it (down a slope).
    let hopped = c.jump && p.coyote > 0.0;
    if hopped {
        p.vel.y += JUMP_SPEED * nrm.y.max(0.0);
        let away = nrm.dot(dir);
        if away > 0.0 { p.vel += dir * (JUMP_SPEED * away); }
        p.coyote = 0.0;
    }

    // Jet and energy. The recharge never stops, even while jetting.
    let armor = crate::defs::defs().armors.football;
    let jet_cost = if wet >= crate::water::DEEP { crate::water::JET_ENERGY_FACTOR } else { 1.0 };
    let spend = armor.drain * jet_cost * dt;
    p.jetting = c.jet && p.energy >= spend;
    p.energy += armor.regen * dt;
    let mut accel = Vec3::new(0.0, -crate::defs::movement().gravity, 0.0);
    if p.jetting {
        // Side jets only once off the ground (the jump window has run out, or
        // you jumped) and with a movement key held.
        if p.coyote <= 0.0 && dir != Vec3::ZERO {
            let speed_along = p.vel.dot(dir);
            let side = if speed_along > JET_FORWARD_VELOCITY { 0.0 }
                else if speed_along < 0.0 { JET_SIDE_FACTOR }
                else { (1.0 - speed_along / JET_FORWARD_VELOCITY).min(JET_SIDE_FACTOR) };
            accel += dir * (side * armor.jet) + Vec3::Y * ((1.0 - side) * armor.jet);
        } else {
            accel.y += armor.jet;
        }
        p.energy -= spend;
    }
    p.energy = p.energy.clamp(0.0, crate::defs::movement().energy_max);
    p.vel += accel * dt;

    // Ground friction pulls the horizontal velocity toward the run speed. A
    // hop costs one movement tick of it, which is what slows a skier on the flat.
    if touched {
        let mut damp = run - p.vel;
        damp.y = 0.0;
        let tick = if hopped { dt.max(CLASSIC_TICK) } else { dt };
        let force = GROUND_ACCEL * (nrm.y * GROUND_TRACTION).clamp(0.0, 1.0) * tick;
        let len = damp.length();
        if len > force { damp *= force / len; }
        p.vel += damp;
    }
    if let (true, Some(volume)) = (wet > 0.0, volume) {
        crate::water::apply(&mut p.vel, wet, false, volume, crate::defs::movement().gravity, dt);
    }
    let h = Vec2::new(p.vel.x, p.vel.z).length();
    if h > crate::defs::movement().horiz_max { p.vel.x *= crate::defs::movement().horiz_max / h; p.vel.z *= crate::defs::movement().horiz_max / h; }

    // Contact takes away the velocity into the surface and nothing else.
    let ground_y = floor + PLAYER_RADIUS;
    if p.pos.y < ground_y {
        p.pos.y = ground_y;
        let vn = p.vel.dot(nrm);
        if vn < 0.0 { p.vel -= nrm * vn; }
    }
    p.skiing = false;
    p.on_ground = false;
    let impact = move_over_terrain(map, p, false, dt);
    let (_, face) = crate::terrain::support_on(map, p.pos);
    if p.on_ground && !c.jet && !hopped && run == Vec3::ZERO && p.vel.length() < CRAWL_STOP {
        p.vel = Vec3::ZERO;
    }
    if p.on_ground && face.y >= JUMP_SURFACE_MIN_DOT {
        p.coyote = JUMP_WINDOW;
    } else {
        p.coyote = (p.coyote - dt).max(0.0);
    }
    // Held jump at speed reads as skiing (pose and sound).
    p.skiing = c.jump && p.on_ground && Vec2::new(p.vel.x, p.vel.z).length() > ahead + 1.0;

    if p.pos.x < 6.0 { p.pos.x = 6.0; p.vel.x = p.vel.x.abs() * 0.4; }
    if p.pos.x > edge { p.pos.x = edge; p.vel.x = -p.vel.x.abs() * 0.4; }
    if p.pos.z < 6.0 { p.pos.z = 6.0; p.vel.z = p.vel.z.abs() * 0.4; }
    if p.pos.z > edge { p.pos.z = edge; p.vel.z = -p.vel.z.abs() * 0.4; }
    (impact - MIN_DAMAGE_SPEED).max(0.0) * IMPACT_DAMAGE
}

/// Top of a raised goal platform above the floor (Highgoal). With no air
/// control you get up there from a run: jump, jet with the keys let go, and
/// let your run carry you over the edge.
pub const RAISED_GOAL_HEIGHT: f32 = 7.0;
/// Bot leaps: the run-up starts this far out from the zone, the jump comes
/// inside the launch window, and the platform's edge is this far from the
/// zone's centre (plus the body's radius).
const LEAP_RUNUP: f32 = 16.0;
const LEAP_LAUNCH: std::ops::Range<f32> = 9.5..12.5;
const LEAP_EDGE: f32 = 4.0 + PLAYER_RADIUS + 0.1;
/// Carriers stop jetting this far from the zone and walk in (8 m/s): the
/// walk recharges about 40 energy on the way.
const LEAP_APPROACH: f32 = 60.0;
/// Energy a bot wants before leaping: the jump and a feathered jet use about
/// two thirds of a tank from the launch window (`a_running_leap_with_a_feathered_jet_lands_on_the_raised_goal`).
const LEAP_ENERGY: f32 = 45.0;

/// Football plays on time: two 15-minute halves, with a score limit high
/// enough (40) that it rarely ends a game early.
pub const HALF_SECONDS: f32 = 15.0 * 60.0;
pub const TOUCHDOWNS_TO_WIN: u32 = 40;
/// Ctrl+K takes this long in Football.
pub const SUICIDE_DELAY: f32 = 15.0;
/// Respawn delay and pre-match warmup.
pub const RESPAWN_SECONDS: f32 = 1.0;
pub const WARMUP_SECONDS: f32 = 20.0;
/// Carrying the ball into this sphere around the enemy flag stand scores.
pub const END_ZONE_RADIUS: f32 = 12.0;
/// Closing speed (rounded to whole m/s) at which the slower player is tackled.
pub const TACKLE_SPEED: f32 = 15.0;
/// Below TACKLE_SPEED, down to this, a slower carrier fumbles instead.
pub const FUMBLE_SPEED: f32 = 2.0;
/// How long a tackled player stays down, unable to move.
pub const TACKLE_STUN: f32 = 2.0;
pub const TACKLE_DAMAGE: f32 = 30.0;
/// A break at half time: the teams swap ends, everyone regroups 3 s before
/// it ends, and the next ball goes to the side opposite whoever last had it.
pub const HALFTIME_BREAK: f32 = 10.0;
/// Seconds before the half-time break ends that everyone is sent back to
/// their (new) spawns.
const HALFTIME_REGROUP_BEFORE: f32 = 3.0;
/// Personal points for a touchdown and for tackling the carrier.
pub const TOUCHDOWN_POINTS: u32 = 5;
pub const TACKLE_POINTS: u32 = 1;
/// A new ball falls in from this far above its kickoff spot (m).
pub const BALL_DROP: f32 = 30.0;
pub const BALL_RADIUS: f32 = 0.3;
/// After a pass the thrower can't throw again for this long (s); the client
/// draws the throw's follow-through over it.
pub const PASS_COOLDOWN: f32 = 0.5;
/// The field's `bounds` are the arena's inside. The ball and players are out
/// only this far past them: over a wall and gone, never just against it
/// (Highgoal's walls are 2 m thick, 0.5 m outside its bounds).
const OUT_MARGIN: f32 = 3.0;
/// Health lost per second outside the field's bounds (0.065 of a 0.66 armor).
const OUT_OF_BOUNDS_DAMAGE: f32 = 0.065 / 0.66 * 100.0;
/// A ball that sits still this long is reset.
const IDLE_RESET: f32 = 20.0;
/// After a touchdown: players regroup, then a new ball appears.
const REGROUP_AFTER: f32 = 6.0;
const NEW_BALL_AFTER: f32 = 10.0;
const RESET_BALL_AFTER: f32 = 2.0;
/// With nobody under the crosshair, a pass takes its power from the nearest
/// player, either team, within this yaw of the thrower's facing: left and
/// right only, at any height and distance.
const PASS_CONE: f32 = 0.15;
/// How far along the look the crosshair finds a receiver.
const PASS_RANGE: f32 = 8000.0;
/// Power of a throw with nobody to aim at.
const LOOSE_THROW: f32 = 35.0;
/// Every throw is lifted `THROW_LIFT * cos(pitch)` rad above the look (the
/// classic engine's own lift, which fades looking straight up or down), and
/// a pass is tilted `PASS_TILT` back down from that (`throw_elevation`): the
/// whole 0.25 rad above a level look.
const THROW_LIFT: f32 = 0.5;
const PASS_TILT: f32 = 0.25;
/// How far up or down a Football player looks: 88 degrees (other modes
/// keep `sim::LOOK_LIMIT`).
pub const LOOK_LIMIT: f32 = 88.0 * std::f32::consts::PI / 180.0;
/// A thrown or dropped ball ignores whoever let it go for this long, so a
/// fumble can't undo itself and a pass can't be caught by its thrower.
const THROWER_NO_CATCH: f32 = 1.0;
const FUMBLE_POP: f32 = 10.0;
/// The ball's own physics values (the classic engine's defaults for an item
/// that sets none): friction against a surface and its share of an
/// impact's speed it bounces back with, each times the surface's own.
const BALL_FRICTION: f32 = 1.0;
const BALL_ELASTICITY: f32 = 0.2;
/// Surface materials, (friction, elasticity): the ground is grass, anything
/// built the default material.
const TURF: (f32, f32) = (1.4, 0.6);
const BUILT: (f32, f32) = (1.0, 1.0);
/// How long a dead player's body stays down on the field (s), its player
/// already back in play (the client sinks it over the last BODY_SINKS).
pub const BODY_LIES: f32 = 10.0;
pub const BODY_SINKS: f32 = 1.5;
/// The chance a carrier touching a body fumbles, for each 32 ms tick of
/// contact: the classic's for its dropped ball-thrower, which the body
/// stands in for now that nobody holds one (2026-10-14).
const BODY_FUMBLE_CHANCE: f32 = 0.30;
/// New balls appear this far from the receiving team's end zone to the other.
const KICKOFF_FRACTION: f32 = 0.35;

/// A map's declared football field (manifest `football`). Index 0 is Ember's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    /// The end zone each team defends (centre of its scoring sphere).
    pub end_zones: [[f32; 3]; 2],
    /// Where a new ball appears on each team's side.
    pub kickoff: [[f32; 3]; 2],
    #[serde(default = "default_zone_radius")]
    pub radius: f32,
    /// In-bounds rectangle `[x0, z0, x1, z1]`: a ball outside it is reset.
    #[serde(default)]
    pub bounds: Option<[f32; 4]>,
}
fn default_zone_radius() -> f32 { END_ZONE_RADIUS }

impl Field {
    pub fn validate(&self) -> Result<(), String> {
        let finite = |p: &[f32; 3]| p.iter().all(|v| v.is_finite() && v.abs() <= 10000.0);
        if !self.end_zones.iter().chain(&self.kickoff).all(finite) || !(4.0..=40.0).contains(&self.radius) {
            return Err("Invalid football field".into());
        }
        if let Some([x0, z0, x1, z1]) = self.bounds {
            let inside = |p: &[f32; 3]| p[0] > x0 && p[0] < x1 && p[2] > z0 && p[2] < z1;
            if !(x0 < x1 && z0 < z1) || !self.end_zones.iter().chain(&self.kickoff).all(inside) {
                return Err("Invalid football bounds".into());
            }
        }
        Ok(())
    }
}

/// Whether `map` declares a football field (and so offers the mode).
pub fn has_field(map: MapId) -> bool {
    crate::map_pack::on(map).is_some_and(|p| p.manifest.football.is_some())
}

pub(crate) fn field(map: MapId) -> Option<&'static Field> {
    crate::map_pack::on(map).and_then(|p| p.manifest.football.as_ref())
}

/// A stretch of the ball's flight in one step: from, to, and its velocity
/// along it.
type Leg = (Vec3, Vec3, Vec3);

/// Whether the ball, moving from `from` to `to`, touches the body of a
/// player centred at `pos`: within the body's radius plus the ball's across,
/// and between their feet and the top of the body (with the ball's radius).
/// Returns how far across it passed, for picking the nearest.
fn touch(pos: Vec3, from: Vec3, to: Vec3) -> Option<f32> { touch_grown(pos, from, to, 1.0) }

/// A catch: the ball touches the body's zone grown CATCH_GROW every way.
fn catch_touch(pos: Vec3, from: Vec3, to: Vec3) -> Option<f32> { touch_grown(pos, from, to, CATCH_GROW) }

/// Catches reach this much (3%) farther than the body every way, across,
/// below and above (0.82 m across, 0.34 below the feet and over the top): a
/// little help for catches the server judges, where the catcher sees the
/// ball about a tenth of a second late. Marker `ball6`.
const CATCH_GROW: f32 = 1.03;

/// `touch` with the zone grown `grow` times about its middle.
fn touch_grown(pos: Vec3, from: Vec3, to: Vec3, grow: f32) -> Option<f32> {
    let feet = pos.y - PLAYER_RADIUS;
    let path = Vec2::new(to.x - from.x, to.z - from.z);
    let rel = Vec2::new(pos.x - from.x, pos.z - from.z);
    let t = if path.length_squared() > 1e-6 { (rel.dot(path) / path.length_squared()).clamp(0.0, 1.0) } else { 1.0 };
    let across = (rel - path * t).length();
    let y = from.y + (to.y - from.y) * t;
    let reach = (super::bump::BODY_RADIUS + BALL_RADIUS) * grow;
    let more = (super::bump::BODY_HEIGHT + 2.0 * BALL_RADIUS) * (grow - 1.0) * 0.5;
    (across < reach && y > feet - BALL_RADIUS - more && y < feet + super::bump::BODY_HEIGHT + BALL_RADIUS + more).then_some(across)
}

/// Where a body at `pos` moving at `vel` would be `t` seconds on, and its
/// velocity then, with nothing changing its course: the same way along the
/// ground, falling under gravity, landing on the floor below. Football has
/// no air control, so a body in the air goes exactly so; turns and jets
/// can't be foreseen. Games draw other players this way past their newest
/// snapshot (`src/online.rs`), and the server judges a hit on a bot where
/// such a drawing put it (`bump::seen_hit`).
pub fn coast(map: MapId, pos: Vec3, vel: Vec3, t: f32) -> (Vec3, Vec3) {
    if t <= 0.0 { return (pos, vel); }
    let g = crate::defs::movement().gravity;
    let (mut at, mut v) = (pos + vel * t - Vec3::Y * (0.5 * g * t * t), vel - Vec3::Y * (g * t));
    let floor = crate::terrain::support_on(map, Vec3::new(at.x, at.y.max(pos.y), at.z)).0 + PLAYER_RADIUS;
    if at.y < floor { at.y = floor; v.y = v.y.max(0.0); }
    (at, v)
}

/// Whether `p` has left `map`'s field: more than OUT_MARGIN past its bounds.
fn out_of_field(map: MapId, p: Vec3) -> bool {
    field(map).and_then(|f| f.bounds).is_some_and(|[x0, z0, x1, z1]|
        p.x < x0 - OUT_MARGIN || p.x > x1 + OUT_MARGIN || p.z < z0 - OUT_MARGIN || p.z > z1 + OUT_MARGIN)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ball {
    /// Football is the mode in force.
    pub active: bool,
    /// A ball is on the field. False between a score and the next ball.
    pub in_play: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    pub carrier: Option<usize>,
    /// Who last threw or dropped the ball (its owner).
    pub thrown_by: Option<usize>,
    /// Team that last held the ball.
    pub last_team: Option<Team>,
    /// Seconds before the thrower can catch their own throw.
    pub no_catch: f32,
    /// Seconds the ball has sat still.
    pub idle: f32,
    /// Seconds until the next ball when none is in play.
    pub respawn: f32,
    /// Seconds until everyone is sent back to spawn (0 = none pending).
    pub regroup: f32,
    /// Team whose side the next ball appears on.
    pub next_side: u8,
    /// 0 in the first half, 1 in the second.
    pub half: u8,
    /// Seconds towards the next out-of-bounds damage tick.
    #[serde(default)]
    pub oob_clock: f32,
    /// The heading (yaw) the ball left with: its thrower's facing, a
    /// fumble's random heading, 0 for a new ball. A catch pushes the
    /// catcher this way. Server only: not in snapshots.
    #[serde(skip)]
    pub facing: f32,
    /// Dead players' bodies down on the field (`DownedBody`).
    #[serde(default)]
    pub bodies: Vec<DownedBody>,
    /// The player a pass in the air goes to: the one whose distance set its
    /// power (`pass_throw`), as the feed's "passes toward" names. Their game
    /// warns them (`football_hud`). None once the ball strikes a surface,
    /// rests or is held. Marker `ball5`.
    #[serde(default)]
    pub pass_to: Option<usize>,
    /// The loose ball left its carrier without a throw: a fumble, or a
    /// carrier who died, respawned, switched team or left. Whoever gets it
    /// picks it up; nobody intercepts a dropped ball. Marker `ball6`.
    #[serde(default)]
    pub dropped: bool,
    /// Fumbles so far, wrapping: a game announces one when it changes, even
    /// if the ball was picked up again before the next snapshot.
    #[serde(default)]
    pub fumbles: u8,
}

/// A dead player's body on the field: it falls where they died, stays down
/// BODY_LIES after the death (their player respawned long before), and a
/// carrier touching it may fumble (`World::body_fumble`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DownedBody {
    pub pos: Vec3,
    /// Seconds it stays down.
    pub life: f32,
    /// The dead player it follows while they stay dead (the body may still
    /// be falling). Server only.
    #[serde(skip)]
    pub body: Option<usize>,
}

impl DownedBody {
    pub fn lying(&self) -> bool { self.life > 0.0 }
}

impl Ball {
    pub fn carried_by(&self, i: usize) -> bool { self.active && self.carrier == Some(i) }
    pub fn loose(&self) -> bool { self.active && self.in_play && self.carrier.is_none() }
}

/// A pass's elevation for a look pitch: `pitch + 0.5 cos(pitch) - 0.25`,
/// the classic engine's lift less the pass's tilt. 0.25 rad above a level
/// look; looking straight up (88 degrees) it leaves at 75 degrees, so the
/// ball comes down about 30 m ahead. (Until 2026-10-11 steep throws were
/// held to 65 degrees, which threw a straight-up look about 47 m ahead.)
pub fn throw_elevation(pitch: f32) -> f32 { drop_elevation(pitch) - PASS_TILT }

/// A dropped or fumbled ball's elevation: the engine's lift alone.
pub fn drop_elevation(pitch: f32) -> f32 { pitch + THROW_LIFT * pitch.cos() }

/// Where a thrown or dropped ball leaves a player: the middle of their
/// body (the collision box), as the classic engine throws.
fn release_point(p: &Player) -> Vec3 { p.pos + Vec3::Y * (super::bump::BODY_HEIGHT * 0.5 - PLAYER_RADIUS) }

/// One touch of a surface (normal `n`, material `(friction, elasticity)`),
/// as the classic engine handles an item's: the speed into the surface goes
/// (with a 0.0001 m/s nudge out), friction takes `ball x surface friction x
/// that speed` off the rest of its motion, all of it if that's more, and
/// it bounces back with `ball x surface elasticity` of the speed it hit
/// with.
fn touch_surface(vel: &mut Vec3, n: Vec3, (friction, elasticity): (f32, f32)) -> bool {
    let into = vel.dot(n);
    if into >= 0.0 { return false; }
    let out = 1e-4 - into;
    *vel += n * out;
    let grip = -into * BALL_FRICTION * friction;
    let slide = vel.length();
    *vel -= if slide > grip { *vel * (grip / slide) } else { *vel };
    *vel += n * (out * BALL_ELASTICITY * elasticity);
    true
}

/// The first surface the ball's sweep meets: how far along, its normal and
/// its material (terrain is turf, anything built the default). The same
/// sweeps as `surface_hit`.
fn ball_hit(map: MapId, pillars: &[Pillar], from: Vec3, to: Vec3, far: f32) -> Option<(f32, Vec3, (f32, f32))> {
    let mut best = crate::map_pack::on(map).and_then(|p| p.sweep(from, to, BALL_RADIUS)).map(|(t, n)| (t, n, BUILT));
    if let Some(t) = crate::terrain::segment_hit(map, from, to, BALL_RADIUS) {
        if best.is_none_or(|(old, ..)| t < old) {
            let at = from.lerp(to, t);
            best = Some((t, crate::terrain::surface_on(map, at.x, at.z).1, TURF));
        }
    }
    if let Some(t) = pillar_hit(map, pillars, from, to, BALL_RADIUS) {
        if best.is_none_or(|(old, ..)| t < old) {
            best = Some((t, grenade_contact_normal(map, pillars, from.lerp(to, t), far, to - from), BUILT));
        }
    }
    best
}

/// How far along the ray from `eye` (unit direction `dir`) it first meets
/// the body of a player centred at `pos`: the collision box `touch` and
/// `bump` use, an upright cylinder from the feet to BODY_HEIGHT up.
fn ray_body(eye: Vec3, dir: Vec3, pos: Vec3) -> Option<f32> {
    let feet = pos.y - PLAYER_RADIUS;
    let r = super::bump::BODY_RADIUS;
    let (mut lo, mut hi) = (0.0f32, f32::INFINITY);
    let (ox, oz) = (eye.x - pos.x, eye.z - pos.z);
    let a = dir.x * dir.x + dir.z * dir.z;
    let c = ox * ox + oz * oz - r * r;
    if a < 1e-9 {
        if c > 0.0 { return None; }
    } else {
        let b = ox * dir.x + oz * dir.z;
        let disc = b * b - a * c;
        if disc < 0.0 { return None; }
        lo = lo.max((-b - disc.sqrt()) / a);
        hi = hi.min((-b + disc.sqrt()) / a);
    }
    let top = feet + super::bump::BODY_HEIGHT;
    if dir.y.abs() < 1e-9 {
        if eye.y < feet || eye.y > top { return None; }
    } else {
        let (t0, t1) = ((feet - eye.y) / dir.y, (top - eye.y) / dir.y);
        lo = lo.max(t0.min(t1));
        hi = hi.min(t0.max(t1));
    }
    (lo <= hi).then_some(lo)
}

/// Throw speed for a receiver `distance` metres away: `6.454 · sqrt(d)`,
/// uncapped (the receiver search reaches 8 km). At a level look the loft makes the throw land about
/// that far away under the sim's gravity: range = v² sin(2·0.25) / 20 ≈ d.
pub fn pass_speed(distance: f32) -> f32 {
    6.454 * distance.max(0.0).sqrt()
}
/// Share of the thrower's own velocity the ball keeps: none, so a pass goes
/// where it's aimed whatever the thrower is doing.
pub const PASS_INHERIT: f32 = 0.0;

pub fn team_name(t: Team) -> &'static str { if t == Team::Ember { "EMBER" } else { "GLACIER" } }

impl World {
    pub fn football(&self) -> bool { self.mode.rules() == crate::modes::Rules::Football }

    /// How far up or down players look in the mode in force.
    pub fn look_limit(&self) -> f32 { if self.football() { LOOK_LIMIT } else { super::LOOK_LIMIT } }

    /// Match clock, from the mode (Football: two 15-minute halves).
    pub fn match_time(&self) -> f32 { self.mode.def().clock }

    /// The end of the field `team` plays from: its own in the first half,
    /// the other team's in the second. The teams swap ends at half time:
    /// spawns, end zones and kickoff spots.
    pub fn side(&self, team: Team) -> Team { if self.ball.half > 0 { team.other() } else { team } }

    /// The end zone a team defends: the map's declared field, or failing
    /// that its own flag stand. Ends swap at half time (`side`).
    pub fn end_zone(&self, team: Team) -> Vec3 {
        let side = self.side(team);
        field(self.map).map_or(self.flags[side.idx()].home, |f| Vec3::from(f.end_zones[side.idx()]))
    }

    pub fn end_zone_radius(&self) -> f32 { field(self.map).map_or(END_ZONE_RADIUS, |f| f.radius) }

    /// Where a new ball appears on `team`'s side of the field.
    pub fn kickoff_spot(&self, team: Team) -> Vec3 {
        if let Some(f) = field(self.map) { return Vec3::from(f.kickoff[self.side(team).idx()]); }
        let own = self.end_zone(team);
        let other = self.end_zone(team.other());
        let p = own.lerp(other, KICKOFF_FRACTION);
        let high = Vec3::new(p.x, own.y.max(other.y) + 80.0, p.z);
        let floor = crate::terrain::support_on(self.map, high).0;
        Vec3::new(p.x, floor.max(p.y - 40.0) + 1.0, p.z)
    }

    /// Fresh ball state for the mode in force: the first ball appears shortly
    /// on a random side.
    pub(super) fn reset_ball(&mut self) {
        if !self.football() { self.ball = Ball::default(); return; }
        let side = if self.rng() < 0.5 { 0 } else { 1 };
        self.ball = Ball { active: true, respawn: RESET_BALL_AFTER, next_side: side, ..Ball::default() };
    }

    pub(crate) fn play(&mut self, text: String) {
        crate::feed::push(&mut self.feed, Entry::Play { text });
    }

    fn spawn_ball(&mut self) {
        let team = if self.ball.next_side == 0 { Team::Ember } else { Team::Glacier };
        let pos = self.kickoff_spot(team);
        let b = &mut self.ball;
        b.in_play = true;
        // Dropped in from the sky over its kickoff spot.
        b.pos = pos + Vec3::Y * BALL_DROP;
        b.vel = Vec3::ZERO;
        b.carrier = None;
        b.thrown_by = None;
        b.last_team = None;
        b.idle = 0.0;
        b.no_catch = 0.0;
        b.facing = 0.0;
        b.pass_to = None;
        b.dropped = false;
        self.play("A new ball is in play".into());
        self.push_event("whistle");
    }

    /// No ball until the next one appears, on the side of the team that did
    /// not touch it last.
    fn dead_ball(&mut self, after: f32, side: Team) {
        let b = &mut self.ball;
        b.in_play = false;
        b.carrier = None;
        b.thrown_by = None;
        b.dropped = false;
        b.vel = Vec3::ZERO;
        b.respawn = after;
        b.next_side = side.idx() as u8;
    }

    /// A reset ball goes to the side that didn't touch it last, and everyone
    /// is sent back to spawn with it: at once for an idle
    /// ball, after `after` seconds for one that left the field.
    fn loose_reset(&mut self, text: &str, after: f32) {
        let side = self.ball.last_team.map(|t| t.other())
            .unwrap_or(if self.rng() < 0.5 { Team::Ember } else { Team::Glacier });
        self.dead_ball(after.max(STEP * 0.5), side);
        self.ball.regroup = after.max(STEP * 0.5);
        self.play(text.into());
    }

    /// The carrier loses the ball: it pops out along a random heading.
    pub(super) fn fumble(&mut self, i: usize) { self.drop_ball(i, true); }

    /// A player who dies, leaves, switches team or respawns drops the ball
    /// ahead of them.
    pub(crate) fn ball_lost(&mut self, i: usize) { self.drop_ball(i, false); }

    /// Dropping the ball: it's thrown at FUMBLE_POP along
    /// the carrier's look, turned to a random heading for a fumble (the pitch
    /// stays theirs), and it keeps the carrier's velocity (only passes drop
    /// it). After a hit that's the velocity
    /// the carrier was just given, the hitter's, so a knocked-loose ball flies
    /// on the way the hit came. Nobody intercepts it (`Ball::dropped`), and a
    /// fumble counts in `Ball::fumbles` for the games to announce.
    fn drop_ball(&mut self, i: usize, random: bool) {
        if !self.ball.carried_by(i) { return; }
        let heading = if random { Some(self.rng() * std::f32::consts::TAU) } else { None };
        let p = &self.players[i];
        let facing = heading.unwrap_or(p.yaw);
        let (from, vel) = (release_point(p), p.vel + look_dir(facing, drop_elevation(p.pitch)) * FUMBLE_POP);
        let b = &mut self.ball;
        b.carrier = None;
        b.thrown_by = Some(i);
        b.facing = facing;
        b.pos = from;
        b.vel = vel;
        b.no_catch = THROWER_NO_CATCH;
        b.idle = 0.0;
        b.dropped = true;
        if random { b.fumbles = b.fumbles.wrapping_add(1); }
        self.bots_look_up();
    }

    /// Players outside the field's bounds take 0.065 damage level (9.8% of
    /// light armor) each second until they return.
    fn out_of_bounds_damage(&mut self, dt: f32) {
        if field(self.map).and_then(|f| f.bounds).is_none() { return; }
        self.ball.oob_clock += dt;
        if self.ball.oob_clock < 1.0 { return; }
        self.ball.oob_clock -= 1.0;
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive || p.remote || !out_of_field(self.map, p.pos) { continue; }
            self.players[i].health -= OUT_OF_BOUNDS_DAMAGE;
            if i == self.player_id { self.msg("YOU'RE OUT OF BOUNDS!", 1.2); self.push_event("oob"); }
            if self.players[i].health <= 0.0 { self.kill(i, None, "Out of bounds"); }
        }
    }

    /// Everyone alive back to their spawns with full health and energy.
    fn regroup_players(&mut self) {
        for i in 0..self.players.len() {
            if self.players[i].alive && !self.players[i].remote {
                self.respawn(i);
                self.players[i].stun = 0.0;
            }
        }
    }

    /// Half time: the ball goes dead and the teams swap ends (`side`). The
    /// next ball goes to the side opposite the team that last had it (its
    /// carrier's, or its last thrower's), or a random side when there's no
    /// ball or nobody has touched it. Everyone is sent to their new spawns 3
    /// s before the break ends, and the second half gets its full time after
    /// the break: the clock gains the break.
    pub(super) fn check_halftime(&mut self) {
        let def = self.mode.def();
        if !self.ball.active || self.ball.half > 0 || def.halves < 2 || self.time_left > def.clock / 2.0 { return; }
        let b = &self.ball;
        let holder = if b.in_play { b.carrier.or(b.thrown_by).and_then(|k| self.players.get(k)) } else { None };
        let side = match holder.map(|p| p.team) {
            Some(t) => t.other(),
            None => if self.rng() < 0.5 { Team::Ember } else { Team::Glacier },
        };
        self.ball.half = 1;
        self.dead_ball(HALFTIME_BREAK, side);
        self.ball.regroup = HALFTIME_BREAK - HALFTIME_REGROUP_BEFORE;
        self.time_left += HALFTIME_BREAK;
        self.play("Half time".into());
        self.push_event("halftime");
    }

    /// Fire while carrying: pass. Power comes from the distance to the
    /// player under the crosshair (the first body along the look, either
    /// team, within PASS_RANGE), else to the nearest player of either team
    /// within PASS_CONE of the facing, left or right only, at any height and
    /// distance (else a fixed loose throw). The throw itself always goes
    /// along the look, lifted by `throw_elevation`, from the middle of the
    /// thrower's body, without the thrower's velocity.
    /// Leading a runner or lofting onto a ledge is the thrower's aim.
    pub(super) fn pass(&mut self, i: usize) {
        if self.predicting || !self.ball.carried_by(i) { return; }
        let (yaw, pitch, team) = (self.players[i].yaw, self.players[i].pitch, self.players[i].team);
        let (target, origin, velocity) = self.pass_throw(i, yaw, pitch);
        let b = &mut self.ball;
        b.carrier = None;
        b.pos = origin;
        b.vel = velocity;
        b.thrown_by = Some(i);
        b.dropped = false;
        b.last_team = Some(team);
        b.facing = yaw;
        b.no_catch = THROWER_NO_CATCH;
        b.idle = 0.0;
        b.pass_to = target;
        self.players[i].cooldown = PASS_COOLDOWN;
        self.bots_look_up();
        let name = self.display_name(i);
        let text = match target {
            Some(j) => format!("{name} {} toward {}", self.verb(i, "passes", "pass"), self.object_name(j)),
            None => format!("{name} {} the ball", self.verb(i, "throws", "throw")),
        };
        self.play(text);
        if i == self.player_id { self.push_event("pass"); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push(("pass", origin)); }
        if target == Some(self.player_id) && i != self.player_id { self.push_event("incoming"); }
    }

    /// The throw player `i` would make looking (yaw, pitch), as `pass` makes
    /// it: who sets its power (None for a loose throw), where it leaves and
    /// its velocity.
    pub(super) fn pass_throw(&self, i: usize, yaw: f32, pitch: f32) -> (Option<usize>, Vec3, Vec3) {
        let p = &self.players[i];
        let eye = p.pos + Vec3::Y * EYE;
        let dir = look_dir(yaw, pitch);
        let from = p.pos;
        // Under the crosshair: the nearest body along the look, unless
        // something solid comes first.
        let mut target: Option<(usize, f32)> = None;
        for (j, o) in self.players.iter().enumerate() {
            if j == i || !o.alive { continue; }
            let Some(along) = ray_body(eye, dir, o.pos) else { continue };
            if along <= PASS_RANGE && target.is_none_or(|(_, d)| along < d)
                && obstacle_hit(self.map, &self.pillars, eye, eye + dir * along, 0.0).is_none() {
                target = Some((j, along));
            }
        }
        // Else the nearest player whose direction is within PASS_CONE of the
        // facing, measured across the half turn (the classic check wasn't,
        // and facing one way on the map missed receivers there).
        if target.is_none() {
            for (j, o) in self.players.iter().enumerate() {
                if j == i || !o.alive { continue; }
                let to = o.pos - from;
                let angle = (-to.x).atan2(-to.z);
                let diff = (angle - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                let d = to.length();
                if diff.abs() <= PASS_CONE && target.is_none_or(|(_, best)| d < best) {
                    target = Some((j, d));
                }
            }
        }
        let origin = release_point(p);
        let speed = target.map_or(LOOSE_THROW, |(j, _)| pass_speed(self.players[j].pos.distance(from)));
        let velocity = look_dir(yaw, throw_elevation(pitch)) * speed + p.vel * PASS_INHERIT;
        (target.map(|(j, _)| j), origin, velocity)
    }

    /// The ball, once per physics step: follow the carrier and score, or fly,
    /// bounce, roll and get picked up; then the resets.
    pub(super) fn step_ball(&mut self, dt: f32) {
        if !self.ball.active || self.predicting { return; }
        self.out_of_bounds_damage(dt);
        self.step_bodies(dt);
        if self.ball.regroup > 0.0 {
            self.ball.regroup -= dt;
            if self.ball.regroup <= 0.0 { self.ball.regroup = 0.0; self.regroup_players(); }
        }
        if !self.ball.in_play {
            self.ball.pass_to = None;
            self.ball.respawn -= dt;
            if self.ball.respawn <= 0.0 { self.spawn_ball(); }
            return;
        }
        if let Some(c) = self.ball.carrier {
            self.ball.pass_to = None;
            let holder = self.players.get(c).filter(|p| p.alive);
            let Some(holder) = holder else { self.ball_lost(c); return; };
            let (pos, vel, team) = (holder.pos, holder.vel, holder.team);
            self.ball.pos = pos;
            self.ball.vel = vel;
            if pos.distance(self.end_zone(team.other())) < self.end_zone_radius() { self.touchdown(c); return; }
            self.body_fumble(c, dt);
            return;
        }
        let legs = self.fly_ball(dt);
        // A pass goes to someone only while it flies clear: not once it
        // strikes a surface (a second leg), rests, is reset or caught.
        if legs.len() > 1 || self.ball.vel == Vec3::ZERO || !self.ball.in_play { self.ball.pass_to = None; }
        if !self.ball.in_play { return; }
        self.pickup_along(&legs);
        if self.ball.carrier.is_some() { self.ball.pass_to = None; }
    }

    /// A dead Football player: their body stays down where it comes to
    /// rest, BODY_LIES from now.
    pub(super) fn body_down(&mut self, i: usize) {
        if !self.ball.active || self.predicting { return; }
        let pos = self.players[i].pos;
        self.ball.bodies.push(DownedBody { pos, life: BODY_LIES, body: Some(i) });
    }

    /// A Football player leaving: their body (if they lie dead) stops being
    /// followed and stays where it is.
    pub(crate) fn leaver_body(&mut self, i: usize) {
        for t in &mut self.ball.bodies { if t.body == Some(i) { t.body = None; } }
    }

    /// Bodies down: each follows its player while they stay dead (falling
    /// with their momentum, `sim::fall_body`), then stays where it lies
    /// until its time is up.
    fn step_bodies(&mut self, dt: f32) {
        for k in 0..self.ball.bodies.len() {
            let body = self.ball.bodies[k].body.and_then(|i| self.players.get(i)).filter(|p| !p.alive).map(|p| p.pos);
            let t = &mut self.ball.bodies[k];
            match body { Some(at) => t.pos = at, None => t.body = None }
            t.life -= dt;
        }
        self.ball.bodies.retain(|t| t.life > 0.0);
    }

    /// A carrier touching a body fumbles BODY_FUMBLE_CHANCE of the time for
    /// each 32 ms tick of contact (the chance, per step, of at least one in
    /// that much contact).
    fn body_fumble(&mut self, c: usize, dt: f32) {
        let pos = self.players[c].pos;
        if !self.ball.bodies.iter().any(|t| t.lying() && t.body != Some(c) && touch(pos, t.pos, t.pos).is_some()) { return; }
        let chance = 1.0 - (1.0 - BODY_FUMBLE_CHANCE).powf(dt / CLASSIC_TICK);
        if self.rng() >= chance { return; }
        let name = self.display_name(c);
        self.play(format!("{name} {} over a body and {}!", self.verb(c, "trips", "trip"), self.verb(c, "fumbles", "fumble")));
        self.fumble(c);
    }

    /// A loose ball, by the classic engine's item physics: gravity, no air
    /// drag, positions moved by the step's average velocity, and at each
    /// surface it touches `touch_surface` (the ball barely bounces and
    /// skids to a stop). Touching something with every velocity component
    /// under twice gravity's pull over a 32 ms tick, it comes to rest, and
    /// a ball at rest stays put until something moves it.
    /// Returns the step's flight, leg by leg between the surfaces it struck
    /// (`Leg`), for `pickup_along`.
    fn fly_ball(&mut self, dt: f32) -> Vec<Leg> {
        let map = self.map;
        let far = self.map_size() - 1.0;
        let gravity = crate::defs::movement().gravity;
        let b = &mut self.ball;
        b.no_catch = (b.no_catch - dt).max(0.0);
        let resting = b.vel == Vec3::ZERO && b.idle > 0.0;
        let mut legs = Vec::with_capacity(2);
        if resting { legs.push((b.pos, b.pos, b.vel)); }
        if !resting {
            let before = b.vel;
            b.vel.y -= gravity * dt;
            if crate::water::at(map, &self.staged_water, b.pos).is_some() {
                // Floats and drags in water.
                b.vel *= (-2.5 * dt).exp();
                b.vel.y += gravity * 1.4 * dt;
            }
            let mut motion = (before + b.vel) * 0.5 * dt;
            let mut touched = false;
            // The struck surface's own normal (`surface_hit`, marker
            // bounce1); the rest of the step goes on from there.
            for _ in 0..4 {
                let next = b.pos + motion;
                // Only a surface it moves into stops it (one it's leaving,
                // right after a bounce, still touches at the start).
                let hit = ball_hit(map, &self.pillars, b.pos, next, far).filter(|(_, n, _)| motion.dot(*n) < 0.0);
                let Some((t, normal, surface)) = hit else { legs.push((b.pos, next, b.vel)); b.pos = next; break };
                let at = b.pos.lerp(next, t) + normal * 0.002;
                legs.push((b.pos, at, b.vel));
                b.pos = at;
                touched |= touch_surface(&mut b.vel, normal, surface);
                motion = b.vel * dt * (1.0 - t);
            }
            // Never below the ground (a sweep that slipped through).
            let (ground, face) = crate::terrain::support_on(map, b.pos + Vec3::Y * 0.5);
            let ground = ground + BALL_RADIUS;
            if b.pos.y < ground && b.pos.y > ground - 2.0 {
                let below = b.pos;
                b.pos.y = ground;
                touched |= touch_surface(&mut b.vel, face, TURF);
                legs.push((below, b.pos, b.vel));
            }
            let rest = 2.0 * gravity * CLASSIC_TICK;
            if touched && b.vel.x.abs() < rest && b.vel.y.abs() < rest && b.vel.z.abs() < rest { b.vel = Vec3::ZERO; }
        }
        b.idle = if b.vel == Vec3::ZERO { b.idle + dt } else { 0.0 };
        let floor = crate::terrain::support_on(map, b.pos + Vec3::Y * 0.5).0 + BALL_RADIUS;
        let out = out_of_field(map, b.pos) || b.pos.x < 2.0 || b.pos.z < 2.0 || b.pos.x > far - 1.0 || b.pos.z > far - 1.0
            || b.pos.y < floor - 30.0 || !b.pos.is_finite();
        let idle = b.idle > IDLE_RESET;
        if out { self.loose_reset("The ball left the field and was reset", RESET_BALL_AFTER); }
        else if idle { self.loose_reset("The ball sat idle and was reset", 0.0); }
        legs
    }

    /// `pickup_along` for a ball that flew straight over the last `dt`
    /// (tests that place the ball).
    #[cfg(test)]
    fn pickup(&mut self, dt: f32) {
        let b = &self.ball;
        self.pickup_along(&[(b.pos - b.vel * dt, b.pos, b.vel)]);
    }

    /// A player picks the ball up when it touches their body (the collision
    /// box, as in `bump`, grown 3%: `catch_touch`), so a catch needs the ball
    /// to reach you, and a hitter a metre away doesn't scoop up a fumble. Swept along the step's flight,
    /// leg by leg (`fly_ball`), so a fast pass can't skip through a receiver
    /// between ticks, and a ball that reaches you before it strikes the turf
    /// in the same step is caught at the speed it had then.
    fn pickup_along(&mut self, legs: &[Leg]) {
        let b = &self.ball;
        let best = legs.iter().find_map(|&(from, to, vel)| self.players.iter().enumerate()
            .filter(|(j, p)| p.alive && p.stun <= 0.0 && !(b.no_catch > 0.0 && b.thrown_by == Some(*j)))
            .filter_map(|(j, p)| catch_touch(p.pos, from, to).map(|d| (j, d)))
            .min_by(|a, c| a.1.total_cmp(&c.1))
            .map(|(j, _)| (j, vel)));
        let Some((j, vel)) = best else { return };
        let team = self.players[j].team;
        let name = self.display_name(j);
        // A ball nobody threw, a dropped one (a fumble) or one barely moving
        // (under 1.1 m/s) is picked up; a moving pass from an enemy is
        // intercepted; anything else is caught.
        let speed = vel.length();
        let owner = self.ball.thrown_by.filter(|&k| k < self.players.len()).map(|k| self.players[k].team);
        let (text, event) = match owner {
            None => (format!("{name} {} the ball", self.verb(j, "picks up", "pick up")), "catch"),
            Some(_) if speed < 1.1 || self.ball.dropped => (format!("{name} {} the ball", self.verb(j, "picks up", "pick up")), "catch"),
            Some(t) if t != team => (format!("{name} {} the ball!", self.verb(j, "intercepts", "intercept")), "intercept"),
            Some(_) => (format!("{name} {} the ball", self.verb(j, "catches", "catch")), "catch"),
        };
        // The catch pushes the catcher along the heading the ball left with
        // (`Ball::facing`: its thrower's facing, whichever way it flies now):
        // an impulse of 2 x ball speed x 6.5 / mass, plus 1 up, on a mass-9
        // armor (the classic's light and carrier armors both weigh 9).
        let heading = look_dir(self.ball.facing, 0.0);
        self.players[j].vel += heading * (2.0 * speed * 6.5 / 9.0 / 9.0) + Vec3::Y * (1.0 / 9.0);
        let b = &mut self.ball;
        b.carrier = Some(j);
        b.thrown_by = None;
        b.dropped = false;
        b.last_team = Some(team);
        b.idle = 0.0;
        self.bots_look_up();
        self.play(text);
        // Your own catch chimes like a flag pickup; your team's interception
        // like a return; anyone else's is a plain catch.
        let mine = self.players.get(self.player_id).map(|p| p.team);
        self.push_event(if j == self.player_id { "flag" } else if event == "intercept" && mine == Some(team) { "return" } else { "catch" });
    }

    fn touchdown(&mut self, c: usize) {
        let team = self.players[c].team;
        self.dead_ball(NEW_BALL_AFTER, team.other());
        self.ball.last_team = Some(team);
        self.ball.regroup = REGROUP_AFTER;
        self.rule_event(super::rules::GameEvent::Touchdown { player: c });
    }

    /// The football consequences of a hit between enemies `a` and `b`, with
    /// their velocities just before it (the bodies have already swapped them).
    /// Speeds are rounded to whole m/s; the slower player loses, and on a tie
    /// both do. A losing carrier fumbles when
    /// the closing speed is 2-14; at 15 or more the loser is tackled. Nobody is
    /// tackled while no ball is in play.
    pub(super) fn football_hit(&mut self, a: usize, b: usize, va: Vec3, vb: Vec3) {
        let closing = (va - vb).length().round();
        let (sa, sb) = (va.length().round(), vb.length().round());
        let losers: &[(usize, usize)] = if sa > sb { &[(b, a)] } else if sb > sa { &[(a, b)] } else { &[(a, b), (b, a)] };
        for &(loser, winner) in losers {
            if !self.players[loser].alive { continue; }
            if self.ball.carried_by(loser) && (FUMBLE_SPEED..TACKLE_SPEED).contains(&closing) {
                self.knock_loose(loser, winner);
            }
            if closing >= TACKLE_SPEED {
                if !self.ball.in_play { return; }
                self.tackle(loser, winner);
            }
        }
    }

    fn knock_loose(&mut self, carrier: usize, by: usize) {
        self.fumble(carrier);
        let (w, l) = (self.display_name(by), self.object_name(carrier));
        self.play(format!("{w} {} the ball loose from {l}", self.verb(by, "knocks", "knock")));
        // A grunt when it's your ball that comes loose.
        self.push_event(if carrier == self.player_id { "pain" } else { "fumble" });
    }

    fn tackle(&mut self, loser: usize, winner: usize) {
        if !self.players[loser].alive { return; }
        if self.ball.carried_by(loser) {
            self.fumble(loser);
            self.rule_event(super::rules::GameEvent::CarrierTackled { winner, loser });
        }
        let (w, l) = (self.display_name(winner), self.object_name(loser));
        self.play(format!("{w} {} {l}", self.verb(winner, "tackles", "tackle")));
        self.players[loser].stun = TACKLE_STUN;
        self.players[loser].health -= TACKLE_DAMAGE;
        if loser == self.player_id { self.trauma = (self.trauma + 0.6).min(1.0); }
        self.push_event("tackle");
        if self.players[loser].health <= 0.0 { self.kill(loser, Some(winner), "Tackle"); }
    }

    /// Whether `team`'s end zone stands well above its kickoff floor.
    pub fn raised_goal(&self, team: Team) -> bool {
        self.end_zone(team).y - self.kickoff_spot(team).y > 4.0
    }

    /// Where a bot carrier starts its run-up to leap onto a raised goal: on
    /// the floor LEAP_RUNUP out from the zone, on the field side.
    pub(super) fn leap_spot(&self, zone: Vec3) -> Vec3 {
        let mid = (self.end_zone(Team::Ember) + self.end_zone(Team::Glacier)) * 0.5;
        let away = Vec3::new(mid.x - zone.x, 0.0, mid.z - zone.z).normalize_or(Vec3::Z);
        let spot = zone + away * LEAP_RUNUP;
        let floor = crate::terrain::support_on(self.map, Vec3::new(spot.x, self.kickoff_spot(Team::Ember).y + 2.0, spot.z)).0;
        Vec3::new(spot.x, floor + PLAYER_RADIUS, spot.z)
    }

    /// A bot carrier's inputs near a raised goal (there is no air control): walk in without jetting from LEAP_APPROACH, and with
    /// enough energy run straight at the goal from the field side (else wait
    /// at the run-up spot for it), jump inside the launch window, then with the keys let go
    /// (held keys would turn the jet sideways) jet only while the arc would
    /// cross the platform's edge too low. Momentum carries it onto the zone.
    /// The leap ends on landing, losing the ball, or after 3.5 s. `None`
    /// leaves the bot to its route.
    pub(super) fn bot_leap_inputs(&mut self, i: usize, dt: f32) -> Option<(f32, f32, bool, bool, bool, bool)> {
        let p = &self.players[i];
        if !p.is_bot || !self.football() { return None; }
        let team = p.team;
        if !self.ball.carried_by(i) || !self.raised_goal(team.other()) {
            self.players[i].bot_leap = 0.0;
            return None;
        }
        let zone = self.end_zone(team.other());
        let to = zone - p.pos;
        let flat = Vec2::new(to.x, to.z).length();
        let toward = Vec2::new(p.vel.x, p.vel.z).dot(Vec2::new(to.x, to.z) / flat.max(0.1));
        let face_zone = (-to.x).atan2(-to.z);
        let t = p.bot_leap;
        if t > 0.0 {
            // In flight.
            if t > 3.5 || (t > 0.3 && p.on_ground) { self.players[i].bot_leap = 0.0; return None; }
            let to_edge = flat - LEAP_EDGE;
            let need = zone.y - 1.2 + PLAYER_RADIUS + 0.4;
            let jet = to_edge > 0.0 && {
                let te = to_edge / toward.max(1.0);
                p.pos.y + p.vel.y * te - 0.5 * crate::defs::movement().gravity * te * te < need
            };
            let p = &mut self.players[i];
            p.bot_leap += dt;
            p.yaw = face_zone;
            return Some((0.0, 0.0, false, false, false, jet));
        }
        let spot = self.leap_spot(zone);
        let to_spot = spot - p.pos;
        let spot_flat = Vec2::new(to_spot.x, to_spot.z).length();
        if t < 0.0 {
            // Running in: launch inside the window, give up if it's missed.
            if !p.on_ground || flat < LEAP_LAUNCH.start || flat > LEAP_RUNUP + 3.0 {
                self.players[i].bot_leap = 0.0;
            } else if LEAP_LAUNCH.contains(&flat) && toward > 6.0 {
                let p = &mut self.players[i];
                p.bot_leap = dt * 0.5;
                p.yaw = face_zone;
                return Some((0.0, 1.0, true, true, false, true));
            } else {
                self.players[i].yaw = face_zone;
                return Some((0.0, 1.0, false, false, false, false));
            }
        }
        // From LEAP_APPROACH out, walk in without the jet, so the tank fills
        // on the way instead of at the spot.
        if flat > LEAP_APPROACH && spot_flat > 4.0 { return None; }
        // Already coming in from the field side with the jet for it: run
        // straight in. Standing still to wait gets you tackled.
        let p = &self.players[i];
        let from_field = (spot - zone).normalize_or_zero().dot((p.pos - zone).normalize_or_zero()) > 0.8;
        let ready = p.on_ground && p.energy >= LEAP_ENERGY;
        let p = &mut self.players[i];
        if ready && from_field && flat > LEAP_LAUNCH.end {
            p.bot_leap = -dt;
            p.yaw = face_zone;
            return Some((0.0, 1.0, false, false, false, false));
        }
        if spot_flat > 1.0 {
            p.yaw = (-to_spot.x).atan2(-to_spot.z);
            return Some((0.0, 1.0, false, false, false, false));
        }
        // At the spot: face the goal and wait for the jet to recharge.
        p.yaw = face_zone;
        if ready { p.bot_leap = -dt; }
        Some((0.0, 0.0, false, false, false, false))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> Match {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::Football);
        let a = m.join(1, "Ann").unwrap();
        let b = m.join(2, "Bob").unwrap();
        assert_ne!(m.world.players[a].team, m.world.players[b].team);
        for _ in 0..((WARMUP_SECONDS + 1.0) / STEP) as usize { m.step(&[]); }
        assert_eq!(m.phase, Phase::Playing);
        m
    }

    fn run(m: &mut Match, seconds: f32) { for _ in 0..(seconds / STEP) as usize { m.step(&[]); } }

    /// A body coasts on the same way: on the turf it keeps its speed along
    /// the ground, and in the air it falls and lands on the floor.
    #[test]
    fn bodies_coast_along_the_ground_and_fall_through_the_air() {
        let map = MapId::Longfield;
        let floor = crate::terrain::support_on(map, Vec3::new(1024.0, 120.0, 1000.0)).0 + PLAYER_RADIUS;
        let run = Vec3::new(3.0, 0.0, 12.0);
        let (at, vel) = coast(map, Vec3::new(1024.0, floor, 1000.0), run, 0.1);
        assert!(at.distance(Vec3::new(1024.3, floor, 1001.2)) < 0.02 && vel.distance(run) < 1e-4, "{at} {vel}");
        let (at, vel) = coast(map, Vec3::new(1024.0, floor + 4.0, 1000.0), Vec3::new(0.0, 5.0, 10.0), 0.2);
        let g = crate::defs::movement().gravity;
        assert!((at.y - (floor + 4.0 + 1.0 - 0.02 * g)).abs() < 1e-3 && (vel.y - (5.0 - 0.2 * g)).abs() < 1e-3, "rising, then falling: {at} {vel}");
        let (at, vel) = coast(map, Vec3::new(1024.0, floor + 0.5, 1000.0), Vec3::new(0.0, -8.0, 10.0), 0.3);
        assert!((at.y - floor).abs() < 0.02 && vel.y == 0.0, "landed on the floor: {at} {vel}");
        assert_eq!(coast(map, Vec3::ONE, run, 0.0), (Vec3::ONE, run));
    }

    /// Catches reach 3% past the body every way (0.824 m across, 0.3435
    /// below the feet and over the top); tripping on a body doesn't.
    #[test]
    fn catches_reach_three_percent_past_the_body() {
        let body = Vec3::new(0.0, PLAYER_RADIUS, 0.0);
        let at = |x: f32, y: f32| Vec3::new(x, y, 0.0);
        let caught = |x: f32, y: f32| catch_touch(body, at(x, y), at(x, y)).is_some();
        let touched = |x: f32, y: f32| touch(body, at(x, y), at(x, y)).is_some();
        assert!(caught(0.82, 1.0) && !touched(0.82, 1.0) && !caught(0.83, 1.0), "across");
        assert!(caught(0.0, -0.34) && !touched(0.0, -0.34) && !caught(0.0, -0.35), "below the feet");
        assert!(caught(0.0, 2.64) && !touched(0.0, 2.64) && !caught(0.0, 2.65), "over the top");
        assert!(touched(0.79, 1.0) && touched(0.0, -0.29) && touched(0.0, 2.59), "the body's own reach");
    }

    #[test]
    fn a_pass_lands_about_as_far_as_the_receiver() {
        for d in [10.0f32, 30.0, 60.0] {
            let v = pass_speed(d);
            let range = v * v * (2.0 * throw_elevation(0.0)).sin() / crate::defs::movement().gravity;
            assert!((range - d).abs() < d * 0.05, "{d} m pass lands at {range:.1} m");
        }
    }

    #[test]
    fn a_ball_appears_is_picked_up_and_scores_a_touchdown() {
        let mut m = field();
        run(&mut m, 3.0);
        assert!(m.world.ball.in_play, "the first ball appears after a short delay");
        let ball = m.world.ball.pos;
        let side = if m.world.ball.next_side == 0 { Team::Ember } else { Team::Glacier };
        // Dropped from the sky, it may have bounced a few metres on a slope.
        assert!(ball.distance(m.world.kickoff_spot(side)) < 4.0);
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        m.world.players[a].pos = ball;
        m.world.players[a].vel = Vec3::ZERO;
        m.step(&[]);
        assert_eq!(m.world.ball.carrier, Some(a));
        assert!(m.world.feed.iter().any(|e| e.line().contains("picks up the ball")));
        let team = m.world.players[a].team;
        m.world.players[a].pos = m.world.end_zone(team.other());
        m.step(&[]);
        assert_eq!(m.world.score[team.idx()], 1);
        assert!(!m.world.ball.in_play, "a touchdown ends the play");
        assert!(m.world.players[a].frags >= TOUCHDOWN_POINTS);
        run(&mut m, REGROUP_AFTER + 0.1);
        assert!(m.world.players[a].pos.distance(m.world.end_zone(team.other())) > END_ZONE_RADIUS,
            "everyone regroups at spawn");
        run(&mut m, NEW_BALL_AFTER - REGROUP_AFTER);
        assert!(m.world.ball.in_play);
        assert_eq!(m.world.ball.next_side, team.other().idx() as u8, "the conceding side gets the ball");
    }

    /// A defender at `defender` m/s runs into a carrier moving at `carrier`
    /// m/s toward them. Returns (ball loose, carrier down, defender down).
    fn hit(defender: f32, carrier: f32) -> (bool, bool, bool) {
        let mut m = field();
        run(&mut m, 3.0);
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        let b = m.world.players.iter().position(|p| p.net_id == 2).unwrap();
        let spot = m.world.ball.pos;
        m.world.players[a].pos = spot;
        m.step(&[]);
        assert!(m.world.ball.carried_by(a));
        let at = m.world.players[a].pos;
        m.world.players[a].vel = Vec3::new(carrier, 0.0, 0.0);
        m.world.players[b].pos = at + Vec3::new(0.9, 0.0, 0.0);
        m.world.players[b].vel = Vec3::new(-defender, 0.0, 0.0);
        m.world.step_bumps(&m.world.players.iter().map(|p| p.pos).collect::<Vec<_>>());
        (!m.world.ball.carried_by(a), m.world.players[a].stun > 0.0, m.world.players[b].stun > 0.0)
    }

    #[test]
    fn hits_follow_the_rule_the_slower_player_loses() {
        assert_eq!(hit(1.0, 0.0), (false, false, false), "closing 1: a bump");
        assert_eq!(hit(8.0, 0.0), (true, false, false), "closing 8: the slower carrier fumbles");
        assert_eq!(hit(14.4, 0.0), (true, false, false), "closing 14.4 rounds to 14: fumble");
        assert_eq!(hit(14.6, 0.0), (true, true, false), "closing 14.6 rounds to 15: tackle");
        assert_eq!(hit(24.0, 0.0), (true, true, false), "hard hit: the carrier goes down");
        // The faster player wins whoever carries: a carrier running into a
        // slower defender tackles them and keeps the ball.
        assert_eq!(hit(3.0, 16.0), (false, false, true), "the faster carrier wins");
        // Equal speeds (rounded): both lose, both go down.
        assert_eq!(hit(10.2, 9.8), (true, true, true), "a tie takes both down");
    }

    #[test]
    fn a_tackled_player_cannot_move_until_they_get_up() {
        let mut m = field();
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        m.world.players[a].stun = TACKLE_STUN;
        let before = m.world.players[a].pos;
        let push = Command { seq: 1, move_z: 1.0, ..Default::default() };
        let mut commands = vec![None; MAX_PLAYERS];
        for s in 0..(TACKLE_STUN * 0.8 / STEP) as u64 {
            commands[a] = Some(Command { seq: s + 1, ..push });
            m.step(&commands);
        }
        let flat = Vec2::new(m.world.players[a].pos.x - before.x, m.world.players[a].pos.z - before.z);
        assert!(flat.length() < 0.5, "moved {} m while down", flat.length());
        for s in 0..(1.0 / STEP) as u64 {
            commands[a] = Some(Command { seq: 1000 + s, ..push });
            m.step(&commands);
        }
        assert_eq!(m.world.players[a].stun, 0.0);
    }

    #[test]
    fn passes_are_caught_by_teammates_and_intercepted_by_enemies() {
        let mut m = field();
        let c = m.join(3, "Cat").unwrap();
        run(&mut m, 4.0);
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        let (team, spot) = (m.world.players[a].team, m.world.ball.pos);
        m.world.players[a].pos = spot;
        m.step(&[]);
        assert!(m.world.ball.carried_by(a));
        let receiver = if m.world.players[c].team == team { c } else {
            m.world.players.iter().position(|p| p.net_id == 2).unwrap()
        };
        let mate = m.world.players[receiver].team == team;
        // Throw at a receiver 20 m ahead, looking at them: teammate or
        // enemy, the player under the crosshair sets the power.
        let throw_from = m.world.players[a].pos;
        m.world.players[receiver].pos = throw_from + Vec3::new(0.0, 0.0, -20.0);
        m.world.players[receiver].pos.y = crate::terrain::support_on(MapId::OldHoller, m.world.players[receiver].pos + Vec3::Y * 3.0).0 + 1.2;
        m.world.players[a].yaw = 0.0;
        let to = m.world.players[receiver].pos + Vec3::Y * 0.4 - (throw_from + Vec3::Y * EYE);
        m.world.players[a].pitch = (to.y / to.length()).asin();
        m.world.pass(a);
        assert!(!m.world.ball.carried_by(a));
        assert!(m.world.feed.iter().any(|e| e.line().contains("passes toward")));
        for _ in 0..(3.0 / STEP) as usize {
            m.world.players[receiver].vel = Vec3::ZERO;
            m.world.players[a].pos = throw_from + Vec3::new(30.0, 0.0, 0.0);
            m.step(&[]);
            if m.world.ball.carrier.is_some() { break; }
        }
        let caught = m.world.ball.carrier;
        let word = if mate { "catches" } else { "intercepts" };
        let lines: Vec<String> = m.world.feed.iter().map(|e| e.line()).collect();
        assert!(lines.iter().any(|l| l.contains(word)), "{word}: carrier {caught:?} {lines:?}");
    }

    #[test]
    fn half_time_swaps_ends_and_gives_the_ball_to_the_side_that_did_not_have_it() {
        let mut m = field();
        run(&mut m, 4.0);
        // Ann has the ball when the half ends.
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        let team = m.world.players[a].team;
        m.world.players[a].pos = m.world.ball.pos;
        m.step(&[]);
        assert!(m.world.ball.carried_by(a));
        let (own, theirs) = (m.world.end_zone(team), m.world.end_zone(team.other()));
        m.world.time_left = HALF_SECONDS + STEP * 0.5;
        m.step(&[]);
        assert_eq!(m.world.ball.half, 1);
        assert!(!m.world.ball.in_play && m.world.ball.carrier.is_none());
        // The ends swap at once, and the second half gets the break back.
        assert_eq!((m.world.end_zone(team), m.world.end_zone(team.other())), (theirs, own));
        assert!(m.world.time_left > HALF_SECONDS + HALFTIME_BREAK - 0.1, "{}", m.world.time_left);
        // Everyone goes to their new end 3 s before the break ends.
        run(&mut m, HALFTIME_BREAK - HALFTIME_REGROUP_BEFORE - 0.2);
        assert!(m.world.ball.regroup > 0.0, "nobody sent back yet");
        run(&mut m, 0.4);
        assert_eq!(m.world.ball.regroup, 0.0);
        let at = m.world.players[a].pos;
        assert!(at.distance(theirs) < at.distance(own), "respawned at the new end: {at} (defends {theirs}, was {own})");
        // The next ball goes to the side that didn't have it.
        run(&mut m, HALFTIME_REGROUP_BEFORE);
        assert!(m.world.ball.in_play);
        assert_eq!(m.world.ball.next_side, team.other().idx() as u8);
    }

    #[test]
    fn a_new_ball_drops_in_from_the_sky() {
        let mut m = field();
        while !m.world.ball.in_play { m.step(&[]); }
        let spot = m.world.kickoff_spot(if m.world.ball.next_side == 0 { Team::Ember } else { Team::Glacier });
        assert!(m.world.ball.pos.y > spot.y + BALL_DROP - 1.0, "starts high: {} over {}", m.world.ball.pos.y, spot.y);
        run(&mut m, 4.0);
        assert!(m.world.ball.carrier.is_some() || m.world.ball.pos.y < spot.y + 2.0, "and lands");
    }

    #[test]
    fn an_idle_ball_is_reset_and_other_modes_have_none() {
        let mut m = field();
        run(&mut m, 3.0);
        assert!(m.world.ball.in_play);
        // A new ball falls BALL_DROP first, then has to sit still.
        run(&mut m, IDLE_RESET + 5.0);
        assert!(m.world.feed.iter().any(|e| e.line().contains("sat idle")));
        let mut ctf = Match::new(MapId::OldHoller);
        ctf.join(1, "Ann").unwrap();
        ctf.join(2, "Bob").unwrap();
        run(&mut ctf, 5.0);
        assert!(!ctf.world.ball.active);
    }

    #[test]
    fn football_state_survives_the_wire() {
        let mut m = field();
        run(&mut m, 3.0);
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        m.world.players[a].pos = m.world.ball.pos;
        m.step(&[]);
        m.world.players[a].stun = 1.25;
        let text = serde_json::to_string(&m.snapshot()).unwrap();
        let back: Snapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(back.ball, m.world.ball);
        assert_eq!(back.players[a].stun, 1.25);
        assert_eq!(back.mode, SupportedMode::Football);
        let mut client = World::new();
        assert!(client.apply_snapshot(&back, 1));
        assert!(client.ball.carried_by(a));
    }

    #[test]
    fn longfield_declares_a_valid_field_with_grounded_spawns() {
        assert!(has_field(MapId::Longfield));
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
            assert!(!has_field(map), "{map:?} is a CTF map");
        }
        let f = super::field(MapId::Longfield).unwrap();
        let apart = Vec3::from(f.end_zones[0]).distance(Vec3::from(f.end_zones[1]));
        assert!((240.0..=260.0).contains(&apart), "end zones {apart} m apart");
        for t in 0..2 {
            for p in [f.end_zones[t], f.kickoff[t]] {
                let floor = crate::terrain::support_on(MapId::Longfield, Vec3::from(p) + Vec3::Y * 2.0).0;
                assert!((Vec3::from(p).y - floor - 1.1).abs() < 0.15, "{p:?} sits over the turf at {floor}");
            }
        }
        for (team, points) in crate::map_pack::on(MapId::Longfield).unwrap().manifest.spawn_points.iter().enumerate() {
            assert_eq!(points.len(), 8);
            for p in points {
                let pos = Vec3::new(p[0], p[1], p[2]);
                let floor = crate::terrain::support_on(MapId::Longfield, pos).0;
                assert!((pos.y - floor - 1.2).abs() < 0.02, "team {team} spawn {pos} floor {floor}");
                // Spawns face up the field, toward the other end zone.
                let ahead = look_dir(p[3], 0.0);
                assert!(ahead.dot(Vec3::from(f.end_zones[1 - team]) - pos) > 0.0);
            }
        }
    }

    /// Offline the player at the screen has no name and is "You": the
    /// lines about them read as English ("You pick up the ball", "Ace
    /// tackles you"), and everyone else's as before.
    #[test]
    fn plays_about_you_read_as_english() {
        let mut w = World::new();
        w.set_map(MapId::Longfield);
        w.set_mode(SupportedMode::Football);
        w.start_match(true);
        let me = w.player_id;
        w.players[me].name.clear();
        let (a, b) = ((me + 1) % w.players.len(), (me + 2) % w.players.len());
        w.players[a].name = "Ace".into();
        w.players[b].name = "Bo".into();
        let last = |w: &World| w.feed.iter().rev().find_map(|e| match e { Entry::Play { text } => Some(text.clone()), _ => None }).unwrap();
        w.tackle(me, a);
        assert_eq!(last(&w), "Ace tackles you");
        w.players[me].stun = 0.0; w.players[me].health = 100.0;
        w.tackle(a, me);
        assert_eq!(last(&w), "You tackle Ace");
        w.tackle(b, a);
        assert_eq!(last(&w), "Ace tackles Bo", "others' lines unchanged");
        assert_eq!((w.verb(me, "picks up", "pick up"), w.verb(a, "picks up", "pick up")), ("pick up", "picks up"));
        // A named local player (online, or a set name) is named, not "you".
        w.players[me].name = "Cat".into();
        w.tackle(me, a);
        assert_eq!(last(&w), "Ace tackles Cat");
    }

    #[test]
    fn bots_carry_pass_tackle_and_score_on_longfield() {
        let mut w = World::new();
        w.set_map(MapId::Longfield);
        w.set_mode(SupportedMode::Football);
        w.start_match(true);
        assert!(w.players.len() >= 6, "a full bot match");
        let (mut carries, mut tackles) = (0, 0);
        let mut carrier = None;
        let mut down = vec![false; w.players.len()];
        for _ in 0..(240.0 / STEP) as usize {
            if w.state != MatchState::Playing { break; }
            w.physics_step();
            if w.ball.carrier != carrier { if w.ball.carrier.is_some() { carries += 1; } carrier = w.ball.carrier; }
            for (d, p) in down.iter_mut().zip(&w.players) { if p.stun > 0.0 && !*d { tackles += 1; } *d = p.stun > 0.0; }
        }
        let touchdowns = w.score[0] + w.score[1];
        println!("{} players: {carries} carries, {tackles} tackles, score {:?}, {:.0} s left, state {:?}", w.players.len(), w.score, w.time_left, w.state);
        assert!(carries >= 4 && touchdowns >= 1 && tackles >= 1, "{carries} carries, {tackles} tackles, {touchdowns} touchdowns in 4 min");
    }

    /// Peak rise of the body centre on flat turf with a full tank, and how
    /// long a full tank of jet lasts: after settling, `jump_at` is when (s)
    /// jump is tapped (None = never), jet held from `jet_at`, running forward
    /// at `run` m/s first.
    pub(crate) fn peak_rise(mode: SupportedMode, jump_at: Option<f32>, jet_at: f32, run: f32) -> (f32, f32) {
        let mut w = World::new();
        w.set_map(MapId::Longfield);
        w.set_mode(mode);
        w.start_match(true);
        w.players.truncate(1);
        let p = &mut w.players[0];
        p.pos = Vec3::new(1024.0, 101.2, 1000.0); p.yaw = 0.0;
        w.input.move_z = if run > 0.0 { 1.0 } else { 0.0 };
        for _ in 0..(1.5 / STEP) as usize { w.physics_step(); }
        let start = w.players[0].pos;
        w.players[0].energy = crate::defs::movement().energy_max;
        let (mut top, mut empty) = (0.0f32, None);
        for k in 0..(6.0 / STEP) as usize {
            let t = k as f32 * STEP;
            w.input.jump = jump_at.is_some_and(|j| t >= j && t < j + 0.05);
            w.input.jet = t >= jet_at;
            w.physics_step();
            let p = &w.players[0];
            top = top.max(p.pos.y - start.y);
            if empty.is_none() && t >= jet_at && !p.jetting { empty = Some(t - jet_at); }
        }
        (top, empty.unwrap_or(f32::INFINITY))
    }

    #[test]
    fn football_jet_is_a_short_hard_burst_that_turns_sideways_in_the_air() {
        let standing = peak_rise(SupportedMode::Football, None, 0.0, 0.0);
        let together = peak_rise(SupportedMode::Football, Some(0.0), 0.0, 0.0);
        let apex = peak_rise(SupportedMode::Football, Some(0.0), 0.5, 0.0);
        let running = peak_rise(SupportedMode::Football, Some(0.0), 0.0, 11.0);
        let hop = peak_rise(SupportedMode::Football, Some(0.0), 99.0, 0.0);
        let ctf = peak_rise(SupportedMode::Ctf, Some(0.0), 0.0, 0.0);
        println!("football: hop {:.2} m, standing jet {:.2} m ({:.2} s burst), jump+jet {:.2}, jet at jump apex {:.2}, \
            running jump+jet {:.2}; ctf jump+jet {:.2} m", hop.0, standing.0, standing.1, together.0, apex.0, running.0, ctf.0);
        // The numbers: a 5.56 m/s jump rises 0.77 m; 44.4 m/s² of jet
        // empties a 60 tank in 60 / (125 - 8) = 0.51 s.
        assert!((0.72..0.82).contains(&hop.0), "hop {}", hop.0);
        assert!((0.47..0.56).contains(&standing.1), "a full tank lasts about half a second: {}", standing.1);
        // Holding the jet on: 12.5 m/s at burnout, about 7 m, and the empty
        // tank's sputter adds only what the recharge pays for.
        assert!((7.0..8.0).contains(&standing.0), "jet alone reaches {}", standing.0);
        assert!(apex.0 < standing.0 && together.0 > 2.0 * RAISED_GOAL_HEIGHT, "jump+jet {}, late jet {}", together.0, apex.0);
        // Keys held in the air turn 80% of the jet sideways: less lift than gravity.
        assert!(running.0 < 2.0, "running jump+jet with the key held reaches {}", running.0);
        assert!(ctf.0 > 2.0 * together.0, "CTF armor is unchanged and far stronger");
    }

    /// Horizontal speed after `secs` on Longfield's flat turf under `mode`
    /// and the given keys, from a standstill.
    fn run_speed(carrier: bool, move_x: f32, move_z: f32, secs: f32) -> f32 {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        w.players[0].pos = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1024.0);
        if carrier { w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() }; }
        for _ in 0..(0.2 / STEP) as usize { w.physics_step(); }
        (w.input.move_x, w.input.move_z) = (move_x, move_z);
        for _ in 0..(secs / STEP) as usize { w.physics_step(); }
        Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length()
    }

    #[test]
    fn holding_jet_jump_and_a_key_levels_off_instead_of_speeding_up_forever() {
        // Hop-skiing with the jet and W held on Longfield's flat turf: one
        // tank's boost, then the sputter can only spend the recharge.
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let start = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1100.0);
        w.players[0].pos = start;
        for _ in 0..30 { w.physics_step(); }
        (w.input.move_z, w.input.jump, w.input.jet) = (1.0, true, true);
        let mut speeds = Vec::new();
        for k in 1..=(30.0 / STEP) as usize {
            w.physics_step();
            if k % (5.0 / STEP) as usize == 0 {
                let p = &mut w.players[0];
                speeds.push(Vec2::new(p.vel.x, p.vel.z).length());
                p.pos.z = start.z; // stay on the field
            }
        }
        println!("speed every 5 s: {speeds:?}");
        assert!(speeds.iter().all(|&v| v < 24.0), "no endless speed: {speeds:?}");
        assert!(speeds[5] < speeds[1] + 1.5, "it levels off: {speeds:?}");
    }

    #[test]
    fn classic_run_speeds_carrier_armor_and_ground_friction() {
        assert!((run_speed(false, 0.0, 1.0, 1.0) - RUN_FORWARD).abs() < 0.05);
        assert!((run_speed(false, 0.0, -1.0, 1.0) - RUN_SIDE).abs() < 0.05);
        assert!((run_speed(false, 1.0, 0.0, 1.0) - RUN_SIDE).abs() < 0.05);
        // Diagonals are capped at the forward speed.
        assert!((run_speed(false, 1.0, 1.0, 1.0) - RUN_FORWARD).abs() < 0.05);
        assert!((run_speed(true, 0.0, 1.0, 1.0) - CARRIER_FORWARD).abs() < 0.05);
        assert!((run_speed(true, 1.0, 0.0, 1.0) - CARRIER_SIDE).abs() < 0.05);
        // 40 m/s² of friction: 11 m/s from a standstill in about 0.28 s.
        let early = run_speed(false, 0.0, 1.0, 0.15);
        assert!((5.0..7.0).contains(&early), "after 0.15 s: {early}");
    }

    #[test]
    fn classic_air_has_no_control_but_held_keys_turn_the_jet() {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let start = |w: &mut World| {
            let p = &mut w.players[0];
            p.pos = Vec3::new(1024.0, 140.0, 1024.0); p.vel = Vec3::ZERO; p.yaw = 0.0;
            p.on_ground = false; p.coyote = 0.0; p.energy = crate::defs::movement().energy_max;
        };
        start(&mut w);
        w.input.move_z = 1.0;
        for _ in 0..30 { w.physics_step(); }
        assert!(Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length() < 1e-4, "no air control");
        start(&mut w);
        w.input.jet = true;
        for _ in 0..12 { w.physics_step(); }
        let v = w.players[0].vel;
        // 0.2 s: 35.5 m/s² forward and 8.9 - 20 up.
        assert!((-v.z - 7.1).abs() < 0.2 && (v.y + 2.2).abs() < 0.2, "side jet {v}");
        // Pressing against your motion always gets the full side share.
        start(&mut w);
        w.players[0].vel = Vec3::new(0.0, 0.0, 30.0);
        for _ in 0..12 { w.physics_step(); }
        assert!((w.players[0].vel.z - (30.0 - 7.1)).abs() < 0.2, "braking jet {}", w.players[0].vel);
    }

    /// A carrier runs at the Highgoal platform from 20 m out and jumps
    /// `launch` metres from the zone's centre. With the keys let go, they jet
    /// only while their arc would cross the platform's edge less than `margin`
    /// above its top (the bots' rule). Returns whether they scored.
    fn running_leap(launch: f32, margin: f32) -> bool {
        let mut w = World::new();
        w.set_map(MapId::Highgoal);
        w.set_mode(SupportedMode::Football);
        w.start_match(true);
        w.players.truncate(1);
        let zone = w.end_zone(Team::Glacier);
        let floor = crate::terrain::support_on(MapId::Highgoal, Vec3::new(zone.x, 105.0, zone.z - 20.0)).0;
        let p = &mut w.players[0];
        p.pos = Vec3::new(zone.x, floor + PLAYER_RADIUS, zone.z - 20.0);
        p.yaw = std::f32::consts::PI;
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        for _ in 0..(0.3 / STEP) as usize { w.physics_step(); }
        w.players[0].energy = crate::defs::movement().energy_max;
        let mut launched = false;
        for _ in 0..(6.0 / STEP) as usize {
            let p = &w.players[0];
            let to_edge = zone.z - p.pos.z - LEAP_EDGE;
            if !launched && zone.z - p.pos.z <= launch {
                launched = true;
                (w.input.jump, w.input.jet, w.input.move_z) = (true, true, 1.0);
            } else if launched {
                let te = to_edge / p.vel.z.max(1.0);
                let rise = p.pos.y + p.vel.y * te - 0.5 * crate::defs::movement().gravity * te * te - floor - PLAYER_RADIUS;
                (w.input.jump, w.input.jet, w.input.move_z) = (false, to_edge > 0.0 && rise < RAISED_GOAL_HEIGHT + margin, 0.0);
            } else {
                w.input.move_z = 1.0;
            }
            w.physics_step();
            if w.score[0] > 0 { return true; }
        }
        false
    }

    #[test]
    fn a_running_leap_with_a_feathered_jet_lands_on_the_raised_goal() {
        // No air control: the run carries you over the edge, and the jet is
        // only for height. Too close and the edge is in the way.
        let launches: Vec<f32> = (14..=36).map(|d| d as f32 * 0.5).collect();
        for margin in [0.3f32, 1.0] {
            let hits: Vec<f32> = launches.iter().copied().filter(|&d| running_leap(d, margin)).collect();
            println!("margin {margin}: scores launching {hits:?} m from the centre");
            assert!([LEAP_LAUNCH.start, 11.0, LEAP_LAUNCH.end - 0.5].iter().all(|d| hits.contains(d)),
                "the bots' launch window must score: {hits:?}");
            assert!(hits.len() >= 10, "a run gives plenty of room: {hits:?}");
        }
        assert!(!running_leap(4.8, 1.0), "jumping right at the edge meets it");
    }

    #[test]
    fn bots_leap_onto_raised_goals_and_score_on_highgoal() {
        let mut w = World::new();
        w.set_map(MapId::Highgoal);
        w.set_mode(SupportedMode::Football);
        w.start_match(true);
        let (mut carries, mut carrier) = (0, None);
        for _ in 0..(600.0 / STEP) as usize {
            if w.state != MatchState::Playing { break; }
            w.physics_step();
            if w.ball.carrier != carrier { if w.ball.carrier.is_some() { carries += 1; } carrier = w.ball.carrier; }
        }
        println!("highgoal: {} players, {carries} carries, score {:?}", w.players.len(), w.score);
        assert!(w.score[0] + w.score[1] >= 1, "bots must score by leaping: {carries} carries, {:?}", w.score);
    }

    /// Throws at every wall of a stadium, grazing to head-on, low and at mid
    /// height, at pass speeds. Walls are found by sweeping out from the centre;
    /// a low throw may bounce about but must end inside them.
    fn wall_escapes(map: MapId, lanes: &[f32], angles: &[f32], speeds: &[f32], lifts: &[f32]) -> (usize, usize, Vec<String>) {
        let mut w = World::new();
        w.set_map(map); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let pack = crate::map_pack::on(map).unwrap();
        let [x0, z0, x1, z1] = super::field(map).unwrap().bounds.unwrap();
        let centre = Vec2::new((x0 + x1) / 2.0, (z0 + z1) / 2.0);
        let floor_at = |p: Vec2| crate::terrain::support_on(map, Vec3::new(p.x, 200.0, p.y)).0;
        w.players[0].pos = Vec3::new(centre.x, 400.0, centre.y);
        let (mut tried, mut escaped, mut examples) = (0, 0, Vec::new());
        for out in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
            for &offset in lanes {
                let along = Vec2::new(-out.y, out.x);
                let lane = centre + along * offset;
                let y = floor_at(lane) + 1.5;
                let from = Vec3::new(lane.x, y, lane.y);
                let Some((t, _)) = pack.sweep(from, from + Vec3::new(out.x, 0.0, out.y) * 400.0, BALL_RADIUS) else { continue };
                let wall = lane + out * (t * 400.0 + BALL_RADIUS);
                // Start inside both the wall and the field's bounds.
                let half = if out.x != 0.0 { (x1 - x0) / 2.0 } else { (z1 - z0) / 2.0 };
                let depth = (t * 400.0 + BALL_RADIUS - 6.0).min(half - 2.0);
                for &angle_deg in angles {
                    for &speed in speeds {
                        for &lift in lifts {
                            for side in [-1.0f32, 1.0] {
                                let a = angle_deg.to_radians();
                                let dir = out * a.sin() + along * side * a.cos();
                                let start = lane + out * depth - along * side * 12.0 * a.cos();
                                let floor = floor_at(start);
                                // A fresh match each throw: the clock never reaches half time.
                                w = World::new();
                                w.set_map(map); w.set_mode(SupportedMode::Football); w.start_match(true);
                                w.players.truncate(1);
                                w.players[0].pos = Vec3::new(centre.x, 400.0, centre.y);
                                w.ball = Ball { active: true, in_play: true, pos: Vec3::new(start.x, floor + lift, start.y),
                                    vel: Vec3::new(dir.x * speed, 1.0, dir.y * speed), ..Ball::default() };
                                if out_of_field(map, w.ball.pos) { continue; }
                                tried += 1;
                                for _ in 0..(1.5 / STEP) as usize {
                                    w.physics_step();
                                    if !w.ball.in_play { break; }
                                }
                                let b = w.ball.pos;
                                let past = (Vec2::new(b.x, b.z) - wall).dot(out);
                                if !w.ball.in_play || past > 0.0 || b.y > floor + 12.0 {
                                    escaped += 1;
                                    if examples.len() < 8 { examples.push(format!("out {out} lane {offset} angle {angle_deg} speed {speed} lift {lift} -> {b} past {past:.1} in_play {}", w.ball.in_play)); }
                                }
                            }
                        }
                    }
                }
            }
        }
        (tried, escaped, examples)
    }

    /// Passes that hit Highgoal's walls stay inside (they went through at a
    /// glancing angle, 866 of the 2520 throws in the full sweep below).
    #[test]
    fn passes_into_highgoal_walls_stay_inside() {
        let (tried, escaped, examples) = wall_escapes(MapId::Highgoal, &[0.0], &[5.0, 10.0, 20.0, 45.0, 90.0], &[35.0, 70.0, 90.0], &[0.5, 4.0]);
        assert!(tried > 100 && escaped == 0, "{escaped} of {tried} throws escaped: {examples:#?}");
    }

    /// The full sweep: `cargo test --release -p peakrunner-core --lib
    /// every_wall_throw_stays_inside_highgoal -- --ignored`.
    #[test]
    #[ignore = "slow; run with --release"]
    fn every_wall_throw_stays_inside_highgoal() {
        let (tried, escaped, examples) = wall_escapes(MapId::Highgoal, &[-18.0, 0.0, 18.0],
            &[5.0, 10.0, 20.0, 35.0, 50.0, 70.0, 90.0], &[20.0, 35.0, 50.0, 70.0, 90.0], &[0.5, 2.0, 4.0]);
        assert!(tried > 2000 && escaped == 0, "{escaped} of {tried} throws escaped: {examples:#?}");
    }

    #[test]
    fn a_ball_against_the_wall_stays_in_play_until_it_goes_over() {
        let mut w = World::new();
        w.set_map(MapId::Highgoal); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        w.players[0].pos = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1024.0);
        let [x0, _, x1, _] = super::field(MapId::Highgoal).unwrap().bounds.unwrap();
        // Thrown hard into each side wall, and rolled along its foot.
        for (x, vx) in [(x0 + 3.0, -40.0), (x1 - 3.0, 40.0)] {
            w.ball = Ball { active: true, in_play: true, pos: Vec3::new(x, 101.5, 990.0),
                vel: Vec3::new(vx, 2.0, 6.0), ..Ball::default() };
            for _ in 0..(3.0 / STEP) as usize { w.physics_step(); }
            assert!(w.ball.in_play && w.ball.carrier.is_none(), "ball against the wall at {}", w.ball.pos);
            assert!(!w.feed.iter().any(|e| e.line().contains("left the field")));
        }
        // A player leaning on the wall takes no out-of-bounds damage.
        w.players[0].pos = Vec3::new(x0 + 0.2, 100.0 + PLAYER_RADIUS, 1024.0);
        w.players[0].health = 100.0;
        w.input.move_x = -1.0;
        for _ in 0..(2.5 / STEP) as usize { w.physics_step(); }
        assert_eq!(w.players[0].health, 100.0);
        // Over the wall and down the far side: out.
        w.ball = Ball { active: true, in_play: true, pos: Vec3::new(x0 - 6.0, 140.0, 990.0),
            vel: Vec3::new(-5.0, 0.0, 0.0), ..Ball::default() };
        w.physics_step();
        assert!(!w.ball.in_play, "a ball over the wall is reset");
    }

    #[test]
    fn leaving_the_field_hurts_every_second_and_an_idle_ball_regroups_everyone() {
        let mut m = Match::new(MapId::Longfield);
        m.world.set_mode(SupportedMode::Football);
        let a = m.join(1, "Ann").unwrap();
        m.join(2, "Bob").unwrap();
        run(&mut m, WARMUP_SECONDS + 1.0);
        // Up on the stands, outside the bounds.
        let [x0, _, x1, _] = super::field(MapId::Longfield).unwrap().bounds.unwrap();
        let stands = Vec3::new(x1 + OUT_MARGIN + 10.0, 145.0, 1024.0);
        m.world.players[a].pos = stands;
        m.world.players[a].health = 100.0;
        let spawns = crate::terrain::spawn_points_on(MapId::Longfield, m.world.players[a].team == Team::Ember);
        run(&mut m, 2.2);
        let lost = 100.0 - m.world.players[a].health;
        // A global once-a-second tick: two or three in 2.2 s.
        let ticks = lost / OUT_OF_BOUNDS_DAMAGE;
        assert!((ticks - 2.0).abs() < 0.01 || (ticks - 3.0).abs() < 0.01, "lost {lost}");
        assert!(x0 < x1);
        // An idle ball sends everyone back to spawn.
        run(&mut m, 25.0);
        assert!(m.world.feed.iter().any(|e| e.line().contains("sat idle")));
        let home = m.world.players[a].pos;
        assert!(spawns.iter().any(|(s, _)| s.distance(home) < 3.0), "back at a spawn: {home}");
    }

    #[test]
    fn a_new_football_match_starts_on_a_full_clock_without_a_false_half_time() {
        let mut m = Match::new(MapId::Longfield);
        m.world.set_mode(SupportedMode::Football);
        assert_eq!(m.world.time_left, 2.0 * HALF_SECONDS);
        m.join(1, "Ann").unwrap();
        run(&mut m, 3.0);
        assert_eq!(m.world.ball.half, 0);
        assert!(!m.world.feed.iter().any(|e| e.line().contains("Half time")));
    }

    #[test]
    fn ctrl_k_kills_you_through_the_server_and_drops_the_ball() {
        let mut m = field();
        run(&mut m, 3.0);
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        m.world.players[a].pos = m.world.ball.pos;
        m.step(&[]);
        assert!(m.world.ball.carried_by(a));
        let mut commands = vec![None; MAX_PLAYERS];
        commands[a] = Some(Command { seq: 1, suicide: true, ..Default::default() });
        m.step(&commands);
        // It takes 15 s, and asking again doesn't restart it.
        assert!(m.world.players[a].alive && m.world.players[a].suicide_in > SUICIDE_DELAY - 0.1);
        run(&mut m, 10.0);
        commands[a] = Some(Command { seq: 2, suicide: true, ..Default::default() });
        m.step(&commands);
        assert!(m.world.players[a].alive && m.world.players[a].suicide_in < SUICIDE_DELAY - 9.0);
        m.world.players[a].pos = m.world.ball.pos;
        run(&mut m, 5.1);
        assert!(!m.world.players[a].alive);
        assert!(!m.world.ball.carried_by(a), "the ball drops");
        assert_eq!(m.world.players[a].losses, 1);
        assert_eq!(m.world.players[a].frags, 0, "no frag for yourself");
        assert!(m.world.feed.iter().any(|e| e.line().contains("took the quick way out")));
        // Football respawns after a second.
        run(&mut m, RESPAWN_SECONDS + 0.1);
        assert!(m.world.players[a].alive);
        // A client never predicts its own death.
        let mut client = World::new();
        assert!(client.apply_snapshot(&m.snapshot(), 1));
        client.predict_command(Command { seq: 3, suicide: true, ..Default::default() });
        assert!(client.players[client.player_id].alive);
    }

    /// A thrower on the Longfield turf passes to a receiver placed `ahead`
    /// metres in front, `up` metres higher (standing on air, held in place
    /// kinematically) and running sideways at `run` m/s. Returns who caught it.
    fn pass_to(ahead: f32, up: f32, run: f32, aim_off: f32) -> Option<usize> {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let mut r = w.players[0].clone(); r.team = w.players[0].team; r.is_bot = false; w.players.push(r);
        let floor = 100.0 + PLAYER_RADIUS;
        // A negative `up` raises the thrower instead (the turf is flat).
        let thrower = Vec3::new(1024.0, floor + (-up).max(0.0), 1060.0);
        let start = Vec3::new(1024.0, floor + up.max(0.0), 1060.0 - ahead);
        w.players[0].pos = thrower; w.players[0].vel = Vec3::ZERO; w.players[0].yaw = aim_off;
        // Look at the receiver the way a player would.
        let eye = thrower + Vec3::Y * EYE;
        let to = start + Vec3::Y * 0.4 - eye;
        w.players[0].pitch = (to.y / to.length()).asin();
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        let v = Vec3::new(run, 0.0, 0.0);
        w.players[1].pos = start; w.players[1].vel = v;
        w.pass(0);
        for k in 1..(8.0 / STEP) as usize {
            w.players[1].pos = start + v * (k as f32 * STEP);
            w.players[1].vel = v;
            w.players[0].pos = thrower + Vec3::new(0.0, 0.0, 30.0);
            w.step_ball(STEP);
            if w.ball.carrier.is_some() { return w.ball.carrier; }
        }
        None
    }

    /// A pass names who it goes to while it flies (`Ball::pass_to`), and
    /// their game hears "incoming"; not once it lands or is caught.
    #[test]
    fn a_pass_names_who_it_goes_to_until_it_lands_or_is_caught() {
        for (receiver_is_you, catch) in [(true, true), (true, false), (false, true)] {
            let mut w = World::new();
            w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
            w.players.truncate(1);
            let mut r = w.players[0].clone(); r.is_bot = false; w.players.push(r);
            w.player_id = if receiver_is_you { 1 } else { 0 };
            let floor = 100.0 + PLAYER_RADIUS;
            let (thrower, start) = (Vec3::new(1024.0, floor, 1060.0), Vec3::new(1024.0, floor, 1030.0));
            w.players[0].pos = thrower; w.players[0].vel = Vec3::ZERO; w.players[0].yaw = 0.0;
            let to = start + Vec3::Y * 0.4 - (thrower + Vec3::Y * EYE);
            w.players[0].pitch = (to.y / to.length()).asin();
            w.players[1].pos = start; w.players[1].vel = Vec3::ZERO;
            w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
            w.events.clear();
            w.pass(0);
            assert_eq!(w.ball.pass_to, Some(1));
            assert_eq!(w.events.split(',').any(|e| e == "incoming"), receiver_is_you, "only the receiver hears it");
            // Out of the way: the ball flies on to the turf.
            if !catch { w.players[1].pos += Vec3::new(40.0, 0.0, 0.0); }
            let mut flew = 0;
            for _ in 0..(6.0 / STEP) as usize {
                w.players[0].pos = thrower + Vec3::new(0.0, 0.0, 30.0);
                w.step_ball(STEP);
                if w.ball.pass_to.is_none() { break; }
                flew += 1;
            }
            assert!(flew > 10, "named through its flight ({flew} steps)");
            assert!(w.ball.pass_to.is_none());
            if catch { assert_eq!(w.ball.carrier, Some(1)); }
            else { assert!(w.ball.carrier.is_none() && w.ball.pos.y < floor + 1.0, "landed: {:?}", w.ball.pos); }
        }
    }

    #[test]
    fn passes_reach_the_receiver_you_aim_at_near_and_across_the_field() {
        for ahead in [10.0, 30.0, 60.0, 90.0, 160.0] {
            assert_eq!(pass_to(ahead, 0.0, 0.0, 0.0), Some(1), "{ahead} m ahead");
        }
        assert!((pass_speed(300.0) - 6.454 * 300f32.sqrt()).abs() < 1e-3, "full power, uncapped");
        // Aim off to the side and the ball goes where you aimed, not to them.
        assert_eq!(pass_to(40.0, 0.0, 0.0, 0.12), None, "the aim, not the receiver, sets the direction");
    }

    #[test]
    fn classic_skiing_is_hopping_and_each_landing_costs_a_tick_of_friction() {
        let slide = |mode: SupportedMode| {
            let mut w = World::new();
            w.set_map(MapId::Longfield); w.set_mode(mode); w.start_match(true);
            w.players.truncate(1);
            w.players[0].pos = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1060.0);
            w.players[0].vel = Vec3::new(0.0, 0.0, -18.0);
            w.players[0].on_ground = true;
            w.input.jump = true;
            let mut hops = 0;
            for _ in 0..(3.0 / STEP) as usize {
                let before = w.players[0].vel.y;
                w.physics_step();
                if w.players[0].vel.y > 4.0 && before < 1.0 { hops += 1; }
            }
            (Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length(), hops)
        };
        let (football, hops) = slide(SupportedMode::Football);
        let (ctf, _) = slide(SupportedMode::Ctf);
        println!("football: {hops} hops in 3 s, 18 -> {football:.2} m/s; ctf {ctf:.2}");
        // A 5.56 m/s hop is 0.56 s in the air: about 6 landings of 1.28 m/s.
        assert!((5..=7).contains(&hops), "held jump re-jumps at every landing: {hops}");
        assert!((18.0 - hops as f32 * 1.4..18.0 - hops as f32 * 1.1).contains(&football), "football slows: {football}");
        assert!(ctf > 17.5, "ctf keeps its speed: {ctf}");
    }

    #[test]
    fn throws_follow_the_aim_only_no_carried_momentum() {
        let throw = |pitch: f32, run: f32| {
            let mut w = World::new();
            w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
            w.players.truncate(1);
            w.players[0].pos = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1060.0);
            w.players[0].yaw = 0.0;
            w.players[0].pitch = pitch;
            w.players[0].vel = Vec3::new(0.0, 0.0, -run);
            w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
            w.pass(0);
            w.ball.vel
        };
        let still = throw(1.0, 0.0);
        let running = throw(1.0, 12.0);
        // Looking straight up (88 degrees) a pass still leaves at 75.
        let up = throw(LOOK_LIMIT, 0.0);
        let elevation = up.y.atan2(-up.z).to_degrees();
        assert!((elevation - 74.7).abs() < 0.2 && up.y > 33.0, "straight up leaves at {elevation} degrees: {up}");
        assert!(still.y > 25.0 && -still.z > 0.2 * still.y, "steep throws go where they're aimed {still}");
        assert!((running - still).length() < 1e-4, "the thrower's velocity is ignored: {running} vs {still}");
        let low = throw(0.0, 0.0);
        let high = throw(0.6, 0.0);
        assert!(high.y > low.y && -low.z > -high.z, "the aim sets the direction");
        // pitch + 0.5 cos(pitch) - 0.25: 0.25 above a level look, the lift
        // fading toward straight up or down, always rising with the look.
        assert!((throw_elevation(0.0) - 0.25).abs() < 1e-6);
        assert!((low.y.atan2(-low.z) - 0.25).abs() < 1e-4, "a level look lobs 0.25 rad: {low}");
        assert!((throw_elevation(-LOOK_LIMIT) + LOOK_LIMIT + 0.25 - 0.5 * LOOK_LIMIT.cos()).abs() < 1e-5);
        let elevations: Vec<f32> = (-153..=153).map(|k| throw_elevation(k as f32 / 100.0)).collect();
        assert!(elevations.windows(2).all(|w| w[1] > w[0]), "a higher look always throws higher");
        // A drop or fumble has the lift without the pass's tilt.
        assert!((drop_elevation(0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn football_looks_up_and_down_88_degrees_and_other_modes_87() {
        for (mode, limit) in [(SupportedMode::Football, LOOK_LIMIT), (SupportedMode::Ctf, super::super::LOOK_LIMIT)] {
            let mut w = World::new();
            w.set_map(MapId::Longfield); w.set_mode(mode); w.start_match(true);
            w.add_look(0.0, -1.0e6);
            assert_eq!(w.players[w.player_id].pitch, limit, "{mode:?} up");
            w.add_look(0.0, 1.0e6);
            assert_eq!(w.players[w.player_id].pitch, -limit, "{mode:?} down");
        }
        assert!((LOOK_LIMIT.to_degrees() - 88.0).abs() < 1e-4);
    }

    /// A loose ball on Longfield's turf at rest height, moving at `vel`;
    /// stepped until it rests: (its path, seconds).
    fn loose_ball(at: Vec3, vel: Vec3) -> (Vec<Vec3>, f32) {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        w.players[0].pos = Vec3::new(900.0, 101.0, 900.0);
        w.ball = Ball { active: true, in_play: true, pos: at, vel, ..Ball::default() };
        let mut path = vec![at];
        for k in 1..(8.0 / STEP) as usize {
            w.fly_ball(STEP);
            path.push(w.ball.pos);
            if w.ball.vel == Vec3::ZERO { return (path, k as f32 * STEP); }
        }
        (path, 8.0)
    }

    #[test]
    fn the_ball_barely_bounces_and_skids_to_a_stop() {
        let ground = crate::terrain::support_on(MapId::Longfield, Vec3::new(1024.0, 110.0, 1000.0)).0 + BALL_RADIUS;
        // Dropped from 20 m: it hits at 28 m/s and bounces back at 0.2 x
        // turf's 0.6 of that, 3.4 m/s, about 30 cm high; the next touch,
        // under the rest speed, stops it.
        let (path, t) = loose_ball(Vec3::new(1024.0, ground + 20.0, 1000.0), Vec3::ZERO);
        let first = path.iter().position(|p| p.y < ground + 0.05).expect("lands");
        let hop = path[first..].iter().map(|p| p.y - ground).fold(0.0, f32::max);
        assert!((0.24..0.36).contains(&hop) && t < first as f32 * STEP + 0.7, "bounced {hop:.3} m, rested at {t:.2} s");
        // From 2 m it hits at 9 m/s: a 1.1 m/s bounce is under the rest
        // speed (1.28), so it stops dead.
        let (path, _) = loose_ball(Vec3::new(1024.0, ground + 2.0, 1000.0), Vec3::ZERO);
        let first = path.iter().position(|p| p.y < ground + 0.05).expect("lands");
        assert!(path[first..].iter().all(|p| p.y < ground + 0.05), "no hop from 2 m");
        // Skidding at 10 m/s on the turf: friction 1.4 against gravity's
        // pull, about 25 m/s², stops it in about 2 m and under half a second.
        let (path, t) = loose_ball(Vec3::new(1024.0, ground, 1000.0), Vec3::new(0.0, 0.0, -10.0));
        let slid = path[0].distance(*path.last().unwrap());
        assert!((1.4..3.0).contains(&slid) && t < 0.6, "slid {slid:.2} m in {t:.2} s");
        // At rest it stays put.
        let (path, _) = loose_ball(Vec3::new(1024.0, ground, 1000.0), Vec3::new(0.5, 0.0, 0.0));
        assert!(path[0].distance(*path.last().unwrap()) < 0.05);
    }

    #[test]
    fn touching_a_surface_cancels_the_impact_rubs_off_speed_and_bounces_a_little() {
        // Straight down at 10 m/s onto the default material: back up at 2.
        let mut v = Vec3::new(0.0, -10.0, 0.0);
        assert!(touch_surface(&mut v, Vec3::Y, BUILT));
        assert!((v.y - 2.0).abs() < 1e-3 && v.x == 0.0, "{v}");
        // A glancing hit, 20 m/s along and 2 into turf: friction 1.4 takes
        // 2.8 off the slide, then the bounce adds 0.24 back.
        let mut v = Vec3::new(20.0, -2.0, 0.0);
        touch_surface(&mut v, Vec3::Y, TURF);
        assert!((v.x - 17.2).abs() < 1e-2 && (v.y - 0.24).abs() < 1e-2, "{v}");
        // Leaving the surface touches nothing.
        let mut v = Vec3::new(3.0, 1.0, 0.0);
        assert!(!touch_surface(&mut v, Vec3::Y, TURF));
    }

    #[test]
    fn a_catch_pushes_along_the_throw_s_heading_not_its_flight() {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let at = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1060.0);
        (w.players[0].pos, w.players[0].vel, w.players[0].yaw, w.players[0].pitch) = (at, Vec3::ZERO, 0.0, 0.0);
        let mut o = w.players[0].clone();
        o.pos = at + Vec3::new(0.0, 0.0, -30.0);
        w.players.push(o);
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        w.pass(0);
        assert_eq!(w.ball.facing, 0.0, "it leaves with the thrower's facing");
        // Thrown toward -z, it comes back (off a wall, say) flying +z into
        // the other player: the catch still pushes toward -z.
        (w.ball.pos, w.ball.vel, w.ball.no_catch) = (w.players[1].pos, Vec3::new(0.0, 0.0, 20.0), 0.0);
        w.pickup(STEP);
        assert!(w.ball.carried_by(1));
        let push = w.players[1].vel;
        assert!((push.z + 2.0 * 20.0 * 6.5 / 81.0).abs() < 1e-3 && push.x.abs() < 1e-4, "{push}");
    }

    #[test]
    fn a_ball_that_reaches_you_before_the_turf_is_caught_at_that_speed() {
        // A low pass reaches the receiver's body and then strikes the turf
        // at their feet, both in one step: the catch takes the speed it
        // came in with, not what the bounce left it.
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let at = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1000.0);
        let mut r = w.players[0].clone();
        (r.pos, r.vel) = (at, Vec3::ZERO);
        w.players[0].pos = at + Vec3::new(0.0, 0.0, 60.0);
        w.players.push(r);
        let vel = Vec3::new(0.0, -15.0, -60.0);
        w.ball = Ball { active: true, in_play: true, carrier: None, thrown_by: Some(0), facing: 0.0,
            pos: Vec3::new(at.x, 100.4, at.z + 1.0), vel, ..Ball::default() };
        w.step_ball(STEP);
        assert!(w.ball.carried_by(1));
        let came = (vel - Vec3::Y * crate::defs::movement().gravity * STEP).length();
        let push = w.players[1].vel;
        assert!((push.z + 2.0 * came * 6.5 / 81.0).abs() < 0.05 && push.x.abs() < 1e-4, "{push}, came in at {came:.1} m/s");
    }

    #[test]
    fn a_throw_aimed_straight_up_still_goes_forward() {
        // 88 degrees up leaves at 75: down about 30 m ahead (forward is -z).
        let (v, land, from, apex, t, _) = throw_probe(Vec3::ZERO, 0.0, LOOK_LIMIT, None, None);
        let ahead = from.z - land.z;
        assert!((26.0..34.0).contains(&ahead) && apex > 25.0 && t > 3.0, "{ahead:.1} m ahead, {apex:.1} m up, {t:.2} s, released at {v}");
    }

    #[test]
    fn a_thrower_cannot_catch_their_own_throw_for_a_second() {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let at = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1060.0);
        (w.players[0].pos, w.players[0].vel, w.players[0].pitch) = (at, Vec3::ZERO, LOOK_LIMIT);
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        w.pass(0);
        // It starts inside the thrower and stays out of their hands.
        for _ in 0..(0.95 / STEP) as usize { w.pickup(STEP); w.ball.no_catch = (w.ball.no_catch - STEP).max(0.0); }
        assert!(!w.ball.carried_by(0), "caught within a second");
        w.ball.pos = w.players[0].pos;
        for _ in 0..(0.1 / STEP) as usize { w.ball.no_catch = (w.ball.no_catch - STEP).max(0.0); }
        w.pickup(STEP);
        assert!(w.ball.carried_by(0), "touchable after a second");
    }

    #[test]
    fn a_body_stays_down_a_while_and_a_carrier_over_it_fumbles() {
        let mut m = field();
        run(&mut m, 4.0);
        let a = m.world.players.iter().position(|p| p.net_id == 1).unwrap();
        let b = m.world.players.iter().position(|p| p.net_id == 2).unwrap();
        // Bob dies: his body stays down where he fell, after he's back.
        let lay = m.world.players[b].pos;
        m.world.kill(b, None, "Fall");
        assert_eq!(m.world.ball.bodies.len(), 1);
        assert!(m.world.ball.bodies[0].lying(), "down at once");
        run(&mut m, 3.0);
        assert!(m.world.players[b].alive, "Bob is back in play");
        let t = m.world.ball.bodies[0].clone();
        assert!(t.lying() && Vec2::new(t.pos.x - lay.x, t.pos.z - lay.z).length() < 3.0, "{t:?} vs {lay}");
        // Ann carries the ball onto it and soon fumbles.
        m.world.players[a].pos = m.world.ball.pos;
        m.step(&[]);
        assert!(m.world.ball.carried_by(a));
        let mut fumbled = false;
        for _ in 0..(1.0 / STEP) as usize {
            m.world.players[a].pos = t.pos + Vec3::Y * (PLAYER_RADIUS + 0.1);
            m.world.players[a].vel = Vec3::ZERO;
            m.step(&[]);
            if !m.world.ball.carried_by(a) { fumbled = true; break; }
        }
        assert!(fumbled, "standing on a body for a second never fumbled");
        assert!(m.world.feed.iter().any(|e| e.line().contains("over a body and fumble")));
        // It stays down BODY_LIES in all, then it's gone.
        run(&mut m, BODY_LIES);
        assert!(m.world.ball.bodies.is_empty());
    }

    #[test]
    fn a_player_who_leaves_leaves_nothing_behind_but_a_body() {
        let mut m = field();
        // A third player keeps the match going when two have left.
        m.join(3, "Cat").unwrap();
        m.join(4, "Dee").unwrap();
        run(&mut m, 2.0);
        let (a, b) = (m.world.players.iter().position(|p| p.net_id == 1).unwrap(), m.world.players.iter().position(|p| p.net_id == 2).unwrap());
        m.world.kill(b, None, "Fall");
        let lay = m.world.ball.bodies[0].pos;
        m.leave(b);
        run(&mut m, 2.0);
        assert_eq!(m.world.ball.bodies.len(), 1, "a dead leaver's body stays down");
        assert!(m.world.ball.bodies[0].pos.distance(lay) < 3.0);
        m.leave(a);
        assert_eq!(m.world.ball.bodies.len(), 1, "a live one leaves nothing: nobody holds a thrower");
    }

    /// One throw on the Longfield turf from a thrower facing -z: moving at
    /// `vel`, `up` metres off the ground, looking `pitch` up, with a
    /// teammate `mate` metres ahead under the crosshair (None: nobody) and
    /// a player `cone` metres ahead in the yaw cone but off the crosshair.
    /// Flies it to its first touch of the ground: (release velocity, landing
    /// spot, apex above the release, seconds, thrower at landing).
    fn throw_probe(vel: Vec3, up: f32, pitch: f32, mate: Option<f32>, cone: Option<f32>) -> (Vec3, Vec3, Vec3, f32, f32, Vec3) {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let floor = 100.0 + PLAYER_RADIUS;
        let at = Vec3::new(1024.0, floor + up, 1060.0);
        (w.players[0].pos, w.players[0].vel, w.players[0].yaw, w.players[0].pitch) = (at, vel, 0.0, pitch);
        let mut add = |offset: Vec3| {
            let mut o = w.players[0].clone();
            (o.pos, o.vel, o.pitch) = (Vec3::new(at.x, floor, at.z) + offset, Vec3::ZERO, 0.0);
            w.players.push(o);
        };
        if let Some(d) = mate {
            // On the crosshair's ray (level ground: below it when looking up).
            let dir = look_dir(0.0, pitch);
            let flat = Vec2::new(dir.x, dir.z).normalize_or(Vec2::new(0.0, -1.0));
            add(Vec3::new(flat.x * d, 0.0, flat.y * d));
        }
        if let Some(d) = cone { add(Vec3::new(d * 0.08, 0.0, -d)); }
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        w.pass(0);
        let (release, from) = (w.ball.vel, w.ball.pos);
        let mut apex = from.y;
        let mut thrower = at;
        for k in 1..(12.0 / STEP) as usize {
            // Everyone else stands; the thrower keeps going.
            for o in w.players.iter_mut().skip(1) { o.vel = Vec3::ZERO; }
            thrower += vel * STEP;
            let before = w.ball.vel.y;
            w.ball.carrier = None;
            w.fly_ball(STEP);
            apex = apex.max(w.ball.pos.y);
            if before < 0.0 && w.ball.vel.y >= 0.0 {
                return (release, w.ball.pos, from, apex - from.y, k as f32 * STEP, thrower);
            }
        }
        (release, w.ball.pos, from, apex - from.y, 12.0, thrower)
    }

    #[test]
    fn any_player_under_the_crosshair_or_in_the_yaw_slice_sets_a_pass_s_power() {
        // Someone 150 m down the field, just off your facing, sets the power
        // whatever the look: the slice is left and right only.
        let speed = |pitch: f32| throw_probe(Vec3::ZERO, 0.0, pitch, None, Some(150.0)).0.length();
        let far = pass_speed((150f32.powi(2) + 12f32.powi(2)).sqrt());
        for pitch in [0.0, 0.35, 0.8, -0.3] {
            assert!((speed(pitch) - far).abs() < 0.5, "{pitch}: {}", speed(pitch));
        }
        // Nobody to aim at: a loose throw.
        assert!((throw_probe(Vec3::ZERO, 0.0, 0.0, None, None).0.length() - LOOSE_THROW).abs() < 1e-3);
        // An enemy on the line, 15 m out in front of a teammate 40 m away:
        // the crosshair meets the enemy first, and they set the power.
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        let floor = 100.0 + PLAYER_RADIUS;
        let at = Vec3::new(1024.0, floor, 1060.0);
        (w.players[0].pos, w.players[0].vel, w.players[0].yaw) = (at, Vec3::ZERO, 0.0);
        let team = w.players[0].team;
        for (d, side) in [(15.0, team.other()), (40.0, team)] {
            let mut o = w.players[0].clone();
            (o.pos, o.team) = (at + Vec3::new(0.0, 0.0, -d), side);
            w.players.push(o);
        }
        let to = w.players[2].pos + Vec3::Y * 0.4 - (at + Vec3::Y * EYE);
        w.players[0].pitch = (to.y / to.length()).asin();
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        w.pass(0);
        assert!((w.ball.vel.length() - pass_speed(15.0)).abs() < 1e-3, "{}", w.ball.vel.length());
    }

    /// Throws from standing, running, strafing, skiing and the air, aimed
    /// level, up and down, at nobody, at a teammate 30 m ahead, and with
    /// someone far up the field in the yaw cone. Run with --ignored
    /// --nocapture.
    #[test]
    #[ignore = "measurement"]
    fn probe_throws() {
        let states = [("stand", Vec3::ZERO, 0.0), ("run fwd 8", Vec3::new(0.0, 0.0, -8.0), 0.0), ("strafe 8", Vec3::new(8.0, 0.0, 0.0), 0.0),
            ("ski fwd 30", Vec3::new(0.0, 0.0, -30.0), 0.0), ("ski side 30", Vec3::new(30.0, 0.0, -10.0), 0.0), ("air 12", Vec3::new(0.0, 4.0, -12.0), 3.0)];
        let aims = [("level", 0.0f32), ("up 20", 20f32.to_radians()), ("up 45", 45f32.to_radians()), ("up 88", LOOK_LIMIT), ("down 15", -15f32.to_radians())];
        let targets: [(&str, Option<f32>, Option<f32>); 3] = [("nobody", None, None), ("mate 30", Some(30.0), None), ("cone 150", None, Some(150.0))];
        println!("{:<11} {:<8} {:<9} {:>6} {:>6} {:>6} {:>7} {:>6} {:>5} {:>8}", "state", "aim", "target", "speed", "elev", "off", "dist", "apex", "time", "drift");
        for (state, vel, up) in states {
            for (aim, pitch) in aims {
                for (target, mate, cone) in targets {
                    let (v, land, from, apex, t, thrower) = throw_probe(vel, up, pitch, mate, cone);
                    let flat = Vec2::new(v.x, v.z);
                    let elev = v.y.atan2(flat.length()).to_degrees() - pitch.to_degrees();
                    // Off the aim line (yaw 0 faces -z): sideways angle of the release.
                    let off = v.x.atan2(-v.z).to_degrees();
                    let dist = Vec2::new(land.x - from.x, land.z - from.z).length();
                    // Where the landing sits from the thrower's own line of sight at landing.
                    let drift = land.x - thrower.x;
                    println!("{state:<11} {aim:<8} {target:<9} {:>6.1} {elev:>+6.1} {off:>+6.1} {dist:>7.1} {apex:>6.1} {t:>5.2} {drift:>+8.1}", v.length());
                }
            }
        }
    }

    #[test]
    fn a_knocked_loose_ball_flies_on_with_the_hit_and_a_dropped_one_goes_ahead() {
        // A defender at 12 m/s runs into a carrier standing still: the bodies
        // swap velocities and the ball leaves with the carrier's new one.
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(2);
        (w.players[0].team, w.players[1].team) = (Team::Ember, Team::Glacier);
        let floor = 100.0 + PLAYER_RADIUS;
        w.players[0].pos = Vec3::new(1024.0, floor, 1060.0);
        w.players[1].pos = Vec3::new(1024.0, floor, 1061.2);
        w.players[0].pitch = 0.0;
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        let (va, vb) = (Vec3::ZERO, Vec3::new(0.0, 0.0, -12.0));
        w.players[0].vel = vb;
        w.football_hit(0, 1, va, vb);
        assert!(!w.ball.carried_by(0), "12 m/s knocks it loose");
        let pop = w.ball.vel - vb;
        let lift = drop_elevation(0.0);
        assert!((pop.length() - FUMBLE_POP).abs() < 0.01 && (pop.y - FUMBLE_POP * lift.sin()).abs() < 0.01,
            "a pop 0.5 rad above a level look, on the hit's momentum: {}", w.ball.vel);
        // A carrier who dies drops it ahead, keeping their own speed.
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        w.players[0].yaw = 0.0;
        w.players[0].vel = Vec3::new(3.0, 0.0, 0.0);
        w.kill(0, None, "Test");
        let ahead = Vec3::new(3.0, FUMBLE_POP * lift.sin(), -FUMBLE_POP * lift.cos());
        assert!((w.ball.vel - ahead).length() < 0.01, "dropped ahead: {}", w.ball.vel);
    }

    #[test]
    fn a_fumbling_carrier_cannot_grab_the_ball_straight_back() {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(1);
        w.players[0].pos = Vec3::new(1024.0, 100.0 + PLAYER_RADIUS, 1060.0);
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        w.fumble(0);
        for _ in 0..5 { w.step_ball(STEP); }
        assert!(!w.ball.carried_by(0), "the fumble stands");
    }

    /// A ball knocked loose is a fumble the games announce (`Ball::fumbles`),
    /// and the other side coming up with it, moving fast, picks it up: only
    /// a pass is intercepted. A carrier who dies drops it without a fumble.
    #[test]
    fn a_fumble_is_counted_and_picked_up_never_intercepted() {
        let mut w = World::new();
        w.set_map(MapId::Longfield); w.set_mode(SupportedMode::Football); w.start_match(true);
        w.players.truncate(2);
        (w.players[0].team, w.players[1].team) = (Team::Ember, Team::Glacier);
        let floor = 100.0 + PLAYER_RADIUS;
        w.players[0].pos = Vec3::new(1024.0, floor, 1060.0);
        w.players[1].pos = Vec3::new(1024.0, floor, 1061.2);
        w.ball = Ball { active: true, in_play: true, carrier: Some(0), ..Ball::default() };
        let vb = Vec3::new(0.0, 0.0, -12.0);
        w.players[0].vel = vb;
        w.football_hit(0, 1, Vec3::ZERO, vb);
        assert!(w.ball.loose() && w.ball.dropped && w.ball.fumbles == 1, "{:?}", w.ball);
        // The hitter's side takes it while it still flies at 8 m/s.
        w.players[1].pos = Vec3::new(1030.0, floor, 1060.0);
        w.ball.pos = w.players[1].pos;
        w.ball.vel = Vec3::new(8.0, 0.0, 0.0);
        w.pickup(STEP);
        assert!(w.ball.carried_by(1) && !w.ball.dropped);
        let last = w.feed.iter().last().map(|e| e.line()).unwrap_or_default();
        assert!(last.ends_with("picks up the ball"), "{last}");
        // Their pass, caught by the other side on the way, is intercepted.
        w.pass(1);
        assert!(!w.ball.dropped && w.ball.fumbles == 1);
        w.ball.pos = w.players[0].pos;
        w.ball.vel = Vec3::new(-8.0, 0.0, 0.0);
        w.ball.no_catch = 0.0;
        w.pickup(STEP);
        assert!(w.ball.carried_by(0));
        let last = w.feed.iter().last().map(|e| e.line()).unwrap_or_default();
        assert!(last.contains(" intercept"), "{last}");
        // Dying with it drops it: picked up after, but no fumble to announce.
        w.kill(0, None, "Test");
        assert!(w.ball.loose() && w.ball.dropped && w.ball.fumbles == 1);
    }
}
