//! Original synthesized sound palette, a fixed-voice stereo mixer and the
//! director that turns world state into cues. Everything here is pure and
//! platform-free: `audio.rs` feeds the mixer to rodio natively and to a
//! WebAudio script node in the browser, so both hear the same thing.
//!
//! Every sound is generated from deterministic noise, oscillators and simple
//! filters; nothing is sampled from another game.

use glam::Vec3;
use std::sync::Arc;

use crate::sim::World;

const TAU: f32 = std::f32::consts::TAU;

/// Deterministic white noise in [-1, 1].
#[derive(Clone, Copy)]
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0
    }
}

/// One-pole low-pass coefficient for a cutoff in Hz.
fn lp_coef(cutoff: f32, sr: f32) -> f32 { 1.0 - (-TAU * cutoff.clamp(20.0, sr * 0.45) / sr).exp() }

/// Two-pole resonator (band-pass) used for tubes, bells and metal.
#[derive(Clone, Copy, Default)]
struct Reso { a1: f32, a2: f32, y1: f32, y2: f32, g: f32 }
impl Reso {
    fn new(freq: f32, q: f32, sr: f32) -> Self {
        let r = (-std::f32::consts::PI * freq / (q * sr)).exp();
        Reso { a1: 2.0 * r * (TAU * freq / sr).cos(), a2: -r * r, y1: 0.0, y2: 0.0, g: 1.0 - r }
    }
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.g * x + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1; self.y1 = y; y
    }
}

/// Every one-shot cue. Variants give repeated sounds different seeds and
/// pitches so rapid fire never sounds like one sample looping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue {
    DiscFire, DiscReady, ChainShot, GrenadeFire, TurretBullet, TurretPlasma,
    BoomNear, BoomFar, GenBlast, ShieldHit, ShieldDown, HullHit, Footstep, Land,
    Switch, RepairKit, Bounce, Hit, Pain, Death, Flag, Drop, Return,
    CaptureWin, CaptureLoss, Start, End, Splash, Wade,
    // Football and body checks; Warning is the out-of-bounds alert,
    // Incoming a pass on its way to you.
    Whistle, Horn, Tackle, Bump, Pass, Catch, Warning, Incoming,
    // Rifles, the plasma gun and the blaster.
    Laser, Rail, PlasmaFire, BlasterFire,
    // A vent's warning and its eruption (`features::Vent`).
    Rumble, Eruption,
    // The blade (`sim::blade`): a swing's whoosh (a heavy one's deeper), a
    // cut, a blow on a guard and blades clashing.
    BladeSwing, BladeHeavy, BladeCut, BladeBlock, BladeClash,
}

pub const CUES: [Cue; 48] = [
    Cue::DiscFire, Cue::DiscReady, Cue::ChainShot, Cue::GrenadeFire, Cue::TurretBullet,
    Cue::TurretPlasma, Cue::BoomNear, Cue::BoomFar, Cue::GenBlast, Cue::ShieldHit,
    Cue::ShieldDown, Cue::HullHit, Cue::Footstep, Cue::Land, Cue::Switch, Cue::RepairKit,
    Cue::Bounce, Cue::Hit, Cue::Pain, Cue::Death, Cue::Flag, Cue::Drop, Cue::Return,
    Cue::CaptureWin, Cue::CaptureLoss, Cue::Start, Cue::End, Cue::Splash, Cue::Wade,
    Cue::Whistle, Cue::Horn, Cue::Tackle, Cue::Bump, Cue::Pass, Cue::Catch, Cue::Warning, Cue::Incoming,
    Cue::Laser, Cue::Rail, Cue::PlasmaFire, Cue::BlasterFire,
    Cue::Rumble, Cue::Eruption,
    Cue::BladeSwing, Cue::BladeHeavy, Cue::BladeCut, Cue::BladeBlock, Cue::BladeClash,
];

impl Cue {
    fn index(self) -> usize { CUES.iter().position(|c| *c == self).unwrap() }
    pub fn variants(self) -> usize {
        match self {
            Cue::ChainShot | Cue::Footstep => 6,
            Cue::DiscFire | Cue::GrenadeFire | Cue::TurretBullet | Cue::BoomNear
            | Cue::HullHit | Cue::Bounce => 3,
            Cue::ShieldHit | Cue::Wade => 4,
            Cue::Splash => 3,
            Cue::TurretPlasma | Cue::BoomFar | Cue::Land | Cue::Tackle | Cue::Bump | Cue::Catch
            | Cue::PlasmaFire | Cue::BlasterFire | Cue::BladeHeavy | Cue::BladeCut | Cue::BladeBlock => 2,
            Cue::BladeSwing => 3,
            _ => 1,
        }
    }
    /// Higher keeps its voice when the pool is full.
    pub fn priority(self) -> u8 {
        match self {
            Cue::CaptureWin | Cue::CaptureLoss | Cue::Flag | Cue::Drop | Cue::Return
            | Cue::Start | Cue::End | Cue::Death | Cue::Whistle | Cue::Horn => 6,
            Cue::GenBlast | Cue::Hit | Cue::Pain | Cue::RepairKit | Cue::ShieldDown | Cue::Warning
            | Cue::Incoming | Cue::Rumble | Cue::Eruption => 5,
            Cue::DiscFire | Cue::GrenadeFire | Cue::DiscReady | Cue::Switch | Cue::Laser | Cue::Rail
            | Cue::PlasmaFire | Cue::BlasterFire => 4,
            Cue::BoomNear | Cue::BoomFar | Cue::TurretPlasma => 3,
            Cue::Tackle | Cue::Pass | Cue::Catch | Cue::BladeCut | Cue::BladeBlock | Cue::BladeClash => 4,
            Cue::BladeSwing | Cue::BladeHeavy => 3,
            Cue::ChainShot | Cue::TurretBullet | Cue::ShieldHit | Cue::HullHit | Cue::Land | Cue::Splash | Cue::Bump => 2,
            Cue::Footstep | Cue::Bounce | Cue::Wade => 1,
        }
    }
    /// Distance at which a world sound fades to silence.
    pub fn range(self) -> f32 {
        match self {
            Cue::Eruption => 1400.0,
            Cue::Rumble => 900.0,
            Cue::GenBlast => 350.0,
            Cue::BoomFar => 400.0,
            Cue::BoomNear => 150.0,
            Cue::DiscFire | Cue::GrenadeFire | Cue::TurretPlasma | Cue::PlasmaFire | Cue::BlasterFire => 110.0,
            Cue::Laser => 150.0,
            Cue::Rail => 180.0,
            Cue::ChainShot | Cue::TurretBullet => 110.0,
            Cue::ShieldHit | Cue::ShieldDown | Cue::HullHit => 110.0,
            Cue::Footstep | Cue::Wade => 38.0,
            Cue::Land | Cue::Bounce | Cue::Splash => 60.0,
            Cue::Tackle | Cue::Bump | Cue::Pass | Cue::Catch => 80.0,
            Cue::BladeSwing | Cue::BladeHeavy | Cue::BladeCut | Cue::BladeBlock | Cue::BladeClash => 70.0,
            _ => 120.0,
        }
    }
    /// Reverb send scale. Explosions stay short and punchy: a long outdoor
    /// tail would stretch their deep hit into a drawn-out roar.
    fn wet(self) -> f32 {
        match self {
            Cue::BoomNear | Cue::BoomFar | Cue::GenBlast => 0.25,
            Cue::Eruption => 0.5,
            _ => 1.0,
        }
    }
    /// Cap on simultaneous copies, so chaingun spam or footsteps can't
    /// crowd out everything else.
    fn max_voices(self) -> usize {
        match self {
            Cue::ChainShot | Cue::TurretBullet => 8,
            Cue::Footstep | Cue::Wade | Cue::BladeSwing => 4,
            Cue::ShieldHit | Cue::HullHit | Cue::Bounce => 4,
            Cue::BoomNear | Cue::BoomFar => 6,
            _ => 3,
        }
    }
    /// Sim and network event names (`push_event`, `spatial_sounds`).
    pub fn from_event(name: &str) -> Option<Cue> {
        Some(match name {
            "disc" => Cue::DiscFire,
            "disc_ready" => Cue::DiscReady,
            "chain" => Cue::ChainShot,
            "grenade" => Cue::GrenadeFire,
            "boom" => Cue::BoomNear,
            "hit" => Cue::Hit,
            "pain" => Cue::Pain,
            "death" => Cue::Death,
            "flag" => Cue::Flag,
            "drop" => Cue::Drop,
            "return" => Cue::Return,
            "capture_win" => Cue::CaptureWin,
            "capture_loss" => Cue::CaptureLoss,
            "start" => Cue::Start,
            // The round countdown: a click each second, the start cue on GO.
            "count" => Cue::Switch,
            "go" => Cue::Start,
            "end" => Cue::End,
            // Control points reuse the objective chimes.
            "point" => Cue::Flag,
            "contest" => Cue::Drop,
            // Football: the referee and the hits. A touchdown sounds like a
            // capture; there is no crowd.
            "whistle" => Cue::Whistle,
            "vote" => Cue::Whistle,
            "horn" | "halftime" => Cue::Horn,
            "touchdown_win" => Cue::CaptureWin,
            "touchdown_loss" => Cue::CaptureLoss,
            "oob" | "alarm" => Cue::Warning,
            "tackle" => Cue::Tackle,
            "bump" | "fumble" => Cue::Bump,
            "pass" => Cue::Pass,
            "catch" | "intercept" => Cue::Catch,
            "incoming" => Cue::Incoming,
            // Loadouts: the mortar's deeper thump, an empty click, placing a
            // deployable, a purchase.
            "mortar" => Cue::GrenadeFire,
            "dry" | "buy" => Cue::Switch,
            "deploy" => Cue::Bump,
            // A grenade or mine leaving the hand.
            "throw" => Cue::Pass,
            "laser" => Cue::Laser,
            "rail" => Cue::Rail,
            "plasma" => Cue::PlasmaFire,
            "blaster" => Cue::BlasterFire,
            // A vent: its warning, then the eruption.
            "rumble" => Cue::Rumble,
            "eruption" => Cue::Eruption,
            "blade_swing" => Cue::BladeSwing,
            "blade_heavy" => Cue::BladeHeavy,
            "blade_cut" => Cue::BladeCut,
            "blade_block" => Cue::BladeBlock,
            "blade_clash" => Cue::BladeClash,
            _ => return None,
        })
    }
}

/// Looping beds whose gain, pitch and pan follow the world every frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loop { Jet, Ski, Wind, Spin, Hum, DiscHum, IdleDisc, IdleChain, IdleGrenade }
pub const N_LOOPS: usize = 9;
pub const LOOPS: [Loop; N_LOOPS] = [Loop::Jet, Loop::Ski, Loop::Wind, Loop::Spin, Loop::Hum, Loop::DiscHum,
    Loop::IdleDisc, Loop::IdleChain, Loop::IdleGrenade];

// ---------------------------------------------------------------- synthesis
//
// Every cue is built from layers: a sub (30–80 Hz) for weight, a saturated
// body, a filtered-noise texture and a short transient. Oscillators are
// band-limited (polyBLEP) so nothing buzzes with aliasing, envelopes rise
// and fall smoothly, and a gentle tanh saturation glues the layers together.

/// Attack/release so every clip starts and ends at silence.
fn edges(data: &mut [f32], sr: f32, attack: f32, release: f32) {
    let n = data.len();
    let a = (sr * attack).max(1.0);
    let r = (sr * release).max(1.0);
    for (i, s) in data.iter_mut().enumerate() {
        let from_end = (n - 1 - i) as f32;
        *s *= (i as f32 / a).min(1.0) * (from_end / r).min(1.0);
    }
    if let Some(first) = data.first_mut() { *first = 0.0; }
}

fn normalize(data: &mut [f32], peak: f32) {
    let max = data.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if max > 1e-6 { for s in data.iter_mut() { *s *= peak / max; } }
}

fn render(sr: f32, seconds: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..(sr * seconds) as usize).map(|i| f(i as f32 / sr)).collect()
}

/// Smooth envelope: rises with time constant `a`, decays with time constant `d`.
fn env(t: f32, a: f32, d: f32) -> f32 {
    if t < 0.0 { return 0.0; }
    (1.0 - (-t / a.max(1e-4)).exp()) * (-t / d.max(1e-4)).exp()
}

/// Level-matched tanh saturation: the warm glue on every layered sound.
fn sat(x: f32, drive: f32) -> f32 { (x * drive).tanh() / drive.tanh() }

/// Zero-delay-feedback state-variable filter (topology-preserving). Stable
/// at any cutoff; coefficients are cached while the cutoff holds still.
#[derive(Clone, Copy, Default)]
struct Svf { ic1: f32, ic2: f32, c: f32, q: f32, k: f32, a1: f32, a2: f32, a3: f32 }
impl Svf {
    /// Returns (low, band normalised to unity peak, high).
    fn tick(&mut self, x: f32, cutoff: f32, q: f32, sr: f32) -> (f32, f32, f32) {
        if cutoff != self.c || q != self.q {
            self.c = cutoff;
            self.q = q;
            let g = (std::f32::consts::PI * cutoff.clamp(10.0, sr * 0.45) / sr).tan();
            self.k = 1.0 / q.max(0.05);
            self.a1 = 1.0 / (1.0 + g * (g + self.k));
            self.a2 = g * self.a1;
            self.a3 = g * self.a2;
        }
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, self.k * v1, x - self.k * v1 - v2)
    }
    fn lp(&mut self, x: f32, c: f32, q: f32, sr: f32) -> f32 { self.tick(x, c, q, sr).0 }
    fn bp(&mut self, x: f32, c: f32, q: f32, sr: f32) -> f32 { self.tick(x, c, q, sr).1 }
    fn hp(&mut self, x: f32, c: f32, q: f32, sr: f32) -> f32 { self.tick(x, c, q, sr).2 }
}

/// Band-limited sawtooth (polyBLEP): rich harmonics without aliasing buzz.
#[derive(Clone, Copy, Default)]
struct Saw(f32);
impl Saw {
    fn tick(&mut self, freq: f32, sr: f32) -> f32 {
        let dt = (freq / sr).clamp(0.0, 0.5);
        let t = self.0;
        let mut s = 2.0 * t - 1.0;
        if t < dt {
            let x = t / dt;
            s -= x + x - x * x - 1.0;
        } else if t > 1.0 - dt {
            let x = (t - 1.0) / dt;
            s -= x * x + x + x + 1.0;
        }
        self.0 += dt;
        if self.0 >= 1.0 { self.0 -= 1.0; }
        s
    }
}

/// Phase-accumulating sine, so pitch sweeps stay smooth.
#[derive(Clone, Copy, Default)]
struct Osc(f32);
impl Osc {
    fn tick(&mut self, freq: f32, sr: f32) -> f32 {
        let s = (TAU * self.0).sin();
        self.0 = (self.0 + freq / sr).fract();
        s
    }
}

/// Pink noise (Paul Kellet's economy filter): darker and fuller than white.
#[derive(Clone, Copy)]
struct Pink { rng: Rng, b: [f32; 3] }
impl Pink {
    fn new(seed: u32) -> Self { Pink { rng: Rng(seed), b: [0.0; 3] } }
    fn next(&mut self) -> f32 {
        let w = self.rng.next();
        self.b[0] = 0.99765 * self.b[0] + w * 0.099_046;
        self.b[1] = 0.963 * self.b[1] + w * 0.296_516_4;
        self.b[2] = 0.57 * self.b[2] + w * 1.052_691_3;
        (self.b[0] + self.b[1] + self.b[2] + w * 0.1848) * 0.25
    }
}

/// Announcement sting: detuned warm saws over a sub octave, a soft low
/// impact underneath. Replaces the old bright FM bells.
fn sting(sr: f32, notes: &[(f32, f32)], seconds: f32) -> Vec<f32> {
    let mut voices: Vec<(Saw, Saw, Osc, Svf)> = notes.iter().map(|_| Default::default()).collect();
    let mut sub = Osc::default();
    let mut d = render(sr, seconds, |t| {
        let mut s = sub.tick(62.0, sr) * env(t, 0.004, 0.3) * 0.9;
        for (i, &(f, start)) in notes.iter().enumerate() {
            let a = t - start;
            if a < 0.0 { continue; }
            let (s1, s2, o, filt) = &mut voices[i];
            let raw = s1.tick(f, sr) + s2.tick(f * 1.006, sr) + o.tick(f * 0.5, sr) * 0.8;
            s += filt.lp(raw, 300.0 + 1300.0 * (-a * 3.0).exp(), 0.9, sr) * env(a, 0.012, 0.55) * 0.35;
        }
        s
    });
    for x in d.iter_mut() { *x = sat(*x, 1.4); }
    d
}

