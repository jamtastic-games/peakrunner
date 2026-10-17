//! The engine's classes as definitions read them: one struct per class,
//! the fields a mod's JSON may set. References to other entries are ids
//! (strings) here; `Registry` resolves them to indices once it is built.
//! Every number's allowed range is in `fields`.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn yes() -> bool { true }
fn one() -> f32 { 1.0 }

/// A body players wear: how it moves, what it takes, what it carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Armor {
    pub name: String,
    #[serde(default)]
    pub about: String,
    /// Chosen at inventory stations (the stock library's extra armors aren't
    /// until a mod offers them).
    #[serde(default = "yes")]
    pub offered: bool,
    /// The body's look.
    #[serde(default)]
    pub build: Build,
    /// The skier movement model's numbers: jets, jump and energy.
    pub movement: crate::sim::Armor,
    /// Walking speed, damage taken and mass, relative to the base light armor.
    pub walk_scale: f32,
    pub damage_taken: f32,
    pub mass_scale: f32,
    /// Weapons carried at once, chosen at an inventory station.
    pub weapon_slots: u8,
    /// Weapons this armor may carry, with full rounds (0: one paid with energy).
    #[serde(default)]
    pub weapons: BTreeMap<String, u16>,
    /// What it carries until its wearer chooses (bots too), slot 1 first.
    #[serde(default)]
    pub kit: Vec<String>,
    /// What the grenade (G) and mine (N) keys throw, and how many are carried.
    #[serde(default)]
    pub grenade: Option<Carry>,
    #[serde(default)]
    pub mine: Option<Carry>,
    /// Damage taken of each kind, as a share (1: as usual).
    #[serde(default)]
    pub resist: BTreeMap<String, f32>,
    /// Its energy tank (default: the world's `movement.energy_max`).
    #[serde(default)]
    pub energy_max: Option<f32>,
    /// Most of a team that may wear it at once (0: no limit).
    #[serde(default)]
    pub team_cap: u16,
    /// The class key: what its wearer can do with it (`packs`).
    #[serde(default)]
    pub ability: Option<Ability>,
    /// What touching another player does (`bump`).
    #[serde(default)]
    pub touch: Option<Touch>,
    /// Things fixed to the body, by name: a weapon that fires by itself, a
    /// plate that stops shots from the front.
    #[serde(default)]
    pub mounts: BTreeMap<String, Mount>,
    /// Credits it costs where the mode has an economy (`modes::Economy`).
    #[serde(default)]
    pub price: u32,
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    /// Its body's size and color (looks only: collisions stay shared).
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub tint: Option<[f32; 3]>,
    /// The bot roles that wear it (`offense`, `defense`, `hunter`); none
    /// named by any armor: bots wear what they always have.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bot_roles: Vec<String>,
    /// The packs it may wear, by id (empty: any its station offers).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packs: Vec<String>,
}

/// An armor's class key. Used for its action, or switched on and off to
/// hold effects on while draining energy, like a pack. Each use (or each
/// second switched on) builds strain that ebbs away; at its limit or more,
/// the strain effect is given.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ability {
    pub name: String,
    #[serde(default)]
    pub about: String,
    #[serde(rename = "use")]
    pub use_: PackUse,
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub drain: f32,
    #[serde(default)]
    pub min_energy: f32,
    #[serde(default)]
    pub action: Option<Action>,
    #[serde(default)]
    pub energy: f32,
    #[serde(default)]
    pub cooldown: f32,
    #[serde(default)]
    pub charges: u8,
    #[serde(default)]
    pub strain: f32,
    #[serde(default)]
    pub strain_decay: f32,
    #[serde(default)]
    pub strain_limit: f32,
    #[serde(default)]
    pub strain_effect: Option<String>,
    /// When bots use it.
    #[serde(default)]
    pub bot: Option<BotUse>,
}

/// What a player wearing it does to those they touch, at most once a
/// `cooldown` per pair: teammates are mended and given energy and an
/// effect; enemies take damage of a kind, lose a share of their energy (to
/// the toucher) and get an effect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Touch {
    #[serde(default)]
    pub heal: f32,
    #[serde(default)]
    pub energy: f32,
    #[serde(default)]
    pub ally_effect: Option<String>,
    #[serde(default)]
    pub damage: f32,
    #[serde(default = "impact")]
    pub damage_kind: String,
    #[serde(default)]
    pub steal_energy: f32,
    #[serde(default)]
    pub effect: Option<String>,
    pub cooldown: f32,
}

fn impact() -> String { "impact".into() }

