//! Locomotion for the third-person body: where the feet go and how the hips
//! carry them, running in any direction, skiing, standing and in the air.
//! `player_model::pose` reaches the feet with two-bone legs.
//!
//! A running foot is planted: from landing to push-off it stays where it
//! came down while the body passes over it, so the contact length, the share
//! of the stride spent on the ground and the step rate follow the speed as a
//! runner's do, and nothing skates. Off the ground the foot travels a path
//! through the heel tucking up behind, the knee driving it forward and the
//! reach for the next landing. The hips settle onto the planted leg and rise
//! through the flight between steps, turn so the swinging leg's hip comes
//! forward, and drop on the swinging side. The lower body turns toward the
//! way the body travels (a strafe steps sideways from turned hips, a
//! backpedal runs the stride backward) while the chest keeps facing the aim.
//! A hop from a run (Football's held jump hops from landing to landing, its
//! skiing) is a bound: the run's own stride carried through the air, the
//! legs split, landing on the other foot (`GaitIn::bound`). A skier's hop
//! draws the ski stance up under the hips; a jet or a long fall lets the
//! legs hang.
//! Render-only, like everything the pose drives.

use glam::{Mat4, Vec3, Vec4};
use std::f32::consts::{PI, TAU};

/// Thigh and shin (m).
pub const THIGH: f32 = 0.44;
pub const SHIN: f32 = 0.44;
/// The ground under a standing body, in its frame (the player's position,
/// the collision sphere's centre, is the origin): straight legs reach it.
pub const SOLE: f32 = -0.145;
/// The ankle above the sole, and the toe's tip from the ankle in the foot's
/// frame (-Z toward the toes): a planted foot rolls off its toe, which lifts
/// the ankle as the heel comes up.
const ANKLE_H: f32 = 0.165;
pub(crate) const TOE: Vec3 = Vec3::new(0.0, -ANKLE_H, -0.21);
/// A hip joint from the pelvis point (the right one; the left is mirrored).
pub const HIP: Vec3 = Vec3::new(0.16, -0.05, 0.0);
/// The pelvis point of a body standing with straight legs.
pub const PELVIS_Y: f32 = 0.95;
/// The longest a leg is let reach (hip to ankle): the knee keeps a little
/// bend, so it never snaps straight.
const REACH: f32 = (THIGH + SHIN) * 0.9985;

/// Steps a second at a ground speed: a walker's below 2 m/s, a jogger's near
/// 3, a sprinter's toward 4 at the top run speed (11.2 m/s).
pub fn steps(speed: f32) -> f32 {
    table(&[(0.0, 1.5), (1.5, 1.9), (3.0, 2.6), (5.0, 2.9), (7.0, 3.2), (9.0, 3.5), (11.2, 3.8), (16.0, 4.2)], speed)
}

/// A planted foot's contact at a speed: how far the body travels over it
/// (m), and the share of the full stride (both steps) it spends on the
/// ground. The share falls from a walker's (both feet down between steps)
/// to a sprinter's fifth of the stride; the length is what keeps the foot
/// still on the ground at that share.
fn contact(speed: f32) -> (f32, f32) {
    let rate = steps(speed);
    let reach = 0.72 + 0.012 * speed.min(16.0);
    let share = (reach * rate / (2.0 * speed.max(0.05))).clamp(0.17, 0.64);
    (share * 2.0 * speed / rate, share)
}

/// What the gait reads, all in the body's frame (-Z the facing, +X right).
#[derive(Clone, Copy, Debug)]
pub struct GaitIn {
    pub vel: Vec3,
    /// Stride phase in radians: a turn is a full stride, the right foot
    /// landing at 0 and the left half a turn later.
    pub phase: f32,
    /// 1 on the ground, 0 in the air, eased between (`Motion::ground`).
    pub ground: f32,
    /// 1 skiing, eased.
    pub ski: f32,
    /// 1 backpedalling (the stride runs backward), eased.
    pub back: f32,
    /// 1 in a skier's low hop at speed (`hopping`), eased: the air's legs
    /// tucked in the ski stance rather than hanging, and the ground's
    /// between hops.
    pub hop: f32,
    /// 1 in a bound (a hop from a run), eased: in the air the run's stride
    /// goes on, its phase carried to the next landing over the flight
    /// (`player_model::AnimTracker`).
    pub bound: f32,
    /// The armor's mass: 0 light, 1 heavy.
    pub mass: f32,
    pub jetting: bool,
    pub time: f32,
    pub seed: f32,
}

