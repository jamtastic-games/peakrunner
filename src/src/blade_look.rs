//! How a blade (`sim::blade`) looks in the hand: where the hilt is and which
//! way the blade points through each swing, the same on a body and in first
//! person, its glow (a color per player), and the smear a cut leaves.
//!
//! A pose is in the body's frame: the middle of the shoulders at the origin,
//! x right, y up, -z ahead. The blade runs along the hilt frame's +Y. Every
//! pose is a pure function of the swing's state, so a cut's trail is drawn
//! by posing the blade a little earlier in the swing.
use glam::{Mat4, Quat, Vec3};
use peakrunner_core::sim::blade::{self, State};

use crate::drawlist::{EmitDraw, LitDraw, MeshId};

/// The hilt's length and the blade's beyond it (m).
pub const HILT: f32 = 0.26;
pub const BLADE: f32 = 1.0;

/// A pose: where the hilt's middle is and the blade's direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose { pub hilt: Vec3, pub dir: Vec3 }

/// A direction `yaw` degrees right of ahead and `pitch` degrees up.
fn toward(yaw: f32, pitch: f32) -> Vec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    Vec3::new(y.sin() * p.cos(), p.sin(), -y.cos() * p.cos())
}

fn smooth(x: f32) -> f32 { let x = x.clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) }

/// A key pose: the hilt, and the blade's yaw and pitch (degrees).
#[derive(Clone, Copy, Debug)]
struct Key { hilt: Vec3, yaw: f32, pitch: f32 }

impl Key {
    fn pose(self) -> Pose { Pose { hilt: self.hilt, dir: toward(self.yaw, self.pitch) } }
    fn lerp(self, o: Key, t: f32) -> Key {
        Key { hilt: self.hilt.lerp(o.hilt, t), yaw: self.yaw + (o.yaw - self.yaw) * t, pitch: self.pitch + (o.pitch - self.pitch) * t }
    }
}

/// Held ready: low in front, the blade up and a little across.
const READY: Key = Key { hilt: Vec3::new(0.2, -0.36, -0.36), yaw: -12.0, pitch: 58.0 };
/// On guard: across the body, the tip up to the left.
const GUARD: Key = Key { hilt: Vec3::new(0.26, -0.12, -0.44), yaw: -80.0, pitch: 24.0 };

/// A slash's hilt for a blade swung `yaw` degrees across: out in front on
/// an arc round the body.
fn arc(yaw: f32, y: f32, reach: f32) -> Vec3 {
    let a = yaw.to_radians();
    Vec3::new(reach * a.sin(), y, -0.18 - (reach - 0.1) * a.cos())
}

/// Where a swing starts its cut and ends it, and its path in between.
fn cut_key(kind: u8, f: f32, charge: f32) -> Key {
    let f = f.clamp(0.0, 1.0);
    match kind {
        blade::SLASH_LEFT | blade::SLASH_RIGHT => {
            let (a, b) = if kind == blade::SLASH_LEFT { (78.0, -78.0) } else { (-78.0, 78.0) };
            let yaw = a + (b - a) * f;
            Key { hilt: arc(yaw, -0.08, 0.46), yaw, pitch: 8.0 - 6.0 * f }
        }
        blade::CHOP => {
            let pitch = 82.0 - 122.0 * f;
            Key { hilt: Vec3::new(0.08, 0.3 - 0.62 * f, -0.2 - 0.32 * f), yaw: -4.0, pitch }
        }
        blade::POWER => {
            let yaw = 84.0 - 168.0 * f;
            let lift = 0.32 + 0.1 * charge;
            Key { hilt: arc(yaw, lift - 0.62 * f, 0.5), yaw, pitch: 52.0 - 80.0 * f }
        }
        blade::WHIRL => {
            let yaw = 20.0 + 360.0 * f;
            Key { hilt: arc(yaw, -0.1, 0.52), yaw, pitch: 4.0 }
        }
        blade::LUNGE => Key { hilt: Vec3::new(0.1, -0.1, -0.24 - 0.62 * smooth(f * 1.6)), yaw: 0.0, pitch: -2.0 },
        blade::SLAM => {
            let pitch = 88.0 - 152.0 * f;
            Key { hilt: Vec3::new(0.04, 0.46 - 0.92 * f, -0.12 - 0.48 * f), yaw: 0.0, pitch }
        }
        _ => READY,
    }
}

/// The blade's pose for a player's blade state, `sub` steps past its last
/// step (0 to 1: drawn between steps).
pub fn pose(b: &State, sub: f32) -> Pose { key(b, b.t as f32 + sub).pose() }