/// Something fixed to an armor's body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mount {
    /// Where it sits (its look; a blocker always faces forward).
    pub at: MountAt,
    /// A weapon firing by itself at the nearest enemy in sight within
    /// `range`, every `cooldown` seconds (default: the weapon's reload).
    #[serde(default)]
    pub weapon: Option<String>,
    #[serde(default)]
    pub range: f32,
    #[serde(default)]
    pub cooldown: Option<f32>,
    /// A plate stopping shots and blasts from the front.
    #[serde(default)]
    pub blocker: Option<Blocker>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountAt { Shoulder, Back, Front, Belt }

/// A plate stopping what comes within `arc` degrees of straight ahead:
/// only `share` of its damage gets through.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocker { pub arc: f32, pub share: f32 }

/// A throwable and how many of it an armor carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Carry { pub throwable: String, pub count: u8 }

/// How an armor's body looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Build { #[default] Light, Medium, Heavy }

/// A hand weapon: what it fires and what a shot costs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weapon {
    pub name: String,
    #[serde(default)]
    pub about: String,
    /// Ties versions of a weapon together across armors: changing armor
    /// swaps a carried weapon for the new armor's one of the same family.
    /// Saved favorites name families.
    pub family: String,
    /// Shown at inventory stations.
    #[serde(default = "yes")]
    pub offered: bool,
    pub projectile: String,
    /// Energy a shot costs; 0: a shot costs a round instead.
    #[serde(default)]
    pub energy: f32,
    /// Seconds between shots.
    pub reload: f32,
    /// Random speed added to a shot (m/s), the view kick up (radians) and the
    /// screen shake.
    #[serde(default)]
    pub spread: f32,
    #[serde(default)]
    pub recoil: f32,
    #[serde(default)]
    pub shake: f32,
    /// How far ahead (m) a shot from the muzzle steers onto the crosshair.
    #[serde(default = "aim")]
    pub aim: f32,
    /// Random speed (m/s) added to a bot's shot.
    #[serde(default)]
    pub bot_spread: f32,
    /// The view model held, which also places the muzzle, unless `muzzle`
    /// says where its model's barrel ends: metres right, up and back in the
    /// view model's frame from its anchor (`sim::vm_muzzle`), where its
    /// rounds, beams and flash leave in first person.
    pub view: View,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muzzle: Option<[f32; 3]>,
    /// The firing sound (a sound cue's name).
    pub sound: String,
    /// The zoom key zooms while holding it.
    #[serde(default)]
    pub zoom: bool,
    /// Its sight shows an elevation gauge: the aim's angle above the
    /// horizon, for lobbing shells.
    #[serde(default)]
    pub elevation: bool,
    /// Each shot also empties its holder's energy, so they can't jet right
    /// after one.
    #[serde(default)]
    pub empties_energy: bool,
    /// What a dying player's ammo pack holds of its rounds.
    #[serde(default)]
    pub loot: Loot,
    /// Lost when its carrier dies (back at respawn if it's in their setup).
    #[serde(default)]
    pub lost_on_death: bool,
    /// How the trigger works: `single` shots; `spin`, firing once the
    /// barrel has spun up (`spin_up`, `spin_down` seconds); `charge`, held
    /// to build power (`charge` seconds to full, `charge_scale` times the
    /// damage and speed at full) and fired on release, hurting the shooter
    /// `overload_damage` if held `overload` seconds past full; `burst`,
    /// `burst` shots per pull `burst_gap` apart; `sustained`, a beam held
    /// on, costing `energy` per second (its projectile is a `stream`).
    #[serde(default)]
    pub fire: Fire,
    #[serde(default)]
    pub spin_up: f32,
    #[serde(default)]
    pub spin_down: f32,
    #[serde(default)]
    pub charge: f32,
    #[serde(default = "one")]
    pub charge_scale: f32,
    #[serde(default)]
    pub overload: f32,
    #[serde(default)]
    pub overload_damage: f32,
    #[serde(default)]
    pub burst: u8,
    #[serde(default)]
    pub burst_gap: f32,
    /// Seconds an enemy must stay in the crosshair (within `lock_cone`
    /// degrees and `lock_range` metres) to lock on; homing rounds then fly
    /// at them.
    #[serde(default)]
    pub lock: f32,
    #[serde(default)]
    pub lock_cone: f32,
    #[serde(default)]
    pub lock_range: f32,
    /// Firing again sets off this weapon's rounds still out, instead of
    /// firing.
    #[serde(default)]
    pub remote: bool,
    /// Other ways to fire it, cycled with the fire-mode key: each changes
    /// some of these fields (and names itself with `name`).
    #[serde(default)]
    pub modes: Vec<serde_json::Value>,
    /// The modes, resolved: this weapon with each one's changes.
    #[serde(skip)]
    pub mode_defs: Vec<Weapon>,
    /// Credits it costs where the mode has an economy.
    #[serde(default)]
    pub price: u32,
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    /// The distances (m) bots fire it at, nearest and farthest.
    #[serde(default)]
    pub bot_range: Option<[f32; 2]>,
}

