//! Ragdolls for bodies going down (dead, or tackled): render-only, on each
//! client. When a player goes down, their pose at that moment becomes a set
//! of points (pelvis, neck, head, the back, shoulders, elbows, hands, hips,
//! knees and ankles) joined by bones that keep their lengths. The points fly
//! with the body's momentum and a push the way it was knocked, fall under
//! gravity and lie however they land on the ground, the joints bending only
//! as joints do: a torso that holds its shape, knees that fold forward, a
//! neck and limbs that can't fold through the body.
//!
//! The server's body (`sim::World::fall_body`) still decides where a body
//! comes to rest, and what it dropped lies there; the ragdoll is kept within
//! reach of it and out of the walls it struck, but never moves it.
use crate::player_model::{Pose, STATURE};
use glam::{Mat4, Vec3};

const PELVIS: usize = 0;
const NECK: usize = 1;
const HEAD: usize = 2;
const SHOULDER: [usize; 2] = [3, 4];
const ELBOW: [usize; 2] = [5, 6];
const HAND: [usize; 2] = [7, 8];
const HIP: [usize; 2] = [9, 10];
const KNEE: [usize; 2] = [11, 12];
const ANKLE: [usize; 2] = [13, 14];
/// The jetpack's middle: out of the shoulders' plane, so the torso is a
/// solid that can't fold flat.
const BACK: usize = 15;
pub const POINTS: usize = 16;
/// How far each point keeps from the ground (m): a limb's half-thickness in
/// the pose, drawn `STATURE` times bigger.
const RADIUS: [f32; POINTS] = [0.14, 0.11, 0.13, 0.1, 0.1, 0.07, 0.07, 0.06, 0.06, 0.1, 0.1, 0.08, 0.08, 0.08, 0.08, 0.15];

/// Simulation step and constraint passes per step.
const STEP: f32 = 1.0 / 120.0;
const PASSES: usize = 6;
/// Velocity kept per step in the air, and the share of sliding a point in
/// contact with the ground loses per step.
const AIR_KEEP: f32 = 0.998;
const GROUND_GRIP: f32 = 0.35;
/// The push that tips a body over, at the head (m/s; less lower down), the
/// way it was knocked: a tackle hits harder.
const TOPPLE: f32 = 2.4;
const TACKLE_TOPPLE: f32 = 3.4;
/// A ragdoll whose points all move slower than this for `SLEEP_AFTER`
/// seconds stops being simulated.
const SLEEP_SPEED: f32 = 0.05;
const SLEEP_AFTER: f32 = 0.4;
/// Farthest the ragdoll's middle may get from the server's body before it
/// is pulled back (m): a body's length, so a fall never fights it.
const LEASH: f32 = 1.6 * STATURE;
/// A point's floor is looked up again once it has moved this far across
/// (m): a map query per point per step would cost a frame's worth with a
/// few bodies down.
const FLOOR_REUSE: f32 = 0.2;

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Keeps its length: a bone, or the torso's frame.
    Bone,
    /// Pulled toward its length a little each pass: the neck.
    Soft(f32),
    /// Never shorter than this: a joint's limit, or a limb kept out of the body.
    AtLeast,
}

#[derive(Clone, Copy, Debug)]
struct Link { a: usize, b: usize, length: f32, kind: Kind }

/// Joints and their limits for a body seeded at `p`.
fn links(p: &[Vec3; POINTS]) -> Vec<Link> {
    let mut out = Vec::new();
    let bone = |a: usize, b: usize, kind: Kind, out: &mut Vec<Link>| out.push(Link { a, b, length: p[a].distance(p[b]), kind });
    // The torso: pelvis, neck, shoulders, hips and back, every pair held.
    let torso = [PELVIS, NECK, SHOULDER[0], SHOULDER[1], HIP[0], HIP[1], BACK];
    for (k, &a) in torso.iter().enumerate() { for &b in &torso[k + 1..] { bone(a, b, Kind::Bone, &mut out); } }
    bone(NECK, HEAD, Kind::Bone, &mut out);
    for &t in &[SHOULDER[0], SHOULDER[1], BACK] { bone(HEAD, t, Kind::Soft(0.25), &mut out); }
    let at_least = |a: usize, b: usize, length: f32, out: &mut Vec<Link>| out.push(Link { a, b, length, kind: Kind::AtLeast });
    for s in 0..2 {
        bone(SHOULDER[s], ELBOW[s], Kind::Bone, &mut out);
        bone(ELBOW[s], HAND[s], Kind::Bone, &mut out);
        bone(HIP[s], KNEE[s], Kind::Bone, &mut out);
        bone(KNEE[s], ANKLE[s], Kind::Bone, &mut out);
        // An elbow and a knee fold only so far; hands, elbows and knees stay
        // out of the torso; feet keep from the pelvis.
        at_least(SHOULDER[s], HAND[s], 0.24, &mut out);
        at_least(HIP[s], ANKLE[s], 0.36, &mut out);
        for (limb, room) in [(HAND[s], 0.26), (ELBOW[s], 0.2)] {
            at_least(limb, BACK, room, &mut out);
            at_least(limb, PELVIS, room, &mut out);
        }
        at_least(KNEE[s], SHOULDER[s], 0.5, &mut out);
        at_least(KNEE[s], BACK, 0.42, &mut out);
        at_least(ANKLE[s], PELVIS, 0.42, &mut out);
    }
    at_least(KNEE[0], KNEE[1], 0.12, &mut out);
    at_least(ANKLE[0], ANKLE[1], 0.12, &mut out);
    at_least(HAND[0], HAND[1], 0.1, &mut out);
    out
}