/// The pose `t` steps into the swing (fractions allowed).
fn key(b: &State, t: f32) -> Key {
    if b.reel > 0 {
        // Reeling: knocked off line, the tip dropping and wavering.
        let w = (b.reel as f32 * 0.9).sin() * 8.0;
        return Key { hilt: READY.hilt + Vec3::new(0.08, -0.06, 0.12), yaw: READY.yaw + 30.0 + w, pitch: 20.0 + w };
    }
    if b.guard { return GUARD; }
    if b.charging() {
        // Raised high over the right shoulder, cocked further back as the
        // power builds (still in sight in first person).
        let c = b.charge();
        return Key { hilt: Vec3::new(0.3, 0.1 + 0.1 * c, -0.3 + 0.08 * c), yaw: 100.0 + 25.0 * c, pitch: 58.0 + 14.0 * c };
    }
    let Some(def) = blade::swing_def(b.swing) else { return READY };
    let step = |s: f32| s / peakrunner_core::sim::STEP;
    let (c0, c1, end) = (step(def.cut[0]), step(def.cut[1]).max(step(def.cut[0]) + 1.0), step(def.time));
    let charge = b.charge();
    if t <= c0 {
        READY.lerp(cut_key(b.swing, 0.0, charge), smooth(t / c0.max(1.0)))
    } else if t <= c1 {
        cut_key(b.swing, (t - c0) / (c1 - c0), charge)
    } else {
        let last = cut_key(b.swing, 1.0, charge);
        // Back to ready the short way round (a whirl ends a turn on).
        let ready = Key { yaw: READY.yaw + 360.0 * ((last.yaw - READY.yaw) / 360.0).round(), ..READY };
        last.lerp(ready, smooth((t - c1) / (end - c1).max(1.0)))
    }
}

/// Earlier poses of a cut in progress, newest first, for its smear: none
/// outside a cut.
pub fn trail(b: &State, sub: f32, count: usize) -> Vec<Pose> {
    let Some(def) = blade::swing_def(b.swing) else { return Vec::new() };
    if b.reel > 0 || b.guard { return Vec::new(); }
    let step = |s: f32| s / peakrunner_core::sim::STEP;
    let (c0, c1) = (step(def.cut[0]), step(def.cut[1]));
    let now = b.t as f32 + sub;
    if now < c0 || now > c1 + 3.0 { return Vec::new(); }
    let gap = if b.swing == blade::WHIRL { 0.7 } else { 0.3 };
    (1..=count).map(|k| now - k as f32 * gap).filter(|&t| t >= c0 - 1.0).map(|t| key(b, t).pose()).collect()
}

/// The hilt's frame for a pose: at the hilt, its +Y along the blade.
pub fn frame(p: Pose) -> Mat4 {
    Mat4::from_translation(p.hilt) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, p.dir.normalize_or(Vec3::Y)))
}

/// Where the hands hold the hilt, in its frame: the right below the guard,
/// the left under it.
pub const RIGHT_HAND: Vec3 = Vec3::new(0.0, 0.05, 0.0);
pub const LEFT_HAND: Vec3 = Vec3::new(0.0, -0.08, 0.0);

/// A blade's glow: one of six, by who holds it.
pub fn color(who: u32) -> Vec3 {
    const GLOWS: [Vec3; 6] = [Vec3::new(0.35, 0.85, 1.0), Vec3::new(0.4, 1.0, 0.45), Vec3::new(0.78, 0.45, 1.0),
        Vec3::new(1.0, 0.32, 0.26), Vec3::new(1.0, 0.72, 0.22), Vec3::new(1.0, 0.38, 0.86)];
    GLOWS[(who as usize) % GLOWS.len()]
}