/// How a weapon's trigger works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fire { #[default] Single, Spin, Charge, Burst, Sustained }
fn aim() -> f32 { 80.0 }

/// A death pack's share of a weapon's rounds: none, a roll of 1 to 5 (never
/// more than carried), or all of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loot { #[default] None, Roll, All }

/// The view models the renderer draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View { Disc, Chaingun, Launcher, Mortar, Rifle, Railgun, Plasma, Blaster }

impl View {
    /// The view model's number, as the renderer's per-model tables and the
    /// muzzle offsets (`sim::muzzle_origin`) know them: the mortar shares the
    /// launcher's (2) and the railgun the rifle's (3), drawn as their heavier
    /// versions.
    pub fn kind(self) -> u8 {
        match self { View::Disc => 0, View::Chaingun => 1, View::Launcher | View::Mortar => 2, View::Rifle | View::Railgun => 3,
            View::Plasma => 4, View::Blaster => 5 }
    }
}

/// What flies, or beams, once something fires or throws it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projectile {
    pub class: ProjectileClass,
    /// Muzzle speed (m/s; a throw's comes from the throwable), the share of
    /// the shooter's velocity kept, and seconds it lasts.
    #[serde(default)]
    pub speed: f32,
    #[serde(default)]
    pub inherit: f32,
    pub life: f32,
    /// Share of gravity it falls with.
    #[serde(default)]
    pub gravity: f32,
    /// Collision radius against the map, and the extra reach against
    /// equipment.
    #[serde(default = "shot_radius")]
    pub radius: f32,
    #[serde(default)]
    pub reach: f32,
    #[serde(default)]
    pub water: Water,
    /// What happens when its life runs out.
    #[serde(default)]
    pub expire: Expire,
    /// The burst on contact (and on expiry): against bodies, and against
    /// equipment and deployables (default: the same one).
    #[serde(default)]
    pub blast: Option<String>,
    #[serde(default)]
    pub structure_blast: Option<String>,
    /// A hit's damage when it doesn't burst (a beam's too), and how hard
    /// such a hit flashes the victim's screen.
    #[serde(default)]
    pub damage: f32,
    /// Damage a second to a body it overlaps while it flies. It keeps
    /// going (the ground still bursts it). 0: only the hit or the burst.
    #[serde(default, skip_serializing_if = "zero_f32")]
    pub burn: f32,
    #[serde(default = "flash")]
    pub flash: f32,
    /// Share of its own blast the shooter takes.
    #[serde(default = "one")]
    pub self_damage: f32,
    /// Shells: seconds of flight before a contact sets it off.
    #[serde(default)]
    pub arm: f32,
    /// Bounces: the normal's share kept (shells, thrown), the slide kept
    /// along a surface (thrown), the speed kept after a bounce (shells; a
    /// straight round with it glances off the map), and the speed under
    /// which a thrown one comes to rest on a floor.
    #[serde(default)]
    pub elasticity: f32,
    #[serde(default)]
    pub slide: f32,
    #[serde(default)]
    pub keep: f32,
    #[serde(default)]
    pub rest_speed: f32,
    /// What its hits and its burst are made of (`damage/`).
    #[serde(default = "blast")]
    pub damage_kind: String,
    /// Trails smoke in flight.
    #[serde(default)]
    pub smoke: bool,
    /// Sets off resting mines along its path.
    #[serde(default)]
    pub clears_mines: bool,
    /// Beams: how far it reaches.
    #[serde(default)]
    pub range: f32,
    /// Thrown ones that lie and wait for an enemy (mines).
    #[serde(default)]
    pub trigger: Option<Trigger>,
    /// Speeding up along its flight (m/s²) to a top speed (0: no limit).
    #[serde(default)]
    pub accel: f32,
    #[serde(default)]
    pub max_speed: f32,
    /// Homing on a target.
    #[serde(default)]
    pub seek: Option<Seek>,
    /// Steered by the shooter's aim while it flies, turning this many
    /// degrees a second (0: not guided).
    #[serde(default)]
    pub guided: f32,
    /// Fired as this many pellets spread over `cone` degrees.
    #[serde(default = "one_pellet")]
    pub pellets: u8,
    #[serde(default)]
    pub cone: f32,
    /// Beams and streams: what it hits is marked for the shooter's team
    /// this many seconds (their screens show it, their turrets see it from
    /// afar, homing rounds fly at a marked player); a stream keeps one mark,
    /// on what it touches while held.
    #[serde(default)]
    pub marks: f32,
    /// Streams: what the held beam does each second to what it touches.
    #[serde(default)]
    pub stream: Option<Stream>,
    /// The kill feed's word for a kill by it, and by it from a turret.
    pub kill: String,
    #[serde(default)]
    pub kill_turret: String,
    /// How it is drawn, and its burst: one of the base game's projectile
    /// looks (default by class: straight a tracer, shell a grenade, thrown a
    /// hand grenade, beam a laser).
    #[serde(default)]
    pub look: Option<Look>,
    /// An effect for whoever it hits, or catches in its blast, or holds in
    /// its stream.
    #[serde(default)]
    pub effect: Option<String>,
    /// A direct hit drags its victim toward the shooter at this speed (m/s;
    /// negative: away); any hit drags the shooter toward it (`grapple`).
    #[serde(default)]
    pub pull: f32,
    #[serde(default)]
    pub grapple: f32,
    /// Come to rest (a thrown one), it becomes this deployable, or this
    /// item.
    #[serde(default)]
    pub becomes: Option<String>,
    #[serde(default)]
    pub drops: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    /// Its authored model (`models/projectiles/<model>/`, docs/model-pipeline.md),
    /// drawn in flight in place of its look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Its look's glow drawn this many times as big (tracers, bolts,
    /// beams), and in this color; a model is painted this color all over.
    #[serde(default = "unit_size")]
    pub size: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    /// A trail behind it in flight, with its look or its model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trail: Option<Trail>,
}
fn shot_radius() -> f32 { 0.12 }
fn unit_size() -> f32 { 1.0 }
fn zero_f32(v: &f32) -> bool { *v == 0.0 }
fn one_pellet() -> u8 { 1 }

