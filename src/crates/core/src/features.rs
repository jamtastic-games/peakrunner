//! Map features beyond terrain and equipment, each an optional list in a
//! map's manifest (`map_pack::Manifest`; docs/map-pipeline.md):
//!
//! - **moveables**: boxes that move along a path: doors that open for a team
//!   (or anyone) nearby or while a circuit is powered, and lifts and
//!   platforms that go back and forth by themselves. Where one is follows
//!   from the physics step and its openness alone, so every client predicts
//!   it exactly; bodies stand on them and ride along, shots stop on them.
//! - **zones**: boxes that hurt (`hazard`), protect (`safe`), take a
//!   player elsewhere (`portal`), tell hooks who comes and goes
//!   (`trigger`), scale the fall (`gravity`), lift whoever is inside
//!   (`updraft`: a pocket of hot air) or put a blade in every hand there
//!   (`blade`: a side game, `sim::blade`). A zone may be round: a `radius`
//!   makes it an upright cylinder.
//! - **items**: pickups (`registry::Item`) that come back a while after
//!   they're taken.
//! - **deployables**: units (`registry::Deployable`) standing from the start
//!   of each round.
//! - **vents**: vents that erupt by themselves, throwing lava bombs (a
//!   projectile) after a warning. When, and where every bomb lands, follow
//!   from the physics step alone, so every client shows the warning and the
//!   landing spots in advance; the server throws the bombs.
//!
//! Openness and item clocks travel in snapshots; zones and items are the
//! server's.

use super::*;

/// A moving box on a map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Moveable {
    /// Half its width, height and depth (m), turned `yaw` degrees.
    pub size: [f32; 3],
    #[serde(default)]
    pub yaw: f32,
    /// Where its center goes, from where it starts (shut) to where it ends
    /// (open): two or more points.
    pub path: Vec<[f32; 3]>,
    /// Its speed along the path (m/s), and seconds it waits at each end
    /// (`loop`).
    pub speed: f32,
    #[serde(default)]
    pub wait: f32,
    /// What moves it.
    #[serde(default)]
    pub drive: Drive,
    /// `team`: whose players open it (0 or 1); `team` and `anyone`: how
    /// near they must be (m) to where it stands shut.
    #[serde(default)]
    pub team: u8,
    #[serde(default = "door_range")]
    pub range: f32,
    /// `power`: the circuit whose generator keeps it open.
    #[serde(default)]
    pub circuit: String,
    #[serde(default)]
    pub look: MoveableLook,
}
fn door_range() -> f32 { 6.0 }

/// What moves a moveable: back and forth by itself (`loop`), open while a
/// player of its team is near (`team`), any player near (`anyone`), or its
/// circuit powered (`power`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Drive { #[default] Loop, Team, Anyone, Power }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveableLook { #[default] Platform, Door, Lift }

/// A zone on a map: an axis-aligned box, or with a `radius` an upright
/// cylinder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub kind: ZoneKind,
    /// Its center and half extents (m).
    pub at: [f32; 3],
    pub size: [f32; 3],
    /// Round: the zone is a cylinder this wide (m) about its center, as
    /// tall as its box (`size[1]` each way); its box's width is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    /// Only for this team's players (0 or 1); absent: everyone.
    #[serde(default)]
    pub team: Option<u8>,
    /// `hazard`: damage a second, its kind, an effect held on, and a line
    /// for whoever's inside.
    #[serde(default)]
    pub damage: f32,
    #[serde(default = "hazard_kind")]
    pub damage_kind: String,
    #[serde(default)]
    pub effect: Option<String>,
    #[serde(default)]
    pub message: String,
    /// `portal`: where it takes them, facing this way (degrees).
    #[serde(default)]
    pub to: Option<[f32; 3]>,
    #[serde(default)]
    pub facing: Option<f32>,
    /// `trigger`: its name, and the hook told when players come in and go
    /// out (`hooks`; a mode's hooks hear it too).
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub hook: Option<String>,
    /// `gravity`: the share of gravity players fall with inside (0.1: a
    /// tenth, so a jet climbs with a tenth of the effort; 2: twice).
    #[serde(default = "full_gravity")]
    pub gravity: f32,
    /// `updraft`: how hard it lifts (m/s², upward, on top of gravity) in its
    /// lower part, fading to nothing over the top `UPDRAFT_FADE` of its
    /// height, so a rider rises to near the top and hangs there.
    #[serde(default)]
    pub lift: f32,
    /// `blade`: where whoever is cut down inside comes back, `[x, y, z,
    /// yaw degrees]` (1.2 m over the floor; up to 16). None: the map's spawns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawns: Vec<[f32; 4]>,
}
fn full_gravity() -> f32 { 1.0 }
fn hazard_kind() -> String { "fire".into() }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind { Hazard, Safe, Portal, Trigger, Gravity, Updraft, Blade }

/// The share of an updraft's height, at its top, over which the lift fades.
pub const UPDRAFT_FADE: f32 = 0.4;
/// The strongest updraft (m/s²): four times gravity.
pub const UPDRAFT_MAX: f32 = 80.0;
/// The drag on vertical speed inside an updraft (per second): the column's
/// air slows a climb to a steady rise and settles a rider where the lift
/// meets gravity instead of bobbing out of the top.
pub const UPDRAFT_DRAG: f32 = 1.5;

/// An item a map places, coming back a while after it's taken.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapItem { pub item: String, pub at: [f32; 3] }

/// A unit a map places, standing from the start of each round.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDeployable { pub deployable: String, pub team: u8, pub at: [f32; 3], #[serde(default)] pub yaw: f32 }