/// Where the legs go and how the hips and chest carry them.
#[derive(Clone, Copy, Debug)]
pub struct Gait {
    /// The pelvis frame: its point, its turn toward the travel and the
    /// stride's swivel, the drop on the swinging side and its tilt.
    pub pelvis: Mat4,
    /// Ankle targets for the legs (left, right) and each foot's frame at
    /// its ankle (pitched and turned; -Z toward the toes).
    pub ankles: [Vec3; 2],
    pub feet: [Mat4; 2],
    /// Which way each knee bends.
    pub poles: [Vec3; 2],
    /// The chest's lean forward and toward the travel's side (radians).
    pub lean: f32,
    pub lean_side: f32,
    /// The chest's own turn with the stride (on top of any blading).
    pub twist: f32,
    /// The pelvis's rise and fall about its mean (m), for the hands to soak.
    pub bob: f32,
    /// Free arms' swing: +1 the right arm forward, -1 the left.
    pub arms: f32,
    /// How much of a run there is to swing the arms to (0 to 1).
    pub stride: f32,
}

/// The parts of a leg pose that blend: the pelvis's height, sideways shift
/// and turns, both ankles and both feet's pitch (toes down positive) and
/// heading (positive turns the toes right), and the chest's lean.
#[derive(Clone, Copy, Debug)]
struct Legs {
    y: f32,
    sway: f32,
    heading: f32,
    swivel: f32,
    roll: f32,
    tilt: f32,
    ankles: [Vec3; 2],
    pitch: [f32; 2],
    toe: [f32; 2],
    lean: f32,
    lean_side: f32,
    knee_out: f32,
}

impl Legs {
    fn mix(&self, o: &Legs, t: f32) -> Legs {
        if t <= 0.0 { return *self; }
        if t >= 1.0 { return *o; }
        let m = |a: f32, b: f32| a + (b - a) * t;
        Legs { y: m(self.y, o.y), sway: m(self.sway, o.sway), heading: m(self.heading, o.heading), swivel: m(self.swivel, o.swivel),
            roll: m(self.roll, o.roll), tilt: m(self.tilt, o.tilt),
            ankles: [self.ankles[0].lerp(o.ankles[0], t), self.ankles[1].lerp(o.ankles[1], t)],
            pitch: [m(self.pitch[0], o.pitch[0]), m(self.pitch[1], o.pitch[1])],
            toe: [m(self.toe[0], o.toe[0]), m(self.toe[1], o.toe[1])],
            lean: m(self.lean, o.lean), lean_side: m(self.lean_side, o.lean_side), knee_out: m(self.knee_out, o.knee_out) }
    }

    /// The pelvis frame these legs hang from.
    fn pelvis(&self) -> Mat4 {
        let right = Vec3::new(self.heading.cos(), 0.0, self.heading.sin());
        Mat4::from_translation(Vec3::new(0.0, self.y, 0.0) + right * self.sway)
            * Mat4::from_rotation_y(-(self.heading - self.swivel)) * Mat4::from_rotation_z(self.roll) * Mat4::from_rotation_x(-self.tilt)
    }
}

/// The frame of a foot whose toes point `toe` (radians, right positive) and
/// pitch `pitch` (toes down positive), at the origin.
fn foot_turn(toe: f32, pitch: f32) -> Mat4 { Mat4::from_rotation_y(-toe) * Mat4::from_rotation_x(-pitch) }

/// The ankle of a foot whose toe rests at `tip`.
fn ankle_over(tip: Vec3, toe: f32, pitch: f32) -> Vec3 { tip - foot_turn(toe, pitch).transform_vector3(TOE) }

/// Forward along a heading (right positive) and its right.
fn axes(heading: f32) -> (Vec3, Vec3) { (Vec3::new(heading.sin(), 0.0, -heading.cos()), Vec3::new(heading.cos(), 0.0, heading.sin())) }