/// A trail behind a round in flight, drawn by players' games only (it
/// changes nothing in play): a glowing ribbon that follows it, or smoke
/// left along its path that spreads and fades. `width` is the ribbon's
/// width or a fresh puff's size (m), `life` the seconds of flight the ribbon
/// spans or a puff lingers; 0 takes the style's own.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trail {
    pub style: TrailStyle,
    pub color: [f32; 3],
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub life: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailStyle { Ribbon, Smoke }

impl Trail {
    pub fn width(&self) -> f32 {
        if self.width > 0.0 { self.width } else { match self.style { TrailStyle::Ribbon => 0.25, TrailStyle::Smoke => 0.2 } }
    }
    pub fn life(&self) -> f32 {
        if self.life > 0.0 { self.life } else { match self.style { TrailStyle::Ribbon => 0.14, TrailStyle::Smoke => 1.4 } }
    }
}

/// How a homing round finds and follows a target: turning this many
/// degrees a second, choosing at launch the nearest target within `cone`
/// degrees of the aim and `range` metres (unless the weapon locked one), of
/// the kinds in `targets`; bursting within `proximity` metres of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seek {
    pub turn: f32,
    #[serde(default)]
    pub cone: f32,
    #[serde(default)]
    pub range: f32,
    #[serde(default = "seek_targets")]
    pub targets: Vec<SeekTarget>,
    #[serde(default)]
    pub proximity: f32,
}
fn seek_targets() -> Vec<SeekTarget> { vec![SeekTarget::Players] }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeekTarget { Players, Jetting, Marked }

/// A held beam's effect each second on what it touches: damage and energy
/// drained from enemies, health and hull restored to teammates and their
/// equipment, within `range` metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stream {
    pub range: f32,
    #[serde(default)]
    pub damage: f32,
    #[serde(default)]
    pub drain: f32,
    #[serde(default)]
    pub heal: f32,
    #[serde(default)]
    pub repair: f32,
    /// Seconds of beam that turn an enemy deployable to the shooter's team
    /// (0: it can't).
    #[serde(default)]
    pub claim: f32,
}
fn blast() -> String { "blast".into() }

/// The projectile looks the renderer draws (and their bursts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look { Disc, Tracer, Grenade, TurretPlasma, Mortar, HandGrenade, Mine, Laser, Rail, Plasma, Blaster }

impl Look {
    /// Its number in the renderer's tables: the base projectile it is.
    pub fn kind(self) -> u8 {
        match self {
            Look::Disc => 0, Look::Tracer => 1, Look::Grenade => 2, Look::TurretPlasma => 3, Look::Mortar => 5,
            Look::HandGrenade => 6, Look::Mine => 7, Look::Laser => 8, Look::Rail => 9, Look::Plasma => 10, Look::Blaster => 11,
        }
    }
}

