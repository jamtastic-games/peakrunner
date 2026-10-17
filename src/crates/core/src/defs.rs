//! Definitions in their older shape: the tunable numbers of armor, weapons,
//! blasts, throwables, the repair tool, map equipment and deployables, as
//! the sim reads them (`defs()`). Since the engine's classes
//! (`crate::registry`), the base package keeps its items as entries in class
//! folders (`src/assets/mods/base/armors/`, `weapons/`, …) and its world
//! numbers in `defs.json`; this view is built from the registry in force, so
//! a mod's entries reach the sim through it. Each value was moved from a
//! Rust constant bit for bit (verified by the behaviour traces, `sim::trace`,
//! which a change to any of them moves).
use crate::combat::BlastProfile;
use crate::sim::Armor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defs {
    pub movement: Movement,
    pub armors: Armors,
    pub weapons: Weapons,
    pub blasts: Blasts,
    pub throwables: Throwables,
    pub repair: Repair,
    pub equipment: Equipment,
    pub deployables: Deployables,
}

/// A map's fixed equipment: how sensors and the two turret kinds detect and
/// fire, hull and generator-powered shields, and the hull share a destroyed
/// object must be repaired past to come back online.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equipment {
    pub sensor: crate::equipment::TurretProfile,
    pub bullet_turret: crate::equipment::TurretProfile,
    pub plasma_turret: crate::equipment::TurretProfile,
    pub turret_hull: crate::equipment::Durability,
    pub sensor_hull: crate::equipment::Durability,
    pub generator_hull: f32,
    pub station_hull: f32,
    pub online_fraction: f32,
}

/// Packs players deploy (turret, wall, force field, ammo station): the
/// deployed turret, each kind's hull and how many a team may have standing,
/// and how far away one can go down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployables {
    pub turret: crate::equipment::TurretProfile,
    pub health: PerDeployable<f32>,
    pub team_limit: PerDeployable<usize>,
    pub range: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerDeployable<T> { pub turret: T, pub wall: T, pub field: T, pub ammo: T, pub inventory: T }


/// The weapons: disc, chaingun, grenade launcher (heavy armor: mortar),
/// rifle (light: laser, heavy: railgun), plasma gun and blaster.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weapons {
    pub disc: Launcher,
    pub chaingun: Launcher,
    pub grenade_launcher: Launcher,
    pub mortar: Launcher,
    pub laser: Laser,
    pub railgun: Railgun,
    pub plasma: Launcher,
    pub blaster: Blaster,
}

/// A weapon that fires a round (`Disc`): muzzle speed, the share of the
/// shooter's velocity it keeps, seconds it flies, seconds between shots,
/// random spread (m/s), the view kick up (radians) and screen shake.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launcher {
    pub speed: f32, pub inherit: f32, pub life: f32, pub reload: f32,
    pub spread: f32, pub recoil: f32, pub shake: f32,
    /// Share of your own blast you take (the disc's jump).
    #[serde(default = "one")]
    pub self_damage: f32,
    /// Seconds after firing before a contact sets it off (it bounces until then).
    #[serde(default)]
    pub arm: f32,
}
fn one() -> f32 { 1.0 }

/// The light armor's rifle: an instant beam paid for with energy.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Laser { pub energy: f32, pub damage: f32, pub reload: f32, pub range: f32, pub fade: f32 }

/// The blaster: fast bolts paid for with energy that hurt only what they
/// hit; `bounce` is the share of speed a bolt keeps glancing off the map (0:
/// it stops there).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blaster { pub speed: f32, pub inherit: f32, pub life: f32, pub reload: f32, pub energy: f32, pub damage: f32, pub bounce: f32 }

/// The heavy armor's rifle: a fast slug.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Railgun { pub speed: f32, pub inherit: f32, pub damage: f32, pub reload: f32, pub life: f32 }