pub fn gait(i: &GaitIn) -> Gait {
    let flat = Vec3::new(i.vel.x, 0.0, i.vel.z);
    let speed = flat.length();
    let moving = smoothstep(0.25, 1.6, speed);
    let dir = if speed > 1e-3 { flat / speed } else { Vec3::NEG_Z };
    let cycle = i.phase / TAU;
    // On the ground: standing, running (forward or backpedalling, eased
    // between), and skiing over either.
    let stand = standing(i);
    let run = if moving > 0.0 {
        let run = if i.back >= 1.0 { running(-dir, speed, -cycle, i.mass) }
            else if i.back <= 0.0 { running(dir, speed, cycle, i.mass) }
            else { running(dir, speed, cycle, i.mass).mix(&running(-dir, speed, -cycle, i.mass), i.back) };
        stand.mix(&run, moving)
    } else { stand };
    // A skier hopping from landing to landing touches down in the ski stance.
    let ski = i.ski.max(i.hop);
    let ground = if ski > 0.0 { run.mix(&skiing(i, dir, speed), ski) } else { run };
    let legs = if i.ground < 1.0 {
        let mut air = if i.hop > 0.0 { flying(i).mix(&hopping(i, dir, speed), i.hop) } else { flying(i) };
        // A bound runs on through the air.
        if i.bound > 0.0 { air = air.mix(&run, i.bound); }
        ground.mix(&air, 1.0 - i.ground)
    } else { ground };
    // The run's share, for the arms and the hands' bob.
    let stride = moving * i.ground.max(i.bound) * (1.0 - ski);
    let pelvis = legs.pelvis();
    let feet = [0, 1].map(|s| Mat4::from_translation(legs.ankles[s]) * foot_turn(legs.toe[s], legs.pitch[s]));
    let (fwd, right) = axes(legs.heading);
    let poles = [-1.0f32, 1.0].map(|side| (fwd + right * side * legs.knee_out).normalize());
    // The bob about the run's mean height, which the hands soak.
    let bob = (legs.y - run_height(speed, i.mass)) * stride;
    Gait { pelvis, ankles: legs.ankles, feet, poles, lean: legs.lean, lean_side: legs.lean_side,
        twist: legs.swivel * 0.35, bob, arms: (cycle * TAU).cos(), stride }
}

/// How high the pelvis rides on average in a run at `speed`: a little lower
/// the faster it goes and the heavier the armor.
fn run_height(speed: f32, mass: f32) -> f32 {
    let k = smoothstep(2.0, 10.0, speed);
    PELVIS_Y - 0.02 - (0.035 + 0.03 * mass) * k
}

/// Standing: feet a little apart and turned out, knees soft, the weight
/// shifting slowly from foot to foot.
fn standing(i: &GaitIn) -> Legs {
    let shift = (i.time * 0.45 + i.seed).sin();
    let breathe = (i.time * 1.7 + i.seed).sin();
    let lead = if i.seed.fract() > 0.5 { 1.0 } else { -1.0 };
    let ankles = [-1.0f32, 1.0].map(|side| Vec3::new(side * (0.13 + 0.03 * i.mass), SOLE + ANKLE_H, -0.035 * side * lead));
    Legs { y: PELVIS_Y - 0.014 - 0.004 * breathe - 0.006 * shift.abs(), sway: 0.014 * shift, heading: 0.0, swivel: 0.03 * shift * lead,
        roll: -0.02 * shift, tilt: 0.03, ankles, pitch: [0.0; 2], toe: [-0.14, 0.14], lean: 0.02, lean_side: 0.0, knee_out: 0.18 }
}