impl Projectile {
    /// How it is drawn.
    pub fn look(&self) -> Look {
        self.look.unwrap_or(match self.class {
            ProjectileClass::Straight => Look::Tracer, ProjectileClass::Shell => Look::Grenade,
            ProjectileClass::Thrown => Look::HandGrenade, ProjectileClass::Beam | ProjectileClass::Stream => Look::Laser,
        })
    }
}
fn flash() -> f32 { 0.5 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectileClass { Straight, Shell, Thrown, Beam, Stream }

/// In water: slowed hard, put out, or burst where it meets the water at
/// `water::BURST_DAMAGE` of its damage and `water::BURST_PUSH` times its push.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Water { #[default] Drag, Fizzle, Burst }

/// At the end of its life: bursts, or is simply gone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expire { #[default] Explode, Vanish }

/// A mine's wait: its life once at rest, the seconds of that before it arms,
/// how near an enemy sets it off, how near a blast sets it off too, and how
/// close a shot must pass to hit it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger { pub rest_life: f32, pub arm: f32, pub radius: f32, pub sympathy: f32, pub shot_radius: f32 }

/// A burst: damage by distance and the kick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blast {
    #[serde(default)]
    pub falloff: Falloff,
    pub radius: f32,
    pub max_damage: f32,
    #[serde(default)]
    pub edge_fraction: f32,
    #[serde(default)]
    pub impulse: f32,
    #[serde(default)]
    pub core: f32,
    /// The quadratic curve: damage at the edge and toward the middle, and a
    /// fixed kick (m/s) instead of an impulse.
    #[serde(default)]
    pub edge_damage: f32,
    #[serde(default)]
    pub core_damage: f32,
    #[serde(default)]
    pub kick: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Falloff { #[default] Linear, Quadratic }

impl Blast {
    /// The linear (or concentrated) profile the sim's blast code uses.
    pub fn profile(&self) -> crate::combat::BlastProfile {
        crate::combat::BlastProfile { radius: self.radius, max_damage: self.max_damage, edge_fraction: self.edge_fraction,
            impulse: self.impulse, core: self.core }
    }
}

/// What a throw key throws.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Throwable {
    pub name: String,
    #[serde(default)]
    pub about: String,
    pub projectile: String,
    /// Full-strength throw speed (m/s), the thrower's velocity kept, the
    /// pause after a throw, and the weakest throw's share of full.
    pub speed: f32,
    #[serde(default = "one")]
    pub inherit: f32,
    pub time: f32,
    pub min_strength: f32,
    /// Most a team may have out at once, and throw in a match (0: no limit).
    #[serde(default)]
    pub team_max: u16,
    #[serde(default)]
    pub match_limit: u16,
}

/// Something players carry as a pack and place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deployable {
    pub name: String,
    #[serde(default)]
    pub about: String,
    pub behavior: Behavior,
    pub hull: f32,
    /// Most a team may have standing, plus this many per player on the
    /// team; most one player may have standing; most a team may place in a
    /// match (0: no limit for the last two).
    pub team_limit: u16,
    #[serde(default)]
    pub per_teammate: f32,
    #[serde(default)]
    pub per_player: u16,
    #[serde(default)]
    pub match_limit: u16,
    #[serde(default)]
    pub turret: Option<Turret>,
    /// Half its width, height and depth (m), for a box-shaped body
    /// (default: its behavior's).
    #[serde(default)]
    pub size: Option<[f32; 3]>,
    /// The base look it is drawn with (default: its behavior's).
    #[serde(default)]
    pub look: Option<DeployLook>,
    /// What it does besides its body: launches those who stand on it,
    /// takes them to its partner, holds an effect on those near, senses
    /// enemies, can be looked through from a command station, or shocks
    /// enemies who touch it.
    #[serde(default)]
    pub pad: Option<Pad>,
    #[serde(default)]
    pub teleport: Option<Teleport>,
    #[serde(default)]
    pub aura: Option<Aura>,
    #[serde(default)]
    pub sensor: Option<Sensor>,
    #[serde(default)]
    pub camera: bool,
    #[serde(default)]
    pub shock: Option<Shock>,
    /// A vehicle station: the vehicles bought standing on it.
    #[serde(default)]
    pub vehicles: Vec<String>,
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    /// Seconds it stands before it's gone by itself (0: until destroyed).
    #[serde(default)]
    pub life: f32,
}

/// A deployable's body: a turret's post (and gun), a plain `post`, a solid
/// `wall`, a `field` its team passes, an `ammo` crate, an `inventory`
/// kiosk, a low `pad` to stand on, a `gate` nobody bumps into, or a
/// `command` kiosk (cameras and turrets).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behavior { Turret, Wall, Field, Ammo, Inventory, Post, Pad, Gate, Command }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployLook { Turret, Wall, Field, Ammo, Inventory, Pylon, Sensor, Camera, Pad, Gate, Command, Beacon }

/// A pad: whoever stands on it is thrown, along their view, the pad's
/// facing or straight up (m/s; heavier bodies less). Predicted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pad { pub forward: f32, pub up: f32, #[serde(default)] pub along: PadAlong }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadAlong { #[default] View, Facing, Up }

/// A teleporter: a team's units of one kind link in pairs, in the order
/// they were placed; stepping into one comes out at the other.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Teleport {
    /// Seconds before the same player can go through again.
    pub cooldown: f32,
    /// A flag carrier may go through (dropping nothing).
    #[serde(default)]
    pub carrier: bool,
}