/// Hand grenades and mines: throw speeds (full strength), the thrower's
/// velocity kept, seconds between throws, the weakest throw's share, and each
/// one's timings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Throwables {
    pub grenade_throw: f32, pub mine_throw: f32, pub inherit: f32, pub time: f32, pub min_strength: f32,
    /// Seconds before a grenade goes off.
    pub grenade_fuse: f32,
    /// Mines: flight time to come to rest, life at rest, arming delay, trigger
    /// radius, most out per team, the enemy blast radius that sets one off,
    /// and how close a shot must pass to hit one.
    pub mine_flight: f32, pub mine_life: f32, pub mine_arm: f32, pub mine_trigger: f32, pub mine_team_max: usize,
    pub mine_sympathy: f32, pub mine_shot_radius: f32,
}

/// The repair tool: reach, energy per second and the least to run it,
/// health (players) or hull (equipment) restored per second, and the seconds
/// it rests after running the tank dry before it tries again.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repair {
    pub range: f32, pub energy: f32, pub min_energy: f32, pub player: f32, pub hull: f32,
    #[serde(default = "repair_rest")]
    pub rest: f32,
}
fn repair_rest() -> f32 { 1.5 }

/// The world and the skier movement model: gravity, a light armor's mass and
/// jump, the speed limits, walking, air and ski control, the energy tank, and
/// the extra kick blasts give an airborne body. Armor classes scale some of
/// these (`ArmorDef`); Football moves by its own model (`sim::football`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Movement {
    pub gravity: f32, pub mass: f32, pub jump_impulse: f32, pub min_jump_speed: f32, pub max_jump_speed: f32,
    pub horiz_max: f32, pub up_max: f32, pub up_resist_speed: f32, pub up_resist: f32,
    pub walk_accel: f32, pub walk_max: f32, pub ground_brake: f32, pub walk_step_down: f32,
    pub air_accel: f32, pub ski_turn_accel: f32, pub ski_slope_gravity: f32,
    pub energy_max: f32, pub air_hit_kick: f32,
    /// Landing damage: none below `landing_speed` (m/s), then
    /// `landing_scale` per m/s faster up to `landing_cap`, times
    /// `landing_share`, rounded down.
    #[serde(default = "landing_speed")]
    pub landing_speed: f32,
    #[serde(default = "landing_scale")]
    pub landing_scale: f32,
    #[serde(default = "landing_cap")]
    pub landing_cap: f32,
    #[serde(default = "landing_share")]
    pub landing_share: f32,
    /// The jump-boost: jets started on the ground, skiing or not, kick you
    /// straight up with this share of the jump (0 to 1) whatever your
    /// speed; a jump on the same takeoff takes its place.
    #[serde(default = "jet_jump")]
    pub jet_jump: f32,
}
fn jet_jump() -> f32 { 0.5 }
fn landing_speed() -> f32 { 18.0 }
fn landing_scale() -> f32 { 1.4 }
fn landing_cap() -> f32 { 22.0 }
fn landing_share() -> f32 { 0.5 }

/// The armor classes (light and heavy, bought at inventory stations) and
/// Football's own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Armors { pub light: ArmorDef, pub heavy: ArmorDef, pub football: Armor }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmorDef {
    /// Jet, jump and energy (the skier movement model's armor parameters).
    pub movement: Armor,
    /// Walking speed, damage taken and mass, relative to light armor.
    pub walk_scale: f32,
    pub damage_taken: f32,
    pub mass_scale: f32,
    /// Rounds per weapon: disc, chaingun, grenade launcher or mortar, rifle.
    pub ammo: [u16; 4],
    /// Plasma gun rounds.
    pub plasma_ammo: u16,
    /// Weapons carried at once, chosen at an inventory station (`arsenal`).
    pub weapon_slots: u8,
    /// Hand grenades and mines carried.
    pub throwables: [u8; 2],
}