/// A vent that erupts by itself: on average every `every` seconds (each gap
/// between `every[0]` and `every[1]`), after `warning` seconds of rumble and
/// glow, it throws `bombs` (a span) of `projectile` from `at` to landing
/// spots `reach` metres away (a span), in a `cone` degrees wide round
/// `toward` (degrees: 0 toward -Z, 90 toward +X; absent: all round), each
/// arriving after about `flight` seconds. `offset` shifts its whole
/// schedule (seconds).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vent {
    pub at: [f32; 3],
    pub every: [f32; 2],
    #[serde(default = "vent_warning")]
    pub warning: f32,
    pub bombs: [u8; 2],
    pub reach: [f32; 2],
    #[serde(default)]
    pub toward: Option<f32>,
    #[serde(default = "full_circle")]
    pub cone: f32,
    #[serde(default = "vent_flight")]
    pub flight: f32,
    #[serde(default = "lava_bomb")]
    pub projectile: String,
    #[serde(default)]
    pub offset: f32,
}
fn vent_warning() -> f32 { 3.0 }
fn full_circle() -> f32 { 360.0 }
fn vent_flight() -> f32 { 3.5 }
fn lava_bomb() -> String { "vent-bomb".into() }

/// The owner a vent's bombs carry: the map itself, hostile to everyone
/// (`World::hostile`), credited to nobody.
pub const MAP_OWNER: usize = crate::sim::MAX_PLAYERS + 1;

/// A vent's state at a moment: how far into the warning before its next
/// eruption (0 idle, rising to 1 as it erupts), the seconds since its last
/// eruption (large before the first), and those eruptions' numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VentPhase { pub warning: f32, pub since: f32, pub next: u64, pub last: Option<u64> }

