//! Client-only weapon effects and first-person animation state.
//!
//! Nothing here feeds back into the simulation. It watches the world the client
//! already has (explosions, bullets, shot counters, the local player) and adds
//! lingering visuals the authoritative state does not carry: smoke that outlives
//! the 0.55 s blast record, thrown debris and casings, impact sparks, scorch
//! marks, plus the viewmodel's switch/bob/spin/heat animation. Every pool is a
//! fixed-size ring allocated once, so a busy fight never allocates per spark.
use glam::{Mat4, Vec2, Vec3};
use peakrunner_core::sim::{MatchState, World};
use peakrunner_core::terrain::{self, MapId};

use crate::drawlist::{EmitDraw, LitDraw, MeshId};

/// Hard cap on live particles of every kind (smoke, sparks, debris, casings,
/// scorch marks). The oldest is overwritten when the ring is full.
pub const MAX_PARTICLES: usize = 360;
/// Smoke trail puffs (`registry::Trail`) alive at once, in their own ring
/// after the particles' so a long lob never pushes out a blast's smoke; and
/// the rounds whose trails are followed at once.
pub const MAX_TRAIL_PUFFS: usize = 192;
const MAX_TRAILING: usize = 32;
/// Bullet impacts spawned per frame; a chaingun storm cannot flood the pool.
pub const MAX_IMPACTS_PER_FRAME: usize = 10;
pub const MAX_CASINGS: usize = 40;
pub const MAX_SCORCH: usize = 12;
/// Skim-water ripple rings alive at once, and new spray droplets per frame
/// across all players (nearest first), so wakes never crowd out blasts.
pub const MAX_RIPPLES: usize = 24;
pub const SPRAY_PER_FRAME: usize = 6;
/// Landing-splash droplets per frame across all players.
pub const SPLASH_PER_FRAME: usize = 20;
/// Beyond this range explosions keep only the stateless flash layers.
pub const DETAIL_RANGE: f32 = 220.0;
const IGNORE_RANGE: f32 = 450.0;
/// A smoke trail's puffs lie this many of their fresh size apart.
const TRAIL_SPACING: f32 = 5.0;
const SWITCH_TIME: f32 = 0.25;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind { Smoke, Trail, Spark, Debris, Casing, Scorch, Flash, Ripple, Droplet }

/// A round trailing smoke, as it was last frame, and the metres it has
/// flown since its last puff.
#[derive(Clone, Copy)]
struct Trailing { kind: u8, pos: Vec3, vel: Vec3, flown: f32, taken: bool }

#[derive(Clone, Copy)]
struct Particle {
    kind: Kind,
    pos: Vec3,
    vel: Vec3,
    normal: Vec3,
    age: f32,
    life: f32,
    size: f32,
    grow: f32,
    color: [f32; 3],
    alpha: f32,
    spin: f32,
    drag: f32,
    gravity: f32,
}

impl Particle {
    const DEAD: Particle = Particle { kind: Kind::Flash, pos: Vec3::ZERO, vel: Vec3::ZERO, normal: Vec3::Y,
        age: 1.0, life: 0.0, size: 0.0, grow: 0.0, color: [0.0; 3], alpha: 0.0, spin: 0.0, drag: 0.0, gravity: 0.0 };
    fn alive(&self) -> bool { self.age < self.life }
}

/// First-person animation. Recoil and flashes are read from the weapon
/// cooldown (stateless, so replays and captures are exact); switch, bob,
/// barrel spin, heat and the grenade cylinder need memory across frames.
#[derive(Clone, Debug)]
pub struct ViewAnim {
    pub weapon: u8,
    pub from: u8,
    pub switch: f32,
    pub spin: f32,
    pub spin_rate: f32,
    pub heat: f32,
    pub bob_phase: f32,
    pub bob: f32,
    pub ski: f32,
    pub air: f32,
    pub lift: f32,
    pub cylinder: u32,
    pub ready_flash: f32,
    last_shots: u32,
    last_cooldown: f32,
    started: bool,
}

impl Default for ViewAnim {
    fn default() -> Self {
        Self { weapon: 0, from: 0, switch: SWITCH_TIME, spin: 0.0, spin_rate: 0.0, heat: 0.0,
            bob_phase: 0.0, bob: 0.0, ski: 0.0, air: 0.0, lift: 0.0, cylinder: 0, ready_flash: 1.0,
            last_shots: 0, last_cooldown: 0.0, started: false }
    }
}

impl ViewAnim {
    /// Progress through the 0.25 s switch: the old weapon drops for the first
    /// half, the new one rises for the second. Returns (weapon shown, drop 0..1).
    pub fn shown(&self) -> (u8, f32) {
        let t = (self.switch / SWITCH_TIME).clamp(0.0, 1.0);
        if t >= 1.0 || self.from == self.weapon { return (self.weapon, 0.0); }
        if t < 0.5 { (self.from, ease(t * 2.0)) } else { (self.weapon, ease((1.0 - t) * 2.0)) }
    }

    fn update(&mut self, p: &peakrunner_core::sim::Player, dt: f32) {
        if !self.started {
            *self = Self { weapon: p.weapon, from: p.weapon, last_shots: p.shots, last_cooldown: p.cooldown,
                started: true, ..Self::default() };
        }
        if p.weapon != self.weapon {
            // Switching mid-switch lowers whatever is currently on screen.
            self.from = self.shown().0;
            self.weapon = p.weapon;
            self.switch = 0.0;
        }
        self.switch += dt;
        let fired = p.shots != self.last_shots || p.cooldown > self.last_cooldown + 0.01;
        let look = crate::sim::arsenal::view_kind(p.weapon);
        if fired && look == 2 { self.cylinder = self.cylinder.wrapping_add(1); }
        if p.cooldown <= 0.0 && self.last_cooldown > 0.0 { self.ready_flash = 0.0; }
        self.ready_flash += dt;
        self.last_shots = p.shots;
        self.last_cooldown = p.cooldown;

        // Chaingun barrels spin up while the trigger cycles and coast down
        // after release; the jacket heats and cools. Cosmetic only.
        let firing = look == 1 && p.cooldown > 0.0;
        let target = if firing { 42.0 } else { 0.0 };
        let accel = if target > self.spin_rate { 110.0 } else { 20.0 };
        self.spin_rate += (target - self.spin_rate).clamp(-accel * dt, accel * dt);
        self.spin = (self.spin + self.spin_rate * dt) % std::f32::consts::TAU;
        self.heat = (self.heat + if firing { dt * 0.45 } else { -dt * 0.3 }).clamp(0.0, 1.0);

        // Bob follows the body: a stride on foot, a low glide while skiing,
        // and a slow float in the air. Weights ease so states blend.
        let speed = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        let ski = if p.skiing && p.on_ground { 1.0 } else { 0.0 };
        let air = if p.on_ground { 0.0 } else { 1.0 };
        let k = (dt * 6.0).min(1.0);
        self.ski += (ski - self.ski) * k;
        self.air += (air - self.air) * k;
        let run = (1.0 - self.ski) * (1.0 - self.air);
        let amp = run * (speed / 9.0).min(1.0) + self.ski * 0.25 + self.air * 0.15;
        self.bob += (amp - self.bob) * k;
        let freq = run * (1.4 + speed * 0.07).min(2.6) + self.ski * 0.7 + self.air * 0.45;
        self.bob_phase = (self.bob_phase + dt * freq * std::f32::consts::TAU) % (std::f32::consts::TAU * 64.0);
        let lift = (-p.vel.y * 0.0025).clamp(-0.03, 0.03) * self.air;
        self.lift += (lift - self.lift) * k;
    }

    /// View-space offset from bob and air lag.
    pub fn bob_offset(&self, time: f32) -> Vec3 {
        let s = self.bob_phase.sin();
        let c = self.bob_phase.cos();
        let idle = (time * 1.4).sin() * 0.004;
        Vec3::new(s * 0.014 * self.bob + idle, -c.abs() * 0.018 * self.bob + self.lift, 0.0)
    }
}

