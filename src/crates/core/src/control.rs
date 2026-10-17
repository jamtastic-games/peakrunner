//! Capture & Hold control points. Pure, server-authoritative rules shared by the
//! CTF-active centre points and the Capture & Hold mode. Maps declare points in
//! the manifest (`control_points`); maps without them are unchanged.
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Uncontested presence needed to flip a point to your team (a point may set
/// its own, `Definition::capture`).
pub const CAPTURE_SECONDS: f32 = 10.0;
/// With nobody inside, partial progress drains back at half the capture rate.
pub const DECAY_SECONDS: f32 = 20.0;
/// At most this many spires on a point, each within `SPIRE_REACH` of it.
pub const MAX_SPIRES: usize = 16;
pub const SPIRE_REACH: f32 = 150.0;
pub const DEFAULT_RADIUS: f32 = 12.0;
pub const DEFAULT_DRAIN_RADIUS: f32 = 60.0;
/// Energy per second drained from the holder's enemies inside a drain field.
/// Jetting costs 15/s and regen is 12/s, so with 10/s drained a full 60-energy
/// tank lasts (60 - 3) / 25 = 2.3 s of jetting (3.8 s normally) and refills at
/// only 2/s (12/s normally): one short hop every few seconds, never sustained flight.
pub const DEFAULT_DRAIN_RATE: f32 = 10.0;
/// Capture & Hold scoring: points per second per held point, first to the target wins.
pub const CNH_POINTS_PER_SECOND: f32 = 1.0;
pub const CNH_TARGET: u32 = 300;
/// Presence is a cylinder over the point: players stand 1.2 m above the floor.
const BELOW: f32 = 2.0;
const ABOVE: f32 = 10.0;

fn default_radius() -> f32 { DEFAULT_RADIUS }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drain {
    #[serde(default = "default_drain_radius")]
    pub radius: f32,
    #[serde(default = "default_drain_rate")]
    pub rate: f32,
    /// Only players off the ground (flying, jumping, falling) are drained.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub airborne: bool,
    /// Health per second it also takes (0 to 40), as energy damage.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub damage: f32,
}
fn is_zero(v: &f32) -> bool { *v == 0.0 }

/// A box round a point where its holders burn: health per second (1 to
/// 150, fire damage) to every member of the holding team inside it, nobody
/// else. A team can't keep the point from inside it, so its enemies are
/// welcome there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cook {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub damage: f32,
}
fn default_drain_radius() -> f32 { DEFAULT_DRAIN_RADIUS }
fn default_drain_rate() -> f32 { DEFAULT_DRAIN_RATE }

/// A turret standing on the point (on its tower's crown), fighting for
/// whichever team holds the point and idle while nobody does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointTurret {
    /// A deployable with turret behavior (`registry::Deployable::turret`).
    pub deployable: String,
    /// Its base, in world space.
    pub at: [f32; 3],
    /// Its reach on this map (m), in place of its deployable's ranges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<f32>,
}

/// Manifest entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub name: String,
    pub pos: [f32; 3],
    #[serde(default = "default_radius")]
    pub radius: f32,
    #[serde(default)]
    pub ctf_active: bool,
    #[serde(default)]
    pub drain: Option<Drain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret: Option<PointTurret>,
    /// Seconds of uncontested presence that flip it (0.5 to 30; default
    /// `CAPTURE_SECONDS`): a short one is a touch, like a post's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<f32>,
    /// Spire tips (world space): they glow in the holder's color, and the
    /// point's drain comes from them, so it reaches only players one of them
    /// can see, from the nearest such spire (`drain_from`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spires: Vec<[f32; 3]>,
    /// The beacon light's height over the point (0 to 40 m); by default it
    /// sits on whatever stands over the point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beacon: Option<f32>,
    /// Where its holders burn (`Cook`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cook: Option<Cook>,
}