/// Explosions: damage falloff and kick (`combat::BlastProfile`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blasts {
    pub disc: BlastProfile,
    pub grenade: BlastProfile,
    pub mortar: BlastProfile,
    pub hand_grenade: BlastProfile,
    pub mine: BlastProfile,
    /// The grenade launcher and mortar against equipment and deployables.
    pub plasma: BlastProfile,
    pub grenade_structure: BlastProfile,
    pub mortar_structure: BlastProfile,
}

/// The definitions in force: the base package's, or with the active mods'
/// changes laid over them (`crate::registry`).
pub fn defs() -> &'static Defs { crate::registry::registry().legacy() }

/// The base package's definitions.
pub fn base() -> &'static Defs { crate::registry::base().legacy() }

/// Lay `over` onto `base`: objects merge key by key, `null` removes a key,
/// anything else replaces. How a mod changes some definitions and keeps the
/// rest.
pub fn merge(base: &mut serde_json::Value, over: &serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                if v.is_null() { b.remove(k); continue; }
                match b.get_mut(k) { Some(slot) => merge(slot, v), None => { b.insert(k.clone(), v.clone()); } }
            }
        }
        (b, o) => *b = o.clone(),
    }
}

/// The blade (`sim::blade`): the side game a map's blade zones play
/// (`features::ZoneKind::Blade`), where every hand holds one and the jets
/// are off. A tap of fire is a quick swing, a hold builds power, quick
/// clicks chain into combos, and the jet key guards. Stamina is the energy
/// tank, counted here in points of 100 (a full tank).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blade {
    /// Seconds a press may last and still be a quick swing; held longer, it
    /// builds power, `charge` seconds to the full.
    pub tap: f32,
    pub charge: f32,
    /// Seconds after a swing in which a press carries the combo on.
    pub combo: f32,
    /// How far (m) from the body's middle a swing reaches, before a swing's
    /// own `reach`.
    pub reach: f32,
    /// Stamina regained a second, once `rest` seconds have passed since the
    /// last swing or guard.
    pub regen: f32,
    pub rest: f32,
    /// Guarding: stamina a second while held, the arc ahead it covers
    /// (degrees), stamina a blocked hit costs per point of its damage (what
    /// stamina can't pay comes off health and breaks the guard), and the
    /// share of walking speed kept.
    pub guard_drain: f32,
    pub guard_arc: f32,
    pub guard_cost: f32,
    pub guard_walk: f32,
    /// Seconds a broken guard or a clash of blades leaves you reeling.
    pub reel: f32,
    /// Seconds before someone cut down in a blade zone is back, at one of
    /// its spawns.
    pub respawn: f32,
    /// A power swing at full charge: its damage and push times this, its
    /// stamina times `charged_cost`.
    pub charged: f32,
    pub charged_cost: f32,
    /// The swings: quick (the three in turn: right to left, left to right,
    /// overhead), power, and the combos' ends: the whirl (a third quick
    /// swing in a combo), the lunge (a hold after one) and the slam (a hold
    /// after two).
    pub quick: Swing,
    pub power: Swing,
    pub whirl: Swing,
    pub lunge: Swing,
    pub slam: Swing,
}

/// One swing: seconds it takes and when in it the blade cuts (from, to),
/// the degrees it sweeps across the facing as it cuts, its damage, the
/// stamina it costs, how hard (m/s) it throws whoever it hits, a dash ahead
/// (m/s) and a hop (m/s) as it starts, and reach (m) over the blade's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Swing {
    pub time: f32,
    pub cut: [f32; 2],
    pub arc: f32,
    pub damage: f32,
    pub stamina: f32,
    pub push: f32,
    #[serde(default)]
    pub dash: f32,
    #[serde(default)]
    pub hop: f32,
    #[serde(default)]
    pub reach: f32,
}

pub fn movement() -> &'static Movement { &defs().movement }
/// The blade's numbers (`Blade`).
pub fn blade() -> &'static Blade { &crate::registry::registry().globals.blade }
pub fn weapons() -> &'static Weapons { &defs().weapons }
pub fn throws() -> &'static Throwables { &defs().throwables }
pub fn repair() -> &'static Repair { &defs().repair }