/// Deterministic mono PCM for one variant of a cue.
pub fn synth(cue: Cue, variant: usize, sr: f32) -> Vec<f32> {
    let v = variant as f32;
    let seed = 0x5EED_0000 ^ ((cue.index() as u32) << 8) ^ variant as u32 * 7919;
    let mut n = Rng(seed);
    let mut pink = Pink::new(seed ^ 0x9E37);
    let (mut f1, mut f2, mut f3) = (Svf::default(), Svf::default(), Svf::default());
    let mut o1 = Osc::default();
    let (mut s1, mut s2) = (Saw::default(), Saw::default());
    let mut data = match cue {
        Cue::BladeSwing | Cue::BladeHeavy => {
            // A blade through the air: a filtered rush that sweeps up and
            // falls away, over the blade's hum bending with the motion.
            let heavy = cue == Cue::BladeHeavy;
            let (len, base) = if heavy { (0.62, 70.0) } else { (0.36, 96.0 + v * 9.0) };
            render(sr, len, |t| {
                let u = t / len;
                let sweep = (u * std::f32::consts::PI).sin();
                let rush = f1.bp(pink.next(), 500.0 + 1900.0 * sweep, 1.1, sr) * sweep * 1.4;
                let hum = f2.lp(s1.tick(base * (1.0 + 0.35 * sweep), sr) + s2.tick(base * 1.5 * (1.0 + 0.3 * sweep), sr) * 0.5,
                    380.0 + 900.0 * sweep, 1.4, sr) * env(t, 0.01, len * 0.5) * if heavy { 0.9 } else { 0.6 };
                sat(rush + hum, 1.4)
            })
        }
        Cue::BladeCut => {
            // A cut: a sizzling crackle over a short low thud.
            render(sr, 0.45, |t| {
                let thud = o1.tick(80.0 + 60.0 * (-t * 20.0).exp(), sr) * env(t, 0.002, 0.06) * 1.2;
                let sizzle = f1.bp(n.next(), 3600.0 - 800.0 * v, 1.2, sr) * env(t, 0.001, 0.14) * (0.6 + 0.4 * (t * 180.0).sin().abs()) * 1.6;
                let buzz = f2.lp(s1.tick(140.0 + 30.0 * v, sr), 700.0, 2.0, sr) * env(t, 0.004, 0.18) * 0.6;
                sat(thud + sizzle + buzz, 1.6)
            })
        }
        Cue::BladeBlock | Cue::BladeClash => {
            // Blades meeting: a bright electric crack and a ringing zap; a
            // clash louder and longer, crackling as it dies.
            let clash = cue == Cue::BladeClash;
            let mut rings = [Reso::new(1180.0 + 70.0 * v, 18.0, sr), Reso::new(2730.0 - 60.0 * v, 22.0, sr)];
            render(sr, if clash { 0.9 } else { 0.5 }, |t| {
                let x = n.next();
                let crack = f1.bp(x, 2600.0, 0.8, sr) * env(t, 0.0003, if clash { 0.05 } else { 0.025 }) * 2.4;
                let ring: f32 = rings.iter_mut().map(|r| r.tick(x * (-t * 160.0).exp())).sum::<f32>() * 4.0 * (-t * if clash { 5.0 } else { 9.0 }).exp();
                let zap = f2.bp(s1.tick(220.0 + 900.0 * (-t * 12.0).exp(), sr), 1400.0, 1.0, sr) * env(t, 0.001, if clash { 0.3 } else { 0.12 }) * 0.9;
                let crackle = if clash { f3.bp(n.next(), 4200.0, 1.0, sr) * env(t, 0.01, 0.4) * (t * 90.0).sin().max(0.0) * 0.9 } else { 0.0 };
                sat(crack + ring + zap + crackle, 1.7)
            })
        }
        Cue::Laser => {
            // A laser shot: a bright falling zap over a thin electric crackle
            // and a short hum.
            render(sr, 0.45, |t| {
                let zap = s1.tick(2400.0*(-t*9.0).exp()+420.0, sr)*env(t, 0.001, 0.08)*0.9;
                let crackle = f1.bp(n.next(), 5200.0, 1.4, sr)*env(t, 0.0005, 0.05)*1.2;
                let hum = o1.tick(180.0, sr)*env(t, 0.004, 0.18)*0.35;
                sat(zap+crackle+hum, 1.3)
            })
        }
        Cue::Rail => {
            // A railgun: a hard electric crack, a deep punch and a ringing
            // metallic tail as the rails discharge.
            render(sr, 1.0, |t| {
                let crack = f1.bp(n.next(), 3000.0, 0.9, sr)*env(t, 0.0003, 0.02)*2.4;
                let punch = o1.tick(60.0+80.0*(-t*14.0).exp(), sr)*env(t, 0.002, 0.12)*1.4;
                let ring = f2.bp(s1.tick(760.0, sr), 1520.0, 6.0, sr)*env(t, 0.004, 0.45)*0.6;
                let hiss = f3.bp(pink.next(), 1800.0*(-t*2.0).exp()+500.0, 1.0, sr)*env(t, 0.01, 0.3)*0.7;
                sat(crack+punch+ring+hiss, 1.6)
            })
        }
        Cue::DiscFire => {
            // Launch thump with real mid body, a pneumatic whoosh and a
            // spinning electric whir that trails off over most of a second.
            let dt = 1.0 + (v - 1.0) * 0.03;
            render(sr, 1.2, |t| {
                let sub = o1.tick((50.0 + 42.0 * (-t * 7.0).exp()) * dt, sr) * env(t, 0.003, 0.16) * 1.3;
                let body = f1.bp(s1.tick(118.0 * dt, sr), 240.0 * dt, 1.2, sr) * env(t, 0.002, 0.14) * 1.5;
                let whoosh = f2.bp(pink.next(), (1700.0 * (-t * 3.0).exp() + 380.0) * dt, 0.9, sr) * env(t, 0.006, 0.28) * 1.6;
                let spin = 0.55 + 0.45 * (TAU * (19.0 - 8.0 * t.min(1.0)) * t).sin();
                let whir = f3.bp(s2.tick((210.0 + 120.0 * (-t * 2.0).exp()) * dt, sr), 780.0 * dt, 2.5, sr)
                    * spin * env(t, 0.02, 0.42) * 0.9;
                let click = n.next() * env(t, 0.0004, 0.004) * 0.3;
                sat(sub + body + whoosh + whir + click, 1.5)
            })
        }
        Cue::DiscReady => render(sr, 0.4, |t| {
            let thunk = o1.tick(68.0, sr) * env(t, 0.002, 0.05) * 0.8;
            let latch = if t > 0.05 { f1.bp(n.next(), 1100.0, 4.0, sr) * env(t - 0.05, 0.0005, 0.02) * 1.2 } else { 0.0 };
            let charge = f2.lp(s1.tick(55.0, sr), 300.0 + 600.0 * t, 2.0, sr) * env(t, 0.03, 0.12) * 0.35;
            thunk + latch + charge
        }),
        Cue::ChainShot | Cue::TurretBullet => {
            // Short, dense report: a low thump under a bright mechanical crack.
            let turret = cue == Cue::TurretBullet;
            let p = 1.0 + (v - 2.5) * 0.02;
            let base = if turret { 72.0 } else { 90.0 } * p;
            let mut rings = [Reso::new((980.0 + v * 31.0) * p, 10.0, sr), Reso::new((2350.0 - v * 27.0) * p, 12.0, sr)];
            render(sr, if turret { 0.22 } else { 0.16 }, |t| {
                let x = n.next();
                let thump = o1.tick(base * (1.0 + 0.6 * (-t * 60.0).exp()), sr) * env(t, 0.0008, 0.03) * 0.75;
                let crack = f1.bp(x, if turret { 2000.0 } else { 2300.0 }, 0.8, sr) * env(t, 0.0003, 0.018) * 2.2;
                let snap = f2.bp(x, 850.0, 1.0, sr) * env(t, 0.0005, 0.03) * 2.4;
                let mech: f32 = rings.iter_mut().map(|r| r.tick(x * (-t * 320.0).exp())).sum::<f32>() * 3.4 * (-t * 26.0).exp();
                sat(thump + crack + snap + mech, 2.0)
            })
        }
        Cue::GrenadeFire => {
            let p = 1.0 + (v - 1.0) * 0.04;
            let mut clack = [Reso::new(680.0 * p, 12.0, sr), Reso::new(1270.0 * p, 14.0, sr)];
            render(sr, 0.75, |t| {
                let x = n.next();
                let thoonk = sat(o1.tick(70.0 * p * (1.0 + 0.9 * (-t * 28.0).exp()), sr) * 1.5, 2.0) * env(t, 0.002, 0.11);
                let tube = f1.bp(x, 250.0 * p, 5.0, sr) * env(t, 0.002, 0.08) * 1.4;
                let hiss = f2.lp(pink.next(), 1400.0, 0.7, sr) * env(t, 0.008, 0.14) * 0.5;
                let latch = if t > 0.34 {
                    clack.iter_mut().map(|r| r.tick(x * (-(t - 0.34) * 280.0).exp())).sum::<f32>() * 2.0
                } else { 0.0 };
                thoonk + tube + hiss + latch
            })
        }
        Cue::PlasmaFire => {
            // The plasma gun: a wet FM thump that drops in pitch, with a
            // short sizzle as the bolt leaves.
            let base = 150.0 + v * 12.0;
            let (mut pc, mut pm) = (0.0_f32, 0.0_f32);
            render(sr, 0.55, |t| {
                let f = base * (0.55 + 0.45 * (-t * 9.0).exp());
                pm = (pm + f * 2.0 / sr).fract();
                pc = (pc + f / sr).fract();
                let idx = 2.6 * (-t * 7.0).exp() + 0.3;
                let thump = sat((TAU * pc + idx * (TAU * pm).sin()).sin() * 1.6, 2.0) * env(t, 0.002, 0.16) * 0.8;
                let sizzle = f1.bp(n.next(), 2600.0 * (-t * 3.0).exp() + 1200.0, 0.9, sr) * env(t, 0.002, 0.12) * 1.3;
                thump + sizzle
            })
        }
        Cue::BlasterFire => {
            // The blaster: a quick bright zap falling in pitch over a tiny click.
            let top = 1900.0 + v * 140.0;
            render(sr, 0.26, |t| {
                let zap = s1.tick(top * (-t * 22.0).exp() + 520.0, sr) * env(t, 0.0008, 0.06) * 0.8;
                let ring = o1.tick(1040.0, sr) * env(t, 0.001, 0.05) * 0.25;
                let click = f1.bp(n.next(), 4200.0, 1.2, sr) * env(t, 0.0003, 0.012) * 0.9;
                sat(zap + ring + click, 1.2)
            })
        }
        Cue::TurretPlasma => {
            // A heavy FM growl with a sizzling plasma hiss over it.
            let base = 84.0 + v * 9.0;
            let (mut pc, mut pm) = (0.0_f32, 0.0_f32);
            render(sr, 1.2, |t| {
                pm = (pm + base * 1.5 / sr).fract();
                pc = (pc + base / sr).fract();
                let idx = 3.5 * (-t * 3.0).exp() + 0.6;
                let growl = sat((TAU * pc + idx * (TAU * pm).sin()).sin() * 1.8, 2.5) * env(t, 0.004, 0.35) * 0.7;
                let sizzle = f1.bp(n.next(), 3200.0 * (-t * 1.5).exp() + 1400.0, 0.8, sr) * env(t, 0.004, 0.3) * 1.8;
                let sub = o1.tick(46.0, sr) * env(t, 0.004, 0.2) * 0.7;
                growl + sizzle + sub
            })
        }
        Cue::BoomNear => {
            // Short and deep: a sharp crack, a heavy 40-120 Hz body that
            // punches and drops away, a quick fireball, a scatter of debris.
            // Audible for under a second.
            let drop = 1.0 + (v - 1.0) * 0.06;
            let mut debris = Rng(0xDEB2 ^ variant as u32);
            let (mut gf, mut b1) = (Svf::default(), Svf::default());
            render(sr, 1.0, |t| {
                let x = n.next();
                let sub = o1.tick((46.0 + 64.0 * (-t * 9.0).exp()) * drop, sr) * env(t, 0.003, 0.14) * 0.8;
                let crack = f1.hp(x, 1600.0, 0.7, sr) * env(t, 0.0004, 0.05) * 3.0;
                let fire = f2.lp(pink.next(), 6500.0 * (-t * 2.5).exp() + 800.0, 0.7, sr) * env(t, 0.003, 0.2) * 6.0;
                let p = pink.next();
                let body = b1.lp(f3.lp(p, 150.0, 0.8, sr), 150.0, 0.8, sr) * env(t, 0.006, 0.2) * 3.2;
                let hit = if t > 0.05 && t < 0.5 && debris.next() > 0.9975 + t * 0.003 { 1.0 } else { 0.0 };
                let grit = gf.bp(hit, 2300.0 + 700.0 * debris.next(), 5.0, sr) * 1.2 * (0.5 - t).max(0.0) * 2.0;
                sat(sub + crack + fire + body + grit, 1.8)
            })
        }
        Cue::Rumble => {
            // A vent's warning: a deep ground rumble that swells over three
            // seconds toward the eruption, with a slow tremor and a hiss of
            // venting gas rising through it.
            let mut b1 = Svf::default();
            render(sr, 3.2, |t| {
                let swell = (t / 3.0).min(1.0).powf(1.6) * (1.0 - ((t - 3.0) / 0.2).clamp(0.0, 1.0));
                let tremor = 0.7 + 0.3 * (t * 7.0 + (t * 2.3).sin() * 2.0).sin();
                let p = pink.next();
                let ground = b1.lp(f1.lp(p, 90.0 + 60.0 * swell, 0.8, sr), 120.0, 0.8, sr) * 9.0;
                let sub = o1.tick(32.0 + 10.0 * swell, sr) * 0.5;
                let hiss = f2.bp(n.next(), 1800.0 + 1400.0 * swell, 0.6, sr) * 0.35 * swell * swell;
                sat((ground + sub) * tremor * (0.15 + 0.85 * swell) + hiss, 1.5)
            })
        }
        Cue::Eruption => {
            // The eruption: a cracking blast, a long rolling roar of rock and
            // fire, and a rain of debris clattering down behind it.
            let mut debris = Rng(0xE2B7);
            let (mut gf, mut b1) = (Svf::default(), Svf::default());
            render(sr, 3.6, |t| {
                let crack = f1.hp(n.next(), 1200.0, 0.7, sr) * env(t, 0.0006, 0.08) * 3.0;
                let sub = o1.tick(34.0 + 40.0 * (-t * 3.0).exp(), sr) * env(t, 0.004, 0.6) * 0.9;
                let p = pink.next();
                let roar = b1.lp(f3.lp(p, 260.0 * (-t * 0.6).exp() + 90.0, 0.8, sr), 200.0, 0.8, sr) * env(t, 0.01, 1.1) * 6.0;
                let fire = f2.lp(pink.next(), 4200.0 * (-t * 1.2).exp() + 500.0, 0.7, sr) * env(t, 0.005, 0.7) * 3.0;
                let hit = if t > 0.6 && t < 3.2 && debris.next() > 0.9985 { 1.0 } else { 0.0 };
                let grit = gf.bp(hit, 1800.0 + 900.0 * debris.next(), 4.0, sr) * 0.9 * (3.2 - t).max(0.0) / 2.6;
                sat(crack + sub + roar + fire + grit, 1.7)
            })
        }
        Cue::BoomFar => {
            // Distant: the crack is gone, a deep low thud arrives with a short
            // roll behind it. Stays under about 1.2 s.
            let c = 380.0 + v * 40.0;
            let mut b1 = Svf::default();
            render(sr, 1.2, |t| {
                let roll = env(t, 0.02, 0.2) + 0.45 * env(t - 0.14, 0.04, 0.18) + 0.25 * env(t - 0.32, 0.05, 0.16);
                let p = pink.next();
                let rumble = b1.lp(f1.lp(p, c, 0.7, sr), c, 0.7, sr) * roll * 7.0;
                let sub = o1.tick(46.0 + 18.0 * (-t * 6.0).exp(), sr) * env(t, 0.02, 0.18) * 0.3;
                sat(rumble + sub, 1.4)
            })
        }
        Cue::GenBlast => {
            // The generator lets go: a deep blast with a metallic groan and
            // a dying electrical hum, over in about a second.
            let mut groan = [Reso::new(112.0, 30.0, sr), Reso::new(157.0, 34.0, sr), Reso::new(233.0, 36.0, sr)];
            let mut arc = Rng(0xA2C);
            let mut gate = 0.0_f32;
            let mut dying = Saw::default();
            let (mut hum_f, mut b1) = (Svf::default(), Svf::default());
            render(sr, 1.2, |t| {
                let x = n.next();
                let sub = o1.tick(38.0 + 52.0 * (-t * 7.0).exp(), sr) * env(t, 0.004, 0.18) * 0.9;
                let crack = f1.hp(x, 1500.0, 0.7, sr) * env(t, 0.0005, 0.05) * 2.2;
                let roar = f2.lp(pink.next(), 4200.0 * (-t * 2.6).exp() + 520.0, 0.8, sr) * env(t, 0.004, 0.24) * 4.4;
                let body = b1.lp(f3.lp(pink.next(), 140.0, 0.8, sr), 140.0, 0.8, sr) * env(t, 0.008, 0.24) * 4.0;
                let metal: f32 = groan.iter_mut().map(|r| r.tick(x * 0.05)).sum::<f32>() * env(t, 0.02, 0.3) * 6.0;
                if arc.next() > 0.9994 && t < 0.6 { gate = 1.0; }
                gate *= 1.0 - 60.0 / sr;
                let crackle = f3.bp(x, 2800.0, 1.2, sr) * gate * 0.6;
                let hum = hum_f.lp(dying.tick(52.0 - 22.0 * t, sr), 260.0, 1.5, sr) * env(t, 0.01, 0.3) * 0.4;
                sat(sub + crack + roar + body + metal + crackle + hum, 1.7)
            })
        }
        Cue::ShieldHit => {
            // An electric crackle over a resonant energy thud.
            let f = 1150.0 * (1.0 + v * 0.08);
            let mut crackle = Rng(0x5A1D ^ variant as u32);
            let mut gate = 0.0_f32;
            render(sr, 0.7, |t| {
                let x = n.next();
                let thud = o1.tick(110.0 * (1.0 + 0.35 * (-t * 45.0).exp()), sr) * env(t, 0.0008, 0.08) * 1.5;
                let energy = f1.bp(x, f, 5.0, sr) * env(t, 0.0008, 0.12) * 1.5;
                if crackle.next() > 0.993 - (-t * 6.0).exp() * 0.01 { gate = 1.0; }
                gate *= 1.0 - 140.0 / sr;
                let fizz = f2.bp(x, 3200.0, 0.8, sr) * gate * env(t, 0.004, 0.25) * 0.5;
                let buzz = f3.bp(s1.tick(118.0, sr), 900.0, 3.0, sr) * env(t, 0.004, 0.2) * 0.6;
                sat(thud + energy + fizz + buzz, 1.8)
            })
        }
        Cue::ShieldDown => {
            // A falling electrical drone, crackles, then the final pop.
            let mut crackle = Rng(0xC7AC);
            let mut gate = 0.0_f32;
            render(sr, 1.4, |t| {
                let x = n.next();
                let drone = f1.lp(s1.tick(40.0 + 150.0 * (-t * 1.6).exp(), sr), 150.0 + 1200.0 * (-t * 2.2).exp(), 4.0, sr)
                    * env(t, 0.006, 0.55) * 0.9;
                if crackle.next() > 0.995 && t < 1.0 { gate = 1.0; }
                gate *= 1.0 - 90.0 / sr;
                let spit = f2.bp(x, 2400.0, 1.5, sr) * gate * 0.6;
                let pop = o1.tick(78.0, sr) * env(t - 1.05, 0.001, 0.06) * 0.9;
                sat(drone + spit + pop, 1.8)
            })
        }
        Cue::HullHit => {
            let mut r = [Reso::new(190.0 + v * 10.0, 14.0, sr), Reso::new(430.0 - v * 15.0, 16.0, sr),
                Reso::new(880.0 + v * 20.0, 18.0, sr)];
            render(sr, 0.5, |t| {
                let x = n.next() * (-t * 260.0).exp();
                let ring: f32 = r.iter_mut().map(|r| r.tick(x)).sum::<f32>() * 3.0 * (-t * 9.0).exp();
                let thump = o1.tick(88.0, sr) * env(t, 0.0008, 0.05) * 0.8;
                sat(ring + thump + x * 0.2, 1.6)
            })
        }
        Cue::Footstep => {
            // An armored boot: a deep, soft thud and a low muffled scuff,
            // with only a faint clink.
            let mut tick = Reso::new(620.0 + v * 30.0, 7.0, sr);
            render(sr, 0.22, |t| {
                let x = n.next();
                let thump = f1.bp(o1.tick(78.0 + v * 5.0, sr), 110.0, 0.9, sr) * env(t, 0.002, 0.055) * 2.2;
                let scuff = f2.bp(x, 340.0 + v * 25.0, 0.9, sr) * env(t, 0.002, 0.04) * 1.2;
                let clink = tick.tick(x * (-t * 420.0).exp()) * 0.25;
                thump + scuff + clink
            })
        }
        Cue::Splash => {
            // Entering water: a deep, short plop (falling low sine), a
            // filtered slap of spray and two bubbly resonances. Under 0.5 s.
            let mut bubble = [Reso::new(420.0 + v * 40.0, 22.0, sr), Reso::new(690.0 - v * 30.0, 26.0, sr)];
            render(sr, 0.5, |t| {
                let x = n.next();
                let plop = o1.tick(95.0 * (-t * 5.0).exp() + 42.0, sr) * env(t, 0.003, 0.12) * 1.3;
                let slap = f1.bp(pink.next(), 520.0 + v * 60.0, 0.8, sr) * env(t, 0.001, 0.07) * 2.2;
                let spray = f2.hp(x, 2400.0, 0.7, sr) * env(t, 0.004, 0.16) * 0.35;
                let bubbles: f32 = bubble.iter_mut().map(|r| r.tick(x * (-t * 90.0).exp())).sum::<f32>() * 0.9;
                sat(plop + slap + spray + bubbles, 1.4)
            })
        }
        Cue::Wade => {
            // A step through water: a low wet slosh, no metal clink.
            render(sr, 0.28, |t| {
                let x = pink.next();
                let slosh = f1.bp(x, 300.0 + v * 35.0, 0.9, sr) * env(t, 0.012, 0.09) * 2.4;
                let body = o1.tick(70.0 + v * 6.0, sr) * env(t, 0.004, 0.05) * 0.7;
                let drip = f2.bp(n.next(), 1500.0 + v * 120.0, 3.0, sr) * env(t, 0.03, 0.05) * 0.4;
                slosh + body + drip
            })
        }
        Cue::Whistle => {
            // Referee's pea whistle: a bright tone with a fast rattling trill
            // and breath, one firm blast.
            render(sr, 0.55, |t| {
                let rattle = 0.6 + 0.4 * (TAU * 27.0 * t).sin().signum() * 0.5 + 0.2 * (TAU * 54.0 * t).sin();
                let tone = o1.tick(2850.0 + 60.0 * (TAU * 27.0 * t).sin(), sr) * rattle;
                let breath = f1.bp(n.next(), 3000.0, 1.5, sr) * 0.35;
                let shape = (t / 0.02).min(1.0) * if t > 0.42 { (-(t - 0.42) / 0.03).exp() } else { 1.0 };
                (tone + breath) * shape
            })
        }
        Cue::Horn => {
            // A deep stadium horn: three low detuned reeds, short.
            render(sr, 1.0, |t| {
                let vib = 1.0 + 0.004 * (TAU * 5.5 * t).sin();
                let reeds = s1.tick(98.0 * vib, sr) + s2.tick(123.5 * vib, sr) * 0.8;
                let body = f1.lp(reeds + o1.tick(146.8 * vib, sr) * 0.6, 900.0, 0.9, sr);
                let shape = (t / 0.04).min(1.0) * if t > 0.72 { (-(t - 0.72) / 0.07).exp() } else { 1.0 };
                sat(body * 1.6, 1.8) * shape
            })
        }
        Cue::Warning => {
            // Out of bounds: two short falling electronic beeps.
            render(sr, 0.3, |t| {
                let (on, age) = if t < 0.11 { (true, t) } else if (0.15..0.26).contains(&t) { (true, t - 0.15) } else { (false, 0.0) };
                let tone = o1.tick(if t < 0.13 { 880.0 } else { 660.0 }, sr);
                let edge = f1.lp(s1.tick(if t < 0.13 { 880.0 } else { 660.0 }, sr), 2600.0, 0.8, sr) * 0.25;
                if !on { return 0.0; }
                let shape = (age / 0.005).min(1.0) * (1.0 - age / 0.11).max(0.0).sqrt();
                sat((tone + edge) * 1.2, 1.3) * shape
            })
        }
        Cue::Tackle => {
            // A body hit: deep falling thump, a padded smack and dirt.
            render(sr, 0.42, |t| {
                let sub = o1.tick(38.0 + 30.0 * (-t * 14.0).exp(), sr) * env(t, 0.002, 0.12) * 1.5;
                let smack = f1.bp(pink.next(), 190.0 + v * 12.0, 1.0, sr) * env(t, 0.001, 0.05) * 3.0;
                let dirt = f2.lp(n.next(), 900.0, 0.7, sr) * env(t, 0.004, 0.09) * 0.8;
                sat(sub + smack + dirt, 1.7)
            })
        }
        Cue::Bump => {
            // A lighter shoulder check.
            render(sr, 0.22, |t| {
                let sub = o1.tick(60.0 + 25.0 * (-t * 18.0).exp(), sr) * env(t, 0.002, 0.06) * 1.2;
                let smack = f1.bp(pink.next(), 240.0 + v * 20.0, 1.0, sr) * env(t, 0.001, 0.035) * 2.4;
                sat(sub + smack, 1.5)
            })
        }
        Cue::Pass => {
            // A throw: a rising grunt of effort and a whoosh.
            render(sr, 0.32, |t| {
                let whoosh = f1.bp(pink.next(), 500.0 + 2200.0 * (t / 0.3).min(1.0), 1.1, sr) * env(t, 0.03, 0.08) * 2.2;
                let effort = f2.bp(s1.tick(110.0, sr), 420.0, 2.0, sr) * env(t, 0.005, 0.05) * 0.6;
                whoosh + effort
            })
        }
        Cue::Incoming => {
            // A pass on its way to you: one flag flap, cloth snapping taut
            // in the wind. A breath of air, a crisp crack, the cloth's
            // whump, then the cloth rippling out unevenly, its surface
            // rustling and its edges ticking, slowing as it settles.
            let (mut air, mut crack, mut slap, mut cloth, mut ripple, mut rustle, mut edge, mut floor) = (Svf::default(),
                Svf::default(), Svf::default(), Svf::default(), Svf::default(), Svf::default(), Svf::default(), Svf::default());
            let snap_at = 0.035;
            let (mut phase, mut level, mut tick) = (0.0_f32, 1.0_f32, 1.0_f32);
            render(sr, 0.52, |t| {
                let x = n.next();
                // No rumble under the cloth.
                let p = floor.hp(pink.next(), 70.0, 0.7, sr);
                let rise = (t / snap_at).min(1.0);
                let lead = air.bp(p, 400.0 + 700.0 * rise, 0.9, sr) * rise * rise
                    * if t < snap_at { 1.0 } else { (-(t - snap_at) / 0.03).exp() } * 0.7;
                let a = t - snap_at;
                if a < 0.0 { return lead; }
                let snap = crack.bp(x, 3200.0, 0.6, sr) * env(a, 0.0003, 0.011) * 1.5
                    + slap.bp(x, 800.0, 0.8, sr) * env(a, 0.0008, 0.025) * 0.9;
                let whump = cloth.bp(p, 250.0, 0.6, sr) * env(a, 0.0015, 0.02) * 5.5;
                // Each ripple a little stronger or weaker than the last,
                // slowing from 30 to 12 Hz; an edge ticks at each.
                phase += (12.0 + 18.0 * (-a * 7.0).exp()) / sr;
                if phase >= 1.0 { phase -= 1.0; level = 0.55 + 0.45 * n.next().abs(); tick = 0.0; }
                tick += 1.0 / sr;
                let rip = level * (0.4 + 0.6 * (TAU * phase).sin().max(-0.6));
                let tail = env(a, 0.01, 0.13);
                let flutter = ripple.bp(p, 380.0, 0.8, sr) * rip * tail * 3.0;
                let surface = rustle.bp(x, 1800.0, 0.7, sr) * rip * rip * tail * 0.45;
                let ticks = edge.bp(x, 2600.0, 0.7, sr) * (-tick / 0.004).exp() * level * tail * 0.5;
                sat(lead + snap + whump + flutter + surface + ticks, 1.4)
            })
        }
        Cue::Catch => {
            // The ball smacks into hands: a padded slap with a low body.
            render(sr, 0.16, |t| {
                let slap = f1.bp(n.next(), 1300.0 + v * 90.0, 1.3, sr) * env(t, 0.0006, 0.018) * 2.2;
                let body = o1.tick(120.0, sr) * env(t, 0.001, 0.04) * 0.9;
                sat(slap + body, 1.4)
            })
        }
        Cue::Land => {
            let mut clank = [Reso::new(330.0 + v * 30.0, 14.0, sr), Reso::new(760.0, 16.0, sr)];
            render(sr, 0.7, |t| {
                let x = n.next();
                let sub = o1.tick(30.0 + 16.0 * (-t * 8.0).exp(), sr) * env(t, 0.002, 0.13) * 1.1;
                let dirt = f1.lp(pink.next(), 420.0, 0.7, sr) * env(t, 0.002, 0.1) * 2.0;
                let metal: f32 = clank.iter_mut().map(|r| r.tick(x * (-t * 230.0).exp())).sum::<f32>() * 1.6;
                sat(sub + dirt + metal, 1.5)
            })
        }
        Cue::Switch => {
            let mut a = [Reso::new(820.0, 10.0, sr), Reso::new(1350.0, 12.0, sr)];
            let mut b = [Reso::new(600.0, 10.0, sr), Reso::new(1100.0, 12.0, sr)];
            render(sr, 0.45, |t| {
                let x = n.next();
                let c1: f32 = a.iter_mut().map(|r| r.tick(x * (-t * 380.0).exp())).sum::<f32>() * 1.8;
                let c2: f32 = if t > 0.22 {
                    b.iter_mut().map(|r| r.tick(x * (-(t - 0.22) * 380.0).exp())).sum::<f32>() * 2.0
                } else { 0.0 };
                let servo = if t > 0.02 && t < 0.24 {
                    f1.lp(s1.tick(95.0 + 60.0 * t, sr), 700.0, 2.0, sr) * (((t - 0.02) / 0.22) * std::f32::consts::PI).sin() * 0.3
                } else { 0.0 };
                c1 + c2 + servo
            })
        }
        Cue::RepairKit => render(sr, 1.1, |t| {
            // A pressurised injector hiss over a low electric hum.
            let shape = (t / 0.06).min(1.0) * (1.0 - t / 1.1).max(0.0);
            let hiss = f1.bp(pink.next(), 4200.0, 0.6, sr) * shape * (-t * 1.2).exp() * 3.4;
            let f = 72.0 + 40.0 * t / 1.1;
            let hum = f2.lp(s1.tick(f, sr) + s2.tick(f * 1.005, sr), 520.0, 2.5, sr) * shape * 0.5;
            let thump = o1.tick(80.0, sr) * env(t, 0.001, 0.05) * 0.7;
            sat(hiss + hum + thump, 1.5)
        }),
        Cue::Bounce => {
            let mut r = [Reso::new(480.0 + v * 40.0, 14.0, sr), Reso::new(1050.0 - v * 30.0, 16.0, sr)];
            render(sr, 0.3, |t| {
                let x = n.next() * (-t * 300.0).exp();
                r.iter_mut().map(|r| r.tick(x)).sum::<f32>() * 2.4 + o1.tick(140.0, sr) * env(t, 0.0008, 0.03) * 0.6
            })
        }
        Cue::Hit => render(sr, 0.14, |t| {
            let tick = f1.bp(n.next(), 1600.0, 5.0, sr) * env(t, 0.0005, 0.02) * 1.4;
            let body = o1.tick(190.0, sr) * env(t, 0.0008, 0.03) * 0.5;
            tick + body
        }),
        Cue::Pain => render(sr, 0.32, |t| {
            let grunt = f1.lp(pink.next(), 520.0, 1.2, sr) * env(t, 0.004, 0.08) * 2.4;
            let body = o1.tick(96.0 - 30.0 * t, sr) * env(t, 0.003, 0.07) * 0.6;
            sat(grunt + body, 1.6)
        }),
        Cue::Death => render(sr, 1.0, |t| {
            let fall = f1.lp(s1.tick(45.0 + 95.0 * (-t * 2.4).exp(), sr), 150.0 + 500.0 * (-t * 2.0).exp(), 2.0, sr)
                * env(t, 0.005, 0.45) * 0.8;
            let breath = f2.lp(pink.next(), 380.0, 0.7, sr) * env(t, 0.02, 0.3) * 0.9;
            sat(fall + breath, 1.5)
        }),
        Cue::Flag => sting(sr, &[(329.63, 0.0), (493.88, 0.11)], 1.0),
        Cue::Drop => sting(sr, &[(261.63, 0.0), (196.0, 0.12)], 0.9),
        Cue::Return => sting(sr, &[(196.0, 0.0), (293.66, 0.1)], 0.95),
        Cue::Start => sting(sr, &[(196.0, 0.0), (246.94, 0.1), (293.66, 0.2)], 1.2),
        Cue::End => sting(sr, &[(293.66, 0.0), (246.94, 0.14), (196.0, 0.28)], 1.4),
        Cue::CaptureWin => capture(sr, true),
        Cue::CaptureLoss => capture(sr, false),
    };
    let peak = match cue {
        Cue::GenBlast => 0.88, Cue::BoomNear => 0.85, Cue::BoomFar => 0.62,
        Cue::Eruption => 0.88, Cue::Rumble => 0.6,
        Cue::DiscFire | Cue::GrenadeFire => 0.66, Cue::ChainShot | Cue::TurretBullet => 0.5,
        Cue::Footstep => 0.08, Cue::Hit => 0.3, Cue::ShieldHit => 0.45,
        Cue::Whistle => 0.34, Cue::Horn => 0.55, Cue::Warning => 0.3,
        Cue::Tackle => 0.7, Cue::Bump => 0.5, Cue::Pass => 0.35, Cue::Catch => 0.4, Cue::Incoming => 0.36,
        Cue::Laser => 0.5, Cue::Rail => 0.7, Cue::PlasmaFire => 0.6, Cue::BlasterFire => 0.42,
        Cue::BladeSwing => 0.32, Cue::BladeHeavy => 0.45, Cue::BladeCut => 0.55, Cue::BladeBlock => 0.5, Cue::BladeClash => 0.68,
        _ => 0.5,
    };
    normalize(&mut data, peak);
    edges(&mut data, sr, 0.0015, 0.03);
    data
}