/// A body going down, in world space.
#[derive(Clone, Debug)]
pub struct Ragdoll {
    pos: [Vec3; POINTS],
    prev: [Vec3; POINTS],
    links: Vec<Link>,
    /// Each point's floor height, and where (x, z) it was looked up.
    floors: [(f32, f32, f32); POINTS],
    /// Unsimulated time, and how long it has lain still.
    acc: f32,
    still: f32,
}

impl Ragdoll {
    /// From `pose` (in the frame `frame` maps to the world: the body's
    /// place and facing), moving at `vel`, knocked toward `knock` (a
    /// horizontal direction in the world).
    pub fn new(pose: &Pose, frame: Mat4, vel: Vec3, knock: Vec3, tackled: bool) -> Self {
        let local = pose_points(pose);
        let pos = local.map(|q| frame.transform_point3(q));
        let feet = pos[ANKLE[0]].y.min(pos[ANKLE[1]].y);
        let push = if tackled { TACKLE_TOPPLE } else { TOPPLE };
        let knock = Vec3::new(knock.x, 0.0, knock.z).normalize_or(Vec3::ZERO);
        let mut prev = pos;
        for k in 0..POINTS {
            // Tipped about the feet: the higher the point, the harder the push;
            // the knees buckle the same way.
            let up = ((pos[k].y - feet) / (1.8 * STATURE)).clamp(0.0, 1.0);
            let buckle = if KNEE.contains(&k) { 0.5 } else { 0.0 };
            let v = vel + knock * push * (up + buckle);
            prev[k] = pos[k] - v * STEP;
        }
        Self { links: links(&pos), pos, prev, floors: [(f32::NAN, 0.0, 0.0); POINTS], acc: 0.0, still: 0.0 }
    }

    /// The points, in the world.
    pub fn points(&self) -> &[Vec3; POINTS] { &self.pos }

    /// Advance by `dt`. `ground(p)` is the floor's height under `p`;
    /// `clear(a, b)` how far a ray from `a` toward `b` gets; `body` is the
    /// server's body (its collision sphere's centre).
    pub fn step(&mut self, dt: f32, body: Vec3, ground: &dyn Fn(Vec3) -> f32, clear: &dyn Fn(Vec3, Vec3) -> f32) {
        if self.still >= SLEEP_AFTER { return; }
        let gravity = peakrunner_core::defs::movement().gravity;
        self.acc = (self.acc + dt).min(0.1);
        let middle_before = self.middle();
        let mut fastest = 0.0f32;
        while self.acc >= STEP {
            self.acc -= STEP;
            for k in 0..POINTS {
                let v = (self.pos[k] - self.prev[k]) * AIR_KEEP;
                self.prev[k] = self.pos[k];
                self.pos[k] += v - Vec3::Y * gravity * STEP * STEP;
            }
            for _ in 0..PASSES {
                self.hold_joints();
                self.knees_forward();
            }
            for k in 0..POINTS {
                let (h, x, z) = self.floors[k];
                let q = self.pos[k];
                let floor = if h.is_nan() || (q.x - x).hypot(q.z - z) > FLOOR_REUSE {
                    let h = ground(q);
                    self.floors[k] = (h, q.x, q.z);
                    h
                } else { h } + RADIUS[k] * STATURE;
                if self.pos[k].y < floor {
                    self.pos[k].y = floor;
                    // Resting on the ground: no bounce, and it grips.
                    self.prev[k].y = self.pos[k].y;
                    let slide = Vec3::new(self.pos[k].x - self.prev[k].x, 0.0, self.pos[k].z - self.prev[k].z);
                    self.prev[k] += slide * GROUND_GRIP;
                }
                fastest = fastest.max(self.pos[k].distance(self.prev[k]) / STEP);
            }
        }
        // The torso doesn't pass through what it struck: a wall stops the
        // whole body.
        let middle = self.middle();
        let travel = middle - middle_before;
        if travel.length() > 1e-4 {
            let free = clear(middle_before, middle);
            if free + 1e-3 < travel.length() {
                let back = travel * (1.0 - (free - 0.05).max(0.0) / travel.length());
                // Back out of it, keeping only the fall.
                for k in 0..POINTS {
                    let v = self.pos[k] - self.prev[k];
                    self.pos[k] -= back;
                    self.prev[k] = self.pos[k] - Vec3::Y * v.y;
                }
            }
        }
        // Never far from the server's body.
        let off = Vec3::new(self.middle().x - body.x, 0.0, self.middle().z - body.z);
        if off.length() > LEASH {
            let pull = off * (1.0 - LEASH / off.length());
            for k in 0..POINTS { self.pos[k] -= pull; self.prev[k] -= pull; }
        }
        self.still = if fastest < SLEEP_SPEED { self.still + dt } else { 0.0 };
    }