fn ease(t: f32) -> f32 { let t = t.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

pub struct Effects {
    parts: Vec<Particle>,
    next: usize,
    next_trail: usize,
    trailing: Vec<Trailing>,
    trailing_spare: Vec<Trailing>,
    seen_blasts: Vec<(Vec3, u8, f32)>,
    prev_bullets: Vec<(Vec3, Vec3)>,
    cur_bullets: Vec<(Vec3, Vec3)>,
    matched: Vec<bool>,
    /// Each player's shot counter last frame, by `player_model::anim_id`.
    shots: Vec<(u64, u32)>,
    /// Each player's blade last frame (`sim::blade`), by the same key: the
    /// blows taken, and the swing and how far in.
    blades: Vec<(u64, u8, u8, u16)>,
    clock: f32,
    seed: std::cell::Cell<u32>,
    map: Option<MapId>,
    /// Each player's water immersion last frame, for splash-on-entry.
    wet: Vec<f32>,
    /// Each player was riding skim water last frame (a landing splashes).
    skimming: Vec<bool>,
    wake_clock: f32,
    pub anim: ViewAnim,
    /// Third-person landing and weapon-switch cues, per player.
    pub players: crate::player_model::AnimTracker,
    /// Each vent's last seen next eruption and whether its warning has
    /// sounded (`update_vents`).
    vent_seen: Vec<(u64, bool)>,
    /// Bombs' landing spots per vent: the eruption they belong to.
    vent_bombs: Vec<Vec<(u64, Vec<(Vec3, f32)>)>>,
    /// This frame's vent glows (where, glow, warning), landing marks,
    /// eruption fireballs (where, seconds since) and lava bombs in flight
    /// (where, which way, the core's radius, and whether it burns on touch),
    /// drawn by `draw`.
    vent_glows: Vec<(Vec3, f32, f32)>,
    bursts: Vec<(Vec3, f32)>,
    bombs: Vec<(Vec3, Vec3, f32, bool)>,
    landings: Vec<(Vec3, Vec3, f32, f32)>,
    /// World sounds the effects started this frame (a vent's rumble and
    /// eruption), for the app to play.
    pub sounds: Vec<(&'static str, Vec3)>,
}

impl Default for Effects { fn default() -> Self { Self::new() } }

impl Effects {
    pub fn new() -> Self {
        Self { parts: vec![Particle::DEAD; MAX_PARTICLES + MAX_TRAIL_PUFFS], next: 0, next_trail: 0,
            trailing: Vec::with_capacity(MAX_TRAILING), trailing_spare: Vec::with_capacity(MAX_TRAILING), seen_blasts: Vec::with_capacity(64),
            prev_bullets: Vec::with_capacity(128), cur_bullets: Vec::with_capacity(128), matched: Vec::with_capacity(128),
            shots: Vec::with_capacity(peakrunner_core::sim::MAX_PLAYERS), blades: Vec::with_capacity(peakrunner_core::sim::MAX_PLAYERS), clock: 0.0, seed: std::cell::Cell::new(0x9E37_79B9), map: None, wet: Vec::with_capacity(16), skimming: Vec::with_capacity(16), wake_clock: 0.0, anim: ViewAnim::default(), players: Default::default(),
            vent_seen: Vec::new(), vent_bombs: Vec::new(), vent_glows: Vec::new(), bursts: Vec::new(), bombs: Vec::new(), landings: Vec::new(), sounds: Vec::new() }
    }

    fn rand(&self) -> f32 {
        let mut x = self.seed.get();
        x ^= x << 13; x ^= x >> 17; x ^= x << 5;
        self.seed.set(x);
        (x >> 8) as f32 / 16_777_216.0
    }

    fn rand_dir(&self, up: f32) -> Vec3 {
        let a = self.rand() * std::f32::consts::TAU;
        let y = up + self.rand() * (1.0 - up);
        let r = (1.0 - y * y).max(0.0).sqrt();
        Vec3::new(a.cos() * r, y, a.sin() * r)
    }

    #[cfg(test)]
    pub fn live(&self) -> usize { self.parts.iter().filter(|p| p.alive()).count() }
    fn count(&self, kind: Kind) -> usize { self.parts.iter().filter(|p| p.alive() && p.kind == kind).count() }

    fn spawn(&mut self, p: Particle) {
        self.parts[self.next] = p;
        self.next = (self.next + 1) % MAX_PARTICLES;
    }

    fn spawn_trail(&mut self, p: Particle) {
        self.parts[MAX_PARTICLES + self.next_trail] = p;
        self.next_trail = (self.next_trail + 1) % MAX_TRAIL_PUFFS;
    }

    /// Advance everything by `dt` and pick up new explosions, impacts and shots.
    pub fn update(&mut self, world: &World, eye: Vec3, dt: f32) {
        // A ragdoll's floor: the ground under a point (just above it, so a
        // point that sank a little still finds the floor it went into).
        let map = world.map;
        // Football's bodies stay down a while after their players respawn.
        let keep = if world.ball.active { peakrunner_core::sim::football::BODY_LIES } else { 0.0 };
        self.players.update(&world.players, dt, &|a, b| world.clear_distance(a, b),
            &|q| peakrunner_core::terrain::support_on(map, q + Vec3::Y * (peakrunner_core::terrain::PLAYER_RADIUS + 0.3)).0, keep);
        if self.map != Some(world.map) {
            // A new map or round: nothing lingers from the last one.
            self.parts.fill(Particle::DEAD);
            self.trailing.clear();
            self.seen_blasts.clear();
            self.prev_bullets.clear();
            self.shots.clear();
            self.vent_seen.clear();
            self.vent_bombs.clear();
            self.map = Some(world.map);
        }
        self.clock += dt;
        self.update_vents(world, dt);
        self.update_updrafts(world, eye, dt);
        self.map_smoke(world, eye, dt);
        let map = world.map;
        for i in 0..self.parts.len() {
            let mut p = self.parts[i];
            if !p.alive() { continue; }
            p.age += dt;
            p.vel.y -= p.gravity * dt;
            p.vel *= (1.0 - p.drag * dt).max(0.0);
            p.pos += p.vel * dt;
            p.spin += dt * 9.0;
            if matches!(p.kind, Kind::Debris | Kind::Casing) && p.vel.y < 0.0 {
                let floor = ground_y(map, p.pos + Vec3::Y * 0.5);
                if p.pos.y < floor + p.size * 0.5 {
                    p.pos.y = floor + p.size * 0.5;
                    p.vel = Vec3::new(p.vel.x * 0.55, -p.vel.y * 0.3, p.vel.z * 0.55);
                }
            }
            self.parts[i] = p;
        }

        let clock = self.clock;
        self.seen_blasts.retain(|s| s.2 > clock);
        let splash = peakrunner_core::registry::registry().projectiles.index(peakrunner_core::sim::LAVA_SPLASH);
        for e in &world.explosions {
            if e.age > 0.3 { continue; }
            if self.seen_blasts.iter().any(|s| s.1 == e.kind && s.0.distance_squared(e.pos) < 0.25) { continue; }
            if self.seen_blasts.len() < 64 { self.seen_blasts.push((e.pos, e.kind, clock + 1.5)); }
            if splash == Some(e.kind as usize) { self.lava_splash(e.pos, e.max_r, eye); continue; }
            self.blast(map, e.pos, peakrunner_core::registry::registry().look_kind(e.kind), e.max_r, eye);
        }
        self.smoke_trails(world, eye, dt);
        if world.state != MatchState::Flyby {
            self.impacts(world, eye, dt);
            self.watch_shots(world, eye);
            self.watch_blades(world, eye);
        }
        self.water(world, eye, dt);
        if let Some(p) = world.players.get(world.player_id) { self.anim.update(p, dt); }
    }

    /// Lava thrown up by a burst in it (`sim::LAVA_SPLASH`): glowing gobs
    /// arcing out to the splash's reach and falling back, a bright ring on
    /// the surface and a little dark smoke.
    fn lava_splash(&mut self, pos: Vec3, radius: f32, eye: Vec3) {
        let distance = eye.distance(pos);
        if distance > IGNORE_RANGE { return; }
        let gobs = if distance < DETAIL_RANGE { 26 } else { 8 };
        for _ in 0..gobs {
            let d = self.rand_dir(0.55);
            let up = 7.0 + self.rand() * 7.0;
            let out = Vec3::new(d.x, 0.0, d.z) * (radius * (0.4 + self.rand() * 0.7));
            let hot = 0.7 + self.rand() * 0.3;
            self.spawn(Particle { kind: Kind::Spark, pos: pos + Vec3::Y * 0.1, vel: out + Vec3::Y * up, normal: Vec3::Y, age: 0.0,
                life: 0.9 + self.rand() * 0.5, size: 0.16 + self.rand() * 0.16, grow: 0.0, color: [1.0, 0.36 + 0.2 * hot, 0.08],
                alpha: 1.0, spin: 0.0, drag: 0.3, gravity: 18.0 });
        }
        let ring = Vec3::new(pos.x, pos.y - 0.06, pos.z);
        if self.count(Kind::Ripple) < MAX_RIPPLES {
            let spin = self.rand() * 6.3;
            self.spawn(Particle { kind: Kind::Ripple, pos: ring, vel: Vec3::ZERO, normal: Vec3::Y, age: 0.0, life: 0.9,
                size: 0.8, grow: radius * 1.6, color: [1.0, 0.55, 0.15], alpha: 0.85, spin, drag: 0.0, gravity: 0.0 });
        }
        for _ in 0..3 {
            let d = self.rand_dir(0.2);
            self.puff(pos + d * 0.6, Vec3::Y * (1.5 + self.rand()), [0.14, 0.1, 0.08], 0.45, 0.7, 2.4, 1.4 + self.rand() * 0.6);
        }
    }

    fn blast(&mut self, map: MapId, pos: Vec3, kind: u8, radius: f32, eye: Vec3) {
        let distance = eye.distance(pos);
        if distance > IGNORE_RANGE { return; }
        let detailed = distance < DETAIL_RANGE;
        let floor = ground_y(map, pos + Vec3::Y * 0.5);
        let near_ground = pos.y - floor < 2.5;
        let ground = ground_tint(map);
        let (smoke, sparks, debris, dark, spark_color, rise) = match kind {
            0 => (5, 7, 0, [0.52, 0.58, 0.64], [0.45, 0.85, 1.0], 0.9),
            3 => (2, 8, 0, [0.42, 0.52, 0.40], [0.45, 1.0, 0.3], 0.6),
            // Mortar: green sparks, dark smoke.
            5 => (10, 12, 8, [0.16, 0.2, 0.15], [0.45, 1.0, 0.25], 1.5),
            4 => (14, 16, 12, [0.10, 0.095, 0.09], [1.0, 0.62, 0.2], 2.0),
            _ => (8, 8, 7, [0.19, 0.18, 0.17], [1.0, 0.58, 0.16], 1.3),
        };
        if !detailed {
            // Distant blasts keep one smoke puff so the column still reads.
            self.puff(pos, Vec3::Y * rise, dark, 0.55, radius * 0.25, radius * 0.6, 1.6);
            return;
        }
        let scale = (radius / 8.0).clamp(0.5, 1.6);
        for _ in 0..smoke {
            let d = self.rand_dir(0.1);
            let spread = self.rand() * radius * 0.35;
            let life = 1.3 + self.rand() * 0.7 + if kind == 4 { 0.8 } else { 0.0 };
            // Smaller puffs with more lift break into a rising column rather
            // than one dark dome.
            let v = d * (1.5 + self.rand() * 2.0) * scale + Vec3::Y * rise * (1.0 + self.rand() * 1.4);
            let big = if kind == 4 { 1.5 } else { 1.0 };
            self.puff(pos + d * spread, v, dark, if kind == 4 { 0.66 } else { 0.5 },
                (0.4 + self.rand() * 0.3) * scale * big, (1.0 + self.rand() * 0.9) * scale * 1.3 * big, life);
        }
        if near_ground {
            let ground_pos = Vec3::new(pos.x, floor + 0.2, pos.z);
            for _ in 0..3 {
                let d = self.rand_dir(0.0) * Vec3::new(1.0, 0.25, 1.0);
                let v = d * (3.0 + self.rand() * 2.0) * scale + Vec3::Y * 0.6;
                self.puff(ground_pos, v, ground, 0.5, 0.6 * scale, 2.2 * scale, 0.9 + self.rand() * 0.4);
            }
            if self.count(Kind::Scorch) < MAX_SCORCH && kind != 3 {
                let n = ground_normal(map, ground_pos);
                self.spawn(Particle { kind: Kind::Scorch, pos: Vec3::new(pos.x, floor + 0.05, pos.z), vel: Vec3::ZERO,
                    normal: n, age: 0.0, life: 3.5, size: radius * 0.32, grow: 0.0, color: [0.05, 0.045, 0.04],
                    alpha: 0.55, spin: self.rand() * 6.3, drag: 0.0, gravity: 0.0 });
            }
        }
        for _ in 0..sparks {
            let d = self.rand_dir(0.05);
            let s = (8.0 + self.rand() * 12.0) * scale;
            self.spawn(Particle { kind: Kind::Spark, pos, vel: d * s, normal: Vec3::Y, age: 0.0,
                life: 0.35 + self.rand() * 0.3, size: 0.05, grow: 0.0, color: spark_color, alpha: 1.0,
                spin: 0.0, drag: 1.2, gravity: 14.0 });
        }
        let metal = [0.26, 0.28, 0.31];
        for _ in 0..debris {
            let d = self.rand_dir(0.35);
            let s = (6.0 + self.rand() * 7.0) * scale;
            let c = if kind == 4 { metal } else { [ground[0] * 0.7, ground[1] * 0.7, ground[2] * 0.7] };
            self.spawn(Particle { kind: Kind::Debris, pos: pos + Vec3::Y * 0.3, vel: d * s, normal: Vec3::Y,
                age: 0.0, life: 1.1 + self.rand() * 0.5, size: 0.12 + self.rand() * 0.14 * scale, grow: 0.0,
                color: c, alpha: 1.0, spin: self.rand() * 6.3, drag: 0.3, gravity: 20.0 });
        }
    }

    /// Splash when a player enters water (scaled by speed) and a light wake
    /// while they move through it.
    fn water(&mut self, world: &World, eye: Vec3, dt: f32) {
        const SPRAY: [f32; 3] = [0.82, 0.88, 0.92];
        self.skim(world, eye, dt);
        self.wet.resize(world.players.len(), 0.0);
        self.wake_clock += dt;
        let wake_now = self.wake_clock > 0.09;
        if wake_now { self.wake_clock = 0.0; }
        for (i, pl) in world.players.iter().enumerate() {
            let (wet, volume) = if pl.alive {
                peakrunner_core::water::immersion(world.map, &world.staged_water, pl.pos)
            } else { (0.0, None) };
            let was = std::mem::replace(&mut self.wet[i], wet);
            let Some(volume) = volume else { continue };
            if pl.pos.distance(eye) > DETAIL_RANGE { continue; }
            let at = Vec3::new(pl.pos.x, volume.surface + 0.05, pl.pos.z);
            let speed = pl.vel.length();
            if was <= 0.0 && wet > 0.0 && speed > 2.0 {
                let strength = (speed / 25.0).clamp(0.3, 2.0);
                for _ in 0..(6.0 * strength) as usize {
                    let d = self.rand_dir(0.55);
                    self.spawn(Particle { kind: Kind::Spark, pos: at, vel: d * (4.0 + self.rand() * 6.0) * strength,
                        normal: Vec3::Y, age: 0.0, life: 0.5 + self.rand() * 0.3, size: 0.07, grow: 0.0,
                        color: SPRAY, alpha: 0.9, spin: 0.0, drag: 0.6, gravity: 16.0 });
                }
                for _ in 0..(2.0 * strength).ceil() as usize {
                    let d = self.rand_dir(0.0) * Vec3::new(1.0, 0.2, 1.0);
                    self.puff(at, d * 2.5 * strength + Vec3::Y * 1.5, SPRAY, 0.35, 0.5 * strength, 1.6 * strength, 0.7);
                }
            } else if wake_now && wet > 0.0 && wet < 0.95 && Vec2::new(pl.vel.x, pl.vel.z).length() > 3.0 {
                let back = -Vec3::new(pl.vel.x, 0.0, pl.vel.z).normalize_or_zero();
                self.puff(at + back * 0.6, back * 1.2 + Vec3::Y * 0.4, SPRAY, 0.22, 0.35, 1.1, 0.55);
            }
        }
    }

    /// Skim water (Reefbreak's sea) is ridden, never entered: spray thrown
    /// back and to the sides by anyone crossing it, harder the faster they
    /// go, a ripple ring spreading from their track, and a splash when they
    /// land on it.
    fn skim(&mut self, world: &World, eye: Vec3, dt: f32) {
        const FOAM: [f32; 3] = [0.9, 0.95, 0.97];
        self.skimming.resize(world.players.len(), false);
        let mut order: Vec<usize> = (0..world.players.len()).collect();
        order.sort_by(|&a, &b| world.players[a].pos.distance_squared(eye).total_cmp(&world.players[b].pos.distance_squared(eye)));
        let mut budget = SPRAY_PER_FRAME;
        let mut splash = SPLASH_PER_FRAME;
        // Rates below are per 60 Hz frame; scale them by the real frame time.
        let frames = (dt * 60.0).clamp(0.0, 4.0);
        for i in order {
            let pl = &world.players[i];
            let surface = if pl.alive { peakrunner_core::water::skim_surface_at(world.map, pl.pos) } else { None };
            let on = surface.is_some_and(|s| pl.pos.y - peakrunner_core::terrain::PLAYER_RADIUS - s < 0.35);
            let was = std::mem::replace(&mut self.skimming[i], on);
            // Lava lying on the ground (`lava.bin`) isn't ridden: it burns
            // whoever walks in it, sparking round their feet.
            if !on && pl.alive && budget > 0 && pl.pos.distance(eye) < DETAIL_RANGE
                && peakrunner_core::water::lava_burn(world.map, pl.pos).is_some() && self.rand() < frames * 0.5 {
                budget -= 1;
                let d = self.rand_dir(0.0);
                let feet = pl.pos - Vec3::Y * peakrunner_core::terrain::PLAYER_RADIUS + Vec3::new(d.x, 0.0, d.z) * 0.35;
                self.spawn(Particle { kind: Kind::Spark, pos: feet, vel: Vec3::new(d.x * 1.5, 2.5 + self.rand() * 2.5, d.z * 1.5),
                    normal: Vec3::Y, age: 0.0, life: 0.45 + self.rand() * 0.3, size: 0.09, grow: 0.0, color: [1.0, 0.45, 0.1],
                    alpha: 1.0, spin: 0.0, drag: 0.8, gravity: 4.0 });
            }
            let Some(surface) = surface.filter(|_| on) else { continue };
            if pl.pos.distance(eye) > DETAIL_RANGE { continue; }
            let at = Vec3::new(pl.pos.x, surface + 0.04, pl.pos.z);
            let flat = Vec3::new(pl.vel.x, 0.0, pl.vel.z);
            let speed = flat.length();
            // Lava is a skim surface too. Sparks, not sea foam.
            if peakrunner_core::water::lava_burn(world.map, Vec3::new(pl.pos.x, surface, pl.pos.z)).is_some() {
                if !was && splash > 0 {
                    splash -= 1;
                    for _ in 0..6 {
                        let d = self.rand_dir(0.4);
                        self.spawn(Particle { kind: Kind::Spark, pos: at, vel: d * (4.0 + self.rand() * 6.0), normal: Vec3::Y, age: 0.0,
                            life: 0.35 + self.rand() * 0.3, size: 0.12, grow: 0.0, color: [1.0, 0.45, 0.1], alpha: 1.0,
                            spin: 0.0, drag: 0.8, gravity: 12.0 });
                    }
                } else if speed > 6.0 && budget > 0 {
                    budget -= 1;
                    let back = -flat / speed;
                    self.spawn(Particle { kind: Kind::Spark, pos: at + back * 0.4, vel: back * speed * 0.15 + Vec3::Y * (1.0 + self.rand() * 2.0),
                        normal: Vec3::Y, age: 0.0, life: 0.3, size: 0.1, grow: 0.0, color: [1.0, 0.4, 0.08], alpha: 1.0,
                        spin: 0.0, drag: 0.6, gravity: 10.0 });
                }
                continue;
            }
            if !was {
                // Landing: a burst and a wide ring, by how hard they came down.
                let hit = (pl.vel.y.abs().max(speed * 0.3) / 12.0).clamp(0.4, 2.0);
                let n = ((10.0 * hit) as usize).min(splash);
                splash -= n;
                for _ in 0..n {
                    let d = self.rand_dir(0.5);
                    self.spawn(Particle { kind: Kind::Droplet, pos: at, vel: d * (3.0 + self.rand() * 5.0) * hit,
                        normal: Vec3::Y, age: 0.0, life: 0.45 + self.rand() * 0.3, size: 0.07, grow: 0.0,
                        color: FOAM, alpha: 0.9, spin: 0.0, drag: 0.6, gravity: 16.0 });
                }
                self.ripple(at, 1.2 * hit, 5.0 * hit, 1.1);
                continue;
            }
            if speed < 2.0 { continue; }
            // Spray every frame, scaled by speed: a rooster tail when skiing.
            let back = -flat / speed;
            let side = Vec3::new(-back.z, 0.0, back.x);
            let rate = (speed / 10.0).clamp(0.3, 3.0);
            let want = rate * 2.0 * frames;
            let n = (want as usize + (self.rand() < want.fract()) as usize).min(budget);
            budget -= n;
            for _ in 0..n {
                let s = if self.rand() < 0.5 { -1.0 } else { 1.0 };
                let dir = (back * (0.6 + self.rand() * 0.6) + side * s * (0.3 + self.rand() * 0.7) + Vec3::Y * (0.5 + self.rand() * 0.6)).normalize();
                self.spawn(Particle { kind: Kind::Droplet, pos: at + back * 0.4, vel: dir * (2.0 + speed * (0.15 + self.rand() * 0.15)),
                    normal: Vec3::Y, age: 0.0, life: 0.4 + self.rand() * 0.3, size: 0.1 + 0.03 * rate, grow: 0.0,
                    color: FOAM, alpha: 0.85, spin: 0.0, drag: 0.9, gravity: 14.0 });
            }
            // A ripple ring left on the water every few tenths of a second.
            if self.rand() < (0.12 + speed / 200.0).min(0.3) * frames {
                self.ripple(at, 0.6, 2.4 + speed * 0.05, 0.9);
            }
            // A foam cloud behind anyone moving fast (skiing or jetting low).
            if speed > 10.0 && budget > 0 && self.rand() < (speed / 40.0).min(0.6) * frames {
                budget -= 1;
                self.puff(at + back * 1.2, back * 2.5 + Vec3::Y * 1.0, FOAM, 0.22, 0.45, 1.4 + speed * 0.02, 0.55);
            }
        }
    }

    /// A pale ring on a water surface that spreads out and fades.
    fn ripple(&mut self, at: Vec3, size: f32, grow: f32, life: f32) {
        if self.count(Kind::Ripple) >= MAX_RIPPLES { return; }
        let spin = self.rand() * 6.3;
        self.spawn(Particle { kind: Kind::Ripple, pos: at, vel: Vec3::ZERO, normal: Vec3::Y, age: 0.0, life, size, grow,
            color: [0.95, 0.99, 1.0], alpha: 0.62, spin, drag: 0.0, gravity: 0.0 });
    }

    /// Smoke trails (`registry::Trail`, style smoke): a puff every few
    /// of their sizes along each round's path, carried from frame to frame
    /// (the round matched to where it was), so they lie evenly whatever the
    /// frame rate; they drift, spread and fade where they were left. Far
    /// away they are fewer and bigger.
    fn smoke_trails(&mut self, world: &World, eye: Vec3, dt: f32) {
        use peakrunner_core::registry::TrailStyle;
        let registry = peakrunner_core::registry::registry();
        let mut last = std::mem::take(&mut self.trailing);
        let mut now = std::mem::take(&mut self.trailing_spare);
        now.clear();
        for d in &world.discs {
            let Some(trail) = registry.projectiles.get(d.kind as usize).and_then(|r| r.trail).filter(|t| t.style == TrailStyle::Smoke) else { continue };
            if now.len() >= MAX_TRAILING { break; }
            let tolerance = (d.vel.length() * dt * 0.5).max(0.75);
            let seen = last.iter_mut().find(|r| !r.taken && r.kind == d.kind && (r.pos + r.vel * dt).distance(d.pos) < tolerance);
            let range = d.pos.distance(eye);
            let size = trail.width() * (1.0 + range / 150.0).min(3.0);
            let spacing = size * TRAIL_SPACING;
            // A round seen for the first time puffs where it is.
            let (from, mut flown) = match seen { Some(r) => { r.taken = true; (r.pos, r.flown) } None => (d.pos, spacing) };
            let step = d.pos - from;
            let len = step.length();
            flown += len;
            if range > IGNORE_RANGE { flown %= spacing; }
            let dir = d.vel.normalize_or(Vec3::Y);
            let mut puffs = 0;
            while flown >= spacing {
                flown -= spacing;
                puffs += 1;
                if puffs > 8 { flown %= spacing; break; }
                // `flown` metres back from here along this frame's step.
                let at = d.pos - if len > 1e-4 { step * (flown / len).min(1.0) } else { Vec3::ZERO };
                let wander = self.rand_dir(-1.0);
                let puff = size * (0.8 + self.rand() * 0.4);
                self.spawn_trail(Particle { kind: Kind::Trail, pos: at + wander * size * 0.3, vel: d.vel * 0.04 + wander * 0.6, normal: dir, age: 0.0,
                    life: trail.life() * (0.85 + self.rand() * 0.3), size: puff, grow: puff * 3.5, color: trail.color, alpha: 0.36,
                    spin: self.rand() * 6.3, drag: 1.4, gravity: -0.3 });
            }
            now.push(Trailing { kind: d.kind, pos: d.pos, vel: d.vel, flown, taken: false });
        }
        self.trailing = now;
        self.trailing_spare = last;
    }

    /// The map's vents (`features::Vent`), from the shared schedule: a glow
    /// that simmers, swells through the warning and flares as it erupts;
    /// smoke and embers that thicken with the warning and burst out with the
    /// eruption; a glowing mark on each spot a bomb will land, from the
    /// warning until it lands; and the warning's rumble and the eruption's
    /// boom (`sounds`). Nothing shows or sounds before a round starts (the
    /// server throws no bombs then).
    fn update_vents(&mut self, world: &World, dt: f32) {
        use peakrunner_core::sim::features::{vent_clock, vents};
        self.vent_glows.clear();
        self.landings.clear();
        self.bursts.clear();
        self.bombs.clear();
        let list = vents(world.map);
        if list.is_empty() || world.countdown > 0.0 { return; }
        let t = vent_clock(world.steps);
        let rounds = &peakrunner_core::registry::registry().projectiles;
        // Lava bombs in flight (the map's own rounds): a glowing halo and a
        // trail of dark smoke and embers behind each. A fireball is the
        // wide one that burns on touch.
        for d in world.discs.iter().filter(|d| d.owner == peakrunner_core::sim::features::MAP_OWNER) {
            let fire = rounds.get(d.kind as usize).is_some_and(|r| r.burn > 0.0);
            let radius = rounds.get(d.kind as usize).map_or(1.6, |r| if fire { r.radius.max(2.4) } else { 1.6 });
            self.bombs.push((d.pos, d.vel, radius, fire));
            let (smoke, spark) = if fire { (2.4, 0.45) } else { (1.4, 0.18) };
            if self.rand() < 40.0 * dt {
                self.puff(d.pos - d.vel * 0.02, d.vel * 0.05 + Vec3::Y * 1.5, [0.14, 0.11, 0.1], 0.55, smoke, smoke * 3.5, 2.6);
            }
            if self.rand() < 25.0 * dt {
                let jitter = self.rand_dir(-1.0);
                self.spawn(Particle { kind: Kind::Spark, pos: d.pos, vel: d.vel * 0.15 + jitter * 4.0, normal: Vec3::Y, age: 0.0,
                    life: 0.5 + self.rand() * 0.4, size: spark, grow: 0.0, color: [1.0, 0.55, 0.15], alpha: 1.0,
                    spin: 0.0, drag: 0.8, gravity: 12.0 });
            }
        }
        self.vent_seen.resize(list.len(), (u64::MAX, false));
        self.vent_bombs.resize(list.len(), Vec::new());
        for (v, vent) in list.iter().enumerate() {
            let at = Vec3::from(vent.at);
            let ph = vent.phase(v, t);
            let (seen_next, warned) = self.vent_seen[v];
            let mut warned = warned && seen_next == ph.next;
            if ph.warning > 0.0 && !warned { self.sounds.push(("rumble", at)); warned = true; }
            let erupted = seen_next != u64::MAX && ph.last == Some(seen_next);
            let summit = list.iter().all(|u| vent.at[1] + 0.5 >= u.at[1]);
            if erupted {
                self.sounds.push(("eruption", at));
                // A billowing column of ash: big warm-grey puffs thrown up
                // fast and spreading as they rise. The summit's is the plume.
                let (ash, embers, lift) = if summit { (90, 220, 28.0) } else { (46, 120, 14.0) };
                for _ in 0..ash {
                    let d = self.rand_dir(0.55);
                    self.puff(at + d * 3.0, d * (8.0 + self.rand() * 10.0) + Vec3::Y * (lift + self.rand() * 22.0),
                        [0.28, 0.22, 0.2], 0.9, 8.0 + self.rand() * 6.0, 34.0, 9.0 + self.rand() * 4.0);
                }
                // A lava fountain: bright embers flung high and raining down.
                for _ in 0..embers {
                    let d = self.rand_dir(0.45);
                    self.spawn(Particle { kind: Kind::Spark, pos: at, vel: d * (22.0 + self.rand() * 36.0), normal: Vec3::Y, age: 0.0,
                        life: 1.6 + self.rand() * 1.4, size: 0.7 + self.rand() * 0.7, grow: 0.0, color: [1.0, 0.55, 0.12], alpha: 1.0,
                        spin: 0.0, drag: 0.2, gravity: 14.0 });
                }
            }
            if ph.since < 3.0 { self.bursts.push((at, ph.since)); }
            // After an eruption the ash keeps pouring up for a while.
            if ph.since < 6.0 && self.rand() < (14.0 - 2.0 * ph.since) * dt {
                let d = self.rand_dir(0.7);
                self.puff(at + d * 2.0, Vec3::Y * (12.0 + self.rand() * 8.0) + d * 3.0, [0.34, 0.3, 0.28], 0.7,
                    5.0 + self.rand() * 3.0, 20.0, 8.0);
            }
            self.vent_seen[v] = (ph.next, warned);
            // Smoke rises all the time, thicker as the warning builds; embers
            // spit through the warning.
            let rate = 1.2 + 9.0 * ph.warning;
            if self.rand() < rate * dt {
                let d = self.rand_dir(0.8);
                self.puff(at + d * 1.5, Vec3::Y * (3.0 + 5.0 * ph.warning) + d * 1.5, [0.2, 0.17, 0.16], 0.45 + 0.3 * ph.warning,
                    2.0 + self.rand(), 5.0, 5.0);
            }
            if ph.warning > 0.0 && self.rand() < 30.0 * ph.warning * dt {
                let d = self.rand_dir(0.6);
                self.spawn(Particle { kind: Kind::Spark, pos: at, vel: d * (5.0 + 12.0 * ph.warning), normal: Vec3::Y, age: 0.0,
                    life: 0.6 + self.rand() * 0.5, size: 0.14, grow: 0.0, color: [1.0, 0.6, 0.2], alpha: 1.0,
                    spin: 0.0, drag: 0.6, gravity: 16.0 });
            }
            let flare = (1.0 - ph.since / 2.5).max(0.0);
            let flicker = 0.85 + 0.15 * (self.clock * 11.0 + v as f32).sin();
            self.vent_glows.push((at, (0.35 + 1.4 * ph.warning * ph.warning + 2.2 * flare) * flicker, ph.warning));
            // Where the coming eruption's bombs land, brightening through the
            // warning; the last one's, pulsing faster until each lands.
            let mut marks = Vec::new();
            if ph.warning > 0.0 { marks.extend(self.bombs_of(world, v, ph.next).into_iter().map(|(p, _)| (p, ph.warning, 2.5))); }
            if let Some(last) = ph.last {
                for (p, flight) in self.bombs_of(world, v, last) {
                    if ph.since < flight + 0.15 { marks.push((p, 1.0, 4.0 + 8.0 * (ph.since / flight).min(1.0))); }
                }
            }
            for (p, strength, pulse) in marks {
                let n = peakrunner_core::terrain::normal_on(world.map, p.x, p.z);
                self.landings.push((p, n, strength, pulse));
            }
        }
    }

    /// Eruption `k`'s landing spots for vent `v`, worked out once each.
    fn bombs_of(&mut self, world: &World, v: usize, k: u64) -> Vec<(Vec3, f32)> {
        let cache = &mut self.vent_bombs[v];
        if let Some((_, b)) = cache.iter().find(|(e, _)| *e == k) { return b.clone(); }
        let b = peakrunner_core::sim::features::vents(world.map)[v].bombs(world.map, v, k);
        cache.push((k, b.clone()));
        if cache.len() > 3 { cache.remove(0); }
        b
    }

    /// The map's updrafts (`features::ZoneKind::Updraft`): embers and thin
    /// ash carried up through each column at about the speed it lifts a
    /// rider, fading out where its lift does. Only columns within sight
    /// spawn anything.
    fn update_updrafts(&mut self, world: &World, eye: Vec3, dt: f32) {
        use peakrunner_core::sim::features::{zones, ZoneKind, UPDRAFT_DRAG};
        for z in zones(world.map).iter().filter(|z| z.kind == ZoneKind::Updraft) {
            let at = Vec3::from(z.at);
            let half = Vec3::from(z.size);
            if (at - eye).length() > 700.0 + half.length() { continue; }
            let floor = at.y - half.y;
            let rise = ((z.lift - 20.0).max(4.0) / UPDRAFT_DRAG).min(30.0);
            let life = (2.0 * half.y / rise).clamp(1.5, 12.0);
            let area = (half.x * half.z).max(1.0);
            let point = |e: &Self| at + Vec3::new((e.rand() * 2.0 - 1.0) * half.x, 0.0, (e.rand() * 2.0 - 1.0) * half.z);
            if self.rand() < (0.12 * area).min(40.0) * dt {
                let mut p = point(self);
                p.y = floor + self.rand() * 2.0;
                let drift = self.rand_dir(0.0) * 0.6;
                self.spawn(Particle { kind: Kind::Spark, pos: p, vel: Vec3::Y * rise * (0.8 + self.rand() * 0.4) + drift, normal: Vec3::Y,
                    age: 0.0, life: life * (0.6 + self.rand() * 0.4), size: 0.12 + self.rand() * 0.12, grow: 0.0,
                    color: [1.0, 0.5 + self.rand() * 0.2, 0.14], alpha: 1.0, spin: 0.0, drag: 0.0, gravity: 0.0 });
            }
            if self.rand() < (0.03 * area).min(8.0) * dt {
                let mut p = point(self);
                p.y = floor + 1.0 + self.rand() * 4.0;
                self.puff(p, Vec3::Y * rise * 0.7, [0.52, 0.38, 0.3], 0.06, 1.0 + self.rand() * 1.0, 0.8, life * 0.6);
            }
        }
    }

    /// The map's smoke sources (`Manifest::smoke`): a chimney's plume or a
    /// fire's smoke, dark puffs rising and spreading as they drift, with
    /// embers over the small fires. Only sources within sight spawn
    /// anything.
    fn map_smoke(&mut self, world: &World, eye: Vec3, dt: f32) {
        let Some(pack) = peakrunner_core::map_pack::on(world.map) else { return };
        for s in &pack.manifest.smoke {
            let (at, size) = (Vec3::new(s[0], s[1], s[2]), s[3]);
            if (at - eye).length() > 900.0 { continue; }
            if self.rand() < (1.2 + size * 1.1) * dt {
                let jitter = Vec3::new(self.rand() - 0.5, 0.0, self.rand() - 0.5) * size * 0.5;
                let drift = Vec3::new(0.9, 0.0, 0.35) * (0.6 + self.rand() * 0.6);
                let rise = 1.6 + size * 0.9 + self.rand() * 1.2;
                self.puff(at + jitter, Vec3::Y * rise + drift, [0.16, 0.13, 0.12], 0.5, size * (0.7 + self.rand() * 0.4),
                    size * 0.55 + 0.4, 7.0 + size * 1.5 + self.rand() * 3.0);
            }
            if size < 2.0 && self.rand() < 6.0 * dt {
                let jitter = self.rand_dir(0.0) * size * 0.4;
                self.spawn(Particle { kind: Kind::Spark, pos: at + jitter, vel: Vec3::Y * (2.5 + self.rand() * 2.0) + jitter,
                    normal: Vec3::Y, age: 0.0, life: 0.8 + self.rand() * 0.8, size: 0.08 + self.rand() * 0.08, grow: 0.0,
                    color: [1.0, 0.55, 0.15], alpha: 1.0, spin: 0.0, drag: 0.6, gravity: -1.0 });
            }
        }
    }

    fn puff(&mut self, pos: Vec3, vel: Vec3, color: [f32; 3], alpha: f32, size: f32, grow: f32, life: f32) {
        let spin = self.rand() * 6.3;
        self.spawn(Particle { kind: Kind::Smoke, pos, vel, normal: Vec3::Y, age: 0.0, life, size, grow,
            color, alpha, spin, drag: 1.4, gravity: -0.4 });
    }

    /// A chaingun round that vanishes between frames and whose last step
    /// crosses a surface struck that surface: sparks and a dust puff there.
    fn impacts(&mut self, world: &World, eye: Vec3, dt: f32) {
        self.cur_bullets.clear();
        for d in &world.discs {
            if peakrunner_core::registry::registry().look_kind(d.kind) == 1 && self.cur_bullets.len() < 128 { self.cur_bullets.push((d.pos, d.vel)); }
        }
        self.matched.clear();
        self.matched.resize(self.cur_bullets.len(), false);
        let mut spawned = 0;
        for k in 0..self.prev_bullets.len() {
            let (pos, vel) = self.prev_bullets[k];
            let step = vel * dt;
            let predicted = pos + step;
            let tolerance = (step.length() * 0.4).max(1.0);
            let dir = vel.normalize_or_zero();
            let found = self.cur_bullets.iter().enumerate().position(|(i, (p, v))| {
                !self.matched[i] && p.distance(predicted) < tolerance && v.normalize_or_zero().dot(dir) > 0.95
            });
            if let Some(i) = found { self.matched[i] = true; continue; }
            if spawned >= MAX_IMPACTS_PER_FRAME || pos.distance(eye) > DETAIL_RANGE { continue; }
            let end = pos + vel * (dt * 1.6 + 0.03);
            if let Some((at, n)) = surface_hit(world.map, pos, end) {
                spawned += 1;
                self.impact(world.map, at, n, dir);
            }
        }
        std::mem::swap(&mut self.prev_bullets, &mut self.cur_bullets);
    }

    fn impact(&mut self, map: MapId, at: Vec3, normal: Vec3, dir: Vec3) {
        let reflect = (dir - normal * 2.0 * dir.dot(normal)).normalize_or(normal);
        for _ in 0..5 {
            let jitter = self.rand_dir(-1.0) * 0.55;
            let v = (reflect + normal * 0.4 + jitter).normalize_or(normal) * (5.0 + self.rand() * 7.0);
            self.spawn(Particle { kind: Kind::Spark, pos: at + normal * 0.05, vel: v, normal, age: 0.0,
                life: 0.18 + self.rand() * 0.18, size: 0.03, grow: 0.0, color: [1.0, 0.78, 0.35], alpha: 1.0,
                spin: 0.0, drag: 2.0, gravity: 12.0 });
        }
        self.spawn(Particle { kind: Kind::Flash, pos: at + normal * 0.08, vel: Vec3::ZERO, normal, age: 0.0,
            life: 0.05, size: 0.16, grow: 0.0, color: [1.0, 0.85, 0.55], alpha: 0.9, spin: 0.0, drag: 0.0, gravity: 0.0 });
        let dust = if normal.y > 0.6 { ground_tint(map) } else { [0.52, 0.52, 0.50] };
        self.puff(at + normal * 0.15, normal * 1.2, dust, 0.45, 0.14, 0.55, 0.55 + self.rand() * 0.25);
    }

    /// Casings and third-person muzzle flashes from the shot counters.
    fn watch_shots(&mut self, world: &World, eye: Vec3) {
        let me = world.player_id;
        for (i, p) in world.players.iter().enumerate() {
            // Empty seats (snapshots carry all 28) are nobody. A seat
            // without a network ID (offline) goes by its slot, which no
            // network ID can match: keyed by slot + 1, an empty seat's
            // counter (0) overwrote a player whose ID was that number, and
            // their every frame after their first shot fired a blast.
            if p.net_id == 0 && p.remote { continue; }
            let id = crate::player_model::anim_id(i, p);
            let before = match self.shots.iter_mut().find(|s| s.0 == id) {
                Some(s) => { let b = s.1; s.1 = p.shots; b }
                None => { if self.shots.len() < peakrunner_core::sim::MAX_PLAYERS { self.shots.push((id, p.shots)); } continue; }
            };
            if p.shots <= before || !p.alive || p.pos.distance(eye) > 120.0 { continue; }
            let yaw = if i == me { world.players[me].yaw } else { p.yaw };
            // In first person your own shot leaves the view's gun; watching
            // from behind, your model's, as everyone else's does.
            let own = i == me && !crate::drawlist::third_person_view(world);
            let drawn = crate::drawlist::drawn_at(world, i);
            let base = Mat4::from_translation(drawn) * Mat4::from_rotation_y(yaw);
            let right = base.transform_vector3(Vec3::X);
            let forward = base.transform_vector3(Vec3::NEG_Z);
            let (gun, muzzle) = if own {
                // The view's receiver, behind and above its anchor.
                let (e, d, _) = world.camera();
                let port = crate::sim::VM_ANCHOR_BOLT + Vec3::new(-0.04, 0.06, 0.27);
                (e + right * port.x + Vec3::Y * port.y - d * port.z, e + d * 0.9)
            } else {
                // The barrel's end as the body holds it; a weapon without a
                // model, about where the hands are.
                let body = crate::player_model::drawn_frame(p, drawn);
                let muzzle = crate::player_model::held_muzzle(p, drawn).unwrap_or_else(|| body.transform_point3(Vec3::new(0.13, 1.25, -1.0)));
                (muzzle - forward * 0.56, muzzle)
            };
            if crate::sim::arsenal::view_kind(p.weapon) == 1 && self.count(Kind::Casing) < MAX_CASINGS && p.shots % 2 == 0 {
                let v = right * (2.2 + self.rand()) + Vec3::Y * (1.8 + self.rand()) - forward * 0.4;
                self.spawn(Particle { kind: Kind::Casing, pos: gun, vel: v, normal: Vec3::Y, age: 0.0,
                    life: 0.9, size: 0.03, grow: 0.0, color: [0.82, 0.62, 0.26], alpha: 1.0,
                    spin: self.rand() * 6.3, drag: 0.2, gravity: 16.0 });
            }
            // Your own blast sits on the first-person barrel (the third-person
            // camera sees your model's), others' along their look.
            let (at, dir) = if own {
                let (e, d, fov) = world.camera();
                // A little past the tip: the gun is drawn over the world.
                (crate::sim::weapon_muzzle(e, d, p.weapon, fov) + d * 0.12, d)
            } else {
                let cp = p.pitch.cos();
                (muzzle, Vec3::new(-yaw.sin() * cp, p.pitch.sin(), -yaw.cos() * cp))
            };
            self.muzzle_blast(crate::sim::arsenal::view_kind(p.weapon), at, dir, own);
        }
    }

    /// Blades (`sim::blade`): a whoosh as each swing starts, and sparks and
    /// a sound where a blow lands: a cut, a blow on a guard, a clash, a guard
    /// broken. Read from each player's blade as it changes, so every client
    /// sees every blow.
    fn watch_blades(&mut self, world: &World, eye: Vec3) {
        use peakrunner_core::sim::blade;
        if !peakrunner_core::sim::features::has_blade_zones(world.map) { self.blades.clear(); return; }
        for (i, p) in world.players.iter().enumerate() {
            if p.net_id == 0 && p.remote { continue; }
            let id = crate::player_model::anim_id(i, p);
            let b = p.blade;
            let before = match self.blades.iter_mut().find(|s| s.0 == id) {
                Some(s) => { let was = (s.1, s.2, s.3); (s.1, s.2, s.3) = (b.struck, b.swing, b.t); was }
                None => { if self.blades.len() < peakrunner_core::sim::MAX_PLAYERS { self.blades.push((id, b.struck, b.swing, b.t)); } continue; }
            };
            if p.hidden() || p.pos.distance(eye) > 90.0 { continue; }
            // Heard and seen from your own floor, or where you can see them.
            let here = world.players.get(world.player_id).and_then(|me| peakrunner_core::sim::features::blade_zone(world.map, me.pos));
            if i != world.player_id && here != peakrunner_core::sim::features::blade_zone(world.map, p.pos)
                && !crate::drawlist::body_in_sight(world, eye, i) { continue; }
            let at = crate::drawlist::drawn_at(world, i) + Vec3::Y * 0.9;
            // A new swing: its whoosh, heavier for the heavy ones.
            if b.swing != blade::NONE && (b.swing != before.1 || b.t < before.2) {
                let heavy = matches!(b.swing, blade::POWER | blade::SLAM | blade::WHIRL);
                self.sounds.push((if heavy { "blade_heavy" } else { "blade_swing" }, at));
            }
            if b.struck == before.0 { continue; }
            // A blow: toward whose blade it was.
            let from = world.players.get(b.by as usize).map_or(at, |_| crate::drawlist::drawn_at(world, b.by as usize) + Vec3::Y * 0.9);
            let toward = (from - at).with_y(0.0).normalize_or(Vec3::NEG_Z);
            let spot = at + toward * 0.45 + Vec3::Y * 0.25;
            let who = world.players.get(b.by as usize).map_or(0, |q| if q.net_id != 0 { q.net_id } else { b.by as u32 + 1 });
            let glow = crate::blade_look::color(who).to_array();
            let (count, speed, color, sound) = match b.how {
                blade::CUT => (14, 6.0, [1.0, 0.62, 0.3], "blade_cut"),
                blade::BLOCKED => (22, 8.0, glow, "blade_block"),
                blade::CLASH => (40, 11.0, [1.0, 1.0, 0.95], "blade_clash"),
                _ => (30, 9.0, [1.0, 0.5, 0.2], "blade_clash"),
            };
            self.sounds.push((sound, spot));
            for _ in 0..count {
                let d = (self.rand_dir(-0.3) + toward * 0.6).normalize_or(Vec3::Y);
                self.spawn(Particle { kind: Kind::Spark, pos: spot, vel: d * speed * (0.5 + self.rand()), normal: Vec3::Y, age: 0.0,
                    life: 0.18 + self.rand() * 0.2, size: 0.016, grow: 0.0, color, alpha: 1.0, spin: 0.0, drag: 3.0, gravity: 9.0 });
            }
            self.spawn(Particle { kind: Kind::Flash, pos: spot, vel: Vec3::ZERO, normal: toward, age: 0.0, life: 0.07,
                size: if b.how == blade::CLASH { 0.5 } else { 0.28 }, grow: 0.0, color, alpha: 0.9, spin: 0.0, drag: 0.0, gravity: 0.0 });
        }
    }

    /// The flash of a shot leaving the gun. The disc launcher's stays a cold
    /// blue; the grenade launcher's is a small orange-red explosion that
    /// throws the round out, with sparks and a puff of smoke; the chaingun's
    /// is the same, much smaller, on every round. Your own are scaled down,
    /// being right in front of you (the gun's own flash carries the core).
    fn muzzle_blast(&mut self, weapon: u8, at: Vec3, dir: Vec3, own: bool) {
        let k = if own { 0.75 } else { 1.0 };
        let flash = |fx: &mut Self, pos: Vec3, vel: Vec3, size: f32, life: f32, color: [f32; 3]| {
            fx.spawn(Particle { kind: Kind::Flash, pos, vel, normal: dir, age: 0.0, life, size: size * k, grow: 0.0,
                color, alpha: 0.95, spin: 0.0, drag: 6.0, gravity: 0.0 });
        };
        let (side, up) = {
            let side = dir.cross(Vec3::Y).normalize_or(Vec3::X);
            (side, side.cross(dir))
        };
        match weapon {
            0 => if !own { flash(self, at, Vec3::ZERO, 0.32, 0.06, [0.45, 0.85, 1.0]); },
            1 => {
                flash(self, at, dir * 2.0, 0.07, 0.035, [1.0, 0.88, 0.55]);
                flash(self, at + dir * 0.07 * k, dir * 3.0, 0.12, 0.05, [1.0, 0.38, 0.1]);
                if self.rand() < 0.35 {
                    let v = (dir + self.rand_dir(0.0) * 0.35).normalize_or(dir) * (7.0 + self.rand() * 5.0);
                    self.spawn(Particle { kind: Kind::Spark, pos: at, vel: v, normal: dir, age: 0.0, life: 0.1,
                        size: 0.012 * k, grow: 0.0, color: [1.0, 0.55, 0.18], alpha: 1.0, spin: 0.0, drag: 3.0, gravity: 6.0 });
                }
            }
            2 => {
                // A hot core, a fireball rolling out after the round, flame
                // tongues round the bore, sparks, then smoke.
                flash(self, at, dir * 1.5, 0.15, 0.05, [1.0, 0.9, 0.6]);
                flash(self, at + dir * 0.16 * k, dir * 4.0, 0.27, 0.12, [1.0, 0.34, 0.07]);
                for n in 0..4 {
                    let a = n as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
                    let out = side * a.cos() + up * a.sin();
                    flash(self, at + out * 0.08 * k + dir * 0.05, out * 2.6 + dir * 1.5, 0.09, 0.08, [1.0, 0.24, 0.05]);
                }
                for _ in 0..5 {
                    let v = (dir + self.rand_dir(0.0) * 0.45).normalize_or(dir) * (8.0 + self.rand() * 6.0);
                    self.spawn(Particle { kind: Kind::Spark, pos: at, vel: v, normal: dir, age: 0.0, life: 0.14 + self.rand() * 0.08,
                        size: 0.018 * k, grow: 0.0, color: [1.0, 0.5, 0.15], alpha: 1.0, spin: 0.0, drag: 2.5, gravity: 8.0 });
                }
                self.puff(at + dir * 0.25, dir * 1.5 + Vec3::Y * 0.4, [0.62, 0.62, 0.60], 0.35, 0.18, 0.7, 0.7);
            }
            // The plasma gun's orange puff and the blaster's violet spark.
            crate::sim::arsenal::PLASMA => flash(self, at, dir * 2.0, if own { 0.14 } else { 0.3 }, 0.07, [1.0, 0.52, 0.14]),
            crate::sim::arsenal::BLASTER => flash(self, at, dir * 3.0, if own { 0.07 } else { 0.18 }, 0.04, [0.74, 0.44, 1.0]),
            _ => if !own { flash(self, at, Vec3::ZERO, 0.32, 0.06, [1.0, 0.55, 0.2]); },
        }
    }

    /// Append this frame's effect draws.
    pub fn draw(&self, lit: &mut Vec<LitDraw>, emit: &mut Vec<EmitDraw>, smoke: &mut Vec<EmitDraw>, eye: Vec3) {
        // Vents: a molten core and a halo round it; each landing spot a
        // glowing disc the size of the burst, pulsing, under a thin light
        // column that reads from afar.
        for &(at, glow, warning) in &self.vent_glows {
            lit.push(LitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(1.6 + 0.8 * glow)),
                color: Vec3::new(1.0, 0.42, 0.1), emit: 0.8 + glow, mode: 0.0 });
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(6.0 + 6.0 * glow)),
                color: [1.0, 0.38, 0.08, -0.25 * glow.min(3.0)] });
            // Through the warning, a shimmering column of heat over the vent.
            if warning > 0.0 {
                let wobble = 1.0 + 0.15 * (self.clock * 9.0).sin();
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at + Vec3::Y * (6.0 + 14.0 * warning))
                    * Mat4::from_scale(Vec3::new(2.5 * wobble, 6.0 + 14.0 * warning, 2.5 * wobble)), color: [1.0, 0.45, 0.12, 0.35 * warning] });
            }
        }
        // Updrafts: a faint shimmer of heat up each column, brightest low
        // down where it lifts hardest, wavering as it rises.
        let updrafts = self.map.map_or(&[][..], peakrunner_core::sim::features::zones);
        for z in updrafts.iter().filter(|z| z.kind == peakrunner_core::sim::features::ZoneKind::Updraft) {
            let at = Vec3::from(z.at);
            let half = Vec3::from(z.size);
            if (at - eye).length() > 900.0 + half.length() { continue; }
            let r = half.x.min(half.z);
            for i in 0..5 {
                let t = i as f32 / 4.0;
                let y = at.y - half.y + (0.1 + 0.8 * t) * 2.0 * half.y;
                let sway = (self.clock * (1.3 + 0.2 * i as f32) + t * 3.0).sin() * r * 0.12;
                let wobble = 1.0 + 0.1 * (self.clock * 4.0 + i as f32).sin();
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(Vec3::new(at.x + sway, y, at.z - sway))
                    * Mat4::from_scale(Vec3::new(r * 0.8 * wobble, half.y * 0.45, r * 0.8 * wobble)),
                    color: [1.0, 0.42, 0.12, -(0.07 * (1.0 - 0.7 * t))] });
            }
        }
        // An eruption: a fireball at the vent, swelling and fading, and a
        // pillar of fire shooting up through it.
        for &(at, since) in &self.bursts {
            let k = (since / 3.0).clamp(0.0, 1.0);
            let fade = (1.0 - k) * (1.0 - k);
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at + Vec3::Y * (4.0 + 10.0 * k))
                * Mat4::from_scale(Vec3::splat(8.0 + 22.0 * k)), color: [1.0, 0.5, 0.15, 0.9 * fade] });
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at + Vec3::Y * (20.0 + 30.0 * k))
                * Mat4::from_scale(Vec3::new(4.0 + 3.0 * k, 22.0 + 25.0 * k, 4.0 + 3.0 * k)), color: [1.0, 0.62, 0.22, 0.75 * fade] });
        }
        // Lava bombs: a hot halo round each. A fireball is a white-hot core,
        // a soft bloom and a tail along its flight.
        for &(at, vel, radius, fire) in &self.bombs {
            if fire {
                let dir = vel.normalize_or(Vec3::Y);
                let rot = glam::Quat::from_rotation_arc(Vec3::Y, dir);
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(radius * 0.42)),
                    color: [1.0, 0.95, 0.7, 1.0] });
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(radius)),
                    color: [1.0, 0.5, 0.1, 0.95] });
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(radius * 2.5)),
                    color: [1.0, 0.34, 0.06, -0.4] });
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at - dir * radius * 1.5)
                    * Mat4::from_quat(rot) * Mat4::from_scale(Vec3::new(radius * 0.7, radius * 3.6, radius * 0.7)),
                    color: [1.0, 0.26, 0.04, -0.45] });
            } else {
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(3.2)),
                    color: [1.0, 0.72, 0.28, 0.95] });
                emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(3.2)),
                    color: [1.0, 0.4, 0.08, 0.7] });
            }
        }
        for &(at, normal, strength, pulse) in &self.landings {
            let beat = 0.6 + 0.4 * (self.clock * pulse).sin();
            let rot = glam::Quat::from_rotation_arc(Vec3::Y, normal.normalize_or(Vec3::Y));
            smoke.push(EmitDraw { mesh: MeshId::Decal, model: Mat4::from_translation(at + normal * 0.08) * Mat4::from_quat(rot)
                * Mat4::from_scale(Vec3::new(8.0, 1.0, 8.0)), color: [1.0, 0.3, 0.05, 0.5 * strength * beat] });
            smoke.push(EmitDraw { mesh: MeshId::Decal, model: Mat4::from_translation(at + normal * 0.1) * Mat4::from_quat(rot)
                * Mat4::from_scale(Vec3::new(2.5, 1.0, 2.5)), color: [1.0, 0.75, 0.3, 0.7 * strength * beat] });
            emit.push(EmitDraw { mesh: MeshId::Sphere, model: Mat4::from_translation(at + Vec3::Y * 14.0) * Mat4::from_scale(Vec3::new(0.25, 14.0, 0.25)),
                color: [1.0, 0.35, 0.08, 0.25 * strength * beat] });
        }
        for p in self.parts.iter().filter(|p| p.alive()) {
            let t = (p.age / p.life).clamp(0.0, 1.0);
            match p.kind {
                Kind::Smoke => {
                    let size = p.size + p.grow * (1.0 - (1.0 - t) * (1.0 - t));
                    let fade_in = (p.age / 0.08).min(1.0);
                    let near = ((p.pos.distance(eye) - size * 0.6) / 1.0).clamp(0.0, 1.0);
                    let a = p.alpha * (1.0 - t).powf(1.4) * fade_in * near;
                    if a < 0.01 { continue; }
                    smoke.push(EmitDraw { mesh: MeshId::Sphere,
                        model: Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.spin * 0.05)
                            * Mat4::from_scale(Vec3::new(size, size * 0.86, size)),
                        color: [p.color[0], p.color[1], p.color[2], a] });
                }
                Kind::Trail => {
                    // Fresh, stretched along the flight to reach its
                    // neighbours, so the line reads unbroken behind the
                    // round; then round and spreading.
                    let size = p.size + p.grow * (1.0 - (1.0 - t) * (1.0 - t));
                    let near = ((p.pos.distance(eye) - size * 0.6) / 1.0).clamp(0.0, 1.0);
                    let a = p.alpha * (1.0 - t).powf(1.4) * near;
                    if a < 0.01 { continue; }
                    let along = p.normal * (size + p.size * TRAIL_SPACING * 0.5 * (1.0 - t).powi(3));
                    let mut side = p.normal.cross(Vec3::Y);
                    if side.length_squared() < 1e-4 { side = Vec3::X; }
                    let side = side.normalize();
                    let up = side.cross(p.normal);
                    smoke.push(EmitDraw { mesh: MeshId::Sphere,
                        model: Mat4::from_cols((side * size).extend(0.0), (up * size * 0.86).extend(0.0), along.extend(0.0), p.pos.extend(1.0)),
                        color: [p.color[0], p.color[1], p.color[2], a] });
                }
                Kind::Ripple => {
                    // Spreads fast then slows; fades as it grows.
                    let size = p.size + p.grow * (1.0 - (1.0 - t) * (1.0 - t));
                    let a = p.alpha * (1.0 - t) * (1.0 - t);
                    if a < 0.01 { continue; }
                    smoke.push(EmitDraw { mesh: MeshId::Decal,
                        model: Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.spin)
                            * Mat4::from_scale(Vec3::new(size, 1.0, size)),
                        color: [p.color[0], p.color[1], p.color[2], a] });
                }
                Kind::Scorch => {
                    let a = p.alpha * (1.0 - t);
                    let rot = glam::Quat::from_rotation_arc(Vec3::Y, p.normal.normalize_or(Vec3::Y));
                    smoke.push(EmitDraw { mesh: MeshId::Decal,
                        model: Mat4::from_translation(p.pos) * Mat4::from_quat(rot) * Mat4::from_rotation_y(p.spin)
                            * Mat4::from_scale(Vec3::new(p.size, 1.0, p.size * 0.8)),
                        color: [p.color[0], p.color[1], p.color[2], a] });
                }
                Kind::Spark => {
                    let dir = p.vel.normalize_or(Vec3::Y);
                    let len = (p.vel.length() * 0.035).clamp(0.05, 0.6);
                    let mut side = dir.cross(Vec3::Y);
                    if side.length_squared() < 1e-4 { side = Vec3::X; }
                    let side = side.normalize();
                    let up = side.cross(dir);
                    let w = p.size * (1.0 - t * 0.5);
                    emit.push(EmitDraw { mesh: MeshId::Sphere,
                        model: Mat4::from_cols((side * w).extend(0.0), (up * w).extend(0.0),
                            (dir * len).extend(0.0), p.pos.extend(1.0)),
                        color: [p.color[0], p.color[1], p.color[2], (1.0 - t) * p.alpha] });
                }
                Kind::Droplet => {
                    // Water, not light: alpha-blended, so it never blooms.
                    let dir = p.vel.normalize_or(Vec3::Y);
                    let len = (p.vel.length() * 0.03).clamp(0.06, 0.45);
                    let mut side = dir.cross(Vec3::Y);
                    if side.length_squared() < 1e-4 { side = Vec3::X; }
                    let side = side.normalize();
                    let up = side.cross(dir);
                    let w = p.size * (1.0 - t * 0.4);
                    let a = (1.0 - t) * p.alpha;
                    if a < 0.02 { continue; }
                    smoke.push(EmitDraw { mesh: MeshId::Sphere,
                        model: Mat4::from_cols((side * w).extend(0.0), (up * w).extend(0.0),
                            (dir * len).extend(0.0), p.pos.extend(1.0)),
                        color: [p.color[0], p.color[1], p.color[2], a] });
                }
                Kind::Flash => {
                    let s = p.size * (0.6 + t * 0.8);
                    emit.push(EmitDraw { mesh: MeshId::Sphere,
                        model: Mat4::from_translation(p.pos) * Mat4::from_scale(Vec3::splat(s)),
                        color: [p.color[0], p.color[1], p.color[2], (1.0 - t) * p.alpha] });
                }
                Kind::Debris | Kind::Casing => {
                    // Shrink out over the last third rather than popping.
                    let shrink = ((1.0 - t) * 3.0).min(1.0);
                    let s = p.size * shrink;
                    if s < 0.004 { continue; }
                    let scale = if p.kind == Kind::Casing { Vec3::new(s, s, s * 2.4) } else { Vec3::new(s, s * 0.8, s * 1.1) };
                    lit.push(LitDraw { mesh: if p.kind == Kind::Casing { MeshId::Disc } else { MeshId::Bevel },
                        model: Mat4::from_translation(p.pos) * Mat4::from_rotation_y(p.spin)
                            * Mat4::from_rotation_x(p.spin * 1.3) * Mat4::from_scale(scale),
                        color: Vec3::from_array(p.color), emit: if p.kind == Kind::Casing { 0.15 } else { 0.0 }, mode: 0.0 });
                }
            }
        }
    }
}