/// Capture motif: an impact, a sequence of warm notes and a sustained pad.
fn capture(sr: f32, victory: bool) -> Vec<f32> {
    let notes = if victory { [130.81, 164.81, 196.0, 261.63] } else { [196.0, 174.61, 155.56, 130.81] };
    let chord = if victory { [65.41, 130.81, 164.81, 196.0] } else { [65.41, 77.78, 98.0, 130.81] };
    let seq: Vec<(f32, f32)> = notes.iter().enumerate().map(|(i, f)| (*f, i as f32 * 0.22)).collect();
    let mut out = sting(sr, &seq, 2.8);
    let mut pad: Vec<(Saw, Saw, Svf)> = chord.iter().map(|_| Default::default()).collect();
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / sr;
        if t < 0.72 { continue; }
        let a = t - 0.72;
        let mut p = 0.0;
        for (j, f) in chord.iter().enumerate() {
            let (s1, s2, filt) = &mut pad[j];
            p += filt.lp(s1.tick(*f, sr) + s2.tick(f * 1.004, sr), 700.0, 0.7, sr);
        }
        *s += p * 0.06 * (a / 0.25).min(1.0) * (-a * 1.1).exp();
    }
    out
}

/// Seamless mono loops: rendered past the end, then the tail is crossfaded
/// into the start so filters and noise join without a seam.
pub fn synth_loop(kind: Loop, sr: f32) -> Vec<f32> {
    let seconds = match kind { Loop::Wind => 4.0, Loop::Spin | Loop::DiscHum | Loop::IdleChain => 1.0, _ => 2.0 };
    let len = (sr * seconds) as usize;
    let overlap = (sr * 0.08) as usize;
    let mut n = Rng(0x100F ^ kind as u32 * 131);
    let mut pink = Pink::new(0x77 ^ kind as u32 * 977);
    let (mut f1, mut f2, mut f3, mut f4) = (Svf::default(), Svf::default(), Svf::default(), Svf::default());
    let (mut s1, mut s2) = (Saw::default(), Saw::default());
    let (mut o1, mut o2) = (Osc::default(), Osc::default());
    let mut grain = 0.0_f32;
    let mut data: Vec<f32> = (0..len + overlap).map(|i| {
        let t = (i % len) as f32 / sr;
        let x = n.next();
        match kind {
            Loop::Jet => {
                // Roaring thrust: dark noise body with a fast combustion
                // flutter, a rasp and a low rumble.
                let p = pink.next();
                let flutter = 0.75 + 0.25 * (TAU * 25.0 * t).sin() + 0.08 * (TAU * 37.0 * t).sin();
                let body = f1.lp(p, 820.0, 0.8, sr) * 2.4;
                let rasp = f2.bp(x, 2000.0, 0.7, sr) * 0.6;
                let rumble = f3.lp(p, 110.0, 0.9, sr) * 2.2;
                let turbine = f4.lp(s1.tick(74.0, sr), 380.0, 1.5, sr) * 0.12;
                sat((body + rasp + rumble + turbine) * flutter, 1.4)
            }
            Loop::Ski => {
                // Gritty scrape with grainy texture over a thin low bed.
                grain += (x.abs() - grain) * lp_coef(28.0, sr);
                let scrape = f1.bp(x, 2100.0, 0.6, sr) * (0.5 + 1.6 * grain) * 1.4;
                let hiss = f3.hp(x, 4500.0, 0.7, sr) * 0.2 * (0.6 + grain);
                let bed = f2.lp(pink.next(), 300.0, 0.7, sr) * 0.45;
                scrape + hiss + bed
            }
            Loop::Wind => {
                // Two gusting noise layers and a faint resonant howl.
                let g1 = 0.55 + 0.3 * (TAU * 0.25 * t).sin() + 0.15 * (TAU * 0.75 * t + 1.3).sin();
                let g2 = 0.5 + 0.35 * (TAU * 0.5 * t + 0.7).sin() + 0.15 * (TAU * 1.25 * t).sin();
                let low = f1.lp(pink.next(), 320.0, 0.7, sr) * g1 * 0.9;
                let air = f2.bp(x, 700.0 + 250.0 * g2, 0.8, sr) * g2 * 1.8;
                let howl = f3.bp(x, 480.0 + 160.0 * g1, 12.0, sr) * g1 * g2 * 1.2;
                low + air + howl
            }
            Loop::Spin => {
                // Chaingun motor: a low saw, a gear whine and ticking.
                let motor = f1.lp(s1.tick(60.0, sr), 620.0, 2.0, sr) * 0.7;
                let gear = f2.lp(s2.tick(180.0, sr), 900.0, 1.2, sr) * 0.22;
                let ticks = f3.bp(x, 2000.0, 2.0, sr) * (0.5 + 0.5 * (TAU * 20.0 * t).sin()).powi(4) * 0.5;
                sat(motor + gear + ticks, 1.5)
            }
            Loop::Hum => {
                // Generator: a saturated drone with a 12 Hz pulse and buzz.
                let base = sat(o1.tick(60.0, sr) * 1.6 + o2.tick(60.5, sr) * 0.6, 2.2);
                let pulse = 0.72 + 0.28 * (TAU * 12.0 * t).sin();
                let buzz = f1.lp(s1.tick(120.0, sr), 420.0, 1.5, sr) * 0.25;
                let air = f2.bp(x, 400.0, 1.0, sr) * 0.05;
                (base * 0.8 + buzz) * pulse + air
            }
            Loop::DiscHum => {
                // A disc in flight: a spinning whine over a light hum.
                let whine = f1.bp(s1.tick(1150.0, sr), 2300.0, 2.0, sr);
                let am = 0.5 + 0.5 * (TAU * 20.0 * t).sin();
                let air = f2.bp(x, 3200.0, 1.0, sr) * 0.3;
                sat(whine * (0.4 + 0.6 * am) + air + o1.tick(110.0, sr) * 0.03, 1.4)
            }
            Loop::IdleDisc => {
                // Held disc launcher: a soft, deep electric hum centred around
                // 90-170 Hz with a faint whir inside it and a gentle pulse
                // about 10 times a second.
                let hum = f1.bp(s1.tick(88.0, sr) + s2.tick(88.6, sr), 160.0, 1.4, sr);
                let throb = 0.5 + 0.5 * (TAU * 10.0 * t).sin();
                let whir = f2.bp(x, 220.0, 3.0, sr) * (0.3 + 0.7 * (0.5 + 0.5 * (TAU * 5.0 * t).sin()));
                sat(hum * (0.75 + 0.25 * throb) * 1.4 + whir * 0.4, 1.8)
            }
            Loop::IdleChain => {
                // Held chaingun: a quiet motor with a slow tick.
                let motor = f1.lp(s1.tick(40.0, sr), 240.0, 1.5, sr) * 0.5;
                let tick = f2.bp(x, 1800.0, 3.0, sr) * (0.5 + 0.5 * (TAU * 8.0 * t).sin()).powi(12) * 1.2;
                motor + tick
            }
            Loop::IdleGrenade => {
                // Held grenade launcher: a low mechanical settle and creak.
                grain += (x - grain) * lp_coef(3.0, sr);
                let settle = f1.lp(pink.next(), 150.0, 0.8, sr) * 1.6;
                let creak = f2.bp(x, 320.0, 10.0, sr) * (grain * 40.0).clamp(-1.0, 1.0).abs() * 0.4;
                let hum = o1.tick(45.0, sr) * 0.18;
                settle + creak + hum
            }
        }
    }).collect();
    for i in 0..overlap {
        let blend = i as f32 / overlap as f32;
        data[i] = data[len + i] * (1.0 - blend) + data[i] * blend;
    }
    data.truncate(len);
    normalize(&mut data, 0.5);
    data
}