pub fn validate(defs: &[Definition]) -> Result<(), String> {
    if defs.len() > 8 { return Err("Too many control points".into()); }
    let mut ids = std::collections::BTreeSet::new();
    for d in defs {
        if !crate::map_catalog::valid_id(&d.id) || !ids.insert(d.id.as_str()) {
            return Err("Invalid control point id".into());
        }
        if d.name.is_empty() || d.name.len() > 24 || d.name.trim() != d.name
            || !d.name.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-') {
            return Err("Invalid control point name".into());
        }
        if d.pos.iter().any(|v| !v.is_finite() || v.abs() > 10000.0)
            || !(2.0..=40.0).contains(&d.radius) {
            return Err("Invalid control point position/radius".into());
        }
        if let Some(drain) = &d.drain {
            if !(d.radius..=200.0).contains(&drain.radius) || !(0.0..=40.0).contains(&drain.rate) || !(0.0..=40.0).contains(&drain.damage) {
                return Err("Invalid control point drain".into());
            }
        }
        if let Some(t) = &d.turret {
            if crate::sim::deploy::DeployKind::from_key(&t.deployable).and_then(|k| k.turret()).is_none() {
                return Err(format!("control point {}: no turret \"{}\"", d.id, t.deployable));
            }
            let (at, pos) = (Vec3::from_array(t.at), Vec3::from_array(d.pos));
            if !at.is_finite() || at.distance(pos) > 60.0 {
                return Err(format!("control point {}: its turret stands within 60 m of the point", d.id));
            }
            if t.range.is_some_and(|r| !(10.0..=1000.0).contains(&r)) {
                return Err(format!("control point {}: its turret's range is 10 to 1000 m", d.id));
            }
        }
        if d.capture.is_some_and(|c| !(0.5..=30.0).contains(&c)) {
            return Err(format!("control point {}: capture takes 0.5 to 30 s", d.id));
        }
        let pos = Vec3::from_array(d.pos);
        if d.spires.len() > MAX_SPIRES
            || d.spires.iter().any(|s| !Vec3::from_array(*s).is_finite() || Vec3::from_array(*s).distance(pos) > SPIRE_REACH) {
            return Err(format!("control point {}: at most {MAX_SPIRES} spires, within {SPIRE_REACH} m of it", d.id));
        }
        if d.beacon.is_some_and(|b| !(0.0..=40.0).contains(&b)) {
            return Err(format!("control point {}: its beacon stands 0 to 40 m over it", d.id));
        }
        if let Some(c) = &d.cook {
            let (lo, hi) = (Vec3::from_array(c.min), Vec3::from_array(c.max));
            if !lo.is_finite() || !hi.is_finite() || lo.cmpge(hi).any()
                || ((lo + hi) * 0.5).distance(pos) > SPIRE_REACH || (hi - lo).max_element() > 2.0 * SPIRE_REACH
                || !(1.0..=150.0).contains(&c.damage) {
                return Err(format!("control point {}: its cook box has min under max, centered within {SPIRE_REACH} m of it, \
                    and burns 1 to 150 a second", d.id));
            }
        }
    }
    Ok(())
}

/// Runtime point: definition plus state. Sent whole in snapshots so clients
/// draw rings and predict drain without separate map data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub name: String,
    pub pos: Vec3,
    pub radius: f32,
    pub drain_radius: f32,
    pub drain_rate: f32,
    /// Simulated and drawn in the current mode.
    pub active: bool,
    /// Owning team index, if any.
    pub owner: Option<u8>,
    /// 0..1 toward `capturing`.
    pub progress: f32,
    pub capturing: Option<u8>,
    pub contested: bool,
    /// Its turret, if it has one: the kind, where it stands, its aim and
    /// its reload (`World::step_point_turrets`).
    #[serde(default)]
    pub turret: Option<crate::sim::deploy::DeployKind>,
    #[serde(default)]
    pub turret_at: Vec3,
    #[serde(default)]
    pub turret_aim: Vec3,
    #[serde(default)]
    pub turret_cooldown: f32,
    /// Its reach on this map (0: its deployable's).
    #[serde(default)]
    pub turret_range: f32,
    /// Seconds to flip it (0: `CAPTURE_SECONDS`).
    #[serde(default)]
    pub capture: f32,
    /// Its drain spares players on the ground.
    #[serde(default)]
    pub drain_airborne: bool,
    /// Health per second its drain takes too.
    #[serde(default)]
    pub drain_damage: f32,
    /// Its spire tips (`Definition::spires`).
    #[serde(default)]
    pub spires: Vec<Vec3>,
    /// Its beacon's height over it (`Definition::beacon`).
    #[serde(default)]
    pub beacon: Option<f32>,
    /// Where its holders burn, and how fast (`Definition::cook`; 0: nowhere).
    #[serde(default)]
    pub cook_min: Vec3,
    #[serde(default)]
    pub cook_max: Vec3,
    #[serde(default)]
    pub cook_damage: f32,
}