/// The run along `dir` at `speed`, `cycle` turns into the stride (the right
/// foot lands at whole turns). A backpedal is this run the other way with
/// the cycle reversed, which plants the feet just as well.
fn running(dir: Vec3, speed: f32, cycle: f32, mass: f32) -> Legs {
    // How much of a sprint this is: high heels and knees, a lean, a short
    // contact. Heavy armor runs lower and more flat-footed.
    let k = smoothstep(2.0, 10.0, speed) * (1.0 - 0.3 * mass);
    let travel = dir.x.atan2(-dir.z);
    // The hips turn part way toward the travel (the chest stays on the aim);
    // the legs turn further at the hip joints.
    let heading = travel.clamp(-0.8, 0.8);
    let legs_at = travel.clamp(-1.25, 1.25);
    let (_, side_ax) = axes(legs_at);
    let (reach, share) = contact(speed);
    let land = 0.42 * reach;
    let track = 0.06 + 0.035 * (1.0 - k) + 0.03 * mass;
    let push = 0.45 + 0.45 * k;
    let lift = (0.10 + 0.50 * k) * (1.0 - 0.3 * mass);
    let rest = SOLE + ANKLE_H;
    let base = run_height(speed, mass);
    let drop = 0.015 + 0.02 * k + 0.015 * mass;
    // A foot's place `u` of the way through its stance, and its pitch.
    let planted = |side: f32, u: f32| -> Vec4 {
        let pitch = push * smoothstep(0.45, 1.0, u).powf(1.3);
        let toe = legs_at + side * 0.07;
        let tip = dir * (land - reach * u) + side_ax * (side * track) + Vec3::Y * SOLE;
        ankle_over(tip, toe, pitch).extend(pitch)
    };
    let mut ankles = [Vec3::ZERO; 2];
    let mut pitch = [0.0; 2];
    let mut y = f32::INFINITY;
    let (mut sway, mut roll) = (0.0, 0.0);
    let mut flight = None;
    for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        // The right foot lands at whole turns, the left half a turn later.
        let u = (cycle + if s == 0 { 0.5 } else { 0.0 }).rem_euclid(1.0);
        let at = if u < share {
            let us = u / share;
            // Settling onto the planted leg: the hips drop over it, shift
            // toward it and lift on its side.
            y = y.min(base - drop * (PI * us).sin());
            sway += side * (0.012 + 0.02 * (1.0 - k)) * (PI * us).sin();
            roll += side * (0.02 + 0.06 * k) * (PI * us).sin();
            planted(side, us)
        } else {
            let ss = (u - share) / (1.0 - share);
            // Both feet up between steps: the body rises and falls through
            // the flight (the time from this foot's push-off to the other's
            // landing, at half a stride less the contact).
            let off = (u - share) / (0.5 - share).max(1e-3);
            if share < 0.5 && off < 1.0 { flight = Some(off); }
            // The swing: push-off, heel up behind, the knee driving the
            // foot through under the hips, the reach and the landing. The
            // ends continue the stance's own motion, so the foot leaves and
            // meets the ground without a jolt.
            let edge = 0.08;
            let late = edge * share / (1.0 - share);
            let lat = |extra: f32| side_ax * (side * (track + extra)) + Vec3::Y * rest;
            let off = planted(side, 1.0);
            let on = planted(side, 0.0);
            let keys = [
                (-late, planted(side, 1.0 - edge)),
                (0.0, off),
                (0.27, (dir * (off.x * dir.x + off.z * dir.z) * 0.55 + lat(0.02) + Vec3::Y * (0.75 * lift)).extend(0.45 + 0.2 * k)),
                (0.52, (dir * (-0.05 - 0.08 * k) + lat(0.035) + Vec3::Y * lift).extend(0.3)),
                (0.78, (dir * (land + 0.08 + 0.06 * k) + lat(0.02) + Vec3::Y * (0.06 + 0.2 * k)).extend(-0.08)),
                (1.0, on),
                (1.0 + late, planted(side, edge)),
            ];
            spline(&keys, ss)
        };
        ankles[s] = at.truncate();
        pitch[s] = at.w;
    }
    if let Some(f) = flight {
        // Ballistic: the rise over the flight's time.
        let t = (0.5 - share).max(0.0) / steps(speed) * 2.0;
        y = base + 1.225 * t * t * (PI * f).sin();
    } else if !y.is_finite() {
        y = base;
    }
    let swivel = 0.10 * k * (cycle * TAU).cos();
    let mut legs = Legs { y, sway, heading, swivel, roll, tilt: 0.04 + 0.08 * k, ankles, pitch, toe: [legs_at - 0.07, legs_at + 0.07],
        lean: (0.05 + 0.22 * k) * (1.0 - 0.25 * mass) * (-dir.z).max(0.0), lean_side: 0.10 * k * dir.x,
        knee_out: 0.12 };
    // Never let a leg reach past its length: the hips come down instead.
    legs.y = reach_guard(&legs);
    legs
}