// ------------------------------------------------------ custom sound files

impl Cue {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// File name (without extension) for a user-supplied replacement.
    pub fn file_stem(self) -> &'static str {
        match self {
            Cue::DiscFire => "disc-fire", Cue::DiscReady => "disc-ready", Cue::ChainShot => "chain-shot",
            Cue::GrenadeFire => "grenade-fire", Cue::TurretBullet => "turret-bullet", Cue::TurretPlasma => "turret-plasma",
            Cue::BoomNear => "explosion-near", Cue::BoomFar => "explosion-far", Cue::GenBlast => "generator-explosion",
            Cue::ShieldHit => "shield-hit", Cue::ShieldDown => "shield-down", Cue::HullHit => "hull-hit",
            Cue::Footstep => "footstep", Cue::Land => "land", Cue::Switch => "weapon-switch",
            Cue::RepairKit => "repair-kit", Cue::Bounce => "grenade-bounce", Cue::Hit => "hit-confirm",
            Cue::Pain => "pain", Cue::Death => "death", Cue::Flag => "flag-taken", Cue::Drop => "flag-dropped",
            Cue::Return => "flag-returned", Cue::CaptureWin => "capture-win", Cue::CaptureLoss => "capture-loss",
            Cue::Start => "match-start", Cue::End => "match-end",
            Cue::Splash => "splash", Cue::Wade => "wade",
            Cue::Whistle => "whistle", Cue::Horn => "stadium-horn", Cue::Warning => "warning",
            Cue::Tackle => "tackle", Cue::Bump => "body-check",
            Cue::Pass => "pass", Cue::Catch => "catch", Cue::Incoming => "pass-incoming", Cue::Laser => "laser", Cue::Rail => "rail",
            Cue::PlasmaFire => "plasma-fire", Cue::BlasterFire => "blaster-fire",
            Cue::Rumble => "vent-rumble", Cue::Eruption => "eruption",
            Cue::BladeSwing => "blade-swing", Cue::BladeHeavy => "blade-heavy", Cue::BladeCut => "blade-cut",
            Cue::BladeBlock => "blade-block", Cue::BladeClash => "blade-clash",
        }
    }
}

impl Loop {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn file_stem(self) -> &'static str {
        match self {
            Loop::Jet => "loop-jet", Loop::Ski => "loop-ski", Loop::Wind => "loop-wind", Loop::Spin => "loop-chaingun-spin",
            Loop::Hum => "loop-generator", Loop::DiscHum => "loop-disc-flight", Loop::IdleDisc => "loop-idle-disc",
            Loop::IdleChain => "loop-idle-chaingun", Loop::IdleGrenade => "loop-idle-grenade",
        }
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
/// Mono samples and rate from a WAV file: 8/16/24/32-bit integer or 32-bit
/// float PCM, any channel count (averaged to mono). `None` for anything else.
pub fn parse_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    let u16_at = |i: usize| bytes.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |i: usize| bytes.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if bytes.get(0..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" { return None; }
    let (mut fmt, mut at) = (None, 12usize);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4)? as usize;
        let body = at + 8;
        if id == b"fmt " {
            let mut tag = u16_at(body)?;
            if tag == 0xFFFE { tag = u16_at(body + 24)?; }
            fmt = Some((tag, u16_at(body + 2)?.max(1) as usize, u32_at(body + 4)?, u16_at(body + 14)?));
        } else if id == b"data" {
            let (tag, channels, rate, bits) = fmt?;
            let data = bytes.get(body..(body + size).min(bytes.len()))?;
            let width = (bits as usize).div_ceil(8);
            if width == 0 || rate == 0 { return None; }
            let sample = |c: &[u8]| -> Option<f32> {
                Some(match (tag, bits) {
                    (1, 8) => (c[0] as f32 - 128.0) / 128.0,
                    (1, 16) => i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0,
                    (1, 24) => (((c[2] as i32) << 24 | (c[1] as i32) << 16 | (c[0] as i32) << 8) >> 8) as f32 / 8_388_608.0,
                    (1, 32) => i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0,
                    (3, 32) => f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    _ => return None,
                })
            };
            let frame = width * channels;
            let mut out = Vec::with_capacity(data.len() / frame);
            for f in data.chunks_exact(frame) {
                let mut sum = 0.0;
                for c in f.chunks_exact(width) { sum += sample(c)?; }
                out.push((sum / channels as f32).clamp(-1.0, 1.0));
            }
            return Some((out, rate));
        }
        at = body + size + (size & 1);
    }
    None
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
/// Linear resample to the mixer rate, with short fades so a custom clip
/// can't click.
fn resample(s: &[f32], from: f32, to: f32, fade: bool) -> Vec<f32> {
    if s.is_empty() { return Vec::new(); }
    let ratio = from / to;
    let len = ((s.len() as f32 / ratio) as usize).max(2);
    let mut out: Vec<f32> = (0..len).map(|i| {
        let x = i as f32 * ratio;
        let j = x as usize;
        let f = x - j as f32;
        s[j.min(s.len() - 1)] * (1.0 - f) + s[(j + 1).min(s.len() - 1)] * f
    }).collect();
    if fade { edges(&mut out, to, 0.002, 0.01); }
    out
}

// -------------------------------------------------------------------- mixer

/// One request for the mixer. `Copy`, so queuing never allocates.
#[derive(Clone, Copy, Debug)]
pub struct Play {
    pub cue: Cue,
    pub variant: usize,
    pub gain: f32,
    /// -1 left .. 1 right.
    pub pan: f32,
    /// Low-pass cutoff in Hz; distance makes sounds duller.
    pub cutoff: f32,
    /// Seconds before the sound starts (sound travel for far explosions).
    pub delay: f32,
    /// Playback rate: small pitch variation.
    pub rate: f32,
    /// A mod's sound (`Mixer::set_custom`) instead of the cue's clips.
    pub custom: Option<u16>,
}

impl Play {
    pub fn local(cue: Cue, variant: usize) -> Self {
        Play { cue, variant, gain: 1.0, pan: 0.0, cutoff: 18_000.0, delay: 0.0, rate: 1.0, custom: None }
    }
}

/// Per-frame targets for every loop, smoothed inside the mixer.
#[derive(Clone, Copy, Debug, Default)]
pub struct LoopTarget { pub gain: f32, pub rate: f32, pub pan: f32, pub cutoff: f32 }

pub const VOICES: usize = 32;
/// Reverb send levels outdoors and under a roof.
const OUTDOOR_WET: f32 = 0.16;
const INDOOR_WET: f32 = 0.34;
const QUEUE: usize = 96;

#[derive(Clone, Default)]
struct Voice {
    clip: Option<Arc<[f32]>>,
    cue_index: usize,
    priority: u8,
    pos: f32,
    rate: f32,
    gl: f32,
    gr: f32,
    lp_a: f32,
    lp_z: f32,
    delay: u32,
    /// Reverb send scale for this cue.
    wet: f32,
}