impl Point {
    pub fn from_def(d: &Definition, active: bool) -> Self {
        let drain = d.drain.clone();
        Self { name: d.name.clone(), pos: Vec3::from_array(d.pos), radius: d.radius,
            drain_radius: drain.as_ref().map_or(0.0, |x| x.radius),
            drain_rate: drain.map_or(0.0, |x| x.rate),
            active, owner: None, progress: 0.0, capturing: None, contested: false,
            turret: d.turret.as_ref().and_then(|t| crate::sim::deploy::DeployKind::from_key(&t.deployable)),
            turret_at: d.turret.as_ref().map_or(Vec3::ZERO, |t| Vec3::from_array(t.at)),
            turret_aim: Vec3::NEG_Z, turret_cooldown: 0.0,
            turret_range: d.turret.as_ref().and_then(|t| t.range).unwrap_or(0.0),
            capture: d.capture.unwrap_or(0.0), drain_airborne: d.drain.as_ref().is_some_and(|x| x.airborne),
            drain_damage: d.drain.as_ref().map_or(0.0, |x| x.damage),
            spires: d.spires.iter().map(|s| Vec3::from_array(*s)).collect(), beacon: d.beacon,
            cook_min: d.cook.as_ref().map_or(Vec3::ZERO, |c| Vec3::from_array(c.min)),
            cook_max: d.cook.as_ref().map_or(Vec3::ZERO, |c| Vec3::from_array(c.max)),
            cook_damage: d.cook.as_ref().map_or(0.0, |c| c.damage) }
    }
    /// Health per second it burns a player of `team` at `pos` with: only
    /// its holders, only inside its cook box.
    pub fn cooks(&self, team: u8, pos: Vec3) -> Option<f32> {
        (self.active && self.cook_damage > 0.0 && self.owner == Some(team)
            && pos.cmpge(self.cook_min).all() && pos.cmple(self.cook_max).all()).then_some(self.cook_damage)
    }
    /// Seconds of uncontested presence that flip it.
    pub fn capture_seconds(&self) -> f32 {
        if self.capture > 0.0 { self.capture } else { CAPTURE_SECONDS }
    }
    pub fn contains(&self, pos: Vec3) -> bool {
        let d = pos - self.pos;
        Vec3::new(d.x, 0.0, d.z).length() <= self.radius && d.y >= -BELOW && d.y <= ABOVE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Point index, new owner, previous owner.
    Captured { point: usize, team: u8, previous: Option<u8> },
    ContestedStart { point: usize },
}

/// Advance every active point by `dt` given living players as (team, position).
pub fn step(points: &mut [Point], players: &[(u8, Vec3)], dt: f32) -> Vec<Event> {
    let mut events = Vec::new();
    for (i, p) in points.iter_mut().enumerate() {
        if !p.active { continue; }
        let mut inside = [0usize; 2];
        for &(team, pos) in players {
            if (team as usize) < 2 && p.contains(pos) { inside[team as usize] += 1; }
        }
        let contested = inside[0] > 0 && inside[1] > 0;
        if contested && !p.contested { events.push(Event::ContestedStart { point: i }); }
        p.contested = contested;
        if contested { continue; }
        let seconds = p.capture_seconds();
        let rate = dt / seconds;
        let present = if inside[0] > 0 { Some(0u8) } else if inside[1] > 0 { Some(1u8) } else { None };
        match present {
            None => {
                p.progress = (p.progress - dt / (seconds * DECAY_SECONDS / CAPTURE_SECONDS)).max(0.0);
                if p.progress == 0.0 { p.capturing = None; }
            }
            Some(team) if p.owner == Some(team) => {
                // Defenders standing on their point undo enemy progress.
                p.progress = (p.progress - rate).max(0.0);
                if p.progress == 0.0 { p.capturing = None; }
            }
            Some(team) => {
                if p.capturing.is_some_and(|c| c != team) && p.progress > 0.0 {
                    p.progress = (p.progress - rate).max(0.0);
                    if p.progress == 0.0 { p.capturing = None; }
                } else {
                    p.capturing = Some(team);
                    p.progress += rate;
                    if p.progress >= 1.0 - 1e-4 {
                        let previous = p.owner;
                        p.owner = Some(team);
                        p.progress = 0.0;
                        p.capturing = None;
                        events.push(Event::Captured { point: i, team, previous });
                    }
                }
            }
        }
    }
    events
}

/// A drain on one player: the point it comes from, its rate, the health
/// per second it takes too, and the spire it strikes from (a point without
/// spires drains from no spire).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drained { pub point: usize, pub rate: f32, pub damage: f32, pub spire: Option<Vec3> }

/// The strongest drain on a player of `team` at `pos` (`airborne`: off the
/// ground) from the fields of active points held by the other team.
/// Neutral points never drain; a field marked `airborne` spares players on
/// the ground; a point with spires drains only players one of them sees
/// (`sees(tip)`), from the nearest that does.
pub fn drain_from(points: &[Point], team: u8, pos: Vec3, airborne: bool, sees: impl Fn(Vec3) -> bool) -> Option<Drained> {
    let mut best: Option<Drained> = None;
    for (k, p) in points.iter().enumerate() {
        if !p.active || p.drain_rate <= 0.0 || !p.owner.is_some_and(|o| o != team) { continue; }
        if p.drain_airborne && !airborne { continue; }
        if best.is_some_and(|b| b.rate >= p.drain_rate) { continue; }
        let d = pos - p.pos;
        if Vec3::new(d.x, 0.0, d.z).length() > p.drain_radius { continue; }
        let spire = if p.spires.is_empty() { None } else {
            let mut tips: Vec<Vec3> = p.spires.clone();
            tips.sort_by(|a, b| a.distance_squared(pos).total_cmp(&b.distance_squared(pos)));
            match tips.into_iter().find(|t| sees(*t)) { Some(t) => Some(t), None => continue }
        };
        best = Some(Drained { point: k, rate: p.drain_rate, damage: p.drain_damage, spire });
    }
    best
}

/// Energy per second drained from a player (`drain_from`).
pub fn drain_at(points: &[Point], team: u8, pos: Vec3, airborne: bool, sees: impl Fn(Vec3) -> bool) -> f32 {
    drain_from(points, team, pos, airborne, sees).map_or(0.0, |d| d.rate)
}

/// Capture & Hold score accrual; returns whole points earned per team this step.
pub fn score(points: &[Point], acc: &mut [f32; 2], dt: f32) -> [u32; 2] {
    let mut out = [0; 2];
    for p in points.iter().filter(|p| p.active) {
        if let Some(o) = p.owner { acc[o as usize] += CNH_POINTS_PER_SECOND * dt; }
    }
    for t in 0..2 {
        // Tolerate float accumulation just shy of a whole point.
        let whole = (acc[t] + 1e-3).floor();
        out[t] = whole as u32;
        acc[t] -= whole;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const DT: f32 = 1.0 / 60.0;
    fn point() -> Point {
        Point::from_def(&Definition { id: "beacon".into(), name: "Beacon".into(), pos: [0.0; 3], radius: 12.0,
            ctf_active: true, drain: Some(Drain { radius: 60.0, rate: DEFAULT_DRAIN_RATE, airborne: false, damage: 0.0 }), turret: None,
            capture: None, spires: Vec::new(), beacon: None, cook: None }, true)
    }
    fn run(p: &mut [Point], players: &[(u8, Vec3)], seconds: f32) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..(seconds / DT).round() as usize { all.extend(step(p, players, DT)); }
        all
    }
    const IN: Vec3 = Vec3::new(3.0, 1.2, 0.0);
    #[test]
    fn ten_seconds_uncontested_flips_the_point() {
        let mut p = [point()];
        assert!(run(&mut p, &[(0, IN)], 9.9).is_empty());
        assert_eq!(p[0].owner, None);
        let e = run(&mut p, &[(0, IN)], 0.2);
        assert_eq!(e, vec![Event::Captured { point: 0, team: 0, previous: None }]);
        assert_eq!(p[0].owner, Some(0));
        // Taking it from the other team also takes 10 s.
        assert!(run(&mut p, &[(1, IN)], 9.9).is_empty());
        assert_eq!(run(&mut p, &[(1, IN)], 0.2), vec![Event::Captured { point: 0, team: 1, previous: Some(0) }]);
    }
    #[test]
    fn contested_pauses_and_empty_decays() {
        let mut p = [point()];
        run(&mut p, &[(0, IN)], 5.0);
        let before = p[0].progress;
        let e = run(&mut p, &[(0, IN), (1, IN)], 3.0);
        assert_eq!(e, vec![Event::ContestedStart { point: 0 }]);
        assert!(p[0].contested && (p[0].progress - before).abs() < 1e-5);
        run(&mut p, &[], 4.0);
        assert!(!p[0].contested && (p[0].progress - (before - 0.2)).abs() < 0.01, "decays at 1/20 per s");
        run(&mut p, &[], 20.0);
        assert_eq!((p[0].progress, p[0].capturing), (0.0, None));
    }
    #[test]
    fn enemy_progress_must_be_undone_and_outsiders_do_not_count() {
        let mut p = [point()];
        run(&mut p, &[(0, IN)], 6.0);
        run(&mut p, &[(1, IN)], 6.0); // undoes 6 s of Ember progress
        assert!(p[0].progress < 0.01 && p[0].owner.is_none());
        let far = Vec3::new(13.0, 1.2, 0.0);
        let high = Vec3::new(0.0, 11.0, 0.0);
        assert!(run(&mut p, &[(1, far), (1, high)], 12.0).is_empty());
    }
    #[test]
    fn drain_only_hits_enemies_of_the_holder() {
        let mut p = [point()];
        let near = Vec3::new(40.0, 5.0, 0.0);
        let at = |p: &[Point], team, pos| drain_at(p, team, pos, false, |_| true);
        assert_eq!(at(&p, 1, near), 0.0, "neutral points never drain");
        run(&mut p, &[(0, IN)], 10.1);
        assert_eq!(at(&p, 1, near), DEFAULT_DRAIN_RATE);
        assert_eq!(at(&p, 0, near), 0.0, "holders are unaffected");
        assert_eq!(at(&p, 1, Vec3::new(61.0, 5.0, 0.0)), 0.0);
        p[0].active = false;
        assert_eq!(at(&p, 1, near), 0.0);
    }
    #[test]
    fn a_post_flips_at_a_touch() {
        let mut p = [point()];
        p[0].capture = 1.0;
        assert!(run(&mut p, &[(0, IN)], 0.9).is_empty());
        assert_eq!(run(&mut p, &[(0, IN)], 0.2), vec![Event::Captured { point: 0, team: 0, previous: None }]);
        assert_eq!(run(&mut p, &[(1, IN)], 1.05), vec![Event::Captured { point: 0, team: 1, previous: Some(0) }]);
        // Left alone, half a touch fades in a second (twice the touch, like 20 s to 10).
        run(&mut p, &[(0, IN)], 0.5);
        run(&mut p, &[], 1.1);
        assert_eq!((p[0].progress, p[0].capturing), (0.0, None));
    }
    #[test]
    fn spires_drain_flyers_they_see_from_the_nearest() {
        let mut p = [point()];
        p[0].owner = Some(0);
        p[0].drain_airborne = true;
        p[0].spires = vec![Vec3::new(10.0, 30.0, 0.0), Vec3::new(-10.0, 30.0, 0.0)];
        let flyer = Vec3::new(30.0, 20.0, 0.0);
        assert_eq!(drain_at(&p, 1, flyer, false, |_| true), 0.0, "players on the ground are spared");
        let d = drain_from(&p, 1, flyer, true, |_| true).unwrap();
        assert_eq!((d.point, d.rate, d.spire), (0, DEFAULT_DRAIN_RATE, Some(Vec3::new(10.0, 30.0, 0.0))));
        let far = drain_from(&p, 1, flyer, true, |t| t.x < 0.0).unwrap();
        assert_eq!(far.spire, Some(Vec3::new(-10.0, 30.0, 0.0)), "the nearest spire that sees them");
        assert_eq!(drain_at(&p, 1, flyer, true, |_| false), 0.0, "no spire sees them");
        assert_eq!(drain_at(&p, 0, flyer, true, |_| true), 0.0);
    }
    #[test]
    fn a_cook_box_burns_only_its_holders_inside_it() {
        let mut p = point();
        p.cook_min = Vec3::new(-10.0, -5.0, -10.0);
        p.cook_max = Vec3::new(10.0, 15.0, 10.0);
        p.cook_damage = 20.0;
        let inside = Vec3::new(5.0, 1.2, -5.0);
        assert_eq!(p.cooks(0, inside), None, "a neutral point burns nobody");
        p.owner = Some(0);
        assert_eq!(p.cooks(0, inside), Some(20.0), "its holders burn inside");
        assert_eq!(p.cooks(1, inside), None, "their enemies are welcome");
        assert_eq!(p.cooks(0, Vec3::new(11.0, 1.2, 0.0)), None, "outside the box");
        assert_eq!(p.cooks(0, Vec3::new(0.0, 16.0, 0.0)), None, "over it");
        p.active = false;
        assert_eq!(p.cooks(0, inside), None);
    }
    #[test]
    fn capture_and_hold_scores_per_held_point() {
        let mut p = [point(), point()];
        p[0].owner = Some(0); p[1].owner = Some(0);
        let mut acc = [0.0; 2];
        let mut total = [0u32; 2];
        for _ in 0..600 { let s = score(&p, &mut acc, DT); total[0] += s[0]; total[1] += s[1]; }
        assert_eq!(total, [20, 0], "two held points for 10 s");
    }
    #[test]
    fn manifest_entries_are_validated() {
        let json = r#"[{"id":"beacon","name":"Beacon","pos":[1,2,3],"ctf_active":true,"drain":{}}]"#;
        let defs: Vec<Definition> = serde_json::from_str(json).unwrap();
        assert_eq!(defs[0].radius, DEFAULT_RADIUS);
        assert_eq!(defs[0].drain, Some(Drain { radius: DEFAULT_DRAIN_RADIUS, rate: DEFAULT_DRAIN_RATE, airborne: false, damage: 0.0 }));
        assert!(validate(&defs).is_ok());
        for bad in [r#"[{"id":"Bad Id","name":"Beacon","pos":[0,0,0]}]"#,
                    r#"[{"id":"a","name":"<b>","pos":[0,0,0]}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"radius":0.5}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"drain":{"radius":1}}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0]},{"id":"a","name":"B","pos":[0,0,0]}]"#] {
            let defs: Vec<Definition> = serde_json::from_str(bad).unwrap();
            assert!(validate(&defs).is_err(), "{bad}");
        }
        assert!(serde_json::from_str::<Vec<Definition>>(r#"[{"id":"a","name":"A","pos":[0,0,0],"script":1}]"#).is_err());
        let post: Vec<Definition> = serde_json::from_str(r#"[{"id":"post","name":"Post","pos":[0,0,0],"radius":3,"capture":1,
            "beacon":4.5,"spires":[[10,30,0]],"drain":{"radius":80,"rate":15,"airborne":true,"damage":7.5},
            "cook":{"min":[-15,-2,-15],"max":[15,24,15],"damage":20}}]"#).unwrap();
        assert!(validate(&post).is_ok());
        let point = Point::from_def(&post[0], true);
        assert_eq!((point.capture_seconds(), point.drain_airborne, point.beacon, point.spires.len()), (1.0, true, Some(4.5), 1));
        assert_eq!((point.drain_damage, point.cook_damage, point.cook_max), (7.5, 20.0, Vec3::new(15.0, 24.0, 15.0)));
        for bad in [r#"[{"id":"a","name":"A","pos":[0,0,0],"capture":0.1}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"spires":[[0,0,400]]}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"beacon":50}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"drain":{"damage":41}}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"cook":{"min":[1,0,0],"max":[0,1,1],"damage":20}}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"cook":{"min":[0,0,0],"max":[1,1,1],"damage":0}}]"#,
                    r#"[{"id":"a","name":"A","pos":[0,0,0],"cook":{"min":[400,0,0],"max":[401,1,1],"damage":20}}]"#] {
            let defs: Vec<Definition> = serde_json::from_str(bad).unwrap();
            assert!(validate(&defs).is_err(), "{bad}");
        }
    }
}