/// The hilt and the lit blade at `hilt` (the hilt's frame in the world or
/// the view), glowing `glow` (a power swing's charge glows hotter), and a
/// smear of the cut behind it (`trail`: earlier hilt frames).
pub fn push(lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, hilt: Mat4, glow: Vec3, heat: f32, trail: &[Mat4], first_person: bool) {
    let steel = Vec3::new(0.32, 0.34, 0.37);
    let dark = Vec3::new(0.08, 0.085, 0.095);
    let at = |p: Vec3, s: Vec3| hilt * Mat4::from_translation(p) * Mat4::from_scale(s);
    lit.push(LitDraw { mesh: MeshId::Capsule, model: at(Vec3::ZERO, Vec3::new(0.042, HILT, 0.042)), color: dark, emit: 0.0, mode: 0.0 });
    for y in [-0.06, 0.02] {
        lit.push(LitDraw { mesh: MeshId::Disc, model: at(Vec3::new(0.0, y, 0.0), Vec3::new(0.05, 0.02, 0.05)), color: steel, emit: 0.0, mode: 0.0 });
    }
    lit.push(LitDraw { mesh: MeshId::Disc, model: at(Vec3::new(0.0, HILT * 0.5, 0.0), Vec3::new(0.062, 0.03, 0.062)), color: steel, emit: 0.0, mode: 0.0 });
    lit.push(LitDraw { mesh: MeshId::Disc, model: at(Vec3::new(0.0, HILT * 0.5 + 0.02, 0.0), Vec3::new(0.035, 0.012, 0.035)), color: glow, emit: 1.2, mode: 0.0 });
    // The blade: a white-hot core in a colored glow.
    let mid = HILT * 0.5 + BLADE * 0.5;
    let core = if first_person { 1.6 } else { 2.6 } * (1.0 + heat);
    lit.push(LitDraw { mesh: MeshId::Capsule, model: at(Vec3::new(0.0, mid, 0.0), Vec3::new(0.024, BLADE, 0.024)),
        color: Vec3::ONE.lerp(glow, 0.25), emit: core, mode: 0.0 });
    let a = if first_person { 0.42 } else { 0.6 };
    emit.push(EmitDraw { mesh: MeshId::Capsule, model: at(Vec3::new(0.0, mid, 0.0), Vec3::new(0.07 + 0.03 * heat, BLADE + 0.05, 0.07 + 0.03 * heat)),
        color: [glow.x, glow.y, glow.z, a] });
    // The smear: the blade where it just was, fading.
    for (k, f) in trail.iter().enumerate() {
        let fade = a * 0.55 * (1.0 - (k as f32 + 1.0) / (trail.len() as f32 + 1.0));
        emit.push(EmitDraw { mesh: MeshId::Capsule, model: *f * Mat4::from_translation(Vec3::new(0.0, mid, 0.0))
            * Mat4::from_scale(Vec3::new(0.06, BLADE, 0.06)), color: [glow.x, glow.y, glow.z, fade] });
    }
}

/// First person: the body-frame pose brought into the view (the camera at
/// the origin looking down -z), the shoulders a little below and behind
/// the eye, so the blade sweeps across the screen.
pub fn view_frame(p: Pose, bob: Vec3) -> Mat4 {
    // The hands held out a little further than on the body, and the blade
    // tipped up, so a slash crosses the screen instead of pointing into it.
    const SHOULDERS: Vec3 = Vec3::new(0.06, -0.1, -0.16);
    let tilt = Quat::from_rotation_x(0.42);
    frame(Pose { hilt: p.hilt * Vec3::new(1.05, 0.75, 1.15) + SHOULDERS + bob, dir: tilt * p.dir })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swinging(kind: u8, t: u16) -> State { State { swing: kind, t, ..State::default() } }

    #[test]
    fn a_slash_sweeps_across_and_comes_back_to_ready() {
        let def = blade::swing_def(blade::SLASH_LEFT).unwrap();
        let steps = |s: f32| (s / peakrunner_core::sim::STEP).round() as u16;
        let start = pose(&swinging(blade::SLASH_LEFT, steps(def.cut[0])), 0.0);
        let end = pose(&swinging(blade::SLASH_LEFT, steps(def.cut[1])), 0.0);
        assert!(start.dir.x > 0.8 && end.dir.x < -0.8, "right to left: {start:?} {end:?}");
        let done = pose(&swinging(blade::SLASH_LEFT, steps(def.time)), 0.0);
        assert!(done.hilt.distance(READY.hilt) < 0.01 && done.dir.dot(READY.pose().dir) > 0.999);
        // Every pose is continuous through the swing.
        for kind in [blade::SLASH_LEFT, blade::SLASH_RIGHT, blade::CHOP, blade::POWER, blade::WHIRL, blade::LUNGE, blade::SLAM] {
            let def = blade::swing_def(kind).unwrap();
            let mut last = pose(&swinging(kind, 0), 0.0);
            for t in 1..=steps(def.time) * 4 {
                let p = pose(&swinging(kind, t / 4), (t % 4) as f32 / 4.0);
                assert!(p.hilt.distance(last.hilt) < 0.2 && p.dir.dot(last.dir) > 0.6, "kind {kind} jumps at {t}: {last:?} -> {p:?}");
                last = p;
            }
        }
    }

    #[test]
    fn a_cut_leaves_a_smear_and_nothing_else_does() {
        let def = blade::swing_def(blade::POWER).unwrap();
        let mid = ((def.cut[0] + def.cut[1]) * 0.5 / peakrunner_core::sim::STEP) as u16;
        assert_eq!(trail(&swinging(blade::POWER, mid), 0.0, 5).len(), 5);
        assert!(trail(&swinging(blade::POWER, 1), 0.0, 5).is_empty());
        assert!(trail(&State::default(), 0.0, 5).is_empty());
        assert!(trail(&State { guard: true, ..State::default() }, 0.0, 5).is_empty());
    }
}