#[derive(Clone)]
struct LoopVoice {
    clip: Arc<[f32]>,
    pos: f32,
    gain: f32,
    rate: f32,
    pan: f32,
    lp_a: f32,
    lp_z: f32,
    target: LoopTarget,
    /// Gain smoothing coefficients (rise, fall) per sample.
    attack: f32,
    release: f32,
    /// Reverb send scale.
    wet: f32,
}

/// One-pole smoothing coefficient for time constant `tau` seconds.
fn tau_coef(tau: f32, sr: f32) -> f32 { 1.0 - (-1.0 / (tau * sr)).exp() }

/// Rise and fall time constants for a loop's gain. Skiing is a contact
/// sound: it bites in fast and stops almost at once, like a one-shot scrape,
/// instead of lingering after the skis leave the ground.
fn loop_edges(l: Loop) -> (f32, f32) {
    match l {
        Loop::Ski => (0.012, 0.02),
        _ => (0.04, 0.04),
    }
}

/// Reverb send per loop. A ski scrape is a dry contact sound; letting it
/// ring in the outdoor reverb made it drag on after the skis lifted.
fn loop_wet(l: Loop) -> f32 {
    match l {
        Loop::Ski => 0.05,
        _ => 0.3,
    }
}

/// Stereo feedback-delay-network reverb: a pre-delay, then four damped lines
/// mixed by a Householder matrix. Outdoors it is long and dark; indoors
/// shorter and brighter. Bass is filtered out of its input so the low end
/// stays mono and tight. Buffers are allocated once.
struct Reverb {
    sr: f32,
    pre: Vec<f32>,
    pre_at: usize,
    lines: [Vec<f32>; 4],
    at: [usize; 4],
    damp: [f32; 4],
    gains: [f32; 4],
    gains_target: [f32; 4],
    bright: f32,
    bright_target: f32,
    hp_z: f32,
    hp_a: f32,
}
const FDN_MS: [f32; 4] = [43.1, 53.7, 67.3, 79.9];
impl Reverb {
    fn new(sr: f32) -> Self {
        let line = |ms: f32| vec![0.0; (sr * ms / 1000.0) as usize];
        let mut r = Reverb {
            sr, pre: vec![0.0; (sr * 0.022) as usize], pre_at: 0,
            lines: [line(FDN_MS[0]), line(FDN_MS[1]), line(FDN_MS[2]), line(FDN_MS[3])], at: [0; 4],
            damp: [0.0; 4], gains: [0.0; 4], gains_target: [0.0; 4], bright: 0.3, bright_target: 0.3,
            hp_z: 0.0, hp_a: lp_coef(140.0, sr),
        };
        r.set(2.4, 0.3);
        r.gains = r.gains_target;
        r
    }
    /// Decay time (RT60, seconds) and brightness (0 dark .. 1 bright).
    fn set(&mut self, t60: f32, bright: f32) {
        for i in 0..4 {
            self.gains_target[i] = 10f32.powf(-3.0 * self.lines[i].len() as f32 / self.sr / t60.max(0.1));
        }
        self.bright_target = bright.clamp(0.05, 1.0);
    }
    fn tick(&mut self, x: f32, k: f32) -> (f32, f32) {
        self.hp_z += (x - self.hp_z) * self.hp_a;
        let x = x - self.hp_z;
        let d = self.pre[self.pre_at];
        self.pre[self.pre_at] = x;
        self.pre_at = (self.pre_at + 1) % self.pre.len();
        self.bright += (self.bright_target - self.bright) * k;
        let mut y = [0.0_f32; 4];
        for i in 0..4 {
            y[i] = self.lines[i][self.at[i]];
            self.damp[i] += (y[i] - self.damp[i]) * self.bright;
        }
        let w = 0.5 * (self.damp[0] + self.damp[1] + self.damp[2] + self.damp[3]);
        for i in 0..4 {
            self.gains[i] += (self.gains_target[i] - self.gains[i]) * k;
            let sign = if i % 2 == 0 { 0.5 } else { -0.5 };
            self.lines[i][self.at[i]] = (self.damp[i] - w) * self.gains[i] + d * sign;
            self.at[i] = (self.at[i] + 1) % self.lines[i].len();
        }
        (y[0] + y[2] * 0.6 - y[3] * 0.3, y[1] + y[3] * 0.6 - y[2] * 0.3)
    }
}

/// Soft-knee peak limiter gain for a level `p` (linear): transparent below
/// about -5 dBFS, 20:1 above a -2 dBFS threshold.
fn limit_gain(p: f32) -> f32 {
    if p < 0.5 { return 1.0; }
    let x = 20.0 * p.log10();
    let (t, w, r) = (-2.0_f32, 6.0_f32, 20.0_f32);
    let over = x - t;
    let y = if 2.0 * over < -w {
        x
    } else if 2.0 * over.abs() <= w {
        x + (1.0 / r - 1.0) * (over + w / 2.0).powi(2) / (2.0 * w)
    } else {
        t + over / r
    };
    10f32.powf((y - x) / 20.0)
}

/// Fixed-size stereo mixer. After construction, playing sounds and rendering
/// never allocate: clips are shared `Arc`s and the queue has fixed capacity.
pub struct Mixer {
    pub sr: f32,
    clips: Vec<Vec<Arc<[f32]>>>,
    voices: Vec<Voice>,
    loops: Vec<LoopVoice>,
    ambient: Option<LoopVoice>,
    ambient_target: f32,
    queue: Vec<Play>,
    master: f32,
    master_target: f32,
    reverb: Reverb,
    room_level: f32,
    room_target: f32,
    smooth: f32,
    lim: f32,
    lim_attack: f32,
    lim_release: f32,
    /// The active mods' sounds (`set_custom`).
    custom: Vec<Arc<[f32]>>,
}

impl Mixer {
    pub fn new(sr: f32) -> Self {
        let clips = CUES.iter().map(|&c| (0..c.variants()).map(|v| Arc::<[f32]>::from(synth(c, v, sr))).collect()).collect();
        let loops = LOOPS.iter().map(|&l| LoopVoice {
            clip: Arc::from(synth_loop(l, sr)), pos: 0.0, gain: 0.0, rate: 1.0, pan: 0.0,
            lp_a: 1.0, lp_z: 0.0, target: LoopTarget { gain: 0.0, rate: 1.0, pan: 0.0, cutoff: 18_000.0 },
            attack: tau_coef(loop_edges(l).0, sr), release: tau_coef(loop_edges(l).1, sr), wet: loop_wet(l),
        }).collect();
        Mixer {
            sr, clips, voices: vec![Voice::default(); VOICES], loops, ambient: None, ambient_target: 0.0,
            queue: Vec::with_capacity(QUEUE), master: 0.85, master_target: 0.85, reverb: Reverb::new(sr),
            room_level: OUTDOOR_WET, room_target: OUTDOOR_WET, smooth: 1.0 - (-1.0 / (0.04 * sr)).exp(),
            lim: 1.0, lim_attack: 1.0 - (-1.0 / (0.0005 * sr)).exp(), lim_release: 1.0 - (-1.0 / (0.25 * sr)).exp(),
            custom: Vec::new(),
        }
    }

    /// Replace a cue's variants with user-supplied clips (any rate, mono).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn override_cue(&mut self, cue: Cue, clips: Vec<(Vec<f32>, u32)>) {
        let sr = self.sr;
        let clips: Vec<Arc<[f32]>> = clips.into_iter().filter(|(s, _)| s.len() > 1)
            .map(|(s, rate)| Arc::from(resample(&s, rate as f32, sr, true))).collect();
        if !clips.is_empty() { self.clips[cue.index()] = clips; }
    }

    /// Replace a loop with a user-supplied clip (any rate, mono).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn override_loop(&mut self, which: Loop, clip: (Vec<f32>, u32)) {
        if clip.0.len() < 2 { return; }
        if let Some(i) = LOOPS.iter().position(|l| *l == which) {
            self.loops[i].clip = Arc::from(resample(&clip.0, clip.1 as f32, self.sr, false));
            self.loops[i].pos = 0.0;
        }
    }

    /// Load `<cue>.wav` / `<cue>-2.wav`… and `loop-<name>.wav` from `dir`,
    /// replacing the synthesized versions. Missing or unreadable files are
    /// skipped. Returns how many files were used.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_overrides(&mut self, dir: &std::path::Path) -> usize {
        let read = |name: &str| std::fs::read(dir.join(format!("{name}.wav"))).ok().and_then(|b| parse_wav(&b));
        let mut used = 0;
        for &cue in &CUES {
            let mut clips = Vec::new();
            if let Some(c) = read(cue.file_stem()) { clips.push(c); }
            for n in 2..=8 {
                match read(&format!("{}-{n}", cue.file_stem())) { Some(c) => clips.push(c), None => break }
            }
            used += clips.len();
            self.override_cue(cue, clips);
        }
        for &l in &LOOPS {
            if let Some(c) = read(l.file_stem()) { used += 1; self.override_loop(l, c); }
        }
        used
    }

    /// Queue a sound; applied at the start of the next render block.
    pub fn play(&mut self, p: Play) {
        if self.queue.len() < QUEUE { self.queue.push(p); }
    }
    pub fn set_loop(&mut self, which: Loop, t: LoopTarget) {
        if let Some(i) = LOOPS.iter().position(|l| *l == which) { self.loops[i].target = t; }
    }
    pub fn set_master(&mut self, gain: f32) { self.master_target = gain; }
    /// Indoors: a shorter, brighter, wetter room. Outdoors: a long dark tail.
    pub fn set_room(&mut self, indoor: bool) {
        if indoor { self.reverb.set(0.9, 0.55); self.room_target = INDOOR_WET; }
        else { self.reverb.set(2.4, 0.3); self.room_target = OUTDOOR_WET; }
    }
    /// Swap the map's ambient bed (mono, `source_rate` Hz); resampled once here.
    pub fn set_ambient(&mut self, samples: Option<(Vec<f32>, f32)>) {
        self.ambient = samples.filter(|(s, _)| !s.is_empty()).map(|(s, rate)| {
            let ratio = rate / self.sr;
            let len = (s.len() as f32 / ratio) as usize;
            let resampled: Vec<f32> = (0..len).map(|i| {
                let x = i as f32 * ratio;
                let j = x as usize;
                let f = x - j as f32;
                s[j.min(s.len() - 1)] * (1.0 - f) + s[(j + 1) % s.len()] * f
            }).collect();
            LoopVoice { clip: Arc::from(resampled), pos: 0.0, gain: 0.0, rate: 1.0, pan: 0.0, lp_a: 1.0, lp_z: 0.0,
                target: LoopTarget::default(), attack: self.smooth, release: self.smooth, wet: 0.3 }
        });
    }
    pub fn set_ambient_gain(&mut self, gain: f32) { self.ambient_target = gain; }
    #[cfg(test)]
    pub fn active_voices(&self) -> usize { self.voices.iter().filter(|v| v.clip.is_some()).count() }

    /// The active mods' sounds, by number (`Play::custom`), resampled here.
    pub fn set_custom(&mut self, clips: Vec<(Vec<f32>, u32)>) {
        self.custom = clips.into_iter().map(|(s, rate)| Arc::from(resample(&s, rate as f32, self.sr, false))).collect();
    }

    fn start(&mut self, p: Play) {
        let (ci, clip) = match p.custom {
            // A mod's sound: its own voice slot family after the cues'.
            Some(k) => match self.custom.get(k as usize) { Some(c) => (CUES.len() + k as usize, c.clone()), None => return },
            None => {
                let ci = p.cue.index();
                let Some(clip) = self.clips[ci].get(p.variant % self.clips[ci].len()).cloned() else { return };
                (ci, clip)
            }
        };
        let mut same_count = 0;
        let mut oldest_same: Option<(usize, f32)> = None;
        for (i, v) in self.voices.iter().enumerate() {
            if v.clip.is_some() && v.cue_index == ci {
                same_count += 1;
                let progress = v.pos / v.clip.as_ref().map_or(1.0, |c| c.len() as f32);
                if oldest_same.is_none_or(|(_, best)| progress > best) { oldest_same = Some((i, progress)); }
            }
        }
        let most = if p.custom.is_some() { 4 } else { p.cue.max_voices() };
        let slot = if same_count >= most {
            oldest_same.map(|(i, _)| i)
        } else if let Some(free) = self.voices.iter().position(|v| v.clip.is_none()) {
            Some(free)
        } else {
            // Steal the least important, most finished voice, but never one
            // that outranks the newcomer.
            let mut victim: Option<(usize, u8, f32)> = None;
            for (i, v) in self.voices.iter().enumerate() {
                let progress = v.pos / v.clip.as_ref().map_or(1.0, |c| c.len() as f32);
                let better = match victim {
                    None => true,
                    Some((_, pr, pg)) => v.priority < pr || (v.priority == pr && progress > pg),
                };
                if better { victim = Some((i, v.priority, progress)); }
            }
            victim.filter(|&(_, pr, _)| pr <= p.cue.priority()).map(|(i, _, _)| i)
        };
        let Some(i) = slot else { return };
        let pan = p.pan.clamp(-1.0, 1.0);
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let g = p.gain.clamp(0.0, 2.0);
        self.voices[i] = Voice {
            clip: Some(clip), cue_index: ci, priority: if p.custom.is_some() { 3 } else { p.cue.priority() }, pos: 0.0,
            rate: p.rate.clamp(0.25, 4.0), gl: g * angle.cos(), gr: g * angle.sin(),
            lp_a: lp_coef(p.cutoff, self.sr), lp_z: 0.0, delay: (p.delay.max(0.0) * self.sr) as u32,
            wet: p.cue.wet(),
        };
    }

    /// Render interleaved stereo into `out` (length must be even).
    pub fn render(&mut self, out: &mut [f32]) {
        let mut queue = std::mem::take(&mut self.queue);
        for p in queue.drain(..) { self.start(p); }
        self.queue = queue;
        let k = self.smooth;
        for frame in out.chunks_exact_mut(2) {
            let (mut l, mut r, mut send) = (0.0_f32, 0.0_f32, 0.0_f32);
            for v in self.voices.iter_mut() {
                let Some(clip) = &v.clip else { continue };
                if v.delay > 0 { v.delay -= 1; continue; }
                let i = v.pos as usize;
                if i + 1 >= clip.len() { v.clip = None; continue; }
                let f = v.pos - i as f32;
                let s = clip[i] * (1.0 - f) + clip[i + 1] * f;
                v.lp_z += (s - v.lp_z) * v.lp_a;
                v.pos += v.rate;
                l += v.lp_z * v.gl;
                r += v.lp_z * v.gr;
                send += v.lp_z * (v.gl + v.gr) * 0.5 * v.wet;
            }
            for lv in self.loops.iter_mut().chain(self.ambient.iter_mut()) {
                let t = lv.target;
                lv.gain += (t.gain - lv.gain) * if t.gain > lv.gain { lv.attack } else { lv.release };
                lv.rate += (t.rate.clamp(0.25, 4.0) - lv.rate) * k;
                lv.pan += (t.pan.clamp(-1.0, 1.0) - lv.pan) * k;
                lv.lp_a += (lp_coef(t.cutoff.max(60.0), self.sr) - lv.lp_a) * k;
                if lv.gain < 1e-4 { continue; }
                let n = lv.clip.len();
                let i = lv.pos as usize % n;
                let f = lv.pos.fract();
                let s = lv.clip[i] * (1.0 - f) + lv.clip[(i + 1) % n] * f;
                lv.pos = (lv.pos + lv.rate) % n as f32;
                lv.lp_z += (s - lv.lp_z) * lv.lp_a;
                let angle = (lv.pan + 1.0) * std::f32::consts::FRAC_PI_4;
                l += lv.lp_z * lv.gain * angle.cos();
                r += lv.lp_z * lv.gain * angle.sin();
                send += lv.lp_z * lv.gain * lv.wet;
            }
            if let Some(a) = &mut self.ambient { a.target.gain = self.ambient_target; a.target.rate = 1.0; a.target.cutoff = 18_000.0; }
            self.room_level += (self.room_target - self.room_level) * k;
            let (wl, wr) = self.reverb.tick(send, k);
            self.master += (self.master_target - self.master) * k;
            let ml = (l + wl * self.room_level) * self.master;
            let mr = (r + wr * self.room_level) * self.master;
            let target = limit_gain(ml.abs().max(mr.abs()));
            let rate = if target < self.lim { self.lim_attack } else { self.lim_release };
            self.lim += (target - self.lim) * rate;
            frame[0] = soft_clip(ml * self.lim);
            frame[1] = soft_clip(mr * self.lim);
        }
    }
}

fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

// -------------------------------------------------------------- spatial math

#[derive(Clone, Copy, Debug)]
pub struct Listener { pub pos: Vec3, pub forward: Vec3 }