/// Soil or snow colour thrown up by blasts on each map (its `info.json` `dust`).
pub fn ground_tint(map: MapId) -> [f32; 3] {
    peakrunner_core::catalog::entry(map).dust
}

/// Highest support under `pos`: terrain or a building floor.
fn ground_y(map: MapId, pos: Vec3) -> f32 {
    let terrain = terrain::height_on(map, pos.x, pos.z);
    let floor = peakrunner_core::map_pack::on(map).and_then(|p| p.floor(pos)).map(|f| f.0);
    let mut y = match floor { Some(f) if f > terrain => f, _ => terrain };
    if let Some(s) = peakrunner_core::water::skim_surface_at(map, pos) { y = y.max(s); }
    y
}

fn ground_normal(map: MapId, at: Vec3) -> Vec3 {
    if let Some((_, n)) = peakrunner_core::map_pack::on(map).and_then(|p| p.floor(at + Vec3::Y * 0.3)) {
        if n.y > 0.3 { return n; }
    }
    terrain::normal_on(map, at.x, at.z)
}

/// First surface a segment crosses (terrain or map geometry), with its normal.
pub fn surface_hit(map: MapId, a: Vec3, b: Vec3) -> Option<(Vec3, Vec3)> {
    let mut best: Option<(f32, Vec3)> = None;
    if let Some(t) = terrain::segment_hit(map, a, b, 0.0) {
        let p = a.lerp(b, t);
        best = Some((t, terrain::normal_on(map, p.x, p.z)));
    }
    if let Some((t, n)) = peakrunner_core::map_pack::on(map).and_then(|p| p.sweep(a, b, 0.0)) {
        if best.is_none_or(|(old, _)| t < old) { best = Some((t, n)); }
    }
    best.map(|(t, n)| {
        let n = if n.dot(b - a) > 0.0 { -n } else { n };
        (a.lerp(b, t), n.normalize_or(Vec3::Y))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::sim::{Disc, Explosion, Team};

    fn world() -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_match(true);
        w.state = MatchState::Playing;
        w
    }

    /// On a server's 28 seats, an empty one (no network ID) never stands in
    /// for a player whose ID matches its slot: shots only show when the
    /// counter moves (a repeater once kept blasting after the trigger).
    #[test]
    fn muzzle_blasts_follow_shots_not_empty_seats() {
        let mut w = world();
        w.players.truncate(2);
        let me = 1;
        w.player_id = me;
        // Seat 0 is empty; the player in seat 1 has network ID 1.
        (w.players[0].net_id, w.players[0].remote, w.players[0].alive, w.players[0].shots) = (0, true, false, 0);
        (w.players[me].net_id, w.players[me].remote, w.players[me].shots) = (1, false, 0);
        // The repeater: its rounds flash at the barrel (the disc's don't, in first person).
        w.players[me].weapon = 1;
        assert_eq!(crate::sim::arsenal::view_kind(1), 1);
        let eye = w.camera().0;
        let mut fx = Effects::new();
        fx.update(&w, eye, 1.0 / 60.0);
        w.players[me].shots = 5;
        fx.update(&w, eye, 1.0 / 60.0);
        let fired = fx.count(Kind::Flash);
        assert!(fired > 0, "a shot shows");
        // The trigger let go: frames go by with the counter still.
        for _ in 0..90 { fx.update(&w, eye, 1.0 / 60.0); }
        assert_eq!(fx.count(Kind::Flash), 0, "no blasts without shots");
    }

    #[test]
    fn a_blast_spawns_lingering_capped_effects_once() {
        let mut w = world();
        let eye = w.camera().0;
        let pos = eye + Vec3::new(0.0, -2.0, -20.0);
        w.explosions.push(Explosion { pos, age: 0.0, max_r: 9.0, kind: 2 });
        let mut fx = Effects::new();
        fx.update(&w, eye, 1.0 / 60.0);
        let first = fx.live();
        assert!(first > 10, "a grenade blast spawns smoke, sparks and debris: {first}");
        // The same blast seen again in later snapshots is not re-spawned.
        for _ in 0..10 { fx.update(&w, eye, 1.0 / 60.0); }
        assert!(fx.live() <= first);
        // Smoke outlives the 0.55 s blast record.
        w.explosions.clear();
        for _ in 0..60 { fx.update(&w, eye, 1.0 / 60.0); }
        assert!(fx.count(Kind::Smoke) > 0);
        // Dozens of blasts never exceed the pool.
        for k in 0..60 {
            w.explosions.push(Explosion { pos: pos + Vec3::X * k as f32, age: 0.0, max_r: 9.0, kind: 4 });
            fx.update(&w, eye, 1.0 / 60.0);
        }
        assert!(fx.live() <= MAX_PARTICLES);
        assert!(fx.count(Kind::Scorch) <= MAX_SCORCH);
        let (mut lit, mut emit, mut smoke) = (Vec::new(), Vec::new(), Vec::new());
        fx.draw(&mut lit, &mut emit, &mut smoke, eye);
        for d in lit.iter() { assert!(d.model.is_finite()); }
        for d in emit.iter().chain(&smoke) { assert!(d.model.is_finite() && d.color.iter().all(|c| c.is_finite())); }
    }

    #[test]
    fn smoke_trails_keep_their_density_and_their_own_ring() {
        let mut w = world();
        let eye = w.camera().0;
        let shell = |x: f32| Disc { pos: eye + Vec3::new(x, 10.0, -30.0), vel: Vec3::new(0.0, 5.0, -65.0), team: Team::Ember, owner: 0,
            life: 18.0, kind: crate::sim::loadout::MORTAR_KIND, spin: 0.0, target: 0, power: 1.0 };
        assert!(peakrunner_core::registry::registry().projectiles.get(shell(0.0).kind as usize).and_then(|r| r.trail).is_some());
        // Half a second of flight leaves about as many puffs at 30 frames a
        // second as at 240.
        let mut puffs = |hz: f32| {
            let mut fx = Effects::new();
            w.discs = vec![shell(0.0)];
            for _ in 0..(hz * 0.5) as usize {
                fx.update(&w, eye, 1.0 / hz);
                let d = &mut w.discs[0];
                d.pos += d.vel / hz;
                d.life -= 1.0 / hz;
            }
            fx.count(Kind::Trail) as f32
        };
        let (slow, fast) = (puffs(30.0), puffs(240.0));
        assert!(slow > 12.0 && (slow / fast - 1.0).abs() < 0.35, "30 Hz {slow}, 240 Hz {fast}");
        // A sky full of shells never pushes out a blast's smoke.
        let mut fx = Effects::new();
        w.discs = (0..8).map(|k| shell(k as f32 * 3.0)).collect();
        w.explosions.push(Explosion { pos: eye + Vec3::new(0.0, -2.0, -20.0), age: 0.0, max_r: 9.0, kind: 2 });
        fx.update(&w, eye, 1.0 / 60.0);
        w.explosions.clear();
        for _ in 0..60 {
            for d in &mut w.discs { d.pos += d.vel / 60.0; d.life -= 1.0 / 60.0; }
            fx.update(&w, eye, 1.0 / 60.0);
        }
        assert!(fx.count(Kind::Smoke) > 0);
        assert!(fx.count(Kind::Trail) > MAX_TRAIL_PUFFS / 2 && fx.count(Kind::Trail) <= MAX_TRAIL_PUFFS);
        assert!(fx.parts[..MAX_PARTICLES].iter().all(|p| !p.alive() || p.kind != Kind::Trail));
        let (mut lit, mut emit, mut smoke) = (Vec::new(), Vec::new(), Vec::new());
        fx.draw(&mut lit, &mut emit, &mut smoke, eye);
        assert!(smoke.len() > MAX_TRAIL_PUFFS / 2);
        for d in &smoke { assert!(d.model.is_finite() && d.color.iter().all(|c| c.is_finite())); }
    }

    #[test]
    fn scorch_marks_draw_as_unflattened_decals() {
        // A scorch drawn as a disc flattened by a tiny Y scale lost its faces to
        // the facing fade, leaving only a thin rim on screen.
        let mut w = world();
        let eye = w.camera().0;
        w.explosions.push(Explosion { pos: eye + Vec3::new(0.0, -2.0, -20.0), age: 0.0, max_r: 9.0, kind: 2 });
        let mut fx = Effects::default();
        fx.update(&w, eye, 1.0 / 60.0);
        assert_eq!(fx.count(Kind::Scorch), 1);
        let (mut lit, mut emit, mut smoke) = (Vec::new(), Vec::new(), Vec::new());
        fx.draw(&mut lit, &mut emit, &mut smoke, eye);
        let decals: Vec<_> = smoke.iter().filter(|d| d.mesh == MeshId::Decal).collect();
        assert_eq!(decals.len(), 1);
        assert!(!smoke.iter().any(|d| d.mesh == MeshId::Disc));
        let up = decals[0].model.transform_vector3(Vec3::Y).length();
        assert!(up > 0.5, "decal flattened by its model scale: {up}");
    }

    #[test]
    fn skim_water_throws_spray_and_ripples_within_budget() {
        let mut w = World::new();
        w.set_map(MapId::Reefbreak);
        w.start_match(true);
        w.state = MatchState::Playing;
        let sea = peakrunner_core::water::volumes(MapId::Reefbreak)[0].surface;
        let at = Vec3::new(1294.0, sea + terrain::PLAYER_RADIUS, 1346.0);
        for p in w.players.iter_mut() { p.pos = at + Vec3::new(0.0, 50.0, 0.0); p.vel = Vec3::ZERO; }
        w.players[1].pos = at;
        w.players[1].vel = Vec3::new(24.0, 0.0, 0.0);
        w.players[1].alive = true;
        let eye = at + Vec3::new(-12.0, 3.0, 8.0);
        let mut fx = Effects::new();
        for _ in 0..60 { fx.update(&w, eye, 1.0 / 60.0); }
        let (spray, rings) = (fx.count(Kind::Droplet), fx.count(Kind::Ripple));
        println!("skim: {spray} spray droplets, {rings} ripple rings alive");
        assert!(spray >= 6 && rings >= 1, "a skier on skim water sprays and leaves ripples");
        assert!(rings <= MAX_RIPPLES && spray <= SPRAY_PER_FRAME * 40);
    }

    #[test]
    fn distant_blasts_skip_debris_and_sparks() {
        let mut w = world();
        let eye = w.camera().0;
        w.explosions.push(Explosion { pos: eye + Vec3::new(0.0, 0.0, -300.0), age: 0.0, max_r: 9.0, kind: 2 });
        let mut fx = Effects::new();
        fx.update(&w, eye, 1.0 / 60.0);
        assert_eq!(fx.count(Kind::Spark) + fx.count(Kind::Debris), 0);
        assert!(fx.live() <= 1);
    }

    #[test]
    fn a_round_that_vanishes_into_the_ground_sparks_and_one_in_flight_does_not() {
        let mut w = world();
        let eye = w.camera().0;
        let target = Vec3::new(eye.x, terrain::height_on(w.map, eye.x, eye.z - 15.0), eye.z - 15.0);
        let from = target + Vec3::new(0.0, 3.0, 3.0);
        let vel = (target - from).normalize() * 420.0;
        w.discs.push(Disc { pos: from, vel, team: Team::Ember, owner: 0, life: 1.0, kind: 1, spin: 0.0, target: 0, power: 1.0 });
        let mut fx = Effects::new();
        fx.update(&w, eye, 1.0 / 60.0);
        assert_eq!(fx.count(Kind::Spark), 0, "a live round is not an impact");
        w.discs.clear();
        fx.update(&w, eye, 1.0 / 60.0);
        assert!(fx.count(Kind::Spark) >= 5, "the vanished round struck the ground");
        // A round that simply leaves (expires in open air) spawns nothing.
        let mut fx = Effects::new();
        w.discs.push(Disc { pos: eye + Vec3::Y * 40.0, vel: Vec3::Y * 420.0, team: Team::Ember, owner: 0, life: 0.01, kind: 1, spin: 0.0, target: 0, power: 1.0 });
        fx.update(&w, eye, 1.0 / 60.0);
        w.discs.clear();
        fx.update(&w, eye, 1.0 / 60.0);
        assert_eq!(fx.count(Kind::Spark), 0);
    }

    #[test]
    fn switch_drops_the_old_weapon_then_raises_the_new_one() {
        let mut w = world();
        let me = w.player_id;
        w.players[me].weapon = 0;
        let mut fx = Effects::new();
        let eye = w.camera().0;
        fx.update(&w, eye, 0.016);
        assert_eq!(fx.anim.shown(), (0, 0.0));
        w.players[me].weapon = 1;
        fx.update(&w, eye, 0.06);
        let (shown, drop) = fx.anim.shown();
        assert_eq!(shown, 0);
        assert!(drop > 0.2);
        fx.update(&w, eye, 0.1);
        assert_eq!(fx.anim.shown().0, 1);
        fx.update(&w, eye, 0.2);
        assert_eq!(fx.anim.shown(), (1, 0.0));
    }

    #[test]
    fn chaingun_barrels_spin_up_and_coast_down() {
        let mut w = world();
        let me = w.player_id;
        w.players[me].weapon = 1;
        let mut fx = Effects::new();
        let eye = w.camera().0;
        fx.update(&w, eye, 0.016);
        for _ in 0..60 {
            w.players[me].cooldown = 0.05;
            fx.update(&w, eye, 0.016);
        }
        let spun = fx.anim.spin_rate;
        assert!(spun > 35.0 && fx.anim.heat > 0.3);
        w.players[me].cooldown = 0.0;
        fx.update(&w, eye, 0.1);
        assert!(fx.anim.spin_rate > 0.0 && fx.anim.spin_rate < spun, "coasting, not snapping still");
        for _ in 0..200 { fx.update(&w, eye, 0.016); }
        assert_eq!(fx.anim.spin_rate, 0.0);
        assert!(fx.anim.heat < 0.01);
    }
}