/// An effect held on those within `range` (m): its team, the enemy, or all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aura { pub effect: String, pub range: f32, #[serde(default)] pub targets: Targets }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Targets { #[default] Team, Enemies, All }

/// Senses enemies within `range` for its team (turrets reach farther, and
/// the team sees them marked), only those moving when `moving`, and
/// sounds an alarm when one comes in when `alarm`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sensor { pub range: f32, #[serde(default)] pub moving: bool, #[serde(default)] pub alarm: bool }

/// What touching it does to an enemy, at most once a `cooldown` each.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shock {
    #[serde(default)]
    pub damage: f32,
    #[serde(default = "shock_kind")]
    pub damage_kind: String,
    #[serde(default)]
    pub effect: Option<String>,
    pub cooldown: f32,
}

fn shock_kind() -> String { "shock".into() }

/// How a turret finds targets and what it fires.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Turret {
    /// Detection range alone, and with a powered team sensor.
    pub range: f32,
    pub sensed_range: f32,
    /// The round's speed and life, and seconds between shots.
    pub speed: f32,
    pub life: f32,
    pub cooldown: f32,
    /// Aims where a target will be.
    pub leads: bool,
    pub fires: bool,
    pub projectile: String,
    /// Engages only enemies using their jets; a homing round it fires
    /// follows the one it picked.
    #[serde(default)]
    pub jetting_only: bool,
}

/// Something worn on the back: a deployable to place, or a pack that does
/// something while worn, while switched on, or when used (the deploy key).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub name: String,
    #[serde(default)]
    pub about: String,
    /// Offered at inventory stations; only a base station hands it out.
    #[serde(default = "yes")]
    pub offered: bool,
    #[serde(default)]
    pub base_station_only: bool,
    /// What the deploy key does with it: `place` its deployable (the pack
    /// is used up); it works whenever worn (`always`); `toggle` it on and
    /// off; or `use` it for its action.
    #[serde(rename = "use")]
    pub use_: PackUse,
    #[serde(default)]
    pub deployable: Option<String>,
    /// While it works: effects held on, energy drained a second (`drain`,
    /// plus `drain_regen` times the wearer's recharge, `movement.regen`),
    /// and the energy it needs to keep working.
    #[serde(default)]
    pub effects: Vec<String>,
    #[serde(default)]
    pub drain: f32,
    #[serde(default)]
    pub drain_regen: f32,
    #[serde(default)]
    pub min_energy: f32,
    /// Used: what it does, its energy, the seconds before it can be used
    /// again, and how many uses it holds (0: no limit).
    #[serde(default)]
    pub action: Option<Action>,
    #[serde(default)]
    pub energy: f32,
    #[serde(default)]
    pub cooldown: f32,
    #[serde(default)]
    pub charges: u8,
    /// The wearer carries this multiple of their rounds.
    #[serde(default = "one")]
    pub ammo_scale: f32,
    /// Credits it costs where the mode has an economy.
    #[serde(default)]
    pub price: u32,
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    /// When bots use it: in a fight, defending, or always.
    #[serde(default)]
    pub bot: Option<BotUse>,
}

/// Something lying on the ground for whoever runs over it (`loadout::Loot`):
/// health, energy, a share of full rounds for each weapon carried,
/// grenades and mines, an effect. Taken only when it gives something.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub name: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub health: f32,
    #[serde(default)]
    pub energy: f32,
    #[serde(default)]
    pub rounds: f32,
    #[serde(default)]
    pub grenades: u8,
    #[serde(default)]
    pub mines: u8,
    #[serde(default)]
    pub effect: Option<String>,
    /// Seconds before one placed by a map comes back once taken.
    #[serde(default = "item_respawn")]
    pub respawn: f32,
    #[serde(default)]
    pub look: ItemLook,
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
}

fn item_respawn() -> f32 { 30.0 }

/// A vehicle (`sim::vehicles`): a flier or a ground vehicle with a hull and
/// an energy tank, seats (the pilot's first), and what each seat fires.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vehicle {
    pub name: String,
    #[serde(default)]
    pub about: String,
    pub hull: f32,
    pub energy_max: f32,
    #[serde(default)]
    pub regen: f32,
    /// How it moves: it flies, or it keeps to the ground.
    #[serde(default, skip_serializing_if = "VehicleDrive::flies")]
    pub drive: VehicleDrive,
    pub flight: Flight,
    /// Half its width, height and length (m), for collisions.
    pub size: [f32; 3],
    /// Mass relative to a light-armored player (blasts kick heavier less).
    #[serde(default = "one")]
    pub mass: f32,
    /// Its seats by name: `pilot` flies it; the rest ride, in name order.
    pub seats: BTreeMap<String, Seat>,
    /// The blast it bursts in when destroyed.
    #[serde(default)]
    pub blast: Option<String>,
    /// Damage it deals for each m/s past 10 that it runs into an enemy on
    /// foot or another vehicle at.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ram: f32,
    /// A flag carrier may ride it.
    #[serde(default)]
    pub carries_flag: bool,
    #[serde(default)]
    pub team_limit: u16,
    #[serde(default)]
    pub price: u32,
    /// Seconds it stays with nobody aboard before it's gone (0: until
    /// destroyed), so a team's limit isn't spent on vehicles left behind.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub abandon: f32,
    #[serde(default)]
    pub look: VehicleLook,
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// Its look built from parts (`Part`), drawn instead of (an armor: as
    /// well as) its base look.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
}