/// Full volume within this distance (m); beyond it, gain falls as 1/distance.
pub const NEAR_FIELD: f32 = 8.0;

/// Gain, pan and low-pass for a world sound, or `None` beyond its range.
/// Gain follows the inverse-distance law beyond NEAR_FIELD (a shot 30 m away
/// is a fifth as loud, 70 m about a twentieth) and fades to zero at `range`;
/// far sounds lose their highs; sounds behind you are slightly quieter and duller.
pub fn spatialize(l: &Listener, src: Vec3, range: f32) -> Option<(f32, f32, f32, f32)> {
    let d = l.pos.distance(src);
    if !d.is_finite() || d >= range { return None; }
    let t = ((d - 5.0) / (range - 5.0)).clamp(0.0, 1.0);
    let mut gain = (NEAR_FIELD / d.max(NEAR_FIELD)) * (1.0 - t).powf(1.5);
    let fwd = Vec3::new(l.forward.x, 0.0, l.forward.z).normalize_or(Vec3::NEG_Z);
    let right = fwd.cross(Vec3::Y);
    let dir = Vec3::new(src.x - l.pos.x, 0.0, src.z - l.pos.z).normalize_or_zero();
    let pan = if d < 1.5 { 0.0 } else { dir.dot(right).clamp(-1.0, 1.0) * 0.85 };
    let behind = (-dir.dot(fwd)).max(0.0);
    gain *= 1.0 - 0.2 * behind;
    let cutoff = (16_000.0 * (1.0 - t).powf(1.6) + 700.0) * (1.0 - 0.3 * behind);
    Some((gain, pan, cutoff, d))
}

/// Speed of sound used for far explosions, so a distant blast arrives late.
pub const SOUND_SPEED: f32 = 340.0;

/// Idle hum levels for the weapon in hand: present, never in the way.
/// The disc hum is a background texture, well under the others' motors.
const IDLE_DISC: f32 = 0.022;
const IDLE_CHAIN: f32 = 0.1;
const IDLE_GRENADE: f32 = 0.1;
/// Football crowd bed, and how much it swells near a touchdown.

// ----------------------------------------------------------------- director

/// Turns world changes into cues and loop targets. Holds only small,
/// fixed-size state between frames.
pub struct Director {
    rng: Rng,
    map: Option<crate::terrain::MapId>,
    weapon: Option<u8>,
    was_ground: bool,
    last_vy: f32,
    step: f32,
    step_side: f32,
    remote_steps: [f32; 16],
    last_heal: f32,
    alive: bool,
    spin_rate: f32,
    shields: Vec<f32>,
    healths: Vec<f32>,
    equip_cool: Vec<f32>,
    grenades: Vec<(Vec3, f32)>,
    grenades_next: Vec<(Vec3, f32)>,
    bounce_cool: f32,
    indoor: bool,
    indoor_check: f32,
    variant: u32,
    /// Water immersion last frame: local player, then others by slot.
    wet: f32,
    remote_wet: [f32; 16],
}

impl Default for Director { fn default() -> Self { Self::new() } }

impl Director {
    pub fn new() -> Self {
        Director {
            rng: Rng(0x50D1), map: None, weapon: None, was_ground: true, last_vy: 0.0, step: 0.0,
            step_side: 1.0, remote_steps: [0.0; 16], last_heal: 0.0, alive: false, spin_rate: 0.0,
            shields: Vec::new(), healths: Vec::new(), equip_cool: Vec::new(),
            grenades: Vec::with_capacity(64), grenades_next: Vec::with_capacity(64), bounce_cool: 0.0, indoor: false, indoor_check: 0.0, variant: 0,
            wet: 0.0, remote_wet: [0.0; 16],
        }
    }

    pub fn indoor(&self) -> bool { self.indoor }

    fn pick(&mut self, cue: Cue) -> usize {
        self.variant = self.variant.wrapping_add(1 + (self.rng.next().abs() * 3.0) as u32);
        self.variant as usize % cue.variants()
    }
    fn jitter(&mut self) -> f32 { 1.0 + self.rng.next() * 0.04 }

    /// A cue at a world position, or `None` when out of range.
    pub fn world(&mut self, l: &Listener, cue: Cue, pos: Vec3, gain: f32) -> Option<Play> {
        let (g, pan, cutoff, d) = spatialize(l, pos, cue.range())?;
        let delay = if matches!(cue, Cue::BoomNear | Cue::BoomFar | Cue::GenBlast) && d > 40.0 { d / SOUND_SPEED } else { 0.0 };
        let variant = self.pick(cue);
        let rate = self.jitter();
        Some(Play { cue, variant, gain: g * gain, pan, cutoff, delay, rate, custom: None })
    }

    /// A non-positional cue for the local player (own weapon, HUD, UI).
    pub fn local(&mut self, cue: Cue) -> Play {
        let variant = self.pick(cue);
        let mut p = Play::local(cue, variant);
        if matches!(cue, Cue::ChainShot | Cue::DiscFire | Cue::GrenadeFire) { p.rate = self.jitter(); }
        p
    }

    /// An explosion reported by the sim: near/far layer by distance, and the
    /// big generator blast when a generator explosion is at that spot.
    pub fn explosion(&mut self, l: &Listener, world: &World, pos: Vec3) -> Option<Play> {
        let generator = world.explosions.iter().any(|e| e.kind == 4 && e.pos.distance(pos) < 4.0 && e.age < 0.5);
        let d = l.pos.distance(pos);
        let cue = if generator { Cue::GenBlast } else if d > 75.0 { Cue::BoomFar } else { Cue::BoomNear };
        self.world(l, cue, pos, 1.0)
    }

    /// Per-frame scan. Pushes one-shots into `out` (cleared by the caller)
    /// and returns loop targets.
    pub fn update(&mut self, world: &World, l: &Listener, dt: f32, playing: bool, out: &mut Vec<Play>) -> [LoopTarget; N_LOOPS] {
        let mut loops = [LoopTarget { gain: 0.0, rate: 1.0, pan: 0.0, cutoff: 18_000.0 }; N_LOOPS];
        let dt = dt.clamp(0.0, 0.1);
        if self.map != Some(world.map) {
            self.map = Some(world.map);
            self.shields.clear();
            self.healths.clear();
            self.equip_cool.clear();
            self.weapon = None;
        }
        self.indoor_check -= dt;
        if self.indoor_check <= 0.0 {
            self.indoor_check = 0.25;
            self.indoor = indoor_at(world.map, l.pos);
        }
        let Some(me) = world.players.get(world.player_id) else { return loops };
        if !playing { self.weapon = Some(me.weapon); self.alive = me.alive; return loops; }

        // Local body: steps, landings, weapon swaps, kits, loops.
        let speed = me.vel.length();
        let flat = Vec3::new(me.vel.x, 0.0, me.vel.z).length();
        let wet = if me.alive { peakrunner_core::water::immersion(world.map, &world.staged_water, me.pos).0 } else { 0.0 };
        if me.alive && wet > 0.0 && self.wet <= 0.0 && speed > 2.0 {
            let mut p = self.local(Cue::Splash);
            p.gain = (0.35 + speed / 50.0).clamp(0.35, 1.0);
            out.push(p);
        }
        self.wet = wet;
        if me.alive {
            if me.on_ground && !me.skiing && flat > 1.2 {
                self.step += dt * flat / 2.2;
                if self.step >= 1.0 {
                    self.step -= 1.0;
                    self.step_side = -self.step_side;
                    let mut p = self.local(if wet > 0.12 { Cue::Wade } else { Cue::Footstep });
                    p.gain = 0.55 * (flat / 10.0).clamp(0.4, 1.0);
                    p.pan = 0.12 * self.step_side;
                    out.push(p);
                }
            } else { self.step = 0.9; }
            // Only a landing hard enough to hurt makes a sound (the sim's
            // landing damage starts at 18 m/s).
            if me.on_ground && !self.was_ground && self.last_vy < -18.0 {
                let mut p = self.local(Cue::Land);
                p.gain = 0.5 + ((-self.last_vy - 18.0) / 25.0).clamp(0.0, 0.5);
                out.push(p);
            }
            if self.weapon.is_some_and(|w| w != me.weapon) { out.push(self.local(Cue::Switch)); }
            // The repair tool's hum starts when its beam catches.
            if me.repair_beam.is_some() && self.last_heal <= 0.0 && self.alive { out.push(self.local(Cue::RepairKit)); }
            if me.jetting {
                loops[0] = LoopTarget { gain: 0.24, rate: 0.9 + (me.vel.y / 40.0).clamp(-0.1, 0.25) + speed / 220.0, pan: 0.0, cutoff: 9_000.0 };
            }
            if me.skiing && me.on_ground {
                let g = (speed / 38.0).clamp(0.0, 1.0);
                loops[1] = LoopTarget { gain: 0.3 * g, rate: 0.8 + speed / 90.0, pan: 0.0, cutoff: 4_000.0 + speed * 150.0 };
            }
            let wind = ((speed - 8.0) / 55.0).clamp(0.0, 1.0) * if self.indoor { 0.2 } else { 1.0 };
            loops[2] = LoopTarget { gain: 0.34 * wind, rate: 0.9 + wind * 0.3, pan: 0.0, cutoff: 500.0 + speed * 70.0 };
        } else { self.step = 0.9; }
        // Mirrors the viewmodel spin in effects.rs so sound and barrels agree.
        let firing = me.alive && me.weapon == 1 && me.cooldown > 0.0;
        let target = if firing { 42.0 } else { 0.0 };
        let accel = if target > self.spin_rate { 110.0 } else { 20.0 };
        self.spin_rate += (target - self.spin_rate).clamp(-accel * dt, accel * dt);
        if self.spin_rate > 0.5 {
            let s = self.spin_rate / 42.0;
            loops[3] = LoopTarget { gain: 0.17 * s, rate: 0.45 + 0.9 * s, pan: 0.0, cutoff: 6_000.0 };
        }
        // The weapon in your hands hums quietly, ducked while it fires; the
        // mixer's smoothing crossfades between weapons on a switch. Football
        // has no weapons, so no hum.
        if me.alive && !world.ball.active {
            let (slot, level) = match me.weapon { 0 => (6, IDLE_DISC), 1 => (7, IDLE_CHAIN), _ => (8, IDLE_GRENADE) };
            let duck = if me.cooldown > 0.0 { 0.3 } else { 1.0 };
            loops[slot] = LoopTarget { gain: level * duck, rate: 1.0, pan: 0.0, cutoff: 6_000.0 };
        }
        self.was_ground = me.on_ground;
        self.last_vy = me.vel.y;
        self.last_heal = if me.repair_beam.is_some() { 1.0 } else { 0.0 };
        self.weapon = Some(me.weapon);
        self.alive = me.alive;

        // Other bodies: a splash when anyone in earshot enters water.
        for (i, p) in world.players.iter().enumerate() {
            if i == world.player_id { continue; }
            let slot = i % self.remote_wet.len();
            let wet = if p.alive { peakrunner_core::water::immersion(world.map, &world.staged_water, p.pos).0 } else { 0.0 };
            let was = std::mem::replace(&mut self.remote_wet[slot], wet);
            if wet > 0.0 && was <= 0.0 && p.vel.length() > 2.0 {
                let gain = (0.35 + p.vel.length() / 50.0).clamp(0.35, 1.0);
                if let Some(play) = self.world(l, Cue::Splash, p.pos, gain) { out.push(play); }
            }
        }
        // Footsteps for the three nearest walkers.
        let mut near = [(usize::MAX, f32::INFINITY); 3];
        for (i, p) in world.players.iter().enumerate() {
            if i == world.player_id || !p.alive || !p.on_ground || p.skiing { continue; }
            let d = p.pos.distance(l.pos);
            if d >= Cue::Footstep.range() { continue; }
            if let Some(slot) = near.iter().position(|n| d < n.1) {
                near[slot..].rotate_right(1);
                near[slot] = (i, d);
            }
        }
        for &(i, _) in near.iter().filter(|n| n.0 != usize::MAX) {
            let p = &world.players[i];
            let flat = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
            let slot = i % self.remote_steps.len();
            if flat < 1.2 { self.remote_steps[slot] = 0.9; continue; }
            self.remote_steps[slot] += dt * flat / 2.2;
            if self.remote_steps[slot] >= 1.0 {
                self.remote_steps[slot] -= 1.0;
                let wading = self.remote_wet[i % self.remote_wet.len()] > 0.12;
                if let Some(p) = self.world(l, if wading { Cue::Wade } else { Cue::Footstep }, p.pos, 0.8) { out.push(p); }
            }
        }

        // Equipment: shield pings, shield collapse, hull clanks, generator hum.
        let defs = peakrunner_core::equipment::definitions(world.map);
        if self.shields.len() != world.equipment.len() {
            self.shields = world.equipment.iter().map(|s| s.shield).collect();
            self.healths = world.equipment.iter().map(|s| s.health).collect();
            self.equip_cool = vec![0.0; world.equipment.len()];
        }
        let mut hum: Option<(f32, Vec3)> = None;
        for (i, (d, s)) in defs.iter().zip(&world.equipment).enumerate() {
            self.equip_cool[i] = (self.equip_cool[i] - dt).max(0.0);
            let pos = d.pos();
            if s.shield < self.shields[i] - 0.5 && self.equip_cool[i] <= 0.0 {
                let amount = ((self.shields[i] - s.shield) / 60.0).clamp(0.35, 1.0);
                if let Some(p) = self.world(l, Cue::ShieldHit, pos, amount) { out.push(p); }
                self.equip_cool[i] = 0.09;
            }
            if self.shields[i] > 0.0 && s.shield <= 0.0 && s.health > 0.0 {
                if let Some(p) = self.world(l, Cue::ShieldDown, pos, 1.0) { out.push(p); }
            }
            if s.health < self.healths[i] - 0.5 && s.shield <= 0.0 && s.health > 0.0 && self.equip_cool[i] <= 0.0 {
                if let Some(p) = self.world(l, Cue::HullHit, pos, 1.0) { out.push(p); }
                self.equip_cool[i] = 0.09;
            }
            self.shields[i] = s.shield;
            self.healths[i] = s.health;
            if d.kind == peakrunner_core::equipment::Kind::Generator && s.powered && !s.offline && s.health > 0.0 {
                let dist = pos.distance(l.pos);
                if dist < 32.0 && hum.is_none_or(|(best, _)| dist < best) { hum = Some((dist, pos)); }
            }
        }
        if let Some((dist, pos)) = hum {
            if let Some((_, pan, cutoff, _)) = spatialize(l, pos, 32.0) {
                let g = (1.0 - dist / 32.0).powi(2);
                loops[4] = LoopTarget { gain: 0.24 * g, rate: 1.0, pan, cutoff: cutoff.min(4_000.0) };
            }
        }

        // Projectiles: turret shots, grenade bounces, the hum of a passing disc.
        let step = dt.max(1.0 / 120.0) * 1.6;
        let mut disc_hum: Option<(f32, Vec3, Vec3)> = None;
        self.bounce_cool = (self.bounce_cool - dt).max(0.0);
        let grenades = std::mem::take(&mut self.grenades);
        let mut next = std::mem::take(&mut self.grenades_next);
        next.clear();
        let registry = peakrunner_core::registry::registry();
        for disc in &world.discs {
            // Sounds follow its look: the base projectile it resembles.
            let look = registry.look_kind(disc.kind);
            if disc.owner >= crate::sim::MAX_PLAYERS {
                let (cue, life) = if look == 3 { (Cue::TurretPlasma, 3.0) } else { (Cue::TurretBullet, 1.0) };
                if disc.life > life - step {
                    if let Some(p) = self.world(l, cue, disc.pos, 1.0) { out.push(p); }
                }
            }
            if look == 2 {
                let prev = grenades.iter().find(|(p, _)| p.distance(disc.pos) < 6.0).map(|(_, vy)| *vy);
                if prev.is_some_and(|vy| vy < -3.0) && disc.vel.y > 1.0 && self.bounce_cool <= 0.0 {
                    if let Some(p) = self.world(l, Cue::Bounce, disc.pos, 1.0) { out.push(p); }
                    self.bounce_cool = 0.05;
                }
                next.push((disc.pos, disc.vel.y));
            }
            if look == 0 && !(disc.owner == world.player_id && disc.pos.distance(l.pos) < 5.0) {
                let dist = disc.pos.distance(l.pos);
                if dist < 26.0 && disc_hum.is_none_or(|(best, _, _)| dist < best) { disc_hum = Some((dist, disc.pos, disc.vel)); }
            }
        }
        self.grenades = next;
        self.grenades_next = grenades;
        if let Some((dist, pos, vel)) = disc_hum {
            if let Some((_, pan, cutoff, _)) = spatialize(l, pos, 26.0) {
                let toward = (l.pos - pos).normalize_or_zero().dot(vel);
                let doppler = (1.0 + toward / SOUND_SPEED).clamp(0.8, 1.25);
                loops[5] = LoopTarget { gain: 0.22 * (1.0 - dist / 26.0).powi(2), rate: doppler, pan, cutoff };
            }
        }
        loops
    }
}