/// The highest the pelvis may ride for both legs to reach their ankles with
/// a little bend left at the knee.
fn reach_guard(legs: &Legs) -> f32 {
    let mut y = legs.y;
    for _ in 0..2 {
        let frame = Legs { y, ..*legs }.pelvis();
        let mut need = y;
        for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let hip = frame.transform_point3(Vec3::new(side * HIP.x, HIP.y, HIP.z));
            let a = legs.ankles[s];
            let flat = (hip.x - a.x).hypot(hip.z - a.z);
            if flat >= REACH { continue; }
            let top = a.y + (REACH * REACH - flat * flat).sqrt();
            if hip.y > top { need = need.min(y - (hip.y - top)); }
        }
        y = need;
    }
    y
}

/// Skiing: the feet together and staggered, sliding flat; knees bent and
/// the hips low; the lower body along the slide.
fn skiing(i: &GaitIn, dir: Vec3, speed: f32) -> Legs {
    let heading = ski_heading(dir, speed);
    let (fwd, right) = axes(heading);
    let lead = if i.seed.fract() > 0.5 { 1.0 } else { -1.0 };
    let ankles = [-1.0f32, 1.0].map(|side| right * (side * (0.115 + 0.02 * i.mass)) + fwd * (if side == lead { 0.13 } else { -0.07 })
        + Vec3::Y * (SOLE + ANKLE_H));
    let carve = (i.time * 0.7 + i.seed).sin();
    // An athlete's crouch: the knees forward over the toes, not out.
    Legs { y: PELVIS_Y - 0.17 - 0.02 * i.mass + 0.01 * carve, sway: 0.0, heading: heading * 0.6, swivel: 0.0, roll: 0.0, tilt: 0.2,
        ankles, pitch: [0.0; 2], toe: [heading - 0.04, heading + 0.04], lean: 0.2, lean_side: 0.04 * carve, knee_out: 0.03 }
}

/// The lower body's heading on skis: along the slide, or sliding backward
/// with the feet still facing ahead of the hips.
fn ski_heading(dir: Vec3, speed: f32) -> f32 {
    let travel = dir.x.atan2(-dir.z) * smoothstep(0.5, 3.0, speed);
    let toward = if travel.abs() > 1.75 { travel - PI * travel.signum() } else { travel };
    toward.clamp(-1.2, 1.2)
}

/// A low hop at speed (a skier's bound from landing to landing, a jump from
/// a run): the ski stance drawn up a little, the feet under the knees and
/// the hips back, the chest forward over them; the knees tuck most at the
/// top and the feet reach back down as it falls.
fn hopping(i: &GaitIn, dir: Vec3, speed: f32) -> Legs {
    let mut legs = skiing(i, dir, speed);
    // 1 at the top of the hop, 0 leaving or meeting the ground.
    let top = 1.0 - (i.vel.y / 5.5).clamp(-1.0, 1.0).abs();
    let (fwd, _) = axes(ski_heading(dir, speed));
    let lead = if i.seed.fract() > 0.5 { 1.0 } else { -1.0 };
    for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        legs.ankles[s] += Vec3::Y * (0.04 + 0.1 * top) + fwd * (0.07 + if side == lead { 0.03 } else { -0.04 } * top);
        legs.pitch[s] = 0.04 + 0.1 * top;
    }
    Legs { y: PELVIS_Y - 0.05 - 0.02 * i.mass, lean: 0.3, tilt: 0.25, ..legs }
}

/// In the air: the legs hang loose under the hips, out of step and trailing
/// the motion; jetting, together with the toes pointed; falling fast, they
/// reach down for the landing.
fn flying(i: &GaitIn) -> Legs {
    let ahead = -i.vel.z;
    let fall = smoothstep(6.0, 18.0, -i.vel.y);
    let trail = (ahead / 40.0).clamp(-0.25, 0.45) * (1.0 - fall);
    let mut legs = Legs { y: PELVIS_Y, sway: 0.0, heading: 0.0, swivel: 0.0, roll: 0.0, tilt: 0.02,
        ankles: [Vec3::ZERO; 2], pitch: [0.0; 2], toe: [-0.08, 0.08],
        lean: ((ahead / 60.0).clamp(-0.08, 0.2) + if i.jetting { 0.05 } else { 0.0 }) * (1.0 - fall), lean_side: 0.0, knee_out: 0.15 };
    let frame = legs.pelvis();
    for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let sway = (i.time * 1.4 + i.seed + side * 1.3).sin();
        let hip = frame.transform_point3(Vec3::new(side * HIP.x, HIP.y, HIP.z));
        let (apart, step, hang, toes) = if i.jetting {
            (0.02, side * 0.07 + sway * 0.04, 0.80, 0.75 + sway * 0.06)
        } else {
            (0.05, side * 0.1 + sway * 0.06, 0.78, 0.6 + sway * 0.08)
        };
        // Falling fast: straighter, a little apart, reaching for the ground.
        let reach = Vec3::new(side * 0.06, -0.86, -0.06);
        let loose = Vec3::new(side * apart, -hang, -step + trail);
        legs.ankles[s] = hip + loose.lerp(reach, fall);
        legs.pitch[s] = toes + (0.15 - toes) * fall;
    }
    legs
}