/// How it moves: accelerations along its nose (forward and back),
/// sideways and up (m/s²); the share of gravity it cancels while piloted;
/// drag (per second); its top speed (m/s); how fast it turns (degrees a
/// second: a flier toward where its pilot looks, a ground vehicle with the
/// sideways keys); energy a second while thrusting; and, on the ground, the
/// steepest slope it climbs and how hard its tracks hold (sideways always,
/// every way while it isn't driven). Predicted for the pilot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flight {
    pub thrust: f32,
    #[serde(default)]
    pub reverse: f32,
    #[serde(default)]
    pub strafe: f32,
    #[serde(default)]
    pub lift: f32,
    #[serde(default)]
    pub hover: f32,
    #[serde(default)]
    pub drag: f32,
    pub top_speed: f32,
    pub turn: f32,
    #[serde(default)]
    pub drain: f32,
    #[serde(default = "climb", skip_serializing_if = "is_climb")]
    pub climb: f32,
    #[serde(default = "grip", skip_serializing_if = "is_grip")]
    pub grip: f32,
}
fn climb() -> f32 { 35.0 }
fn is_climb(x: &f32) -> bool { *x == climb() }
fn grip() -> f32 { 8.0 }
fn is_grip(x: &f32) -> bool { *x == grip() }

/// How a vehicle moves (`sim::vehicles`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleDrive {
    /// It flies: the nose turns toward where the pilot looks, the jet key
    /// lifts, and `hover` holds it up while piloted.
    #[default]
    Fly,
    /// It keeps to the ground under full gravity: forward and back drive it,
    /// the sideways keys turn it, and it climbs slopes up to `climb`.
    Ground,
}
impl VehicleDrive { fn flies(&self) -> bool { *self == VehicleDrive::Fly } }