    fn middle(&self) -> Vec3 { (self.pos[PELVIS] + self.pos[BACK]) * 0.5 }

    fn hold_joints(&mut self) {
        for l in &self.links {
            let d = self.pos[l.b] - self.pos[l.a];
            let len = d.length();
            if len < 1e-6 { continue; }
            let error = match l.kind {
                Kind::Bone => len - l.length,
                Kind::Soft(k) => (len - l.length) * k,
                Kind::AtLeast if len < l.length => len - l.length,
                Kind::AtLeast => continue,
            };
            let fix = d / len * (error * 0.5);
            self.pos[l.a] += fix;
            self.pos[l.b] -= fix;
        }
    }

    /// Knees bend forward only: a knee that ends up behind the line from its
    /// hip to its ankle is put back in front of it.
    fn knees_forward(&mut self) {
        let up = self.pos[NECK] - self.pos[PELVIS];
        let right = self.pos[SHOULDER[1]] - self.pos[SHOULDER[0]];
        let ahead = up.cross(right).normalize_or(Vec3::ZERO);
        if ahead == Vec3::ZERO { return; }
        for s in 0..2 {
            let mid = (self.pos[HIP[s]] + self.pos[ANKLE[s]]) * 0.5;
            let along = (self.pos[KNEE[s]] - mid).dot(ahead);
            if along < 0.0 { self.pos[KNEE[s]] -= ahead * (2.0 * along); }
        }
    }
}

/// The points of `pose` (in its own frame, its body transform applied).
fn pose_points(pose: &Pose) -> [Vec3; POINTS] {
    let at = |q: Vec3| pose.body.transform_point3(q);
    let mut p = [Vec3::ZERO; POINTS];
    p[PELVIS] = at(pose.pelvis);
    p[NECK] = at(pose.chest.transform_point3(Vec3::new(0.0, 0.58, 0.0)));
    p[HEAD] = at(pose.head.transform_point3(Vec3::new(0.0, 0.155, 0.0)));
    p[BACK] = at(pose.chest.transform_point3(Vec3::new(0.0, 0.30, 0.25)));
    for s in 0..2 {
        p[SHOULDER[s]] = at(pose.shoulders[s]);
        p[ELBOW[s]] = at(pose.elbows[s]);
        p[HAND[s]] = at(pose.hands[s]);
        p[HIP[s]] = at(pose.hips[s]);
        p[KNEE[s]] = at(pose.knees[s]);
        p[ANKLE[s]] = at(pose.ankles[s]);
    }
    p
}

/// A frame at `origin` whose Y runs along `y`, its X as near `x` as that
/// allows.
fn frame(origin: Vec3, y: Vec3, x: Vec3) -> Mat4 {
    let y = y.normalize_or(Vec3::Y);
    let x = (x - y * x.dot(y)).normalize_or(y.any_orthonormal_vector());
    let z = x.cross(y);
    Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), origin.extend(1.0))
}