/// A non-uniform Catmull-Rom through `(time, value)` keys at `t`, between
/// the second and second-last keys (the ends only steer the tangents).
fn spline(keys: &[(f32, Vec4)], t: f32) -> Vec4 {
    let last = keys.len() - 3;
    let k = (1..=last).rev().find(|&k| keys[k].0 <= t).unwrap_or(1);
    let ((tm, pm), (t0, p0), (t1, p1), (t2, p2)) = (keys[k - 1], keys[k], keys[k + 1], keys[k + 2]);
    let span = (t1 - t0).max(1e-5);
    let s = ((t - t0) / span).clamp(0.0, 1.0);
    let m0 = (p1 - pm) / (t1 - tm).max(1e-5) * span;
    let m1 = (p2 - p0) / (t2 - t0).max(1e-5) * span;
    let (s2, s3) = (s * s, s * s * s);
    p0 * (2.0 * s3 - 3.0 * s2 + 1.0) + m0 * (s3 - 2.0 * s2 + s) + p1 * (3.0 * s2 - 2.0 * s3) + m1 * (s3 - s2)
}

/// Linear between `(x, y)` points, held past the ends.
fn table(points: &[(f32, f32)], x: f32) -> f32 {
    if x <= points[0].0 { return points[0].1; }
    for w in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 { return y0 + (y1 - y0) * (x - x0) / (x1 - x0); }
    }
    points[points.len() - 1].1
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(speed: f32, phase: f32) -> GaitIn {
        GaitIn { vel: Vec3::new(0.0, 0.0, -speed), phase, ground: 1.0, ski: 0.0, back: 0.0, hop: 0.0, bound: 0.0, mass: 0.5, jetting: false, time: 0.0, seed: 0.0 }
    }

    /// The toe of a foot: where it stays planted while it's down.
    fn ball(g: &Gait, s: usize) -> Vec3 { g.feet[s].transform_point3(TOE) }

    #[test]
    fn a_planted_foot_stays_put_while_the_body_runs_over_it() {
        for speed in [3.0f32, 6.0, 9.0, 11.2] {
            let (_, share) = contact(speed);
            let rate = steps(speed) * 0.5;
            // The right foot's stance: from landing (phase 0) to push-off.
            let n = 12;
            let mut prev: Option<Vec3> = None;
            for k in 0..=n {
                let u = share * k as f32 / n as f32 * 0.999;
                let g = gait(&run(speed, u * TAU));
                // In the world the body moved speed * time forward since landing.
                let t = u / rate;
                let world = ball(&g, 1) + Vec3::new(0.0, 0.0, -speed * t);
                assert!((world.y - SOLE).abs() < 1e-3, "{speed} m/s: the ball sits on the ground ({})", world.y);
                if let Some(p) = prev { assert!(world.distance(p) < 0.02, "{speed} m/s: the planted foot slid {}", world.distance(p)); }
                prev = Some(world);
            }
        }
    }

    #[test]
    fn the_stride_follows_the_speed() {
        let (slow, fast) = (contact(3.0), contact(11.2));
        assert!(slow.1 > fast.1 + 0.1, "a sprinter spends less of the stride on the ground");
        assert!(steps(11.2) > steps(3.0) + 0.8 && steps(11.2) < 4.5, "the step rate rises, but stays a runner's");
        // Swinging, the foot lifts higher at a sprint than a jog.
        let peak = |speed: f32| (0..40).map(|k| gait(&run(speed, k as f32 / 40.0 * TAU)).ankles[0].y).fold(f32::MIN, f32::max);
        assert!(peak(11.2) > peak(3.5) + 0.15);
    }

    #[test]
    fn the_feet_land_near_the_middle_and_the_legs_reach_them() {
        for k in 0..32 {
            let g = gait(&run(9.0, k as f32 / 32.0 * TAU));
            for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
                let hip = g.pelvis.transform_point3(Vec3::new(side * HIP.x, HIP.y, HIP.z));
                assert!(hip.distance(g.ankles[s]) <= REACH + 1e-3, "phase {k}: a leg overreaches");
                assert!(g.ankles[s].x * side > 0.0 && g.ankles[s].x.abs() < HIP.x, "phase {k}: feet track inside the hips");
            }
        }
    }

    #[test]
    fn the_gait_is_continuous_and_a_backpedal_plants_its_feet_too() {
        let mut prev = gait(&run(9.0, 0.0));
        for k in 1..=400 {
            let g = gait(&run(9.0, k as f32 / 400.0 * TAU));
            for s in 0..2 { assert!(g.ankles[s].distance(prev.ankles[s]) < 0.05, "jump at {k}"); }
            assert!(g.pelvis.w_axis.distance(prev.pelvis.w_axis) < 0.01);
            prev = g;
        }
        // Backpedalling at 6 m/s (the body backing away along +Z): the
        // planted foot stays put in the world, drifting toward the body's
        // front as it backs off. Phase -0.06 of a turn comes before -0.02.
        let back = |phase: f32| gait(&GaitIn { vel: Vec3::new(0.0, 0.0, 6.0), back: 1.0, ..run(6.0, phase) });
        let (a, b) = (back(-0.02 * TAU), back(-0.06 * TAU));
        let rate = steps(6.0) * 0.5;
        let (wa, wb) = (ball(&a, 1).z - 6.0 * 0.02 / rate, ball(&b, 1).z - 6.0 * 0.06 / rate);
        assert!((ball(&a, 1).y - SOLE).abs() < 1e-3 && (wa - wb).abs() < 0.02, "backpedal plants: {wa} {wb}");
    }

    #[test]
    fn a_hop_tucks_the_legs_under_the_body_where_a_flight_lets_them_hang() {
        let air = |hop: f32, up: f32| gait(&GaitIn { vel: Vec3::new(0.0, up, -11.0), ground: 0.0, hop, ..run(11.0, 0.0) });
        let (hang, top) = (air(0.0, 0.0), air(1.0, 0.0));
        for (s, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let hip = top.pelvis.transform_point3(Vec3::new(side * HIP.x, HIP.y, HIP.z));
            assert!(top.ankles[s].y > hang.ankles[s].y + 0.03, "tucked: {} over {}", top.ankles[s].y, hang.ankles[s].y);
            assert!(hip.distance(top.ankles[s]) < 0.85 * (THIGH + SHIN), "the knees bend: {}", hip.distance(top.ankles[s]));
        }
        // Leaving the ground, the feet are lower than at the top.
        let rising = air(1.0, 5.5);
        assert!(rising.ankles[0].y < top.ankles[0].y - 0.05);
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    #[test]
    #[ignore]
    fn print_cycle() {
        let speed: f32 = std::env::var("SPEED").ok().and_then(|s| s.parse().ok()).unwrap_or(9.0);
        let (lc, share) = contact(speed);
        println!("speed {speed} steps {} reach {lc} share {share}", steps(speed));
        for k in 0..40 {
            let u = k as f32 / 40.0;
            let g = gait(&GaitIn { vel: Vec3::new(0.0, 0.0, -speed), phase: u * TAU, ground: 1.0, ski: 0.0, back: 0.0, hop: 0.0, bound: 0.0, mass: 0.5, jetting: false, time: 0.0, seed: 0.0 });
            let p = g.pelvis.w_axis;
            println!("{u:.3} pelvis y {:.3} x {:.3}  R ankle ({:.2},{:.2},{:.2}) L ankle ({:.2},{:.2},{:.2})", p.y, p.x,
                g.ankles[1].x, g.ankles[1].y, g.ankles[1].z, g.ankles[0].x, g.ankles[0].y, g.ankles[0].z);
        }
    }
}