/// Under a roof or below the terrain surface.
fn indoor_at(map: crate::terrain::MapId, eye: Vec3) -> bool {
    if crate::terrain::height_on(map, eye.x, eye.z) > eye.y + 0.5 { return true; }
    peakrunner_core::map_pack::on(map).is_some_and(|pack| pack.sweep(eye, eye + Vec3::Y * 24.0, 0.0).is_some())
}

/// Linear PCM WAV bytes (mono or stereo, 16-bit) for listening to renders.
#[cfg(test)]
pub fn wav(samples: &[f32], sr: u32, channels: u16) -> Vec<u8> {
    let mut b = Vec::with_capacity(44 + samples.len() * 2);
    let data = (samples.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&sr.to_le_bytes());
    b.extend_from_slice(&(sr * channels as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(channels * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples { b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()); }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::MapId;

    #[test]
    fn every_cue_is_deterministic_bounded_and_clean_at_both_rates() {
        for sr in [44_100.0, 48_000.0] {
            for &cue in &CUES {
                for v in 0..cue.variants() {
                    let a = synth(cue, v, sr);
                    assert_eq!(a, synth(cue, v, sr), "{cue:?} not deterministic");
                    assert!(!a.is_empty());
                    assert!(a.iter().all(|s| s.is_finite()), "{cue:?} non-finite");
                    let peak = a.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                    assert!(peak > 0.05 && peak <= 0.9, "{cue:?} peak {peak}");
                    assert_eq!(a[0], 0.0);
                    assert!(a.last().unwrap().abs() < 0.01, "{cue:?} ends with a click");
                }
            }
        }
    }

    #[test]
    fn variants_actually_differ() {
        for &cue in &CUES {
            if cue.variants() > 1 { assert_ne!(synth(cue, 0, 44_100.0), synth(cue, 1, 44_100.0), "{cue:?}"); }
        }
    }

    #[test]
    fn loops_are_seamless_and_bounded() {
        for &l in &LOOPS {
            let d = synth_loop(l, 44_100.0);
            assert!(d.iter().all(|s| s.is_finite() && s.abs() <= 0.51));
            // The wrap-around step must look like any other step in the loop.
            let steepest = d.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0_f32, f32::max);
            assert!((d[0] - d[d.len() - 1]).abs() <= steepest, "{l:?} seam");
        }
    }

    #[test]
    fn every_sim_and_network_event_has_a_cue() {
        // Scan the real sources so a new event name can't slip in silently.
        for src in [include_str!("../crates/core/src/sim.rs"), include_str!("online.rs")] {
            for part in src.split("push_event(").skip(1).chain(src.split("spatial_sounds.push((").skip(1)) {
                let mut names = Vec::new();
                for quoted in part.split('"').skip(1).step_by(2).take(3) {
                    if quoted.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !quoted.is_empty() { names.push(quoted); }
                }
                let line = part.lines().next().unwrap_or("");
                for name in names.into_iter().filter(|n| line.contains(&format!("\"{n}\""))) {
                    assert!(Cue::from_event(name).is_some(), "event {name:?} has no cue");
                }
            }
            for word in ["capture_win", "capture_loss", "flag"] {
                if src.contains(&format!("\"{word},\"")) { assert!(Cue::from_event(word).is_some()); }
            }
        }
    }

    /// Transient onsets in a mono clip: rises above half the peak envelope
    /// after at least 15 ms below a quarter of it (the first rise counts).
    fn onsets(x: &[f32], sr: f32) -> usize {
        let k = ((sr * 0.003) as usize).max(1);
        let mut env = Vec::with_capacity(x.len());
        let mut acc = 0.0_f32;
        for (i, v) in x.iter().enumerate() {
            acc += v.abs();
            if i >= k { acc -= x[i - k].abs(); }
            env.push(acc / k as f32);
        }
        let peak = env.iter().cloned().fold(0.0_f32, f32::max) + 1e-9;
        let hop = (sr * 0.005) as usize;
        let (mut n, mut armed, mut low) = (0, true, 0usize);
        for e in env.iter().step_by(hop) {
            if *e < 0.25 * peak { low += 1 } else { low = 0 }
            if low * 5 >= 15 { armed = true; }
            if armed && *e > 0.5 * peak { n += 1; armed = false; low = 0; }
        }
        n
    }

    #[test]
    fn chaingun_and_footsteps_are_single_hits() {
        let sr = 44_100.0;
        for cue in [Cue::ChainShot, Cue::TurretBullet, Cue::Footstep] {
            for v in 0..cue.variants() {
                assert_eq!(onsets(&synth(cue, v, sr), sr), 1, "{cue:?} variant {v} must be one hit");
            }
        }
    }

    #[test]
    fn full_auto_chaingun_stays_within_its_voice_cap() {
        let sr = 44_100.0;
        let mut m = Mixer::new(sr);
        let mut chunk = vec![0.0; (0.075 * sr) as usize * 2];
        let mut peak_voices = 0;
        for i in 0..40 {
            m.play(Play::local(Cue::ChainShot, i));
            m.render(&mut chunk);
            let n = m.voices.iter().filter(|v| v.clip.is_some() && v.cue_index == Cue::ChainShot.index()).count();
            peak_voices = peak_voices.max(n);
            assert!(n <= Cue::ChainShot.max_voices());
        }
        // One voice per round; with 0.16 s tails at 0.075 s spacing about three overlap.
        assert!((2..=4).contains(&peak_voices), "{peak_voices} overlapping chaingun voices");
    }

    #[test]
    fn ski_stops_quickly_when_the_skis_lift() {
        let sr = 44_100.0;
        let mut m = Mixer::new(sr);
        let rms = |b: &[f32]| (b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32).sqrt();
        let mut on = vec![0.0; (sr * 0.6) as usize * 2];
        m.set_loop(Loop::Ski, LoopTarget { gain: 0.3, rate: 1.1, pan: 0.0, cutoff: 7000.0 });
        m.render(&mut on);
        let steady = rms(&on[on.len() / 2..]);
        // Onset: within 40 ms of touching down it is already near full level.
        let mut m2 = Mixer::new(sr);
        m2.set_loop(Loop::Ski, LoopTarget { gain: 0.3, rate: 1.1, pan: 0.0, cutoff: 7000.0 });
        let mut first = vec![0.0; (sr * 0.06) as usize * 2];
        m2.render(&mut first);
        assert!(rms(&first[(sr * 0.04) as usize * 2..]) > steady * 0.6, "ski onset too slow");
        // Release: 150 ms after lifting, the last 50 ms are 40 dB down.
        m.set_loop(Loop::Ski, LoopTarget { gain: 0.0, rate: 1.1, pan: 0.0, cutoff: 7000.0 });
        let mut off = vec![0.0; (sr * 0.15) as usize * 2];
        m.render(&mut off);
        let tail = rms(&off[off.len() - (sr * 0.05) as usize * 2..]);
        assert!(tail < steady * 0.01, "ski tail {tail} vs steady {steady}");
    }

    /// (share of the first 0.3 s's energy below ~150 Hz, share of all
    /// energy arriving after 1.0 s).
    fn boom_shape(x: &[f32], sr: f32) -> (f32, f32) {
        let a = tau_coef(1.0 / (std::f32::consts::TAU * 150.0), sr);
        let (mut z1, mut z2) = (0.0_f32, 0.0_f32);
        let (mut low, mut early, mut late, mut total) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for (i, v) in x.iter().enumerate() {
            z1 += (v - z1) * a;
            z2 += (z1 - z2) * a;
            let t = i as f32 / sr;
            total += v * v;
            if t < 0.3 { early += v * v; low += z2 * z2; }
            if t > 1.0 { late += v * v; }
        }
        (low / early.max(1e-9), late / total.max(1e-9))
    }

    #[test]
    fn explosions_are_short_and_deep() {
        let sr = 44_100.0;
        for cue in [Cue::BoomNear, Cue::BoomFar, Cue::GenBlast] {
            for v in 0..cue.variants() {
                let x = synth(cue, v, sr);
                let (low, late) = boom_shape(&x, sr);
                assert!(low > 0.35, "{cue:?} v{v}: first 0.3 s only {low:.2} below 150 Hz");
                assert!(late < 0.03, "{cue:?} v{v}: {late:.3} of its energy after 1 s");
                assert!(x.len() as f32 / sr <= 1.21, "{cue:?} too long");
            }
        }
    }

    #[test]
    fn stacked_explosions_do_not_clip() {
        let sr = 44_100.0;
        let mut m = Mixer::new(sr);
        for i in 0..4 { m.play(Play::local(Cue::BoomNear, i)); }
        m.play(Play::local(Cue::GenBlast, 0));
        let mut buf = vec![0.0; (sr * 3.0) as usize * 2];
        m.render(&mut buf);
        assert!(buf.iter().all(|s| s.abs() <= 1.0 && s.is_finite()));
    }

    #[test]
    fn voice_cap_and_priority_stealing() {
        let mut m = Mixer::new(22_050.0);
        let mut buf = vec![0.0; 64];
        for i in 0..40 { m.play(Play::local(Cue::Footstep, i)); }
        m.render(&mut buf);
        assert_eq!(m.active_voices(), Cue::Footstep.max_voices(), "per-cue cap");
        // Fill the pool with low-priority voices of many kinds.
        let fillers = [Cue::ChainShot, Cue::TurretBullet, Cue::ShieldHit, Cue::HullHit, Cue::Bounce, Cue::BoomNear, Cue::BoomFar, Cue::Land];
        for _ in 0..8 { for &c in &fillers { m.play(Play::local(c, 0)); } }
        m.render(&mut buf);
        assert!(m.active_voices() <= VOICES);
        // A high-priority cue always gets a voice even when the pool is full.
        for c in [Cue::Flag, Cue::CaptureWin, Cue::GenBlast] {
            m.play(Play::local(c, 0));
            m.render(&mut buf);
            assert!(m.voices.iter().any(|v| v.clip.is_some() && v.cue_index == c.index()), "{c:?} dropped");
        }
        assert!(m.active_voices() <= VOICES);
        assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn low_priority_never_steals_from_high_priority() {
        let mut m = Mixer::new(22_050.0);
        let mut buf = vec![0.0; 2];
        for v in m.voices.iter_mut() {
            *v = Voice { clip: Some(Arc::from(vec![0.1; 1000])), cue_index: Cue::Flag.index(), priority: 6, rate: 1.0, lp_a: 1.0, ..Voice::default() };
        }
        m.play(Play::local(Cue::Footstep, 0));
        m.render(&mut buf);
        assert!(!m.voices.iter().any(|v| v.cue_index == Cue::Footstep.index() && v.clip.is_some()));
    }

    #[test]
    fn distance_pan_and_filter_behave() {
        let l = Listener { pos: Vec3::ZERO, forward: Vec3::NEG_Z };
        let (near, _, near_cut, _) = spatialize(&l, Vec3::new(0.0, 0.0, -10.0), 150.0).unwrap();
        let (far, _, far_cut, _) = spatialize(&l, Vec3::new(0.0, 0.0, -100.0), 150.0).unwrap();
        assert!(near > far && near_cut > far_cut);
        assert!(spatialize(&l, Vec3::new(0.0, 0.0, -151.0), 150.0).is_none());
        assert!(spatialize(&l, Vec3::splat(f32::NAN), 150.0).is_none());
        let (_, right, ..) = spatialize(&l, Vec3::new(30.0, 0.0, 0.0), 150.0).unwrap();
        let (_, left, ..) = spatialize(&l, Vec3::new(-30.0, 0.0, 0.0), 150.0).unwrap();
        assert!(right > 0.5 && left < -0.5);
        let (front_g, _, front_c, _) = spatialize(&l, Vec3::new(0.0, 0.0, -30.0), 150.0).unwrap();
        let (back_g, _, back_c, _) = spatialize(&l, Vec3::new(0.0, 0.0, 30.0), 150.0).unwrap();
        assert!(back_g < front_g && back_c < front_c);
    }

    #[test]
    fn far_explosions_arrive_late_and_generators_get_the_big_blast() {
        let mut d = Director::new();
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        let l = Listener { pos: Vec3::ZERO, forward: Vec3::NEG_Z };
        let near = d.explosion(&l, &w, Vec3::new(0.0, 0.0, -20.0)).unwrap();
        let far = d.explosion(&l, &w, Vec3::new(0.0, 0.0, -300.0)).unwrap();
        assert_eq!(near.cue, Cue::BoomNear);
        assert_eq!(far.cue, Cue::BoomFar);
        assert!((far.delay - 300.0 / SOUND_SPEED).abs() < 0.01 && near.delay == 0.0);
        w.explosions.push(crate::sim::Explosion { pos: Vec3::new(0.0, 0.0, -60.0), age: 0.0, max_r: 20.0, kind: 4 });
        assert_eq!(d.explosion(&l, &w, Vec3::new(0.0, 0.0, -60.0)).unwrap().cue, Cue::GenBlast);
    }

    fn walker() -> (World, Director) {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_rift(true);
        (w, Director::new())
    }

    #[test]
    fn walking_makes_footsteps_and_skiing_hisses() {
        let (mut w, mut d) = walker();
        let me = w.player_id;
        w.players[me].alive = true;
        w.players[me].on_ground = true;
        w.players[me].skiing = false;
        w.players[me].vel = Vec3::new(8.0, 0.0, 0.0);
        let l = Listener { pos: w.players[me].pos, forward: Vec3::NEG_Z };
        let mut out = Vec::new();
        for _ in 0..60 { d.update(&w, &l, 1.0 / 60.0, true, &mut out); }
        let steps = out.iter().filter(|p| p.cue == Cue::Footstep).count();
        assert!((3..=5).contains(&steps), "{steps} steps in 1 s at 8 m/s");
        w.players[me].skiing = true;
        w.players[me].vel = Vec3::new(35.0, 0.0, 0.0);
        let loops = d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert!(loops[1].gain > 0.2, "ski hiss");
        assert!(loops[2].gain > 0.0, "wind at speed");
        let silent = d.update(&w, &l, 1.0 / 60.0, false, &mut out);
        assert!(silent.iter().all(|t| t.gain == 0.0), "nothing plays outside a match");
    }

    #[test]
    fn landing_switching_and_kits_cue_once() {
        let (mut w, mut d) = walker();
        let me = w.player_id;
        w.players[me].alive = true;
        let l = Listener { pos: w.players[me].pos, forward: Vec3::NEG_Z };
        let mut out = Vec::new();
        w.players[me].on_ground = false;
        w.players[me].vel = Vec3::new(0.0, -20.0, 0.0);
        d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        w.players[me].on_ground = true;
        w.players[me].vel = Vec3::ZERO;
        d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert_eq!(out.iter().filter(|p| p.cue == Cue::Land).count(), 1);
        w.players[me].weapon = (w.players[me].weapon + 1) % 3;
        w.players[me].repair_beam = Some(w.players[me].pos);
        d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert_eq!(out.iter().filter(|p| p.cue == Cue::Switch).count(), 1);
        assert_eq!(out.iter().filter(|p| p.cue == Cue::RepairKit).count(), 1);
    }

    #[test]
    fn shield_damage_pings_and_collapse_and_generator_hums() {
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold] {
            let mut w = World::new();
            w.set_map(map);
            w.start_rift(true);
            let defs = peakrunner_core::equipment::definitions(map);
            let Some(ti) = defs.iter().position(|d| d.kind == peakrunner_core::equipment::Kind::Turret) else { continue };
            let gi = defs.iter().position(|d| d.kind == peakrunner_core::equipment::Kind::Generator).unwrap();
            let mut d = Director::new();
            let l = Listener { pos: defs[ti].pos() + Vec3::new(8.0, 1.0, 0.0), forward: Vec3::NEG_Z };
            let mut out = Vec::new();
            d.update(&w, &l, 1.0 / 60.0, true, &mut out);
            w.equipment[ti].shield -= 50.0;
            d.update(&w, &l, 1.0 / 60.0, true, &mut out);
            assert!(out.iter().any(|p| p.cue == Cue::ShieldHit), "{map:?} shield ping");
            w.equipment[ti].shield = 0.0;
            for _ in 0..10 { d.update(&w, &l, 1.0 / 60.0, true, &mut out); }
            assert_eq!(out.iter().filter(|p| p.cue == Cue::ShieldDown).count(), 1, "{map:?}");
            let lg = Listener { pos: defs[gi].pos() + Vec3::new(3.0, 1.0, 0.0), forward: Vec3::NEG_Z };
            let loops = d.update(&w, &lg, 1.0 / 60.0, true, &mut out);
            assert!(loops[4].gain > 0.05, "{map:?} generator hum");
            w.equipment[gi].offline = true;
            let loops = d.update(&w, &lg, 1.0 / 60.0, true, &mut out);
            assert_eq!(loops[4].gain, 0.0, "{map:?} dead generator is silent");
        }
    }

    #[test]
    fn chaingun_spin_tracks_the_trigger() {
        let (mut w, mut d) = walker();
        let me = w.player_id;
        w.players[me].alive = true;
        w.players[me].weapon = 1;
        w.players[me].cooldown = 0.1;
        let l = Listener { pos: w.players[me].pos, forward: Vec3::NEG_Z };
        let mut out = Vec::new();
        let mut loops = [LoopTarget::default(); N_LOOPS];
        for _ in 0..30 { loops = d.update(&w, &l, 1.0 / 60.0, true, &mut out); }
        assert!(loops[3].gain > 0.1 && loops[3].rate > 1.0);
        w.players[me].cooldown = 0.0;
        for _ in 0..240 { loops = d.update(&w, &l, 1.0 / 60.0, true, &mut out); }
        assert_eq!(loops[3].gain, 0.0, "spin coasts to silence");
    }

    #[test]
    fn mixer_renders_finite_panned_audio() {
        let mut m = Mixer::new(44_100.0);
        let mut buf = vec![0.0; 44_100];
        m.play(Play { pan: 1.0, ..Play::local(Cue::DiscFire, 0) });
        m.render(&mut buf);
        let (l, r) = buf.chunks_exact(2).fold((0.0_f32, 0.0_f32), |(l, r), f| (l + f[0].abs(), r + f[1].abs()));
        assert!(r > l * 3.0, "hard right pan");
        assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn indoor_detects_roofs_and_tunnels() {
        assert!(!indoor_at(MapId::OldHoller, Vec3::new(1024.0, 2000.0, 1024.0)));
        // Deep under the terrain surface counts as a tunnel.
        let y = crate::terrain::height_on(MapId::OldHoller, 1024.0, 1024.0);
        assert!(indoor_at(MapId::OldHoller, Vec3::new(1024.0, y - 10.0, 1024.0)));
    }

    /// Share of a clip's energy below `cutoff` Hz (two cascaded one-poles).
    fn low_share(s: &[f32], sr: f32, cutoff: f32) -> f32 {
        let c = lp_coef(cutoff, sr);
        let (mut a, mut b, mut low, mut all) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for &x in s { a += (x - a) * c; b += (a - b) * c; low += b * b; all += x * x; }
        low / all.max(1e-9)
    }

    /// Share of a clip's energy above `cutoff` Hz (one-pole high-pass).
    fn high_share(s: &[f32], sr: f32, cutoff: f32) -> f32 {
        let c = lp_coef(cutoff, sr);
        let (mut a, mut high, mut all) = (0.0_f32, 0.0_f32, 0.0_f32);
        for &x in s { a += (x - a) * c; high += (x - a) * (x - a); all += x * x; }
        high / all.max(1e-9)
    }

    #[test]
    fn the_palette_has_weight() {
        // Floors calibrated against study references: weight without mud.
        let sr = 44_100.0;
        for (cue, min) in [(Cue::DiscFire, 0.12), (Cue::BoomNear, 0.3), (Cue::BoomFar, 0.5), (Cue::GenBlast, 0.3),
            (Cue::Land, 0.3), (Cue::ShieldHit, 0.1), (Cue::GrenadeFire, 0.25)] {
            let share = low_share(&synth(cue, 0, sr), sr, 150.0);
            assert!(share >= min, "{cue:?}: only {:.0}% below 150 Hz", share * 100.0);
        }
        // Loops: most of their energy sits below 500 Hz (the idle hum lives
        // around 200-400 Hz, not in the sub).
        for (l, min) in [(Loop::IdleDisc, 0.5), (Loop::Hum, 0.8), (Loop::Jet, 0.6)] {
            let share = low_share(&synth_loop(l, sr), sr, 500.0);
            assert!(share >= min, "{l:?}: only {:.0}% below 500 Hz", share * 100.0);
        }
    }

    /// A pass to you sounds like one flag flap: short, a crisp crack up
    /// front, the cloth's weight in the low mids, no bell. Floors from study
    /// references by this measure: a flag grab 40% below 700 Hz and 4%
    /// above 4 kHz, a lone flap 67% and under 1%.
    #[test]
    fn the_pass_warning_is_one_cloth_flap() {
        let sr = 44_100.0;
        let x = synth(Cue::Incoming, 0, sr);
        assert!(x.len() as f32 / sr <= 0.55, "short");
        let low = low_share(&x, sr, 700.0);
        assert!(low > 0.35, "the cloth's weight: {:.0}% below 700 Hz", low * 100.0);
        assert!(high_share(&x, sr, 4_000.0) < 0.1, "no hiss or bell");
        // The crack: the first 10 ms after the snap are brighter than the
        // rest, rustle and all.
        let at = (0.035 * sr) as usize;
        let crack = high_share(&x[at..at + (0.01 * sr) as usize], sr, 1_500.0);
        let rest = high_share(&x[at + (0.05 * sr) as usize..], sr, 1_500.0);
        assert!(crack > rest * 1.5, "a crisp crack up front: {crack:.2} vs {rest:.2}");
    }

    #[test]
    fn tonal_sounds_do_not_alias() {
        let sr = 44_100.0;
        for cue in [Cue::Flag, Cue::Start, Cue::CaptureWin, Cue::DiscReady, Cue::Death, Cue::ShieldDown, Cue::RepairKit] {
            let share = high_share(&synth(cue, 0, sr), sr, 12_000.0);
            assert!(share < 0.01, "{cue:?}: {:.2}% above 12 kHz", share * 100.0);
        }
        for l in [Loop::IdleDisc, Loop::Hum, Loop::DiscHum, Loop::Spin] {
            let share = high_share(&synth_loop(l, sr), sr, 12_000.0);
            assert!(share < 0.01, "{l:?}: {:.2}% above 12 kHz", share * 100.0);
        }
    }

    #[test]
    fn custom_wavs_parse_and_replace_synthesis() {
        let tone: Vec<f32> = (0..22_050).map(|i| (TAU * 220.0 * i as f32 / 22_050.0).sin() * 0.5).collect();
        let (mono, rate) = parse_wav(&wav(&tone, 22_050, 1)).unwrap();
        assert_eq!(rate, 22_050);
        assert!(mono.iter().zip(&tone).all(|(a, b)| (a - b).abs() < 1e-3));
        let stereo: Vec<f32> = tone.iter().flat_map(|s| [*s, *s]).collect();
        assert_eq!(parse_wav(&wav(&stereo, 22_050, 2)).unwrap().0.len(), tone.len());
        assert!(parse_wav(b"not a wav file at all").is_none());
        let mut m = Mixer::new(44_100.0);
        let before = m.clips[Cue::DiscFire.index()][0].len();
        m.override_cue(Cue::DiscFire, vec![(mono, rate)]);
        assert_eq!(m.clips[Cue::DiscFire.index()].len(), 1);
        assert_ne!(m.clips[Cue::DiscFire.index()][0].len(), before);
        assert!((m.clips[Cue::DiscFire.index()][0].len() as i64 - 44_100).abs() < 4, "resampled to the mixer rate");
        // A missing folder changes nothing.
        let mut m = Mixer::new(44_100.0);
        assert_eq!(m.load_overrides(std::path::Path::new("/definitely/not/here")), 0);
    }

    #[test]
    fn the_limiter_holds_a_pileup() {
        let mut m = Mixer::new(44_100.0);
        for i in 0..12 { m.play(Play { gain: 2.0, ..Play::local(Cue::BoomNear, i) }); m.play(Play { gain: 2.0, ..Play::local(Cue::GenBlast, 0) }); }
        let mut buf = vec![0.0; 44_100 * 2];
        m.render(&mut buf);
        assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let pinned = buf.iter().filter(|s| s.abs() > 0.99).count();
        assert!(pinned < buf.len() / 200, "{pinned} samples pinned at full scale");
    }

    #[test]
    fn the_held_weapon_hums_and_ducks_when_firing() {
        let (mut w, mut d) = walker();
        let me = w.player_id;
        w.players[me].alive = true;
        let l = Listener { pos: w.players[me].pos, forward: Vec3::NEG_Z };
        let mut out = Vec::new();
        w.players[me].weapon = 0;
        w.players[me].cooldown = 0.0;
        let idle = d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert!(idle[6].gain == IDLE_DISC && idle[7].gain == 0.0 && idle[8].gain == 0.0, "disc hum");
        w.players[me].cooldown = 0.5;
        let firing = d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert!(firing[6].gain < idle[6].gain * 0.5, "ducked while firing");
        w.players[me].weapon = 1;
        w.players[me].cooldown = 0.0;
        let chain = d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert!(chain[7].gain > 0.0 && chain[6].gain == 0.0, "chaingun idle");
        w.players[me].alive = false;
        let dead = d.update(&w, &l, 1.0 / 60.0, true, &mut out);
        assert!(dead[6..].iter().all(|t| t.gain == 0.0), "no hum when dead");
    }

    /// Football has no weapons and no crowd: nothing hums under the play, a
    /// touchdown sounds like a capture, and leaving the field beeps.
    #[test]
    fn football_is_quiet_between_plays() {
        let (mut w, mut d) = walker();
        let me = w.player_id;
        w.players[me].alive = true;
        w.players[me].weapon = 0;
        w.ball.active = true;
        let l = Listener { pos: w.players[me].pos, forward: Vec3::NEG_Z };
        let loops = d.update(&w, &l, 1.0 / 60.0, true, &mut Vec::new());
        assert!(loops[6..].iter().all(|t| t.gain == 0.0), "no weapon hum in Football");
        assert_eq!(Cue::from_event("touchdown_win"), Some(Cue::CaptureWin));
        assert_eq!(Cue::from_event("touchdown_loss"), Some(Cue::CaptureLoss));
        assert_eq!(Cue::from_event("oob"), Some(Cue::Warning));
        assert_eq!(Cue::from_event("incoming"), Some(Cue::Incoming));
        let warning = synth(Cue::Warning, 0, 44_100.0);
        assert!(warning.iter().any(|s| s.abs() > 0.1) && warning.len() < 44_100 / 2, "a short, audible beep");
    }

    /// Writes listenable samples to target/audio-samples/ (ignored in git).
    /// Run: cargo test -p peakrunner --lib render_audio_samples -- --ignored
    #[test]
    #[ignore]
    fn render_audio_samples() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/audio-samples");
        std::fs::create_dir_all(&dir).unwrap();
        let sr = 44_100.0;
        let write = |name: &str, s: &[f32], ch: u16| std::fs::write(dir.join(format!("{name}.wav")), wav(s, 44_100, ch)).unwrap();
        let mono = |name: &str, s: Vec<f32>| write(name, &s, 1);
        mono("disc-fire", synth(Cue::DiscFire, 0, sr));
        mono("grenade-fire", synth(Cue::GrenadeFire, 0, sr));
        mono("shield-hit", synth(Cue::ShieldHit, 1, sr));
        mono("shield-down", synth(Cue::ShieldDown, 0, sr));
        mono("generator-explosion", synth(Cue::GenBlast, 0, sr));
        mono("repair-kit", synth(Cue::RepairKit, 0, sr));
        mono("turret-plasma", synth(Cue::TurretPlasma, 0, sr));
        mono("flag-taken", synth(Cue::Flag, 0, sr));
        mono("capture-win", synth(Cue::CaptureWin, 0, sr));
        // Single hits: these pair with the one-shot references for A/B.
        mono("footsteps", synth(Cue::Footstep, 0, sr));
        mono("chaingun-burst", synth(Cue::ChainShot, 0, sr));
        // Sequences, built only by triggering single hits.
        mono("footsteps-walk", (0..6).flat_map(|v| { let mut s = synth(Cue::Footstep, v, sr); s.resize(16_000, 0.0); s }).collect());
        let mut m = Mixer::new(sr);
        let mut burst = vec![0.0; 44_100 * 2];
        m.set_loop(Loop::Spin, LoopTarget { gain: 0.17, rate: 1.35, pan: 0.0, cutoff: 6000.0 });
        let every = (0.075 * sr) as usize * 2;
        for (i, chunk) in burst.chunks_mut(every).enumerate() {
            if i < 13 { m.play(Play { rate: 1.0 + (i % 3) as f32 * 0.02, ..Play::local(Cue::ChainShot, i) }); }
            m.render(chunk);
        }
        write("chaingun-rapid", &burst, 2);
        // A short skid: skis touch for 0.3 s, then lift.
        let mut m = Mixer::new(sr);
        let mut skid = vec![0.0; (sr * 0.45) as usize * 2];
        let on = (sr * 0.3) as usize * 2;
        let (a, b) = skid.split_at_mut(on);
        m.set_loop(Loop::Ski, LoopTarget { gain: 0.3, rate: 1.1, pan: 0.0, cutoff: 7000.0 });
        m.render(a);
        m.set_loop(Loop::Ski, LoopTarget { gain: 0.0, rate: 1.1, pan: 0.0, cutoff: 7000.0 });
        m.render(b);
        write("ski-hiss", &skid, 2);
        let l = Listener { pos: Vec3::ZERO, forward: Vec3::NEG_Z };
        let mut d = Director::new();
        let w = World::new();
        for (name, dist) in [("explosion-near", 25.0), ("explosion-far", 260.0)] {
            let mut m = Mixer::new(sr);
            m.play(d.explosion(&l, &w, Vec3::new(dist * 0.4, 0.0, -dist)).unwrap());
            let mut s = vec![0.0; 44_100 * 2 * 4];
            m.render(&mut s);
            write(name, &s, 2);
        }
        for (name, lp, gain, cutoff) in [
            ("jet-loop", Loop::Jet, 0.24, 9000.0), ("ski-sustain", Loop::Ski, 0.3, 8000.0),
            ("wind-at-speed", Loop::Wind, 0.34, 3500.0), ("generator-hum", Loop::Hum, 0.24, 3000.0),
            ("disc-idle", Loop::IdleDisc, IDLE_DISC * 3.0, 6000.0), ("chaingun-idle", Loop::IdleChain, IDLE_CHAIN * 3.0, 6000.0),
            ("grenade-idle", Loop::IdleGrenade, IDLE_GRENADE * 3.0, 6000.0), ("disc-in-flight", Loop::DiscHum, 0.22, 9000.0),
        ] {
            let mut m = Mixer::new(sr);
            m.set_loop(lp, LoopTarget { gain, rate: 1.0, pan: 0.0, cutoff });
            let mut s = vec![0.0; 44_100 * 2 * 3];
            m.render(&mut s);
            write(name, &s, 2);
        }
        // Demo: disc idle, fire, a nearby explosion, a jet burst, then skiing.
        let mut m = Mixer::new(sr);
        let mut demo = vec![0.0_f32; 44_100 * 2 * 9];
        let block = 4_410;
        let idle = |g: f32| LoopTarget { gain: g, rate: 1.0, pan: 0.0, cutoff: 6000.0 };
        for (i, chunk) in demo.chunks_mut(block).enumerate() {
            let t = i as f32 * block as f32 / 2.0 / sr;
            m.set_loop(Loop::IdleDisc, idle(if t < 5.0 { if (1.5..2.1).contains(&t) { IDLE_DISC * 0.3 } else { IDLE_DISC } } else { 0.0 }));
            if i == 15 { m.play(Play::local(Cue::DiscFire, 0)); }
            if i == 26 { m.play(Play { pan: 0.35, gain: 0.8, ..Play::local(Cue::BoomNear, 1) }); }
            m.set_loop(Loop::Jet, LoopTarget { gain: if (5.0..6.8).contains(&t) { 0.24 } else { 0.0 }, rate: 1.05, pan: 0.0, cutoff: 9000.0 });
            let ski = (6.6..9.0).contains(&t);
            m.set_loop(Loop::Ski, LoopTarget { gain: if ski { 0.28 } else { 0.0 }, rate: 1.1, pan: 0.0, cutoff: 7000.0 });
            m.set_loop(Loop::Wind, LoopTarget { gain: if t > 5.0 { 0.25 } else { 0.0 }, rate: 1.0, pan: 0.0, cutoff: 3000.0 });
            m.render(chunk);
        }
        write("demo", &demo, 2);
    }
}