/// The pose a ragdoll's `points` (world) make, in the frame `frame` maps to
/// the world (the body's place and facing): what `player_model` draws.
pub fn pose_of(points: &[Vec3; POINTS], frame_to_world: Mat4) -> Pose {
    let to_local = frame_to_world.inverse();
    let p = points.map(|q| to_local.transform_point3(q));
    let right = p[SHOULDER[1]] - p[SHOULDER[0]];
    let chest = frame(p[PELVIS], p[NECK] - p[PELVIS], right);
    let pelvis_frame = frame(p[PELVIS], (p[SHOULDER[0]] + p[SHOULDER[1]]) * 0.5 - p[PELVIS], p[HIP[1]] - p[HIP[0]]);
    let head = frame(p[NECK], p[HEAD] - p[NECK], right);
    let hip_right = pelvis_frame.transform_vector3(Vec3::X);
    let mut feet = [Mat4::IDENTITY; 2];
    let mut wrists = [Mat4::IDENTITY; 2];
    for s in 0..2 {
        // The foot square to the shin, toes relaxed down and away.
        feet[s] = frame(p[ANKLE[s]], p[KNEE[s]] - p[ANKLE[s]], hip_right) * Mat4::from_rotation_x(-0.45);
        let along = (p[HAND[s]] - p[ELBOW[s]]).normalize_or(Vec3::NEG_Y);
        wrists[s] = Mat4::from_translation(p[HAND[s]]) * Mat4::from_quat(glam::Quat::from_rotation_arc(Vec3::NEG_Y, along));
    }
    Pose {
        body: Mat4::IDENTITY, pelvis: p[PELVIS], pelvis_frame, chest, head,
        aim: chest * Mat4::from_translation(Vec3::new(0.13, 0.30, -0.3)),
        hips: [p[HIP[0]], p[HIP[1]]], knees: [p[KNEE[0]], p[KNEE[1]]], ankles: [p[ANKLE[0]], p[ANKLE[1]]], feet,
        shoulders: [p[SHOULDER[0]], p[SHOULDER[1]]], elbows: [p[ELBOW[0]], p[ELBOW[1]]], hands: [p[HAND[0]], p[HAND[1]]],
        wrists, grips: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_model::{pose, PoseInput};

    fn standing() -> Pose {
        let mut w = peakrunner_core::sim::World::new();
        w.start_match(true);
        let mut p = w.players[0].clone();
        p.alive = true;
        p.yaw = 0.0;
        pose(&PoseInput { hold: crate::player_model::Hold::Free, ..PoseInput::from_player(&p, 0.0) })
    }

    fn flat(_: Vec3) -> f32 { 0.0 }
    fn open(a: Vec3, b: Vec3) -> f32 { a.distance(b) }

    /// Knocked over on flat ground, a body falls and lies flat on it, every
    /// bone its own length and every joint inside its limits.
    #[test]
    fn a_knocked_body_lies_down_on_the_ground_in_one_piece() {
        let start = standing();
        // The root (the collision sphere's centre) 0.52 m up, as in play.
        let frame = Mat4::from_translation(Vec3::Y * 0.52);
        let mut r = Ragdoll::new(&start, frame, Vec3::ZERO, Vec3::NEG_Z, false);
        let lengths: Vec<f32> = r.links.iter().map(|l| r.pos[l.a].distance(r.pos[l.b])).collect();
        for _ in 0..240 { r.step(1.0 / 60.0, Vec3::Y * 0.52, &flat, &open); }
        let top = r.points().iter().map(|q| q.y).fold(f32::MIN, f32::max);
        assert!(top < 0.6, "lying down, highest point {top}");
        assert!(r.points()[HEAD].z < -0.8, "it fell the way it was knocked: head at {:?}", r.points()[HEAD]);
        for (l, &was) in r.links.iter().zip(&lengths) {
            let now = r.pos[l.a].distance(r.pos[l.b]);
            match l.kind {
                Kind::Bone => assert!((now - was).abs() < 0.03, "a bone kept its length: {l:?} {now}"),
                Kind::AtLeast => assert!(now > l.length - 0.03, "a joint held its limit: {l:?} {now}"),
                Kind::Soft(_) => {}
            }
        }
        for q in r.points() { assert!(q.y > -0.01, "nothing under the ground: {q:?}"); }
        assert!(r.still >= SLEEP_AFTER, "it comes to rest");
        let drawn = pose_of(r.points(), frame);
        assert!(drawn.chest.is_finite() && drawn.head.is_finite() && drawn.feet.iter().all(|f| f.is_finite()));
    }

    /// Falling with momentum it travels with it; a wall it strikes stops it.
    #[test]
    fn a_body_flies_with_its_momentum_and_a_wall_stops_it() {
        let start = standing();
        let frame = Mat4::from_translation(Vec3::Y * 6.0);
        let mut free = Ragdoll::new(&start, frame, Vec3::new(6.0, 0.0, 0.0), Vec3::X, false);
        for _ in 0..30 { free.step(1.0 / 60.0, free.middle(), &flat, &open); }
        assert!(free.middle().x > 2.5, "it carried on: {:?}", free.middle());
        let wall = |a: Vec3, b: Vec3| if b.x > 1.0 && a.x <= 1.0 { ((1.0 - a.x) / (b.x - a.x).max(1e-6) * a.distance(b)).max(0.0) } else { a.distance(b) };
        let mut stopped = Ragdoll::new(&start, frame, Vec3::new(6.0, 0.0, 0.0), Vec3::X, false);
        for _ in 0..60 { stopped.step(1.0 / 60.0, stopped.middle(), &flat, &wall); }
        assert!(stopped.middle().x < 1.0, "the wall held it: {:?}", stopped.middle());
    }
}