/// A seat: where it is on the vehicle (m: right, up, forward), the weapon
/// whoever sits there fires along their view, and whether its rider sits
/// in the open (seen, and hit by shots and blasts) rather than inside.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seat {
    #[serde(default)]
    pub at: [f32; 3],
    #[serde(default)]
    pub weapon: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exposed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleLook { #[default] Skiff, Transport }

impl Vehicle {
    /// Its seats in order: the pilot's, then the rest by name.
    pub fn seat_list(&self) -> Vec<&Seat> {
        self.seats.get("pilot").into_iter().chain(self.seats.iter().filter(|(k, _)| *k != "pilot").map(|(_, s)| s)).collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemLook { #[default] Kit, Box, Cell }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackUse { Place, Always, Toggle, Use }

/// Something a pack or a class key does when used. Exactly one of these.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    /// Fire this weapon from the shoulder (its projectile, at the crosshair).
    #[serde(default)]
    pub fire: Option<String>,
    /// Throw this throwable.
    #[serde(default)]
    pub throw: Option<String>,
    /// Put this deployable down where the player aims.
    #[serde(default)]
    pub place: Option<String>,
    /// Recall: put this deployable down where the player aims; used again,
    /// go to it (it's used up).
    #[serde(default)]
    pub recall: Option<String>,
    /// Give its user this effect.
    #[serde(default)]
    pub effect: Option<String>,
    /// A shove along the view (m/s): forward and up; the wearer's mass
    /// scales it down. Predicted by every client.
    #[serde(default)]
    pub impulse: Option<[f32; 2]>,
    /// Wear this armor for `seconds`, then go back.
    #[serde(default)]
    pub armor: Option<String>,
    #[serde(default)]
    pub seconds: f32,
}

/// A status effect: something a player has for a while (or while a pack
/// or ability holds it on). Its movement changes are predicted by every
/// client, so they are data only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub name: String,
    #[serde(default)]
    pub about: String,
    /// How long it lasts once given (a pack or ability holding it on keeps
    /// renewing it).
    pub seconds: f32,
    /// Each second: damage (of `damage_kind`), health restored, energy
    /// gained (negative: drained).
    #[serde(default)]
    pub damage: f32,
    #[serde(default = "blast")]
    pub damage_kind: String,
    #[serde(default)]
    pub heal: f32,
    #[serde(default)]
    pub energy: f32,
    /// Energy regained, as a multiple of the armor's (1: as usual).
    #[serde(default = "one")]
    pub regen_scale: f32,
    /// Movement, as multiples (1: as usual): jet thrust, sideways thrust,
    /// the speed the jet adds speed up to, walking speed, and the jump.
    #[serde(default)]
    pub movement: Moves,
    /// A shield: damage taken is paid from energy instead, this much energy
    /// per point of the hit before armor (0: no shield), only for these
    /// kinds (empty: all).
    #[serde(default)]
    pub absorb: f32,
    #[serde(default)]
    pub absorb_kinds: Vec<String>,
    /// Seen by enemies at this opacity (1: plainly; 0: not at all), with or
    /// without a name tag, by sensors or not; within `shimmer` metres they
    /// see a shimmer anyway. Firing or being hit ends it when `reveal`.
    #[serde(default = "one")]
    pub visibility: f32,
    #[serde(default)]
    pub hide_name: bool,
    #[serde(default)]
    pub sensor_hidden: bool,
    #[serde(default)]
    pub shimmer: f32,
    #[serde(default)]
    pub reveal: bool,
    /// What it stops while it lasts.
    #[serde(default)]
    pub blocks: Blocks,
    /// Given again while it lasts: start over (`refresh`), add the time
    /// (`extend`), or ignore (`keep`).
    #[serde(default)]
    pub stack: Stack,
    /// An inventory or ammo station ends it.
    #[serde(default)]
    pub cleansed: bool,
    /// Armors it never takes hold on.
    #[serde(default)]
    pub immune: Vec<String>,
    /// The screen tint its bearer sees (red, green, blue, strength).
    #[serde(default)]
    pub tint: [f32; 4],
    /// The hook (`hooks/<name>.rhai`, server only) that hears its events.
    #[serde(default)]
    pub hook: Option<String>,
    /// A glow round its bearer's body: color and strength.
    #[serde(default)]
    pub glow: Option<[f32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Moves { pub jet: f32, pub side: f32, pub air_cap: f32, pub walk: f32, pub jump: f32 }
impl Default for Moves { fn default() -> Self { Moves { jet: 1.0, side: 1.0, air_cap: 1.0, walk: 1.0, jump: 1.0 } } }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Blocks { pub fire: bool, pub jet: bool, pub flag: bool }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stack { #[default] Refresh, Extend, Keep }

/// A damage kind: what a hit is made of.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Damage {
    pub name: String,
    /// Direct hits: damage times this in the head, and in the back.
    #[serde(default = "one")]
    pub head: f32,
    #[serde(default = "one")]
    pub back: f32,
    /// A direct hit of at least this much (0: never) knocks the flag off a
    /// carrier hit from behind, drops the victim's pack, or knocks the
    /// weapon out of their hand.
    #[serde(default)]
    pub knock_flag: f32,
    #[serde(default)]
    pub drop_pack: f32,
    #[serde(default)]
    pub disarm: f32,
}

/// The world's own numbers (`defs.json`): movement, the repair tool, map
/// equipment, how far away a pack can be placed, and the blade of blade
/// zones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Globals {
    pub movement: crate::defs::Movement,
    pub repair: crate::defs::Repair,
    pub equipment: crate::defs::Equipment,
    pub placement: Placement,
    pub blade: crate::defs::Blade,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement { pub range: f32 }

/// One part of a look: a primitive shape placed, sized and turned (degrees:
/// pitch, yaw, roll) in its owner's frame (x right, y up, -z ahead), colored
/// (or its team's color), and glowing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub mesh: PartMesh,
    #[serde(default)]
    pub at: [f32; 3],
    pub size: [f32; 3],
    #[serde(default)]
    pub turn: [f32; 3],
    #[serde(default = "part_color")]
    pub color: [f32; 3],
    #[serde(default)]
    pub glow: f32,
    #[serde(default)]
    pub team: bool,
    /// A vehicle's part that turns with this seat's rider's view, about the
    /// upright line through the seat (a turret); with `elevates`, it tilts
    /// up and down with it too, about `pivot` (a gun barrel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aim: Option<String>,
    #[serde(default, skip_serializing_if = "is_origin")]
    pub pivot: [f32; 3],
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub elevates: bool,
}
fn part_color() -> [f32; 3] { [0.5, 0.52, 0.55] }
fn is_origin(x: &[f32; 3]) -> bool { *x == [0.0; 3] }
fn is_zero(x: &f32) -> bool { *x == 0.0 }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartMesh { Cube, Bevel, Sphere, Disc, Capsule, Dome }

/// When bots use a pack or class key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotUse { Combat, Defense, Always }