/// A number in 0..1 from a vent, an eruption and a salt (splitmix64).
fn hash01(v: usize, k: u64, j: u64) -> f32 {
    let mut x = (v as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k.wrapping_mul(0xBF58_476D_1CE4_E5B9) ^ j.wrapping_mul(0x94D0_49BB_1331_11EB);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

impl Vent {
    /// Eruptions sit one to a slot of the mean gap, each jittered within it,
    /// so the gaps stay inside `every` and any moment's neighbours are found
    /// at once.
    fn slot(&self) -> f64 { (((self.every[0] + self.every[1]) * 0.5).max(1.0)) as f64 }
    fn jitter(&self) -> f64 { (((self.every[1] - self.every[0]) * 0.5).max(0.0)) as f64 }
    /// When eruption `k` of vent number `v` happens (seconds of physics steps).
    pub fn eruption_time(&self, v: usize, k: u64) -> f64 {
        self.offset as f64 + (k + 1) as f64 * self.slot() + self.jitter() * hash01(v, k, 0) as f64
    }
    /// The first eruption after time `t`: its number and time.
    pub fn next_after(&self, v: usize, t: f64) -> (u64, f64) {
        let start = ((t - self.offset as f64 - self.jitter()) / self.slot()).floor() - 1.0;
        let mut k = start.max(0.0) as u64;
        loop {
            let at = self.eruption_time(v, k);
            if at > t { return (k, at); }
            k += 1;
        }
    }
    /// What vent number `v` is doing at time `t`.
    pub fn phase(&self, v: usize, t: f64) -> VentPhase {
        let (next, at) = self.next_after(v, t);
        let left = (at - t) as f32;
        let warning = if self.warning > 0.0 && left < self.warning { 1.0 - left / self.warning } else { 0.0 };
        let last = next.checked_sub(1);
        let since = last.map_or(f32::MAX, |k| (t - self.eruption_time(v, k)) as f32);
        VentPhase { warning, since, next, last }
    }
    /// Eruption `k`'s bombs: where each lands (on whatever is there: ground,
    /// a roof, lava) and its flight in seconds.
    pub fn bombs(&self, map: MapId, v: usize, k: u64) -> Vec<(Vec3, f32)> {
        let [lo, hi] = self.bombs;
        let n = lo as u32 + ((hi.saturating_sub(lo) as f32 + 1.0) * hash01(v, k, 1)).floor().min(hi.saturating_sub(lo) as f32) as u32;
        let at = Vec3::from(self.at);
        (0..n as u64).map(|j| {
            let a = match self.toward {
                Some(t) => t + self.cone * 0.5 * (2.0 * hash01(v, k, 10 + 3 * j) - 1.0),
                None => 360.0 * hash01(v, k, 10 + 3 * j),
            }.to_radians();
            let d = self.reach[0] + (self.reach[1] - self.reach[0]) * hash01(v, k, 11 + 3 * j);
            let (x, z) = (at.x + a.sin() * d, at.z - a.cos() * d);
            let flight = self.flight * (0.85 + 0.3 * hash01(v, k, 12 + 3 * j));
            // The spot is where it meets whatever is there: ground, a roof, lava.
            let y = crate::terrain::support_on(map, Vec3::new(x, at.y.max(crate::terrain::height_on(map, x, z)) + 40.0, z)).0;
            (Vec3::new(x, y, z), flight)
        }).collect()
    }
}

/// What's wrong with a manifest's features, against the definitions in
/// force (the base game's and the active mods'), for `--check`.
pub fn check(m: &crate::map_pack::Manifest) -> Result<(), String> {
    let r = crate::registry::registry();
    let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
    for (k, v) in m.moveables.iter().enumerate() {
        let at = format!("moveables[{k}]");
        if v.path.len() < 2 || v.path.len() > 16 || !v.path.iter().all(|p| finite(p)) { return Err(format!("{at}.path: 2 to 16 points")); }
        if !v.size.iter().all(|x| x.is_finite() && (0.05..=50.0).contains(x)) { return Err(format!("{at}.size: half extents 0.05 to 50 m")); }
        if !(v.speed.is_finite() && v.speed > 0.0 && v.speed <= 100.0) { return Err(format!("{at}.speed: above 0, up to 100 m/s")); }
        if !(v.wait.is_finite() && (0.0..=600.0).contains(&v.wait)) || !(v.range.is_finite() && (0.0..=200.0).contains(&v.range)) || v.team > 1 {
            return Err(format!("{at}: wait 0 to 600 s, range 0 to 200 m, team 0 or 1"));
        }
        if v.drive == Drive::Power && v.circuit.is_empty() { return Err(format!("{at}: \"power\" needs a circuit")); }
    }
    if m.moveables.len() > 64 { return Err("moveables: at most 64".into()); }
    for (k, z) in m.zones.iter().enumerate() {
        let at = format!("zones[{k}]");
        if !finite(&z.at) || !z.size.iter().all(|x| x.is_finite() && *x > 0.0) { return Err(format!("{at}: a center and half extents above 0")); }
        if z.radius.is_some_and(|r| !(r.is_finite() && r > 0.0 && r <= 2000.0)) { return Err(format!("{at}.radius: above 0, up to 2000 m")); }
        if z.spawns.len() > 16 || !z.spawns.iter().all(|s| finite(s)) { return Err(format!("{at}.spawns: up to 16 [x, y, z, yaw] points")); }
        if !z.spawns.is_empty() && z.kind != ZoneKind::Blade { return Err(format!("{at}.spawns: only a blade zone has spawns")); }
        if z.kind == ZoneKind::Portal && z.to.is_none_or(|t| !finite(&t)) { return Err(format!("{at}: a portal needs \"to\"")); }
        if r.damage.index(&z.damage_kind).is_none() { return Err(format!("{at}.damage_kind: no damage kind \"{}\"", z.damage_kind)); }
        if let Some(e) = z.effect.as_deref().filter(|e| r.effects.index(e).is_none()) { return Err(format!("{at}.effect: no effect \"{e}\"")); }
        if z.kind == ZoneKind::Trigger && (z.name.is_empty() || z.name.chars().count() > 32) { return Err(format!("{at}: a trigger needs a name (up to 32)")); }
        if z.hook.as_deref().is_some_and(|h| !crate::modes::valid_key(h)) { return Err(format!("{at}.hook: a-z, 0-9, _ and -")); }
        if z.kind == ZoneKind::Gravity && !(z.gravity.is_finite() && (0.0..=4.0).contains(&z.gravity)) {
            return Err(format!("{at}.gravity: a share of gravity, 0 to 4"));
        }
        if z.kind == ZoneKind::Updraft && !(z.lift.is_finite() && z.lift > 0.0 && z.lift <= UPDRAFT_MAX) {
            return Err(format!("{at}.lift: above 0, up to {UPDRAFT_MAX} m/s²"));
        }
    }
    for (k, i) in m.items.iter().enumerate() {
        if r.items.index(&i.item).is_none() { return Err(format!("items[{k}]: no item \"{}\"", i.item)); }
    }
    for (k, d) in m.deployables.iter().enumerate() {
        if r.deployables.index(&d.deployable).is_none() || d.team > 1 { return Err(format!("deployables[{k}]: no deployable \"{}\" (or a team other than 0 or 1)", d.deployable)); }
    }
    if m.duel_spots.len() > 64 { return Err("duel_spots: at most 64 pairs".into()); }
    if let Some(k) = m.duel_spots.iter().position(|pair| !pair.iter().all(|s| finite(s) && s[1].abs() < 5000.0)) {
        return Err(format!("duel_spots[{k}]: two [x, y, z, yaw] points"));
    }
    if m.vents.len() > 16 { return Err("vents: at most 16".into()); }
    for (k, v) in m.vents.iter().enumerate() {
        let at = format!("vents[{k}]");
        if !finite(&v.at) { return Err(format!("{at}.at: a point")); }
        if !(v.every[0].is_finite() && v.every[1].is_finite() && v.every[0] >= 2.0 && v.every[1] >= v.every[0] && v.every[1] <= 3600.0) {
            return Err(format!("{at}.every: 2 to 3600 s, the least first"));
        }
        if !(v.warning.is_finite() && (0.0..=30.0).contains(&v.warning) && v.warning < v.every[0]) { return Err(format!("{at}.warning: 0 to 30 s, under the shortest gap")); }
        if v.bombs[0] > v.bombs[1] || v.bombs[1] > 32 { return Err(format!("{at}.bombs: up to 32, the least first")); }
        if !(v.reach[0].is_finite() && v.reach[1].is_finite() && v.reach[0] >= 0.0 && v.reach[1] >= v.reach[0] && v.reach[1] <= 2000.0) {
            return Err(format!("{at}.reach: 0 to 2000 m, the least first"));
        }
        if v.toward.is_some_and(|t| !t.is_finite()) || !(v.cone.is_finite() && (0.0..=360.0).contains(&v.cone)) { return Err(format!("{at}: toward a heading, cone 0 to 360")); }
        if !(v.flight.is_finite() && (0.5..=20.0).contains(&v.flight)) || !v.offset.is_finite() { return Err(format!("{at}: flight 0.5 to 20 s")); }
        match r.projectiles.index(&v.projectile).and_then(|i| r.projectiles.get(i)) {
            Some(p) if p.life >= v.flight * 1.2 => {}
            Some(_) => return Err(format!("{at}.projectile: its life must outlast the flight")),
            None => return Err(format!("{at}.projectile: no projectile \"{}\"", v.projectile)),
        }
    }
    Ok(())
}

/// The manifest's features for `map` (empty for maps without a pack).
pub fn moveables(map: MapId) -> &'static [Moveable] { crate::map_pack::on(map).map_or(&[], |p| p.manifest.moveables.as_slice()) }
pub fn zones(map: MapId) -> &'static [Zone] { crate::map_pack::on(map).map_or(&[], |p| p.manifest.zones.as_slice()) }
pub fn map_items(map: MapId) -> &'static [MapItem] { crate::map_pack::on(map).map_or(&[], |p| p.manifest.items.as_slice()) }
pub fn vents(map: MapId) -> &'static [Vent] { crate::map_pack::on(map).map_or(&[], |p| p.manifest.vents.as_slice()) }

/// Seconds of physics steps: the clock vents keep.
pub fn vent_clock(steps: u64) -> f64 { steps as f64 * STEP as f64 }

/// The share of gravity a player of `team` at `pos` falls with: the
/// `gravity` zone they stand in (`ZoneKind::Gravity`), else 1. Movement
/// applies it on both sides of the wire, so clients predict it exactly.
pub fn gravity_at(map: MapId, team: Team, pos: Vec3) -> f32 {
    zones(map).iter().find(|z| z.kind == ZoneKind::Gravity && z.for_team(team) && z.contains(pos)).map_or(1.0, |z| z.gravity)
}

/// The upward push (m/s²) on a player of `team` at `pos`: the `updraft`
/// zone they're in (`ZoneKind::Updraft`), full in its lower part and fading
/// to nothing at its top, else 0. Movement applies it on both sides of the
/// wire, so clients predict it exactly.
pub fn lift_at(map: MapId, team: Team, pos: Vec3) -> f32 {
    updraft_at(map, team, pos).map_or(0.0, |z| z.lift_at(pos.y))
}

/// The updraft zone a player of `team` at `pos` is in, if any.
pub fn updraft_at(map: MapId, team: Team, pos: Vec3) -> Option<&'static Zone> {
    zones(map).iter().find(|z| z.kind == ZoneKind::Updraft && z.for_team(team) && z.contains(pos))
}

/// The blade zone (`ZoneKind::Blade`) a body at `pos` stands in, by its
/// index among the map's zones. For everyone alike, whatever their team:
/// movement and the blade read it on both sides of the wire.
pub fn blade_zone(map: MapId, pos: Vec3) -> Option<usize> {
    zones(map).iter().position(|z| z.kind == ZoneKind::Blade && z.contains(pos))
}

/// Whether `map` has any blade zone.
pub fn has_blade_zones(map: MapId) -> bool { zones(map).iter().any(|z| z.kind == ZoneKind::Blade) }

impl Moveable {
    fn points(&self) -> impl Iterator<Item = Vec3> + '_ { self.path.iter().map(|p| Vec3::from(*p)) }
    /// The path's length.
    pub fn length(&self) -> f32 { self.points().zip(self.points().skip(1)).map(|(a, b)| a.distance(b)).sum() }
    /// Where its center is `d` metres along the path.
    pub fn along(&self, d: f32) -> Vec3 {
        let mut left = d.max(0.0);
        let mut last = self.path.first().map_or(Vec3::ZERO, |p| Vec3::from(*p));
        for (a, b) in self.points().zip(self.points().skip(1)) {
            let len = a.distance(b);
            if left <= len { return if len > 0.0 { a.lerp(b, left / len) } else { a }; }
            left -= len;
            last = b;
        }
        last
    }
    /// Where its center is at physics step `steps` with openness `open`
    /// (0 shut, 1 open; a `loop` ignores it).
    pub fn at(&self, steps: u64, open: f32) -> Vec3 {
        let len = self.length();
        if self.drive != Drive::Loop || self.speed <= 0.0 || len <= 0.0 { return self.along(open.clamp(0.0, 1.0) * len); }
        let travel = len / self.speed;
        let cycle = 2.0 * (travel + self.wait);
        // Whole steps keep the phase exact however long a match runs.
        let t = ((steps % ((cycle / STEP).round().max(1.0) as u64)) as f32) * STEP;
        let d = if t < self.wait { 0.0 }
            else if t < self.wait + travel { (t - self.wait) * self.speed }
            else if t < 2.0 * self.wait + travel { len }
            else { len - (t - 2.0 * self.wait - travel) * self.speed };
        self.along(d)
    }
    fn half(&self) -> Vec3 { Vec3::from(self.size) }
    fn turn(&self) -> f32 { self.yaw.to_radians() }
    /// `p` in its own frame, its center at `center`.
    fn local(&self, center: Vec3, p: Vec3) -> Vec3 { Mat4::from_rotation_y(-self.turn()).transform_point3(p - center) }
}

impl Zone {
    pub fn contains(&self, p: Vec3) -> bool {
        let d = (p - Vec3::from(self.at)).abs();
        let h = Vec3::from(self.size);
        match self.radius {
            Some(r) => d.y <= h.y && d.x * d.x + d.z * d.z <= r * r,
            None => d.x <= h.x && d.y <= h.y && d.z <= h.z,
        }
    }
    fn for_team(&self, team: Team) -> bool { self.team.is_none_or(|t| t as usize == team.idx()) }
    /// An updraft's push at height `y` inside it: `lift` up to where the
    /// fade starts, then down to nothing at the top face.
    pub fn lift_at(&self, y: f32) -> f32 {
        let top = self.at[1] + self.size[1];
        let fade = (2.0 * self.size[1] * UPDRAFT_FADE).max(0.01);
        self.lift * ((top - y) / fade).clamp(0.0, 1.0)
    }
}

impl World {
    /// The map's moveables where they are this step (`moveable_at`), and
    /// where they were last step, opening and shutting the driven ones.
    /// Predicted: every machine runs it from the same state.
    pub(super) fn step_moveables(&mut self, dt: f32) {
        let list = moveables(self.map);
        if list.is_empty() { return; }
        self.moveable_open.resize(list.len(), 0.0);
        self.moveable_at.resize(list.len(), Vec3::ZERO);
        let defs = crate::equipment::definitions(self.map);
        for (k, m) in list.iter().enumerate() {
            let len = m.length().max(0.01);
            // Measured from where it stands shut, so it doesn't sense away
            // from whoever opened it.
            let frame = m.along(0.0);
            let near = |team: Option<Team>| self.players.iter().any(|p| p.alive && !p.hidden()
                && team.is_none_or(|t| p.team == t) && p.pos.distance(frame) <= m.range);
            let wanted = match m.drive {
                Drive::Loop => false,
                Drive::Team => near(Some(if m.team == 1 { Team::Glacier } else { Team::Ember })),
                Drive::Anyone => near(None),
                Drive::Power => defs.iter().zip(&self.equipment).any(|(d, s)| d.circuit == m.circuit && s.powered),
            };
            if m.drive != Drive::Loop {
                let step = m.speed * dt / len;
                let o = &mut self.moveable_open[k];
                *o = if wanted { (*o + step).min(1.0) } else { (*o - step).max(0.0) };
            }
        }
        self.moveable_was = std::mem::take(&mut self.moveable_at);
        self.moveable_was.resize(list.len(), Vec3::ZERO);
        self.moveable_at = list.iter().enumerate().map(|(k, m)| m.at(self.steps, self.moveable_open[k])).collect();
        // The first step of a match has nothing to compare with.
        for k in 0..list.len() { if self.moveable_was[k] == Vec3::ZERO { self.moveable_was[k] = self.moveable_at[k]; } }
    }

    /// Player `i` against the moveables: standing on one (riding along), or
    /// pushed out of its side. Predicted, after `collide_deployables`.
    pub(super) fn collide_moveables(&mut self, i: usize) {
        let list = moveables(self.map);
        if list.is_empty() { return; }
        let r = PLAYER_RADIUS;
        for (k, m) in list.iter().enumerate() {
            let (Some(&at), Some(&was)) = (self.moveable_at.get(k), self.moveable_was.get(k)) else { continue };
            let half = m.half();
            let p = &mut self.players[i];
            let local = m.local(at, p.pos);
            if local.x.abs() >= half.x + r || local.z.abs() >= half.z + r || local.y.abs() >= half.y + r { continue; }
            let rise = (at.y - was.y).max(0.0);
            if local.y >= half.y - 0.05 - rise && local.x.abs() < half.x + r * 0.5 && local.z.abs() < half.z + r * 0.5 {
                // On top: ride along.
                let moved = at - was;
                p.pos.x += moved.x;
                p.pos.z += moved.z;
                p.pos.y = at.y + half.y + r;
                if p.vel.y < moved.y / STEP.max(1e-4) { p.vel.y = p.vel.y.max(0.0); }
                p.on_ground = true;
                p.footing = true;
                continue;
            }
            // Out through the nearer side.
            let (push, axis) = if half.z + r - local.z.abs() < half.x + r - local.x.abs() {
                ((half.z + r - local.z.abs()) * local.z.signum(), Vec3::Z)
            } else {
                ((half.x + r - local.x.abs()) * local.x.signum(), Vec3::X)
            };
            let out = Mat4::from_rotation_y(m.turn()).transform_vector3(axis);
            p.pos += out * push;
            let dir = out * push.signum();
            let vn = p.vel.dot(dir);
            if vn < 0.0 { p.vel -= dir * vn; }
        }
    }

    /// The first moveable a sphere of `radius` meets along `a` to `b`: the
    /// fraction and the face's outward normal.
    pub fn moveable_hit(&self, a: Vec3, b: Vec3, radius: f32) -> Option<(f32, Vec3)> {
        let list = moveables(self.map);
        if list.is_empty() { return None; }
        let mut best: Option<(f32, Vec3)> = None;
        for (k, m) in list.iter().enumerate() {
            let Some(&at) = self.moveable_at.get(k) else { continue };
            let half = m.half() + Vec3::splat(radius);
            let Some(t) = segment_box(m.local(at, a), m.local(at, b), -half, half) else { continue };
            if best.is_some_and(|(u, _)| u <= t) { continue; }
            let hit = m.local(at, a.lerp(b, t)) / half;
            let face = if hit.x.abs() >= hit.y.abs() && hit.x.abs() >= hit.z.abs() { Vec3::X * hit.x.signum() }
                else if hit.y.abs() >= hit.z.abs() { Vec3::Y * hit.y.signum() } else { Vec3::Z * hit.z.signum() };
            best = Some((t, Mat4::from_rotation_y(m.turn()).transform_vector3(face)));
        }
        best
    }

    /// Whether player `i` stands in a safe zone (no damage there).
    pub fn in_safe_zone(&self, i: usize) -> bool {
        let Some(p) = self.players.get(i) else { return false };
        zones(self.map).iter().any(|z| z.kind == ZoneKind::Safe && z.for_team(p.team) && z.contains(p.pos))
    }

    /// The server's part of the vents each step: an eruption due this step
    /// throws its bombs (`Vent::bombs`), each on the arc that lands it there.
    pub(super) fn step_vents(&mut self) {
        if self.predicting { return; }
        let list = vents(self.map);
        if list.is_empty() { return; }
        let (t1, t0) = (vent_clock(self.steps), vent_clock(self.steps.saturating_sub(1)));
        let reg = &crate::registry::registry().projectiles;
        for (v, vent) in list.iter().enumerate() {
            let (k, at) = vent.next_after(v, t0);
            if at > t1 { continue; }
            let Some(kind) = reg.index(&vent.projectile) else { continue };
            let Some(round) = reg.get(kind) else { continue };
            let g = crate::defs::movement().gravity * round.gravity;
            // A fat round (a fireball) starts above the mouth, clear of the
            // crown, and the arc is from there so it still lands on its spot.
            let lift = if round.burn > 0.0 { round.radius + 0.5 } else { 0.0 };
            let from = Vec3::from(vent.at) + Vec3::Y * lift;
            for (target, flight) in vent.bombs(self.map, v, k) {
                let vel = (target - from) / flight + Vec3::Y * (0.5 * g * flight);
                self.discs.push(Disc { pos: from, vel, team: Team::Ember, owner: MAP_OWNER, life: round.life, kind: kind as u8,
                    spin: 0.0, target: 0, power: 1.0 });
            }
        }
    }

    /// The server's part each step: hazards hurt, lava burns, portals take
    /// players away, and the map's items come back and are taken.
    pub(super) fn step_features(&mut self, dt: f32) {
        if self.predicting { return; }
        if crate::water::has_lava(self.map) {
            for i in 0..self.players.len() {
                let p = &self.players[i];
                if !p.alive || p.riding > 0 { continue; }
                if let Some(burn) = crate::water::lava_burn(self.map, p.pos) { self.damage(i, burn * dt, "fire", None, "Lava"); }
            }
        }
        self.burn_fireballs(dt);
        let zs = zones(self.map);
        if !zs.is_empty() {
            let effects = &crate::registry::registry().effects;
            for i in 0..self.players.len() {
                if !self.players[i].alive { continue; }
                for z in zs {
                    let p = &self.players[i];
                    if !z.for_team(p.team) || !z.contains(p.pos) { continue; }
                    match z.kind {
                        ZoneKind::Hazard => {
                            if let Some(e) = z.effect.as_deref().and_then(|e| effects.index(e)) { self.give_effect(i, e as u8); }
                            if z.damage > 0.0 { self.damage(i, z.damage * dt, &z.damage_kind, None, "Hazard"); }
                            if i == self.player_id && !z.message.is_empty() { self.msg(&z.message, 0.5); }
                        }
                        ZoneKind::Trigger => {}
                        ZoneKind::Portal if self.players[i].teleport_clock <= 0.0 => {
                            let Some(to) = z.to else { continue };
                            if self.players[i].carrying.is_some() { self.drop_carried_flag(i); }
                            let p = &mut self.players[i];
                            p.pos = Vec3::from(to);
                            p.vel = Vec3::ZERO;
                            if let Some(f) = z.facing { p.yaw = f.to_radians(); }
                            p.teleport_clock = 1.0;
                            if i == self.player_id { self.push_event("deploy"); }
                        }
                        _ => {}
                    }
                }
            }
        }
        // Triggers: who came in, who went out (`hooks`).
        if self.hooks.live && zs.iter().any(|z| z.kind == ZoneKind::Trigger) {
            for i in 0..self.players.len() {
                for (k, z) in zs.iter().enumerate().take(64) {
                    if z.kind != ZoneKind::Trigger { continue; }
                    let p = &self.players[i];
                    let inside = p.alive && z.for_team(p.team) && z.contains(p.pos);
                    let was = p.zones_in & (1 << k) != 0;
                    if inside == was { continue; }
                    self.players[i].zones_in ^= 1 << k;
                    let event = if inside { super::hooks::HookEvent::Enter { player: i, zone: z.name.clone() } }
                        else { super::hooks::HookEvent::Leave { player: i, zone: z.name.clone() } };
                    self.hook(z.hook.as_deref(), event);
                }
            }
        }
        let items = map_items(self.map);
        if items.is_empty() { return; }
        self.map_item_clocks.resize(items.len(), 0.0);
        let reg = &crate::registry::registry().items;
        for (k, it) in items.iter().enumerate() {
            if self.map_item_clocks[k] > 0.0 { self.map_item_clocks[k] = (self.map_item_clocks[k] - dt).max(0.0); continue; }
            let Some(id) = reg.index(&it.item) else { continue };
            let at = Vec3::from(it.at);
            for i in 0..self.players.len() {
                if !self.players[i].alive || self.players[i].pos.distance(at + Vec3::Y * 0.5) > loadout::LOOT_REACH { continue; }
                if self.take_item(i, id as u8) { self.map_item_clocks[k] = reg.get(id).map_or(30.0, |t| t.respawn); break; }
            }
        }
    }

    /// A round with `burn` keeps flying through people and hurts whoever it
    /// overlaps, each second. The ground still bursts it (`step_discs`
    /// skips the body when `burn` is set). Server only: the caller returns
    /// first while predicting.
    fn burn_fireballs(&mut self, dt: f32) {
        let rounds = &crate::registry::registry().projectiles;
        let mut hits: Vec<(usize, f32)> = Vec::new();
        for d in &self.discs {
            let Some(round) = rounds.get(d.kind as usize).filter(|r| r.burn > 0.0) else { continue };
            let reach = round.radius + PLAYER_RADIUS;
            for i in 0..self.players.len() {
                let p = &self.players[i];
                // Inside a vehicle it can't reach you; on an exposed seat it can.
                if !p.alive || (p.riding > 0 && !self.rides_exposed(i)) { continue; }
                if !self.hostile(d.owner, d.team, i) && !self.team_fire(d.owner, d.team, i) { continue; }
                if d.pos.distance(p.pos + Vec3::Y * 0.9) <= reach { hits.push((i, round.burn * dt)); }
            }
        }
        for (i, amount) in hits { self.damage(i, amount, "fire", None, "Eruption"); }
    }

    /// The map's deployables, standing again (a new match or round).
    pub fn place_map_deployables(&mut self) {
        let Some(pack) = crate::map_pack::on(self.map) else { return };
        for d in &pack.manifest.deployables {
            let Some(kind) = deploy::DeployKind::from_key(&d.deployable) else { continue };
            let team = if d.team == 1 { Team::Glacier } else { Team::Ember };
            self.deployables.push(deploy::Deployable { owner: 0, kind, team, pos: Vec3::from(d.at), yaw: d.yaw.to_radians(),
                health: kind.def().hull, cooldown: 0.0, aim: Vec3::NEG_Z, age: 0.0 });
        }
        self.map_item_clocks = vec![0.0; map_items(self.map).len()];
    }
}

#[cfg(test)]
mod vent_tests {
    use super::*;

    fn vent() -> Vent {
        Vent { at: [1024.0, 180.0, 1024.0], every: [24.0, 40.0], warning: 3.0, bombs: [4, 9], reach: [40.0, 220.0],
            toward: None, cone: 360.0, flight: 3.5, projectile: "vent-bomb".into(), offset: 5.0 }
    }

    #[test]
    fn eruptions_keep_their_gaps_and_every_moment_finds_its_neighbours() {
        let v = vent();
        let times: Vec<f64> = (0..400).map(|k| v.eruption_time(2, k)).collect();
        for w in times.windows(2) {
            let gap = w[1] - w[0];
            assert!(gap >= 24.0 - 1e-6 && gap <= 40.0 + 1e-6, "gap {gap}");
        }
        for k in 0..390 {
            for t in [times[k] - 0.01, times[k], times[k] + 0.01, (times[k] + times[k + 1]) * 0.5] {
                let (next, at) = v.next_after(2, t);
                assert!(at > t && (next == 0 || v.eruption_time(2, next - 1) <= t), "eruption {next} for {t}");
            }
        }
        // The phase: warning rises to 1 into the eruption, then counts since.
        let t = times[7];
        assert!((v.phase(2, t - 1.5).warning - 0.5).abs() < 1e-3);
        assert_eq!(v.phase(2, t - 10.0).warning, 0.0);
        let after = v.phase(2, t + 0.5);
        assert_eq!(after.last, Some(7));
        assert!((after.since - 0.5).abs() < 1e-3);
    }

    #[test]
    fn bombs_are_the_same_everywhere_and_land_within_reach() {
        let v = Vent { toward: Some(90.0), cone: 60.0, ..vent() };
        let map = MapId::OldHoller;
        for k in 0..40 {
            let a = v.bombs(map, 1, k);
            assert_eq!(a, v.bombs(map, 1, k), "deterministic");
            assert!((4..=9).contains(&a.len()));
            for (p, flight) in a {
                let d = Vec2::new(p.x - 1024.0, p.z - 1024.0);
                assert!(d.length() >= 40.0 - 1e-3 && d.length() <= 220.0 + 1e-3);
                // Toward +X, within 30 degrees either side.
                assert!(d.x > 0.0 && d.y.abs() <= d.x * (30f32.to_radians().tan()) + 1e-3, "{d:?}");
                assert!((3.5 * 0.85..=3.5 * 1.15).contains(&flight));
            }
        }
    }

    /// The pool on the volcanic copy of Old Holler (x0, z0, x1, z1), and its vent.
    const POOL: [f32; 4] = [960.0, 760.0, 1010.0, 810.0];
    const VENT: [f32; 2] = [1300.0, 600.0];

    /// Old Holler, copied with a lava pool and a vent (no stock map has
    /// vents): the pool 0.5 m over the highest ground in `POOL`, burning 150
    /// a second; the vent at `VENT` throwing fireballs all round. Loaded once.
    fn volcanic_holler() -> MapId {
        static MAP: std::sync::OnceLock<MapId> = std::sync::OnceLock::new();
        *MAP.get_or_init(|| {
            let source = crate::catalog::entry(MapId::OldHoller).pack().expect("Old Holler installed").root.clone();
            let dir = std::env::temp_dir().join(format!("peakrunner-volcanic-holler-{}", std::process::id())).join("maps/volcanic-holler");
            std::fs::create_dir_all(&dir).unwrap();
            for f in std::fs::read_dir(&source).unwrap().filter_map(Result::ok) {
                let name = f.file_name();
                if name == "map.json" || name == "info.json" { continue; }
                let to = dir.join(&name);
                if !to.exists() && std::fs::hard_link(f.path(), &to).is_err() { std::fs::copy(f.path(), &to).unwrap(); }
            }
            std::fs::write(dir.join("info.json"), r#"{"name":"Volcanic Holler"}"#).unwrap();
            let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(source.join("map.json")).unwrap()).unwrap();
            let [x0, z0, x1, z1] = POOL;
            let mut top = f32::MIN;
            for i in 0..=20 { for j in 0..=20 {
                let (x, z) = (x0 + (x1 - x0) * i as f32 / 20.0, z0 + (z1 - z0) * j as f32 / 20.0);
                top = top.max(crate::terrain::height_on(MapId::OldHoller, x, z));
            } }
            let pool = serde_json::json!({"surface": top + 0.5, "rect": POOL, "lava": true, "burn": 150.0, "color": [1.0, 0.42, 0.1]});
            m["water_volumes"].as_array_mut().expect("Old Holler's river").push(pool);
            let ground = crate::terrain::height_on(MapId::OldHoller, VENT[0], VENT[1]);
            m["vents"] = serde_json::json!([{"at": [VENT[0], ground + 1.0, VENT[1]], "every": [20, 30], "warning": 3.0, "bombs": [4, 6],
                "reach": [30, 120], "flight": 3.5, "projectile": "vent-fireball"}]);
            std::fs::write(dir.join("map.json"), serde_json::to_vec_pretty(&m).unwrap()).unwrap();
            crate::catalog::add_folder("volcanic-holler", dir).expect("the copy loads")
        })
    }

    fn pool_surface(map: MapId) -> f32 {
        crate::water::volumes(map).iter().find(|v| v.lava).expect("the pool").surface
    }

    /// A vent, through the real step: each eruption throws its bombs as the
    /// map's, on arcs that come down on the spots every client was shown.
    #[test]
    fn vents_throw_their_bombs_on_schedule() {
        let map = volcanic_holler();
        let mut w = World::new();
        w.set_map(map);
        w.start_match(true);
        w.players.truncate(1);
        let list = vents(map);
        assert_eq!(list.len(), 1);
        let (k, at) = list[0].next_after(0, vent_clock(w.steps));
        let spots = list[0].bombs(map, 0, k);
        while vent_clock(w.steps) < at - 0.05 { w.tick(STEP); }
        assert!(!w.discs.iter().any(|d| d.owner == MAP_OWNER), "nothing before the eruption");
        while vent_clock(w.steps) < at + 0.1 { w.tick(STEP); }
        let bombs: Vec<_> = w.discs.iter().filter(|d| d.owner == MAP_OWNER).cloned().collect();
        assert_eq!(bombs.len(), spots.len(), "every bomb thrown");
        let fireball = crate::registry::registry().projectiles.index("vent-fireball").unwrap() as u8;
        assert!(bombs.iter().all(|d| d.kind == fireball), "the vent throws its projectile");
        // Each lands within a few metres of its spot: follow the arc.
        let g = crate::defs::movement().gravity;
        for ((target, flight), b) in spots.iter().zip(&bombs) {
            let t = flight - (vent_clock(w.steps) - at) as f32;
            let p = b.pos + b.vel * t - Vec3::Y * (0.5 * g * t * t);
            assert!(p.distance(*target) < 3.0, "lands at {p:?}, shown {target:?}");
        }
    }

    /// A lava pool: whoever stands on it burns (and skims on it); its shore
    /// does not.
    #[test]
    fn lava_burns_whoever_stands_on_it() {
        let map = volcanic_holler();
        let surface = pool_surface(map);
        let burned = |x: f32, z: f32| {
            let mut w = World::new();
            w.set_map(map);
            w.start_match(true);
            w.players.truncate(1);
            w.player_id = 0;
            w.network_inputs.clear();
            let ground = crate::terrain::support_on(map, Vec3::new(x, 2000.0, z)).0;
            let p = &mut w.players[0];
            p.is_bot = false; p.remote = false; p.alive = true; p.health = 100.0;
            p.pos = Vec3::new(x, ground + PLAYER_RADIUS, z);
            p.vel = Vec3::ZERO;
            for _ in 0..(1.0 / STEP).round() as usize { w.tick(STEP); }
            (100.0 - w.players[0].health, w.players[0].pos.y)
        };
        let (on, y) = burned((POOL[0] + POOL[2]) / 2.0, (POOL[1] + POOL[3]) / 2.0);
        assert!(y > surface, "standing on it, not sunk");
        assert!(on > 30.0, "a second on the lava burns ({on:.1})");
        let (shore, _) = burned(POOL[2] + 30.0, (POOL[1] + POOL[3]) / 2.0);
        assert!(shore < 1.0, "the shore does not ({shore:.1})");
    }

    /// A disc fired down at a lava pool bursts on the lava, not on the bed
    /// under it, and throws a splash that burns whoever it reaches,
    /// credited to the shooter. A vent's own bomb throws none, and neither
    /// does a burst on dry ground.
    #[test]
    fn shots_burst_on_lava_and_splash_it() {
        let map = volcanic_holler();
        let surface = pool_surface(map);
        let (cx, cz) = ((POOL[0] + POOL[2]) / 2.0, (POOL[1] + POOL[3]) / 2.0);
        let reg = crate::registry::registry();
        let splash = reg.projectiles.index(crate::sim::LAVA_SPLASH).expect("the splash") as u8;
        let world = || {
            let mut w = World::new();
            w.set_map(map);
            w.start_match(true);
            w.players.clear();
            w.players.push(make_player(Team::Ember, false, Vec3::new(cx, surface + 300.0, cz), 0.0, BotRole::Offense));
            // An enemy just over the lava, clear of its burn.
            w.players.push(make_player(Team::Glacier, false, Vec3::new(cx + 3.0, surface + 2.5, cz), 0.0, BotRole::Offense));
            w.player_id = 0;
            w.network_inputs.clear();
            for p in &mut w.players { p.alive = true; p.health = 100.0; p.vel = Vec3::ZERO; p.is_bot = false; }
            w
        };
        assert!(crate::water::lava_burn(map, Vec3::new(cx + 3.0, surface + 2.5, cz)).is_none(), "out of the burn");

        let mut w = world();
        let disc = reg.projectiles.index("disc").unwrap() as u8;
        w.discs.push(Disc { pos: Vec3::new(cx, surface + 6.0, cz), vel: Vec3::new(0.0, -80.0, 0.0), team: Team::Ember,
            owner: 0, life: 5.0, kind: disc, spin: 0.0, target: 0, power: 1.0 });
        for _ in 0..30 { w.tick(STEP); if !w.explosions.is_empty() { break; } }
        let burst = w.explosions.iter().find(|e| e.kind == disc).expect("the disc burst");
        assert!((burst.pos.y - surface).abs() < 0.5, "on the lava at {}, not under it ({surface})", burst.pos.y);
        let thrown = w.explosions.iter().find(|e| e.kind == splash).expect("a splash");
        assert!((thrown.pos.y - surface - 0.1).abs() < 0.01 && thrown.pos.distance(burst.pos) < 0.6);
        assert!(w.players[1].health < 100.0);

        // The splash alone burns, and a kill by it is the shooter's.
        let mut w = world();
        let at = Vec3::new(cx, surface + 0.1, cz);
        w.explode(at, 0, splash, Team::Ember);
        let hurt = 100.0 - w.players[1].health;
        assert!(hurt > 5.0, "the splash burns ({hurt:.1})");
        assert_eq!(w.explosions.len(), 1, "a splash throws no splash of its own");
        w.players[1].health = 1.0;
        w.explode(at, 0, splash, Team::Ember);
        assert!(!w.players[1].alive);
        let lines: Vec<_> = w.feed.iter().map(|e| e.line()).collect();
        assert!(lines.iter().any(|l| l.contains("Lava splash") && !l.contains("Environment")), "{lines:?}");

        // The map's bombs, and bursts on dry ground, throw none.
        let mut w = world();
        w.explode(at, MAP_OWNER, reg.projectiles.index("vent-bomb").unwrap() as u8, Team::Ember);
        assert!(w.explosions.iter().all(|e| e.kind != splash));
        let (dx, dz) = (POOL[2] + 60.0, cz);
        let dry = Vec3::new(dx, crate::terrain::support_on(map, Vec3::new(dx, 2000.0, dz)).0, dz);
        assert!(crate::water::lava_splash_surface(map, dry).is_none());
        let mut w = world();
        w.explode(dry, 0, disc, Team::Ember);
        assert!(w.explosions.iter().all(|e| e.kind != splash));
    }

    /// A fireball burns a body it overlaps and keeps flying. One that misses
    /// does nothing.
    #[test]
    fn a_fireball_burns_whoever_it_touches_and_keeps_flying() {
        let kind = crate::registry::registry().projectiles.index("vent-fireball").unwrap();
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_match(true);
        w.players.clear();
        w.players.push(make_player(Team::Ember, false, Vec3::new(1080.0, 500.0, 1000.0), 0.0, BotRole::Offense));
        w.players.push(make_player(Team::Glacier, false, Vec3::new(1100.0, 500.0, 1000.0), 0.0, BotRole::Offense));
        w.player_id = 0;
        w.network_inputs.clear();
        for p in &mut w.players { p.alive = true; p.health = 100.0; p.vel = Vec3::ZERO; p.is_bot = false; }
        w.discs.push(Disc { pos: Vec3::new(1080.0, 500.9, 1000.0), vel: Vec3::ZERO, team: Team::Ember, owner: MAP_OWNER,
            life: 10.0, kind: kind as u8, spin: 0.0, target: 0, power: 1.0 });
        w.tick(STEP);
        assert!(w.players[0].health < 97.0, "a touch burns: {}", w.players[0].health);
        assert!((w.players[1].health - 100.0).abs() < 0.1, "a miss does not: {}", w.players[1].health);
        assert!(w.discs.iter().any(|d| d.owner == MAP_OWNER && d.kind == kind as u8), "it keeps flying");
        w.players[0].health = 1.0;
        w.tick(STEP);
        assert!(!w.players[0].alive, "a second in the fireball kills");
        let lines: Vec<_> = w.feed.iter().map(|e| e.line()).collect();
        assert!(lines.iter().any(|l| l.contains("Environment") && l.contains("Eruption")), "{lines:?}");
    }

    #[test]
    fn the_maps_fire_hurts_both_teams() {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_match(true);
        let mate = w.players.iter().position(|p| p.team == Team::Ember).unwrap();
        let foe = w.players.iter().position(|p| p.team == Team::Glacier).unwrap();
        assert!(w.hostile(MAP_OWNER, Team::Ember, mate) && w.hostile(MAP_OWNER, Team::Ember, foe));
        assert!(!w.hostile(crate::sim::MAX_PLAYERS, Team::Ember, mate), "a turret still spares its own team");
    }
}

