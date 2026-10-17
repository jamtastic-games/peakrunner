use glam::{Mat4, Vec2, Vec3};
use serde::{Deserialize, Serialize};
#[path = "matchplay.rs"]
mod matchplay;
pub use matchplay::*;
#[path = "football.rs"]
pub mod football;
#[path = "football_bots.rs"]
mod football_bots;
#[path = "rabbit.rs"]
pub mod rabbit;
#[path = "duel.rs"]
pub mod duel;
#[path = "blade.rs"]
pub mod blade;
#[path = "bump.rs"]
pub mod bump;
#[path = "loadout.rs"]
pub mod loadout;
#[path = "deploy.rs"]
pub mod deploy;
#[path = "throwables.rs"]
pub mod throwables;
#[path = "rifles.rs"]
pub mod rifles;
#[path = "arsenal.rs"]
pub mod arsenal;
#[path = "deathmatch.rs"]
pub mod deathmatch;
#[path = "lagcomp.rs"]
pub mod lagcomp;
#[path = "firing.rs"]
pub mod firing;
#[path = "status.rs"]
pub mod status;
#[path = "packs.rs"]
pub mod packs;
#[path = "placed.rs"]
pub mod placed;
#[path = "items.rs"]
pub mod items;
#[path = "features.rs"]
pub mod features;
#[path = "vehicles.rs"]
pub mod vehicles;
#[path = "hooks.rs"]
pub mod hooks;
#[path = "rules.rs"]
pub mod rules;
#[cfg(test)]
#[path = "trace.rs"]
mod trace;
pub use loadout::ArmorClass;

use crate::terrain::{
    height, pillars, MapId, Pillar, EMBER_HOME, EYE, GLACIER_HOME, PLAYER_RADIUS,
};

pub const STEP: f32 = 1.0 / 60.0;
/// How far up or down players look (Football: `football::LOOK_LIMIT`).
pub const LOOK_LIMIT: f32 = 1.52;
// Movement, armor, weapon, blast, throwable and equipment tuning is data, in
// metres and seconds: the base package's defs.json (`crate::defs`).

/// Bots notice enemies inside this range, and only with a clear line of sight.
const BOT_SIGHT_RANGE: f32 = 95.0;
/// Most sight rays one bot casts per think (nearest enemies first).
const BOT_SIGHT_RAYS: usize = 3;
/// Seconds a bot keeps heading for an enemy's last-seen spot after losing sight.
const BOT_MEMORY: f32 = 2.5;
/// Route searches allowed per tick across all bots, and the node budget of
/// each: bounds the worst tick when every bot re-plans at once.
const BOT_PLANS_PER_TICK: u32 = 2;
const BOT_SEARCH_BUDGET: usize = 40_000;
/// Re-plan at least this often, and when a bot makes no progress this long.
const BOT_REPLAN_SECONDS: f32 = 6.0;
const BOT_STUCK_SECONDS: f32 = 2.5;
/// Flyers: horizontal distance at which they hand back to the ground route.
const BOT_FLY_HANDOVER: f32 = 30.0;
/// Flyers: cruise height above the highest ground ahead.
const BOT_FLY_CLEARANCE: f32 = 28.0;
/// Flyers: how far a target below can be for a dive.
const BOT_DIVE_RANGE: f32 = 90.0;
/// Flyers: jet again once the tank refills past this, coast below the floor.
const BOT_FLY_REFILL: f32 = 45.0;
const BOT_FLY_EMPTY: f32 = 8.0;
/// Flyers: height a single tank reliably climbs; higher goals take the route.
const BOT_FLY_CLIMB: f32 = 30.0;
/// Flyers: a flyer on the ground route takes off again only beyond this.
const BOT_FLY_TAKEOFF: f32 = 320.0;
/// Flyers: within this of a goal out of climbing reach, land and take the
/// route from the open field (routes from under a floating base fail).
const BOT_FLY_CLIMB_RANGE: f32 = 260.0;
/// Viewmodel pose. The world muzzle uses the same numbers, or the disc leaves the eye.
pub const VM_FOV: f32 = 65.0;
pub const VM_TURN_Y: f32 = 0.18;
pub const VM_TURN_X: f32 = -0.08;
/// The disc launcher's and the chaingun's barrels converge on the point
/// their rounds fly to (80 m ahead on the crosshair): their muzzles sit
/// about 0.27 m right of and 0.15 m below the eye, so they turn in
/// atan(0.27 / 79) and up atan(0.15 / 79). The chaingun used the grenade
/// launcher's 0.18 / -0.08, which pointed the barrel well left of and below
/// the stream of rounds.
pub const VM_DISC_TURN_Y: f32 = 0.0034;
pub const VM_DISC_TURN_X: f32 = 0.0018;
pub const VM_CHAIN_TURN_Y: f32 = 0.0035;
pub const VM_CHAIN_TURN_X: f32 = 0.0021;

/// Viewmodel yaw and pitch for each weapon slot.
pub fn vm_turn(weapon: u8) -> (f32, f32) {
    match weapon { 0 => (VM_DISC_TURN_Y, VM_DISC_TURN_X), 1 => (VM_CHAIN_TURN_Y, VM_CHAIN_TURN_X), _ => (VM_TURN_Y, VM_TURN_X) }
}
/// Where the view models sit in the view (m right, up and forward of the
/// eye at `VM_FOV`): the hand on the grip low on the right, the barrel up
/// toward the crosshair (marker `muzzle3`; they sat lower and farther out,
/// the grip and hand below the screen's edge).
pub const VM_ANCHOR_DISC: Vec3 = Vec3::new(0.30, -0.13, -0.60);
pub const VM_ANCHOR_BOLT: Vec3 = Vec3::new(0.31, -0.15, -0.60);
/// The mortar's wide tube sits lower and farther out, below the crosshair.
pub const VM_ANCHOR_MORTAR: Vec3 = Vec3::new(0.32, -0.22, -0.62);

/// Where view model `view` sits in the view (`VM_ANCHOR_DISC` and the rest).
pub fn vm_anchor(view: crate::registry::View) -> Vec3 {
    match view {
        crate::registry::View::Disc => VM_ANCHOR_DISC,
        crate::registry::View::Mortar => VM_ANCHOR_MORTAR,
        _ => VM_ANCHOR_BOLT,
    }
}
pub const VM_MUZZLE_DISC: Vec3 = Vec3::new(0.0, 0.075, -0.79);
pub const VM_MUZZLE_BOLT: Vec3 = Vec3::new(0.0, 0.02, -0.59);

/// Where a view model's muzzle sits in its turned frame (`vm_turn`), from
/// its anchor. A weapon whose model's barrel ends elsewhere says where
/// (`registry::Weapon::muzzle`, `weapon_muzzle`).
pub fn vm_muzzle(weapon: u8) -> Vec3 {
    if weapon == 0 { VM_MUZZLE_DISC } else { VM_MUZZLE_BOLT }
}
/// Collision radius of player discs, bolts and grenades against map geometry.
const SHOT_RADIUS: f32 = 0.12;
/// Jet and jump capability. CTF and Capture & Hold use the approved
/// standard armor; Football uses its own (`defs().armors.football`), a short strong
/// burst. The mode travels in snapshots, so prediction uses the same one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Armor {
    /// Jet acceleration (m/s², before gravity): all upward with no movement
    /// key held.
    pub jet: f32,
    /// Horizontal jet acceleration along WASD, paid out of `jet`: what
    /// pushes sideways no longer lifts.
    pub side: f32,
    /// Energy per second while jetting, and regained per second otherwise.
    pub drain: f32,
    pub regen: f32,
    /// Energy needed to start or keep jetting.
    pub min_jet: f32,
    /// Jump impulse as a fraction of the standard jump.
    pub jump: f32,
    /// Upward thrust fades as climb speed nears the thrust cap.
    pub fade_up: bool,
    /// Horizontal speed (m/s) up to which the jet adds speed; faster motion
    /// (skiing, a blast) is kept but not added to.
    pub air_cap: f32,
    /// Skiing slowdown (m/s²) on flat ground, fading on slopes. The approved
    /// CTF movement has none; Football bleeds speed so runners must boost.
    pub ski_drag: f32,
}

/// The light armor's jet, jump and energy (`defs().armors.light`).
pub fn standard_armor() -> Armor { crate::defs::defs().armors.light.movement }

/// The armor every player wears under `mode`.
pub fn armor_for(mode: crate::map_catalog::SupportedMode) -> Armor {
    if mode.def().movement == crate::modes::MovementModel::Classic { crate::defs::defs().armors.football } else { standard_armor() }
}

/// Seconds between shots of weapon `kind` (a wire number).
pub fn weapon_reload(kind: u8) -> f32 {
    let r = crate::registry::registry();
    r.weapons.get(kind as usize).or_else(|| r.weapons.get(0)).map_or(1.0, |w| w.reload)
}
/// Mouse look: radians per mouse count (also the spectator camera's).
pub const SENS: f32 = 0.00235;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Team {
    Ember = 0,
    Glacier = 1,
}

impl Team {
    pub fn other(self) -> Team {
        match self {
            Team::Ember => Team::Glacier,
            Team::Glacier => Team::Ember,
        }
    }
    pub fn idx(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchState {
    Flyby = 0,
    Playing = 1,
    Paused = 2,
    Ended = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum BotRole {
    Offense,
    Defense,
    Hunter,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub move_x: f32,
    pub move_z: f32,
    pub jump: bool,
    pub jet: bool,
    pub fire: bool,
    pub interact: bool,
    /// Repair tool held (Q). An intent: the server decides what it repairs.
    pub repair: bool,
    /// An inventory purchase (`loadout::BUY_*`), applied at a station.
    pub buy: u16,
    /// A saved favorite to equip at a station, once.
    pub equip: Option<loadout::Equip>,
    /// Toss the flag you carry (Ctrl+F), once.
    pub toss: bool,
    /// The fire-mode key, pressed this tick.
    pub mode: bool,
    /// The class key, pressed this tick.
    pub ability: bool,
    /// At a command station: the team turret operated (deployable index +
    /// 1), and a team waypoint to set, once.
    pub operate: u16,
    pub waypoint: Option<Vec3>,
    /// Drop the weapon in hand or the pack worn (`items::DROP_*`), once.
    pub drop: u8,
    /// Board the nearest vehicle, or leave the one ridden (`vehicles`), once.
    pub board: bool,
    /// Deploy the carried pack where you aim, turned this much (radians)
    /// from your facing.
    pub deploy: bool,
    pub deploy_turn: f32,
    /// Suicide (Ctrl+K), for a quick respawn. An intent, like `kit`.
    pub suicide: bool,
    /// Throw a grenade or mine (`throwables::THROW_*`) at this strength (0..1).
    pub throw: u8,
    pub throw_strength: f32,
    pub weapon: u8,
    pub look_stick_x: f32,
    pub look_stick_y: f32,
    #[allow(dead_code)]
    pub keys_override: Option<Vec<String>>,
    jump_prev: bool,
}

impl Input {
    /// Forget the one-shot intents (buy, deploy, suicide, throw) once a tick
    /// has used them. The client latches a key press until then, so a press
    /// on a frame that runs no tick isn't lost, and one press acts once.
    pub fn clear_once(&mut self) {
        self.buy = 0;
        self.equip = None;
        self.toss = false;
        self.mode = false;
        self.ability = false;
        self.waypoint = None;
        self.drop = 0;
        self.board = false;
        self.deploy = false;
        self.deploy_turn = 0.0;
        self.suicide = false;
        self.throw = 0;
        self.throw_strength = 0.0;
    }
}

impl Default for Input {
    fn default() -> Self {
        Self {
            move_x: 0.0,
            move_z: 0.0,
            jump: false,
            jet: false,
            fire: false,
            interact: false,
            repair: false,
            buy: 0,
            equip: None,
            toss: false,
            mode: false,
            ability: false,
            operate: 0,
            waypoint: None,
            drop: 0,
            board: false,
            deploy: false,
            deploy_turn: 0.0,
            suicide: false,
            throw: 0,
            throw_strength: 0.0,
            weapon: 0,
            look_stick_x: 0.0,
            look_stick_y: 0.0,
            keys_override: None,
            jump_prev: false,
        }
    }
}

/// The setup a player chose at an inventory station, given back at every
/// respawn (and round or map change) until they choose otherwise: their
/// armor, weapons and a pack, each once chosen. Server and offline state,
/// never sent; unset parts leave the spawn as it was, so someone who never
/// buys (and every bot) spawns as before.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Setup {
    pub armor: Option<ArmorClass>,
    /// The weapons chosen, slot 1 first (`arsenal`).
    pub weapons: Option<arsenal::Loadout>,
    pub pack: Option<packs::PackKind>,
}

/// Where a snapshot puts a player its viewer can't see (`status::UNSEEN`):
/// far below every map, standing still.
pub const HIDDEN_Y: f32 = -4000.0;
/// How fast a dead body skids to a stop (per second on the ground), the
/// speed it rests below, and how water slows and floats it (`fall_body`).
const BODY_FRICTION: f32 = 9.0;
const BODY_REST_SPEED: f32 = 1.2;
const BODY_WATER_DRAG: f32 = 3.0;
const BODY_BUOYANCY: f32 = 1.12;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub net_id: u32,
    pub name: String,
    pub frags: u32,
    pub losses: u32,
    pub shots: u32,
    pub hits: u32,
    jump_prev: bool,
    /// The jets were held last tick: a fresh press on the ground jump-boosts.
    #[serde(default)]
    jet_prev: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
    pub energy: f32,
    pub team: Team,
    pub alive: bool,
    pub respawn: f32,
    pub carrying: Option<Team>,
    pub is_bot: bool,
    pub remote: bool,
    pub on_ground: bool,
    pub skiing: bool,
    pub jetting: bool,
    pub cooldown: f32,
    pub weapon: u8,
    /// Armor class, chosen at an inventory station (kept across deaths).
    #[serde(default)]
    pub armor: ArmorClass,
    /// Rounds per weapon, by the weapon's wire number (weapons paid with
    /// energy have none); refilled on respawn and at stations.
    #[serde(default)]
    pub ammo: Vec<u16>,
    /// A rifle bought at an inventory station (weapon slot 4; the laser in
    /// light armor, the railgun in heavy). Lost on death; back on respawn
    /// when it's part of the player's `setup`.
    /// The weapons carried, slot 1 first (`arsenal`): the rifle is one of
    /// them once chosen (lost on death, back at respawn from the `setup`).
    #[serde(default)]
    pub loadout: arsenal::Loadout,
    /// The setup chosen at an inventory station (`Setup`).
    #[serde(skip)]
    pub setup: Setup,
    /// The pack worn (`packs`; bought at a station, lost on death; back
    /// on respawn when it's part of the player's `setup`).
    #[serde(default)]
    pub pack: Option<packs::PackKind>,
    /// The pack and the class key (`packs`): a switched pack is on; seconds
    /// before the pack and the class key can be used again and the uses
    /// spent; strain built up by the class key; and a borrowed armor (the
    /// armor worn before, + 1; 0 none) with its seconds left.
    #[serde(default)]
    pub pack_on: bool,
    #[serde(default)]
    pub pack_clock: f32,
    #[serde(default)]
    pub pack_uses: u8,
    #[serde(default)]
    pub ability_clock: f32,
    #[serde(default)]
    pub ability_uses: u8,
    #[serde(default)]
    pub strain: f32,
    #[serde(default)]
    pub ability_on: bool,
    /// Standing on a pad, platform or moving part (`features`): ground for
    /// the next step.
    #[serde(default)]
    pub footing: bool,
    /// The vehicle ridden (its index + 1; 0 none; `vehicles`).
    #[serde(default)]
    pub riding: u8,
    /// Credits of their own, where the mode's economy isn't shared (`items`).
    #[serde(default)]
    pub credits: f32,
    /// Seconds until each mounted weapon fires again, and until a
    /// teleporter takes them or a shock hurts them again (server only).
    #[serde(skip)]
    pub mount_clocks: [f32; packs::MAX_MOUNTS],
    #[serde(skip)]
    pub teleport_clock: f32,
    /// The trigger zones they're in (bits by zone; server only).
    #[serde(skip)]
    pub zones_in: u64,
    /// A bot's presses this think (`bot_uses`): 1 its pack, 2 its class
    /// key, 4 placing its pack (server only).
    #[serde(skip)]
    pub bot_press: u8,
    /// A claiming stream's target (deployable index + 1) and seconds on it
    /// (server only).
    #[serde(skip)]
    pub claim_on: u16,
    #[serde(skip)]
    pub claim_time: f32,
    #[serde(skip)]
    pub shock_clock: f32,
    #[serde(default)]
    pub swap_back: u8,
    #[serde(default)]
    pub swap_clock: f32,
    /// Hand grenades and mines carried (`throwables::max_throwables`).
    #[serde(default)]
    pub throwables: [u8; 2],
    /// Seconds until this player can throw again.
    #[serde(default)]
    pub throw_clock: f32,
    /// A grenade/mine asked for during the pause after a throw; it goes as
    /// soon as the pause ends, so a quick press is never lost.
    #[serde(default)]
    pub throw_held: u8,
    /// Where this player's repair beam lands this tick, while repairing.
    #[serde(default)]
    pub repair_beam: Option<Vec3>,
    /// The repair tool is out (its key held), whether its beam runs or it
    /// rests on an empty tank; the hands hold it either way.
    #[serde(default)]
    pub repair_out: bool,
    /// The repair tool works on its holder (no target in sight): drawn as
    /// a glow on the body, not a stream (marker `repair2`).
    #[serde(default)]
    pub repair_self: bool,
    /// Seconds before a repair tool that ran the tank dry tries again.
    /// Server-only state.
    #[serde(skip)]
    repair_rest: f32,
    /// The trigger (`firing`): a spinning barrel (0 to 1), seconds a charge
    /// has been held, shots left in a burst, the held weapon's fire mode
    /// (and the weapon it's for), whether fire was held last tick, a lock (a
    /// target's slot + 1, and seconds held), and where a held beam lands.
    #[serde(default)]
    pub spin: f32,
    #[serde(default)]
    pub charge: f32,
    #[serde(default)]
    pub burst_left: u8,
    #[serde(default)]
    pub fire_mode: u8,
    #[serde(default)]
    pub mode_of: u8,
    #[serde(default)]
    pub fire_prev: bool,
    #[serde(default)]
    pub lock_target: u16,
    #[serde(default)]
    pub lock_time: f32,
    #[serde(default)]
    pub stream: Option<Vec3>,
    /// Status effects (`status`): each effect's wire number and seconds left.
    #[serde(default)]
    pub effects: Vec<(u8, f32)>,
    /// The blade of blade zones (`blade`): the swing, guard and combo, and
    /// the latest blow taken.
    #[serde(default)]
    pub blade: blade::State,
    /// Football: seconds left knocked down by a tackle (no movement).
    #[serde(default)]
    pub stun: f32,
    /// Football: seconds until a Ctrl+K takes effect (0 = none asked).
    #[serde(default)]
    pub suicide_in: f32,
    bot_role: BotRole,
    bot_goal: Vec3,
    bot_think: f32,
    /// Enemy this bot can see, chosen at its last think. Server-only state.
    #[serde(skip)]
    bot_target: Option<usize>,
    /// Where it last saw an enemy, and for how long it keeps hunting there.
    #[serde(skip)]
    bot_seen: Vec3,
    #[serde(skip)]
    bot_memory: f32,
    /// Personality and route-following state. Server-only; never sent.
    #[serde(skip)]
    bot_profile: crate::bot_nav::BotProfile,
    #[serde(skip)]
    bot_path: Vec<u32>,
    #[serde(skip)]
    bot_path_i: usize,
    #[serde(skip)]
    bot_path_goal: Vec3,
    #[serde(skip)]
    bot_replan: f32,
    #[serde(skip)]
    bot_stuck: f32,
    #[serde(skip)]
    bot_best: f32,
    #[serde(skip)]
    bot_link: Option<crate::bot_nav::Link>,
    /// The next link is a jet climb: save energy for it on the way.
    #[serde(skip)]
    bot_saving: bool,
    /// Seconds the current target has stayed in sight (reaction time).
    #[serde(skip)]
    bot_seen_for: f32,
    /// This think's aim error, yaw and pitch radians.
    #[serde(skip)]
    bot_aim: Vec2,
    #[serde(skip)]
    bot_repair: bool,
    /// Links this bot got stuck on recently; its routes avoid them.
    #[serde(skip)]
    bot_avoid: Vec<(u32, u32)>,
    /// Flyers: crossing high on jet arcs instead of following a ground route.
    #[serde(skip)]
    bot_fly: bool,
    /// Football: pass this tick (offline bots).
    #[serde(skip)]
    bot_pass: bool,
    /// Football: seconds into a leap onto a raised goal (0 = not leaping).
    #[serde(skip)]
    bot_leap: f32,
    /// Football: seconds into a pounce on an enemy (0 = none).
    #[serde(skip)]
    bot_pounce: f32,
    /// Football: the play this bot is making (`football_bots`).
    #[serde(skip)]
    bot_fb: football_bots::Brain,
    /// Football: what this bot last saw of each enemy, by slot
    /// (`football_bots`, `World::fb_watch`).
    #[serde(skip)]
    bot_sight: Vec<Option<football_bots::Sighting>>,
    /// Sparring with the blade in the duel lobby's tower (`blade::Spar`).
    #[serde(skip)]
    pub(crate) spar: blade::Spar,
    coyote: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Disc {
    pub pos: Vec3,
    pub vel: Vec3,
    pub team: Team,
    pub owner: usize,
    pub life: f32,
    /// The projectile's wire number (`crate::registry`): in the base game
    /// 0 disc, 1 bullet, 2 grenade, 3 turret plasma, 5 mortar, 6 hand
    /// grenade, 7 mine, 8 laser beam, 9 railgun slug, 10 plasma bolt, 11
    /// blaster bolt.
    pub kind: u8,
    pub spin: f32,
    /// What a homing round flies at: a player's slot + 1 (0: nothing).
    /// Server state; snapshots leave it out.
    #[serde(default)]
    pub target: u16,
    /// Its damage as a multiple (a charged shot's power; 1: as defined).
    /// Server state; snapshots leave it out.
    #[serde(default = "one")]
    pub power: f32,
}
fn one() -> f32 { 1.0 }

/// What a bolt or round struck (resolved after the projectile step).
/// A player hit carries (victim, owner, where it struck, which way it flew).
enum Hit { Player(usize, usize, Vec3, Vec3), Equipment(usize), Deployable(usize), Vehicle(usize, usize) }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Explosion {
    pub pos: Vec3,
    pub age: f32,
    pub max_r: f32,
    pub kind: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmokePuff {
    pub pos: Vec3,
    pub age: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Flag {
    pub team: Team,
    pub pos: Vec3,
    pub home: Vec3,
    pub carrier: Option<usize>,
    pub drop_timer: f32,
    /// A tossed flag's velocity while it flies (`World::toss_flag`); zero
    /// at rest.
    #[serde(default)]
    pub vel: Vec3,
    /// Who tossed it, and seconds before they may take it again.
    #[serde(skip)]
    pub tossed_by: Option<usize>,
    #[serde(skip)]
    pub toss_guard: f32,
}

/// A flag toss: speed along the aim (plus the thrower's velocity and a
/// little lift), and how long the thrower can't take it back (marker `toss1`).
pub const TOSS_SPEED: f32 = 16.0;
pub const TOSS_LIFT: f32 = 3.0;
pub const TOSS_GUARD: f32 = 1.0;

pub struct World {
    pub feed: Vec<crate::feed::Entry>,
    pub equipment: Vec<crate::equipment::State>,
    pub players: Vec<Player>,
    pub discs: Vec<Disc>,
    pub explosions: Vec<Explosion>,
    pub smoke: Vec<SmokePuff>,
    pub flags: [Flag; 2],
    pub pillars: Vec<Pillar>,
    pub score: [u32; 2],
    pub time_left: f32,
    pub state: MatchState,
    pub acc: f32,
    pub flyby: f32,
    pub player_id: usize,
    pub input: Input,
    pub message: String,
    pub message_t: f32,
    pub hitmarker: f32,
    /// Client only: the local player is lining up their pack (the weapon is
    /// replaced by the deployer), and the hologram of where it would go:
    /// the unit and whether it fits there (None: nothing in reach).
    pub placing: bool,
    pub deploy_ghost: Option<(deploy::Deployable, bool)>,
    /// Client only: zoomed through a rifle's sight (held zoom key), at
    /// which of its levels (`rifles::ZOOM_FOVS`; the wheel picks), and the
    /// third-person view (the camera behind your body; toggled).
    pub zoomed: bool,
    pub zoom_level: u8,
    pub third_person: bool,
    /// Client only: watching as a spectator (`teams1`). The local "player"
    /// is then a stand-in past the real slots, and the view comes from
    /// `spectator_camera` (eye, direction, field of view) when set.
    pub spectating: bool,
    pub spectator_camera: Option<(Vec3, Vec3, f32)>,
    pub damage_flash: f32,
    pub trauma: f32,
    /// World time of the local player's last body-check sound (bump.rs).
    pub bump_heard: f32,
    /// Football: enemy pairs that hit lately, and when (`bump`).
    pub(crate) recent_hits: Vec<(usize, usize, f32)>,
    pub kills: u32,
    pub deaths: u32,
    pub events: String,
    /// Server: what's said to one networked player (by slot) that their own
    /// game can't work out (it doesn't run that part), for the server to
    /// send them as a notice (`World::tell`).
    pub notices: Vec<(usize, String)>,
    pub spatial_sounds: Vec<(&'static str, Vec3)>,
    pub time: f32,
    pub map: MapId,
    pub net_camera_offset: Vec3,
    /// Client only: the local player's position before the latest fixed
    /// step, and this frame's render offset from the current position (see
    /// `World::interpolate_local`). The camera and the local body are drawn
    /// between steps so motion is smooth at any frame rate.
    pub interp_from: Vec3,
    pub render_offset: Vec3,
    /// Client only, offline (where the fixed steps move every body): each
    /// player's position before the latest step, and this frame's offset
    /// back toward it, so the others are drawn between steps like you are
    /// (`World::drawn_offset`). Online these stay empty: the other players
    /// arrive interpolated from snapshots.
    pub drawn_from: Vec<Vec3>,
    pub drawn_offsets: Vec<Vec3>,
    /// Client only: fixed steps run in the latest frame (frame-timing overlay).
    pub steps_last_frame: u8,
    pub blast_serial: u64,
    /// Game mode rules in force (CTF or Capture & Hold).
    pub mode: crate::map_catalog::SupportedMode,
    /// Control points, with state. Sent whole in snapshots.
    pub points: Vec<crate::control::Point>,
    control_defs: Vec<crate::control::Definition>,
    /// Football: the ball and its play state. Inactive in other modes.
    pub ball: football::Ball,
    /// Rabbit and Team Rabbit: the neutral flag. Inactive in other modes.
    pub rabbit: rabbit::RabbitFlag,
    /// A duel server's arena (`duel`): this round's spots and its end.
    /// Server and offline state, never sent.
    pub duel: Option<duel::Duel>,
    /// Deployed turrets, walls and force fields. Sent whole in snapshots.
    pub deployables: Vec<deploy::Deployable>,
    /// This round's time of day, weather and twist (deathmatch modes only).
    pub conditions: crate::conditions::Conditions,
    /// A server's time of day and weather, each as the mode does, random or
    /// fixed (`roll_conditions`).
    pub forecast: crate::conditions::Forecast,
    /// Ammo packs dropped by the dead. Sent whole in snapshots.
    pub loot: Vec<loadout::Loot>,
    /// What each team has used up this round, against entries'
    /// `match_limit` (server and offline state, never sent).
    pub uses: std::collections::HashMap<(deploy::Used, u8, u8), u16>,
    /// Marks beams left (`firing`), server and offline state.
    pub marks: Vec<firing::Mark>,
    /// Touches waiting out their cooldown: toucher, touched, seconds left
    /// (`packs`), server and offline state.
    pub touches: Vec<(u8, u8, f32)>,
    /// Local: the alarm sensors (by index) an enemy was within last tick.
    alarmed: u64,
    /// Whether any armor names bot roles (`registry::Armor::bot_roles`).
    bot_armors: bool,
    /// Who the damage being dealt now is from (for hooks' `on_damage`).
    pub(super) hit_by: Option<usize>,
    /// Each team's waypoint, set at a command station (`placed`).
    pub waypoints: [Option<Vec3>; 2],
    /// Each team's credits, where the mode's economy is shared (`items`).
    pub credits: [f32; 2],
    /// Physics steps so far (the server's tick; `features` moves by it).
    pub steps: u64,
    /// The map's moveables (`features`): how open each is (in snapshots),
    /// and where each is this step and was last step.
    pub moveable_open: Vec<f32>,
    pub moveable_at: Vec<Vec3>,
    pub moveable_was: Vec<Vec3>,
    /// The map's items: seconds until each is back (0: there).
    pub map_item_clocks: Vec<f32>,
    /// Vehicles in play (`vehicles`). Sent whole in snapshots.
    pub vehicles: Vec<vehicles::Craft>,
    /// Client only: looking through a team camera or turret from a command
    /// station: the eye.
    pub view_from: Option<Vec3>,
    /// Local only: seconds until an empty weapon may click again.
    dry_clock: f32,
    /// Extra water volumes beside the map's own: QA staging (`QA_WATER`) and
    /// tests. Empty in normal play.
    pub staged_water: Vec<crate::water::Volume>,
    cnh_acc: [f32; 2],
    network_inputs: Vec<Input>,
    predicting: bool,
    /// A scripted mode's rule set (`rules::scripted`); None plays the
    /// mode's built-in rules.
    pub(crate) rules: Option<Box<dyn rules::RuleSet + Send>>,
    /// Definitions' and the mode's hooks (`hooks`), server only.
    pub(crate) hooks: hooks::HookSet,
    /// Server only: recent player positions and this tick's claimed views,
    /// for laser shots (`lagcomp`). Empty offline and on clients.
    pub lag: lagcomp::History,
    /// Football with bots: everyone as they looked over the last quarter
    /// second, oldest first, for what the bots see (`football_bots`).
    fb_glimpses: std::collections::VecDeque<(f32, Vec<football_bots::Glimpse>)>,
    rng: u32,
    last_spawn: [usize; 2],
    /// Offline bot mix; see `crate::bot_nav::Difficulty`.
    pub bot_difficulty: crate::bot_nav::Difficulty,
    /// Route searches left this tick, so bots re-planning together can't
    /// stall one frame.
    bot_plans: u32,
    /// Seconds before this round's play begins (0: playing). Everyone, bots
    /// included, is held still and the match clock waits. Offline matches
    /// count it down themselves; a server sets it from its Countdown phase.
    pub countdown: f32,
}

struct Rng(u32);
impl Rng {
    fn f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 16_777_216.0
    }
}

/// The third-person camera looks over the right shoulder: this far behind
/// the eye along the look, above it and to its right (horizontally), so the
/// body sits left of center and the crosshair sees past the shoulder.
pub const THIRD_PERSON_BACK: f32 = 2.5;
pub const THIRD_PERSON_UP: f32 = 0.35;
pub const THIRD_PERSON_RIGHT: f32 = 0.8;

pub fn camera_fov(horiz_speed: f32) -> f32 {
    // Walking/short strafe taps never change the lens. Ease into the skiing
    // boost with zero slope at both ends, keeping camera and muzzle in sync.
    let t = ((horiz_speed - 20.0) / 100.0).clamp(0.0, 1.0);
    76.0 + 12.0 * t * t * (3.0 - 2.0 * t)
}

/// Camera-space muzzle, widened to the world field of view so it sits on the gun.
pub fn muzzle_origin(eye: Vec3, forward: Vec3, weapon: u8, fov_deg: f32) -> Vec3 {
    let anchor = if weapon == 0 { VM_ANCHOR_DISC } else { VM_ANCHOR_BOLT };
    muzzle_at(eye, forward, anchor, weapon, vm_muzzle(weapon), fov_deg)
}

/// Where a shot from weapon `w` leaves: its own `muzzle` where it has one
/// (its model's barrel end, marker `muzzle2`), else its view model's.
pub fn muzzle_of(eye: Vec3, forward: Vec3, w: &crate::registry::Weapon, fov_deg: f32) -> Vec3 {
    let view = w.view.kind();
    muzzle_at(eye, forward, vm_anchor(w.view), view, w.muzzle.map_or(vm_muzzle(view), Vec3::from), fov_deg)
}

/// `muzzle_of` the weapon with wire number `weapon` (a disc launcher's
/// muzzle for a number with no entry).
pub fn weapon_muzzle(eye: Vec3, forward: Vec3, weapon: u8, fov_deg: f32) -> Vec3 {
    match crate::registry::registry().weapons.get(weapon as usize) {
        Some(w) => muzzle_of(eye, forward, w, fov_deg),
        None => muzzle_origin(eye, forward, 0, fov_deg),
    }
}

/// A point `local` in view model `view`'s turned frame, from its `anchor`,
/// in the world: where first person draws it at `fov_deg`.
fn muzzle_at(eye: Vec3, forward: Vec3, anchor: Vec3, view: u8, local: Vec3, fov_deg: f32) -> Vec3 {
    let (yaw, pitch) = vm_turn(view);
    let turned = Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(pitch);
    let cam = anchor + turned.transform_point3(local);
    let widen = (fov_deg * 0.5).to_radians().tan() / (VM_FOV * 0.5).to_radians().tan();
    let (right, up) = view_basis(forward);
    eye + right * cam.x * widen + up * cam.y * widen + forward * (-cam.z)
}

fn view_basis(forward: Vec3) -> (Vec3, Vec3) {
    let mut right = forward.cross(Vec3::Y);
    if right.length_squared() < 1.0e-6 {
        right = forward.cross(Vec3::Z);
    }
    let right = right.normalize();
    let up = right.cross(forward).normalize();
    (right, up)
}

fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    let cp = pitch.cos();
    Vec3::new(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp)
}

fn move_basis(yaw: f32) -> (Vec3, Vec3) {
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    (forward, right)
}

impl World {
    pub fn new() -> Self {
        let pillars = pillars();
        let ember_h = height(EMBER_HOME.x, EMBER_HOME.z);
        let glac_h = height(GLACIER_HOME.x, GLACIER_HOME.z);
        Self {
            equipment: Vec::new(),
            players: Vec::new(),
            discs: Vec::new(),
            explosions: Vec::new(),
            smoke: Vec::new(),
            flags: [
                Flag {
                    team: Team::Ember,
                    pos: Vec3::new(EMBER_HOME.x, ember_h + 0.2, EMBER_HOME.z),
                    home: Vec3::new(EMBER_HOME.x, ember_h + 0.2, EMBER_HOME.z),
                    carrier: None,
                    drop_timer: 0.0,
                    vel: Vec3::ZERO, tossed_by: None, toss_guard: 0.0,
                },
                Flag {
                    team: Team::Glacier,
                    pos: Vec3::new(GLACIER_HOME.x, glac_h + 0.2, GLACIER_HOME.z),
                    home: Vec3::new(GLACIER_HOME.x, glac_h + 0.2, GLACIER_HOME.z),
                    carrier: None,
                    drop_timer: 0.0,
                    vel: Vec3::ZERO, tossed_by: None, toss_guard: 0.0,
                },
            ],
            pillars,
            score: [0, 0],
            time_left: crate::map_catalog::SupportedMode::Ctf.def().clock,
            state: MatchState::Flyby,
            acc: 0.0,
            flyby: 0.0,
            player_id: 0,
            input: Input::default(),
            message: String::new(),
            message_t: 0.0,
            hitmarker: 0.0,
            placing: false,
            deploy_ghost: None,
            zoomed: false,
            zoom_level: 0,
            third_person: false,
            spectating: false,
            spectator_camera: None,
            damage_flash: 0.0,
            trauma: 0.0,
            bump_heard: f32::NEG_INFINITY,
            recent_hits: Vec::new(),
            kills: 0,
            deaths: 0,
            events: String::new(),
            notices: Vec::new(),
            spatial_sounds: Vec::new(),
            time: 0.0,
            map: MapId::Valley,
            net_camera_offset: Vec3::ZERO,
            interp_from: Vec3::ZERO,
            render_offset: Vec3::ZERO,
            drawn_from: Vec::new(),
            drawn_offsets: Vec::new(),
            steps_last_frame: 0,
            blast_serial: 0,
            mode: crate::map_catalog::SupportedMode::Ctf,
            points: Vec::new(),
            control_defs: Vec::new(),
            ball: football::Ball::default(),
            rabbit: rabbit::RabbitFlag::default(),
            duel: None,
            deployables: Vec::new(),
            conditions: crate::conditions::Conditions::default(),
            forecast: Default::default(),
            loot: Vec::new(),
            uses: Default::default(),
            marks: Vec::new(),
            touches: Vec::new(),
            alarmed: 0,
            bot_armors: false,
            hit_by: None,
            waypoints: [None; 2],
            credits: [0.0; 2],
            steps: 0,
            moveable_open: Vec::new(),
            moveable_at: Vec::new(),
            moveable_was: Vec::new(),
            map_item_clocks: Vec::new(),
            vehicles: Vec::new(),
            view_from: None,
            dry_clock: 0.0,
            staged_water: crate::water::qa_staged(),
            cnh_acc: [0.0; 2],
            network_inputs: Vec::new(),
            predicting: false,
            rules: None,
            hooks: hooks::HookSet::default(),
            lag: lagcomp::History::default(),
            fb_glimpses: Default::default(),
            feed: Vec::new(),
            rng: 0xC0FFEE,
            last_spawn: [usize::MAX; 2],
            bot_difficulty: crate::bot_nav::Difficulty::default(),
            bot_plans: 0,
            countdown: 0.0,
        }
    }

    fn rng(&mut self) -> f32 {
        let mut r = Rng(self.rng);
        let v = r.f32();
        self.rng = r.0;
        v
    }

    pub fn set_map(&mut self, id: MapId) {
        self.map = id;
        self.equipment=crate::equipment::fresh(id);
        self.pillars = crate::terrain::pillars_on(id);
        self.control_defs = crate::map_pack::on(id).map(|p| p.manifest.control_points.clone()).unwrap_or_default();
        self.moveable_open.clear();
        self.moveable_at.clear();
        self.moveable_was.clear();
        self.map_item_clocks.clear();
        self.vehicles.clear();
        self.reset_points();
        if self.players.is_empty() {
            self.place_flags();
        }
    }

    /// Fresh, neutral points for the current map and mode. CTF only runs the
    /// points marked `ctf_active`; Capture & Hold runs them all.
    pub fn reset_points(&mut self) {
        let run = self.mode.def().control_points;
        self.points = self.control_defs.iter()
            .map(|d| crate::control::Point::from_def(d, match run {
                crate::modes::Points::All => true,
                crate::modes::Points::CtfActive => d.ctf_active,
                crate::modes::Points::None => false,
            })).collect();
        self.cnh_acc = [0.0; 2];
        self.reset_ball();
        self.reset_rabbit();
    }

    pub fn set_mode(&mut self, mode: crate::map_catalog::SupportedMode) {
        self.mode = mode;
        // A scripted mode's rules, fresh for the round (servers only).
        self.rules = rules::scripted(mode);
        self.hooks.reset(mode);
        self.bot_armors = crate::registry::registry().armors.iter().any(|(_, _, a)| !a.bot_roles.is_empty());
        // Each mode has its own clock (Football plays two 15-minute halves).
        self.time_left = self.match_time();
        self.reset_points();
        // Bots go by football names in Football (`football_bots::bot_name`).
        for i in 0..self.players.len() {
            if self.players[i].is_bot && !self.players[i].remote && !self.players[i].name.is_empty() {
                self.players[i].name = self.bot_name(i);
            }
        }
    }

    /// Replace the map's control points (tests and QA fixtures only use this;
    /// real maps declare them in the manifest).
    pub fn set_control_points(&mut self, defs: Vec<crate::control::Definition>) {
        self.control_defs = defs;
        self.reset_points();
    }

    pub fn control_point_count(&self) -> usize { self.control_defs.len() }

    /// The control point drain on player `i` (`control::drain_from`): off
    /// the ground for an airborne field, and seen by a spire for a point
    /// with spires (nothing solid between its tip and their chest).
    pub fn point_drain(&self, i: usize) -> Option<crate::control::Drained> {
        let p = self.players.get(i).filter(|p| p.alive)?;
        if self.points.iter().all(|q| q.drain_rate <= 0.0 || q.owner.is_none()) { return None; }
        let chest = p.pos + Vec3::Y * 0.4;
        crate::control::drain_from(&self.points, p.team.idx() as u8, p.pos, !p.on_ground,
            |tip| obstacle_hit(self.map, &self.pillars, tip, chest, 0.0).is_none())
    }

    /// Health per second a point burns living player `i` with: their team
    /// holds it and they stand in its cook box (`control::Point::cooks`).
    pub fn point_cook(&self, i: usize) -> Option<(usize, f32)> {
        let p = self.players.get(i).filter(|p| p.alive && p.riding == 0)?;
        self.points.iter().enumerate().find_map(|(k, q)| q.cooks(p.team.idx() as u8, p.pos).map(|d| (k, d)))
    }

    fn ground(&self, x: f32, z: f32) -> f32 {
        crate::terrain::height_on(self.map, x, z)
    }

    fn map_size(&self) -> f32 {
        crate::terrain::info(self.map).size
    }

    fn stand(&self, ember: bool) -> Vec3 {
        crate::terrain::spawn_on(self.map, ember)
    }

    /// Server-chosen spawn among a map's authored points: random, avoiding
    /// the point this team used last and points with a live enemy nearby when
    /// any other choice exists. `None` keeps the legacy single-spawn path.
    /// Football's second half uses the other end's points (`side`).
    fn pick_spawn(&mut self, team: Team) -> Option<(Vec3, f32)> {
        if self.ffa() { return self.ffa_spawn(usize::MAX); }
        let points = crate::terrain::spawn_points_on(self.map, self.side(team) == Team::Ember);
        if points.is_empty() { return None; }
        let last = self.last_spawn[team.idx()];
        let safe = |p: Vec3, players: &[Player]| players.iter()
            .all(|o| !o.alive || o.team == team || (o.pos - p).length() > 30.0);
        let mut pool: Vec<usize> = (0..points.len())
            .filter(|&i| i != last && safe(points[i].0, &self.players)).collect();
        if pool.is_empty() { pool = (0..points.len()).filter(|&i| i != last).collect(); }
        if pool.is_empty() { pool = vec![0]; }
        let pick = pool[((self.rng() * pool.len() as f32) as usize).min(pool.len() - 1)];
        self.last_spawn[team.idx()] = pick;
        Some(points[pick])
    }

    pub fn set_paused(&mut self, paused: bool) {
        match (self.state, paused) {
            (MatchState::Playing, true) => self.state = MatchState::Paused,
            (MatchState::Paused, false) => self.state = MatchState::Playing,
            _ => {}
        }
    }

    pub fn start_match(&mut self, ember: bool) {
        self.feed.clear();
        self.equipment=crate::equipment::fresh(self.map);
        self.network_inputs.clear();
        self.net_camera_offset = Vec3::ZERO;
        let team = if ember { Team::Ember } else { Team::Glacier };
        self.players.clear();
        self.discs.clear();
        self.deployables.clear();
        self.uses.clear();
        self.loot.clear();
        self.waypoints = [None; 2];
        self.moveable_open.clear();
        self.moveable_at.clear();
        self.moveable_was.clear();
        self.vehicles.clear();
        self.place_map_deployables();
        self.explosions.clear();
        self.smoke.clear();
        self.score = [0, 0];
        self.kills = 0;
        self.deaths = 0;
        self.time_left = self.match_time();
        self.state = MatchState::Playing;
        self.countdown = 0.0;
        self.acc = 0.0;
        self.trauma = 0.0;
        self.message = self.mode.def().start_message.clone();
        self.reset_points();
        self.roll_conditions();
        if self.conditions != crate::conditions::Conditions::default() { self.message = format!("{} · {}", self.message, self.conditions.describe().to_uppercase()); }
        self.message_t = 3.2;
        self.events = "start".into();

        // Face the valley, not the rim. Ember sits at low Z.
        let yaw = if ember { std::f32::consts::PI } else { 0.0 };
        self.last_spawn = [usize::MAX; 2];
        let (pos, face) = self.pick_spawn(team).unwrap_or((self.stand(ember), yaw));
        self.players.push(make_player(team, false, pos, face, BotRole::Offense));
        self.player_id = 0;
        self.fill_match(ember, team, yaw);
        self.reset_credits();
    }

    /// The offline match the menu starts: `start_match`, then everyone held
    /// at their spawns for ROUND_COUNTDOWN seconds before play.
    pub fn start_counted_match(&mut self, ember: bool) {
        self.start_match(ember);
        self.countdown = ROUND_COUNTDOWN;
    }

    /// A joined rift: you on the snow, no local bots. Other skiers come from the server.
    pub fn start_rift(&mut self, ember: bool) {
        self.feed.clear();
        self.equipment=crate::equipment::fresh(self.map);
        let team = if ember { Team::Ember } else { Team::Glacier };
        self.players.clear();
        self.discs.clear();
        self.deployables.clear();
        self.uses.clear();
        self.loot.clear();
        self.explosions.clear();
        self.smoke.clear();
        self.score = [0, 0];
        self.kills = 0;
        self.deaths = 0;
        self.time_left = self.match_time();
        self.state = MatchState::Playing;
        self.countdown = 0.0;
        self.acc = 0.0;
        self.trauma = 0.0;
        self.message = "IN THE RIFT".into();
        self.reset_points();
        self.message_t = 2.4;
        self.events = "start".into();
        let yaw = if ember { std::f32::consts::PI } else { 0.0 };
        self.last_spawn = [usize::MAX; 2];
        let (pos, face) = self.pick_spawn(team).unwrap_or((self.stand(ember), yaw));
        self.players.push(make_player(team, false, pos, face, BotRole::Offense));
        self.player_id = 0;
        self.place_flags();
    }

    fn fill_match(&mut self, ember: bool, team: Team, yaw: f32) {
        for i in 0..2 {
            let o = Vec3::new((i as f32 - 0.5) * 6.0, 0.0, 4.0);
            let mut p = self.stand(ember) + o;
            p.y = self.ground(p.x, p.z) + 1.2;
            let (p, face) = self.pick_spawn(team).unwrap_or((p, yaw));
            let role = if i == 0 { BotRole::Offense } else { BotRole::Defense };
            self.players.push(make_player(team, true, p, face, role));
        }
        let other = team.other();
        let other_ember = other == Team::Ember;
        let oyaw = if other_ember { std::f32::consts::PI } else { 0.0 };
        for i in 0..3 {
            let o = Vec3::new((i as f32 - 1.0) * 5.5, 0.0, 3.0);
            let mut p = self.stand(other_ember) + o;
            p.y = self.ground(p.x, p.z) + 1.2;
            let (p, face) = self.pick_spawn(other).unwrap_or((p, oyaw));
            let role = if i == 2 { BotRole::Defense } else { BotRole::Offense };
            self.players.push(make_player(other, true, p, face, role));
        }

        for i in 0..self.players.len() {
            if self.players[i].is_bot { self.assign_personality(i); }
        }
        self.place_flags();
    }

    /// Give bot `i` a personality from the difficulty mix, a matching role
    /// and a readable name ("Rider", "Ace 2").
    fn assign_personality(&mut self, i: usize) {
        let profile = self.bot_difficulty.pick(self.rng());
        self.players[i].bot_profile = profile;
        self.players[i].name = self.bot_name(i);
        let p = &mut self.players[i];
        match profile.name {
            "Anchor" => p.bot_role = BotRole::Defense,
            "Skirmisher" | "Ace" if p.bot_role != BotRole::Defense => p.bot_role = BotRole::Hunter,
            _ => {}
        }
    }

    /// Offline bot mix for the next match.
    pub fn set_bot_difficulty(&mut self, difficulty: crate::bot_nav::Difficulty) {
        self.bot_difficulty = difficulty;
    }

    fn place_flags(&mut self) {
        let ember = crate::terrain::info(self.map).ember;
        let glacier = crate::terrain::info(self.map).glacier;
        let eh = self.ground(ember.x, ember.z);
        let gh = self.ground(glacier.x, glacier.z);
        self.flags[0] = Flag {
            team: Team::Ember,
            pos: Vec3::new(ember.x, eh + 0.2, ember.z),
            home: Vec3::new(ember.x, eh + 0.2, ember.z),
            carrier: None,
            drop_timer: 0.0,
            vel: Vec3::ZERO, tossed_by: None, toss_guard: 0.0,
        };
        self.flags[1] = Flag {
            team: Team::Glacier,
            pos: Vec3::new(glacier.x, gh + 0.2, glacier.z),
            home: Vec3::new(glacier.x, gh + 0.2, glacier.z),
            carrier: None,
            drop_timer: 0.0,
            vel: Vec3::ZERO, tossed_by: None, toss_guard: 0.0,
        };
        if let Some(pack)=crate::map_pack::on(self.map) {
            for (flag,source) in self.flags.iter_mut().zip(pack.manifest.flags) {
                flag.pos=Vec3::from_array(source);flag.home=flag.pos;
            }
        }
    }

    pub fn add_look(&mut self, dx: f32, dy: f32) {
        if self.state != MatchState::Playing || self.players.is_empty() {
            return;
        }
        // Zoomed, the view turns in proportion to the narrower lens.
        let sens = if self.zoom_active() { SENS*self.zoom_fov()/76.0 } else { SENS };
        let limit = self.look_limit();
        let p = &mut self.players[self.player_id];
        if !p.alive {
            return;
        }
        p.yaw -= dx * sens;
        p.pitch = (p.pitch - dy * sens).clamp(-limit, limit);
    }

    /// Whether the local player is looking through a rifle's zoom.
    pub fn zoom_active(&self) -> bool {
        self.zoomed && self.players.get(self.player_id).is_some_and(|p| p.alive
            && crate::registry::registry().weapons.get(p.weapon as usize).is_some_and(|w| w.zoom))
    }

    /// The sight's field of view (degrees) at the local player's zoom level.
    pub fn zoom_fov(&self) -> f32 { rifles::ZOOM_FOVS[(self.zoom_level as usize).min(rifles::ZOOM_FOVS.len() - 1)] }

    /// How many times nearer the sight brings things (1 without it): name
    /// tags and markers reach that much farther through it.
    pub fn zoom_power(&self) -> f32 { if self.zoom_active() { rifles::magnification(self.zoom_fov()) } else { 1.0 } }

    #[allow(dead_code)]
    pub fn apply_override(&mut self) {
        let Some(codes) = self.input.keys_override.clone() else {
            return;
        };
        let has = |c: &str| codes.iter().any(|k| k == c);
        let mut mx = 0.0;
        let mut mz = 0.0;
        if has("KeyW") || has("ArrowUp") {
            mz += 1.0;
        }
        if has("KeyS") || has("ArrowDown") {
            mz -= 1.0;
        }
        if has("KeyD") || has("ArrowRight") {
            mx += 1.0;
        }
        if has("KeyA") || has("ArrowLeft") {
            mx -= 1.0;
        }
        self.input.move_x = mx;
        self.input.move_z = mz;
        self.input.jump = has("Space");
        self.input.jet = has("Mouse2");
        if has("Mouse0") || has("KeyF") {
            self.input.fire = true;
        }
        // Number keys pick slots 1, 2 and 3.
        let loadout = self.players.get(self.player_id).map(|p| p.loadout).unwrap_or_default();
        for (key, slot) in [("Digit2", 1), ("Digit1", 0), ("Digit3", 2)] {
            if has(key) { if let Some(k) = loadout.slot(slot) { self.input.weapon = k; } }
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.events.clear();
        self.spatial_sounds.clear();
        self.flyby += dt;
        self.time += dt;
        self.hitmarker = (self.hitmarker - dt * 3.5).max(0.0);
        self.damage_flash = (self.damage_flash - dt * 2.8).max(0.0);
        self.trauma = (self.trauma - dt * 1.6).max(0.0);
        self.message_t = (self.message_t - dt).max(0.0);
        if self.message_t <= 0.0 {
            self.message.clear();
        }

        if self.state == MatchState::Flyby {
            return;
        }
        if self.state != MatchState::Playing {
            return;
        }

        if !self.players.is_empty() {
            let wanted = self.input.weapon;
            let w = self.allowed_weapon(self.player_id, wanted);
            self.players[self.player_id].weapon = w;
            let limit = self.look_limit();
            let p = &mut self.players[self.player_id];
            p.yaw -= self.input.look_stick_x * 1.8 * dt;
            p.pitch = (p.pitch - self.input.look_stick_y * 1.4 * dt).clamp(-limit, limit);
        }

        self.acc += dt.min(0.1);
        let mut n = 0;
        while self.acc >= STEP && n < 5 {
            self.mark_step();
            self.drawn_from.clear();
            self.drawn_from.extend(self.players.iter().map(|p| p.pos));
            self.physics_step();
            self.input.clear_once();
            self.acc -= STEP;
            n += 1;
        }
        self.steps_last_frame = n as u8;
        self.interpolate_local(self.acc / STEP);
        self.interpolate_others(self.acc / STEP);
    }

    /// Before a fixed step: remember where the local player was.
    pub fn mark_step(&mut self) {
        if let Some(p) = self.players.get(self.player_id) { self.interp_from = p.pos; }
    }

    /// After this frame's steps, with `alpha` (0..1) the fraction of the next
    /// step already elapsed: draw the local player that far from the previous
    /// step's position toward the current one (`between_steps`). The
    /// others' offsets are cleared: online they come from snapshots, and
    /// offline `interpolate_others` follows.
    pub fn interpolate_local(&mut self, alpha: f32) {
        self.drawn_offsets.clear();
        let Some(p) = self.players.get(self.player_id) else { self.render_offset = Vec3::ZERO; return; };
        self.render_offset = between_steps(p, self.interp_from, alpha);
    }

    /// Offline, after `interpolate_local`: the other players' offsets the
    /// same way, from where the latest step found them (`drawn_from`).
    pub fn interpolate_others(&mut self, alpha: f32) {
        self.drawn_offsets.clear();
        let from = &self.drawn_from;
        self.drawn_offsets.extend(self.players.iter().enumerate().map(|(i, p)| match from.get(i) {
            Some(&was) if i != self.player_id => between_steps(p, was, alpha),
            _ => Vec3::ZERO,
        }));
    }

    /// How far from its position player `i`'s body is drawn this frame:
    /// between the fixed steps, toward where the latest one found it.
    pub fn drawn_offset(&self, i: usize) -> Vec3 {
        if i == self.player_id { self.render_offset } else { self.drawn_offsets.get(i).copied().unwrap_or(Vec3::ZERO) }
    }

    fn physics_step(&mut self) {
        let dt = STEP;
        self.steps += 1;
        self.step_hooks(dt);
        // The map's moving parts first: bodies meet them where they are now.
        self.step_moveables(dt);
        // Before a round: everyone settles at their spawn, held still; the
        // clock, objectives, turrets and purchases all wait.
        if self.countdown > 0.0 {
            let before: Vec<Vec3> = self.players.iter().map(|p| p.pos).collect();
            self.step_players(dt);
            self.step_bumps(&before);
            self.countdown = (self.countdown - dt).max(0.0);
            return;
        }
        self.time_left -= dt;
        if self.time_left <= 0.0 {
            self.time_left = 0.0;
            self.rule_event(rules::GameEvent::ClockExpired);
            // The rule set ends the match; time can't run on regardless.
            self.state = MatchState::Ended;
            return;
        }

        self.check_halftime();
        if self.football() { self.fb_watch(); }
        self.think_bots(dt);
        let before: Vec<Vec3> = self.players.iter().map(|p| p.pos).collect();
        self.step_players(dt);
        self.step_bumps(&before);
        self.step_discs(dt);
        self.step_marks(dt);
        self.step_status(dt);
        self.step_vehicles(dt);
        self.step_boarding();
        self.step_touches(dt);
        self.step_mounts(dt);
        self.step_placed(dt);
        self.step_waypoints();
        self.step_drops();
        self.step_economy(dt);
        self.step_vents();
        self.step_features(dt);
        self.step_alarms();
        self.step_repair(dt);
        self.step_purchases();
        self.step_deploys();
        self.step_throws(dt);
        self.step_tosses();
        self.step_deployables(dt);
        self.step_point_turrets(dt);
        self.step_ammo_stations();
        self.step_loot(dt);
        self.step_suicides();
        // Football has no weapons, and in Deathmatch turrets can't take a
        // side: turrets and sensors stand idle.
        if self.mode.def().equipment { self.step_equipment(dt); }
        if self.mode.rules() == crate::modes::Rules::Ctf { self.step_flags(&before, dt); }
        if self.rabbit_mode() { self.step_rabbit(&before, dt); }
        self.step_ball(dt);
        self.step_points(dt);
        if self.state == MatchState::Ended { return; }
        self.step_explosions(dt);
    }

    /// Control points: capture progress and ownership, and in Capture & Hold
    /// the score. Clients announce ownership changes by diffing snapshots, so
    /// replayed snapshots never re-announce.
    fn step_points(&mut self, dt: f32) {
        if self.predicting || self.points.iter().all(|p| !p.active) { return; }
        let players: Vec<(u8, Vec3)> = self.players.iter().filter(|p| p.alive)
            .map(|p| (p.team.idx() as u8, p.pos)).collect();
        for e in crate::control::step(&mut self.points, &players, dt) {
            match e {
                crate::control::Event::Captured { .. } => self.push_event("point"),
                crate::control::Event::ContestedStart { .. } => self.push_event("contest"),
            }
        }
        // A drain that hurts as well (`control::Drain::damage`, spire
        // lightning): energy damage to whoever it drains, riders aside.
        if self.points.iter().any(|p| p.active && p.drain_damage > 0.0 && p.owner.is_some()) {
            for i in 0..self.players.len() {
                if self.players[i].riding > 0 { continue; }
                let Some(d) = self.point_drain(i).filter(|d| d.damage > 0.0) else { continue };
                let cause = if d.spire.is_some() { "Lightning" } else { "Drain" };
                self.damage(i, d.damage * dt, "energy", None, cause);
            }
        }
        // A point whose hall burns its holders (`control::Cook`): one team
        // can't keep it from inside, so the enemy is welcome there.
        if self.points.iter().any(|p| p.active && p.cook_damage > 0.0 && p.owner.is_some()) {
            for i in 0..self.players.len() {
                let Some((_, burn)) = self.point_cook(i) else { continue };
                self.damage(i, burn * dt, "fire", None, "Heat");
            }
        }
        if self.mode.rules() != crate::modes::Rules::CaptureAndHold { return; }
        let gained = crate::control::score(&self.points, &mut self.cnh_acc, dt);
        if gained != [0, 0] { self.rule_event(rules::GameEvent::ZonePoints { gained }); }
    }

    /// Ctrl+K: a living player who asks to die does, like any other death
    /// (a carried flag or ball drops). Never predicted; the server applies it.
    fn step_suicides(&mut self) {
        if self.predicting { return; }
        let delay = self.mode.def().suicide_delay;
        let started = self.state == MatchState::Playing && self.countdown <= 0.0;
        for i in 0..self.players.len() {
            let wants = if self.network_inputs.is_empty() { i == self.player_id && !self.players[i].is_bot && self.input.suicide }
                else { self.network_inputs.get(i).is_some_and(|c| c.suicide) };
            let p = &mut self.players[i];
            if !p.alive { p.suicide_in = 0.0; continue; }
            if delay > 0.0 {
                // A delayed Ctrl+K (Football: 15 s) counts only once the
                // match is on, so it's no escape from a tackle.
                if wants && started && p.suicide_in <= 0.0 { p.suicide_in = delay; }
                if p.suicide_in > 0.0 {
                    p.suicide_in -= STEP;
                    if p.suicide_in <= 0.0 { p.suicide_in = 0.0; self.kill(i, Some(i), "Suicide"); }
                }
            } else if wants { self.kill(i, Some(i), "Suicide"); }
        }
    }

    fn step_equipment(&mut self, dt:f32) {
        use crate::equipment::{self,Kind};
        if self.predicting {return;}
        // Free-for-all: every station serves everyone and turrets stay idle.
        let ffa=self.ffa();
        let defs=equipment::definitions(self.map);
        equipment::power(defs,&mut self.equipment);
        let mut sensed:Vec<_>=defs.iter().zip(&self.equipment)
            .filter(|(d,s)|d.kind==Kind::Sensor && s.powered).map(|(d,_)|d.team).collect();
        // A team's marks (`firing`) reach as far as a powered sensor.
        sensed.extend(self.marks.iter().map(|m| m.team.idx() as u8));
        // Players sensors and turrets can't pick out (an effect hides them).
        let hidden: Vec<bool> = (0..self.players.len()).map(|j| self.sensor_hidden(j)).collect();
        for (i,(d,s)) in defs.iter().zip(&mut self.equipment).enumerate() {
            s.cooldown=(s.cooldown-dt).max(0.);s.contacts=0;
            for (pid,p) in self.players.iter_mut().enumerate() {
                // Do not cast map-wide service rays for distant players. These
                // proximity rejects are also part of the server trust boundary.
                if !p.alive || (!ffa && p.team.idx()!=d.team as usize)
                    || p.pos.distance_squared(d.pos())>(d.radius+1.5).powi(2) {continue;}
                // Bots restock just by reaching their inventory station.
                let input=if p.is_bot {true} else if self.network_inputs.is_empty() {
                    if pid==self.player_id {self.input.interact} else {false}
                } else {self.network_inputs.get(pid).is_some_and(|c|c.interact)};
                let end=d.pos();let start=p.pos+Vec3::Y*0.7;
                let delta=end-start;
                // Station interaction points are in free space; repairable
                // solids use their near surface. Buildings still occlude use.
                let trim=if matches!(d.kind,Kind::Generator|Kind::Turret|Kind::Sensor) {d.radius} else {0.};
                let end=end-delta.normalize_or_zero()*trim.min(delta.length());
                if obstacle_hit(self.map,&self.pillars,start,end,0.).is_none() {
                    equipment::service(d,s,p,input,dt);
                }
            }
            if !s.powered || ffa {continue;}
            let Some(profile)=equipment::profile(d.kind,d.weapon) else {continue};
            let (map,pillars)=(self.map,&self.pillars);
            let clear=|a:Vec3,b:Vec3|obstacle_hit(map,pillars,a,b,0.).is_none();
            // The field of fire keeps a doorway turret from shooting back into
            // its own rooms; anyone under open sky (a roof, a ramp, the ground
            // behind it) is fair game all round.
            let reach=profile.sensed_range.max(profile.range)+d.radius;
            let candidates=self.players.iter().enumerate().filter(|(j,p)|p.alive && !hidden[*j] && p.pos.distance(d.pos())<=reach
                    && (d.in_arc(p.pos) || under_open_sky(map,pillars,p.pos)))
                .map(|(index,p)|equipment::Candidate {index,team:p.team.idx() as u8,pos:p.pos,vel:p.vel,jetting:p.jetting});
            let Some(hit)=equipment::acquire_target(d.pos(),d.radius,d.team,&profile,
                sensed.contains(&d.team),candidates,clear) else {continue};
            s.contacts=1;s.aim=hit.aim;
            if profile.fires && s.cooldown<=0. && self.discs.len()<256 {
                if !clear(hit.muzzle,hit.aim_point) {continue;}
                // A homing round (a rocket turret's) follows the one it was fired at.
                let homing=crate::registry::registry().projectiles.get(profile.projectile as usize).is_some_and(|r|r.seek.is_some());
                self.discs.push(Disc {pos:hit.muzzle,vel:hit.aim*profile.speed,
                    team:if d.team==0 {Team::Ember} else {Team::Glacier},owner:MAX_PLAYERS,
                    life:profile.life,kind:profile.projectile,spin:i as f32,
                    target: if homing {hit.index as u16+1} else {0}, power: 1.0 });
                s.cooldown=profile.cooldown;
            }
        }
        equipment::power(defs,&mut self.equipment);
        for (d,s) in defs.iter().zip(&mut self.equipment) {s.regen(d,dt);}
    }

    /// Clear sight between two points through terrain, pillars and the map
    /// collision mesh. Client overlays use it cosmetically; positions already
    /// arrive in snapshots, so it reveals nothing new.
    /// How far a body-width ray from `from` toward `to` gets before the map
    /// blocks it (m). Client-side cosmetic use: where a fallen body can lie.
    pub fn clear_distance(&self, from: Vec3, to: Vec3) -> f32 {
        let len = from.distance(to);
        obstacle_hit(self.map, &self.pillars, from, to, 0.25).map_or(len, |t| t * len)
    }

    pub fn sight_clear(&self, from:Vec3, to:Vec3)->bool {
        obstacle_hit(self.map,&self.pillars,from,to,0.).is_none()
    }

    pub fn equipment_prompt(&self)->Option<String> {
        use crate::equipment::Kind;
        let p=self.players.get(self.player_id)?;
        if !p.alive {return None;}
        crate::equipment::definitions(self.map).iter().zip(&self.equipment)
            .filter(|(d,_)|(self.ffa() || d.team as usize==p.team.idx()) && p.pos.distance(d.pos())<d.radius+1.5)
            .min_by(|(a,_),(b,_)|p.pos.distance_squared(a.pos()).total_cmp(&p.pos.distance_squared(b.pos())))
            .map(|(d,s)| {
                // "[repair]" and "[use]" are replaced by the bound keys on the HUD.
                if s.health<d.max_health() {format!("Damaged {:?} · {:.0}% · hold [repair] to repair",d.kind,100.*s.health/d.max_health())}
                else if !s.powered {"Power offline — repair the generator ([repair])".into()}
                else if d.kind==Kind::Inventory {"Inventory · press [use] to refit".into()}
                else if d.kind==Kind::Repair {"Repair pad: restoring health and energy".into()}
                else {format!("{:?} ONLINE",d.kind)}
            })
    }

    fn think_bots(&mut self, dt: f32) {
        self.bot_plans = BOT_PLANS_PER_TICK;
        let nav = crate::bot_nav::ready(self.map);
        let n = self.players.len();
        let sparring = self.sparring();
        for i in 0..n {
            if !self.players[i].is_bot || !self.players[i].alive {
                continue;
            }
            // Sparring bots think for themselves (`blade::spar_wish`).
            if sparring || self.blade_zone_of(i).is_some() { continue; }
            self.players[i].bot_think -= dt;
            self.players[i].bot_memory = (self.players[i].bot_memory - dt).max(0.0);
            self.players[i].bot_replan -= dt;
            if self.players[i].bot_think > 0.0 {
                continue;
            }
            let wait = 0.18 + self.rng() * 0.22;
            let jx = (self.rng() - 0.5) * 12.0;
            let jz = (self.rng() - 0.5) * 8.0;
            // Aim error for this think: a rough normal sample, per profile.
            let spread = self.players[i].bot_profile.aim_error;
            let gauss = |w: &mut Self| (w.rng() + w.rng() + w.rng() - 1.5) * 2.0;
            let aim_err = Vec2::new(gauss(self), gauss(self)) * spread;
            self.players[i].bot_aim = aim_err;
            self.players[i].bot_think = wait;
            let profile = self.players[i].bot_profile;
            let team = self.players[i].team;
            let role = self.players[i].bot_role;
            let carrying = self.players[i].carrying;
            let pos = self.players[i].pos;
            let health = self.players[i].health;
            let enemy_flag_pos = self.flags[team.other().idx()].pos;
            let enemy_carrier = self.flags[team.other().idx()].carrier;
            let own_home = self.flags[team.idx()].home;
            let own_carrier = self.flags[team.idx()].carrier;

            // Heal: the repair tool on themselves when hurt and nobody's about;
            // cautious personalities fall back to an inventory station when
            // badly hurt or out of disc and chaingun rounds.
            let quiet = self.players[i].bot_memory <= 0.0;
            // Team Rabbit's rabbit can't mend. Stations don't heal, so a
            // rabbit falls back to one only out of rounds, and only in Rabbit
            // (`rabbit`).
            let rabbit = self.is_rabbit(i);
            let team_rabbit = self.mode.rules() == crate::modes::Rules::TeamRabbit;
            // Football bots mend only while no ball is in play: the repair
            // tool keeps the hands off the ball.
            let playing_ball = self.ball.active && self.ball.in_play;
            self.players[i].bot_repair = profile.uses_kit && health < 70.0 && quiet && !(rabbit && team_rabbit) && !playing_ball
                && self.players[i].energy > crate::defs::repair().min_energy + 20.0;
            let dry = self.players[i].ammo[0] == 0 && self.players[i].ammo[1] == 0;
            let retreat = carrying.is_none() && if rabbit { !team_rabbit && dry } else { health < profile.retreat_health || dry };

            let mut goal = enemy_flag_pos;
            let cnh_goal = if self.mode.rules() == crate::modes::Rules::CaptureAndHold {
                // Minimal Capture & Hold play: defenders guard the nearest held
                // point, everyone else heads for the nearest point not yet theirs.
                // A held point that burns its holders (`control::Cook`) isn't
                // guarded from inside: defenders go to another, or attack.
                let mine = |p: &crate::control::Point| p.owner == Some(team.idx() as u8);
                let pick = |want_mine: bool| self.points.iter().filter(|p| p.active && mine(p) == want_mine && !(want_mine && p.cook_damage > 0.0))
                    .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos))).map(|p| p.pos);
                if role == BotRole::Defense { pick(true).or_else(|| pick(false)) } else { pick(false).or_else(|| pick(true)) }
                    .map(|g| g + Vec3::new(jx * 0.5, 1.2, jz * 0.5))
            } else { None };
            // A post taken at a touch (a point that flips in 2 s or less, like
            // Ashveil's Sanctum): in CTF, each team's first living attacker
            // goes and touches it while the team doesn't hold it. Maps without
            // one play as before.
            let runner = role != BotRole::Defense && (0..n).find(|&j| {
                let o = &self.players[j];
                o.is_bot && o.alive && o.team == team && o.bot_role != BotRole::Defense
            }) == Some(i);
            let post = (runner && self.mode.rules() == crate::modes::Rules::Ctf).then(|| self.points.iter()
                .filter(|p| p.active && p.capture > 0.0 && p.capture <= 2.0 && p.owner != Some(team.idx() as u8))
                .min_by(|a, b| a.pos.distance(pos).total_cmp(&b.pos.distance(pos))).map(|p| p.pos + Vec3::Y * 1.2)).flatten();
            let football = self.football();
            let fb_goal = football.then(|| self.football_think(i, jx, jz));
            let station = retreat.then(|| crate::equipment::definitions(self.map).iter()
                .filter(|d| d.kind == crate::equipment::Kind::Inventory && d.team as usize == team.idx())
                .map(|d| d.pos())
                .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)))).flatten();
            let dm_goal = self.rabbit_goal(i).or_else(|| self.rabbit_flight(i)).or_else(|| self.deathmatch_goal(i));
            if let Some(g) = station {
                goal = g;
            } else if let Some(g) = dm_goal {
                goal = g + Vec3::new(jx * 0.4, 0.0, jz * 0.4);
            } else if let Some(g) = cnh_goal {
                goal = g;
            } else if let Some(g) = fb_goal {
                goal = g;
            } else if carrying.is_some() {
                goal = own_home;
            } else if own_carrier.is_some() && role != BotRole::Offense {
                if let Some(c) = own_carrier {
                    if c < n {
                        goal = self.players[c].pos;
                    }
                }
            } else if role == BotRole::Defense {
                goal = own_home + Vec3::new(jx, 0.0, jz);
            } else if let Some(c) = enemy_carrier {
                if c < n {
                    goal = self.players[c].pos;
                }
            } else if let Some(g) = post {
                goal = g;
            }

            // Sight uses the turrets' line-of-sight rule (eye to chest against
            // terrain, map collision and pillars), testing only the nearest
            // few enemies so each think casts at most BOT_SIGHT_RAYS rays.
            let eye = pos + Vec3::Y * EYE;
            // An enemy cloaked out of sight (`status::UNSEEN`) shows only
            // within its shimmer range, as it would to a person.
            let mut near: Vec<(f32, usize)> = self.players.iter().enumerate()
                .filter(|(j, o)| o.alive && self.hostile(i, team, *j))
                .map(|(j, o)| (pos.distance(o.pos), j))
                .filter(|(d, j)| *d < BOT_SIGHT_RANGE && (self.players[*j].effects.is_empty()
                    || status::visibility(&self.players[*j].effects) >= status::UNSEEN || *d <= status::shimmer(&self.players[*j].effects)))
                .collect();
            near.sort_by(|a, b| a.0.total_cmp(&b.0));
            let map = self.map;
            let seen = near.iter().take(BOT_SIGHT_RAYS).find(|(_, j)| {
                let chest = self.players[*j].pos + Vec3::Y * crate::equipment::CHEST_HEIGHT;
                obstacle_hit(map, &self.pillars, eye, chest, 0.0).is_none()
            }).copied();
            // A mod's hints for its packs and class keys (`registry::BotUse`).
            self.bot_uses(i, seen.is_some(), role == BotRole::Defense && pos.distance(own_home) < 30.0);

            // Hunt what it can see; once sight breaks, head for where the enemy
            // was last seen for a moment instead of tracking it through walls.
            // Aggressive personalities chase farther.
            let chase = 20.0 + 60.0 * profile.aggression;
            let hunts = !football && (self.deathmatch() || (role == BotRole::Hunter || carrying.is_none()) && role != BotRole::Defense) && !retreat;
            let mut direct = false;
            match seen {
                Some((d, j)) => {
                    let same = self.players[i].bot_target == Some(j);
                    self.players[i].bot_seen_for = if same { self.players[i].bot_seen_for + wait } else { 0.0 };
                    self.players[i].bot_target = Some(j);
                    self.players[i].bot_seen = self.players[j].pos;
                    self.players[i].bot_memory = BOT_MEMORY;
                    if hunts && d < chase {
                        goal = self.players[j].pos;
                        // In sight and close: fight it out without a route.
                        direct = d < 28.0;
                    }
                }
                None => {
                    self.players[i].bot_target = None;
                    self.players[i].bot_seen_for = 0.0;
                    let last = self.players[i].bot_seen;
                    if hunts && self.players[i].bot_memory > 0.0 && pos.distance(last) < chase {
                        goal = last;
                    }
                }
            }

            // Football bots run their own plays (`football_inputs`), and
            // follow a route only round something in the way.
            if football { direct = !self.players[i].bot_fb.blocked; }
            self.players[i].bot_goal = goal;
            // Flyers cross high instead of following the ground route, dive on
            // targets below them, and hand back to the route for the landing.
            let flyer = profile.style == crate::bot_nav::MoveStyle::Flyer;
            let flying = self.players[i].bot_fly;
            // Only take off under open sky: indoors, the route leads out first.
            // The climb it would fly must be clear too, not just the sky
            // straight over it: its run carried on, the jump-boost off the
            // ground and the jet's lift, for its first two seconds (a door's
            // lintel or a porch roof ahead otherwise pins it under them).
            let open_sky = flyer && (flying || crate::map_pack::on(map).map_or(true, |pack| {
                let armor = self.armor_of(i);
                let p = &self.players[i];
                let run = Vec3::new(p.vel.x, 0.0, p.vel.z);
                let rise = p.vel.y.max(0.0) + if p.on_ground { jet_kick(&armor) } else { 0.0 };
                let lift = armor.jet - crate::defs::movement().gravity;
                let at = |t: f32| pos + Vec3::Y * 0.5 + run * t + Vec3::Y * (rise * t + 0.5 * lift * t * t);
                pack.sweep(pos + Vec3::Y * 0.5, pos + Vec3::Y * 20.0, PLAYER_RADIUS).is_none()
                    && (0..5).all(|k| pack.sweep(at(k as f32 * 0.4), at((k + 1) as f32 * 0.4), PLAYER_RADIUS).is_none())
            }));
            let dive = open_sky && seen.is_some_and(|(d, j)| d < BOT_DIVE_RANGE && pos.y > self.players[j].pos.y + 6.0);
            let flat = Vec2::new(goal.x - pos.x, goal.z - pos.z).length();
            // Hysteresis: once landed on the route, take off again only when
            // well away from the goal, so it doesn't flip at the boundary.
            // Near the goal the route owns the approach (its jet climbs must
            // not be interrupted), so a landed flyer only takes off again far out.
            let far = if flying { BOT_FLY_HANDOVER } else { BOT_FLY_TAKEOFF };
            // Keep flying until level with the goal, so it lands at deck height
            // (a floating base) rather than under it.
            // A goal well above it nearby (a floating deck) is out of reach of
            // one tank: take the route's pads and ledges up instead of hovering
            // under the hull.
            let out_of_reach = flat < BOT_FLY_CLIMB_RANGE && pos.y + BOT_FLY_CLIMB < goal.y;
            let fly = open_sky && (dive || (!direct && !out_of_reach
                && (flat > far || (flying && (pos.y - goal.y).abs() > 10.0))));
            if fly {
                let p = &mut self.players[i];
                p.bot_fly = true;
                p.bot_link = None;
                p.bot_path.clear();
            } else {
                self.players[i].bot_fly = false;
                if !direct {
                    if let Some(g) = nav { self.follow_route(i, g, goal); }
                }
            }

            // Aim at the visible target with this personality's lead and
            // error, or face its last-seen spot, or face the way it's going.
            let aim = if football { None } else { match seen {
                Some((d, j)) => {
                    let o = &self.players[j];
                    let lead = d / crate::defs::weapons().disc.speed * profile.lead;
                    Some((o.pos + Vec3::Y * 1.1 + o.vel * lead, Some(d)))
                }
                None if self.players[i].bot_memory > 0.0 => Some((self.players[i].bot_seen + Vec3::Y * 1.1, None)),
                None => None,
            } };
            if self.players[i].bot_pass {
                // Facing the receiver; bot_football_pass set the aim.
            } else if let Some((at, dist)) = aim {
                let dir = (at - (pos + Vec3::Y * 1.4)).normalize_or_zero();
                if dir.length_squared() > 0.1 {
                    self.players[i].yaw = (-dir.x).atan2(-dir.z) + aim_err.x;
                    // Flyers can aim steeply down onto a target below.
                    let down = if profile.style == crate::bot_nav::MoveStyle::Flyer { -1.3 } else { -0.7 };
                    self.players[i].pitch = (dir.y.asin() + aim_err.y).clamp(down, 0.7);
                }
                if let Some(d) = dist {
                    // A mod's range hints first (`registry::Weapon::bot_range`).
                    let hinted = crate::registry::registry().weapons.iter().map(|(k, _, w)| (k as u8, w))
                        .find(|(k, w)| self.players[i].loadout.carries(*k) && w.bot_range.is_some_and(|[lo, hi]| d >= lo && d <= hi)).map(|(k, _)| k);
                    let wanted = hinted.unwrap_or(if d < 28.0 { 1 } else { 0 });
                    self.players[i].weapon = rifles::allowed_slot(self.mode.def(), &self.players[i], wanted);
                }
            } else {
                let to = self.players[i].bot_goal - pos;
                if Vec2::new(to.x, to.z).length() > 1.0 {
                    self.players[i].yaw = (-to.x).atan2(-to.z);
                    self.players[i].pitch = -0.05;
                }
            }
        }
    }

    /// Point bot `i` at the next waypoint of a route to `goal`, planning or
    /// re-planning when the goal moves, the route runs out, or the bot stalls.
    fn follow_route(&mut self, i: usize, g: &crate::bot_nav::NavGraph, goal: Vec3) {
        use crate::bot_nav::Link;
        let pos = self.players[i].pos;
        // Last few metres with nothing in the way: go straight in rather
        // than threading waypoints that can sit behind a lip or railing.
        if pos.distance(goal) < 14.0 && (goal.y - pos.y).abs() < 3.0 {
            // A chest-height line: plinths and steps are hopped, not walls.
            let body = crate::map_pack::on(self.map).map_or(true, |pack| {
                let chest = Vec3::Y * 1.2;
                pack.sweep(pos + chest, goal + chest, 0.35).is_none()
            });
            if body {
                let p = &mut self.players[i];
                p.bot_goal = goal;
                p.bot_link = None;
                p.bot_saving = false;
                p.bot_stuck = 0.0;
                return;
            }
        }
        let p = &self.players[i];
        // Never swap routes mid-climb or mid-fall: the new route would start
        // from the floor below and drop the move in progress.
        let committed = !p.on_ground && matches!(p.bot_link, Some(Link::Jet) | Some(Link::Drop))
            && p.bot_path_i < p.bot_path.len() && p.bot_stuck < BOT_STUCK_SECONDS * 2.0;
        let stale = !committed && (p.bot_path.is_empty() || p.bot_path_i >= p.bot_path.len()
            || p.bot_path_goal.distance(goal) > 12.0 || p.bot_replan <= 0.0 || p.bot_stuck > BOT_STUCK_SECONDS);
        if stale && self.bot_plans > 0 {
            self.bot_plans -= 1;
            // Stuck: remember the link that failed so the new route avoids it.
            let p = &mut self.players[i];
            if p.bot_stuck > BOT_STUCK_SECONDS && p.bot_path_i > 0 && p.bot_path_i < p.bot_path.len() {
                let bad = (p.bot_path[p.bot_path_i - 1], p.bot_path[p.bot_path_i]);
                if p.bot_avoid.len() >= 8 { p.bot_avoid.remove(0); }
                p.bot_avoid.push(bad);
            }
            let weights = p.bot_profile.route;
            let avoid = p.bot_avoid.clone();
            let mut forgive = false;
            let path = match (g.nearest_in_reach(self.map, pos), g.nearest(goal)) {
                // The failed links may have been the only way: forget them
                // and take the plain route again.
                (Some(a), Some(b)) => g.path_avoiding(a, b, weights, BOT_SEARCH_BUDGET, &avoid)
                    .or_else(|| { forgive = true; g.path(a, b, weights, BOT_SEARCH_BUDGET) })
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let p = &mut self.players[i];
            if forgive { p.bot_avoid.clear(); }
            p.bot_path = path;
            p.bot_path_i = 0;
            p.bot_path_goal = goal;
            p.bot_replan = BOT_REPLAN_SECONDS;
            p.bot_stuck = 0.0;
            p.bot_best = f32::MAX;
        }
        let p = &mut self.players[i];
        // Advance past waypoints already reached.
        while p.bot_path_i < p.bot_path.len() {
            let w = g.nodes[p.bot_path[p.bot_path_i] as usize];
            let flat = Vec2::new(w.x - pos.x, w.z - pos.z).length();
            let reach = if g.terrain[p.bot_path[p.bot_path_i] as usize] { 5.0 } else { 1.8 };
            // Already past it at speed: the next waypoint is nearer.
            let passed = p.bot_path.get(p.bot_path_i + 1).is_some_and(|&n| {
                let nx = g.nodes[n as usize];
                g.terrain[p.bot_path[p.bot_path_i] as usize] && nx.distance(pos) < w.distance(pos)
            });
            if passed || (flat < reach && (w.y - pos.y).abs() < 2.8) {
                p.bot_path_i += 1;
                p.bot_best = f32::MAX;
                p.bot_stuck = 0.0;
            } else {
                break;
            }
        }
        if p.bot_path_i < p.bot_path.len() {
            let here = p.bot_path[p.bot_path_i];
            p.bot_goal = g.nodes[here as usize];
            p.bot_link = Some(if p.bot_path_i == 0 { Link::Walk } else { g.link(p.bot_path[p.bot_path_i - 1], here) });
            p.bot_saving = p.bot_path.get(p.bot_path_i + 1).is_some_and(|&next| g.link(here, next) == Link::Jet);
            let d = p.bot_goal.distance(pos);
            if d < p.bot_best - 0.5 { p.bot_best = d; p.bot_stuck = 0.0; }
        } else {
            p.bot_link = None;
            p.bot_saving = false;
        }
    }

    fn step_players(&mut self, dt: f32) {
        let n = self.players.len();
        let jump_edge_player = self.input.jump && !self.input.jump_prev;
        self.input.jump_prev = self.input.jump;

        for i in 0..n {
            if self.players[i].remote {
                continue;
            }
            if !self.players[i].alive {
                self.fall_body(i, dt);
                self.players[i].respawn -= dt;
                if self.players[i].respawn <= 0.0 {
                    self.respawn(i);
                }
                continue;
            }
            // A rider goes where their vehicle goes (`vehicles`).
            if self.players[i].riding > 0 {
                let _ = self.status_movement(i, dt);
                continue;
            }

            let is_local = i == self.player_id && !self.players[i].is_bot;
            if is_local { self.dry_clock = (self.dry_clock - dt).max(0.0); }
            let drain = self.point_drain(i).map_or(0.0, |d| d.rate);
            // Bots fly their own wishes, also on a server (whose network
            // inputs cover every slot, bots' included).
            let (wish_x, wish_z, jump_held, jump_edge, fire, jet) = if self.players[i].is_bot && (self.sparring() || self.blade_zone_of(i).is_some()) {
                self.spar_wish(i, dt)
            } else if self.players[i].is_bot {
                self.bot_wish(i)
            } else if let Some(input) = self.network_inputs.get(i) {
                let edge = input.jump && !self.players[i].jump_prev;
                self.players[i].jump_prev = input.jump;
                (input.move_x, input.move_z, input.jump, edge, input.fire, input.jet)
            } else if is_local {
                (
                    self.input.move_x.clamp(-1.0, 1.0),
                    self.input.move_z.clamp(-1.0, 1.0),
                    self.input.jump,
                    jump_edge_player,
                    self.input.fire,
                    self.input.jet,
                )
            } else {
                self.bot_wish(i)
            };
            // Football: a bot fires only to pass; nobody fires anything else.
            let fire = if self.players[i].is_bot && self.football() { self.players[i].bot_pass } else { fire };
            // A bot leaping onto a raised goal flies its own inputs, as does a
            // football bot making its play.
            let (wish_x, wish_z, jump_held, jump_edge, fire, jet) = match self.bot_leap_inputs(i, dt)
                .or_else(|| self.football_inputs(i, dt)) {
                Some(flight) => flight,
                None => (wish_x, wish_z, jump_held, jump_edge, fire, jet),
            };
            // A tackled player is down: no movement, jets or throws.
            let down = self.players[i].stun > 0.0;
            if down {
                self.players[i].stun = (self.players[i].stun - dt).max(0.0);
                // A tackled player gets up with an empty tank.
                if self.players[i].stun == 0.0 { self.players[i].energy = 0.0; }
            }
            let (wish_x, wish_z, jump_held, jump_edge, fire, jet) = if down || self.countdown > 0.0 {
                (0.0, 0.0, false, false, false, false)
            } else { (wish_x, wish_z, jump_held, jump_edge, fire, jet) };
            // A blade zone (`blade`): fire swings the blade, the jet key
            // guards, and no jets fly.
            let in_blade = self.blade_zone_of(i).is_some();
            let blade_walk = if in_blade { self.blade_step(i, fire, jet, dt) } else { self.sheathe(i); 1.0 };
            let (fire, jet) = if in_blade { (false, false) } else { (fire, jet) };
            // A fresh press of the jets (the jump-boost's trigger).
            let jet_edge = jet && !self.players[i].jet_prev;
            self.players[i].jet_prev = jet;
            // The pack and a borrowed armor (`packs`): predicted, like the
            // effects they hold on.
            let pack_press = !down && self.countdown <= 0.0 && if self.players[i].is_bot { self.players[i].bot_press & 1 != 0 }
                else if let Some(c) = self.network_inputs.get(i) { c.deploy } else { is_local && self.input.deploy };
            self.step_pack(i, pack_press, dt);
            let ability_press = !down && self.countdown <= 0.0 && if self.players[i].is_bot { self.players[i].bot_press & 2 != 0 }
                else if let Some(c) = self.network_inputs.get(i) { c.ability } else { is_local && self.input.ability };
            self.players[i].bot_press &= 4;
            self.step_ability(i, ability_press, dt);
            self.step_swap(i, dt);
            // Status effects (`status`): predicted with the movement.
            let has_effects = !self.players[i].effects.is_empty();
            let bearing = self.status_movement(i, dt);
            let (fire, jet) = if has_effects { (fire && !bearing.no_fire, jet && !bearing.no_jet) } else { (fire, jet) };
            let carrier = self.ball.carried_by(i);
            let football_rules = self.mode.def().movement == crate::modes::MovementModel::Classic;
            let mut armor = self.armor_of(i);
            let mut walk_class = loadout::walk_scale(self.players[i].armor) * blade_walk;
            // The blade's stamina is the tank: it fills at the blade's pace.
            if in_blade { armor.regen = 0.0; }
            if has_effects {
                let m = bearing.moves;
                (armor.jet, armor.side, armor.air_cap, armor.jump) = (armor.jet * m.jet, armor.side * m.side, armor.air_cap * m.air_cap, armor.jump * m.jump);
                armor.regen *= bearing.regen;
                walk_class *= m.walk;
            }

            let map = self.map;
            let edge = self.map_size() - 6.0;
            let disc_ready = is_local && self.players[i].weapon == 0
                && self.players[i].cooldown > 0.0 && self.players[i].cooldown <= dt;
            let land_hit = if football_rules {
                // Football has its own movement (football.rs).
                self.players[i].cooldown = (self.players[i].cooldown - dt).max(0.0);
                let controls = football::Controls { wish_x, wish_z, jump: jump_held, jet, carrier };
                let hurt = football::classic_move(map, &self.staged_water, &mut self.players[i], controls, edge, dt);
                if hurt > 0.0 && !self.predicting { self.players[i].health -= hurt; }
                if hurt > 0.0 { 0.25 } else { 0.0 }
            } else {
                let p = &mut self.players[i];
                p.cooldown = (p.cooldown - dt).max(0.0);
                let (fwd, right) = move_basis(p.yaw);
                let mut wish = right * wish_x + fwd * wish_z;
                let wlen = wish.length();
                if wlen > 1.0 {
                    wish /= wlen;
                }

                // A gravity zone (`features`) scales the fall: in low gravity a
                // jump carries and a jet climbs for a share of the effort.
                p.vel.y -= gravity_for_speed(Vec2::new(p.vel.x, p.vel.z).length()) * features::gravity_at(map, p.team, p.pos) * dt;
                // An updraft (`features`) pushes up, fading near its top, and
                // its air drags on vertical speed so a rider settles in it.
                if let Some(z) = features::updraft_at(map, p.team, p.pos) {
                    p.vel.y += (z.lift_at(p.pos.y) - p.vel.y * features::UPDRAFT_DRAG) * dt;
                }

                // Standing on a pad, platform or moving part last step
                // (`deploy`, `features`): ground, but never the terrain below.
                let footing = std::mem::take(&mut p.footing);
                let (h,face) = crate::terrain::support_on(map,p.pos);
                let nrm = if footing { Vec3::Y } else if crate::map_pack::on(map).is_some() {face} else {crate::terrain::normal_on(map, p.pos.x, p.pos.z)};
                let ground_y = h + PLAYER_RADIUS;
                let was_ground = p.on_ground;
                // Contact depends on motion relative to the slope, not world Y:
                // a skier climbing a ramp can have positive Y velocity.
                let ground_normal = contact_normal(map,p.pos);
                // After letting go of the movement keys, a walker going down a
                // slope moves away from it at full speed; one who was grounded
                // and still touches stays grounded, so the brake applies (it
                // used to count as air control, which coasts: an icy stop).
                let released = wish.length_squared() < 0.01;
                p.on_ground = footing || p.pos.y <= ground_y + 0.12
                    && (p.vel.dot(ground_normal) <= 0.5 || (released && was_ground && !jump_held && !jet));
                if p.on_ground {
                    p.coyote = 0.14;
                } else {
                    p.coyote = (p.coyote - dt).max(0.0);
                }

                if p.pos.y < ground_y {
                    p.pos.y = ground_y;
                    // A held jump is frictionless. Don't let the bumpy contact
                    // normal eat the slide; the ski projects gravity itself.
                    if !jump_held {
                        let vn = p.vel.dot(nrm);
                        if vn < 0.0 {
                            p.vel -= nrm * vn;
                        }
                    }
                    p.on_ground = true;
                }

                let grounded = p.on_ground || p.coyote > 0.0;
                let horiz_speed = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
                let mass = player_mass(p);
                // A fresh press can hop; holding through landing always skis.
                // This removes the repeated low-speed pogo of the earlier hybrid.
                let steep = nrm.y < 0.82;
                let hop = if jump_edge && grounded && !steep {
                    jump_scale(horiz_speed, p.vel.y)
                } else {
                    0.0
                };
                if hop > 0.01 {
                    let impulse = (crate::defs::movement().jump_impulse * crate::defs::movement().mass / mass) * hop * armor.jump;
                    p.vel += nrm * impulse;
                    p.on_ground = false;
                    p.coyote = 0.0;
                }
                p.skiing = jump_held && p.on_ground && (steep || hop <= 0.01);
                // How much of the body is under water (0 = dry: nothing below changes).
                let (wet, wet_volume) = crate::water::immersion(map, &self.staged_water, p.pos);
                let jet_cost = if wet >= crate::water::DEEP {crate::water::JET_ENERGY_FACTOR} else {1.0};

                p.jetting = false;
                if jet && p.energy >= armor.min_jet {
                    p.jetting = true;
                    // The jump-boost: jets pressed on the ground, skiing or
                    // not, kick you straight up with a share of the jump
                    // (`movement.jet_jump`) whatever your speed, so a
                    // jump-jet rises while you fly on. A jump this takeoff
                    // takes its place; climbing already shrinks it, as it
                    // does a jump.
                    if jet_edge && grounded && !steep {
                        let m = crate::defs::movement();
                        let full = m.jump_impulse * m.mass / mass * armor.jump;
                        let rising = (1.0 - p.vel.y.max(0.0) / m.max_jump_speed).max(0.0);
                        let kick = (full * m.jet_jump * rising - full * hop).max(0.0) * nrm.y;
                        p.vel.y += kick;
                    }
                    // Look does not aim the jet. WASD is the direction: it can
                    // turn a fast line, and it only adds speed up to 72 km/h.
                    // Steering is paid for out of the jet's thrust: what pushes
                    // sideways no longer lifts, so leaning in from a standstill
                    // sinks, and only above the push's speed is it all lift.
                    let scale = crate::defs::movement().mass / mass;
                    let thrust = armor.jet * scale;
                    let before = Vec3::new(p.vel.x, 0.0, p.vel.z);
                    let before_h = before.length();
                    let wish_dir = wish.normalize_or_zero();
                    let push = if wish.length_squared() > 0.04 {
                        let along = before.dot(wish_dir).max(0.0);
                        (armor.side * scale * jet_falloff_to(along, armor.air_cap) * wish.length().min(1.0)).min(thrust)
                    } else { 0.0 };
                    let up_speed = p.vel.y.max(0.0);
                    let fade = if armor.fade_up { jet_falloff(up_speed) } else { 1.0 };
                    p.vel.y += (thrust - push) * fade * dt;
                    if push > 0.0 {
                        p.vel += wish_dir * push * dt;
                        let after = Vec3::new(p.vel.x, 0.0, p.vel.z);
                        let now = after.length();
                        let cap = if before_h >= armor.air_cap {
                            before_h
                        } else {
                            armor.air_cap
                        };
                        if now > cap && now > 0.01 {
                            let s = cap / now;
                            p.vel.x = after.x * s;
                            p.vel.z = after.z * s;
                        }
                    }
                    p.on_ground = false;
                    p.skiing = false;
                    p.coyote = 0.0;
                    p.energy = (p.energy - armor.drain * jet_cost * dt).max(0.0);
                } else if p.repair_beam.is_none() && p.stream.is_none() {
                    // No recharge while the repair tool or a held beam runs
                    // (they drain energy).
                    p.energy = (p.energy + armor.regen * dt).min(loadout::energy_max(p.armor));
                }
                // A held control point's drain field saps its enemies' energy.
                p.energy = (p.energy - drain * dt).max(0.0);
                // Effects that give or drain energy.
                if bearing.energy != 0.0 { p.energy = (p.energy + bearing.energy * dt).clamp(0.0, loadout::energy_max(p.armor)); }

                if released && p.on_ground && !p.skiing && !steep && !p.jetting && wet <= 0.0 {
                    // Keys released on a walkable slope: brake in the ground's
                    // plane with gravity's downhill pull held by your footing,
                    // so you stop the same uphill, downhill or across. (Braking
                    // only the horizontal part let the ground contact turn each
                    // step's fall into downhill speed: on Ozarktic's giant hill
                    // you slid up to 18 m after releasing the key pointing
                    // downhill.) Walking with a key held is unchanged.
                    p.vel -= nrm * p.vel.dot(nrm);
                    let g = Vec3::new(0.0, -gravity_for_speed(horiz_speed), 0.0);
                    p.vel -= (g - nrm * g.dot(nrm)) * dt;
                    let walk_max = crate::defs::movement().walk_max * (1.0 - crate::water::WADE_SLOW * wet) * walk_class;
                    let wish_on_slope = (wish - nrm * wish.dot(nrm)).normalize_or_zero() * wish.length();
                    let delta = wish_on_slope * walk_max - p.vel;
                    let accel = if p.vel.length() > walk_max || wish.length_squared() < 0.01 { crate::defs::movement().ground_brake } else { crate::defs::movement().walk_accel };
                    p.vel += delta.clamp_length_max(accel * dt);
                } else if p.on_ground && !p.skiing {
                    // Approach the desired walking speed; releasing ski brakes
                    // progressively instead of instantly deleting route momentum.
                    let horizontal = Vec3::new(p.vel.x, 0.0, p.vel.z);
                    let walk_max = crate::defs::movement().walk_max * (1.0 - crate::water::WADE_SLOW * wet) * walk_class;
                    let delta = wish * walk_max - horizontal;
                    let accel = if horizontal.length() > walk_max || wish.length_squared() < 0.01 {
                        crate::defs::movement().ground_brake
                    } else { crate::defs::movement().walk_accel };
                    p.vel += delta.clamp_length_max(accel * dt);
                } else if p.skiing {
                    ride_ski(map, p, wish, armor.ski_drag, dt);
                } else {
                    // Air control: coast without drag, steer with
                    // WASD. Above walking speed, steering cannot add energy.
                    if !p.jetting {
                        steer_horizontal(p, wish, crate::defs::movement().air_accel, crate::defs::movement().walk_max, dt);
                    }
                }
                if let (true, Some(volume)) = (wet > 0.0, wet_volume) {
                    let g = gravity_for_speed(Vec2::new(p.vel.x, p.vel.z).length());
                    crate::water::apply(&mut p.vel, wet, p.skiing, volume, g, dt);
                }
                apply_speed_limits(p, dt);

                // Keys released: keep the footing down a slope while braking
                // (walking downhill leaves the ground briefly every step).
                let walk_snap = released && (p.on_ground || p.coyote > 0.0) && !p.skiing && !p.jetting && hop <= 0.01 && !jump_held && !footing;
                let impact_speed = move_over_terrain(map, p, jump_held, dt);
                if walk_snap && !p.skiing {
                    let (floor, face) = crate::terrain::support_on(map, p.pos);
                    let drop = p.pos.y - (floor + PLAYER_RADIUS);
                    if drop > 0.0 && drop < crate::defs::movement().walk_step_down && face.y >= 0.82 && p.vel.y <= 0.5
                        && snap_clear(map, p.pos, floor + PLAYER_RADIUS) {
                        p.pos.y = floor + PLAYER_RADIUS;
                        let vn = p.vel.dot(face);
                        if vn < 0.0 { p.vel -= face * vn; }
                        p.on_ground = true;
                    }
                }
                if p.skiing {
                    let ground_y = crate::terrain::support_on(map,p.pos).0 + PLAYER_RADIUS;
                    let drop = p.pos.y - ground_y;
                    let nrm = contact_normal(map,p.pos);
                    // Catch a nearby descending surface, never pull an outgoing
                    // velocity down to a crest. Ramps must launch the player.
                    if drop <= 0.0 || (drop < 0.15 && p.vel.dot(nrm) <= 0.0 && snap_clear(map, p.pos, ground_y)) {
                        p.pos.y = ground_y;
                        let vn = p.vel.dot(nrm);
                        if vn < 0.0 {
                            p.vel -= nrm * vn;
                        }
                        p.on_ground = true;
                    } else {
                        p.on_ground = false;
                        p.skiing = false;
                    }
                }

                // Bounds.
                if p.pos.x < 6.0 {
                    p.pos.x = 6.0;
                    p.vel.x = p.vel.x.abs() * 0.4;
                }
                if p.pos.x > edge {
                    p.pos.x = edge;
                    p.vel.x = -p.vel.x.abs() * 0.4;
                }
                if p.pos.z < 6.0 {
                    p.pos.z = 6.0;
                    p.vel.z = p.vel.z.abs() * 0.4;
                }
                if p.pos.z > edge {
                    p.pos.z = edge;
                    p.vel.z = -p.vel.z.abs() * 0.4;
                }

                let mut land_hit = 0.0;
                if !was_ground && p.on_ground && impact_speed > crate::defs::movement().landing_speed {
                    if !self.predicting { p.health -= landing_damage(impact_speed) * loadout::damage_taken(p.armor) * loadout::resist(p.armor, "impact"); }
                    land_hit = 0.25;
                }
                land_hit
            };

            if land_hit > 0.0 && i == self.player_id {
                self.trauma = (self.trauma + land_hit).min(1.0);
            }
            if disc_ready { self.push_event("disc_ready"); }

            self.collide_pillars(i);
            self.collide_deployables(i);
            self.collide_moveables(i);

            // Holding the repair tool keeps your weapon down.
            let repairing = if self.players[i].is_bot { self.players[i].bot_repair }
                else if let Some(input) = self.network_inputs.get(i) { input.repair }
                else { is_local && self.input.repair };
            // The fire-mode key cycles the held weapon's modes.
            let mode_press = if self.players[i].is_bot { false } else if let Some(c) = self.network_inputs.get(i) { c.mode }
                else { is_local && self.input.mode };
            if mode_press { self.next_fire_mode(i); }
            // The trigger (`firing`): a weapon the mode leaves out never
            // fires; prediction spends rounds too (the next snapshot
            // corrects the count), so an empty weapon never predicts a shot.
            // Operating a turret from a command station, fire is the turret's.
            let operating = self.operated_by(i).is_some();
            self.trigger(i, fire && !repairing && !operating, is_local, dt);

            if !self.predicting && self.players[i].health <= 0.0 && self.players[i].alive {
                self.kill(i, None, "Fall");
            }
        }
        // Blades cut once everyone has moved and swung this step, so two
        // that meet clash whoever's turn came first.
        if !self.predicting && features::has_blade_zones(self.map) {
            for i in 0..n { if self.blade_zone_of(i).is_some() { self.blade_cuts(i); } }
        }
    }

    /// A flyer's steering while crossing high: (wish, jet, ski).
    ///
    /// Cruises `BOT_FLY_CLEARANCE` above the highest ground over the next
    /// 120 m, pulsing the jet between a full tank and empty (energy refills
    /// while it coasts), and climbs over a structure ahead. Near the goal it
    /// glides down a slope onto it; above a visible target it dives.
    fn fly_controls(&mut self, i: usize, target_in_view: bool) -> (Vec3, bool, bool) {
        let map = self.map;
        let p = &self.players[i];
        let (pos, vel, goal, energy, on_ground) = (p.pos, p.vel, p.bot_goal, p.energy, p.on_ground);
        let target = p.bot_target.and_then(|j| self.players.get(j)).map(|o| o.pos);
        let to = goal - pos;
        let horiz = Vec3::new(to.x, 0.0, to.z);
        let flat = horiz.length();
        let dir = if flat > 0.5 { horiz / flat } else { Vec3::ZERO };
        let height = |x: f32, z: f32| crate::terrain::height_on(map, x, z);
        let below_me = pos.y - height(pos.x, pos.z);
        // Dive: fall onto a target below, braking with the jet near the
        // ground, keys let go so all of it lifts.
        if let Some(t) = target.filter(|t| target_in_view && pos.y > t.y + 6.0) {
            let at = Vec3::new(t.x - pos.x, 0.0, t.z - pos.z).normalize_or_zero();
            let stop = vel.y * vel.y / (2.0 * (standard_armor().jet - crate::defs::movement().gravity));
            let brake = vel.y < -10.0 && below_me < stop + 4.0 && energy >= standard_armor().min_jet;
            let skier = self.mode.def().movement != crate::modes::MovementModel::Classic;
            return (if brake && skier { Vec3::ZERO } else { at }, brake, false);
        }
        let mut ground = height(pos.x, pos.z);
        for k in 1..=4 {
            let s = pos + dir * (30.0 * k as f32).min(flat);
            ground = ground.max(height(s.x, s.z));
        }
        // Far out: clear the ground ahead and never cruise below the goal
        // (a floating base's deck). Over the last 150 m: glide down onto it.
        let glide = goal.y + 4.0 + (flat - BOT_FLY_HANDOVER).max(0.0) * 0.3;
        let cruise = if flat > 150.0 {
            (ground + BOT_FLY_CLEARANCE).max(goal.y + 10.0)
        } else {
            glide.max(ground + 8.0)
        };
        let horiz_speed = Vec2::new(vel.x, vel.z).length();
        let reach = (horiz_speed + 10.0).min(40.0);
        let blocked = crate::map_pack::on(map).is_some_and(|pack| {
            pack.sweep(pos + Vec3::Y * 0.5, pos + Vec3::Y * 0.5 + dir * reach, PLAYER_RADIUS).is_some()
        });
        // Pulse: jet from a full tank down to empty, then coast to refill.
        let mut coast = self.players[i].bot_saving;
        if energy < BOT_FLY_EMPTY { coast = true; } else if energy > BOT_FLY_REFILL { coast = false; }
        self.players[i].bot_saving = coast;
        let low = pos.y < cruise;
        let jet = !coast && energy >= standard_armor().min_jet && (low || blocked || horiz_speed < 18.0);
        // A wall ahead (a courtyard, a hull): stop pushing into it and climb
        // straight up on a full tank until the way is clear.
        let wish = if blocked { Vec3::ZERO } else { dir };
        // Grounded while refilling: ski on toward the goal.
        let ski = on_ground && coast && !blocked;
        (wish, jet, ski)
    }

    fn bot_wish(&mut self, i: usize) -> (f32, f32, bool, bool, bool, bool) {
        use crate::bot_nav::{Link, MoveStyle};
        let p = &self.players[i];
        let to = p.bot_goal - p.pos;
        let yaw = p.yaw;
        let pitch = p.pitch;
        let team = p.team;
        let pos = p.pos;
        let vel = p.vel;
        let on_ground = p.on_ground;
        let energy = p.energy;
        let profile = p.bot_profile;
        let link = p.bot_link;
        let saving = p.bot_saving;
        let goal = p.bot_goal;
        let seen_for = p.bot_seen_for;
        let target = p.bot_target;
        let (fwd, right) = move_basis(yaw);
        let horiz = Vec3::new(to.x, 0.0, to.z);
        let hlen = horiz.length();
        let mut wish = if hlen > 1.0 { horiz / hlen } else { horiz };
        // Fire only at the enemy it can see, re-checking sight at the trigger,
        // and only once it has had the target in view for its reaction time.
        let mut fire = false;
        let mut target_in_view = false;
        if let Some(o) = target.and_then(|j| self.players.get(j)) {
            let d = pos.distance(o.pos);
            let dir = look_dir(yaw, pitch);
            let to_e = ((o.pos + Vec3::Y) - (pos + Vec3::Y * 1.4)).normalize_or_zero();
            target_in_view = o.alive && target.is_some_and(|j| self.hostile(i, team, j));
            if target_in_view && d < 78.0 && dir.dot(to_e) > 0.86 && seen_for >= profile.reaction {
                let chest = o.pos + Vec3::Y * crate::equipment::CHEST_HEIGHT;
                fire = obstacle_hit(self.map, &self.pillars, pos + Vec3::Y * EYE, chest, 0.0).is_none();
            }
        }
        let r = self.rng();
        let horiz_speed = Vec2::new(vel.x, vel.z).length();
        // A jet up to the waypoint: the keys' share for the speed toward it
        // the climb needs (`bot_nav::lean_speed`), since steering takes lift.
        // Football's own movement keeps its bots' steering as it was.
        let armor = self.armor_of(i);
        let skier = self.mode.def().movement != crate::modes::MovementModel::Classic;
        let lean = |toward: Vec3| if !skier { toward } else {
            let gap = toward * crate::bot_nav::lean_speed(to.y, hlen, vel.y, &armor) - Vec3::new(vel.x, 0.0, vel.z);
            gap.normalize_or_zero() * crate::bot_nav::lean_share(gap.length(), crate::bot_nav::climbing(to.y, vel.y), &armor)
        };
        let fly = self.players[i].bot_fly;
        let (jet, ski) = if fly {
            let (w, jet, ski) = self.fly_controls(i, target_in_view);
            wish = w;
            (jet, ski)
        } else { match link {
            // Climbs: wait for a tank first, then jet until a coast would
            // carry it over the waypoint, braking horizontally near the end.
            // Steering takes lift: push only for the speed toward the
            // waypoint the climb needs, cancelling any drift (`lean`, as the
            // link was planned).
            Some(Link::Jet) => {
                let above = pos.y > goal.y + 1.0;
                wish = lean(wish);
                let brake = horiz_speed * horiz_speed / (2.0 * standard_armor().side);
                if !above && hlen < brake + 1.0 && horiz_speed > 3.0 {
                    let back = -Vec3::new(vel.x, 0.0, vel.z).normalize_or_zero();
                    wish = back;
                }
                // Under an overhang (a roof's eave over its wall): the climb
                // straight up is blocked, so back away from the wall until it
                // clears, then rise and cross, instead of pinning under it.
                if pos.y < goal.y + 0.5 && hlen < 4.0 && hlen > 0.05 {
                    let blocked = crate::map_pack::on(self.map).is_some_and(|pack| {
                        let top = goal.y + 1.5;
                        top > pos.y + 0.5 && pack.sweep(pos + Vec3::Y * 0.5, Vec3::new(pos.x, top, pos.z), PLAYER_RADIUS).is_some()
                    });
                    if blocked { wish = -horiz / hlen; }
                }
                let tank = energy >= loadout::energy_max(self.players[i].armor) * crate::bot_nav::JET_PLAN_ENERGY - 4.0;
                // A climb starts from the ground, as it was planned: reaching
                // its launch in the air and falling (a hop off a ski run),
                // land first instead of burning the tank against the fall;
                // jets pressed on landing kick it up as well (the jump-boost).
                let falling_in = !on_ground && !self.players[i].jetting && vel.y < -1.0 && pos.y < goal.y;
                if falling_in {
                    (false, false)
                } else if on_ground && !tank && (hlen < 12.0 || horiz_speed < 10.0) {
                    // Recharge in place before committing to the climb.
                    wish = Vec3::ZERO;
                    (false, false)
                } else {
                    // Hold the jet while a coast from here would arrive
                    // under the waypoint, not just until it's briefly above.
                    let t = hlen / horiz_speed.max(4.0);
                    let arrive = pos.y + vel.y * t - 0.5 * crate::defs::movement().gravity * t * t;
                    (arrive < goal.y + 1.5 && energy >= standard_armor().min_jet && (!on_ground || tank), on_ground)
                }
            }
            // Drops: walk off, then burn jet near the bottom to soften the
            // landing, keys let go so all of it lifts.
            Some(Link::Drop) => {
                let fall = (pos.y - goal.y).max(0.0);
                let stop = vel.y * vel.y / (2.0 * (standard_armor().jet - crate::defs::movement().gravity));
                let brake = vel.y < -12.0 && fall < stop + 2.0 && energy >= standard_armor().min_jet;
                if brake && skier { wish = Vec3::ZERO; }
                (brake, false)
            }
            // Terrain: skis down on descents and at speed; jet up climbs and
            // across flats while the tank is healthy, as players do.
            Some(Link::Ski) => {
                let downhill = to.y < -0.5;
                let ski = profile.style == MoveStyle::Skier || downhill || horiz_speed > 15.0;
                let push = !saving && ((to.y > 1.0 && energy > 15.0) || (!downhill && horiz_speed < 18.0 && energy > 40.0));
                (push, ski)
            }
            // Floors and ramps: walk, so a slide doesn't carry it through
            // doors; hop a lip or step with a short jet, steered like a climb.
            Some(Link::Walk) => {
                let hop = !saving && to.y > 1.5 && hlen < 4.0 && energy > standard_armor().min_jet;
                if hop { wish = lean(wish); }
                (hop, false)
            }
            // No route yet (or a close fight): the original steering.
            None => {
                let uphill = to.y > 5.0;
                (energy >= standard_armor().min_jet && (!on_ground || uphill), true)
            }
        } };
        // Jetters fight from the air when they have the energy for it.
        let jet = jet || (profile.style == MoveStyle::Jetter && target_in_view && energy > 30.0 && r < 0.6);
        let mx = wish.dot(right).clamp(-1.0, 1.0);
        let mz = wish.dot(fwd).clamp(-1.0, 1.0);
        // Hop a step or plinth edge too tall to walk up; elsewhere the odd
        // random hop off rough ground.
        let step_up = matches!(link, Some(Link::Walk) | None) && to.y > 0.4 && hlen < 3.0;
        let jump_edge = on_ground && (step_up || (link != Some(Link::Walk) && r < 0.02));
        // Stall tracking: time without getting closer to the waypoint (the
        // route follower resets it on progress), except while recharging.
        // A climb that keeps falling short counts, so it gets re-planned.
        if link.is_some() && !(on_ground && !jet && wish == Vec3::ZERO) {
            self.players[i].bot_stuck += STEP;
        }
        let forward = if link.is_some() { mz } else { mz.max(0.15) };
        (mx, forward, ski, jump_edge, fire && r < 0.7, jet)
    }

    fn collide_pillars(&mut self, i: usize) {
        let pos = self.players[i].pos;
        let mut vel = self.players[i].vel;
        let mut new_pos = pos;
        let map = self.map;
        let pillars: Vec<(f32, f32, f32, f32)> = self.pillars.iter().map(|p| (p.x, p.z, p.r, p.h)).collect();
        for (px, pz, pr, ph) in pillars {
            let base_y = crate::terrain::height_on(map, px, pz);
            if pos.y > base_y + ph + 1.2 {
                continue;
            }
            let dx = new_pos.x - px;
            let dz = new_pos.z - pz;
            let d = dx.hypot(dz);
            let min = pr + PLAYER_RADIUS;
            if d < min {
                let (nx, nz) = if d > 1e-3 { (dx / d, dz / d) } else { (1.0, 0.0) };
                new_pos.x = px + nx * min;
                new_pos.z = pz + nz * min;
                let vn = vel.x * nx + vel.z * nz;
                if vn < 0.0 {
                    vel.x -= nx * vn * 1.35;
                    vel.z -= nz * vn * 1.35;
                }
            }
        }
        self.players[i].pos = new_pos;
        self.players[i].vel = vel;
    }

    /// Where a shot from player `i`'s weapon `w` leaves, and its direction:
    /// from the muzzle, steered onto the crosshair `aim` metres out.
    fn muzzle_shot(&self, i: usize, w: &crate::registry::Weapon, aim: f32) -> (Vec3, Vec3) {
        let p = &self.players[i];
        let dir = look_dir(p.yaw, p.pitch);
        let eye = p.pos + Vec3::Y * EYE;
        let spd = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        let origin = muzzle_of(eye, dir, w, camera_fov(spd));
        // The viewmodel muzzle sits up to ~2 m ahead of the eye at wide FOV,
        // farther than a wall is thick: never spawn a shot past a surface.
        let origin = match obstacle_hit(self.map, &self.pillars, eye, origin, SHOT_RADIUS) {
            Some(t) => eye.lerp(origin, (t - 0.05 / eye.distance(origin).max(0.05)).max(0.0)),
            None => origin,
        };
        // Leave the muzzle, but steer onto the crosshair so a close shot still hits.
        let aim = eye + dir * aim;
        let shot = (aim - origin).normalize_or_zero();
        (origin, if shot.length_squared() > 0.5 { shot } else { dir })
    }

    /// Where player `i`'s next shot leaves and the way it flies (the held
    /// weapon's muzzle, steered onto the first-person crosshair): what the
    /// third-person view's crosshair marks.
    pub fn aim_line(&self, i: usize) -> Option<(Vec3, Vec3)> {
        let w = self.weapon_def(i)?;
        self.players.get(i).filter(|p| p.alive)?;
        Some(self.muzzle_shot(i, w, w.aim))
    }

    /// Fire player `i`'s held weapon (the shot already paid for).
    #[cfg(test)]
    fn shoot(&mut self, i: usize) { self.shoot_with(i, 1.0); }

    /// Fire player `i`'s held weapon at `power` (a charged shot's multiple
    /// of speed and damage): its projectile from the muzzle (or its pellets),
    /// homing on its target if it seeks, or a beam resolved at once.
    pub(super) fn shoot_with(&mut self, i: usize, power: f32) {
        let Some((w, round, kind)) = self.held(i) else { return };
        // Firing ends what firing reveals (a cloak).
        self.reveal(i);
        if round.class == crate::registry::ProjectileClass::Beam { self.fire_beam(i, w, round, kind, power); return; }
        self.players[i].shots += 1;
        let (origin, shot) = self.muzzle_shot(i, w, w.aim);
        let p = &self.players[i];
        let dir = look_dir(p.yaw, p.pitch);
        let speed = if power != 1.0 { round.speed * power } else { round.speed };
        let mut vel = shot * speed + p.vel * round.inherit;
        let carried = p.vel * round.inherit;
        let is_bot = p.is_bot;
        let team = p.team;
        if w.spread > 0.0 {
            let (right, up) = view_basis(dir);
            vel += (right * (self.rng() - 0.5) + up * (self.rng() - 0.5)) * w.spread;
        }
        if is_bot && w.bot_spread > 0.0 {
            let a = self.rng() - 0.5;
            let b = self.rng() - 0.5;
            let c = self.rng() - 0.5;
            vel += Vec3::new(a, b, c) * w.bot_spread;
        }
        let target = round.seek.as_ref().map_or(0, |s| self.seek_target(i, s, origin, shot));
        if round.pellets > 1 {
            // Pellets: each its own way within the cone.
            let (right, up) = view_basis(shot);
            let spread = (round.cone.to_radians() * 0.5).tan();
            let extra = vel - (shot * speed + carried);
            for _ in 0..round.pellets {
                let (a, b) = ((self.rng() - 0.5) * 2.0, (self.rng() - 0.5) * 2.0);
                let d = (shot + (right * a + up * b) * spread).normalize_or(shot);
                self.discs.push(Disc { pos: origin, vel: d * speed + carried + extra, team, owner: i, life: round.life, kind, spin: 0.0, target, power });
            }
        } else {
            // A shell flies until it hits something past its launch safety.
            self.discs.push(Disc { pos: origin, vel, team, owner: i, life: round.life, kind, spin: 0.0, target, power });
        }
        {
            let p = &mut self.players[i];
            p.cooldown = w.reload;
            if (i == self.player_id || !self.network_inputs.is_empty()) && w.recoil > 0.0 {
                p.pitch = (p.pitch + w.recoil).min(1.5);
            }
        }
        if i == self.player_id {
            self.trauma = (self.trauma + w.shake).min(1.0);
            self.push_event(&w.sound);
        } else if self.network_inputs.is_empty() {
            self.spatial_sounds.push((w.sound.as_str(), origin));
        }
    }

    fn step_discs(&mut self, dt: f32) {
        use crate::registry::{Expire, ProjectileClass, Water};
        for puff in &mut self.smoke {
            puff.age += dt;
            puff.pos.y += dt * 0.35;
        }
        self.smoke.retain(|p| p.age < 0.5);
        let n_sub = 4;
        let sdt = dt / n_sub as f32;
        let mut explode: Vec<(Vec3, usize, u8, Team, Option<usize>)> = Vec::new();
        // Rounds that met water and burst there (`Water::Burst`).
        let mut water_bursts: Vec<(Vec3, usize, u8, Team)> = Vec::new();
        let mut grapples: Vec<(usize, Vec3, f32)> = Vec::new();
        let mut keep = Vec::new();
        // Rounds that hit without bursting, in the order they struck.
        let mut hits: Vec<(Hit, u8)> = Vec::new();
        let mut vehicle_hits: Vec<(usize, f32, usize)> = Vec::new();
        // Paths of rounds that clear mines this step: a resting mine they
        // cross goes off.
        let mut shot_paths: Vec<(Vec3, Vec3)> = Vec::new();

        let map = self.map;
        let far = self.map_size() - 1.0;
        let rounds = &crate::registry::registry().projectiles;
        for mut d in std::mem::take(&mut self.discs) {
            // A kind no entry has (a stale snapshot) is dropped.
            let Some(round) = rounds.get(d.kind as usize) else { continue };
            if round.smoke && self.smoke.len() < 256 {
                self.smoke.push(SmokePuff { pos: d.pos, age: 0.0 });
            }
            d.life -= dt;
            if d.life <= 0.0 {
                // A mine that never settled, or one that has lain out its
                // life, fizzles; rounds that only hit are simply gone.
                if round.expire == Expire::Explode {
                    explode.push((d.pos, d.owner, d.kind, d.team, None));
                }
                continue;
            }
            // A beam is only a picture of a shot already resolved.
            if round.class == ProjectileClass::Beam { keep.push(d); continue; }
            // A resting grenade or mine lies still.
            if throwables::resting(&d) { keep.push(d); continue; }
            let mut dead = false;
            for _ in 0..n_sub {
                d.spin += sdt * 42.0;
                d.vel.y -= crate::defs::movement().gravity * round.gravity * sdt;
                // Rockets speed up; homing rounds turn toward their target
                // and burst near it; guided ones follow their shooter's aim.
                if round.accel > 0.0 {
                    let speed = d.vel.length();
                    if round.max_speed <= 0.0 || speed < round.max_speed {
                        let faster = speed + round.accel * sdt;
                        let capped = if round.max_speed > 0.0 { faster.min(round.max_speed) } else { faster };
                        d.vel = d.vel.normalize_or_zero() * capped;
                    }
                }
                if let Some(seek) = &round.seek {
                    if let Some(q) = (d.target as usize).checked_sub(1).and_then(|j| self.players.get(j)).filter(|q| q.alive) {
                        let to = q.pos + Vec3::Y * 0.9 - d.pos;
                        d.vel = firing::turn_toward(d.vel, to, seek.turn.to_radians() * sdt);
                        if seek.proximity > 0.0 && to.length() <= seek.proximity {
                            explode.push((d.pos, d.owner, d.kind, d.team, Some(d.target as usize - 1)));
                            dead = true;
                            break;
                        }
                    }
                }
                if round.guided > 0.0 {
                    if let Some(o) = self.players.get(d.owner).filter(|o| o.alive) {
                        let aim = o.pos + Vec3::Y * EYE + look_dir(o.yaw, o.pitch) * 500.0;
                        d.vel = firing::turn_toward(d.vel, aim - d.pos, round.guided.to_radians() * sdt);
                    }
                }
                // Water: some rounds are put out, some burst where they meet
                // it (below), and the rest slow hard but still burst on
                // contact or fuse. One already in it (fired from under the
                // surface) bursts at once.
                if let Some(v) = crate::water::at(map, &self.staged_water, d.pos) {
                    match round.water {
                        Water::Fizzle => { dead = true; break; }
                        Water::Burst if !v.lava => { water_bursts.push((d.pos, d.owner, d.kind, d.team)); dead = true; break; }
                        Water::Drag | Water::Burst => d.vel *= (-crate::water::PROJECTILE_DRAG * sdt).exp(),
                    }
                }
                let next = d.pos + d.vel * sdt;
                let surface = surface_hit(map, &self.pillars, d.pos, next, round.radius, far);
                let mut hit = surface.map(|(t, _)| t);
                // The struck surface's normal, while the map is what was hit first.
                let mut surface_normal = surface.map(|(_, n)| n);
                // Lava is a surface to shots as it is to players: they burst
                // on it (and splash it, `World::lava_splash`) instead of sinking.
                if let Some(t) = crate::water::lava_crossing(map, d.pos, next) {
                    if hit.is_none_or(|old| t < old) { hit = Some(t); surface_normal = Some(Vec3::Y); }
                }
                // Keep the actual boundary impact height, rather than moving
                // an airborne explosion down onto the terrain.
                for (a, b) in [(d.pos.x, next.x), (d.pos.z, next.z)] {
                    if b < 1.0 || b > far {
                        let t = if (b - a).abs() > 1e-6 {
                            ((b.clamp(1.0, far) - a) / (b - a)).clamp(0.0, 1.0)
                        } else { 0.0 };
                        if hit.is_none_or(|old| t < old) { surface_normal = None; }
                        hit = Some(hit.map_or(t, |old| old.min(t)));
                    }
                }
                // A map's moving parts are surfaces too (`features`).
                if let Some((t, n)) = self.moveable_hit(d.pos, next, round.radius) {
                    if hit.is_none_or(|old| t < old) { hit = Some(t); surface_normal = Some(n); }
                }
                let mut victim = None;
                let mut equipment_victim=None;
                // Walls stop everything; force fields stop the other team's shots.
                let mut deploy_victim = None;
                if let Some((t, k)) = self.deployable_hit(d.pos, next, SHOT_RADIUS, Some(d.team)) {
                    if hit.is_none_or(|old| t < old) { hit = Some(t); deploy_victim = Some(k); }
                }
                for (idx,(obj,state)) in crate::equipment::definitions(map).iter().zip(&self.equipment).enumerate() {
                    if obj.team as usize==d.team.idx() || state.health<=0. {continue;}
                    if let Some(t)=segment_sphere(d.pos,next,obj.pos(),obj.radius+round.reach) {
                        if hit.is_none_or(|old|t<old) {hit=Some(t);equipment_victim=Some(idx);deploy_victim=None;}
                    }
                }
                // Vehicles stop shots (`vehicles`): a burst bursts on the hull,
                // a round hurts it.
                let mut craft_victim = None;
                if !self.vehicles.is_empty() {
                    let own = self.players.get(d.owner).and_then(|p| (p.riding as usize).checked_sub(1));
                    if let Some((t, v, n)) = self.vehicle_hit(d.pos, next, SHOT_RADIUS, own) {
                        if hit.is_none_or(|old| t < old) { hit = Some(t); surface_normal = Some(n); craft_victim = Some(v); equipment_victim = None; deploy_victim = None; }
                    }
                }
                // A burning round (a summit fireball) flies through people.
                // `features::burn_fireballs` hurts whoever it overlaps, and
                // the ground still bursts it.
                if round.burn <= 0.0 {
                    for (idx, pl) in self.players.iter().enumerate() {
                        if !pl.alive || (pl.riding > 0 && !self.rides_exposed(idx)) || (!self.hostile(d.owner, d.team, idx) && !self.team_fire(d.owner, d.team, idx)) {
                            continue;
                        }
                        let c = pl.pos + Vec3::Y * 0.9;
                        if let Some(t) = segment_sphere(d.pos, next, c, PLAYER_RADIUS + 0.45) {
                            if hit.is_none_or(|old| t < old) {
                                hit = Some(t);
                                victim = Some(idx);
                                equipment_victim=None;
                                deploy_victim = None;
                                craft_victim = None;
                            }
                        }
                    }
                }
                // A round that bursts at water does so where it goes in,
                // unless it hits something first.
                if round.water == Water::Burst {
                    let wet = crate::water::water_crossing(map, &self.staged_water, d.pos, next).filter(|&t| hit.is_none_or(|old| t < old));
                    if let Some(t) = wet {
                        water_bursts.push((d.pos.lerp(next, t), d.owner, d.kind, d.team));
                        dead = true;
                        break;
                    }
                }
                if round.clears_mines {
                    shot_paths.push((d.pos, hit.map_or(next, |t| d.pos.lerp(next, t))));
                }
                if let Some(t) = hit {
                    let contact = d.pos.lerp(next, t);
                    // A grappling round drags its shooter toward what it met.
                    if round.grapple > 0.0 && round.class != ProjectileClass::Thrown { grapples.push((d.owner, contact, round.grapple)); }
                    let normal_at = |w: &World, d: &Disc| if let Some(idx) = victim {
                        (contact - (w.players[idx].pos + Vec3::Y * 0.9)).normalize_or_zero()
                    } else if let Some(idx)=equipment_victim {
                        (contact-crate::equipment::definitions(map)[idx].pos()).normalize_or_zero()
                    } else if let Some(k) = deploy_victim {
                        w.deployables[k].normal_at(contact, SHOT_RADIUS)
                    } else { surface_normal.unwrap_or_else(|| grenade_contact_normal(map, &w.pillars, contact, far, d.vel)) };
                    match round.class {
                        ProjectileClass::Thrown => {
                            // Grenades and mines bounce off everything,
                            // players included, and never go off on contact.
                            let normal = normal_at(self, &d);
                            World::bounce_throwable(&mut d, contact, normal);
                            if d.vel == Vec3::ZERO { break; }
                            continue;
                        }
                        ProjectileClass::Shell => {
                            // A brief launch safety lets close surfaces bounce
                            // the shell. Armed shells burst on their next contact.
                            if d.life <= round.life - round.arm {
                                explode.push((contact, d.owner, d.kind, d.team, victim));
                                dead = true;
                                break;
                            }
                            let normal = normal_at(self, &d);
                            let inward = d.vel.dot(normal);
                            if inward < 0.0 { d.vel -= normal * inward * (1.0 + round.elasticity); }
                            d.vel *= round.keep;
                            d.pos = contact + normal * 0.03;
                            continue;
                        }
                        ProjectileClass::Straight | ProjectileClass::Beam | ProjectileClass::Stream => {}
                    }
                    if round.blast.is_some() {
                        explode.push((contact, d.owner, d.kind, d.team, victim));
                    } else {
                        // A round that glances off the map keeps going;
                        // anything else it meets takes the hit.
                        if let (None, None, None, None, Some(n)) = (victim, equipment_victim, deploy_victim, craft_victim, surface_normal) {
                            if round.keep > 0.0 {
                                d.vel = (d.vel - n * 2.0 * d.vel.dot(n)) * round.keep;
                                d.pos = contact + n * 0.03;
                                continue;
                            }
                        }
                        if let Some(k) = deploy_victim { hits.push((Hit::Deployable(k), d.kind)); }
                        if let Some(idx) = equipment_victim { hits.push((Hit::Equipment(idx), d.kind)); }
                        if let Some(idx) = victim { hits.push((Hit::Player(idx, d.owner, contact, d.vel.normalize_or_zero()), d.kind)); }
                        if let Some(v) = craft_victim { hits.push((Hit::Vehicle(v, d.owner), d.kind)); }
                    }
                    dead = true;
                    break;
                }
                d.pos = next;
            }
            if !dead {
                keep.push(d);
            }
        }
        self.discs = keep;
        if !self.predicting { self.settle_thrown(); }
        for (owner, at, speed) in grapples { self.drag(owner, at, speed); }
        // Any team's rounds damage deployables: players can clear their own
        // team's walls and fields out of the way.
        for (hit, kind) in hits {
            let Some(round) = rounds.get(kind as usize) else { continue };
            match hit {
                Hit::Deployable(k) => self.damage_deployable(k, round.damage),
                Hit::Equipment(idx) => {
                    let defs = crate::equipment::definitions(self.map);
                    if self.equipment[idx].damage(&defs[idx], round.damage, true) { self.equipment_destroyed(&defs[idx]); }
                }
                Hit::Player(idx, owner, at, dir) => {
                    let name = if owner < self.players.len() || round.kill_turret.is_empty() { &round.kill } else { &round.kill_turret };
                    self.round_hit(idx, owner, round.damage, round.flash, name, &round.damage_kind, at, dir);
                    self.hit_extras(idx, owner, round);
                }
                Hit::Vehicle(v, owner) => vehicle_hits.push((v, round.damage, owner)),
            }
        }
        // Highest index first: a wreck removes its vehicle.
        vehicle_hits.sort_by(|a, b| b.0.cmp(&a.0));
        for (v, damage, owner) in vehicle_hits { self.damage_vehicle(v, damage, (owner < self.players.len()).then_some(owner)); }
        // Armed mines with an enemy in reach go off, and so do mines shot.
        let mut tripped = self.tripped_mines();
        for (a, b) in &shot_paths { tripped.extend(self.shot_mines(*a, *b)); }
        tripped.sort_unstable();
        tripped.dedup();
        tripped.sort_unstable();
        for k in tripped.into_iter().rev() {
            let d = self.discs.remove(k);
            explode.push((d.pos, d.owner, d.kind, d.team, None));
        }
        for (pos, owner, kind, team, direct) in explode {
            self.explode_direct(pos, owner, kind, team, direct);
        }
        for (pos, owner, kind, team) in water_bursts {
            self.explode_scaled(pos, owner, kind, team, None, crate::water::BURST_DAMAGE, crate::water::BURST_PUSH);
        }
    }

    #[cfg(test)]
    fn explode(&mut self, pos: Vec3, owner: usize, kind: u8, team: Team) {
        self.explode_direct(pos,owner,kind,team,None);
    }

    fn explode_direct(&mut self, pos: Vec3, owner: usize, kind: u8, team: Team, direct:Option<usize>) {
        self.explode_scaled(pos, owner, kind, team, direct, 1.0, 1.0);
    }

    /// A burst doing `scale` of its damage to everything it reaches and
    /// pushing people `push` times as hard (a disc bursting at water).
    fn explode_scaled(&mut self, pos: Vec3, owner: usize, kind: u8, team: Team, direct:Option<usize>, scale: f32, push: f32) {
        self.blast_serial += 1;
        // Mines close by go off with it, any team's (after this blast).
        let mut chained = self.sympathetic_mines(pos);
        chained.sort_unstable();
        let chained: Vec<Disc> = chained.into_iter().rev().map(|k| self.discs.remove(k)).collect();
        let round = crate::registry::registry().projectiles.get(kind as usize);
        let blast = crate::combat::blast_of(kind).unwrap_or(&FALLBACK_BLAST);
        // Linear (or concentrated) blasts against players; the quadratic
        // curve (the base turrets' plasma) is its own case below.
        let profile = (blast.falloff == crate::registry::Falloff::Linear).then(|| blast.profile());
        // Equipment and deployables keep the linear splash (the shield
        // time-to-break balance); players take the concentrated blasts.
        let structure=crate::combat::structure_blast(kind);
        let max_r = blast.radius;
        self.explosions.push(Explosion {
            pos,
            age: 0.0,
            max_r,
            kind,
        });
        self.spatial_sounds.push(("boom", pos));
        let dmg_core = blast.core_damage;
        // Vehicles take the blast like structures (`vehicles`).
        if !self.vehicles.is_empty() { self.blast_vehicles(pos, max_r, (dmg_core + blast.edge_damage) * scale, owner); }
        let mut wrecked=Vec::new();
        for (d,s) in crate::equipment::definitions(self.map).iter().zip(&mut self.equipment) {
            if d.team as usize==team.idx() || s.health<=0. {continue;}
            let delta=d.pos()-pos;let distance=(delta.length()-d.radius).max(0.);
            if distance>max_r {continue;}
            let end=d.pos()-delta.normalize_or_zero()*d.radius.min(delta.length());
            if !splash_reaches(self.map,&self.pillars,pos,end,d) {continue;}
            let damage=structure.map_or((dmg_core+blast.edge_damage)*(1.-distance/max_r),|p|p.damage(distance))*scale;
            if s.damage(d,damage,false) {wrecked.push(d.clone());}
        }
        for d in wrecked {self.equipment_destroyed(&d);}
        for k in 0..self.deployables.len() {
            // Friendly blasts hurt deployables too (clearing your own team's).
            let e = &self.deployables[k];
            let distance = (e.center().distance(pos) - 1.0).max(0.0);
            if distance > max_r { continue; }
            let damage = structure.map_or((dmg_core + blast.edge_damage) * (1.0 - distance / max_r), |p| p.damage(distance)) * scale;
            self.damage_deployable(k, damage);
        }
        let mut killed_by_player = false;
        let pid = self.player_id;
        for i in 0..self.players.len() {
            // A vehicle's riders are inside it, and it takes the blast;
            // an exposed seat's rider takes it too.
            if !self.players[i].alive || (self.players[i].riding > 0 && !self.rides_exposed(i)) {
                continue;
            }
            let origin_distance = self.players[i].pos.distance(pos);
            // Damage reaches the body, not just a point at the player's feet.
            // A confirmed impact touches its victim: full splash once, never
            // an additional direct-damage bonus. Other targets still fall off.
            let d = if profile.is_some() {
                if direct==Some(i) {0.} else {player_blast_distance(pos,self.players[i].pos)}
            } else {origin_distance};
            if d > max_r {
                continue;
            }
            // Impulse falls to zero at the blast edge, independently of the
            // nonzero edge damage. Use body contact distance, not the feet.
            let fall = (1.0 - d / max_r).clamp(0.0, 1.0);
            let same = !self.hostile(owner, team, i);
            if same && i != owner && !self.team_fire(owner, team, i) {
                continue;
            }
            let body = self.players[i].pos + Vec3::Y * 0.7;
            // Terrain and midfield pillars shield splash as well as projectiles.
            if obstacle_hit(self.map, &self.pillars, pos + Vec3::Y * 0.02, body, 0.0)
                .is_some_and(|t| t < 0.995) { continue; }
            // Your own disc hurts you at 40% (disc jumps stay possible);
            // your own explosives (grenades, mortars, mines) at full strength.
            let mul = if i != owner { if same { self.mode.def().friendly_fire } else { 1.0 } } else { round.map_or(1.0, |r| r.self_damage) };
            let dmg = if let Some(profile)=profile {profile.damage(d)*mul}
                else if blast.max_damage > 0.0 && direct==Some(i) {blast.max_damage}
                else {(blast.edge_damage * fall + dmg_core * fall * fall) * mul};
            // Kick from the blast, not from the chest, so a disc at your feet throws you up.
            let away = (body - pos).try_normalize().unwrap_or(Vec3::Y);
            // A plate on the front stops a blast from ahead (`packs`).
            let share = self.blocked_share(i, away);
            let dmg = if share < 1.0 { dmg * share } else { dmg } * scale;
            self.hit_by = (owner < self.players.len()).then_some(owner);
            self.hurt(i, dmg, round.map_or("blast", |r| r.damage_kind.as_str()));
            self.hit_by = None;
            // A blast's effect for whoever it catches (`registry::Projectile::effect`).
            if let Some(e) = round.and_then(|r| r.effect.as_deref()).and_then(|e| crate::registry::registry().effects.index(e)) {
                if dmg > 0.0 && !self.predicting { self.give_effect(i, e as u8); }
            }
            let heavy = if self.mode.def().armor_scaling { loadout::mass_scale(self.players[i].armor) } else { 1.0 };
            let kick = profile.map_or(blast.kick,|p|p.impulse/player_mass(&self.players[i])) / heavy;
            // A mid-air hit on someone else throws them hard.
            let airborne = if i != owner && !self.players[i].on_ground { crate::defs::movement().air_hit_kick } else { 1.0 };
            let push = away * kick * fall * airborne * push;
            self.players[i].vel += push;
            if push.y > 4.0 {
                self.players[i].on_ground = false;
                self.players[i].skiing = false;
                self.players[i].coyote = 0.0;
            }
            if i == pid {
                self.damage_flash = 0.7;
                self.trauma = (self.trauma + 0.35 * fall).min(1.0);
                self.push_event("pain");
            }
            if owner == pid && i != pid && !same {
                self.hitmarker = 1.0;
                self.push_event("hit");
            }
            if i != owner && !same {
                if let Some(p) = self.players.get_mut(owner) { p.hits += 1; }
            }
            if self.players[i].health <= 0.0 {
                if owner == pid && i != pid {
                    killed_by_player = true;
                }
                let name = round.map_or("Explosion", |r| if owner < self.players.len() || r.kill_turret.is_empty() { &r.kill } else { &r.kill_turret });
                self.kill(i, Some(owner), name);
            }
        }
        if killed_by_player {
            self.kills += 1;
            self.msg("FRAG", 1.1);
        }
        if owner == pid {
            self.trauma = (self.trauma + 0.12).min(1.0);
        }        for d in chained { self.explode_direct(d.pos, d.owner, d.kind, d.team, None); }
        self.lava_splash(pos, owner, kind, team);
    }

    /// A burst in or on lava throws it about: a second blast from the lava's
    /// surface (the `LAVA_SPLASH` projectile's), burning whoever it reaches
    /// and credited to the shooter. The map's own bombs and a splash itself
    /// throw none.
    fn lava_splash(&mut self, pos: Vec3, owner: usize, kind: u8, team: Team) {
        if owner == features::MAP_OWNER { return; }
        let Some(splash) = crate::registry::registry().projectiles.index(LAVA_SPLASH) else { return };
        if kind as usize == splash { return; }
        if let Some(surface) = crate::water::lava_splash_surface(self.map, pos) {
            self.explode_direct(Vec3::new(pos.x, surface + 0.1, pos.z), owner, splash as u8, team, None);
        }
    }

    /// Damage player `victim` by `amount` of damage kind `kind` (before
    /// armor) for `attacker`, killing them as `cause` should it be fatal.
    /// The way in for rules scripts' commands and for tests.
    pub fn damage(&mut self, victim: usize, amount: f32, kind: &str, attacker: Option<usize>, cause: &str) {
        if !self.players.get(victim).is_some_and(|p| p.alive) || !amount.is_finite() || amount <= 0.0 { return; }
        self.hit_by = attacker;
        self.hurt(victim, amount, kind);
        self.hit_by = None;
        if self.players[victim].health <= 0.0 { self.kill(victim, attacker.filter(|&a| a < self.players.len()), cause); }
    }

    fn kill(&mut self, i: usize, killer: Option<usize>, weapon: &str) {
        if !self.players[i].alive {
            return;
        }
        self.players[i].alive = false;
        if self.football() { self.body_down(i); }
        let killer = killer.filter(|&k| k < self.players.len());
        // A death in a blade zone counts for nothing (`blade`): no feed, no
        // score, and back soon on the zone's own spawns. So does any in a
        // duel server's lobby, which scores nothing (a fall off the plaza, a
        // bump, Ctrl+K).
        let blade_zone = if self.players[i].hidden() { None } else { features::blade_zone(self.map, self.players[i].pos) };
        self.players[i].blade.fell_in = blade_zone.map_or(0, |z| (z + 1).min(255) as u8);
        if blade_zone.is_some() || self.mode == crate::map_catalog::SupportedMode::DuelLobby {
            self.blade_death(i, blade_zone.is_some());
            return;
        }
        let entry = crate::feed::Entry::Frag {
            killer: killer.map(|k| self.display_name(k)).unwrap_or_else(|| {
                match weapon {
                    "Fall" | "Lava" | "Eruption" => "Environment".into(),
                    "Lightning" => "Spires".into(),
                    "Drain" | "Heat" => "Point".into(),
                    _ => "Turret".into(),
                }
            }),
            victim: self.display_name(i), weapon: weapon.into(),
        };
        crate::feed::push(&mut self.feed, entry);
        self.players[i].losses += 1;
        if let Some(k) = killer.filter(|&k| k != i && k < self.players.len()
            && self.hostile(k, self.players[k].team, i)) {
            self.players[k].frags += 1;
            self.earn_for(k, items::Earned::Frag);
        }
        if let Some(k) = killer { self.rule_event(rules::GameEvent::Frag { killer: k, victim: i }); }
        self.rule_event(rules::GameEvent::Died { victim: i, killer });
        self.rabbit_death(i);
        if self.hooks.live { self.hook(self.armor_hook(i), hooks::HookEvent::Death { victim: i, killer, cause: weapon.to_string() }); }
        self.after_death(i);
        self.players[i].respawn = self.mode.def().respawn;
        if i == self.player_id {
            self.deaths += 1;
            self.msg("YOU WERE FRAGGED", 2.2);
            self.push_event("death");
            self.trauma = 0.8;
        }
    }

    /// A death that counts for nothing, in a blade zone (`in_zone`, `blade`)
    /// or the duel lobby: what any death does to the body and what it
    /// carried, nothing to anyone's score.
    fn blade_death(&mut self, i: usize, in_zone: bool) {
        self.rabbit_death(i);
        self.after_death(i);
        self.players[i].respawn = if in_zone { crate::defs::blade().respawn } else { self.mode.def().respawn };
        if i == self.player_id {
            self.msg(if in_zone { "CUT DOWN" } else { "YOU DIED" }, 2.0);
            self.push_event("death");
            self.trauma = 0.6;
        }
    }

    /// What every death does to the body: the gear, the pack, a seat, a
    /// borrowed armor, what's lost on death, a carried flag or ball, the
    /// death pack, and a puff where they fell.
    fn after_death(&mut self, i: usize) {
        self.players[i].health = 0.0;
        self.players[i].repair_beam = None;
        self.players[i].repair_out = false;
        self.players[i].repair_self = false;
        self.players[i].effects.clear();
        self.players[i].stream = None;
        // What the mode has a death leave besides its rounds (`items`).
        self.death_drops(i);
        self.players[i].pack = None;
        self.players[i].pack_on = false;
        self.players[i].ability_on = false;
        self.unseat(i);
        // A borrowed armor goes back.
        let p = &mut self.players[i];
        if p.swap_back > 0 { p.armor = ArmorClass(p.swap_back - 1); p.swap_back = 0; p.swap_clock = 0.0; }
        // Weapons lost on death (the rifles) come back at respawn from the setup.
        let weapons = &crate::registry::registry().weapons;
        for k in self.players[i].loadout.kinds().collect::<Vec<_>>() {
            if weapons.get(k as usize).is_some_and(|w| w.lost_on_death) { self.players[i].loadout.remove(k); }
        }
        if weapons.get(self.players[i].weapon as usize).is_some_and(|w| w.lost_on_death) { self.players[i].weapon = 0; }
        self.players[i].jetting = false;
        self.players[i].skiing = false;
        if let Some(team) = self.players[i].carrying.take() {
            self.drop_flag(team, self.players[i].pos);
        }
        self.ball_lost(i);
        self.drop_loot(i);
        self.players[i].stun = 0.0;
        self.players[i].blade.put_away_all();
        self.explosions.push(Explosion {
            pos: self.players[i].pos + Vec3::Y,
            age: 0.0,
            max_r: 4.5,
            kind: 2,
        });
    }

    /// A dead body falls with the momentum it died with, skids to a stop
    /// on what it lands on, and floats on water and lies on lava (marker
    /// `fall1`). It used to hang where the player died until the respawn;
    /// `player_model` lays it down, and a pack it dropped rides it down
    /// (`items::step_carried_loot`).
    fn fall_body(&mut self, i: usize, dt: f32) {
        let map = self.map;
        let p = &self.players[i];
        if p.hidden() || p.riding > 0 || !p.pos.is_finite() || (p.on_ground && p.vel == Vec3::ZERO) { return; }
        let (wet, _) = crate::water::immersion(map, &self.staged_water, p.pos);
        let gravity = crate::defs::movement().gravity;
        let p = &mut self.players[i];
        p.vel.y -= gravity * dt;
        if wet > 0.0 {
            // Water slows a body and floats it, a little under.
            p.vel *= (1.0 - BODY_WATER_DRAG * wet * dt).max(0.0);
            p.vel.y += gravity * BODY_BUOYANCY * wet * dt;
        }
        p.on_ground = false;
        let _ = move_over_terrain(map, p, false, dt);
        p.skiing = false;
        // Skim water and lava bear a body as they bear a skier.
        if let Some(s) = crate::water::skim_surface_at(map, p.pos) {
            if p.pos.y < s + PLAYER_RADIUS {
                p.pos.y = s + PLAYER_RADIUS;
                p.vel.y = p.vel.y.max(0.0);
                p.on_ground = true;
            }
        }
        if p.on_ground {
            p.vel *= (1.0 - BODY_FRICTION * dt).max(0.0);
            // At rest on anything a player could stand on.
            if p.vel.length() < BODY_REST_SPEED && contact_normal(map, p.pos).y >= 0.7 { p.vel = Vec3::ZERO; }
        }
    }

    pub fn display_name(&self, i: usize) -> String {
        let p = &self.players[i];
        if !p.name.is_empty() { p.name.clone() }
        else if i == self.player_id { "You".into() }
        else { format!("Bot {}", i + 1) }
    }

    /// Whether feed lines call player `i` "you": the local player with no
    /// name (offline), whom `display_name` calls "You".
    pub fn is_you(&self, i: usize) -> bool { i == self.player_id && self.players.get(i).is_some_and(|p| p.name.is_empty()) }

    /// Player `i` as the object of a feed line: "you" (`is_you`), else
    /// `display_name` ("Ace tackles you").
    pub fn object_name(&self, i: usize) -> String { if self.is_you(i) { "you".into() } else { self.display_name(i) } }

    /// The verb agreeing with player `i` as a feed line's subject: `you`
    /// for `is_you` ("You pick up the ball"), else `they` ("Ace picks up").
    pub fn verb<'a>(&self, i: usize, they: &'a str, you: &'a str) -> &'a str { if self.is_you(i) { you } else { they } }

    fn respawn(&mut self, i: usize) {
        self.ball_lost(i);
        let ember = self.side(self.players[i].team) == Team::Ember;
        // Cut down in a blade zone: back on its own spawns (`blade`). An
        // arena's round starts from its duel spots (`duel`).
        let fell_in = std::mem::take(&mut self.players[i].blade.fell_in);
        // In the lobby's tower, people come in at any of its spawn points,
        // every floor's and outside's alike, and sparring bots on their own
        // floor (`blade`).
        let home = self.spar_home(i);
        let spawn = if let Some(spot) = home.and_then(|h| self.blade_spawn(i, h)) { Some(spot) }
            else if self.sparring() { self.lobby_spawn(i) }
            else if let Some(spot) = fell_in.checked_sub(1).and_then(|z| self.blade_spawn(i, z as usize)) { Some(spot) }
            else if let Some(spot) = self.duel_spot(i) { Some(spot) }
            else if self.ffa() { self.ffa_spawn(i) } else { self.pick_spawn(self.players[i].team) };
        let (pos, yaw) = match spawn {
            Some(spawn) => spawn,
            None => {
                let mut pos = self.stand(ember);
                pos.x += (self.rng() - 0.5) * 8.0;
                pos.z += (self.rng() - 0.5) * 6.0;
                pos.y = crate::terrain::support_on(self.map,pos).0 + 1.2;
                (pos, if ember { std::f32::consts::PI } else { 0.0 })
            }
        };
        // The setup chosen at an inventory station comes back, where the
        // mode sells it (`Setup`).
        let def = self.mode.def();
        let setup = if def.purchases { self.players[i].setup } else { Setup::default() };
        // With an economy, what they chose comes back only if it's paid for
        // again; else they spawn with their armor's kit (`items`).
        let unpaid = def.economy.is_some() && def.purchases && !self.pay(i, self.setup_cost(&setup));
        if unpaid && i == self.player_id { self.msg("NOT ENOUGH CREDITS FOR YOUR SETUP", 2.0); }
        let setup = if unpaid { Setup::default() } else { setup };
        if let Some(armor) = setup.armor { self.players[i].armor = armor; }
        // Unpaid: the first armor the mode offers.
        if unpaid {
            self.players[i].armor = def.offered_armors().first().copied().unwrap_or(ArmorClass::Light);
        }
        // A random armor each spawn where the mode says so.
        if def.spawn_armor == crate::modes::SpawnArmor::Random {
            let offered: Vec<ArmorClass> = crate::registry::registry().armors.iter().map(|(k, _, _)| ArmorClass(k as u8))
                .filter(|&a| def.offers_armor(a)).collect();
            if !offered.is_empty() {
                let k = ((self.rng() * offered.len() as f32) as usize).min(offered.len() - 1);
                self.players[i].armor = offered[k];
            }
        }
        // A mode that doesn't offer the armor worn: its first that fits.
        if !def.offers_armor(self.players[i].armor) {
            let offered = def.offered_armors();
            if let Some(&a) = offered.iter().find(|&&a| self.armor_fits(i, a)).or(offered.first()) { self.players[i].armor = a; }
        }
        // Bots wear what a mod's armors offer their role (`bot_roles`).
        if self.bot_armors && self.players[i].is_bot {
            let role = match self.players[i].bot_role { BotRole::Defense => "defense", BotRole::Hunter => "hunter", _ => "offense" };
            let fits: Vec<ArmorClass> = crate::registry::registry().armors.iter().map(|(k, _, _)| ArmorClass(k as u8))
                .filter(|&a| def.offers_armor(a) && a.def().bot_roles.iter().any(|r| r == role)).collect();
            if !fits.is_empty() { self.players[i].armor = fits[i % fits.len()]; }
        }
        // Never past an armor's team cap: the first the mode offers that fits.
        if !self.armor_fits(i, self.players[i].armor) {
            let fits = def.offered_armors().into_iter().find(|&a| self.armor_fits(i, a));
            self.players[i].armor = fits.unwrap_or(ArmorClass::Light);
        }
        let p = &mut self.players[i];
        p.pos = pos;
        p.vel = Vec3::ZERO;
        p.health = 100.0;
        p.energy = loadout::energy_max(p.armor);
        p.alive = true;
        p.pack = setup.pack.filter(|k| (def.deployables || k.deployable().is_none()) && k.fits(p.armor));
        (p.pack_on, p.pack_clock, p.pack_uses, p.ability_on, p.ability_clock, p.ability_uses, p.strain) = (false, 0.0, 0, false, 0.0, 0, 0.0);
        p.mount_clocks = [0.0; packs::MAX_MOUNTS];
        p.ammo = loadout::rounds_max(p);
        World::restock_throwables(p);
        p.throw_clock = 0.0;
        p.repair_beam = None;
        p.repair_out = false;
        p.repair_self = false;
        p.repair_rest = 0.0;
        p.carrying = None;
        p.yaw = yaw;
        p.pitch = 0.0;
        p.cooldown = 0.4;
        p.skiing = false;
        p.jetting = false;
        p.on_ground = true;
        p.coyote = 0.0;
        p.jump_prev = false;
        p.jet_prev = false;
        self.apply_twist(i);
        // The weapons chosen (or the default), for the armor and mode; the
        // kit when the setup wasn't paid for (it's kept for later).
        if unpaid {
            let kept = std::mem::take(&mut self.players[i].setup);
            self.equip(i);
            self.players[i].setup = kept;
        } else { self.equip(i); }
        if i == self.player_id {
            self.msg("REDEPLOYED", 1.4);
        }
        if self.hooks.live { self.hook(self.armor_hook(i), hooks::HookEvent::Spawn { player: i }); }
    }

    /// Flag tosses: a carrier who asks throws the flag ahead along their aim.
    fn step_tosses(&mut self) {
        if self.predicting { return; }
        for i in 0..self.players.len() {
            let toss = if self.players[i].is_bot { false }
                else if self.network_inputs.is_empty() { i == self.player_id && self.input.toss }
                else { self.network_inputs.get(i).is_some_and(|c| c.toss) };
            if toss { if self.rabbit_mode() { self.toss_rabbit(i); } else { self.toss_flag(i); } }
        }
    }

    /// Player `i` throws the flag they carry: it flies from their chest along
    /// their aim (never steeply down), keeping their velocity, until it lands;
    /// anyone may take or return it in the air, the thrower only after
    /// `TOSS_GUARD`. It returns home on the dropped flag's timer.
    pub fn toss_flag(&mut self, i: usize) {
        let p = &self.players[i];
        let Some(team) = p.carrying.filter(|_| p.alive) else { return };
        let dir = look_dir(p.yaw, p.pitch.max(-0.35));
        let (pos, vel) = (p.pos + Vec3::Y * 1.4 + dir * 0.6, p.vel + dir * TOSS_SPEED + Vec3::Y * TOSS_LIFT);
        self.players[i].carrying = None;
        let flag_return = self.mode.def().flag_return;
        let f = &mut self.flags[team.idx()];
        f.carrier = None;
        f.pos = pos;
        f.vel = vel;
        f.drop_timer = flag_return;
        f.tossed_by = Some(i);
        f.toss_guard = TOSS_GUARD;
        if i == self.player_id { self.push_event("throw"); }
        else if self.network_inputs.is_empty() { self.spatial_sounds.push(("throw", pos)); }
        self.push_event("drop");
    }

    /// Player `i` lets go of the flag they carry where they stand.
    pub(super) fn drop_carried_flag(&mut self, i: usize) {
        let Some(team) = self.players[i].carrying.take() else { return };
        let pos = self.players[i].pos;
        self.drop_flag(team, pos);
    }

    fn drop_flag(&mut self, team: Team, pos: Vec3) {
        let y = crate::terrain::support_on(self.map,pos).0 + 0.4;
        let flag_return = self.mode.def().flag_return;
        let f = &mut self.flags[team.idx()];
        f.carrier = None;
        f.pos = Vec3::new(pos.x, y, pos.z);
        f.drop_timer = flag_return;
        // Flag text is announced by the client from flag state (flag_announce.rs).
        self.push_event("drop");
    }

    fn step_flags(&mut self, before: &[Vec3], dt: f32) {
        for fi in 0..2 {
            if let Some(c) = self.flags[fi].carrier {
                if c < self.players.len() && self.players[c].alive {
                    self.flags[fi].pos = self.players[c].pos + Vec3::Y * 2.2;
                    continue;
                } else {
                    let pos = self.flags[fi].pos;
                    let team = self.flags[fi].team;
                    self.drop_flag(team, pos);
                }
            }
            self.flags[fi].toss_guard = (self.flags[fi].toss_guard - dt).max(0.0);
            if self.flags[fi].carrier.is_none() && self.flags[fi].vel != Vec3::ZERO {
                // A tossed flag flies until it meets the map, then lies there.
                let f = &self.flags[fi];
                let vel = f.vel - Vec3::Y * crate::defs::movement().gravity * dt;
                let next = f.pos + vel * dt;
                let far = self.map_size() - 1.0;
                let hit = obstacle_hit(self.map, &self.pillars, f.pos, next, 0.15)
                    .or_else(|| (next.x < 1.0 || next.z < 1.0 || next.x > far || next.z > far).then_some(0.0));
                let f = &mut self.flags[fi];
                match hit {
                    Some(t) => { f.pos = f.pos.lerp(next, t); f.vel = Vec3::ZERO; }
                    None => { f.pos = next; f.vel = vel; }
                }
            }
            if self.flags[fi].carrier.is_none() && self.flags[fi].drop_timer > 0.0 {
                self.flags[fi].drop_timer -= dt;
                if self.flags[fi].vel == Vec3::ZERO {
                    let pos = self.flags[fi].pos;
                    let y = crate::terrain::support_on(self.map,pos+Vec3::Y*PLAYER_RADIUS).0 + 0.4;
                    self.flags[fi].pos.y = y;
                }
                if self.flags[fi].drop_timer <= 0.0 {
                    self.return_flag(fi);
                }
            }
        }

        // Pickups / captures. Each flag is touchable up to its finial or the
        // first ceiling over it, so nobody takes one through a roof.
        let tops: [f32; 2] = [0, 1].map(|fi| flag_top(self.map, self.flags[fi].pos));
        let n = self.players.len();
        for i in 0..n {
            if !self.players[i].alive {
                continue;
            }
            let team = self.players[i].team;
            let pos = self.players[i].pos;
            let from = before.get(i).copied().unwrap_or(pos);
            let enemy = team.other().idx();
            let own = team.idx();

            // Return own dropped flag.
            if self.flags[own].carrier.is_none() {
                let at_home = self.flags[own].pos.distance(self.flags[own].home) < 2.5;
                if !at_home && touches_flag(self.flags[own].pos, tops[own], from, pos, FLAG_RETURN_REACH) {
                    self.return_flag(own);
                    self.earn_for(i, items::Earned::Return);
                }
            }

            // Grab enemy flag.
            let guarded = self.flags[enemy].tossed_by == Some(i) && self.flags[enemy].toss_guard > 0.0;
            // An effect may stop its bearer taking a flag (a cloak).
            let barred = !self.players[i].effects.is_empty() && status::bearing(&self.players[i].effects).no_flag;
            if self.players[i].carrying.is_none() && self.flags[enemy].carrier.is_none() && !guarded && !barred {
                if touches_flag(self.flags[enemy].pos, tops[enemy], from, pos, FLAG_GRAB_REACH) {
                    self.flags[enemy].carrier = Some(i);
                    self.flags[enemy].drop_timer = 0.0;
                    self.flags[enemy].vel = Vec3::ZERO;
                    self.players[i].carrying = Some(self.flags[enemy].team);
                    self.push_event("flag");
                }
            }

            // Capture.
            if self.players[i].carrying.is_some() {
                let own_home = self.flags[own].home;
                let own_at_home = self.flags[own].carrier.is_none()
                    && self.flags[own].pos.distance(own_home) < 3.0;
                if own_at_home && touches_flag(own_home, tops[own], from, pos, CAPTURE_REACH) {
                    self.capture(i);
                }
            }
        }
    }

    fn return_flag(&mut self, fi: usize) {
        let home = self.flags[fi].home;
        self.flags[fi].pos = home;
        self.flags[fi].carrier = None;
        self.flags[fi].drop_timer = 0.0;
        self.flags[fi].vel = Vec3::ZERO;
        self.push_event("return");
    }

    /// A capture: the carried flag goes home, and the rule set scores it.
    fn capture(&mut self, i: usize) {
        if let Some(ft) = self.players[i].carrying.take() {
            self.return_flag(ft.idx());
        }
        self.earn_for(i, items::Earned::Capture);
        self.rule_event(rules::GameEvent::FlagCaptured { player: i });
    }

    /// A generator's destruction is a visible, audible blast that hurts no one.
    /// It travels as an ordinary explosion (serial plus snapshot), so clients
    /// replaying a snapshot never see it twice.
    fn equipment_destroyed(&mut self, d:&crate::equipment::Definition) {
        if d.kind!=crate::equipment::Kind::Generator {return;}
        self.blast_serial+=1;
        self.explosions.push(Explosion {pos:d.pos()+Vec3::Y*2.,age:0.,max_r:GENERATOR_BLAST_RADIUS,kind:4});
        self.spatial_sounds.push(("boom",d.pos()));
    }

    fn step_explosions(&mut self, dt: f32) {
        for e in &mut self.explosions {
            e.age += dt;
        }
        self.explosions.retain(|e| e.age < 0.55);
    }

    fn msg(&mut self, s: &str, t: f32) {
        self.message = s.into();
        self.message_t = t;
    }

    /// Say `s` to player `i`, from a part of the game only a server runs (a
    /// server's slot 0 is a networked player like any): to someone playing
    /// online, as a notice their server sends; else on screen if it's this
    /// game's own player.
    fn tell(&mut self, i: usize, s: &str, t: f32) {
        if self.players.get(i).is_some_and(|p| p.remote && p.net_id != 0 && !p.is_bot) {
            if self.notices.len() < 64 { self.notices.push((i, s.into())); }
        } else if i == self.player_id { self.msg(s, t); }
    }

    fn push_event(&mut self, e: &str) {
        if !self.events.is_empty() {
            self.events.push(',');
        }
        self.events.push_str(e);
    }

    pub fn player_yaw(&self) -> f32 {
        self.players.get(self.player_id).map(|p| p.yaw).unwrap_or(0.0)
    }
    pub fn player_speed(&self) -> f32 {
        self.players
            .get(self.player_id)
            .map(|p| Vec2::new(p.vel.x, p.vel.z).length())
            .unwrap_or(0.0)
    }

    pub fn player_pos(&self) -> Vec3 {
        self.players
            .get(self.player_id)
            .map(|p| p.pos)
            .unwrap_or(Vec3::ZERO)
    }

    pub fn camera(&self) -> (Vec3, Vec3, f32) {
        if self.state != MatchState::Flyby {
            if let Some(view) = self.spectator_camera.filter(|_| self.spectating) { return view; }
        }
        if self.state == MatchState::Flyby || self.players.is_empty() {
            let t = self.flyby;
            let c = self.map_size() * 0.5;
            let span = (self.map_size() * 0.22).clamp(52.0, 380.0);
            let eye = Vec3::new(
                c + (t * 0.18).sin() * span * 0.6,
                crate::terrain::overview_height(self.map).max(self.pillars.iter()
                    .map(|p|self.ground(p.x,p.z)+p.h+28.).fold(f32::NEG_INFINITY,f32::max)),
                c + (t * 0.18).cos() * span,
            );
            let target = Vec3::new(c, self.ground(c, c) + 8.0, c);
            let dir = (target - eye).normalize_or_zero();
            return (eye, dir, 62.0);
        }
        let p = &self.players[self.player_id];
        // Looking through a camera or turret from a command station.
        if let Some(eye) = self.view_from.filter(|_| p.alive) { return (eye, look_dir(p.yaw, p.pitch), 72.0); }
        // Riding a vehicle: a chase view from behind it, along the rider's look.
        if let Some(c) = self.riding(self.player_id).and_then(|(v, _)| self.vehicles.get(v)).filter(|_| p.alive) {
            let half = c.def().map_or(Vec3::ONE, |d| Vec3::from(d.size));
            let look = look_dir(p.yaw, p.pitch);
            let body = c.pos + Vec3::Y * (half.y + 1.4);
            let want = body - look * (half.z * 2.0 + 7.0);
            let eye = crate::map_pack::on(self.map).and_then(|pack| pack.sweep(body, want, 0.35))
                .map_or(want, |(t, _)| body.lerp(want, (t - 0.01).max(0.0)));
            return (eye, look, 74.0);
        }
        // Tackled: the view leaves your eyes and orbits your body on the
        // ground until you get up.
        if p.alive && p.stun > 0.0 {
            let a = p.yaw + (football::TACKLE_STUN - p.stun) * 0.9;
            let body = p.pos + Vec3::Y * 0.3;
            // Orbit 8 m out.
            let want = body + Vec3::new(a.sin() * 7.0, 3.9, a.cos() * 7.0);
            let eye = crate::map_pack::on(self.map)
                .and_then(|pack| pack.sweep(body, want, 0.35))
                .map_or(want, |(t, _)| body.lerp(want, (t - 0.001).max(0.0)));
            return (eye, (body - eye).normalize_or(Vec3::NEG_Z), 72.0);
        }
        let shake = self.trauma * self.trauma;
        let t = self.time;
        let off = Vec3::new(
            (t * 37.1).sin() * shake * 0.18,
            (t * 41.7).cos() * shake * 0.14,
            (t * 29.3).sin() * shake * 0.12,
        );
        let spd = Vec2::new(p.vel.x, p.vel.z).length();
        let bob = if p.on_ground && !p.skiing && spd > 2.0 && p.alive {
            (self.time * 9.5).sin() * 0.04
        } else {
            0.0
        };
        let body = p.pos + Vec3::Y * EYE;
        // Drawn between fixed steps (render_offset); the sweep below starts
        // from the real body, so interpolation can't carry the eye into a wall.
        let anchor = body + self.render_offset;
        let desired = anchor + off + Vec3::Y * bob + self.net_camera_offset;
        // Visual reconciliation and shake must not move the near plane through
        // a wall even when the authoritative collision body is safely inside.
        let eye = crate::map_pack::on(self.map)
            .and_then(|pack|pack.sweep(body,desired,0.35))
            .map_or(desired,|(t,_)|body.lerp(desired,(t-0.001).max(0.)));
        let dir = look_dir(p.yaw, p.pitch);
        if self.zoom_active() { return (eye, dir, self.zoom_fov()); }
        let fov = camera_fov(spd);
        if self.third_person && p.alive {
            // Over the right shoulder: out to the shoulder, then back along
            // the look, each leg pulled in off any wall so the camera never
            // ends up inside one. The view stays parallel to the look; the
            // HUD puts the crosshair where the shot goes (`aim_line`).
            let right = Vec3::new(p.yaw.cos(), 0.0, -p.yaw.sin());
            let pull = |from: Vec3, to: Vec3| {
                let to = crate::map_pack::on(self.map).and_then(|pack| pack.sweep(from, to, 0.35))
                    .map_or(to, |(t, _)| from.lerp(to, (t - 0.02).max(0.0)));
                match obstacle_hit(self.map, &self.pillars, from, to, 0.3) {
                    Some(t) => from.lerp(to, (t - 0.02).max(0.0)),
                    None => to,
                }
            };
            let shoulder = pull(anchor, anchor + right * THIRD_PERSON_RIGHT + Vec3::Y * THIRD_PERSON_UP);
            return (pull(shoulder, shoulder - dir * THIRD_PERSON_BACK), dir, fov);
        }
        (eye, dir, fov)
    }

    pub fn hud_json(&self) -> String {
        let p = self.players.get(self.player_id);
        let health = p.map(|x| x.health.max(0.0)).unwrap_or(0.0);
        let energy = p.map(|x| x.energy).unwrap_or(0.0);
        let speed = self.player_speed();
        let yaw = self.player_yaw();
        let pitch = p.map(|x| x.pitch).unwrap_or(0.0);
        let pos = self.player_pos();
        let team = p.map(|x| x.team.idx()).unwrap_or(0);
        let weapon = p.map(|x| x.weapon).unwrap_or(0);
        let carrying = p.and_then(|x| x.carrying).map(|t| t.idx() as i32).unwrap_or(-1);
        let on_g = p.map(|x| if x.on_ground { 1 } else { 0 }).unwrap_or(0);
        let ski = p.map(|x| if x.skiing { 1 } else { 0 }).unwrap_or(0);
        let jet = p.map(|x| if x.jetting { 1 } else { 0 }).unwrap_or(0);
        let alive = p.map(|x| if x.alive { 1 } else { 0 }).unwrap_or(0);
        let cd = p.map(|x| x.cooldown).unwrap_or(0.0);
        // The armor's wire number (`ArmorClass`).
        let armor = p.map_or(0, |x| x.armor.0);
        // Weapons paid with energy show shots left in the energy tank.
        let weapons = &crate::registry::registry().weapons;
        let energy_cost = |x: &Player| weapons.get(x.weapon as usize).map(|w| w.energy).filter(|&e| e > 0.0);
        let ammo = p.map(|x| match energy_cost(x) { Some(c) => (x.energy / c) as u16, None => x.ammo.get(x.weapon as usize).copied().unwrap_or(0) }).unwrap_or(0);
        let ammo_max = p.map(|x| match energy_cost(x) { Some(c) => (loadout::energy_max(x.armor) / c) as u16,
            None => loadout::rounds_max(x).get(x.weapon as usize).copied().unwrap_or(0) }).unwrap_or(0);
        // A carried weapon that zooms: 1 one paid with energy, 2 one with rounds.
        let rifle = p.map_or(0, |x| x.loadout.kinds().filter_map(|k| weapons.get(k as usize)).find(|w| w.zoom)
            .map_or(0, |w| if w.energy > 0.0 { 1 } else { 2 }));
        // What the mode takes out of play (`ModeDef::banned`) shows as none.
        let out = |x: &Player, slot: usize| throwables::banned(self.mode.def(), x.armor, slot);
        let (grenades, mines) = p.map_or((0, 0), |x| (if out(x, 0) { 0 } else { x.throwables[0] }, if out(x, 1) { 0 } else { x.throwables[1] }));
        let throw_max = p.map_or([0, 0], |x| { let m = throwables::max_throwables(x.armor); [if out(x, 0) { 0 } else { m[0] }, if out(x, 1) { 0 } else { m[1] }] });
        let pack = p.and_then(|x| x.pack).map_or(-1, |k| k.0 as i32);
        // 1 the repair tool running, 2 held out but resting on an empty tank.
        let repairing = p.map_or(0, |x| if x.repair_beam.is_some() { 1 } else if x.repair_out { 2 } else { 0 });
        // 1 an inventory station, 2 an ammo station, 3 a field inventory, 4
        // beside or behind a station (it opens from the front).
        let station = if self.players.is_empty() { 0 } else {
            let (fixed, field) = (self.inventory_reach(self.player_id), self.field_inventory_reach(self.player_id));
            if fixed == Some(true) { 1 } else if field == Some(true) { 3 }
            else if self.at_ammo_station(self.player_id) { 2 } else if fixed.is_some() || field.is_some() { 4 } else { 0 }
        };
        // 1 home, 2 carried, 3 dropped somewhere on the map.
        let flag_state = |f: &Flag| if f.carrier.is_none() && f.pos.distance(f.home) < 3.0 { 1 }
            else if f.carrier.is_some() { 2 } else { 3 };
        let own_flag_home = self.flags.get(team).map(flag_state).unwrap_or(1);
        let enemy_flag = self.flags.get(1 - team.min(1)).map(flag_state).unwrap_or(1);
        let winner = if self.state == MatchState::Ended {
            if self.score[0] > self.score[1] {
                0
            } else if self.score[1] > self.score[0] {
                1
            } else {
                -1
            }
        } else {
            -2
        };
        let mut blips = String::new();
        let me = self.players.get(self.player_id).map(|p| p.team);
        for (i, pl) in self.players.iter().enumerate() {
            // Not those a snapshot hid, nor enemies sensors can't pick out.
            if !pl.alive || pl.hidden() || (i != self.player_id && (Some(pl.team) != me || self.ffa()) && status::sensor_hidden(&pl.effects)) {
                continue;
            }
            if !blips.is_empty() {
                blips.push(';');
            }
            blips.push_str(&format!(
                "{:.0},{:.0},{},{}",
                pl.pos.x,
                pl.pos.z,
                pl.team.idx(),
                if i == self.player_id { 1 } else { 0 }
            ));
        }
        for f in self.flags.iter().filter(|_| self.mode.rules() == crate::modes::Rules::Ctf) {
            blips.push_str(&format!(
                ";{:.0},{:.0},{},2",
                f.pos.x,
                f.pos.z,
                f.team.idx()
            ));
        }
        let msg = self.equipment_prompt().unwrap_or_else(||self.message.clone()).replace('"', "");
        format!(
            "{{\"health\":{:.1},\"energy\":{:.1},\"speed\":{:.1},\"yaw\":{:.4},\"pitch\":{:.4},\"px\":{:.2},\"py\":{:.2},\"pz\":{:.2},\"ember\":{},\"glacier\":{},\"time\":{:.1},\"state\":{},\"weapon\":{},\"flag\":{},\"ownFlag\":{},\"hit\":{:.2},\"flash\":{:.2},\"msg\":\"{}\",\"kills\":{},\"deaths\":{},\"winner\":{},\"team\":{},\"onGround\":{},\"ski\":{},\"jet\":{},\"alive\":{},\"cd\":{:.2},\"events\":\"{}\",\"blips\":\"{}\",\"mapSize\":{:.0},\"armor\":{},\"ammo\":{},\"ammoMax\":{},\"pack\":{},\"repairing\":{},\"station\":{},\"grenades\":{},\"mines\":{},\"grenadesMax\":{},\"minesMax\":{},\"enemyFlag\":{},\"rifle\":{}}}",
            health,
            energy,
            speed,
            yaw,
            pitch,
            pos.x,
            pos.y,
            pos.z,
            self.score[0],
            self.score[1],
            self.time_left,
            self.state as u8,
            weapon,
            carrying,
            own_flag_home,
            self.hitmarker,
            self.damage_flash,
            msg,
            self.kills,
            self.deaths,
            winner,
            team,
            on_g,
            ski,
            jet,
            alive,
            cd,
            self.events,
            blips,
            self.map_size(),
            armor,
            ammo,
            ammo_max,
            pack,
            repairing,
            station,
            grenades,
            mines,
            throw_max[0],
            throw_max[1],
            enemy_flag,
            rifle
        )
    }
}

/// How close (horizontally, body centre to pole) a body must come to a flag
/// to grab it, return it, or capture at its stand.
const FLAG_GRAB_REACH: f32 = 2.6;
const FLAG_RETURN_REACH: f32 = 2.4;
const CAPTURE_REACH: f32 = 4.2;
/// A flag is touchable from just under its base to its finial (the pole is
/// drawn 3.5 m tall), so a body passing through the cloth takes it.
const FLAG_BELOW: f32 = 0.3;
const FLAG_TOP: f32 = 3.6;
/// A body moving further than this in one step respawned or teleported: only
/// where it ended up counts, not a sweep across the map.
const FLAG_SWEEP_MAX: f32 = 8.0;

/// How far up a flag at `flag` can be touched: its finial, or less under a
/// lower ceiling (a flag dropped in a tunnel isn't reachable from above ground).
fn flag_top(map: MapId, flag: Vec3) -> f32 {
    const START: f32 = 0.1;
    crate::map_pack::on(map)
        .and_then(|pack| pack.sweep(flag + Vec3::Y * START, flag + Vec3::Y * FLAG_TOP, 0.05))
        .map_or(FLAG_TOP, |(t, _)| START + t * (FLAG_TOP - START))
}

/// Whether a body that moved from `from` to `to` (player centres) this step
/// touched the flag standing at `flag`, touchable up to `top` over its base:
/// at some moment it was within `reach` of the pole horizontally while the
/// body (feet to head) overlapped the pole vertically. Swept, so a fast
/// skier can't pass through between ticks.
fn touches_flag(flag: Vec3, top: f32, from: Vec3, to: Vec3, reach: f32) -> bool {
    let from = if from.distance(to) > FLAG_SWEEP_MAX { to } else { from };
    // Moments (t in 0..1 along the step) within reach horizontally.
    let path = Vec2::new(to.x - from.x, to.z - from.z);
    let rel = Vec2::new(flag.x - from.x, flag.z - from.z);
    let a = path.length_squared();
    let (mut lo, mut hi) = if a < 1e-9 {
        if rel.length_squared() < reach * reach { (0.0, 1.0) } else { return false }
    } else {
        let b = -2.0 * rel.dot(path);
        let c = rel.length_squared() - reach * reach;
        let disc = b * b - 4.0 * a * c;
        if disc <= 0.0 { return false; }
        let root = disc.sqrt();
        ((-b - root) / (2.0 * a), (-b + root) / (2.0 * a))
    };
    lo = lo.max(0.0);
    hi = hi.min(1.0);
    // Narrow to the moments the body overlaps the pole: the centre between
    // `low` and `high`, from the feet (a radius under the centre) touching
    // the finial down to the head (BODY_HEIGHT over the feet) at the base.
    let radius = crate::terrain::PLAYER_RADIUS;
    let (low, high) = (flag.y - FLAG_BELOW - bump::BODY_HEIGHT + radius, flag.y + top + radius);
    let rise = to.y - from.y;
    if rise.abs() < 1e-6 {
        if from.y <= low || from.y >= high { return false; }
    } else {
        let (ta, tb) = ((low - from.y) / rise, (high - from.y) / rise);
        lo = lo.max(ta.min(tb));
        hi = hi.min(ta.max(tb));
    }
    lo <= hi
}

#[cfg(test)]
mod flag_touch_tests {
    use super::*;

    const R: f32 = crate::terrain::PLAYER_RADIUS;

    /// Ember player 0 carrying Glacier's flag on open ground, facing -z,
    /// with player 1 (Ember) `ahead` metres in front.
    fn carrier(ahead: f32) -> World {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);
        w.players.truncate(2);
        let spot = Vec3::new(1060.0, 0.0, 700.0);
        for (i, dz) in [(0, 0.0), (1, -ahead)] {
            let at = spot + Vec3::new(0.0, 0.0, dz);
            let floor = crate::terrain::support_on(MapId::OldHoller, at + Vec3::Y * 200.0).0;
            let p = &mut w.players[i];
            (p.team, p.is_bot, p.alive) = (Team::Ember, false, true);
            p.pos = Vec3::new(at.x, floor + R, at.z);
            p.vel = Vec3::ZERO;
            (p.yaw, p.pitch) = (0.0, 0.1);
        }
        let g = Team::Glacier.idx();
        w.flags[g].carrier = Some(0);
        w.players[0].carrying = Some(Team::Glacier);
        w
    }

    #[test]
    fn a_carrier_tosses_the_flag_ahead_where_it_lands_and_lies() {
        let mut w = carrier(80.0);
        let g = Team::Glacier.idx();
        let start = w.players[0].pos;
        w.input.toss = true;
        w.physics_step();
        w.input.toss = false;
        assert_eq!((w.flags[g].carrier, w.players[0].carrying), (None, None), "thrown");
        assert!(w.flags[g].vel.length() > TOSS_SPEED * 0.8, "flying: {:?}", w.flags[g].vel);
        for _ in 0..180 { w.physics_step(); }
        let f = &w.flags[g];
        assert_eq!(f.vel, Vec3::ZERO, "landed");
        let flown = start.z - f.pos.z;
        assert!(flown > 8.0 && flown < 40.0, "ahead: {flown}");
        let floor = crate::terrain::support_on(MapId::OldHoller, f.pos + Vec3::Y * R).0;
        assert!((f.pos.y - (floor + 0.4)).abs() < 0.05, "lies on the ground");
        assert!(f.carrier.is_none() && f.drop_timer > 0.0, "returns on the dropped timer");
        // Without a flag nothing happens.
        let mut w = carrier(80.0);
        w.players[0].carrying = None;
        w.flags[g].carrier = None;
        let before = w.flags[g].pos;
        w.input.toss = true;
        w.physics_step();
        assert_eq!(w.flags[g].pos, before);
    }

    #[test]
    fn a_teammate_in_its_path_catches_a_tossed_flag_but_the_thrower_waits() {
        let mut w = carrier(5.0);
        let g = Team::Glacier.idx();
        w.input.toss = true;
        w.physics_step();
        w.input.toss = false;
        for _ in 0..60 {
            w.physics_step();
            if w.flags[g].carrier.is_some() { break; }
        }
        assert_eq!((w.flags[g].carrier, w.players[1].carrying), (Some(1), Some(Team::Glacier)), "caught");
        // The thrower standing right where it lands can't take it back at once.
        let mut w = carrier(80.0);
        w.input.toss = true;
        w.physics_step();
        w.input.toss = false;
        w.flags[g].vel = Vec3::ZERO;
        w.flags[g].pos = w.players[0].pos + Vec3::new(0.0, 0.1, -0.5);
        w.physics_step();
        assert_eq!(w.flags[g].carrier, None, "guarded for a moment");
        for _ in 0..80 { w.physics_step(); }
        assert_eq!(w.flags[g].carrier, Some(0), "then theirs again");
    }

    #[test]
    fn a_body_touches_the_whole_pole_not_just_its_base() {
        let flag = Vec3::new(100.0, 50.0, 100.0);
        let at = |dx: f32, feet_over_base: f32| flag + Vec3::new(dx, feet_over_base + R, 0.0);
        // Standing beside it.
        assert!(touches_flag(flag, FLAG_TOP, at(1.5, 0.0), at(1.5, 0.0), FLAG_GRAB_REACH));
        // Feet 2.6 m over the base, through the cloth: the old 2.6 m sphere
        // round the base missed this (centre 3.1 m away).
        assert!(at(0.6, 2.6).distance(flag) > FLAG_GRAB_REACH);
        assert!(touches_flag(flag, FLAG_TOP, at(0.6, 2.6), at(0.6, 2.6), FLAG_GRAB_REACH));
        // Clearly over the finial, or with the head under the base: no.
        assert!(!touches_flag(flag, FLAG_TOP, at(0.0, 4.0), at(0.0, 4.0), FLAG_GRAB_REACH));
        assert!(!touches_flag(flag, FLAG_TOP, at(0.0, -3.0), at(0.0, -3.0), FLAG_GRAB_REACH));
        // Out of reach sideways.
        assert!(!touches_flag(flag, FLAG_TOP, at(2.8, 0.0), at(2.8, 0.0), FLAG_GRAB_REACH));
    }

    #[test]
    fn a_fast_skier_cannot_pass_through_between_ticks() {
        let flag = Vec3::new(100.0, 50.0, 100.0);
        // A 6 m step (360 m/s is far beyond a real skier, the point is the
        // sweep) passing 1 m from the pole: both ends are out of reach.
        let from = flag + Vec3::new(-3.0, R, 1.0);
        let to = flag + Vec3::new(3.0, R, 1.0);
        assert!(from.distance(flag) > FLAG_GRAB_REACH && to.distance(flag) > FLAG_GRAB_REACH);
        assert!(touches_flag(flag, FLAG_TOP, from, to, FLAG_GRAB_REACH));
        // Diving past: over the finial at the start, at the base by the end
        // but already gone by then.
        let dive_from = flag + Vec3::new(-2.0, 5.0 + R, 0.0);
        let dive_to = flag + Vec3::new(4.0, 0.0 + R, 0.0);
        assert!(touches_flag(flag, FLAG_TOP, dive_from, dive_to, FLAG_GRAB_REACH), "passes through the pole mid-step");
        // Climbing over it too high the whole way: no.
        assert!(!touches_flag(flag, FLAG_TOP, flag + Vec3::new(-3.0, 4.2 + R, 0.0), flag + Vec3::new(3.0, 4.6 + R, 0.0), FLAG_GRAB_REACH));
    }

    #[test]
    fn a_low_ceiling_stops_a_grab_from_above() {
        let flag = Vec3::new(100.0, 50.0, 100.0);
        // A flag dropped in a 3 m tunnel under a 0.4 m roof: someone standing
        // on the ground above is within the pole's height but not its reach.
        let above = flag + Vec3::new(0.5, 3.4 + R, 0.0);
        assert!(touches_flag(flag, FLAG_TOP, above, above, FLAG_GRAB_REACH), "without the ceiling it would");
        assert!(!touches_flag(flag, 3.0, above, above, FLAG_GRAB_REACH));
        // Inside the tunnel it still can.
        let inside = flag + Vec3::new(0.5, R, 0.0);
        assert!(touches_flag(flag, 3.0, inside, inside, FLAG_GRAB_REACH));
    }

    #[test]
    fn every_flag_stand_is_touchable_to_the_finial() {
        for m in crate::terrain::maps() {
            let Some(pack) = crate::map_pack::on(m.id) else { continue };
            for f in pack.manifest.flags {
                assert!(flag_top(m.id, Vec3::from_array(f)) >= FLAG_TOP - 1e-3, "{:?} {f:?}: ceiling over the flag", m.id);
            }
        }
    }

    #[test]
    fn a_respawn_is_not_swept_across_the_map() {
        let flag = Vec3::new(100.0, 50.0, 100.0);
        // Died on one side, respawned on the other: the straight line between
        // runs through the flag, but nobody travelled it.
        let from = flag + Vec3::new(-150.0, R, 0.0);
        let to = flag + Vec3::new(150.0, R, 0.0);
        assert!(!touches_flag(flag, FLAG_TOP, from, to, FLAG_GRAB_REACH));
    }

    #[test]
    fn a_player_passing_through_the_cloth_takes_the_flag() {
        let mut w = World::new();
        w.start_match(true);
        for _ in 0..30 { w.tick(STEP); }
        let me = w.player_id;
        let enemy = w.players[me].team.other().idx();
        let home = w.flags[enemy].home;
        // Feet 2.6 m over the flag's base, half a metre off the pole: in the
        // cloth, where the old check missed.
        w.players[me].pos = home + Vec3::new(0.5, 2.6 + R, 0.0);
        w.players[me].vel = Vec3::ZERO;
        w.tick(STEP);
        assert_eq!(w.players[me].carrying, Some(w.flags[enemy].team), "took the flag through its cloth");
    }
}

#[cfg(test)]
mod controls {
    #[test]
    fn walking_fov_is_fixed_and_skiing_curve_is_gentle() {
        for speed in [0.,0.1,1.,5.,10.,15.,20.] {assert_eq!(super::camera_fov(speed),76.);}
        let mut last=76.;
        for speed in 21..=200 {
            let fov=super::camera_fov(speed as f32);
            assert!(fov>=last && fov<=88.);
            assert!(fov-last<0.2);
            last=fov;
        }
    }
    use super::*;

    fn solo_airborne() -> World {
        let mut world = World::new();
        world.start_match(true);
        world.players.truncate(1);
        let p = &mut world.players[0];
        p.pos = Vec3::new(128.0, 150.0, 128.0);
        p.vel = Vec3::ZERO;
        p.yaw = 0.0;
        p.pitch = 0.0;
        p.on_ground = false;
        world
    }

    #[test]
    fn bullets_hit_one_target_without_splash_or_expiry_explosions() {
        let mut world = solo_airborne();
        for x in [128.0, 130.0] {
            world.players.push(make_player(Team::Glacier, false,
                Vec3::new(x, 150.0, 120.0), 0.0, BotRole::Defense));
        }
        world.discs.push(Disc { pos: Vec3::new(128.0, 150.9, 125.0),
            vel: Vec3::new(0.0, 0.0, -1000.0), team: Team::Ember,
            owner: 0, life: 1.0, kind: 1, spin: 0.0, target: 0, power: 1.0 });
        world.step_discs(STEP);
        assert_eq!(world.players[1].health, 92.0);
        assert_eq!(world.players[2].health, 100.0);
        assert!(world.explosions.is_empty());
        assert!(world.discs.is_empty());
        world.players[0].weapon = 1;
        world.shoot(0);
        world.discs[0].life = 0.001;
        world.step_discs(STEP);
        assert!(world.explosions.is_empty());
        assert!(world.discs.is_empty());
    }

    #[test]
    fn grenade_bounces_then_explodes_once_at_fuse_end() {
        for map in [MapId::Valley, MapId::OldHoller] {
            let mut world = solo_airborne();
            world.set_map(map);
            let home = crate::terrain::spawn_on(map,true);
            let ground = crate::terrain::height_on(map, home.x, home.z);
            world.discs.push(Disc { pos: Vec3::new(home.x, ground + 0.25, home.z),
                vel: Vec3::new(0.0, -30.0, 0.0), team: Team::Ember,
                owner: 0, life: 2.0, kind: 2, spin: 0.0, target: 0, power: 1.0 });
            world.step_discs(STEP);
            assert_eq!(world.discs.len(), 1);
            assert!(world.discs[0].vel.y > 0.0);
            assert!(world.explosions.is_empty());
            world.discs[0].life = 0.001;
            world.step_discs(STEP);
            assert!(world.discs.is_empty());
            assert_eq!(world.explosions.len(), 1);
            world.step_discs(STEP);
            assert_eq!(world.explosions.len(), 1);
        }
    }

    #[test]
    fn pillar_stops_bullets_and_bounces_grenades_without_detonation() {
        for kind in [1, 2] {
            let mut world = solo_airborne();
            world.pillars = vec![Pillar { x: 128.0, z: 124.0, r: 1.0, h: 200.0 }];
            world.discs.push(Disc { pos: Vec3::new(128.0, 150.0, 126.0),
                vel: Vec3::new(0.0, 0.0, -400.0), team: Team::Ember,
                owner: 0, life: 2.0, kind, spin: 0.0, target: 0, power: 1.0 });
            world.step_discs(STEP);
            assert!(world.explosions.is_empty());
            if kind == 1 { assert!(world.discs.is_empty()); }
            else { assert_eq!(world.discs.len(), 1); assert!(world.discs[0].vel.z > 0.0); }
        }
    }

    #[test]
    fn armed_grenade_detonates_on_contact_and_smoke_outlives_it() {
        let mut world = solo_airborne();
        world.pillars = vec![Pillar { x: 128.0, z: 124.0, r: 1.0, h: 200.0 }];
        world.discs.push(Disc { pos: Vec3::new(128.0, 150.0, 126.0),
            vel: Vec3::new(0.0, 0.0, -400.0), team: Team::Ember,
            owner: 0, life: 1.5, kind: 2, spin: 0.0, target: 0, power: 1.0 });
        world.step_discs(STEP);
        assert!(world.discs.is_empty());
        assert_eq!(world.explosions.len(), 1);
        assert!(!world.smoke.is_empty());
        world.step_discs(0.6);
        assert!(world.smoke.is_empty());
        assert_eq!(world.explosions.len(), 1);
    }

    #[test]
    fn grenade_arcs_and_third_slot_can_fire() {
        let mut world = solo_airborne();
        world.input.weapon = 2;
        world.input.fire = true;
        step(&mut world, 1);
        world.input.fire = false;
        assert_eq!(world.players[0].weapon, 2);
        assert_eq!(world.discs[0].kind, 2);
        let vy = world.discs[0].vel.y;
        world.step_discs(0.1);
        assert!(world.discs[0].vel.y < vy - 1.0);
        assert!(world.explosions.is_empty());
    }

    #[test]
    fn landing_damage_is_halved_and_rounded_down() {
        for (speed, damage) in [(0.0, 0.0), (18.0, 0.0), (19.0, 0.0),
            (20.0, 1.0), (23.0, 3.0), (30.0, 8.0), (80.0, 11.0)] {
            assert_eq!(landing_damage(speed), damage);
        }
    }

    #[test]
    fn a_fast_landing_is_resolved_before_the_frame_is_drawn() {
        for map in [MapId::Valley, MapId::OldHoller] {
            let mut world = solo_airborne();
            world.set_map(map);
            let home = crate::terrain::spawn_on(map,true);
            let floor = crate::terrain::height_on(map, home.x, home.z) + PLAYER_RADIUS;
            world.players[0].pos = Vec3::new(home.x, floor + 0.25, home.z);
            world.players[0].vel = Vec3::new(50.0, -80.0, 0.0);
            world.input.jump = true;
            step(&mut world, 1);
            let p = &world.players[0];
            let floor = crate::terrain::height_on(map, p.pos.x, p.pos.z) + PLAYER_RADIUS;
            assert!(p.pos.y >= floor - 0.001, "{map:?}: a fast landing must not render below ground");
            assert!(p.vel.is_finite() && p.pos.is_finite());
        }
    }

    #[test]
    fn fast_discs_hit_the_nearest_target_between_sample_points() {
        let mut world = solo_airborne();
        for z in [118.0, 125.0] {
            world.players.push(make_player(Team::Glacier, false,
                Vec3::new(128.0, 150.0 + EYE - 0.9, z), 0.0, BotRole::Defense));
        }
        world.shoot(0);
        world.discs[0].vel = Vec3::new(0.0, 0.0, -2000.0);
        world.step_discs(STEP);
        assert!(world.discs.is_empty());
        assert_eq!(world.explosions.len(), 1);
        assert!(world.explosions[0].pos.z > 125.0, "nearest target wins regardless of player list order");
        assert!(world.players[2].health < 100.0);
    }

    #[test]
    fn pillars_stop_discs_and_shield_targets_from_splash() {
        let mut world = solo_airborne();
        world.pillars = vec![Pillar { x: 128.0, z: 124.0, r: 1.0, h: 200.0 }];
        world.players.push(make_player(Team::Glacier, false,
            Vec3::new(128.0, 150.0, 122.0), 0.0, BotRole::Defense));
        world.shoot(0);
        world.discs[0].vel = Vec3::new(0.0, 0.0, -2000.0);
        world.step_discs(STEP);
        assert!(world.discs.is_empty());
        assert!(world.explosions[0].pos.z > 124.8);
        assert_eq!(world.players[1].health, 100.0, "solid cover must shield splash");
    }

    #[test]
    fn disc_aim_stays_on_the_eye_line_and_does_not_drift_up() {
        let mut world = solo_airborne();
        let initial_pitch = 0.3;
        world.players[0].pitch = initial_pitch;
        for _ in 0..5 { world.shoot(0); }
        assert_eq!(world.players[0].pitch, initial_pitch);
        let p = &world.players[0];
        let eye = p.pos + Vec3::Y * EYE;
        let dir = look_dir(0.0, initial_pitch);
        let d = &world.discs[0];
        let (right, up) = view_basis(dir);
        let rel = d.pos - eye;
        assert!(rel.dot(up) < -0.1, "the disc should leave below the eye, at the gun");
        assert!(rel.dot(right) > 0.1, "the disc should leave from the right-hand weapon");
        let aim = (eye + dir * 80.0 - d.pos).normalize();
        assert!(d.vel.normalize().dot(aim) > 0.999, "the shot still converges on the crosshair");
    }

    #[test]
    fn linear_splash_retains_edge_damage_but_stops_outside_radius() {
        let mut world = solo_airborne();
        let blast = world.players[0].pos + Vec3::X * (crate::defs::defs().blasts.disc.radius + PLAYER_RADIUS - 0.01);
        world.explode(blast, usize::MAX, 0, Team::Glacier);
        let damage = 100.0 - world.players[0].health;
        assert!(damage > 9. && damage < 9.3, "edge damage, got {damage}");
        let health=world.players[0].health;
        world.explode(blast+Vec3::X*0.02,usize::MAX,0,Team::Glacier);
        assert_eq!(world.players[0].health,health);
    }

    #[test]
    fn reload_ready_cue_occurs_once_and_fire_stays_gated() {
        let mut world = solo_airborne();
        world.input.fire = true;
        step(&mut world, 1);
        assert!(world.players[0].cooldown > crate::defs::weapons().disc.reload - STEP * 2.0);
        step(&mut world, 30);
        assert_eq!(world.discs.len(), 1, "holding fire cannot bypass reload");
        world.input.fire = false;
        let mut cues = 0;
        for _ in 0..80 {
            step(&mut world, 1);
            cues += world.events.matches("disc_ready").count();
        }
        assert_eq!(cues, 1);
    }

    #[test]
    fn airborne_momentum_survives_coasting_and_flag_pickup() {
        for flag in [None, Some(Team::Glacier)] {
            let mut world = solo_airborne();
            world.players[0].vel = Vec3::new(0.0, 0.0, -50.0);
            world.players[0].carrying = flag;
            step(&mut world, 60);
            assert!((world.players[0].vel.z + 50.0).abs() < 0.01, "coasting must preserve route speed");
            assert!(world.players[0].alive);
        }
    }

    #[test]
    fn air_strafe_steers_left_and_right_without_free_speed() {
        for direction in [-1.0, 1.0] {
            let mut world = solo_airborne();
            world.players[0].vel = Vec3::new(0.0, 0.0, -50.0);
            world.input.move_x = direction;
            world.input.move_z = 1.0;
            step(&mut world, 30);
            let p = &world.players[0];
            assert!(p.pos.x * direction > 128.0 * direction + 0.2, "strafe sign must agree with the view");
            assert!(p.vel.x * direction > 1.5);
            assert!(Vec2::new(p.vel.x, p.vel.z).length() <= 50.01);
        }
    }

    #[test]
    fn holding_ski_through_landing_does_not_repeat_the_jump() {
        let mut world = World::new();
        world.start_match(true);
        world.players.truncate(1);
        world.input.jump = true;
        step(&mut world, 120);
        let p = &world.players[0];
        assert!(p.skiing && p.on_ground, "holding Space should settle into skiing");
        assert!(p.vel.y.abs() < 0.1, "no automatic second hop");
        world.input.jump = false;
        step(&mut world, 1);
        world.input.jump = true;
        step(&mut world, 1);
        assert!(world.players[0].vel.y > 4.0, "a new press can still jump");
    }

    #[test]
    fn ski_contact_does_not_pull_an_outgoing_velocity_back_to_terrain() {
        let mut world = World::new();
        world.start_match(true);
        world.players.truncate(1);
        let p = &mut world.players[0];
        p.vel = Vec3::new(30.0, 8.0, 0.0);
        p.on_ground = true;
        let start_y = p.pos.y;
        world.input.jump = true;
        step(&mut world, 1);
        let p = &world.players[0];
        assert!(!p.on_ground && !p.skiing, "outgoing motion must launch from a crest");
        assert!(p.pos.y > start_y + 0.1);
        assert!(p.vel.x > 29.9);
    }

    #[test]
    fn empty_jetpack_recharges_in_five_seconds() {
        let mut world = solo_airborne();
        world.players[0].pos.y = 1000.0;
        world.players[0].energy = 0.0;
        step(&mut world, 150);
        assert!((world.players[0].energy - 30.0).abs() < 0.1);
        step(&mut world, 150);
        assert!((world.players[0].energy - crate::defs::movement().energy_max).abs() < 0.1);
    }

    #[test]
    fn discs_inherit_velocity_and_fly_without_gravity() {
        let mut world = solo_airborne();
        world.players[0].vel = Vec3::new(20.0, 0.0, 0.0);
        world.shoot(0);
        let d = &world.discs[0];
        let inherited = Vec3::new(20.0, 0.0, 0.0) * crate::defs::weapons().disc.inherit;
        let weapon = d.vel - inherited;
        assert!(
            (weapon.length() - crate::defs::weapons().disc.speed).abs() < 0.05,
            "retain weapon velocity inheritance, weapon speed {}",
            weapon.length()
        );
        assert!(weapon.z < -90.0, "the round still flies downrange");
        let y = d.pos.y;
        let vy = d.vel.y;
        for _ in 0..30 { world.step_discs(STEP); }
        assert_eq!(world.discs.len(), 1);
        let expected = y + vy * 30.0 * STEP;
        assert!(
            (world.discs[0].pos.y - expected).abs() < 0.002,
            "disc should not sag"
        );
    }

    #[test]
    fn running_reaches_its_advertised_speed() {
        let mut world = World::new();
        world.start_match(true);
        world.players.truncate(1);
        world.players[0].yaw = 0.0;
        world.input.move_z = 1.0;
        step(&mut world, 30);
        let v = world.players[0].vel;
        assert!(Vec2::new(v.x, v.z).length() > crate::defs::movement().walk_max * 0.95);
    }

    #[test]
    fn fast_routes_get_a_smooth_gravity_reduction() {
        assert_eq!(gravity_for_speed(50.0), crate::defs::movement().gravity);
        assert!((gravity_for_speed(250.0 / 3.6 - 0.001)
            - gravity_for_speed(250.0 / 3.6 + 0.001)).abs() < 0.001);
        assert!(gravity_for_speed(90.0) < crate::defs::movement().gravity * 0.8);
        assert!(gravity_for_speed(120.0) >= crate::defs::movement().gravity * 0.65 - 0.001);
    }

    fn step(world: &mut World, frames: u32) {
        for _ in 0..frames {
            world.tick(STEP);
        }
    }

    #[test]
    fn a_strafes_left_when_facing_negative_z() {
        let mut world = World::new();
        world.start_match(true);
        world.players[world.player_id].yaw = 0.0;
        let x0 = world.player_pos().x;
        world.input.move_x = -1.0;
        step(&mut world, 180);
        assert!(
            world.player_pos().x < x0 - 0.4,
            "A should move toward -X (left of a -Z facing), x0={x0} x={}",
            world.player_pos().x
        );
    }

    #[test]
    fn d_strafes_right_when_facing_negative_z() {
        let mut world = World::new();
        world.start_match(true);
        world.players[world.player_id].yaw = 0.0;
        let x0 = world.player_pos().x;
        world.input.move_x = 1.0;
        step(&mut world, 180);
        assert!(
            world.player_pos().x > x0 + 0.4,
            "D should move toward +X (right of a -Z facing), x0={x0} x={}",
            world.player_pos().x
        );
    }

    #[test]
    fn tap_space_jumps_from_a_stop() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..30 {
            world.tick(STEP);
        }
        world.input.jump = true;
        world.tick(STEP);
        world.input.jump = false;
        world.tick(STEP);
        let p = &world.players[world.player_id];
        assert!(p.vel.y > 4.0, "a tap of space from a stop should jump, vel.y={}", p.vel.y);
        assert!(!p.skiing, "a standing jump is not a ski");
    }

    #[test]
    fn hold_space_skis_from_a_stop_and_a_hill_builds_speed() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..20 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.pos = Vec3::new(128.0, height(128.0, 46.0) + PLAYER_RADIUS, 46.0);
            p.vel = Vec3::new(0.0, 0.0, crate::defs::movement().max_jump_speed + 2.0);
            p.on_ground = true;
            p.yaw = std::f32::consts::PI;
        }
        let mut peak = 0.0f32;
        let mut skied = false;
        for _ in 0..100 {
            world.input.jump = true;
            world.input.move_z = 1.0;
            world.tick(STEP);
            let p = &world.players[world.player_id];
            let speed = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
            peak = peak.max(speed);
            skied |= p.skiing;
        }
        assert!(skied, "holding jump on the drop should enter the ski path");
        assert!(
            peak > crate::defs::movement().max_jump_speed + 2.0,
            "gravity on frictionless snow should add speed, peak={peak:.1}"
        );
    }

    #[test]
    fn a_flat_ski_does_not_add_speed() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..30 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.pos = Vec3::new(EMBER_HOME.x, height(EMBER_HOME.x, EMBER_HOME.z) + PLAYER_RADIUS, EMBER_HOME.z);
            p.vel = Vec3::new(crate::defs::movement().max_jump_speed + 4.0, 0.0, 0.0);
            p.on_ground = true;
        }
        let before = crate::defs::movement().max_jump_speed + 4.0;
        for _ in 0..12 {
            world.input.jump = true;
            world.tick(STEP);
        }
        let p = &world.players[world.player_id];
        let speed = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        assert!(p.skiing, "a fast hold on the pad should still be a ski");
        assert!(
            speed <= before + 0.4,
            "skiing must not drive you faster on a flat, before={before:.1} after={speed:.1}"
        );
    }

    #[test]
    fn downhill_ski_outruns_plain_gravity() {
        let x = 128.0;
        let z = 46.0;
        let n = ski_normal(MapId::Valley, x, z);
        let g = Vec3::new(0.0, -crate::defs::movement().gravity, 0.0);
        let tangent = g - n * g.dot(n);
        let down = Vec3::new(tangent.x, 0.0, tangent.z);
        assert!(
            down.length() > 3.0,
            "the drop should be a real slope, accel={}",
            down.length()
        );
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..10 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            // Start tangent to the slope. A horizontal velocity pointing out
            // of this downhill face is a launch, not supported skiing.
            let start = tangent.normalize_or_zero() * (crate::defs::movement().max_jump_speed + 4.0);
            p.pos = Vec3::new(x, height(x, z) + PLAYER_RADIUS, z);
            p.vel = start;
            p.on_ground = true;
            p.yaw = start.x.atan2(start.z);
        }
        let before = {
            let p = &world.players[world.player_id];
            Vec3::new(p.vel.x, 0.0, p.vel.z).length()
        };
        let mut contact_frames = 0;
        for _ in 0..45 {
            world.input.jump = true;
            world.tick(STEP);
            contact_frames += u32::from(world.players[world.player_id].skiing);
        }
        let p = &world.players[world.player_id];
        let after = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        let gain = after - before;
        let plain = down.length() * 45.0 * STEP;
        assert!(contact_frames > 10, "the slope should support sustained ski contact, got {contact_frames}");
        assert!(
            gain > plain * 1.25,
            "downhill ski should outrun plain gravity, gain={gain:.2} plain={plain:.2}"
        );
    }

    #[test]
    fn a_single_press_does_not_keep_friction_off() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..30 {
            world.tick(STEP);
        }
        world.input.jump = true;
        world.tick(STEP);
        world.input.jump = false;
        for _ in 0..40 {
            world.tick(STEP);
        }
        let p = &world.players[world.player_id];
        assert!(!p.skiing, "a released jump must not keep the ski path on");
    }

    #[test]
    fn hold_space_skis_once_moving() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..30 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.vel.x = crate::defs::movement().max_jump_speed + 4.0;
            p.vel.y = 0.0;
            p.on_ground = true;
        }
        world.input.jump = true;
        world.tick(STEP);
        let p = &world.players[world.player_id];
        assert!(
            p.skiing && p.on_ground,
            "above maxJumpSpeed a held jump is a ski, not a hop"
        );
    }

    #[test]
    fn a_slow_hold_still_hops() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..30 {
            world.tick(STEP);
        }
        world.input.jump = true;
        world.tick(STEP);
        let p = &world.players[world.player_id];
        assert!(!p.on_ground, "below minJumpSpeed a hold still hops");
        assert!(p.vel.y > 3.0, "the hop is a real impulse, vy={}", p.vel.y);
    }

    #[test]
    fn flag_pickup_does_not_change_jump_strength() {
        let mut light = World::new();
        let mut heavy = World::new();
        for world in [&mut light, &mut heavy] {
            world.start_match(true);
            for _ in 0..30 {
                world.tick(STEP);
            }
        }
        heavy.players[heavy.player_id].carrying = Some(Team::Glacier);
        light.input.jump = true;
        heavy.input.jump = true;
        light.tick(STEP);
        heavy.tick(STEP);
        let a = light.players[light.player_id].vel.y;
        let b = heavy.players[heavy.player_id].vel.y;
        assert!((b - a).abs() < 0.01, "a flag must not alter movement, light={a:.2} carrier={b:.2}");
    }

    #[test]
    fn air_control_can_start_a_slow_drift() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..40 {
            world.tick(STEP);
        }
        world.input.jump = true;
        world.tick(STEP);
        world.input.jump = false;
        world.tick(STEP);
        {
            let p = &mut world.players[world.player_id];
            p.on_ground = false;
            p.vel.y = 8.0;
            p.pos.y += 1.5;
        }
        world.input.move_z = 1.0;
        let h0 = {
            let p = &world.players[world.player_id];
            Vec3::new(p.vel.x, 0.0, p.vel.z).length()
        };
        for _ in 0..12 {
            world.input.move_z = 1.0;
            world.tick(STEP);
        }
        let p = &world.players[world.player_id];
        let h1 = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        assert!(!p.on_ground, "still airborne");
        assert!(
            h1 > h0 + 0.8 && h1 <= crate::defs::movement().walk_max,
            "W should give limited air control, h0={h0:.2} h1={h1:.2}"
        );
    }

    /// The jump-boost (`movement.jet_jump`): jets pressed on the ground kick
    /// you up half a jump, standing or skiing fast; holding them on doesn't
    /// kick again, and a jump on the same takeoff takes its place.
    #[test]
    fn jets_pressed_on_the_ground_kick_up_half_a_jump_at_any_speed() {
        let settled = |speed: f32, ski: bool| {
            let mut w = World::new();
            w.start_match(true);
            w.players.truncate(1);
            for _ in 0..40 { w.tick(STEP); }
            let i = w.player_id;
            assert!(w.players[i].on_ground, "standing before the press");
            w.players[i].yaw = 0.0;
            w.players[i].vel = Vec3::new(0.0, 0.0, -speed);
            w.players[i].energy = 60.0;
            if ski { w.input.jump = true; w.input.jump_prev = true; }
            w
        };
        let rise = |mut w: World, held_before: bool, jump: bool| {
            let i = w.player_id;
            w.players[i].jet_prev = held_before;
            w.input.jet = true;
            if jump { w.input.jump = true; w.input.jump_prev = false; }
            w.tick(STEP);
            w.players[i].vel.y
        };
        let m = crate::defs::movement();
        let half = m.jump_impulse * m.jet_jump;
        assert!((m.jet_jump - 0.5).abs() < 1e-6 && half > 4.0, "half a jump: {half}");
        let standing = rise(settled(0.0, false), false, false) - rise(settled(0.0, false), true, false);
        assert!((standing - half).abs() < 0.35, "standing: kicked {standing:.2}, half a jump {half:.2}");
        let skiing = rise(settled(30.0, true), false, false) - rise(settled(30.0, true), true, false);
        assert!((skiing - half).abs() < 0.35, "skiing at 108 km/h the kick doesn't fade: {skiing:.2}");
        // A jump with the press: the jump alone, not the two stacked.
        let jumped = rise(settled(0.0, false), false, true) - rise(settled(0.0, false), true, true);
        assert!(jumped.abs() < 0.05, "a jump on the same takeoff takes the kick's place: {jumped:.2}");
        // In the air a fresh press does nothing extra.
        let mut air = settled(0.0, false);
        let i = air.player_id;
        air.players[i].pos.y += 6.0; air.players[i].on_ground = false;
        for _ in 0..10 { air.input.jet = false; air.tick(STEP); }
        let before = air.players[i].vel.y;
        air.input.jet = true; air.tick(STEP);
        let thrust = air.players[i].vel.y - before;
        assert!(thrust < 1.0, "no kick in the air: {thrust:.2}");
    }

    #[test]
    fn steering_the_jet_takes_its_push_from_the_climb() {
        let mut idle = World::new();
        let mut held = World::new();
        let mut looked = World::new();
        for world in [&mut idle, &mut held, &mut looked] {
            world.start_match(true);
            // Alone: teammates block, and a bump would skew the comparison.
            world.players.truncate(1);
            for _ in 0..30 {
                world.tick(STEP);
            }
            world.players[world.player_id].on_ground = false;
            world.players[world.player_id].coyote = 0.0;
            world.players[world.player_id].vel = Vec3::ZERO;
            world.players[world.player_id].pos.y += 6.0;
            world.players[world.player_id].yaw = 0.0;
        }
        looked.players[looked.player_id].pitch = 1.1;
        for _ in 0..16 {
            idle.input.jet = true;
            held.input.jet = true;
            held.input.move_z = 1.0;
            looked.input.jet = true;
            idle.tick(STEP);
            held.tick(STEP);
            looked.tick(STEP);
        }
        let a = &idle.players[idle.player_id];
        let b = &held.players[held.player_id];
        let c = &looked.players[looked.player_id];
        let b_h = Vec3::new(b.vel.x, 0.0, b.vel.z).length();
        let a_h = Vec3::new(a.vel.x, 0.0, a.vel.z).length();
        assert!(b_h > a_h + 1.0, "WASD should add a push, held={b_h:.1} idle={a_h:.1}");
        // One thrust: what pushes along the keys no longer lifts. Both stay
        // under the knees, so neither push nor climb has faded.
        assert!(
            ((a.vel.y - b.vel.y) - (b_h - a_h)).abs() < 0.05,
            "the push is paid from the climb, idle vy={} held vy={} push {}",
            a.vel.y,
            b.vel.y,
            b_h - a_h
        );
        // From a standstill, light armor leaning in sinks: the lift left is
        // under gravity until the push fades with speed.
        let armor = crate::sim::standard_armor();
        assert!(armor.jet - armor.side < crate::defs::movement().gravity && b.vel.y < 0.0, "leaning in sinks: vy={}", b.vel.y);
        assert!(
            (a.vel.y - c.vel.y).abs() < 0.4,
            "look must not change the jet, idle vy={} looked vy={}",
            a.vel.y,
            c.vel.y
        );
    }

    #[test]
    fn a_fast_ski_keeps_full_jet_climb_until_it_turns() {
        let mut straight = World::new();
        let mut held = World::new();
        let mut turned = World::new();
        for world in [&mut straight, &mut held, &mut turned] {
            world.start_match(true);
            for _ in 0..20 {
                world.tick(STEP);
            }
            // Sliding forward (yaw 0 faces -Z) past the push's speed.
            let p = &mut world.players[world.player_id];
            p.on_ground = false;
            p.pos.y += 12.0;
            p.vel = Vec3::new(0.0, 0.0, -(crate::sim::standard_armor().air_cap + 10.0));
            p.energy = crate::defs::movement().energy_max;
            p.yaw = 0.0;
        }
        for _ in 0..20 {
            straight.input.jet = true;
            held.input.jet = true;
            held.input.move_z = 1.0;
            turned.input.jet = true;
            turned.input.move_x = 1.0;
            straight.tick(STEP);
            held.tick(STEP);
            turned.tick(STEP);
        }
        let a = straight.players[straight.player_id].vel.y;
        let b = held.players[held.player_id].vel.y;
        let c = turned.players[turned.player_id].vel.y;
        assert!(
            (a - b).abs() < 0.4,
            "past the thrust cap a move key along the slide must not change climb, straight={a:.2} held={b:.2}"
        );
        // Across the slide nothing is along the keys yet: turning pays for
        // itself out of the climb.
        assert!(c < a - 3.0, "a turn at speed takes lift, straight={a:.2} turned={c:.2}");
    }

    #[test]
    fn jet_tank_supports_a_four_second_route_burn() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..20 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.on_ground = false;
            p.pos.y += 40.0;
            p.vel = Vec3::ZERO;
            p.energy = crate::defs::movement().energy_max;
        }
        let mut burned = 0;
        for _ in 0..400 {
            world.input.jet = true;
            world.tick(STEP);
            if world.players[world.player_id].jetting {
                burned += 1;
            } else if burned > 0 {
                break;
            }
        }
        let seconds = burned as f32 * STEP;
        assert!(
            (seconds - 3.8).abs() < 0.1,
            "tank should burn for roughly four seconds before reserve, got {seconds:.2}"
        );
        assert!(
            world.players[world.player_id].energy < crate::sim::standard_armor().min_jet,
            "the burn should stop at minJetEnergy"
        );
    }

    #[test]
    fn jet_alone_does_not_make_a_ski_line() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..20 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.on_ground = false;
            p.pos.y += 40.0;
            p.vel = Vec3::ZERO;
            p.energy = crate::defs::movement().energy_max;
            p.yaw = 0.0;
        }
        for _ in 0..180 {
            world.input.jet = true;
            world.input.move_z = 1.0;
            world.tick(STEP);
            if !world.players[world.player_id].jetting && world.players[world.player_id].energy < crate::sim::standard_armor().min_jet {
                break;
            }
        }
        let p = &world.players[world.player_id];
        let h = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
        assert!(
            h <= crate::sim::standard_armor().air_cap + 1.0,
            "jetting with W should fade by 72 km/h, h={h:.2} cap={:.2}",
            crate::sim::standard_armor().air_cap
        );
    }

    #[test]
    fn a_disc_at_your_feet_throws_you() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..20 {
            world.tick(STEP);
        }
        let id = world.player_id;
        {
            let p = &mut world.players[id];
            p.pitch = -1.2;
            p.vel = Vec3::ZERO;
            p.weapon = 0;
            p.cooldown = 0.0;
            p.on_ground = true;
        }
        world.input.weapon = 0;
        world.input.fire = true;
        world.tick(STEP);
        world.input.fire = false;
        let mut peak = 0.0_f32;
        for _ in 0..50 {
            world.tick(STEP);
            peak = peak.max(world.players[id].vel.y);
        }
        assert!(
            peak > 8.0,
            "a disc into the snow at your feet should throw you, peak vy={peak:.2}"
        );
    }

    #[test]
    fn jet_steers_a_fast_line_without_speeding_it_up() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..20 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.on_ground = false;
            p.pos.y += 30.0;
            p.vel = Vec3::new(crate::sim::standard_armor().air_cap + 15.0, 0.0, 0.0);
            p.energy = crate::defs::movement().energy_max;
            p.yaw = 0.0;
        }
        let before = {
            let p = &world.players[world.player_id];
            Vec3::new(p.vel.x, 0.0, p.vel.z).length()
        };
        for _ in 0..30 {
            world.input.jet = true;
            world.input.move_z = 1.0;
            world.tick(STEP);
        }
        let p = &world.players[world.player_id];
        let h = Vec3::new(p.vel.x, 0.0, p.vel.z);
        assert!(
            h.z < -1.0,
            "holding W while jetting should steer a fast line, vz={}",
            h.z
        );
        assert!(
            (h.length() - before).abs() < 1.5,
            "steering must not build a faster line, before={before:.1} after={:.1}",
            h.length()
        );
    }

    #[test]
    fn jet_lifts_off_the_snow_even_looking_down() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..30 {
            world.tick(STEP);
        }
        {
            let p = &mut world.players[world.player_id];
            p.pitch = -0.2;
            p.vel = Vec3::ZERO;
        }
        world.input.jet = true;
        for _ in 0..30 {
            world.tick(STEP);
        }
        let p = &world.players[world.player_id];
        assert!(p.jetting, "right click should jet");
        assert!(!p.on_ground, "jet should leave the snow");
        assert!(p.vel.y > 3.0, "jet should lift, vy={}", p.vel.y);
        // The jump-boost off the snow comes on top (`movement.jet_jump`).
        let kick = jet_kick(&standard_armor());
        assert!(p.vel.y < 12.0 + kick, "a half-second of jet should not be a missile, vy={}", p.vel.y);
    }

    #[test]
    fn right_click_jets_without_space() {
        let mut world = World::new();
        world.start_match(true);
        for _ in 0..20 {
            world.tick(STEP);
        }
        world.input.jet = true;
        let before = world.players[world.player_id].energy;
        world.tick(STEP);
        let p = &world.players[world.player_id];
        assert!(p.jetting, "jet is its own button");
        assert!(p.energy < before, "jet spends energy");
        assert!(!p.skiing, "jet does not start a ski");
    }

    #[test]
    fn w_moves_along_facing() {
        let mut world = World::new();
        world.start_match(true);
        world.players[world.player_id].yaw = 0.0;
        let z0 = world.player_pos().z;
        world.input.move_z = 1.0;
        step(&mut world, 180);
        assert!(
            world.player_pos().z < z0 - 0.4,
            "W should move toward -Z at yaw 0, z0={z0} z={}",
            world.player_pos().z
        );
    }
}

fn segment_sphere(start: Vec3, end: Vec3, center: Vec3, radius: f32) -> Option<f32> {
    let offset = start - center;
    let c = offset.length_squared() - radius * radius;
    if c <= 0.0 { return Some(0.0); }
    let travel = end - start;
    let a = travel.length_squared();
    if a < 1e-10 { return None; }
    let b = offset.dot(travel);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 { return None; }
    let t = (-b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

fn segment_box(start: Vec3, end: Vec3, low: Vec3, high: Vec3) -> Option<f32> {
    let delta = end - start;
    let mut entry = 0.0_f32;
    let mut exit = 1.0_f32;
    for axis in 0..3 {
        if delta[axis].abs() < 1e-7 {
            if start[axis] < low[axis] || start[axis] > high[axis] { return None; }
        } else {
            let a = (low[axis] - start[axis]) / delta[axis];
            let b = (high[axis] - start[axis]) / delta[axis];
            entry = entry.max(a.min(b));
            exit = exit.min(a.max(b));
            if entry > exit { return None; }
        }
    }
    Some(entry)
}

/// Whether a player at `pos` stands outdoors: nothing overhead for 40 m.
fn under_open_sky(map: MapId, pillars: &[Pillar], pos: Vec3) -> bool {
    obstacle_hit(map, pillars, pos + Vec3::Y * 1.5, pos + Vec3::Y * 40.0, 0.0).is_none()
}

fn obstacle_hit(map: MapId, pillars: &[Pillar], start: Vec3, end: Vec3, radius: f32) -> Option<f32> {
    let mut hit = crate::terrain::segment_hit(map, start, end, radius);
    if let Some((t,_))=crate::map_pack::on(map).and_then(|p|p.sweep(start,end,radius)) {
        hit=Some(hit.map_or(t,|old|old.min(t)));
    }
    if let Some(t) = pillar_hit(map, pillars, start, end, radius) {
        hit = Some(hit.map_or(t, |old| old.min(t)));
    }
    hit
}

/// The first map surface a sphere of `radius` meets moving from `from` to
/// `to` (same fraction as `obstacle_hit`), with that surface's normal toward
/// the sphere: from the map-geometry sweep, the terrain face, or a pillar's
/// face. Anything that bounces uses it. The normal used to come from a second
/// short probe along the flight, which missed walls struck at a glancing angle
/// and fell back to the ground's normal: the thing kept moving into the wall
/// and, starting the next step in contact, went through.
pub(crate) fn surface_hit(map: MapId, pillars: &[Pillar], from: Vec3, to: Vec3, radius: f32, far: f32) -> Option<(f32, Vec3)> {
    let mut best = crate::map_pack::on(map).and_then(|p| p.sweep(from, to, radius));
    if let Some(t) = crate::terrain::segment_hit(map, from, to, radius) {
        if best.is_none_or(|(old, _)| t < old) {
            let at = from.lerp(to, t);
            best = Some((t, crate::terrain::surface_on(map, at.x, at.z).1));
        }
    }
    if let Some(t) = pillar_hit(map, pillars, from, to, radius) {
        if best.is_none_or(|(old, _)| t < old) {
            best = Some((t, grenade_contact_normal(map, pillars, from.lerp(to, t), far, to - from)));
        }
    }
    best
}

/// Pillars (the Valley fixture) as square columns.
fn pillar_hit(map: MapId, pillars: &[Pillar], start: Vec3, end: Vec3, radius: f32) -> Option<f32> {
    let mut hit: Option<f32> = None;
    for pillar in pillars {
        let y = crate::terrain::height_on(map, pillar.x, pillar.z);
        // Match the rendered square column, not an unrelated circular volume.
        let half = pillar.r * 0.8;
        let padding = Vec3::splat(radius);
        let low = Vec3::new(pillar.x - half, y, pillar.z - half) - padding;
        let high = Vec3::new(pillar.x + half, y + pillar.h, pillar.z + half) + padding;
        if let Some(t) = segment_box(start, end, low, high) {
            hit = Some(hit.map_or(t, |old| old.min(t)));
        }
    }
    hit
}

fn player_mass(_p: &Player) -> f32 {
    crate::defs::movement().mass
}

/// Sweep across the rendered ground in short steps, resolving contacts in the
/// same tick as movement. This changes collision, not gravity/thrust/steering.
fn move_over_terrain(map: MapId, p: &mut Player, ski_held: bool, dt: f32) -> f32 {
    if let Some(pack)=crate::map_pack::on(map) {return move_in_imported_map(map,pack,p,ski_held,dt);}
    let steps = ((p.vel.length() * dt / 0.75).ceil() as usize).clamp(1, 64);
    let sub_dt = dt / steps as f32;
    let mut impact = 0.0_f32;
    for _ in 0..steps {
        let start = p.pos;
        let end = start + p.vel * sub_dt;
        if p.skiing {
            // Supported skiing keeps the existing smooth contact response once
            // per tick. Re-projecting at every substep would add artificial
            // friction and change the movement the player already approved.
            p.pos = end;
            p.pos.y = p.pos.y.max(crate::terrain::height_on(map, end.x, end.z) + PLAYER_RADIUS);
            continue;
        }
        if let Some(t) = crate::terrain::segment_hit(map, start, end, PLAYER_RADIUS) {
            p.pos = start.lerp(end, t);
            let (ground, face) = crate::terrain::surface_on(map, p.pos.x, p.pos.z);
            p.pos.y = ground + PLAYER_RADIUS;
            let inward = p.vel.dot(face);
            if inward < 0.0 {
                impact = impact.max(-inward);
                p.vel -= face * inward;
                p.on_ground = true;
                p.skiing = ski_held && !p.jetting;
            }
            p.pos += p.vel * (sub_dt * (1.0 - t));
            let floor = crate::terrain::height_on(map, p.pos.x, p.pos.z) + PLAYER_RADIUS;
            p.pos.y = p.pos.y.max(floor);
        } else {
            p.pos = end;
        }
    }
    impact
}

/// Whether lowering the body to `y` leaves it clear of the walls it was
/// clear of (`walls1`, on maps with careful collision): the ground is found
/// under its middle, so on stairs a body still over an upper step's edge
/// would be pulled down into it.
fn snap_clear(map: MapId, pos: Vec3, y: f32) -> bool {
    y >= pos.y || !careful_collision(map)
        || crate::map_pack::on(map).is_none_or(|pack| !pack.push_enters(pos, pos.with_y(y), true))
}

/// Careful collision (`walls1`): on maps with blade zones (Tenfold's tower),
/// where nobody has jets to get out of a trap, a walking body is never
/// pushed or snapped into a face, and never ends a move inside one: in a
/// corner tighter than square, rounding at a contact can leave a body a
/// hair inside a face, which a sweep from inside can't see, and the body
/// went on through. Other maps move exactly as they did (their behaviour
/// traces pin it), where jets get anyone out.
fn careful_collision(map: MapId) -> bool { features::has_blade_zones(map) }

fn contact_normal(map:MapId,pos:Vec3)->Vec3 {
    // Skim water is flat: over it, the seabed's slope doesn't reach the skier.
    let skim=crate::water::skim_surface_at(map,pos);
    if let Some(pack)=crate::map_pack::on(map) {
        if let Some((h,n))=pack.floor(pos-Vec3::Y*PLAYER_RADIUS) {
            if skim.is_some_and(|s|s>h) {return Vec3::Y;}
            if h>=crate::terrain::height_on(map,pos.x,pos.z) || pack.hole(pos.x,pos.z) {return n;}
        }
    }
    if skim.is_some_and(|s|s>=crate::terrain::height_on(map,pos.x,pos.z)) {return Vec3::Y;}
    ski_normal(map,pos.x,pos.z)
}

fn move_in_imported_map(map:MapId,pack:&crate::map_pack::MapPack,p:&mut Player,ski_held:bool,dt:f32)->f32 {
    let mut impact=0.0_f32;
    let mut remaining=dt;
    let careful=careful_collision(map);
    // A hair inside a wall at the start (moved there by something other
    // than a move; `walls1`): back out of it and along it, never through.
    // (Walls only: a sweep can't see a face it starts inside.)
    for _ in 0..if careful {4} else {0} {
        let Some((n,depth))=pack.wall_overlap(p.pos) else {break};
        p.pos+=n*(depth+0.001);
        let vn=p.vel.dot(n);
        if vn<0.0 {impact=impact.max(-vn);p.vel-=n*vn;}
    }
    let from=p.pos;
    // Continuous sweeps, with up to six sliding contacts per simulation tick.
    for _ in 0..6 {
        let start=p.pos;let end=start+p.vel*remaining;
        let mut hit=crate::terrain::segment_hit(map,start,end,PLAYER_RADIUS)
            .map(|t|(t,crate::terrain::surface_on(map,start.lerp(end,t).x,start.lerp(end,t).z).1));
        // Overlapping spheres cover the full body, including the eye and a
        // head/near-plane margin. The old stack ended below the camera.
        if let Some(h)=pack.body_sweep(start,end) {
            if hit.is_none_or(|old|h.0<old.0) {hit=Some(h);}
        }
        let Some((t,n))=hit else {p.pos=end;break;};
        // Off the surface by a hair. In a corner tighter than square that
        // push can reach into the other face, which no sweep from inside it
        // sees: then off that one by a hair too (`walls1`).
        let at=start.lerp(end,t);
        p.pos=at+n*0.001;
        if careful && pack.push_enters(at,p.pos,false) {p.pos=pack.settle_push(at,p.pos);}
        let vn=p.vel.dot(n);
        if vn<0.0 {impact=impact.max(-vn);p.vel-=n*vn;}
        if n.y>0.25 {p.on_ground=true;p.skiing=ski_held&&!p.jetting;}
        remaining*=1.0-t;
        if remaining<0.00001 {break;}
    }
    // Never ends a move sunk into a face it started clear of (`walls1`):
    // rounding at a contact can start a sweep a hair inside a face, which
    // it then can't see.
    if careful {p.pos=pack.unslip(from,p.pos);}
    impact
}

pub(crate) fn gravity_for_speed(speed: f32) -> f32 {
    // Gentle high-speed float, blended rather than a sudden threshold change.
    let blend = ((speed - 250.0 / 3.6) / (100.0 / 3.6)).clamp(0.0, 1.0);
    crate::defs::movement().gravity * (1.0 - 0.35 * blend * blend * (3.0 - 2.0 * blend))
}

fn steer_horizontal(p: &mut Player, wish: Vec3, accel: f32, low_speed_cap: f32, dt: f32) {
    let before = Vec3::new(p.vel.x, 0.0, p.vel.z);
    let cap = before.length().max(low_speed_cap);
    let after = (before + wish * accel * dt).clamp_length_max(cap);
    p.vel.x = after.x;
    p.vel.z = after.z;
}

/// Full hop below minJumpSpeed, none at maxJumpSpeed. Vertical speed also fades
/// the hop so a rising player does not stack impulses.
fn jump_scale(horiz: f32, vy: f32) -> f32 {
    let h = if horiz >= crate::defs::movement().max_jump_speed {
        0.0
    } else if horiz <= crate::defs::movement().min_jump_speed {
        1.0
    } else {
        1.0 - (horiz - crate::defs::movement().min_jump_speed) / (crate::defs::movement().max_jump_speed - crate::defs::movement().min_jump_speed)
    };
    let v = if vy >= crate::defs::movement().max_jump_speed {
        0.0
    } else if vy <= 0.0 {
        1.0
    } else {
        1.0 - (vy / crate::defs::movement().max_jump_speed).min(1.0)
    };
    h * v
}

fn ski_normal(map: MapId, x: f32, z: f32) -> Vec3 {
    let e = if crate::terrain::info(map).size > 1000.0 { 12.0 } else { 6.0 };
    let h = |x, z| crate::terrain::height_on(map, x, z);
    let hl = h(x - e, z);
    let hr = h(x + e, z);
    let hd = h(x, z - e);
    let hu = h(x, z + e);
    Vec3::new(hl - hr, 2.0 * e, hd - hu).normalize_or_zero()
}

#[cfg(test)]
mod equipment_tests {
    use super::*;
    use crate::equipment::{self,Kind};
    #[test]
    fn entire_menu_orbit_clears_terrain_and_solid_scenery() {
        for map in [MapId::Valley, MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold,
            MapId::Frostline, MapId::Dustreach] {
            let mut w=World::new(); w.set_map(map);
            // Render-only masts and spires can stand above the solid scenery.
            let render_top=crate::map_pack::on(map).map_or(f32::NEG_INFINITY,|pack| {
                let bytes=pack.asset("vertices.bin").expect("vertices");
                bytes.chunks_exact(48).map(|v|f32::from_le_bytes(v[4..8].try_into().unwrap()))
                    .fold(f32::NEG_INFINITY,f32::max)
            });
            for step in 0..720 {
                w.flyby=step as f32 * std::f32::consts::TAU / 720. / 0.18;
                let (eye,dir,_)=w.camera();
                assert!(eye.is_finite() && dir.is_normalized());
                assert!(eye.y >= w.ground(eye.x,eye.z)+27.9, "{map:?} orbit meets terrain");
                if let Some(pack)=crate::map_pack::on(map) { assert!(eye.y >= pack.highest_solid()+27.9, "{map:?} orbit meets solid"); }
                assert!(eye.y >= render_top+20.0, "{map:?} orbit {} passes render geometry topping {render_top}", eye.y);
            }
        }
    }
    #[test]
    fn player_blasts_push_outward_and_lift_with_distance_falloff() {
        for kind in [0,2] {
            let profile=crate::combat::player_weapon_blast(kind).unwrap();
            let mut w=world(); w.players.truncate(1);
            let pos=Vec3::new(1000.,300.,1000.);
            w.players[0].pos=pos; w.players[0].vel=Vec3::ZERO;
            w.players[0].on_ground=true; w.players[0].skiing=true;
            w.explode(pos-Vec3::Y*PLAYER_RADIUS,0,kind,Team::Ember);
            assert!((w.players[0].vel.y-profile.impulse/90.).abs()<0.001);
            assert!(!w.players[0].on_ground && !w.players[0].skiing);
            w.players[0].vel=Vec3::ZERO; w.players[0].health=100.;
            w.explode(pos-Vec3::X*(profile.radius*0.5+PLAYER_RADIUS),0,kind,Team::Ember);
            assert!((w.players[0].vel.length()-profile.impulse/90.*0.5).abs()<0.001);
            assert!(w.players[0].vel.x>0.);
        }
    }
    #[test]
    #[ignore = "measurement"]
    fn probe_trace_hill_slide() {
        let mut w = World::new(); w.set_map(MapId::OzarkticBlast); w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true); w.players.truncate(1);
        let (x, z) = (1090.0, 560.0);
        let (floor, _) = crate::terrain::support_on(MapId::OzarkticBlast, Vec3::new(x, 500.0, z));
        w.players[0].pos = Vec3::new(x, floor + PLAYER_RADIUS + 0.02, z); w.players[0].vel = Vec3::ZERO; w.players[0].yaw = 4.71;
        w.input.move_z = 1.0;
        for _ in 0..90 { w.physics_step(); }
        w.input.move_z = 0.0;
        for t in 0..60 {
            w.physics_step();
            let p = &w.players[0];
            let (fl, face) = crate::terrain::support_on(MapId::OzarkticBlast, p.pos);
            if t % 4 == 0 { println!("t{t:2} pos ({:.1},{:.2},{:.1}) vel ({:.2},{:.2},{:.2}) ground {} ski {} drop {:.2} face.y {:.2} contact.y {:.2}",
                p.pos.x, p.pos.y, p.pos.z, p.vel.x, p.vel.y, p.vel.z, p.on_ground, p.skiing, p.pos.y - fl - PLAYER_RADIUS, face.y, contact_normal(MapId::OzarkticBlast, p.pos).y); }
        }
    }

    #[test]
    fn letting_go_on_a_hill_stops_you_the_same_in_every_direction() {
        for (x, z) in [(1024.0, 480.0), (960.0, 520.0), (1090.0, 560.0), (1024.0, 600.0), (1000.0, 1560.0)] {
            for yaw in [0.0f32, 1.57, 3.14, 4.71] {
                let mut line = format!("({x:.0},{z:.0}) yaw {yaw:.2}:");
                for (label, mx, mz) in [("F", 0.0, 1.0), ("B", 0.0, -1.0), ("L", -1.0, 0.0), ("R", 1.0, 0.0)] {
                    let mut w = World::new(); w.set_map(MapId::OzarkticBlast); w.set_mode(crate::map_catalog::SupportedMode::Ctf);
                    w.start_match(true); w.players.truncate(1);
                    let at = Vec3::new(x, 0.0, z);
                    let (floor, face) = crate::terrain::support_on(MapId::OzarkticBlast, at + Vec3::Y * 500.0);
                    w.players[0].pos = Vec3::new(x, floor + PLAYER_RADIUS + 0.02, z); w.players[0].vel = Vec3::ZERO;
                    w.players[0].yaw = yaw;
                    w.input.move_x = mx; w.input.move_z = mz;
                    for _ in 0..90 { w.physics_step(); }
                    let v0 = Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length();
                    let p0 = w.players[0].pos;
                    w.input.move_x = 0.0; w.input.move_z = 0.0;
                    let mut t = 0;
                    while Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length() > 0.3 && t < 300 { w.physics_step(); t += 1; }
                    let d = Vec2::new(w.players[0].pos.x - p0.x, w.players[0].pos.z - p0.z).length();
                    line += &format!(" {label} v{v0:4.1} {:.2}s {d:4.1}m |", t as f32 / 60.0);
                    assert!(d < 4.5, "slid {d:.1} m after releasing {label} at ({x},{z}) yaw {yaw}");
                    if label == "F" && yaw == 0.0 { line = format!("slope ny {:.2} {line}", face.y); }
                }
                println!("{line}");
            }
        }
    }

    #[test]
    #[ignore = "measurement"]
    fn probe_stopping_distance_by_direction() {
        for (label, mx, mz) in [("forward", 0.0, 1.0), ("back", 0.0, -1.0), ("strafe", 1.0, 0.0), ("diagonal", 0.7, 0.7)] {
            let mut w = World::new(); w.set_map(MapId::Longfield); w.set_mode(crate::map_catalog::SupportedMode::Ctf); w.start_match(true); w.players.truncate(1);
            let at = Vec3::new(1000.0, 0.0, 1024.0);
            w.players[0].pos = Vec3::new(at.x, crate::terrain::support_on(MapId::Longfield, at + Vec3::Y * 200.0).0 + PLAYER_RADIUS + 0.02, at.z);
            w.players[0].vel = Vec3::ZERO;
            w.players[0].yaw = 0.3;
            w.input.move_x = mx; w.input.move_z = mz;
            for _ in 0..120 { w.physics_step(); }
            let v0 = Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length();
            let p0 = w.players[0].pos;
            w.input.move_x = 0.0; w.input.move_z = 0.0;
            let mut t = 0;
            while Vec2::new(w.players[0].vel.x, w.players[0].vel.z).length() > 0.1 && t < 600 { w.physics_step(); t += 1; }
            let d = Vec2::new(w.players[0].pos.x - p0.x, w.players[0].pos.z - p0.z).length();
            println!("{label:9} speed {v0:5.2} m/s  stops in {:.2} s over {d:.2} m", t as f32 / 60.0);
        }
    }

    #[test]
    fn your_own_grenade_hurts_you_at_full_strength_and_a_disc_at_40_percent() {
        let hit = |kind: u8| {
            let mut w=world(); w.players.truncate(1);
            let p0=w.players[0].pos; let team=w.players[0].team;
            w.discs.push(Disc{pos:p0+Vec3::new(0.6,0.3,0.),vel:Vec3::new(0.,-20.,0.),team,owner:0,life:1.0,kind,spin:0.,target:0,power:1.0});
            let mut min_h=100.0f32;
            for _ in 0..60 { w.physics_step(); min_h=min_h.min(w.players[0].health); }
            100.0-min_h
        };
        let grenade=hit(2); let disc=hit(0);
        assert!(grenade>15.0, "a grenade at your feet hurts: {grenade}");
        assert!(disc>5.0 && disc<40.0, "a disc at your feet hurts a little: {disc}");
    }

    fn world()->World {
        let mut w=World::new();w.set_map(MapId::OldHoller);w.start_rift(true);w
    }
    #[test]
    fn disc_and_grenade_impacts_apply_one_blast_and_kill_in_two() {
        for kind in [0,2] {
            let profile=crate::combat::player_weapon_blast(kind).unwrap();
            let mut w=world();w.players.truncate(1);
            w.players[0].pos=Vec3::new(1000.,300.,1000.);
            w.players[0].team=Team::Glacier;w.players[0].health=100.;
            let mut nearby=w.players[0].clone();nearby.pos.z+=4.;w.players.push(nearby);
            let mut distant=w.players[0].clone();distant.pos.z+=16.;w.players.push(distant);
            for shot in 0..2 {
                w.discs.push(Disc {pos:w.players[0].pos+Vec3::new(-5.,0.9,0.),vel:Vec3::X*80.,
                    owner:MAX_PLAYERS,team:Team::Ember,kind,life:1.5,spin:0., target: 0, power: 1.0 });
                for _ in 0..8 {w.step_discs(STEP);}
                assert!(w.discs.is_empty());
                if shot==0 {
                    assert!((w.players[0].health-(100.-profile.max_damage)).abs()<0.001,
                        "kind {kind}: {:?}",w.players[0].health);
                    assert!(w.players[0].alive);
                    assert_eq!(w.explosions.len(),1);
                    assert_eq!(w.explosions[0].max_r,profile.radius);
                    let expected=profile.damage(player_blast_distance(w.explosions[0].pos,w.players[1].pos));
                    assert!((w.players[1].health-(100.-expected)).abs()<0.001);
                    assert!(expected>0. && expected<profile.max_damage);
                    assert_eq!(w.players[2].health,100.);
                }
            }
            assert!(!w.players[0].alive,"two direct hits must kill for kind {kind}");
        }
    }
    #[test]
    fn grenade_shock_reaches_wider_than_its_damage_and_cover_blocks_both_weapons() {
        for kind in [0,2] {
            let mut w=world();w.players.truncate(1);w.players[0].team=Team::Glacier;
            w.players[0].pos=Vec3::new(1012.,300.,1000.);
            w.players[0].vel=Vec3::ZERO;
            w.explode(Vec3::new(1000.,300.7,1000.),MAX_PLAYERS,kind,Team::Ember);
            if kind==0 {assert_eq!(w.players[0].health,100.);assert_eq!(w.players[0].vel,Vec3::ZERO);}
            else {
                // Splash reaches 15 m: 12 m off still takes about a fifth, and shoves.
                assert!(w.players[0].health<90. && w.players[0].health>65.,"{}",w.players[0].health);
                assert!(w.players[0].vel.length()>0.5,"kicked: {}",w.players[0].vel);
            }
            w.players[0].health=100.;w.players[0].pos=Vec3::new(829.,110.,1400.);
            w.explode(Vec3::new(832.5,110.7,1400.),MAX_PLAYERS,kind,Team::Ember);
            assert_eq!(w.players[0].health,100.,"base wall must shield kind {kind}");
        }
    }
    /// The mortar is the scariest blast: 6 m away it kills light armor
    /// outright and leaves heavy armor badly hurt.
    #[test]
    fn a_mortar_six_metres_off_kills_light_armor_and_maims_heavy() {
        for (armor, alive) in [(ArmorClass::Light, false), (ArmorClass::Heavy, true)] {
            let mut w=world();w.players.truncate(1);w.players[0].team=Team::Glacier;
            w.players[0].armor=armor;w.players[0].health=100.;
            w.players[0].pos=Vec3::new(1006.,300.,1000.);w.players[0].vel=Vec3::ZERO;
            w.explode(Vec3::new(1000.,300.7,1000.),MAX_PLAYERS,loadout::MORTAR_KIND,Team::Ember);
            assert_eq!(w.players[0].alive,alive,"{armor:?}: health {}",w.players[0].health);
            if alive {assert!(w.players[0].health<50.,"{armor:?}: {}",w.players[0].health);}
        }
    }
    #[test]
    fn roof_turrets_engage_someone_on_the_roof_behind_them() {
        let mut w=World::new();w.set_map(MapId::OldHoller);w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);w.players.truncate(1);
        let defs=crate::equipment::definitions(MapId::OldHoller);
        let k=defs.iter().position(|d|d.id=="turret-5").unwrap();
        let d=&defs[k];
        // On the hall roof, 22 m behind the turret: outside its 200° field.
        let behind=Vec3::new(1173.,121.12,479.);
        assert!(!d.in_arc(behind),"the spot is behind the turret's field of fire");
        w.players[0].team=Team::Glacier;w.players[0].pos=behind;w.players[0].vel=Vec3::ZERO;
        w.step_equipment(STEP);
        assert_eq!(w.equipment[k].contacts,1,"a player on the open roof is engaged");
        assert!(w.equipment[k].aim.dot((behind-d.pos()).normalize())>0.9);
    }

        #[test]
    fn a_mid_air_hit_throws_harder_than_one_on_the_ground() {
        let kick = |airborne: bool| {
            let mut w=world();w.players.truncate(1);w.players[0].team=Team::Glacier;
            w.players[0].pos=Vec3::new(1003.,300.,1000.);w.players[0].vel=Vec3::ZERO;
            w.players[0].on_ground=!airborne;
            w.explode(Vec3::new(1000.,300.7,1000.),MAX_PLAYERS,0,Team::Ember);
            w.players[0].vel.length()
        };
        let (ground, air) = (kick(false), kick(true));
        assert!((air / ground - crate::defs::movement().air_hit_kick).abs() < 0.01, "{air} vs {ground}");
    }
        #[test]
    fn own_disc_hurts_at_40_percent_own_grenade_in_full_and_team_damage_stays_off() {
        for kind in [0,2] {
            let mut w=world();w.players.truncate(1);w.players[0].pos=Vec3::new(1000.,300.,1000.);
            let mut enemy=w.players[0].clone();enemy.team=Team::Glacier;w.players.push(enemy);
            w.players.push(w.players[0].clone());
            w.explode(w.players[0].pos+Vec3::Y*0.7,0,kind,Team::Ember);
            let peak=crate::combat::player_weapon_blast(kind).unwrap().max_damage;
            let own=if kind==0 {crate::defs::weapons().disc.self_damage} else {1.0};
            assert!((w.players[0].health-(100.-peak*own)).abs()<0.001);
            assert!((w.players[1].health-(100.-peak)).abs()<0.001);
            assert_eq!(w.players[2].health,100.);
            assert_eq!(w.players[2].vel,Vec3::ZERO);
        }
    }
    #[test]
    fn equipment_uses_the_same_explosive_curve() {
        for kind in [0,2] {
            let mut w=world();let defs=equipment::definitions(w.map);
            let i=defs.iter().position(|d|d.kind==Kind::Sensor && d.team==0).unwrap();
            let d=&defs[i];let blast=d.pos()+Vec3::Y*(d.radius+2.);
            let profile=crate::combat::structure_blast(kind).unwrap();
            w.explode(blast,MAX_PLAYERS,kind,Team::Glacier);
            // The sensor's generator shield soaks the blast before the hull.
            assert!((w.equipment[i].shield-(d.max_shield()-profile.damage(2.))).abs()<0.001);
            assert_eq!(w.equipment[i].health,d.max_health());
        }
    }
    #[test]
    fn full_head_stops_at_ceiling_even_at_high_speed() {
        let mut w=world();
        let pack=crate::map_pack::on(w.map).unwrap();
        let p=&mut w.players[0];
        p.pos=Vec3::new(822.,115.,1400.);p.vel=Vec3::Y*150.;
        move_in_imported_map(w.map,pack,p,false,0.1);
        assert!(p.pos.y>117. && p.pos.y+EYE+PLAYER_RADIUS<=119.401,"head {:?}",p.pos);
        assert!(p.vel.y.abs()<0.001);
        // Keep pressing into the ceiling without creeping through it.
        for _ in 0..60 {p.vel=Vec3::Y*10.;move_in_imported_map(w.map,pack,p,false,STEP);}
        assert!(p.pos.y+EYE+PLAYER_RADIUS<=119.401);
    }
    #[test]
    fn camera_offsets_cannot_cross_ceiling_or_wall() {
        let mut w=world();w.trauma=1.;
        w.players[0].pos=Vec3::new(822.,117.,1400.);
        w.net_camera_offset=Vec3::Y*8.;
        let (eye,_,_)=w.camera();
        assert!(eye.y<119.1 && eye.y>118.5,"ceiling camera {eye:?}");
        w.players[0].pos=Vec3::new(828.,110.,1400.);
        w.net_camera_offset=Vec3::X*10.;
        let (eye,_,_)=w.camera();
        assert!(eye.x<830. && eye.x>828.,"wall camera {eye:?}");
    }
    #[test]
    fn third_person_camera_looks_over_the_right_shoulder_and_stays_out_of_walls() {
        let mut w=world();w.third_person=true;
        w.players[0].pos=Vec3::new(1000.,300.,1000.);w.players[0].yaw=0.;w.players[0].pitch=0.;
        w.players[0].vel=Vec3::ZERO;w.players[0].alive=true;w.players[0].stun=0.;
        let eye=w.players[0].pos+Vec3::Y*EYE;
        let (cam,dir,_)=w.camera();
        // Yaw 0 looks along -Z: the right is +X, behind is +Z.
        let d=cam-eye;
        assert!((d.x-THIRD_PERSON_RIGHT).abs()<0.05 && (d.y-THIRD_PERSON_UP).abs()<0.05 && (d.z-THIRD_PERSON_BACK).abs()<0.05,"{d:?}");
        assert!(dir.dot(Vec3::NEG_Z)>0.999);
        // The shot line starts near the body and runs along the look.
        let (from,shot)=w.aim_line(0).unwrap();
        assert!(from.distance(eye)<2.5 && shot.dot(dir)>0.95,"{from:?} {shot:?}");
        // Backed against the hall's east wall (x 830), looking west: the
        // camera is pulled in short of it.
        w.players[0].pos=Vec3::new(828.,110.,1400.);w.players[0].yaw=std::f32::consts::FRAC_PI_2;
        let (cam,_,_)=w.camera();
        assert!(cam.x<830. && cam.x>828.,"wall camera {cam:?}");
    }
    #[test]
    fn plasma_direct_hits_kill_in_two_with_splash_and_small_push() {
        let mut w=world();w.players.truncate(1);
        w.players[0].pos=Vec3::new(1000.,300.,1000.);
        w.players[0].team=Team::Glacier;w.players[0].health=100.;w.players[0].vel=Vec3::ZERO;
        let mut nearby=w.players[0].clone();nearby.pos.z+=3.;w.players.push(nearby);
        let mut distant=w.players[0].clone();distant.pos.z+=10.;w.players.push(distant);
        for shot in 0..2 {
            w.discs.push(Disc {pos:w.players[0].pos+Vec3::new(-5.,0.9,0.),vel:Vec3::X*80.,
                owner:MAX_PLAYERS,team:Team::Ember,kind:3,life:3.,spin:0., target: 0, power: 1.0 });
            for _ in 0..8 {w.step_discs(STEP);}
            assert!(w.discs.is_empty());
            if shot==0 {
                assert_eq!(w.players[0].health,45.);assert!(w.players[0].alive);
                assert!(w.players[0].vel.length()>0. && w.players[0].vel.length()<=3.5);
                assert!(w.players[1].health>45. && w.players[1].health<100.);
                assert_eq!(w.players[2].health,100.);
            }
        }
        assert!(!w.players[0].alive);
    }
    #[test]
    fn plasma_splash_respects_base_walls_and_friendly_teams() {
        let mut w=world();w.players.truncate(1);
        w.players[0].pos=Vec3::new(829.,110.,1400.);w.players[0].team=Team::Glacier;
        w.explode(Vec3::new(832.5,110.7,1400.),MAX_PLAYERS,3,Team::Ember);
        assert_eq!(w.players[0].health,100.);
        w.players[0].pos=Vec3::new(1000.,300.,1000.);w.players[0].team=Team::Ember;
        w.explode(w.players[0].pos,MAX_PLAYERS,3,Team::Ember);
        assert_eq!(w.players[0].health,100.);
    }
    #[test]
    fn base_turrets_fire_plasma_and_towers_keep_bullets() {
        let mut w=world();let defs=equipment::definitions(w.map);
        assert_eq!(defs.iter().filter(|d|d.kind==Kind::Turret && d.weapon==equipment::TurretWeapon::Plasma).count(),4);
        assert_eq!(defs.iter().filter(|d|d.kind==Kind::Turret && d.weapon==equipment::TurretWeapon::Bullet).count(),2);
        let i=defs.iter().position(|d|d.weapon==equipment::TurretWeapon::Plasma).unwrap();
        w.players[0].team=Team::Glacier;w.players[0].pos=defs[i].pos()+Vec3::Y*30.;
        w.players[0].vel=Vec3::X*25.;
        w.step_equipment(STEP);
        assert!(w.discs.iter().any(|d|d.kind==3 && d.owner==MAX_PLAYERS));
        assert!(w.equipment[i].aim.x>0.1,"plasma turret did not lead sideways motion");
        assert!(w.equipment[i].cooldown>1.);
    }
    /// Old Holler's flag towers carry rocket turrets on their canopies:
    /// heat-seeking rockets, fired only at enemies using their jets, each
    /// homing on the one it was fired at.
    #[test]
    fn flag_tower_rocket_turrets_fire_homing_rockets_at_jetting_enemies() {
        let defs=equipment::definitions(MapId::OldHoller);
        let rockets:Vec<usize>=defs.iter().enumerate()
            .filter(|(_,d)|d.kind==Kind::Turret && d.weapon==equipment::TurretWeapon::Rocket).map(|(i,_)|i).collect();
        assert_eq!(rockets.len(),2);
        let kind=crate::registry::registry().projectile_kind("turret-rocket");
        for i in rockets {
            let mut w=world();w.players.truncate(1);
            let d=&defs[i];
            w.players[0].team=if d.team==0 {Team::Glacier} else {Team::Ember};
            // In the open sky 60 m out behind the base.
            let behind=if d.team==0 {-1.} else {1.};
            w.players[0].pos=d.pos()+Vec3::new(0.,10.,60.*behind);
            w.players[0].vel=Vec3::ZERO;w.players[0].jetting=false;
            w.step_equipment(STEP);
            assert!(!w.discs.iter().any(|x|x.kind==kind),"{}: fired at an enemy not jetting",d.id);
            w.players[0].jetting=true;
            w.step_equipment(STEP);
            let rocket=w.discs.iter().find(|x|x.kind==kind).unwrap_or_else(|| panic!("{}: no rocket at a jetting enemy",d.id));
            assert_eq!((rocket.owner,rocket.target),(MAX_PLAYERS,1),"{}: not the turret's rocket homing on the enemy",d.id);
        }
    }
    #[test]
    fn inventory_requires_use_team_distance_and_power() {
        let mut w=world();let defs=equipment::definitions(w.map);
        let Some(i)=defs.iter().position(|d|d.kind==Kind::Inventory && d.team==0) else {return;};
        w.players[0].pos=defs[i].pos();w.players[0].health=25.;w.players[0].energy=10.;
        w.step_equipment(STEP);assert_eq!(w.players[0].health,25.);
        w.input.interact=true;w.step_equipment(STEP);assert!(w.players[0].health>25.);
        w.players[0].team=Team::Glacier;let before=w.players[0].health;
        w.step_equipment(STEP);assert_eq!(w.players[0].health,before);
        w.players[0].team=Team::Ember;w.players[0].pos+=Vec3::X*30.;
        w.step_equipment(STEP);assert_eq!(w.players[0].health,before);
        w.players[0].pos=defs[i].pos();
        for (d,s) in defs.iter().zip(&mut w.equipment) {if d.kind==Kind::Generator && d.team==0 {s.health=0.;}}
        w.step_equipment(STEP);assert_eq!(w.players[0].health,before);assert!(!w.equipment[i].powered);
    }
    /// Stand player 0 near equipment `k` where their repair tool connects,
    /// aimed at it; true when found (the hull is left as it was).
    #[test]
    fn a_generator_in_view_can_always_be_repaired() {
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach, MapId::OzarkticBlast, MapId::Reefbreak] {
            let mut w=World::new(); w.set_map(map); w.start_match(true); w.players.truncate(1);
            let defs=equipment::definitions(map);
            for (k,d) in defs.iter().enumerate().filter(|(_,d)|d.kind==Kind::Generator) {
                w.players[0].team=if d.team==0 {Team::Ember} else {Team::Glacier};
                let (mut ok, mut seen)=(0,0);
                for ring in [2.5f32,3.5,5.0] { for step in 0..24 {
                    let a=step as f32/24.*std::f32::consts::TAU; let at=d.pos();
                    let spot=Vec3::new(at.x+a.sin()*(ring+d.radius), 0.0, at.z+a.cos()*(ring+d.radius));
                    let floor=crate::terrain::support_on(map, Vec3::new(spot.x, at.y+2.0, spot.z)).0;
                    if (floor-at.y).abs()>3.0 {continue;}
                    let spot=Vec3::new(spot.x, floor+PLAYER_RADIUS, spot.z);
                    let eye=spot+Vec3::Y*EYE; let to=at-eye;
                    // Only spots with a clear view of the generator's own surface count.
                    if obstacle_hit(map,&w.pillars,eye,at,0.0).is_some_and(|t| t*to.length() < to.length()-d.radius-0.6) {continue;}
                    seen+=1;
                    let p=&mut w.players[0]; p.pos=spot; p.alive=true; p.energy=crate::defs::movement().energy_max;
                    p.yaw=(-to.x).atan2(-to.z); p.pitch=(to.y/to.length()).asin().clamp(-1.5,1.5);
                    w.input.repair=true;
                    let h=d.max_health()-50.; w.equipment[k].health=h; w.step_repair(STEP);
                    if w.equipment[k].health>h {ok+=1;}
                }}
                assert!(seen > 10 && ok == seen, "{map:?} generator {k}: repairs from {ok} of {seen} spots in view");
            }
        }
    }
    pub(crate) fn aim_repair_at(w:&mut World,k:usize)->bool {
        let d=equipment::definitions(w.map)[k].clone();
        let at=d.pos();
        w.input.repair=true;
        for ring in [3.5f32,5.0,7.0] { for step in 0..16 { for lift in [0.0f32,1.5,-1.0] {
            let a=step as f32/16.*std::f32::consts::TAU;
            let spot=at+Vec3::new(a.sin()*ring,lift,a.cos()*ring);
            let eye=spot+Vec3::Y*EYE;let to=at-eye;
            let p=&mut w.players[0];p.pos=spot;p.alive=true;p.energy=crate::defs::movement().energy_max;p.repair_rest=0.;
            p.yaw=(-to.x).atan2(-to.z);p.pitch=(to.y/to.length()).asin().clamp(-1.5,1.5);
            let before=w.equipment[k].health;
            w.equipment[k].health=before.min(d.max_health()-50.);
            let tested=w.equipment[k].health;
            w.step_repair(STEP);
            let worked=w.equipment[k].health>tested;
            w.equipment[k].health=before;
            if worked {return true;}
        }}}
        false
    }
    #[test]
    fn generators_take_bullet_damage_and_can_be_repaired() {
        let mut w=world();let defs=equipment::definitions(w.map);
        let Some(i)=defs.iter().position(|d|d.kind==Kind::Generator && d.team==0) else {return;};
        let pos=defs[i].pos();w.players[0].pos=pos+Vec3::X*12.;w.players[0].team=Team::Glacier;
        w.discs.push(Disc {pos:pos+Vec3::X*9.,vel:-Vec3::X*crate::defs::weapons().chaingun.speed,owner:0,team:Team::Glacier,kind:1,life:1.,spin:0., target: 0, power: 1.0 });
        w.step_discs(STEP);assert!(w.equipment[i].health<defs[i].max_health());
        // Repairs are the repair tool's: aimed at the wreck, it spends energy
        // and brings it back online past half its hull.
        w.equipment[i].health=0.;w.players[0].team=Team::Ember;w.step_equipment(STEP);
        assert!(w.equipment[i].offline,"a wreck is offline");
        assert!(aim_repair_at(&mut w,i),"somewhere to repair it from");
        let energy=w.players[0].energy;
        w.step_repair(STEP);w.step_equipment(STEP);
        assert!(w.equipment[i].health>0.);assert!(!w.equipment[i].powered,"offline until half repaired");
        assert!(w.players[0].energy<energy);
        for _ in 0..3000 {w.players[0].energy=crate::defs::movement().energy_max;w.step_repair(STEP);w.step_equipment(STEP);if w.equipment[i].powered {break;}}
        assert!(w.equipment[i].powered && w.equipment[i].health>=defs[i].max_health()*crate::equipment::online_fraction());
    }
    #[test]
    fn equipment_snapshots_and_empty_server_reset() {
        let mut m=Match::new(MapId::OldHoller);let Some(s)=m.join(31,"tester") else {panic!()};
        if m.world.equipment.is_empty(){return;}
        m.world.equipment[0].health=12.;m.world.equipment[0].shield=7.5;let snap=m.snapshot();
        let mut client=world();assert!(client.apply_snapshot(&snap,31));assert_eq!(client.equipment[0].health,12.);
        assert_eq!(client.equipment[0].shield,7.5,"shield state reaches clients");
        m.leave(s);assert_eq!(m.world.equipment[0].health,equipment::definitions(m.world.map)[0].max_health());
    }
    #[test]
    fn shielded_turrets_fall_only_after_the_generator_or_a_sustained_attack() {
        let mut w=world();let defs=equipment::definitions(w.map);
        let Some(i)=defs.iter().position(|d|d.kind==Kind::Turret && d.team==0) else {return;};
        let d=&defs[i];assert!(w.equipment[i].shield>0.,"fixed turrets start shielded");
        let blast=d.pos()+Vec3::Y*(d.radius+0.5);
        w.explode(blast,MAX_PLAYERS,0,Team::Glacier);
        assert_eq!(w.equipment[i].health,d.max_health(),"one disc does not reach the hull");
        for (g,s) in defs.iter().zip(&mut w.equipment) {if g.kind==Kind::Generator && g.team==0 {s.health=0.;}}
        w.step_equipment(STEP);
        assert!(!w.equipment[i].powered);assert_eq!(w.equipment[i].shield,0.,"no generator, no shield");
        for _ in 0..40 {w.step_equipment(STEP);}
        assert_eq!(w.equipment[i].shield,0.,"shield cannot regenerate without power");
        let before=w.equipment[i].health;w.explode(blast,MAX_PLAYERS,0,Team::Glacier);
        assert!(w.equipment[i].health<before,"the bare hull takes the blast");
    }
    #[test]
    fn original_base_walk_routes_enter_service_hall_and_reach_roof() {
        if equipment::definitions(MapId::OldHoller).is_empty(){return;}
        for (x,roof) in [(800.,false),(822.,true)] {
            let mut w=world();w.players[0].team=Team::Glacier;
            w.players[0].pos=Vec3::new(x,112.+PLAYER_RADIUS,1340.);
            w.players[0].yaw=std::f32::consts::PI;w.players[0].on_ground=true;
            w.input.move_z=1.;
            for _ in 0..255 {w.physics_step();}
            let p=&w.players[0];
            assert!(p.alive && p.pos.is_finite(),"route must stay playable");
            assert!(p.pos.z>1374.,"route blocked at {:?}",p.pos);
            if roof {assert!(p.pos.y>120.,"roof route: {:?}",p.pos);}
            else {assert!(p.pos.y<110. && p.pos.y>101.,"lower hall: {:?}",p.pos);}
        }
    }
    #[test]
    fn powered_turrets_engage_enemies_and_stop_when_power_is_lost() {
        let mut w=world();let defs=equipment::definitions(w.map);
        let Some(i)=defs.iter().rposition(|d|d.kind==Kind::Turret && d.team==0) else {return;};
        w.players[0].team=Team::Glacier;
        // In front of the turret, inside its field of fire.
        let f=defs[i].facing.map_or(Vec3::X,|f|Vec3::new(f[0],0.,f[1]).normalize());
        w.players[0].pos=defs[i].pos()+f*45.-Vec3::Y*0.8;
        w.step_equipment(STEP);
        assert!(w.discs.iter().any(|d|d.owner==MAX_PLAYERS && d.team==Team::Ember));
        w.discs.clear();
        for (d,s) in defs.iter().zip(&mut w.equipment) {if d.kind==Kind::Generator && d.team==0 {s.health=0.;}}
        for _ in 0..60 {w.step_equipment(STEP);}
        assert!(!w.discs.iter().any(|d|d.team==Team::Ember));
    }
}

/// Visual size of a generator's destruction blast (kind 4). Cosmetic only.
pub const GENERATOR_BLAST_RADIUS: f32 = 11.;
/// A burst from a kind with no blast of its own (none in the base game):
/// the old generic curve.
const FALLBACK_BLAST: crate::registry::Blast = crate::registry::Blast { falloff: crate::registry::Falloff::Quadratic, radius: 9.0,
    max_damage: 0.0, edge_fraction: 0.0, impulse: 0.0, core: 0.0, edge_damage: 12.0, core_damage: 68.0, kick: 6.0 };
/// The projectile whose blast is a lava splash (`World::lava_splash`).
/// Content may retune it, or remove it to turn splashes off.
pub const LAVA_SPLASH: &str = "volcanic-splash";

/// Splash reaches equipment when nothing but the equipment's own mount lies
/// between the blast and the object. The collision mesh does not tag which
/// solid belongs to which object, so a hit inside the object's footprint column
/// (its mount, pedestal or housing) counts as reaching it. Walls farther out
/// still block the blast.
fn splash_reaches(map:MapId,pillars:&[Pillar],blast:Vec3,end:Vec3,d:&crate::equipment::Definition)->bool {
    let Some(t)=obstacle_hit(map,pillars,blast,end,0.) else {return true};
    if t>=0.995 {return true;}
    let hit=blast.lerp(end,t);let c=d.pos();
    let horizontal=Vec3::new(hit.x-c.x,0.,hit.z-c.z).length();
    horizontal<=d.radius+EQUIPMENT_MOUNT_MARGIN && hit.y<=c.y+d.radius && hit.y>=c.y-EQUIPMENT_MOUNT_DEPTH
}
/// How far an equipment mount extends beyond the object's hit radius, and how
/// far below its centre, for `splash_reaches`.
const EQUIPMENT_MOUNT_MARGIN: f32 = 1.0;
const EQUIPMENT_MOUNT_DEPTH: f32 = 6.0;

fn player_blast_distance(blast:Vec3,player:Vec3)->f32 {
    let spine=Vec3::new(player.x,blast.y.clamp(player.y,player.y+EYE),player.z);
    (blast.distance(spine)-PLAYER_RADIUS).max(0.)
}

fn landing_damage(impact_speed: f32) -> f32 {
    let m = crate::defs::movement();
    (((impact_speed - m.landing_speed).max(0.0) * m.landing_scale).min(m.landing_cap) * m.landing_share).floor()
}

fn grenade_contact_normal(map: MapId, pillars: &[Pillar], p: Vec3, far: f32, incoming:Vec3) -> Vec3 {
    if let Some(pack)=crate::map_pack::on(map) {
        let direction=incoming.normalize_or_zero();
        if let Some((_,n))=pack.sweep(p-direction*0.3,p+direction*0.3,0.12) {return n;}
    }
    if p.x <= 1.001 { return Vec3::X; }
    if p.x >= far - 0.001 { return -Vec3::X; }
    if p.z <= 1.001 { return Vec3::Z; }
    if p.z >= far - 0.001 { return -Vec3::Z; }
    let (height, normal) = crate::terrain::surface_on(map, p.x, p.z);
    if p.y <= height + 0.15 { return normal; }
    for pillar in pillars {
        let half = pillar.r * 0.8 + 0.12;
        let base = crate::terrain::height_on(map, pillar.x, pillar.z);
        if (p.x - pillar.x).abs() <= half + 0.02 && (p.z - pillar.z).abs() <= half + 0.02
            && p.y >= base - 0.14 && p.y <= base + pillar.h + 0.14 {
            let faces = [((p.x - pillar.x - half).abs(), Vec3::X),
                ((p.x - pillar.x + half).abs(), -Vec3::X),
                ((p.z - pillar.z - half).abs(), Vec3::Z),
                ((p.z - pillar.z + half).abs(), -Vec3::Z),
                ((p.y - base - pillar.h - 0.12).abs(), Vec3::Y)];
            return faces.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap().1;
        }
    }
    normal
}

pub(crate) fn jet_falloff(speed: f32) -> f32 {
    jet_falloff_to(speed, standard_armor().air_cap)
}

/// The jump-boost a skier's jets give when pressed on the ground
/// (`movement.jet_jump`, a share of the armor's jump), at the standard mass
/// and before a climb already under way shrinks it: what bots plan with.
pub(crate) fn jet_kick(armor: &Armor) -> f32 {
    let m = crate::defs::movement();
    m.jump_impulse * armor.jump * m.jet_jump
}

/// Thrust share at `speed` for a jet that adds speed only up to `cap`.
pub(crate) fn jet_falloff_to(speed: f32, cap: f32) -> f32 {
    let knee = cap * 0.8;
    if speed <= knee {
        1.0
    } else if speed >= cap {
        0.0
    } else {
        1.0 - (speed - knee) / (cap - knee)
    }
}

fn ride_ski(map: MapId, p: &mut Player, wish: Vec3, drag: f32, dt: f32) {
    // Gravity was already applied straight down. On frictionless snow that
    // into-ground part becomes slide. Descending multiplies that slide.
    let n = contact_normal(map,p.pos);
    let vn = p.vel.dot(n);
    if vn < 0.0 {
        p.vel -= n * vn;
    }
    let g = Vec3::new(0.0, -gravity_for_speed(Vec2::new(p.vel.x, p.vel.z).length()), 0.0);
    let tangent = g - n * g.dot(n);
    let down = Vec3::new(tangent.x, 0.0, tangent.z);
    let moving_down = Vec3::new(p.vel.x, 0.0, p.vel.z).dot(down) > 0.0;
    if moving_down {
        p.vel += tangent * (crate::defs::movement().ski_slope_gravity - 1.0) * dt;
    }
    if drag > 0.0 {
        // Friction on the flat, gone on a steep face (n.y: 1 flat, 0 a wall).
        let flatness = ((n.y - 0.8) / 0.2).clamp(0.0, 1.0);
        let speed = p.vel.length();
        if speed > 0.0 { p.vel *= (speed - drag * flatness * dt).max(0.0) / speed; }
    }
    let speed = p.vel.length();
    if speed > 1.2 && wish.length_squared() > 0.04 {
        // Steer in the slope plane, preserving total speed. Fixed acceleration
        // gives wide high-speed turns instead of the old instant rail-like carve.
        let tangent_wish = wish - n * wish.dot(n);
        p.vel = (p.vel + tangent_wish * crate::defs::movement().ski_turn_accel * dt).normalize_or_zero() * speed;
    }
}

fn apply_speed_limits(p: &mut Player, dt: f32) {
    let h = Vec3::new(p.vel.x, 0.0, p.vel.z).length();
    if h > 0.01 {
        let mut nh = h;
        if nh > crate::defs::movement().horiz_max {
            nh = crate::defs::movement().horiz_max;
        }
        let s = nh / h;
        p.vel.x *= s;
        p.vel.z *= s;
    }
    if p.vel.y > crate::defs::movement().up_resist_speed {
        let excess = p.vel.y - crate::defs::movement().up_resist_speed;
        p.vel.y = crate::defs::movement().up_resist_speed + excess * (1.0 - crate::defs::movement().up_resist * dt).max(0.0);
    }
    if p.vel.y > crate::defs::movement().up_max {
        p.vel.y = crate::defs::movement().up_max;
    }
}

/// For the snapshot wire codec (`peakrunner_protocol::delta`), which builds
/// players field by field: the private state a client's prediction replays
/// from (the jump edge and coyote time). Server-only bot fields aren't sent;
/// decoded players keep `wire_blank`'s.
impl Player {
    /// Left out of this snapshot by its viewer's team (`HIDDEN_Y`).
    pub fn hidden(&self) -> bool { self.pos.y <= HIDDEN_Y + 1.0 }
    /// A bot's role as armors' `bot_roles` name it.
    pub fn bot_role_name(&self) -> &'static str {
        match self.bot_role { BotRole::Defense => "defense", BotRole::Hunter => "hunter", _ => "offense" }
    }
}

/// A body drawn `alpha` (0..1) of a step past where the latest fixed step
/// found it (`was`), as an offset from its position now. Teleports (a
/// respawn) and the dead snap: any gap wider than a step covers at top
/// speed is a teleport.
fn between_steps(p: &Player, was: Vec3, alpha: f32) -> Vec3 {
    const INTERP_SNAP: f32 = 3.0;
    let back = was - p.pos;
    if p.alive && back.length() < INTERP_SNAP { back * (1.0 - alpha.clamp(0.0, 1.0)) } else { Vec3::ZERO }
}

impl Player {
    pub fn wire_blank() -> Player { make_player(Team::Ember, false, Vec3::ZERO, 0.0, BotRole::Offense) }
    pub fn wire_private(&self) -> (bool, bool, f32) { (self.jump_prev, self.jet_prev, self.coyote) }
    pub fn set_wire_private(&mut self, jump_prev: bool, jet_prev: bool, coyote: f32) {
        (self.jump_prev, self.jet_prev, self.coyote) = (jump_prev, jet_prev, coyote);
    }
}

fn make_player(team: Team, bot: bool, pos: Vec3, yaw: f32, role: BotRole) -> Player {
    Player {
        net_id: 0,
        name: String::new(),
        frags: 0,
        losses: 0,
        shots: 0,
        hits: 0,
        jump_prev: false,
        jet_prev: false,
        pos,
        vel: Vec3::ZERO,
        yaw,
        pitch: -0.08,
        health: 100.0,
        energy: crate::defs::movement().energy_max,
        team,
        alive: true,
        respawn: 0.0,
        carrying: None,
        is_bot: bot,
        remote: false,
        on_ground: true,
        skiing: false,
        jetting: false,

        cooldown: 0.0,
        weapon: 0,
        armor: ArmorClass::Light,
        ammo: loadout::max_ammo(ArmorClass::Light),
        loadout: arsenal::Loadout::default(),
        setup: Setup::default(),
        pack: None,
        pack_on: false,
        pack_clock: 0.0,
        pack_uses: 0,
        ability_clock: 0.0,
        ability_uses: 0,
        strain: 0.0,
        ability_on: false,
        footing: false,
        riding: 0,
        credits: 0.0,
        mount_clocks: [0.0; packs::MAX_MOUNTS],
        teleport_clock: 0.0,
        zones_in: 0,
        bot_press: 0,
        claim_on: 0,
        claim_time: 0.0,
        shock_clock: 0.0,
        swap_back: 0,
        swap_clock: 0.0,
        throwables: throwables::max_throwables(ArmorClass::Light),
        throw_clock: 0.0,
        throw_held: 0,
        repair_beam: None,
        repair_out: false,
        repair_self: false,
        repair_rest: 0.0,
        spin: 0.0,
        charge: 0.0,
        burst_left: 0,
        fire_mode: 0,
        mode_of: 0,
        fire_prev: false,
        lock_target: 0,
        lock_time: 0.0,
        stream: None,
        effects: Vec::new(),
        blade: blade::State::default(),
        bot_role: role,
        bot_goal: pos,
        bot_think: 0.0,
        bot_target: None,
        bot_seen: pos,
        bot_memory: 0.0,
        bot_profile: crate::bot_nav::BotProfile::default(),
        bot_path: Vec::new(),
        bot_path_i: 0,
        bot_path_goal: pos,
        bot_replan: 0.0,
        bot_stuck: 0.0,
        bot_best: f32::MAX,
        bot_link: None,
        bot_saving: false,
        bot_seen_for: 0.0,
        bot_aim: Vec2::ZERO,
        bot_repair: false,
        bot_avoid: Vec::new(),
        bot_fly: false,
        bot_pass: false,
        bot_leap: 0.0,
        bot_pounce: 0.0,
        bot_fb: Default::default(),
        bot_sight: Vec::new(),
        spar: blade::Spar::default(),
        stun: 0.0,
        suicide_in: 0.0,
        coyote: 0.0,
    }
}

#[cfg(test)]
mod spawn_point_tests {
    use super::*;

    #[test]
    fn tower_spawn_points_land_on_the_floor_and_walking_brakes() {
        // Regression: a spawn centre only 0.2 m above a 1 m slab started the
        // support ray inside the slab, so the player never counted as grounded
        // and coasted without ground braking (the "slick" spawn).
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
        let mut world = World::new();
        world.set_map(map);
        world.start_match(true);
        world.players.truncate(1);
        world.player_id = 0;
        for ember in [true, false] {
            let points = crate::terrain::spawn_points_on(map, ember);
            assert!(points.len() >= 6);
            for (pos, yaw) in points {
                let floor = crate::terrain::support_on(map, pos).0;
                assert!((pos.y - floor - 1.2).abs() < 0.01, "spawn {pos:?} support {floor}");
                let p = &mut world.players[0];
                p.pos = pos; p.vel = Vec3::ZERO; p.yaw = yaw; p.alive = true; p.health = 100.0;
                p.on_ground = true; p.skiing = false; p.jetting = false;
                world.input = Input::default();
                for _ in 0..30 { world.step_players(STEP); }
                let p = &world.players[0];
                assert!(p.on_ground, "{map:?} not grounded at {pos:?}");
                assert!((p.pos.y - floor - PLAYER_RADIUS).abs() < 0.05, "sunk or floating at {pos:?}: {}", p.pos.y);
                // Walk forward for half a second, release, and stop like flat ground.
                let start = p.pos;
                world.input.move_z = 1.0;
                for _ in 0..30 { world.step_players(STEP); }
                world.input.move_z = 0.0;
                let released = world.players[0].pos;
                // Spawns face open floor, so the half-second walk stays on the
                // spawn's own floor and must brake like flat ground.
                assert!((crate::terrain::support_on(map, released).0 - floor).abs() < 0.3
                    && (released.y - floor - PLAYER_RADIUS).abs() < 0.05,
                    "{map:?} walk from {start:?} left the spawn floor (faces an edge or wall)");
                for _ in 0..90 { world.step_players(STEP); }
                let p = &world.players[0];
                let coast = Vec2::new(p.pos.x - released.x, p.pos.z - released.z).length();
                assert!(p.on_ground && coast < 2.2, "{map:?} slides {coast} m after release from {start:?}");
                assert!(Vec2::new(p.vel.x, p.vel.z).length() < 0.05);
            }
        }
        }
    }

    /// A skier holding ski, with no steering, enters Frostline's ice cavern
    /// from either trench and coasts out through the far trench: no snag on a
    /// wall, seam or mouth, no fall through the floor, and the dip keeps
    /// momentum. Uses the real movement code on the embedded pack.
    #[test]
    fn frostline_cavern_skis_through_mouth_to_mouth() {
        let map = MapId::Frostline;
        for (z0, dir, speed) in [(950.0_f32, 1.0_f32, 30.0_f32), (1098.0, -1.0, 30.0), (950.0, 1.0, 16.0)] {
            let mut world = World::new();
            world.set_map(map);
            world.start_match(true);
            world.players.truncate(1);
            world.player_id = 0;
            let floor = crate::terrain::support_on(map, Vec3::new(1024.0, 226.0, z0)).0;
            let p = &mut world.players[0];
            p.pos = Vec3::new(1024.0, floor + PLAYER_RADIUS, z0);
            p.vel = Vec3::new(0.0, 0.0, dir * speed);
            p.yaw = if dir > 0.0 { std::f32::consts::PI } else { 0.0 };
            p.alive = true; p.health = 100.0; p.on_ground = true; p.skiing = true; p.jetting = false;
            world.input = Input::default();
            world.input.jump = true;
            let (mut min_speed, mut exit_speed) = (f32::MAX, None);
            for _ in 0..(60 * 12) {
                world.step_players(STEP);
                let p = &world.players[0];
                let d = p.pos.z - 1024.0;
                assert!(p.pos.y > 210.0, "fell through at {:?}", p.pos);
                if d.abs() < 48.0 {
                    assert!((p.pos.x - 1024.0).abs() < 11.4, "drifted into the wall at {:?}", p.pos);
                    min_speed = min_speed.min(Vec2::new(p.vel.x, p.vel.z).length());
                }
                if d * dir > 72.0 { exit_speed = Some(Vec2::new(p.vel.x, p.vel.z).length()); break; }
            }
            let exit = exit_speed.unwrap_or_else(|| panic!("skier from z {z0} at {speed} m/s never left the far trench: {:?}", world.players[0].pos));
            assert!(min_speed > 8.0, "skier from z {z0} slowed to {min_speed} m/s inside");
            assert!(exit > speed * 0.6, "skier from z {z0} left at {exit} m/s after entering at {speed}");
        }
    }

    /// Dustreach's sewer: a skier holding ski, with no steering, drops into
    /// the red leg at its mouth and coasts the whole 656 m leg to the cross
    /// hall under the Sun Gate: no snag on a wall, seam or rib, no fall
    /// through the floor, and the rolling floor keeps momentum. The blue leg
    /// is the red one rotated 180 degrees. Real movement code, embedded pack.
    #[test]
    fn dustreach_sewer_leg_skis_mouth_to_hall() {
        let map = MapId::Dustreach;
        for speed in [30.0_f32, 16.0] {
            let mut world = World::new();
            world.set_map(map);
            world.start_match(true);
            world.players.truncate(1);
            world.player_id = 0;
            let pack = crate::map_pack::on(map).unwrap();
            // Probe from just over the open trench's floor at the portal: a
            // probe from higher up would find the culvert's roof.
            let lip = pack.floor(Vec3::new(1076.0, 200.0, 356.0)).expect("trench floor").0;
            let floor = pack.floor(Vec3::new(1076.0, lip + 3.0, 364.0)).expect("leg floor").0;
            let p = &mut world.players[0];
            p.pos = Vec3::new(1076.0, floor + PLAYER_RADIUS, 364.0);
            p.vel = Vec3::new(0.0, 0.0, speed);
            p.yaw = std::f32::consts::PI;
            p.alive = true; p.health = 100.0; p.on_ground = true; p.skiing = true; p.jetting = false;
            world.input = Input::default();
            world.input.jump = true;
            let (mut min_speed, mut arrived) = (f32::MAX, None);
            for _ in 0..(60 * 60) {
                world.step_players(STEP);
                let p = &world.players[0];
                // The leg's lowest floor is about 99.5 m.
                assert!(p.pos.y > 95.0, "fell through at {:?}", p.pos);
                assert!((p.pos.x - 1076.0).abs() < 3.5, "drifted into the wall at {:?}", p.pos);
                min_speed = min_speed.min(Vec2::new(p.vel.x, p.vel.z).length());
                if p.pos.z > 1010.0 { arrived = Some(Vec2::new(p.vel.x, p.vel.z).length()); break; }
            }
            let at_hall = arrived.unwrap_or_else(|| panic!("skier at {speed} m/s never reached the hall: {:?}", world.players[0].pos));
            eprintln!("sewer leg ski: in {speed} m/s, slowest {min_speed:.1}, at the hall {at_hall:.1}");
            assert!(min_speed > 5.0, "skier entering at {speed} m/s slowed to {min_speed} m/s");
        }
    }

    /// Every opening can be left by jet with the real movement code: from the
    /// floor of each drop shaft (midfield and flank, both teams) and from each
    /// mouth's portal, a player holding jet rises past the lip, steers out
    /// over the ground and lands standing outside the cut cell.
    #[test]
    fn dustreach_sewer_shafts_and_mouths_jet_out() {
        let map = MapId::Dustreach;
        let pack = crate::map_pack::on(map).unwrap();
        // (red start x, z, cell x0, x1, z0, z1, outside target x, z)
        let red: [(f32, f32, f32, f32, f32, f32, f32, f32); 3] = [
            (1076.0, 940.0, 1072.0, 1080.0, 936.0, 944.0, 1090.0, 940.0),   // midfield shaft
            (1156.0, 396.0, 1152.0, 1160.0, 392.0, 400.0, 1170.0, 396.0),   // flank shaft
            (1076.0, 358.0, 1072.0, 1080.0, 296.0, 360.0, 1090.0, 352.0)];  // red mouth, out over the trench's east wall
        for &(sx, sz, x0, x1, z0, z1, tx, tz) in &red {
            for blue in [false, true] {
                let m = |x: f32, z: f32| if blue { (2048.0 - x, 2048.0 - z) } else { (x, z) };
                let (sx, sz) = m(sx, sz); let (tx, tz) = m(tx, tz);
                let ((ax, az), (bx, bz)) = (m(x0, z0), m(x1, z1));
                let (cx0, cx1, cz0, cz1) = (ax.min(bx), ax.max(bx), az.min(bz), az.max(bz));
                let mut world = World::new();
                world.set_map(map);
                world.start_match(true);
                world.players.truncate(1);
                world.player_id = 0;
                let lip = surface_on_edge_max(map, cx0, cx1, cz0, cz1);
                let floor = pack.floor(Vec3::new(sx, lip - 1.0, sz)).expect("opening floor").0;
                {
                    let p = &mut world.players[0];
                    p.pos = Vec3::new(sx, floor + PLAYER_RADIUS, sz);
                    p.vel = Vec3::ZERO; p.energy = crate::defs::movement().energy_max;
                    p.alive = true; p.health = 100.0; p.on_ground = true; p.skiing = false; p.jetting = false;
                    p.yaw = (-(tx - sx)).atan2(-(tz - sz));
                }
                world.input = Input::default();
                let mut out = false;
                for _ in 0..(60 * 10) {
                    let p = &world.players[0];
                    let above = p.pos.y > lip + 2.0;
                    world.input.jet = !above || p.vel.y < 0.0 && p.pos.y < lip + 1.0;
                    world.input.move_z = if above { 1.0 } else { 0.0 };
                    world.step_players(STEP);
                    let p = &world.players[0];
                    let inside = p.pos.x > cx0 && p.pos.x < cx1 && p.pos.z > cz0 && p.pos.z < cz1;
                    if !inside && p.on_ground { out = true; break; }
                }
                let p = &world.players[0];
                assert!(out, "{} opening at ({sx}, {sz}): no way out by jet, ended at {:?}", if blue { "blue" } else { "red" }, p.pos);
            }
        }
    }

    fn surface_on_edge_max(map: MapId, x0: f32, x1: f32, z0: f32, z1: f32) -> f32 {
        let mut top = f32::MIN;
        for k in 0..=16 {
            let t = k as f32 / 16.0;
            for (x, z) in [(x0 + (x1 - x0) * t, z0), (x0 + (x1 - x0) * t, z1), (x0, z0 + (z1 - z0) * t), (x1, z0 + (z1 - z0) * t)] {
                top = top.max(crate::terrain::surface_on(map, x, z).0);
            }
        }
        top
    }

    /// Evidence, not a rule: crossing Dustreach from the red mouth's top to
    /// the blue mouth's top, over the dunes (through the Sun Gate) versus
    /// through the sewer, with the same simple ski-and-jet driver. Run with
    /// `cargo test -p peakrunner-core --lib dustreach_surface_versus_sewer -- --ignored --nocapture`.
    #[test]
    #[ignore = "route timing probe"]
    fn dustreach_surface_versus_sewer_crossing_time() {
        let map = MapId::Dustreach;
        let pack = crate::map_pack::on(map).unwrap();
        let at = |x: f32, z: f32, below: f32| {
            let y = pack.floor(Vec3::new(x, below, z)).map(|f| f.0).unwrap_or_else(|| crate::terrain::surface_on(map, x, z).0);
            Vec3::new(x, y, z)
        };
        let start = at(1096.0, 302.0, 400.0);
        let finish = at(952.0, 1746.0, 400.0);
        let hall = pack.floor(Vec3::new(1024.0, 140.0, 1024.0)).unwrap().0;
        let surface = vec![start, finish];
        let sewer = vec![start, at(1076.0, 304.0, 400.0), at(1076.0, 364.0, 140.0), at(1076.0, 1020.0, hall + 3.0),
            at(972.0, 1028.0, hall + 3.0), at(972.0, 1684.0, 140.0), at(972.0, 1744.0, 400.0), finish];
        let run = |route: &[Vec3]| -> Option<f32> {
            let mut world = World::new();
            world.set_map(map);
            world.start_match(true);
            world.players.truncate(1);
            world.player_id = 0;
            {
                let p = &mut world.players[0];
                p.pos = route[0] + Vec3::Y * PLAYER_RADIUS;
                p.vel = Vec3::ZERO; p.energy = crate::defs::movement().energy_max;
                p.alive = true; p.health = 100.0; p.on_ground = true;
            }
            world.input = Input::default();
            let mut k = 1;
            for tick in 0..(60 * 240) {
                let p = &world.players[0];
                let target = route[k];
                let d = Vec2::new(target.x - p.pos.x, target.z - p.pos.z);
                if d.length() < if k + 1 == route.len() { 5.0 } else { 3.0 } {
                    if k + 1 == route.len() { return Some(tick as f32 * STEP); }
                    k += 1;
                    continue;
                }
                let speed = Vec2::new(p.vel.x, p.vel.z).length();
                let climb = target.y > p.pos.y + 2.0;
                world.players[0].yaw = (-d.x).atan2(-d.y);
                world.input.move_z = 1.0;
                world.input.jump = true;
                world.input.jet = climb || speed < 18.0;
                world.step_players(STEP);
            }
            None
        };
        let (a, b) = (run(&surface), run(&sewer));
        eprintln!("Dustreach crossing, red mouth top to blue mouth top: surface {a:?} s, sewer {b:?} s");
        assert!(a.is_some() && b.is_some(), "both routes must finish");
        assert!(a.unwrap() < b.unwrap(), "the surface should stay the faster crossing");
    }

    /// Old Holler's flag towers on the halls' roofs, both teams. Using only
    /// inputs (facing, W, jet) and the real movement code, a player jets up
    /// onto the deck from the roof in front of the tower and on either side,
    /// standing on the deck each time; and leaves again the same way.
    #[test]
    fn old_holler_flag_tower_entries_and_exits_are_flyable() {
        const DECK: f32 = 17.6;
        const TZ: f32 = 20.0;
        const ROOF: f32 = 8.6;
        let map = MapId::OldHoller;
        type Route = (&'static str, (f32, f32, f32), Vec<(f32, f32, f32, bool)>, Vec<(f32, f32, f32, bool)>);
        let routes: [Route; 3] = [
            ("front", (0., ROOF, 6.),
                vec![(0., DECK + 1.5, 9.), (0., DECK + 1., 14.), (0., DECK, 17.)].into_iter()
                    .map(|(x, y, z)| (x, y, z, y > DECK)).collect(),
                vec![(0., DECK, 14.5, false), (0., ROOF, 9., false)]),
            ("east", (16., ROOF, TZ),
                vec![(12., DECK + 1.5, TZ, true), (5., DECK + 1., TZ, true), (2., DECK, TZ, false)],
                vec![(6., DECK, TZ, false), (11., ROOF, TZ, false)]),
            ("west", (-16., ROOF, TZ),
                vec![(-12., DECK + 1.5, TZ, true), (-5., DECK + 1., TZ, true), (-2., DECK, TZ, false)],
                vec![(-6., DECK, TZ, false), (-11., ROOF, TZ, false)]),
        ];
        for team in [0u8, 1] {
            let to_world = |x: f32, y: f32, z: f32| if team == 0 {
                Vec3::new(1160. - x, 112. + y, 480. - z) } else { Vec3::new(800. + x, 112. + y, 1400. + z) };
            let axis = to_world(0., DECK, TZ);
            for (name, start, way, out) in &routes {
                let mut world = World::new();
                world.set_map(map);
                world.start_match(true);
                world.players.truncate(1);
                world.player_id = 0;
                let p = &mut world.players[0];
                p.pos = to_world(start.0, start.1, start.2) + Vec3::Y * PLAYER_RADIUS;
                p.vel = Vec3::ZERO; p.alive = true; p.health = 100.; p.energy = crate::defs::movement().energy_max;
                p.on_ground = true; p.skiing = false; p.jetting = false;
                let wps: Vec<(Vec3, bool)> = way.iter().map(|w| (to_world(w.0, w.1, w.2), w.3)).collect();
                fly(&mut world, &wps, 60 * 25).unwrap_or_else(|e| panic!("team {team} {name} in: {e}"));
                let p = &world.players[0];
                assert!(p.on_ground && (p.pos.y - axis.y - PLAYER_RADIUS).abs() < 0.05,
                    "team {team} {name}: not standing on the deck at {:?}", p.pos);
                assert!(Vec2::new(p.pos.x - axis.x, p.pos.z - axis.z).length() < 7.0,
                    "team {team} {name}: ended off the deck at {:?}", p.pos);
                let back: Vec<(Vec3, bool)> = out.iter().map(|w| (to_world(w.0, w.1, w.2), w.3)).collect();
                world.players[0].energy = crate::defs::movement().energy_max;
                fly(&mut world, &back, 60 * 25).unwrap_or_else(|e| panic!("team {team} {name} out: {e}"));
                let p = &world.players[0];
                assert!(Vec2::new(p.pos.x - axis.x, p.pos.z - axis.z).length() > 8.5 && p.pos.y < axis.y - 5.,
                    "team {team} {name}: did not leave the tower, at {:?}", p.pos);
            }
        }
    }

    /// Old Holler's flag decks are open on every side. An enemy flying
    /// straight across at 20, 30 or 40 m/s, jetting only to hold height, grabs
    /// the flag without touching anything and leaves at nearly full speed,
    /// along both axes, both ways, on both teams' towers.
    #[test]
    fn old_holler_flag_deck_fly_through_grabs_the_flag_at_speed() {
        const FLOOR: f32 = 17.6;
        const TZ: f32 = 20.0;
        for team in [0u8, 1] {
            let to_world = |x: f32, y: f32, z: f32| if team == 0 {
                Vec3::new(1160. - x, 112. + y, 480. - z) } else { Vec3::new(800. + x, 112. + y, 1400. + z) };
            // (name, local start): front to back and back to front, east to west and west to east.
            let passes = [("front-back", (0., TZ - 15.)), ("back-front", (0., TZ + 15.)),
                          ("east-west", (15., TZ)), ("west-east", (-15., TZ))];
            for (name, (x0, z0)) in passes {
                for speed in [20.0f32, 30., 40.] {
                    let mut world = World::new();
                    world.set_map(MapId::OldHoller);
                    world.start_match(true);
                    world.players.truncate(1);
                    world.player_id = 0;
                    let fi = team as usize;
                    let home = world.flags[fi].home;
                    let start = to_world(x0, FLOOR + 1.9, z0);
                    let through = to_world(-x0, FLOOR + 1.9, 2. * TZ - z0);
                    let dir = (through - start).normalize();
                    let p = &mut world.players[0];
                    p.team = if team == 0 { Team::Glacier } else { Team::Ember };
                    p.carrying = None;
                    p.pos = start; p.vel = dir * speed; p.alive = true; p.health = 100.;
                    p.energy = crate::defs::movement().energy_max; p.on_ground = false; p.skiing = false; p.jetting = false;
                    p.yaw = (-dir.x).atan2(-dir.z);
                    let hold = start.y;
                    let mut slowest = speed;
                    let mut crossed = false;
                    for _ in 0..240 {
                        let (pos, vel) = (world.players[0].pos, world.players[0].vel);
                        if (pos - start).dot(dir) > 30. { crossed = true; break; }
                        let mut input = Input::default();
                        input.jet = pos.y + vel.y * vel.y.abs() / 40. < hold;
                        world.input = input;
                        let before = vec![world.players[0].pos];
                        world.step_players(STEP);
                        world.step_flags(&before, STEP);
                        slowest = slowest.min(Vec2::new(world.players[0].vel.x, world.players[0].vel.z).length());
                    }
                    let p = &world.players[0];
                    assert!(crossed, "team {team} {name} at {speed}: stuck at {:?}", p.pos);
                    assert_eq!(world.flags[fi].carrier, Some(0), "team {team} {name} at {speed}: flag not grabbed (home {home:?})");
                    assert!(slowest > 0.9 * speed, "team {team} {name} at {speed}: slowed to {slowest}, hit something");
                }
            }
        }
    }

    /// Steer the one player through waypoints with inputs only (facing, W,
    /// jet). A waypoint with `jet` holds its height with the jet and is
    /// reached within 0.8 m across and 1.2 m of height; one without is reached
    /// on foot, or by falling onto it, within 0.8 m across and 0.6 m of height.
    fn fly(world: &mut World, wps: &[(Vec3, bool)], ticks: usize) -> Result<(), String> {
        let mut k = 0;
        for _ in 0..ticks {
            let (target, jet) = wps[k];
            let (pos, vel, ground) = { let p = &world.players[0]; (p.pos, p.vel, p.on_ground) };
            if !pos.is_finite() || !world.players[0].alive { return Err(format!("lost the player at {pos:?}")); }
            let to = Vec3::new(target.x - pos.x, 0., target.z - pos.z);
            let dist = to.length();
            let reached = dist < 0.8 && if jet { (pos.y - PLAYER_RADIUS - target.y).abs() < 1.2 }
                else { (pos.y - PLAYER_RADIUS - target.y).abs() < 0.6 };
            if reached {
                k += 1;
                if k == wps.len() {
                    for _ in 0..60 { world.input = Input::default(); world.step_players(STEP); }
                    return Ok(());
                }
                continue;
            }
            let mut input = Input::default();
            // Steer by velocity error: want a speed that can still stop in the
            // remaining distance, straight at the target; face the difference
            // between that and the current velocity and push along it. This
            // cancels sideways drift, so the player does not orbit a waypoint.
            let brake = if ground { 20. } else { 6. };
            let wanted = if dist > 1e-3 { to / dist * (2. * brake * dist).sqrt().min(if ground { 6. } else { 4. }) } else { Vec3::ZERO };
            let error = wanted - Vec3::new(vel.x, 0., vel.z);
            if error.length() > 0.3 {
                world.players[0].yaw = (-error.x).atan2(-error.z);
                input.move_z = 1.;
            }
            // Hold height by the apex a coast would reach (gravity 20 m/s/s).
            let apex = pos.y - PLAYER_RADIUS + vel.y * vel.y.abs() / 40.;
            input.jet = jet && apex < target.y;
            world.input = input;
            world.step_players(STEP);
        }
        let p = &world.players[0];
        Err(format!("stuck before waypoint {k} {:?} at {:?} vel {:?}", wps[k].0, p.pos, p.vel))
    }

    #[test]
    fn server_respawn_picks_varied_points_for_the_right_team() {
        for map in [MapId::TowerComplex, MapId::OldHoller] {
        let mut world = World::new();
        world.set_map(map);
        world.start_match(true);
        for team in [Team::Ember, Team::Glacier] {
            let points = crate::terrain::spawn_points_on(map, team == Team::Ember);
            let i = world.players.iter().position(|p| p.team == team).unwrap();
            let mut used = std::collections::BTreeSet::new();
            let mut last = None;
            for _ in 0..60 {
                world.respawn(i);
                let p = &world.players[i];
                let k = points.iter().position(|(pos, _)| *pos == p.pos).expect("respawned on an authored point");
                assert_eq!(p.yaw, points[k].1);
                assert_ne!(Some(k), last, "same point twice in a row");
                last = Some(k);
                used.insert(k);
            }
            assert!(used.len() >= points.len() - 1, "{map:?} {team:?} used only {used:?}");
        }
        }
        // Maps without spawn_points keep the legacy single-spawn path.
        assert!(crate::terrain::spawn_points_on(MapId::Valley, false).is_empty());
    }
}

#[cfg(test)]
mod line_of_sight_tests {
    use super::*;
    use crate::equipment::{self,Kind};

    fn world(map:MapId)->World {
        let mut w=World::new();w.set_map(map);w.start_rift(true);w.players.truncate(1);
        w.players[0].team=Team::Glacier;w.players[0].alive=true;w.players[0].vel=Vec3::ZERO;w
    }
    fn spawns(map:MapId)->Vec<Vec3> {
        let m=&crate::map_pack::on(map).unwrap().manifest;
        let mut v:Vec<Vec3>=m.spawn_points.iter().flatten().map(|p|Vec3::new(p[0],p[1],p[2])).collect();
        v.extend(m.spawns.iter().map(|p|Vec3::from_array(*p)));v
    }
    fn fired_by(w:&World,turret:usize)->bool {w.discs.iter().any(|d|d.owner==MAX_PLAYERS && d.spin==turret as f32)}

    /// Doorways and windows are open: a turret may see this far past an
    /// opening's inner face, and no further into a room.
    const DOOR_DEPTH:f32=3.;
    /// Whether turret `d` would engage an enemy standing at `pos`: field of
    /// fire, sensor-boosted range and a clear barrel-to-chest line, exactly as
    /// `step_equipment` decides it.
    fn engages(map:MapId,d:&equipment::Definition,pos:Vec3)->bool {
        let Some(profile)=equipment::profile(d.kind,d.weapon) else {return false};
        if !d.in_arc(pos) {return false;}
        let enemy=equipment::Candidate {index:0,team:1-d.team,pos,vel:Vec3::ZERO,jetting:false};
        equipment::acquire_target(d.pos(),d.radius,d.team,&profile,true,[enemy],
            |a,b|obstacle_hit(map,&[],a,b,0.).is_none()).is_some()
    }
    /// Whether `p` lies within DOOR_DEPTH (horizontally) of any opening's
    /// inner face, given as world-space (x, z) segments.
    fn in_doorway(p:Vec3,openings:&[(Vec2,Vec2)])->bool {
        let q=Vec2::new(p.x,p.z);
        openings.iter().any(|&(a,b)| {
            let ab=b-a;let t=((q-a).dot(ab)/ab.length_squared().max(1e-9)).clamp(0.,1.);
            q.distance(a+ab*t)<DOOR_DEPTH
        })
    }

    /// Every Ember turret, against a Glacier player at each Ember spawn it
    /// cannot see: no track, no shot; a bolt aimed straight at the hidden
    /// player dies in the wall and does no damage.
    #[test]
    fn tower_turrets_do_not_engage_or_hit_players_behind_walls() {
        let map=MapId::TowerComplex;
        let pack=crate::map_pack::on(map).unwrap();
        let defs=equipment::definitions(map);
        let mut hidden=0;
        for (i,d) in defs.iter().enumerate().filter(|(_,d)|d.kind==Kind::Turret && d.team==0) {
            for spot in spawns(map) {
                let target=spot+Vec3::Y*0.8;let delta=target-d.pos();
                if delta.length()>80. {continue;}
                let muzzle=d.pos()+delta.normalize()*(d.radius+0.6);
                let Some((t,_))=pack.sweep(muzzle,target,0.) else {continue};
                assert!(t<0.99);
                hidden+=1;
                let mut w=world(map);w.players[0].pos=spot;
                w.step_equipment(STEP);
                assert_eq!(w.equipment[i].contacts,0,"{} tracked a player behind a wall at {spot}",d.id);
                assert!(!fired_by(&w,i),"{} fired at a player behind a wall at {spot}",d.id);
                w.discs.push(Disc {pos:muzzle,vel:delta.normalize()*crate::defs::weapons().chaingun.speed,team:Team::Ember,owner:MAX_PLAYERS,life:1.,kind:1,spin:i as f32, target: 0, power: 1.0 });
                for _ in 0..30 {w.step_discs(STEP);}
                assert!(w.discs.is_empty());
                assert_eq!(w.players[0].health,100.,"bolt from {} passed a wall to {spot}",d.id);
            }
        }
        assert!(hidden>=4,"too few hidden spawn cases ({hidden}) to mean anything");
    }

    /// Pod turrets must not engage anyone inside the rooms they guard.
    /// Standable points in the tower (all levels), the rear tunnels and both
    /// rear rooms are sampled on a 1 m grid; no turret may engage one more
    /// than DOOR_DEPTH past an open front opening, tunnel mouth or Level 3
    /// window.
    #[test]
    fn pod_turrets_cannot_see_into_tower_rooms() {
        let map=MapId::TowerComplex;
        let pack=crate::map_pack::on(map).unwrap();
        let info=crate::terrain::info(map);
        let defs=equipment::definitions(map);
        // Local base frame: red faces +z (yaw 180), blue faces -z (yaw 0).
        let to_world=|team:u8,lx:f32,y:f32,lz:f32| if team==0 {
            Vec3::new(info.ember.x-lx,info.ember.y+y,info.ember.z-lz)
        } else {Vec3::new(info.glacier.x+lx,info.glacier.y+y,info.glacier.z+lz)};
        let flat=|team:u8,lx:f32,lz:f32| {let p=to_world(team,lx,0.,lz);Vec2::new(p.x,p.z)};
        // Inner faces of the open openings (local x0, z0, x1, z1): Level 1 front
        // door and 6 m bridge doors, the 8 m rear tunnel mouths, Level 3
        // window bands (the side bands also cover the Level 2 side doors).
        let inner=11.6;
        let local:[(f32,f32,f32,f32);9]=[(-12.,-inner,-6.,-inner),(-3.,-inner,3.,-inner),(6.,-inner,12.,-inner),
            (-12.,inner,-4.,inner),(4.,inner,12.,inner),
            (-9.,-inner,9.,-inner),(-9.,inner,9.,inner),(-inner,-9.,-inner,9.),(inner,-9.,inner,9.)];
        let openings:Vec<(Vec2,Vec2)>=[0u8,1].iter().flat_map(|&t|local.iter().map(move |&(x0,z0,x1,z1)|(t,x0,z0,x1,z1)))
            .map(|(t,x0,z0,x1,z1)|(flat(t,x0,z0),flat(t,x1,z1))).collect();
        // (x0,x1,z0,z1,floor levels to probe from)
        let rooms:[(f32,f32,f32,f32,&[f32]);7]=[
            (-11.5,11.5,-11.5,11.5,&[0.,8.,17.]),   // tower: atrium floor, balconies
            (-11.5,-4.5,12.,26.,&[0.]),(4.5,11.5,12.,26.,&[0.]),   // tunnels
            (-12.5,-3.5,26.5,35.5,&[0.]),(3.5,12.5,26.5,35.5,&[0.]),   // armory, ship room
            (-8.,8.,-8.,8.,&[-8.]),   // keel-level generator room
            (8.6,13.3,-2.,2.,&[-8.])];   // keel hatch passage
        let (mut sampled,mut seen)=(0,Vec::new());
        for (i,d) in defs.iter().enumerate().filter(|(_,d)|d.kind==Kind::Turret) {
            for (r,&(x0,x1,z0,z1,levels)) in rooms.iter().enumerate() {
                let mut lx=x0; while lx<=x1 { let mut lz=z0; while lz<=z1 {
                    for &level in levels {
                        let probe=to_world(d.team,lx,level+5.5,lz);
                        let Some((fy,_))=pack.floor(probe) else {continue};
                        if fy<probe.y-6.9 {continue;}
                        let pos=Vec3::new(probe.x,fy+1.2,probe.z);
                        if pack.body_sweep(pos,pos+Vec3::Y*0.01).is_some() {continue;}
                        sampled+=1;
                        if !in_doorway(pos,&openings) && engages(map,d,pos) {seen.push((r,i,lx,fy-info.ember.y,lz));}
                    }
                lz+=1.;} lx+=1.;}
            }
        }
        assert!(sampled>2000,"too few interior samples ({sampled})");
        let by_room:Vec<usize>=(0..7).map(|r|seen.iter().filter(|s|s.0==r).count()).collect();
        assert!(seen.is_empty(),"{} interior points engaged by pod turrets (tower, left tunnel, right tunnel, armory, ship, keel room, hatch passage: {by_room:?}), e.g. {:?}",seen.len(),&seen[..seen.len().min(12)]);
    }

    /// Cairnhold's sentry, roof and battery turrets, through the server's
    /// rule (field of fire, sensor-boosted range, line of sight), must not
    /// engage a player standing anywhere inside the bunker, the trench, the
    /// guard hut, the flag tower's interior floors, the vault or the sally
    /// port, beyond DOOR_DEPTH of the open front and exit-house doors.
    /// Tolerance: the columns under the tower's open roof hatches.
    #[test]
    fn cairnhold_turrets_cannot_see_into_base_rooms() {
        let map=MapId::Cairnhold;
        let pack=crate::map_pack::on(map).unwrap();
        let info=crate::terrain::info(map);
        let defs=equipment::definitions(map);
        let to_world=|team:u8,lx:f32,y:f32,lz:f32| if team==0 {
            Vec3::new(info.ember.x-lx,info.ember.y+y,info.ember.z-lz)
        } else {Vec3::new(info.glacier.x+lx,info.glacier.y+y,info.glacier.z+lz)};
        let trench=|_x:f32,z:f32| 22.*((z-20.)/56.).clamp(0.,1.);
        let level=|y:f32| move |_x:f32,_z:f32| y;
        // Sally port floors: vault door -7.5 -> 3 over x 17..45; south leg 3 -> 8 over z -20..-36.
        let east=|x:f32,_z:f32| -7.5+10.5*((x-17.)/28.).clamp(0.,1.);
        let south=|_x:f32,z:f32| 3.+5.*((-20.-z)/16.).clamp(0.,1.);
        // (x0,x1,z0,z1,floor(x,z),skip open columns: tower hatches and tunnel skylights)
        let rooms:[(f32,f32,f32,f32,&dyn Fn(f32,f32)->f32,bool);14]=[
            (-15.,15.,-18.6,19.,&level(0.),false),      // bunker: hall, inventory and back rooms
            (-14.6,-11.6,-12.4,3.6,&level(7.),false),     // hall gallery, west side
            (11.6,14.6,-12.4,3.6,&level(7.),false),       // hall gallery, east side
            (-10.8,10.8,0.4,3.6,&level(7.),false),        // hall gallery, back
            (1.6,6.4,20.6,75.4,&trench,false),            // covered trench
            (-6.6,14.6,77.4,98.6,&level(22.),true),       // guard hut
            (3.4,14.6,77.4,94.6,&level(29.),true),        // tower floor over the hut
            (3.4,14.6,92.4,94.9,&level(35.),false),       // tower landing
            (3.8,10.6,-9.2,-0.8,&level(-7.5),false),      // vault (generator room)
            (3.8,9.8,4.,11.4,&level(-9.7),false),         // generator well
            (11.8,14.6,4.6,11.4,&level(-7.5),false),      // walkway to the tunnel door
            (16.6,86.6,5.4,10.6,&east,true),              // sally port, east leg
            (81.4,86.6,-35.4,4.,&south,false),            // sally port, south leg
            (81.4,86.6,-42.6,-36.2,&level(8.),false)];    // exit house
        let open_column=|lx:f32,lz:f32| ((9.5..=15.2).contains(&lx) && (80.0..=94.0).contains(&lz))   // tower hatches
            || ((47.5..=52.5).contains(&lx) || (63.5..=68.5).contains(&lx)) && (4.0..=12.0).contains(&lz);  // skylights
        let flat=|team:u8,lx:f32,lz:f32| {let p=to_world(team,lx,0.,lz);Vec2::new(p.x,p.z)};
        // Open doorways: the 6 m front door, the gallery doorways in both hall
        // side walls, and the exit house door.
        let openings:Vec<(Vec2,Vec2)>=[0u8,1].iter().flat_map(|&t|[(flat(t,-3.,-19.2),flat(t,3.,-19.2)),
            (flat(t,-15.2,-7.),flat(t,-15.2,-1.)),(flat(t,15.2,-7.),flat(t,15.2,-1.)),
            (flat(t,87.2,-43.),flat(t,87.2,-37.4))]).collect();
        let (mut sampled,mut seen)=(0,Vec::new());
        for d in defs.iter().filter(|d|matches!(d.kind,Kind::Turret)) {
            {   let team=d.team;   // attackers inside the turret's own base
                for (r,&(x0,x1,z0,z1,floor,hatch)) in rooms.iter().enumerate() {
                    let mut lx=x0; while lx<=x1 { let mut lz=z0; while lz<=z1 {
                        if !(hatch && open_column(lx,lz)) {
                            let y=floor(lx,lz);
                            let probe=to_world(team,lx,y+1.5,lz);
                            if let Some((fy,_))=pack.floor(probe) {
                                let pos=Vec3::new(probe.x,fy+1.2,probe.z);
                                if (fy-(info.ember.y+y)).abs()<1.5 && pack.body_sweep(pos,pos+Vec3::Y*0.01).is_none() {
                                    sampled+=1;
                                    if !in_doorway(pos,&openings) && engages(map,d,pos) {
                                        seen.push((d.id.clone(),r,lx,y,lz,pos));
                                    }
                                }
                            }
                        }
                    lz+=1.;} lx+=1.;}
                }
            }
        }
        assert!(sampled>3000,"too few interior samples ({sampled})");
        let by_room:Vec<usize>=(0..14).map(|r|seen.iter().filter(|s|s.1==r).count()).collect();
        assert!(seen.is_empty(),"{} interior points visible to turrets (bunker, 3 galleries, trench, hut, tower floor, landing, vault, well, walkway, east, south, exit: {by_room:?}), e.g. {:?}",
            seen.len(),&seen[..seen.len().min(8)]);
    }

    /// Same rule for Old Holler's sunken halls and the
    /// generator basements beneath them. The roof turrets sit beside the ramp
    /// openings and the atrium; nothing may show them a hall floor, basement
    /// or service-passage point (nor may the lookout towers).
    #[test]
    fn old_holler_turrets_cannot_see_into_the_halls() {
        let map=MapId::OldHoller;
        let pack=crate::map_pack::on(map).unwrap();
        let defs=equipment::definitions(map);
        let to_world=|team:u8,lx:f32,y:f32,lz:f32| if team==0 {
            Vec3::new(1160.-lx,112.+y,480.-lz)} else {Vec3::new(800.+lx,112.+y,1400.+lz)};
        // (x0, x1, z0, z1, floor above the base origin): hall, basement and
        // passage. The flag tower's deck stands outdoors, so turrets may cover it.
        let rooms:&[(f32,f32,f32,f32,f32)]=&[(-29.,29.,-25.,25.,-10.),(-10.5,10.5,-5.5,23.5,-19.2),(-4.5,-1.5,25.3,31.3,-19.2)];
        let openings:Vec<(Vec2,Vec2)>=Vec::new();
        let (mut sampled,mut seen)=(0,Vec::new());
        for d in defs.iter().filter(|d|matches!(d.kind,Kind::Turret)) {
            for team in [0u8,1] { for &(x0,x1,z0,z1,level) in rooms {
                let mut lx=x0; while lx<=x1 { let mut lz=z0; while lz<=z1 {
                    let probe=to_world(team,lx,level+1.5,lz);
                    if let Some((fy,_))=pack.floor(probe) {
                        let pos=Vec3::new(probe.x,fy+1.2,probe.z);
                        if (fy-(112.+level)).abs()<0.05 && pack.body_sweep(pos,pos+Vec3::Y*0.01).is_none() {
                            sampled+=1;
                            if !in_doorway(pos,&openings) && engages(map,d,pos) {
                                seen.push((d.id.clone(),team,lx,lz,pos));
                            }
                        }
                    }
                lz+=1.5;} lx+=1.5;}
            }}
        }
        assert!(sampled>3500,"too few hall and basement samples ({sampled})");
        assert!(seen.is_empty(),"{} hall or basement points visible to turrets, e.g. {:?}",seen.len(),&seen[..seen.len().min(8)]);
    }

    /// Same rule for Frostline and Dustreach, over the rooms their Python
    /// suites check (local base coordinates; team 0 is yawed 180 degrees).
    /// Doors and windows are open; DOOR_DEPTH past their inner faces is
    /// allowed. Baffled vestibules (the storehouse, the outpost, the
    /// basement's tunnel door) lie outside these boxes.
    #[test]
    fn frostline_and_dustreach_turrets_cannot_see_into_base_rooms() {
        // (x0, x1, z0, z1, floor above the base origin)
        let frost:&[(f32,f32,f32,f32,f32)]=&[
            (-9.8,9.8,-12.8,5.4,0.),       // station hall, between the two ramps
            (-12.8,12.8,7.2,12.8,0.),      // rear hall
            (-12.8,12.8,-12.8,12.8,7.5),   // command deck (flag level)
            (-6.8,4.2,-6.8,6.8,-7.5),      // basement generator room, west of its baffle
            (4.8,6.8,1.,6.8,-7.5),         // basement, north of the baffle
            (8.6,15.4,-6.6,-1.4,-7.5),     // tunnel to the service shed
            (98.1,109.9,-77.9,-69.,20.)];  // relay outpost behind its baffle
        let dust:&[(f32,f32,f32,f32,f32)]=&[
            (-12.6,12.6,-14.6,2.6,5.),     // keep hall
            (-12.6,12.6,-15.6,5.6,-4.),    // cistern (generator room) under the keep, 8 m tall
            (-14.6,-9.4,7.6,21.6,-4.),     // service tunnel, north leg
            (-23.4,-16.6,16.4,21.6,-4.),   // service tunnel, west leg
            (-30.6,-25.4,16.4,18.4,-4.),   // watch tower base room, south of its exit ramp
            (-26.8,-25.4,18.4,26.,-4.),    // tower room beside the exit ramp
            (32.2,46.6,3.4,17.8,0.),       // storehouse hall floor, behind its baffles
            (32.2,33.2,3.4,20.6,5.),       // storehouse mezzanine, behind the gate baffle
            (34.2,37.,15.4,20.6,5.)];      // storehouse landing at the ramp's head
        for (map,rooms,min) in [(MapId::Frostline,frost,1500),(MapId::Dustreach,dust,1500)] {
            let pack=crate::map_pack::on(map).unwrap();
            let info=crate::terrain::info(map);
            let to_world=|team:u8,lx:f32,y:f32,lz:f32| if team==0 {
                Vec3::new(info.ember.x-lx,info.ember.y+y,info.ember.z-lz)
            } else {Vec3::new(info.glacier.x+lx,info.glacier.y+y,info.glacier.z+lz)};
            // Frostline has no floor over its two ramp openings, the
            // two-level void in front of the flag, or its basement stair's
            // opening in the hall.
            let skip=|lx:f32,y:f32,lz:f32| map==MapId::Frostline && (
                (y==7.5 && ((-13.4..=-10.4).contains(&lx) || (10.4..=13.4).contains(&lx)) && (-10.0..=-2.6).contains(&lz))
                || (y==7.5 && (-5.3..=5.3).contains(&lx) && (-4.3..=6.3).contains(&lz))
                || (y==0. && (-7.7..=-4.1).contains(&lx) && (-7.4..=0.3).contains(&lz)));
            let mut targets=Vec::new();
            for team in [0u8,1] {
                for &(x0,x1,z0,z1,y) in rooms {
                    let mut lx=x0; while lx<=x1+0.01 { let mut lz=z0; while lz<=z1+0.01 {
                        if !skip(lx,y,lz) {
                            let probe=to_world(team,lx,y+1.5,lz);
                            if let Some((fy,_))=pack.floor(probe) {
                                let pos=Vec3::new(probe.x,fy+1.2,probe.z);
                                if (fy-(info.ember.y+y)).abs()<1.5 && pack.body_sweep(pos,pos+Vec3::Y*0.01).is_none() {
                                    targets.push(pos);
                                }
                            }
                        }
                    lz+=1.;} lx+=1.;}
                }
            }
            assert!(targets.len()>min,"{map:?}: too few interior samples ({})",targets.len());
            // Inner faces of the open doors and windows, local (x0, z0, x1, z1).
            let local:&[(f32,f32,f32,f32)]=if map==MapId::Frostline {&[
                (-3.,-13.4,3.,-13.4),(-12.,13.4,-6.,13.4),(13.4,6.8,13.4,12.8),
                (-11.,-13.4,-4.,-13.4),(4.,-13.4,11.,-13.4),(-11.,13.4,-4.,13.4),(4.,13.4,11.,13.4),
                (-13.4,-10.,-13.4,-3.),(-13.4,3.,-13.4,10.),(13.4,-10.,13.4,-3.),(13.4,3.,13.4,10.)]
            } else {&[(-3.,-15.2,3.,-15.2),(-3.,3.2,3.,3.2),(-30.5,30.5,-26.5,30.5)]};
            let flat=|team:u8,lx:f32,lz:f32| {let p=to_world(team,lx,0.,lz);Vec2::new(p.x,p.z)};
            let openings:Vec<(Vec2,Vec2)>=[0u8,1].iter().flat_map(|&t|local.iter().map(move |&(x0,z0,x1,z1)|(t,x0,z0,x1,z1)))
                .map(|(t,x0,z0,x1,z1)|(flat(t,x0,z0),flat(t,x1,z1))).collect();
            let mut seen=Vec::new();
            for d in equipment::definitions(map).iter().filter(|d|d.kind==Kind::Turret) {
                for &pos in &targets {
                    if !in_doorway(pos,&openings) && engages(map,d,pos) {seen.push((d.id.clone(),pos));}
                }
            }
            assert!(seen.is_empty(),"{map:?}: {} interior points visible to turrets, e.g. {:?}",seen.len(),&seen[..seen.len().min(8)]);
        }
    }

    /// Dustreach's new routes are walkable end to end with the full player
    /// body (engine body sweep, 0.6 m above each floor or ramp), for both
    /// teams: hall -> stair -> cistern -> tunnel -> tower room -> exit ramp ->
    /// back door, and courtyard -> gate -> bridge -> storehouse -> its ramp and
    /// doors. Local base coordinates; team 0 is yawed 180 degrees.
    #[test]
    fn dustreach_underground_and_storehouse_routes_are_walkable() {
        let map=MapId::Dustreach;
        let pack=crate::map_pack::on(map).unwrap();
        let info=crate::terrain::info(map);
        let stair=|z:f32| -4.0+9.0*((z+14.2)/16.2).clamp(0.,1.);           // hall 5 at z 2, cistern -4 at z -14.2
        let tramp=|z:f32| -4.0+3.9*((z-18.5)/8.0).clamp(0.,1.);           // tower room -4 to landing -0.1
        let sramp=|z:f32| 5.0*((z-4.8)/10.0).clamp(0.,1.);                // storehouse hall 0 to mezzanine 5
        let routes:Vec<(&str,Vec<(f32,f32,f32)>)>=vec![
            ("hall to tower exit",vec![(4.,5.,2.6),(11.25,5.,2.6),(11.25,stair(1.9),1.9),(11.25,stair(-14.1),-14.1),
                (11.25,-4.,-15.4),(8.2,-4.,-15.4),(-3.,-4.,-15.4),(-10.5,-4.,-15.4),(-11.,-4.,-10.),(-11.,-4.,5.),(-11.1,-4.,6.6),
                (-12.,-4.,8.),(-12.,-4.,19.),(-20.,-4.,19.),(-26.,-4.,17.5),(-29.2,-4.,17.5),(-29.2,tramp(18.6),18.6),
                (-29.2,tramp(26.4),26.4),(-28.5,-0.1,28.5),(-28.5,-0.1,32.3),(-23.,-0.1,32.3),(-20.,-0.1,32.3),
                (-18.,-0.1,36.)]),
            ("bridge onto the mezzanine and down",vec![(15.,5.,15.5),(20.6,5.,15.5),(24.6,5.,15.5),(28.4,5.,15.5),
                (29.9,5.,15.5),(29.9,5.,20.6),(33.,5.,20.6),(35.7,5.,18.),(35.7,sramp(14.7),14.7),(35.7,sramp(4.9),4.9),
                (35.7,0.,3.6),(40.,0.,3.6),(40.,0.,14.5),(46.4,0.,14.5),(46.4,0.,20.3),(41.5,0.,20.3),(41.5,0.,22.5),
                (41.5,-0.1,25.)]),
            ("storehouse ground west door",vec![(25.,-0.1,7.5),(28.4,0.,7.5),(29.9,0.,7.5),(29.9,0.,13.),(33.,0.,13.),
                (33.,0.,16.),(38.,0.,16.),(40.,0.,5.)]),
        ];
        for team in [0u8,1] {
            let world=|(x,y,z):(f32,f32,f32)| if team==0 {
                Vec3::new(info.ember.x-x,info.ember.y+y+0.6,info.ember.z-z)
            } else {Vec3::new(info.glacier.x+x,info.glacier.y+y+0.6,info.glacier.z+z)};
            for (name,points) in &routes {
                for pair in points.windows(2) {
                    let (a,b)=(world(pair[0]),world(pair[1]));
                    assert!(pack.body_sweep(a,b).is_none(),"team {team} {name}: blocked between {:?} and {:?}",pair[0],pair[1]);
                }
            }
        }
    }

    /// Cairnhold's generator route is walkable end to end with the full body
    /// (engine body sweep, 0.6 m above each floor or ramp), for both teams:
    /// hall -> stair -> vault -> sally port (east, then south) -> exit house ->
    /// round its vestibule -> out onto the battery bench.
    #[test]
    fn cairnhold_vault_and_sally_port_route_is_walkable() {
        let map=MapId::Cairnhold;
        let pack=crate::map_pack::on(map).unwrap();
        let info=crate::terrain::info(map);
        // Vault floor -7.5 (6.5 m under the hall slab); stair head z -9.5, foot z 4.5.
        let stair=|z:f32| -7.5*((z+9.5)/14.).clamp(0.,1.);
        let east=|x:f32| -7.5+10.5*((x-17.)/28.).clamp(0.,1.);
        let south=|z:f32| 3.+5.*((-20.-z)/16.).clamp(0.,1.);
        let route=[(13.2,0.,-13.),(13.2,0.,-9.5),(13.2,stair(-5.),-5.),(13.2,stair(1.4),1.4),(13.2,stair(3.6),3.6),(13.2,-7.5,5.6),
            (13.2,-7.5,8.),(15.6,-7.5,8.),(17.,-7.5,8.),(29.,east(29.),8.),(45.,3.,8.),(60.,3.,8.),(84.,3.,8.),(84.,3.,4.),(84.,3.,-20.),
            (84.,south(-28.),-28.),(84.,8.,-36.),(82.8,8.,-38.),(82.8,8.,-42.5),(86.3,8.,-42.5),(86.3,8.,-41.3),
            (89.,8.,-41.3),(94.,8.,-46.)];
        for team in [0u8,1] {
            let world=|(x,y,z):(f32,f32,f32)| if team==0 {
                Vec3::new(info.ember.x-x,info.ember.y+y+0.6,info.ember.z-z)
            } else {Vec3::new(info.glacier.x+x,info.glacier.y+y+0.6,info.glacier.z+z)};
            for pair in route.windows(2) {
                let (a,b)=(world(pair[0]),world(pair[1]));
                assert!(pack.body_sweep(a,b).is_none(),"team {team}: blocked between {:?} and {:?}",pair[0],pair[1]);
            }
            // The stair head is the only way down from the hall: the rest of
            // the hall floor is solid over the vault.
            for (x,z) in [(5.,-6.),(9.,0.),(5.,6.)] {
                let p=world((x,0.,z));
                let (fy,_)=pack.floor(p).expect("hall or inventory floor");
                assert!((fy-(p.y-0.6)).abs()<0.01,"team {team}: floor at ({x},{z}) is {fy}");
            }
        }
    }

    /// The front openings are open straight in: a body walks from outside
    /// each opening across Level 1 with nothing in the way, and from the left
    /// bridge door on to the L1->L2 ramp foot.
    #[test]
    fn tower_entries_walk_straight_in() {
        let map=MapId::TowerComplex;
        let pack=crate::map_pack::on(map).unwrap();
        let info=crate::terrain::info(map);
        for (base,s) in [(info.ember,-1.),(info.glacier,1.)] {
            let at=|lx:f32,lz:f32| {
                let p=Vec3::new(base.x+s*lx,base.y+3.,base.z+s*lz);
                let (fy,_)=pack.floor(p).expect("floor under route point");
                Vec3::new(p.x,fy+1.2,p.z)
            };
            let routes:[&[(f32,f32)];3]=[
                &[(-9.,-14.),(-9.,-6.),(-9.,-2.)],
                &[(0.,-11.2),(0.,-6.)],
                &[(9.,-14.),(9.,-6.)]];
            for route in routes {
                for w in route.windows(2) {
                    let (a,b)=(at(w[0].0,w[0].1),at(w[1].0,w[1].1));
                    // Rise with the ramp in short steps, as movement would.
                    let steps=((b-a).length()/0.25).ceil() as usize;
                    for k in 0..steps {
                        let p0=a.lerp(b,k as f32/steps as f32);let p1=a.lerp(b,(k+1) as f32/steps as f32);
                        let lift=Vec3::Y*0.3;
                        assert!(pack.body_sweep(p0+lift,p1+lift).is_none(),"route blocked between {p0} and {p1} ({:?} -> {:?})",w[0],w[1]);
                    }
                }
            }
        }
    }

    #[test]
    fn tower_turrets_still_engage_a_player_on_their_bridge() {
        let map=MapId::TowerComplex;
        let pack=crate::map_pack::on(map).unwrap();
        let defs=equipment::definitions(map);
        let flag=Vec3::from_array(pack.manifest.flags[0]);
        for (i,d) in defs.iter().enumerate().filter(|(_,d)|d.kind==Kind::Turret && d.team==0) {
            // Halfway along the bridge toward the tower, standing on its deck.
            let probe=d.pos()+Vec3::new(0.,3.,(flag.z-d.pos().z).signum()*9.);
            let (floor,_)=pack.floor(probe).expect("bridge deck below the probe");
            let mut w=world(map);w.players[0].pos=Vec3::new(probe.x,floor+1.2,probe.z);
            w.step_equipment(STEP);
            assert_eq!(w.equipment[i].contacts,1,"{} ignored a player on its bridge",d.id);
            assert!(fired_by(&w,i));
        }
    }

    /// The viewmodel muzzle reaches ~1.5 m ahead of the eye, past a thin
    /// wall. A shot must start on the shooter's side and stop there.
    #[test]
    fn point_blank_shots_never_spawn_past_a_wall() {
        for map in [MapId::TowerComplex,MapId::Cairnhold] {
            let pack=crate::map_pack::on(map).unwrap();
            let mut cases=0;
            for spot in spawns(map) {
                for step in 0..72 {
                    let yaw=step as f32*std::f32::consts::TAU/72.;
                    let fwd=look_dir(yaw,0.);
                    // Walk the real body into the wall, as movement would.
                    let Some((t,_))=pack.body_sweep(spot,spot+fwd*6.) else {continue};
                    let stand=spot+fwd*(t*6.-0.01);
                    let eye=stand+Vec3::Y*EYE;
                    let raw=muzzle_origin(eye,fwd,0,camera_fov(0.));
                    if obstacle_hit(map,&[],eye,raw,SHOT_RADIUS).is_none() {continue;}
                    let mut w=world(map);w.players[0].pos=stand;w.players[0].yaw=yaw;w.players[0].pitch=0.;w.players[0].weapon=0;
                    w.shoot(0);
                    let disc=w.discs.last().unwrap().pos;
                    assert!(obstacle_hit(map,&[],eye,disc,0.).is_none(),"{map:?}: shot spawned past a wall at {stand} yaw {yaw}: eye {eye} raw {raw} disc {disc} pos {} rawhit {:?} dischit {:?} pillars {}",w.players[0].pos,obstacle_hit(map,&w.pillars,eye,raw,SHOT_RADIUS),obstacle_hit(map,&[],eye,disc,0.),w.pillars.len());
                    cases+=1;
                }
                if cases>=6 {break;}
            }
            assert!(cases>0,"{map:?}: no wall-hugging case found; the test did not exercise the muzzle clamp");
        }
    }
}

#[cfg(test)]
mod targeting_equivalence {
    use super::*;
    use crate::equipment::{self,Kind,Definition};

    /// Verbatim copy of the inline targeting that `step_equipment` used before
    /// `equipment::acquire_target`: (index, aim target, aim, fire line clear).
    fn legacy(map:MapId, d:&Definition, sensed:bool, players:&[(u8,Vec3,Vec3)])->Option<(usize,Vec3,Vec3,bool)> {
        let range=if d.kind==Kind::Sensor {260.} else if sensed {150.} else {80.};
        let target=players.iter().enumerate().filter(|(_,p)|p.0 as usize!=d.team as usize)
            .filter_map(|(i,p)| {
                let target=p.1+Vec3::Y*0.8;let delta=target-d.pos();
                if delta.length()>range {return None;}
                let start=d.pos()+delta.normalize_or_zero()*(d.radius+0.6);
                obstacle_hit(map,&[],start,target,0.).is_none().then_some((delta.length(),i,target,p.2))
            }).min_by(|a,b|a.0.total_cmp(&b.0));
        let (_,i,target,velocity)=target?;
        let plasma=d.weapon==equipment::TurretWeapon::Plasma;
        let speed=if plasma {80.} else {crate::defs::weapons().chaingun.speed};
        let life=if plasma {3.} else {1.};
        let aim_target=if d.kind==Kind::Turret && plasma {
            equipment::intercept_time(target-d.pos(),velocity,speed,d.radius+0.6,life)
                .map_or(target,|t|target+velocity*t)
        } else {target};
        let aim=(aim_target-d.pos()).normalize_or_zero();
        let muzzle=d.pos()+aim*(d.radius+0.6);
        Some((i,aim_target,aim,obstacle_hit(map,&[],muzzle,aim_target,0.).is_none()))
    }

    fn shared(map:MapId, d:&Definition, sensed:bool, players:&[(u8,Vec3,Vec3)])->Option<(usize,Vec3,Vec3,bool)> {
        let profile=equipment::profile(d.kind,d.weapon)?;
        let clear=|a:Vec3,b:Vec3|obstacle_hit(map,&[],a,b,0.).is_none();
        let candidates=players.iter().enumerate()
            .map(|(index,p)|equipment::Candidate {index,team:p.0,pos:p.1,vel:p.2,jetting:false});
        let hit=equipment::acquire_target(d.pos(),d.radius,d.team,&profile,sensed,candidates,clear)?;
        Some((hit.index,hit.aim_point,hit.aim,clear(hit.muzzle,hit.aim_point)))
    }

    #[test]
    fn shared_targeting_matches_the_previous_inline_rule() {
        let mut seed=0x9e37_79b9u32;
        let mut rnd=move || {seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;(seed as f32/u32::MAX as f32)*2.-1.};
        for map in [MapId::TowerComplex,MapId::Cairnhold,MapId::OldHoller] {
            let defs=equipment::definitions(map);
            let (mut scenarios,mut acquired,mut leading,mut sensed_only)=(0,0,0,0);
            // Rocket turrets (`TurretWeapon::Rocket`) came after the inline rule.
            for d in defs.iter().filter(|d|matches!(d.kind,Kind::Sensor|Kind::Turret) && d.weapon!=equipment::TurretWeapon::Rocket) {
                for _ in 0..400 {
                    let enemy=1-d.team.min(1);
                    let mut players=Vec::new();
                    for k in 0..3 {
                        let r=[40.,120.,280.][k%3]*rnd().abs();
                        let pos=d.pos()+Vec3::new(rnd()*r,rnd()*30.,rnd()*r);
                        let vel=Vec3::new(rnd()*40.,rnd()*10.,rnd()*40.);
                        players.push((if k==1 {d.team} else {enemy},pos,vel));
                    }
                    for sensed in [false,true] {
                        let old=legacy(map,d,sensed,&players);
                        let new=shared(map,d,sensed,&players);
                        assert_eq!(old,new,"{map:?} {} sensed={sensed} players={players:?}",d.id);
                        scenarios+=1;
                        if let Some(o)=old {
                            acquired+=1;
                            if o.1!=players[o.0].1+Vec3::Y*0.8 {leading+=1;}
                            if sensed && legacy(map,d,false,&players).is_none() {sensed_only+=1;}
                        }
                    }
                }
            }
            assert!(acquired>0 && sensed_only>0,"{map:?}: {scenarios} scenarios, {acquired} acquired, {sensed_only} sensed-only");
            if matches!(map,MapId::Cairnhold|MapId::OldHoller) {assert!(leading>0,"{map:?}: no plasma intercept case exercised");}
        }
    }
}

/// Collision-budget evidence, not a correctness check. Times whole sim ticks
/// (a full offline match with bots firing) and raw map-pack queries around
/// each base, so a per-base collision budget can be set from numbers. Run with
/// `cargo test --release -p peakrunner-core --lib collision_budget_timing -- --ignored --nocapture`.
#[cfg(test)]
mod collision_budget_timing {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore = "timing probe; run in release"]
    fn collision_budget_timing() {
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Frostline, MapId::Dustreach] {
            let pack = crate::map_pack::on(map).unwrap();
            let info = crate::terrain::info(map);
            let mut world = World::new();
            world.set_map(map);
            world.start_match(true);
            let dt = 1.0 / 60.0;
            for _ in 0..120 { world.tick(dt); }
            let (mut total, mut worst) = (0.0f64, 0.0f64);
            let ticks = 3600;
            for i in 0..ticks {
                world.input.move_z = 1.0; world.input.fire = i % 30 < 15; world.input.jet = i % 240 < 90;
                let t = Instant::now();
                world.tick(dt);
                let e = t.elapsed().as_secs_f64();
                total += e; worst = worst.max(e);
            }
            // Raw queries in a 120 m box round each base: short movement
            // sweeps, body sweeps, floor probes and 80 m sight rays.
            let mut seed = 0x2545F491u32;
            let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5; seed as f32 / u32::MAX as f32 };
            let n = 200_000;
            let t = Instant::now();
            let mut hits = 0;
            for i in 0..n {
                let home = if i % 2 == 0 { info.ember } else { info.glacier };
                let p = home + Vec3::new((rnd() - 0.5) * 120.0, rnd() * 25.0 - 5.0, (rnd() - 0.5) * 120.0);
                let d = Vec3::new(rnd() - 0.5, (rnd() - 0.5) * 0.3, rnd() - 0.5).normalize_or_zero();
                match i % 4 {
                    0 => hits += pack.sweep(p, p + d * 1.5, 0.52).is_some() as usize,
                    1 => hits += pack.body_sweep(p, p + d * 1.5).is_some() as usize,
                    2 => hits += pack.floor(p).is_some() as usize,
                    _ => hits += pack.sweep(p, p + d * 80.0, 0.0).is_some() as usize,
                }
            }
            let per_query = t.elapsed().as_secs_f64() / n as f64;
            println!("{map:?}: {} collision tris, tick avg {:.1} us worst {:.1} us over {ticks} ticks ({} players), query avg {:.2} us ({hits} hits)",
                pack.triangle_count(), total / ticks as f64 * 1e6, worst * 1e6, world.players.len(), per_query * 1e6);
        }
    }
}


#[cfg(test)]
mod shield_and_kit_tests {
    use super::*;
    use crate::equipment::{self,Kind};
    const MAPS:[MapId;5]=[MapId::OldHoller,MapId::TowerComplex,MapId::Cairnhold,MapId::Frostline,MapId::Dustreach];

    fn world(map:MapId)->World {
        let mut w=World::new();w.set_map(map);w.start_rift(true);w.players.truncate(1);
        w.players[0].team=Team::Glacier;w.players[0].pos=Vec3::new(1.,-500.,1.);w
    }
    fn first(map:MapId,kind:Kind)->usize {
        equipment::definitions(map).iter().position(|d|d.team==0 && d.kind==kind).unwrap()
    }

    #[test]
    fn floor_grenades_and_discs_reach_shielded_equipment_past_their_mounts() {
        for map in MAPS {
            for (i,d) in equipment::definitions(map).iter().enumerate()
                .filter(|(_,d)|d.team==0 && matches!(d.kind,Kind::Turret|Kind::Sensor)) {
                let c=d.pos();let mut tried=0;
                for k in 0..8 {
                    let a=k as f32/8.*std::f32::consts::TAU;
                    let probe=Vec3::new(c.x+a.cos()*3.,c.y+4.,c.z+a.sin()*3.);
                    let Some((fy,_))=crate::map_pack::on(map).and_then(|pk|pk.floor(probe)) else {continue};
                    // Only open floor below the object: a deck over it rightly blocks.
                    if fy>=c.y || c.y-fy>6. {continue;}
                    tried+=1;
                    let mut w=world(map);let before=w.equipment[i].shield;
                    w.explode(Vec3::new(probe.x,fy+0.1,probe.z),MAX_PLAYERS,2,Team::Glacier);
                    assert!(w.equipment[i].shield<before,"{map:?} {}: floor grenade at {a:.2} rad missed",d.id);
                }
                let _=tried;
                let mut w=world(map);let from=c+Vec3::new(0.,3.,25.);
                if obstacle_hit(map,&[],from,c,0.).is_some_and(|t|t<0.9) {continue;}
                let before=w.equipment[i].shield;
                w.discs.push(Disc{pos:from,vel:(c-from).normalize()*crate::defs::weapons().disc.speed,team:Team::Glacier,owner:MAX_PLAYERS,life:5.,kind:0,spin:0.,target:0,power:1.0});
                for _ in 0..60 {w.step_discs(STEP);}
                assert!(w.equipment[i].shield<before,"{map:?} {}: an aimed disc missed",d.id);
            }
        }
    }

    #[test]
    fn walls_beyond_the_mount_still_stop_splash() {
        let mut blocked=0;
        for map in MAPS {
            for (i,d) in equipment::definitions(map).iter().enumerate().filter(|(_,d)|d.team==0 && d.kind==Kind::Turret) {
                let c=d.pos();
                for k in 0..16 {
                    let a=k as f32/16.*std::f32::consts::TAU;
                    for r in [5.,8.,11.] {
                        let probe=Vec3::new(c.x+a.cos()*r,c.y+3.,c.z+a.sin()*r);
                        let Some((fy,_))=crate::map_pack::on(map).and_then(|pk|pk.floor(probe)) else {continue};
                        let blast=Vec3::new(probe.x,fy+0.1,probe.z);
                        let delta=c-blast;let end=c-delta.normalize_or_zero()*d.radius.min(delta.length());
                        let Some(t)=obstacle_hit(map,&[],blast,end,0.) else {continue};
                        let hit=blast.lerp(end,t);
                        if Vec3::new(hit.x-c.x,0.,hit.z-c.z).length()<=d.radius+EQUIPMENT_MOUNT_MARGIN+0.3 {continue;}
                        let mut w=world(map);let (sh,hp)=(w.equipment[i].shield,w.equipment[i].health);
                        w.explode(blast,MAX_PLAYERS,2,Team::Glacier);
                        assert_eq!((w.equipment[i].shield,w.equipment[i].health),(sh,hp),"{map:?} {}: splash crossed a wall",d.id);
                        blocked+=1;
                    }
                }
            }
        }
        assert!(blocked>20,"the wall check must actually sample walls ({blocked})");
    }

    /// Every attacker fires at max rate with the best possible result: every
    /// explosive lands point blank for its full damage, every bullet hits.
    /// Returns the seconds until damage first gets through the shield to the
    /// hull, or None if the shield holds for `secs`.
    fn sustained(map:MapId,kind:Kind,weapon:u8,attackers:usize,secs:f32)->Option<f32> {
        let mut w=world(map);let i=first(map,kind);let d=equipment::definitions(map)[i].clone();
        let blast=crate::combat::structure_blast(weapon).map(|b|b.max_damage);
        let reload=weapon_reload(weapon);
        let mut next:Vec<f32>=(0..attackers).map(|k|k as f32*reload/attackers as f32).collect();
        let mut t=0.;
        while t<secs {
            for n in &mut next {
                while *n<=t {
                    // Isolated from splash on neighbours such as the generator.
                    match blast {Some(dmg)=>{w.equipment[i].damage(&d,dmg,false);}
                        None=>{w.equipment[i].damage(&d,8.,true);}}
                    *n+=reload;
                }
            }
            w.step_equipment(STEP);
            assert!(w.equipment[i].powered,"the attacked object keeps its generator");
            // Broken means damage gets past the shield to the hull.
            if w.equipment[i].health<d.max_health() {return Some(t);}
            t+=STEP;
        }
        None
    }

    #[test]
    fn one_attacker_with_any_weapon_never_breaks_a_powered_shield() {
        for kind in [Kind::Turret,Kind::Sensor] {
            for weapon in [0u8,1,2] {
                assert_eq!(sustained(MapId::OldHoller,kind,weapon,1,60.),None,"{kind:?} broke under solo weapon {weapon}");
            }
        }
    }

    #[test]
    fn focused_explosive_fire_breaks_shields_and_regen_keeps_running() {
        for kind in [Kind::Turret,Kind::Sensor] {
            for weapon in [0u8,2] {
                let t=sustained(MapId::OldHoller,kind,weapon,2,30.);
                assert!(t.is_some_and(|t|t<20.),"{kind:?} held against two attackers with weapon {weapon}: {t:?}");
            }
            assert_eq!(sustained(MapId::OldHoller,kind,1,2,60.),None,"two chainguns alone do not break it");
            assert!(sustained(MapId::OldHoller,kind,1,3,60.).is_some(),"three chainguns do");
        }
    }

    #[test]
    #[ignore]
    fn print_time_to_break_table() {
        for kind in [Kind::Turret,Kind::Sensor] {
            for (weapon,name) in [(0u8,"disc"),(2,"grenade"),(1,"chaingun")] {
                let row:Vec<String>=(1..=3).map(|n|sustained(MapId::OldHoller,kind,weapon,n,120.)
                    .map_or("never".into(),|t|format!("{t:.1}s"))).collect();
                println!("{kind:?} {name}: 1p {} · 2p {} · 3p {}",row[0],row[1],row[2]);
            }
        }
    }

    #[test]
    fn destroying_a_generator_explodes_once_harmlessly_and_replays_nothing() {
        let mut w=world(MapId::OldHoller);let g=first(MapId::OldHoller,Kind::Generator);
        let d=equipment::definitions(w.map)[g].clone();
        w.players[0].team=Team::Ember;w.players[0].pos=d.pos()+Vec3::X*(d.radius+1.);w.players[0].health=100.;
        w.equipment[g].health=1.;let serial=w.blast_serial;
        w.equipment[g].damage(&d,0.5,true);
        assert!(w.explosions.iter().all(|e|e.kind!=4),"no blast before the hull is gone");
        let hits=|w:&World|w.explosions.iter().filter(|e|e.kind==4).count();
        // Destroy it through the normal bullet path.
        w.discs.push(Disc{pos:d.pos()+Vec3::Z*6.,vel:-Vec3::Z*crate::defs::weapons().chaingun.speed,team:Team::Glacier,owner:MAX_PLAYERS,life:1.,kind:1,spin:0.,target:0,power:1.0});
        for _ in 0..4 {w.step_discs(STEP);}
        assert!(w.equipment[g].health<=0. && w.equipment[g].offline);
        assert_eq!((hits(&w),w.blast_serial),(1,serial+1),"one generator blast");
        assert_eq!(w.players[0].health,100.,"the blast hurts no one");
        // A wreck cannot be destroyed again.
        w.explode(d.pos()+Vec3::X*(d.radius+0.1),MAX_PLAYERS,0,Team::Glacier);
        assert_eq!(w.explosions.iter().filter(|e|e.kind==4).count(),1);
        // Snapshot replay leaves the serial and the blast list as they were.
        let mut m=Match::new(MapId::OldHoller);m.world.equipment=w.equipment.clone();
        m.world.explosions=w.explosions.clone();m.world.blast_serial=w.blast_serial;
        let snap=m.snapshot();let mut client=World::new();client.set_map(MapId::OldHoller);
        for _ in 0..2 {client.apply_snapshot(&snap,0);}
        assert_eq!((client.blast_serial,client.explosions.iter().filter(|e|e.kind==4).count()),(w.blast_serial,1));
    }

    #[test]
    fn a_wrecked_generator_needs_half_hull_before_power_returns() {
        let mut w=world(MapId::OldHoller);let defs=equipment::definitions(w.map);
        let g=first(MapId::OldHoller,Kind::Generator);let t=first(MapId::OldHoller,Kind::Turret);
        w.equipment[g].damage(&defs[g],10000.,false);w.step_equipment(STEP);
        assert!(!w.equipment[t].powered && w.equipment[t].shield==0.);
        w.equipment[g].repair(&defs[g],defs[g].max_health()*0.4);w.step_equipment(STEP);
        assert!(!w.equipment[t].powered,"40% is not enough");
        w.equipment[g].repair(&defs[g],defs[g].max_health()*0.1);w.step_equipment(STEP);
        assert!(w.equipment[t].powered && w.equipment[t].shield>0.,"50% brings the circuit and shields back");
    }

    #[test]
    fn the_repair_tool_stays_out_and_rests_on_an_empty_tank() {
        // Held while jetting until the tank runs dry: the tool stays in the
        // hands every tick; the beam stops and starts again only after its
        // rest, while the tank refills (it used to flicker with each tick's
        // recharge).
        let mut w=world(MapId::OldHoller);
        w.players[0].team=Team::Ember;w.players[0].health=20.;w.players[0].energy=crate::defs::movement().energy_max;
        w.state=MatchState::Playing;
        w.input.repair=true;w.input.jet=true;
        let ticks=(10.0/STEP) as usize;
        let (mut starts,mut was,mut rested)=(Vec::new(),false,false);
        for t in 0..ticks {
            w.physics_step();
            let p=&w.players[0];
            assert!(p.repair_out,"the tool stays out at tick {t}");
            let runs=p.repair_beam.is_some();
            if runs && !was {starts.push(t);}
            if !runs && p.repair_rest>0. && !rested {
                rested=true;
                assert!(w.hud_json().contains("\"repairing\":2"),"the HUD says it waits");
            }
            was=runs;
        }
        assert!(rested && starts.len()>=2,"it ran dry and tried again: starts {starts:?}");
        let rest=(crate::defs::repair().rest/STEP) as usize;
        assert!(starts.windows(2).all(|g| g[1]-g[0]>rest),"restarts at least a rest apart: {starts:?}");
        // Let go: the tool goes away.
        w.input.repair=false;w.physics_step();
        assert!(!w.players[0].repair_out&&w.players[0].repair_beam.is_none());
    }

    #[test]
    fn the_repair_tool_heals_slowly_on_energy_and_fixes_what_you_aim_at() {
        let mut w=world(MapId::OldHoller);
        w.players[0].team=Team::Ember;w.players[0].pos=Vec3::new(1000.,300.,1000.);
        w.players[0].health=30.;w.players[0].energy=crate::defs::movement().energy_max;w.input.repair=true;
        for _ in 0..(3.0/STEP) as usize {w.step_repair(STEP);}
        let p=&w.players[0];
        assert!((p.health-(30.+3.*crate::defs::repair().player)).abs()<0.5,"slow self repair: {}",p.health);
        assert!((p.energy-(crate::defs::movement().energy_max-3.*crate::defs::repair().energy)).abs()<0.5,"it costs energy: {}",p.energy);
        assert!(p.repair_beam.is_some());
        assert!(p.repair_self, "working on themselves: a glow, not a stream (`repair2`)");
        // Through the whole tick the tank falls: no recharge while repairing.
        let mut v=world(MapId::OldHoller);
        v.players[0].team=Team::Ember;v.players[0].health=40.;v.players[0].energy=crate::defs::movement().energy_max;v.input.repair=true;
        v.state=MatchState::Playing;
        for _ in 0..(2.0/STEP) as usize {v.physics_step();}
        assert!(v.players[0].energy<crate::defs::movement().energy_max-2.0*crate::defs::repair().energy+1.0,"repairing drains the tank: {}",v.players[0].energy);
        // Out of energy, it stops.
        w.players[0].energy=1.;let before=w.players[0].health;w.step_repair(STEP);
        assert_eq!(w.players[0].health,before);
        // Aimed at a damaged generator of your own, it repairs the hull.
        let defs=equipment::definitions(w.map);let g=first(MapId::OldHoller,Kind::Generator);
        w.players[0].team=if defs[g].team==0 {Team::Ember} else {Team::Glacier};
        assert!(equipment_tests::aim_repair_at(&mut w,g),"somewhere to repair the generator from");
        w.equipment[g].health=100.;w.players[0].energy=crate::defs::movement().energy_max;
        for _ in 0..60 {w.step_repair(STEP);}
        assert!((w.equipment[g].health-(100.+crate::defs::repair().hull)).abs()<1.0,"hull {}",w.equipment[g].health);
        assert!(w.players[0].repair_beam.is_some()&&!w.players[0].repair_self,"a stream to the generator");
        // Inventory stations restock ammunition.
        let s=first(MapId::OldHoller,Kind::Inventory);
        let mut q=w.players[0].clone();q.team=if defs[s].team==0 {Team::Ember} else {Team::Glacier};
        q.ammo.iter_mut().for_each(|n| *n=0);q.pos=defs[s].pos();
        equipment::service(&defs[s],&mut w.equipment[s],&mut q,true,STEP);
        assert_eq!(q.ammo,loadout::max_ammo(q.armor));
    }

    #[test]
    fn repairs_are_a_server_intent_never_predicted() {
        let mut m=Match::new(MapId::OldHoller);let Some(a)=m.join(3,"medic") else {panic!()};
        m.world.players[a].health=40.;m.world.players[a].energy=crate::defs::movement().energy_max;
        let mut commands=vec![None;m.world.players.len()];
        for seq in 1..=60 {commands[a]=Some(Command{seq,repair:true,..Default::default()});m.step(&commands);}
        assert!(m.world.players[a].health>44.,"the server repairs: {}",m.world.players[a].health);
        let mut client=World::new();client.set_map(MapId::OldHoller);
        assert!(client.apply_snapshot(&m.snapshot(),3));
        let me=client.player_id;let before=client.players[me].health;
        client.predict_command(Command{seq:999,repair:true,..Default::default()});
        assert_eq!(client.players[me].health,before);
    }
}

#[cfg(test)]
mod bot_line_of_sight {
    use super::*;

    const MAPS: [MapId; 5] = [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold,
        MapId::Frostline, MapId::Dustreach];

    fn visible_enemy(w: &World, i: usize) -> bool {
        let p = &w.players[i];
        let eye = p.pos + Vec3::Y * EYE;
        w.players.iter().any(|o| o.alive && o.team != p.team && o.pos.distance(p.pos) < 95.0
            && obstacle_hit(w.map, &w.pillars, eye, o.pos + Vec3::Y * crate::equipment::CHEST_HEIGHT, 0.).is_none())
    }

    /// Bot-only match on `map`: (bot shots, shots fired with no enemy in sight, kills).
    fn soak(map: MapId, seconds: f32) -> (u32, u32, u32) {
        let mut w = World::new();
        w.set_map(map);
        w.start_match(true);
        w.players[0].is_bot = true;
        let (mut shots, mut blind) = (0, 0);
        for _ in 0..(seconds / STEP) as u32 {
            let before: Vec<u32> = w.players.iter().map(|p| p.shots).collect();
            let seen: Vec<bool> = (0..w.players.len()).map(|i| visible_enemy(&w, i)).collect();
            w.tick(STEP);
            for i in 0..w.players.len() {
                let fired = w.players[i].shots.saturating_sub(before[i]);
                shots += fired;
                if !seen[i] { blind += fired; }
            }
        }
        (shots, blind, w.players.iter().map(|p| p.frags).sum())
    }

    /// A match with just one Ember bot at `a` and one Glacier player at `b`.
    fn duel(map: MapId, a: Vec3, b: Vec3, b_is_bot: bool) -> World {
        let mut w = World::new();
        w.set_map(map);
        w.start_match(true);
        let face = (-(b - a).x).atan2(-(b - a).z);
        w.players = vec![make_player(Team::Ember, true, a, face, BotRole::Hunter),
            make_player(Team::Glacier, b_is_bot, b, face + std::f32::consts::PI, BotRole::Hunter)];
        w.player_id = 1;
        w
    }

    fn stand(map: MapId, x: f32, z: f32) -> Vec3 {
        Vec3::new(x, crate::terrain::height_on(map, x, z) + 1.2, z)
    }

    fn clear(map: MapId, from: Vec3, to: Vec3) -> bool {
        obstacle_hit(map, &crate::terrain::pillars_on(map), from + Vec3::Y * EYE, to + Vec3::Y * crate::equipment::CHEST_HEIGHT, 0.).is_none()
    }

    /// A spawn point inside a room, plus a standable spot outside within 60 m
    /// that the room's walls hide from it.
    fn hidden_pair(map: MapId) -> (Vec3, Vec3) {
        let pack = crate::map_pack::on(map).expect("embedded map");
        for (spawn, _) in crate::terrain::spawn_points_on(map, true) {
            for r in [25.0f32, 35.0, 45.0, 55.0] {
                for k in 0..24 {
                    let t = k as f32 / 24.0 * std::f32::consts::TAU;
                    let (x, z) = (spawn.x + r * t.cos(), spawn.z + r * t.sin());
                    let Some((fy, _)) = pack.floor(Vec3::new(x, spawn.y + 30.0, z)) else { continue };
                    let p = Vec3::new(x, fy + 1.2, z);
                    if pack.body_sweep(p, p + Vec3::Y * 0.01).is_none() && !clear(map, spawn, p) && !clear(map, p, spawn) {
                        return (spawn, p);
                    }
                }
            }
        }
        panic!("{map:?}: no hidden position found near a spawn");
    }

    #[test]
    fn bots_engage_an_enemy_in_the_open() {
        let map = MapId::OldHoller;
        let (a, b) = (600..1500).step_by(40).flat_map(|x| (600..1500).step_by(40).map(move |z| (x as f32, z as f32)))
            .map(|(x, z)| (stand(map, x, z), stand(map, x, z + 40.0)))
            .find(|(a, b)| clear(map, *a, *b) && clear(map, *b, *a))
            .expect("two open spots 40 m apart");
        let mut w = duel(map, a, b, false);
        for _ in 0..(3.0 / STEP) as u32 { w.players[1].pos = b; w.players[1].vel = Vec3::ZERO; w.tick(STEP); }
        assert_eq!(w.players[0].bot_target, Some(1));
        assert!(w.players[0].shots > 0, "a bot with a clear line of sight fires");
    }

    #[test]
    fn bots_do_not_track_or_fire_through_walls() {
        for map in [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold, MapId::Dustreach] {
            let (a, b) = hidden_pair(map);
            let mut w = duel(map, a, b, false);
            for _ in 0..(3.0 / STEP) as u32 {
                w.players[0].pos = a; w.players[0].vel = Vec3::ZERO;
                w.players[1].pos = b; w.players[1].vel = Vec3::ZERO;
                w.tick(STEP);
                assert_eq!(w.players[0].bot_target, None, "{map:?}: bot targeted an enemy behind a wall");
            }
            assert_eq!(w.players[0].shots, 0, "{map:?}: bot fired at an enemy behind a wall");
        }
    }

    #[test]
    fn losing_sight_drops_the_target_and_heads_for_the_last_seen_spot() {
        let map = MapId::Dustreach;
        let (hidden_from, hidden) = hidden_pair(map);
        // Find a spot that does see `hidden_from`, then move the enemy behind the wall.
        let pack = crate::map_pack::on(map).unwrap();
        let open = (0..48).find_map(|k| {
            let t = k as f32 / 48.0 * std::f32::consts::TAU;
            let (x, z) = (hidden_from.x + 20.0 * t.cos(), hidden_from.z + 20.0 * t.sin());
            let (fy, _) = pack.floor(Vec3::new(x, hidden_from.y + 10.0, z))?;
            let p = Vec3::new(x, fy + 1.2, z);
            (pack.body_sweep(p, p + Vec3::Y * 0.01).is_none() && clear(map, hidden_from, p)).then_some(p)
        }).expect("a visible spot near the spawn");
        let mut w = duel(map, hidden_from, open, false);
        for _ in 0..30 { w.players[0].pos = hidden_from; w.players[1].pos = open; w.tick(STEP); }
        assert_eq!(w.players[0].bot_target, Some(1));
        let shots = w.players[0].shots;
        w.players[1].pos = hidden;
        for _ in 0..30 { w.players[0].pos = hidden_from; w.players[0].vel = Vec3::ZERO; w.players[1].pos = hidden; w.tick(STEP); }
        assert_eq!(w.players[0].bot_target, None, "target is dropped once sight breaks");
        assert!(w.players[0].bot_memory > 0.0, "the last-seen spot is remembered briefly");
        assert!(w.players[0].bot_seen.distance(open) < 0.5);
        assert_eq!(w.players[0].shots, shots, "no shots at the wall after sight breaks");
    }

    #[test]
    fn the_camera_moves_smoothly_between_fixed_steps() {
        // Frames at 144 Hz never line up with the 60 Hz simulation. Without
        // interpolation some frames show no movement and others a whole
        // step; with it, the camera advances evenly every frame.
        let run = |interpolate: bool| {
            let mut w = World::new();
            w.set_map(MapId::OldHoller);
            w.set_mode(crate::map_catalog::SupportedMode::Ctf);
            w.start_match(true);
            w.players.truncate(1);
            let at = Vec3::new(1060.0, 0.0, 700.0);
            let floor = crate::terrain::support_on(MapId::OldHoller, at + Vec3::Y * 200.0).0;
            w.players[0].pos = Vec3::new(at.x, floor + 400.0, at.z);   // falling freely: steady physics
            w.players[0].vel = Vec3::new(20.0, 0.0, 0.0);
            let mut xs = Vec::new();
            for _ in 0..60 {
                w.tick(1.0 / 144.0);
                if !interpolate { w.render_offset = Vec3::ZERO; }
                xs.push(w.camera().0.x);
            }
            // Skip the first frames, before the first steps have run.
            let steps: Vec<f32> = xs[3..].windows(2).map(|p| p[1] - p[0]).collect();
            let mean = steps.iter().sum::<f32>() / steps.len() as f32;
            steps.iter().map(|d| (d - mean).abs()).fold(0.0, f32::max) / mean
        };
        let (raw, smooth) = (run(false), run(true));
        assert!(raw > 0.9, "fixed steps alone judder: {raw}");
        assert!(smooth < 0.15, "interpolated frames advance evenly: {smooth}");
    }

    #[test]
    fn offline_bodies_move_smoothly_between_fixed_steps() {
        // At 75 Hz, a 60 Hz simulation leaves every fifth frame without a
        // step. Offline every body moves in those steps, so the others are
        // drawn between them like you are (`drawn_offset`); online their
        // offsets stay empty, since snapshots arrive already interpolated.
        let run = |interpolate: bool| {
            let mut w = World::new();
            w.set_map(MapId::OldHoller);
            w.set_mode(crate::map_catalog::SupportedMode::Ctf);
            w.start_match(true);
            w.players.truncate(2);
            w.players[1].is_bot = false;
            let at = Vec3::new(1060.0, 0.0, 700.0);
            let floor = crate::terrain::support_on(MapId::OldHoller, at + Vec3::Y * 200.0).0;
            for (k, p) in w.players.iter_mut().enumerate() {
                p.pos = Vec3::new(at.x + 30.0 * k as f32, floor + 400.0, at.z);   // falling freely
                p.vel = Vec3::new(20.0, 0.0, 0.0);
            }
            let mut xs = Vec::new();
            for _ in 0..60 {
                w.tick(1.0 / 75.0);
                xs.push(w.players[1].pos.x + if interpolate { w.drawn_offset(1).x } else { 0.0 });
            }
            let steps: Vec<f32> = xs[3..].windows(2).map(|p| p[1] - p[0]).collect();
            let mean = steps.iter().sum::<f32>() / steps.len() as f32;
            (steps.iter().map(|d| (d - mean).abs()).fold(0.0, f32::max) / mean, w)
        };
        let ((raw, _), (smooth, mut w)) = (run(false), run(true));
        assert!(raw > 0.9, "fixed steps alone judder: {raw}");
        assert!(smooth < 0.15, "interpolated bodies advance evenly: {smooth}");
        w.interpolate_local(0.5);
        assert_eq!(w.drawn_offset(1), Vec3::ZERO, "online, only your own body is offset");
    }

    #[test]
    #[ignore = "bot flag play per map (headless); run with --release -- --ignored --nocapture"]
    fn bot_flag_play_report() {
      for map in [MapId::Reefbreak, MapId::OzarkticBlast, MapId::Cairnhold] {
        let mut w = World::new();
        w.set_map(map);
        w.set_mode(crate::map_catalog::SupportedMode::Ctf);
        w.start_match(true);
        w.players[0].is_bot = true;
        w.assign_personality(0);
        let (mut grabs, mut carried) = (0, [false; 2]);
        let mut left_hold = 0usize;
        let mut kills = 0u32;
        for t in 0..(300.0 / STEP) as usize {
            if w.state != MatchState::Playing { break; }
            w.physics_step();
            for f in 0..2 {
                let c = w.flags[f].carrier.is_some();
                if c && !carried[f] { grabs += 1; }
                carried[f] = c;
            }
            if t % 60 == 0 {
                left_hold += w.players.iter().filter(|p| p.alive && p.pos.y < 70.0).count();
            }
        }
        for p in &w.players { kills += p.frags; }
        println!("{map:?}: {grabs} flag grabs, score {:?}, {kills} frags, {left_hold} low samples", w.score);
      }
    }

    #[test]
    #[ignore = "bot soak; run with --release -- --ignored --nocapture"]
    fn bot_soak_report() {
        for map in MAPS {
            for secs in [60., 180.] { let (shots, blind, kills) = soak(map, secs);
            println!("{map:?} {secs}s: shots {shots}, fired with no enemy in sight {blind}, kills {kills}"); }
        }
    }
}

#[cfg(test)]
mod capture_and_hold_tests {
    use super::*;
    use crate::control::{Definition, Drain};
    use crate::map_catalog::SupportedMode;

    fn defs(map: MapId) -> Vec<Definition> {
        let at = |x: f32, z: f32| [x, crate::terrain::height_on(map, x, z), z];
        vec![Definition { id: "west".into(), name: "West".into(), pos: at(900.0, 1000.0), radius: 12.0, ctf_active: false, drain: None, turret: None,
                capture: None, spires: Vec::new(), beacon: None, cook: None },
             Definition { id: "beacon".into(), name: "Beacon".into(), pos: at(1024.0, 1024.0), radius: 12.0, ctf_active: true,
                drain: Some(Drain { radius: 60.0, rate: crate::control::DEFAULT_DRAIN_RATE, airborne: false, damage: 0.0 }), turret: None,
                capture: None, spires: Vec::new(), beacon: None, cook: None }]
    }

    fn playing(mode: SupportedMode) -> (Match, usize, usize) {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(mode);
        m.world.set_control_points(defs(MapId::OldHoller));
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        assert_eq!(m.phase, Phase::Playing);
        (m, a, b)
    }

    fn hold(m: &mut Match, slot: usize, at: Vec3, seconds: f32) {
        for _ in 0..(seconds / STEP) as usize {
            let p = &mut m.world.players[slot];
            p.pos = at + Vec3::Y * 1.2; p.vel = Vec3::ZERO; p.alive = true;
            m.step(&[]);
        }
    }

    #[test]
    fn capture_and_hold_captures_scores_and_wins() {
        let (mut m, a, b) = playing(SupportedMode::CaptureAndHold);
        m.world.players[b].pos = Vec3::new(100.0, 500.0, 100.0);
        assert!(m.world.points.iter().all(|p| p.active), "all points run in Capture & Hold");
        let west = m.world.points[0].pos;
        hold(&mut m, a, west, 9.8);
        assert_eq!(m.world.points[0].owner, None);
        hold(&mut m, a, west, 0.4);
        let team = m.world.players[a].team.idx() as u8;
        assert_eq!(m.world.points[0].owner, Some(team));
        let before = m.world.score[team as usize];
        hold(&mut m, a, west, 5.0);
        let gained = m.world.score[team as usize] - before;
        assert!((4..=6).contains(&gained), "one held point scores 1/s, got {gained}");
        m.world.score[team as usize] = crate::control::CNH_TARGET - 1;
        hold(&mut m, a, west, 1.1);
        assert_eq!(m.world.state, MatchState::Ended);
        assert_eq!(m.world.score[team as usize], crate::control::CNH_TARGET);
        assert!(m.world.flags.iter().all(|f| f.carrier.is_none()), "flags stay inactive");
    }

    #[test]
    fn ctf_only_runs_points_marked_ctf_active_and_warmup_never_captures() {
        let (mut m, a, _) = playing(SupportedMode::Ctf);
        assert_eq!(m.world.points.iter().map(|p| p.active).collect::<Vec<_>>(), vec![false, true]);
        let (west, beacon) = (m.world.points[0].pos, m.world.points[1].pos);
        hold(&mut m, a, west, 11.0);
        assert_eq!(m.world.points[0].owner, None, "inactive in CTF");
        hold(&mut m, a, beacon, 10.2);
        assert!(m.world.points[1].owner.is_some());
        assert_eq!(m.world.score, [0, 0], "CTF points never score");
        let mut w = Match::new(MapId::OldHoller);
        w.world.set_control_points(defs(MapId::OldHoller));
        let solo = w.join(1, "Solo").unwrap();
        let beacon = w.world.points[1].pos;
        hold(&mut w, solo, beacon, 11.0);
        assert_eq!(w.phase, Phase::Waiting);
        assert_eq!(w.world.points[1].owner, None, "warmup never captures");
    }

    #[test]
    fn a_point_turret_fights_for_whoever_holds_the_point() {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::Ctf);
        let mut defs = defs(MapId::OldHoller);
        let top = Vec3::from_array(defs[1].pos) + Vec3::Y * 20.15;
        defs[1].turret = Some(crate::control::PointTurret { deployable: "rocket-turret".into(), at: top.to_array(), range: Some(200.0) });
        crate::control::validate(&defs).unwrap();
        m.world.set_control_points(defs);
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        let beacon = m.world.points[1].pos;
        // An enemy of Alpha's out in the open 150 m off the tower (past the
        // rocket turret's own 150 m, inside this map's 200), holding their
        // jets or not.
        let jetting = std::cell::Cell::new(false);
        let foe_at = |m: &mut Match| {
            let at = beacon + Vec3::new(150.0, 0.0, 0.0);
            let p = &mut m.world.players[b];
            p.pos = Vec3::new(at.x, crate::terrain::support_on(MapId::OldHoller, at + Vec3::Y * 200.0).0 + 6.0, at.z);
            p.vel = Vec3::ZERO; p.alive = true; p.energy = 60.0;
        };
        let controls = |m: &Match| {
            let mut c = vec![None; m.world.players.len()];
            c[b] = Some(Command { seq: m.tick + 1, jet: jetting.get(), ..Default::default() });
            c
        };
        let turret_shots = |m: &Match| m.world.discs.iter().filter(|d| d.owner == MAX_PLAYERS && d.spin <= -1000.0).count();
        foe_at(&mut m);
        for _ in 0..120 { foe_at(&mut m); m.step(&[]); }
        assert_eq!(turret_shots(&m), 0, "idle while nobody holds the point");
        hold(&mut m, a, beacon, 10.5);
        let team = m.world.players[a].team;
        assert_eq!(m.world.points[1].owner, Some(team.idx() as u8));
        m.world.players[a].pos = Vec3::new(100.0, 500.0, 100.0);
        for _ in 0..120 { foe_at(&mut m); let c = controls(&m); m.step(&c); }
        assert_eq!(turret_shots(&m), 0, "held, but its enemy isn't using their jets");
        jetting.set(true);
        let mut fired = false;
        for _ in 0..240 {
            foe_at(&mut m);
            let c = controls(&m);
            m.step(&c);
            if m.world.discs.iter().any(|d| d.owner == MAX_PLAYERS && d.spin <= -1000.0 && d.team == team) { fired = true; break; }
        }
        assert!(fired, "it fires for the holder at their jetting enemy");
        let rocket = crate::registry::registry().projectiles.index("turret-rocket").unwrap() as u8;
        let shot = m.world.discs.iter().find(|d| d.kind == rocket).expect("its rockets");
        assert_eq!(shot.target as usize, b + 1, "the rocket seeks the one it was fired at");
    }

    /// Seconds of continuous jetting from a full tank, and energy regained in
    /// the next four seconds on the ground, with or without an enemy drain field.
    fn jet_budget(drained: bool) -> (f32, f32) {
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.start_match(true);
        w.players.truncate(1);
        w.set_control_points(defs(MapId::OldHoller));
        let beacon = w.points[1].pos;
        w.points[1].owner = Some(if drained { 1 } else { 0 });
        let high = beacon + Vec3::new(20.0, 300.0, 0.0);
        w.players[0].pos = high; w.players[0].vel = Vec3::ZERO; w.players[0].energy = crate::defs::movement().energy_max;
        w.input.jet = true;
        let mut airtime = 0.0;
        for _ in 0..600 {
            w.players[0].pos.y = high.y; w.players[0].vel.y = 0.0;
            w.step_players(STEP);
            if !w.players[0].jetting { break; }
            airtime += STEP;
        }
        w.input.jet = false;
        let start = w.players[0].energy;
        for _ in 0..240 { w.players[0].pos.y = high.y; w.players[0].vel.y = 0.0; w.step_players(STEP); }
        (airtime, w.players[0].energy - start)
    }

    #[test]
    fn drain_field_allows_hops_but_not_sustained_flight() {
        let (free, free_regen) = jet_budget(false);
        let (drained, drained_regen) = jet_budget(true);
        assert!((free - 3.8).abs() < 0.1, "normal tank: {free} s");
        assert!((drained - 2.3).abs() < 0.1, "drained tank: {drained} s");
        assert!((free_regen - 48.0).abs() < 1.0 && (drained_regen - 8.0).abs() < 1.0,
            "regen over 4 s: {free_regen} normal, {drained_regen} drained");
    }

    /// A post with spires: a touch takes it, and its drain strikes only
    /// enemies off the ground that a spire can see (`World::point_drain`).
    #[test]
    fn a_post_flips_at_a_touch_and_its_spires_strike_flyers_in_sight() {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::Ctf);
        let mut d = defs(MapId::OldHoller);
        let post = Vec3::from_array(d[1].pos);
        let tip = post + Vec3::new(0.0, 30.0, 0.0);
        d[1].capture = Some(1.0);
        d[1].drain = Some(Drain { radius: 60.0, rate: 15.0, airborne: true, damage: 7.5 });
        d[1].spires = vec![tip.to_array()];
        crate::control::validate(&d).unwrap();
        m.world.set_control_points(d);
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        hold(&mut m, a, post, 1.1);
        let owner = m.world.players[a].team.idx() as u8;
        assert_eq!(m.world.points[1].owner, Some(owner), "a touch takes it");
        assert_ne!(m.world.players[b].team.idx() as u8, owner);
        let enemy = &mut m.world.players[b];
        enemy.pos = post + Vec3::new(20.0, 25.0, 0.0); enemy.on_ground = false; enemy.alive = true;
        let drained = m.world.point_drain(b).expect("a flyer in sight");
        assert_eq!((drained.rate, drained.damage, drained.spire), (15.0, 7.5, Some(tip)));
        // The lightning hurts too, on the server's step.
        let health = m.world.players[b].health;
        for _ in 0..60 {
            let enemy = &mut m.world.players[b];
            enemy.pos = post + Vec3::new(20.0, 25.0, 0.0); enemy.vel = Vec3::ZERO; enemy.on_ground = false;
            m.step(&[]);
        }
        let lost = health - m.world.players[b].health;
        assert!(lost > 2.0 && lost < 7.5 + 0.5, "about 7.5 a second before armor: lost {lost}");
        m.world.players[b].on_ground = true;
        assert!(m.world.point_drain(b).is_none(), "players on the ground are spared");
        let enemy = &mut m.world.players[b];
        enemy.on_ground = false; enemy.pos = post + Vec3::new(20.0, -30.0, 0.0);
        assert!(m.world.point_drain(b).is_none(), "the ground hides them from the spire");
        assert!(m.world.point_drain(a).is_none(), "holders are never drained");
    }

    /// A post whose hall burns its holders (`control::Cook`): the team that
    /// holds it burns inside, its enemies are welcome there, and C&H bots
    /// don't guard it from inside.
    #[test]
    fn a_cooking_post_burns_its_holders_inside_and_welcomes_their_enemies() {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::Ctf);
        let mut d = defs(MapId::OldHoller);
        let post = Vec3::from_array(d[1].pos);
        d[1].capture = Some(1.0);
        d[1].cook = Some(crate::control::Cook { min: (post - Vec3::new(15.0, 5.0, 15.0)).to_array(),
            max: (post + Vec3::new(15.0, 20.0, 15.0)).to_array(), damage: 20.0 });
        crate::control::validate(&d).unwrap();
        m.world.set_control_points(d);
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        m.world.players[b].pos = post + Vec3::new(10.0, 1.2, 10.0);
        hold(&mut m, a, post, 1.1);
        assert_eq!(m.world.points[1].owner, Some(m.world.players[a].team.idx() as u8));
        let (ha, hb) = (m.world.players[a].health, m.world.players[b].health);
        let side = post + Vec3::new(10.0, 1.2, -10.0);
        for _ in 0..(1.0 / STEP) as usize {
            for (k, at) in [(a, side), (b, side + Vec3::X)] {
                let p = &mut m.world.players[k]; p.pos = at; p.vel = Vec3::ZERO;
            }
            m.step(&[]);
        }
        let lost = ha - m.world.players[a].health;
        assert!(lost > 15.0 && lost < 20.5, "its holder burns about 20 a second before armor: lost {lost}");
        assert_eq!(m.world.players[b].health, hb, "their enemy is welcome inside");
        assert_eq!(m.world.point_cook(a).map(|(_, burn)| burn), Some(20.0));
        m.world.players[a].pos = post + Vec3::new(16.0, 1.2, 0.0);
        assert!(m.world.point_cook(a).is_none(), "outside the hall");
    }

    #[test]
    fn rotation_keeps_mode_and_restart_resets_points() {
        let (mut m, a, _) = playing(SupportedMode::CaptureAndHold);
        let west = m.world.points[0].pos;
        hold(&mut m, a, west, 10.2);
        assert!(m.world.points[0].owner.is_some());
        m.rotate_to(MapId::OldHoller);
        assert_eq!(m.world.mode, SupportedMode::CaptureAndHold);
        m.rotate_to_mode(MapId::OldHoller, SupportedMode::Ctf);
        assert_eq!(m.world.mode, SupportedMode::Ctf);
        // Back to Old Holler's own three points, none of them CTF-active.
        assert_eq!(m.world.points.len(), 3);
        assert!(m.world.points.iter().all(|p| !p.active && p.owner.is_none()));
    }

    /// Old Holler places three Capture & Hold towers: the
    /// Crossing platform beside the bridge at mid-span and two knolls that
    /// mirror each other through the map centre. None runs in CTF.
    #[test]
    fn old_holler_declares_three_capture_and_hold_points() {
        let pack = crate::map_pack::on(MapId::OldHoller).expect("embedded");
        let points = &pack.manifest.control_points;
        assert_eq!(points.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["crossing", "west-knoll", "east-knoll"]);
        assert!(points.iter().all(|p| !p.ctf_active && p.drain.is_none() && p.radius == 12.0));
        let (w, e) = (Vec3::from_array(points[1].pos), Vec3::from_array(points[2].pos));
        assert!((w.x + e.x - 1960.0).abs() < 0.01 && (w.z + e.z - 1880.0).abs() < 0.01, "knolls mirror through (980, 940)");
        // Every point's ring is standable floor under its centre.
        for p in points {
            let c = Vec3::from_array(p.pos);
            let floor = crate::terrain::support_on(MapId::OldHoller, c + Vec3::new(8.0, 1.2, 0.0)).0;
            assert!((floor - c.y).abs() < 2.1, "{} ring floor {floor} vs {}", p.id, c.y);
        }
        let (mut m, a, _) = {
            let mut m = Match::new(MapId::OldHoller);
            m.world.set_mode(SupportedMode::CaptureAndHold);
            let a = m.join(1, "Alpha").unwrap(); let b = m.join(2, "Bravo").unwrap();
            for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
            (m, a, b)
        };
        assert_eq!(m.world.points.len(), 3);
        assert!(m.world.points.iter().all(|p| p.active), "all three run in Capture & Hold");
        let crossing = m.world.points[0].pos;
        hold(&mut m, a, crossing, 10.2);
        assert!(m.world.points[0].owner.is_some(), "the Crossing platform captures");
    }

    #[test]
    fn tower_complex_holds_three_mirrored_capture_towers() {
        let map = MapId::TowerComplex;
        let pack = crate::map_pack::on(map).expect("embedded");
        let points = &pack.manifest.control_points;
        assert_eq!(points.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Summit", "Westfall", "Eastfall"]);
        for p in points {
            assert_eq!(p.ctf_active, p.name == "Summit", "{}: only the Summit is a CTF centre", p.name);
            assert!(p.drain.is_none());
            assert_eq!(p.turret.as_ref().map(|t| t.deployable.as_str()), (p.name == "Summit").then_some("plasma-turret"));
            assert_eq!(p.radius, 12.0);
            let ground = crate::terrain::height_on(map, p.pos[0], p.pos[2]);
            assert!((ground - p.pos[1]).abs() < 0.2, "{} ring centre {:?} off the ground ({ground})", p.name, p.pos);
        }
        let (a, b) = (points[1].pos, points[2].pos);
        assert_eq!((a[0] + b[0], a[2] + b[2]), (2.0 * points[0].pos[0], 2.0 * points[0].pos[2]));
        let mut m = Match::new(map);
        m.world.set_mode(SupportedMode::CaptureAndHold);   // points come from the manifest
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        assert_eq!(m.phase, Phase::Playing);
        assert_eq!(m.world.points.len(), 3);
        assert!(m.world.points.iter().all(|p| p.active));
        m.world.players[b].pos = Vec3::new(100.0, 500.0, 100.0);
        let summit = m.world.points[0].pos;
        hold(&mut m, a, summit, 10.2);
        assert!(m.world.points[0].owner.is_some(), "the Summit ring captures on its plateau");
        m.world.set_mode(SupportedMode::Ctf);
        assert_eq!(m.world.points.iter().map(|p| p.active).collect::<Vec<_>>(), [true, false, false], "the Summit is Tower Complex's CTF centre");
    }

    /// Frostline's centre is the beacon: a Capture & Hold point that also runs
    /// in CTF, with a drain field. The West and East Cols are Capture & Hold
    /// only and mirror each other through the map centre.
    #[test]
    fn frostline_beacon_runs_in_ctf_and_drains_only_the_holders_enemies() {
        let map = MapId::Frostline;
        let pack = crate::map_pack::on(map).expect("embedded");
        let points = &pack.manifest.control_points;
        assert_eq!(points.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Beacon", "West Col", "East Col"]);
        assert!(points[0].ctf_active && points[0].drain.is_some());
        assert!(points[1..].iter().all(|p| !p.ctf_active && p.drain.is_none()));
        let (w, e) = (points[1].pos, points[2].pos);
        assert_eq!((w[0] + e[0], w[2] + e[2]), (2048.0, 2048.0));
        for p in points {
            let floor = crate::terrain::support_on(map, Vec3::from_array(p.pos) + Vec3::new(7.0, 1.2, 7.0)).0;
            assert!((floor - p.pos[1]).abs() < 2.1, "{} ring floor {floor} vs {:?}", p.name, p.pos);
        }
        let mut m = Match::new(map);
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        assert_eq!(m.phase, Phase::Playing);
        assert_eq!(m.world.mode, SupportedMode::Ctf);
        assert!(m.world.points[0].active && m.world.points[1..].iter().all(|p| !p.active), "only the beacon runs in CTF");
        m.world.players[b].pos = Vec3::new(100.0, 500.0, 100.0);
        let beacon = m.world.points[0].pos;
        hold(&mut m, a, beacon, 9.0);
        assert!(m.world.points[0].owner.is_none(), "not held after 9 s");
        hold(&mut m, a, beacon, 1.2);
        let owner = m.world.points[0].owner.expect("the beacon captures in about 10 s");
        assert_eq!(owner, m.world.players[a].team.idx() as u8);
        // Drain: the holder's enemies inside 60 m lose energy, the holders do not.
        let near = beacon + Vec3::new(40.0, 30.0, 0.0);
        let enemy = 1 - owner;
        let at = |team, pos| crate::control::drain_at(&m.world.points, team, pos, false, |_| true);
        assert_eq!(at(enemy, near), crate::control::DEFAULT_DRAIN_RATE);
        assert_eq!(at(owner, near), 0.0);
        assert_eq!(at(enemy, beacon + Vec3::new(70.0, 0.0, 0.0)), 0.0);
        m.world.set_mode(SupportedMode::CaptureAndHold);
        assert!(m.world.points.iter().all(|p| p.active), "all three run in Capture & Hold");
    }

    /// Dustreach: the Sun Gate is the centre point (its arch spans the flag
    /// lane, so the ring has marker posts rather than a pylon), and the West
    /// and East Wadi towers mirror each other through the map centre. None
    /// runs in CTF.
    #[test]
    fn dustreach_holds_the_sun_gate_and_two_mirrored_wadi_towers() {
        let map = MapId::Dustreach;
        let pack = crate::map_pack::on(map).expect("embedded");
        let points = &pack.manifest.control_points;
        assert_eq!(points.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Sun Gate", "West Wadi", "East Wadi"]);
        assert!(points.iter().all(|p| !p.ctf_active && p.drain.is_none() && p.radius == 12.0));
        assert_eq!((points[0].pos[0], points[0].pos[2]), (1024.0, 1024.0));
        let (w, e) = (points[1].pos, points[2].pos);
        assert_eq!((w[0] + e[0], w[2] + e[2]), (2048.0, 2048.0));
        for p in points {
            let floor = crate::terrain::support_on(map, Vec3::from_array(p.pos) + Vec3::new(0.0, 1.2, 8.0)).0;
            assert!((floor - p.pos[1]).abs() < 2.1, "{} ring floor {floor} vs {:?}", p.name, p.pos);
        }
        let mut m = Match::new(map);
        m.world.set_mode(SupportedMode::CaptureAndHold);
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        assert_eq!(m.phase, Phase::Playing);
        assert!(m.world.points.iter().all(|p| p.active));
        m.world.players[b].pos = Vec3::new(100.0, 500.0, 100.0);
        let gate = m.world.points[0].pos;
        hold(&mut m, a, gate, 10.2);
        assert!(m.world.points[0].owner.is_some(), "the Sun Gate ring captures under the arch");
        m.world.set_mode(SupportedMode::Ctf);
        assert!(m.world.points.iter().all(|p| !p.active), "no CTF-active point on Dustreach");
    }

    #[test]
    /// Cairnhold: the Ring's dais is the centre point (the shared tower stands
    /// where the old brazier stone was, and the 12 m capture ring is the
    /// dais), and West and East Cairn mirror each other through the map
    /// centre on levelled plateaus. The Ring is also the CTF centre, with a
    /// rocket turret on its tower.
    fn cairnhold_holds_the_ring_and_two_mirrored_cairns() {
        let map = MapId::Cairnhold;
        let pack = crate::map_pack::on(map).expect("embedded");
        let points = &pack.manifest.control_points;
        assert_eq!(points.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["The Ring", "West Cairn", "East Cairn"]);
        assert!(points.iter().all(|p| p.drain.is_none() && p.radius == 12.0));
        assert_eq!(points.iter().map(|p| p.ctf_active).collect::<Vec<_>>(), [true, false, false]);
        assert_eq!(points[0].turret.as_ref().map(|t| t.deployable.as_str()), Some("rocket-turret"));
        assert_eq!((points[0].pos[0], points[0].pos[2]), (1024.0, 1024.0));
        let (w, e) = (points[1].pos, points[2].pos);
        assert_eq!((w[0] + e[0], w[2] + e[2]), (2048.0, 2048.0));
        for p in points {
            // Standing ground 8 m out from the pylon, inside the ring.
            let floor = crate::terrain::support_on(map, Vec3::from_array(p.pos) + Vec3::new(0.0, 1.2, 8.0)).0;
            assert!((floor - p.pos[1]).abs() < 1.0, "{} ring floor {floor} vs {:?}", p.name, p.pos);
        }
        let mut m = Match::new(map);
        m.world.set_mode(SupportedMode::CaptureAndHold);
        let a = m.join(1, "Alpha").unwrap();
        let b = m.join(2, "Bravo").unwrap();
        for _ in 0..(ROUND_COUNTDOWN / STEP) as usize + 60 { m.step(&[]); if m.phase == Phase::Playing { break; } }
        assert_eq!(m.phase, Phase::Playing);
        assert!(m.world.points.iter().all(|p| p.active));
        m.world.players[b].pos = Vec3::new(100.0, 500.0, 100.0);
        let ring = m.world.points[0].pos;
        hold(&mut m, a, ring, 10.2);
        assert!(m.world.points[0].owner.is_some(), "the Ring's dais captures");
        let west = m.world.points[1].pos;
        hold(&mut m, a, west, 10.2);
        assert!(m.world.points[1].owner.is_some(), "West Cairn captures on its plateau");
        m.world.set_mode(SupportedMode::Ctf);
        assert_eq!(m.world.points.iter().map(|p| p.active).collect::<Vec<_>>(), [true, false, false], "the Ring is Cairnhold's CTF centre");
    }
}

#[cfg(test)]
mod bot_soak_metrics {
    use super::*;

    pub(crate) const MAPS: [MapId; 5] = [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold,
        MapId::Frostline, MapId::Dustreach];

    /// Bot-only match: (seconds until each team first touches the enemy flag,
    /// sight encounters per minute, kills per minute, [mean, worst] tick cost
    /// in microseconds).
    pub(crate) fn soak(map: MapId, seconds: f32) -> ([Option<f32>; 2], f32, f32, [f32; 2]) {
        crate::bot_nav::graph(map);
        let mut w = World::new();
        w.set_map(map);
        w.start_match(true);
        w.players[0].is_bot = true;
        // Offline fills five bots; add two so the cost covers seven bots
        // beside the player's slot (which a bot also drives here).
        for team in [Team::Ember, Team::Glacier] {
            let (pos, yaw) = w.players.iter().find(|p| p.team == team).map(|p| (p.pos, p.yaw)).unwrap();
            w.players.push(make_player(team, true, pos + Vec3::new(2.0, 0.0, 0.0), yaw, BotRole::Offense));
            let i = w.players.len() - 1;
            w.assign_personality(i);
        }
        let mut reach = [None; 2];
        let mut encounters = 0u32;
        let mut had: Vec<bool> = vec![false; w.players.len()];
        let steps = (seconds / STEP) as u32;
        let (mut total, mut worst) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
        for s in 0..steps {
            let clock = std::time::Instant::now();
            w.tick(STEP);
            let spent = clock.elapsed();
            total += spent;
            worst = worst.max(spent);
            let t = s as f32 * STEP;
            for (i, p) in w.players.iter().enumerate() {
                let now = p.bot_target.is_some();
                if now && !had[i] { encounters += 1; }
                had[i] = now;
                let team = p.team.idx();
                if reach[team].is_none() && p.alive
                    && (p.carrying.is_some() || p.pos.distance(w.flags[1 - team].home) < 6.0) {
                    reach[team] = Some(t);
                }
            }
        }
        let kills: u32 = w.players.iter().map(|p| p.frags).sum();
        let min = seconds / 60.0;
        let cost = [total.as_secs_f32() * 1e6 / steps as f32, worst.as_secs_f32() * 1e6];
        (reach, encounters as f32 / min, kills as f32 / min, cost)
    }

    /// A lone bot from each team's spawn, on every map: seconds until it
    /// reaches (touches or takes) the enemy flag, or `None`.
    pub(crate) fn lone_run(map: MapId, ember: bool, limit: f32) -> Option<f32> {
        crate::bot_nav::graph(map);
        let mut w = World::new();
        w.set_map(map);
        w.start_match(ember);
        let team = w.players[0].team;
        let mut bot = make_player(team, true, w.players[0].pos, w.players[0].yaw, BotRole::Offense);
        bot.bot_profile = crate::bot_nav::ARCHETYPES[1];
        w.players = vec![bot];
        let target = w.flags[team.other().idx()].home;
        for s in 0..(limit / STEP) as u32 {
            // Navigation, not survival: base turrets would otherwise end the run.
            w.players[0].health = 100.0;
            w.tick(STEP);
            let p = &w.players[0];
            if p.carrying.is_some() || (p.alive && p.pos.distance(target) < 6.0) { return Some(s as f32 * STEP); }
        }
        None
    }

    /// A lone flyer (the Hawk archetype): (seconds to the enemy flag, share
    /// of the run spent airborne), or `None` if it never got there.
    pub(crate) fn flyer_run(map: MapId, ember: bool, limit: f32) -> Option<(f32, f32)> {
        crate::bot_nav::graph(map);
        let mut w = World::new();
        w.set_map(map);
        w.start_match(ember);
        let team = w.players[0].team;
        let mut bot = make_player(team, true, w.players[0].pos, w.players[0].yaw, BotRole::Offense);
        bot.bot_profile = *crate::bot_nav::ARCHETYPES.iter().find(|a| a.style == crate::bot_nav::MoveStyle::Flyer).unwrap();
        w.players = vec![bot];
        let target = w.flags[team.other().idx()].home;
        let mut air = 0u32;
        for s in 0..(limit / STEP) as u32 {
            w.players[0].health = 100.0;
            w.tick(STEP);
            let p = &w.players[0];
            if !p.on_ground { air += 1; }
            if p.carrying.is_some() || (p.alive && p.pos.distance(target) < 6.0) {
                return Some((s as f32 * STEP, air as f32 / (s + 1) as f32));
            }
        }
        None
    }

    #[test]
    fn flyers_cross_every_map_mostly_airborne() {
        let mut missed = Vec::new();
        for map in MAPS {
            for ember in [true, false] {
                let r = flyer_run(map, ember, 240.0);
                println!("{map:?} {} flyer: {r:?}", if ember { "ember" } else { "glacier" });
                match r {
                    Some((_, air)) if air >= 0.4 => {}
                    _ => missed.push((map, ember, r)),
                }
            }
        }
        assert!(missed.is_empty(), "flyers that never arrived or stayed grounded: {missed:?}");
    }

    #[test]
    fn a_lone_bot_reaches_the_enemy_flag_on_every_map() {
        let mut missed = Vec::new();
        for map in MAPS {
            for ember in [true, false] {
                // Old Holler's glacier route is the long one (230 s before
                // released-key footing, 327 s after: the bot's path is chaotic).
                let t = lone_run(map, ember, 360.0);
                println!("{map:?} {}: {t:?}", if ember { "ember" } else { "glacier" });
                if t.is_none() { missed.push((map, ember)); }
            }
        }
        assert!(missed.is_empty(), "never reached the enemy flag: {missed:?}");
    }

    #[test]
    #[ignore = "bot navigation soak; run with --release -- --ignored --nocapture"]
    fn bot_navigation_soak_report() {
        for map in MAPS {
            let (reach, enc, kills, cost) = soak(map, 180.0);
            println!("{map:?}: flag reach ember {:?} glacier {:?}, encounters/min {enc:.1}, kills/min {kills:.2}, tick {:.0} us mean {:.0} us worst ({} players)",
                reach[0].map(|t| t.round()), reach[1].map(|t| t.round()), cost[0], cost[1], 8);
            // (seven bots plus the player's slot, also bot-driven)
        }
    }
}

#[cfg(test)]
mod water_tests {
    use super::*;
    use crate::water::Volume;

    /// A flat-ish spot on the Valley fixture, with a lone local player on it.
    fn solo_on_ground() -> (World, Vec3) {
        let mut world = World::new();
        world.set_map(MapId::OldHoller);
        world.start_match(true);
        world.players.truncate(1);
        let map = world.map;
        assert_eq!(map, MapId::OldHoller);
        let mut spot = Vec3::new(512.0, 0.0, 512.0);
        let ground = |x: f32, z: f32, from: f32| crate::terrain::support_on(map, Vec3::new(x, from, z)).0;
        'find: for x in (300..1700).step_by(20) {
            for z in (300..1700).step_by(20) {
                let (x, z) = (x as f32, z as f32);
                let h0 = ground(x, z, 2000.0);
                let flat = (-6..=6).step_by(2).all(|a| (-6..=6).step_by(2).all(|b| {
                    let (sx, sz) = (x + a as f32 * 4.0, z + b as f32 * 4.0);
                    (ground(sx, sz, h0 + 3.0) - h0).abs() < 0.8 && (ground(sx, sz, 2000.0) - h0).abs() < 0.8
                }));
                if flat { spot = Vec3::new(x, h0, z); break 'find; }
            }
        }
        assert!(spot.y != 0.0, "found a flat spot");
        world.player_id = 0;
        world.network_inputs.clear();
        let p = &mut world.players[0];
        p.is_bot = false;
        p.remote = false;
        p.alive = true;
        p.health = 100.0;
        p.energy = crate::defs::movement().energy_max;
        p.pos = spot + Vec3::Y * PLAYER_RADIUS;
        p.vel = Vec3::ZERO;
        p.yaw = std::f32::consts::FRAC_PI_2;
        p.on_ground = true;
        (world, spot)
    }

    fn pool(surface: f32, at: Vec3) -> Volume {
        Volume {surface, rect: Some([at.x - 300.0, at.z - 300.0, at.x + 300.0, at.z + 300.0]), ..Default::default()}
    }

    /// Horizontal speed after `secs`, entering at `speed` along +x.
    fn coast(depth: Option<f32>, speed: f32, ski: bool, secs: f32) -> f32 {
        let (mut world, spot) = solo_on_ground();
        if let Some(d) = depth { world.staged_water.push(pool(spot.y + d, spot)); }
        world.players[0].vel = Vec3::new(speed, 0.0, 0.0);
        world.input.jump = ski;
        world.input.jump_prev = ski;
        for _ in 0..(secs / STEP).round() as usize { world.step_players(STEP); }
        let v = world.players[0].vel;
        Vec2::new(v.x, v.z).length()
    }

    #[test]
    fn water_drag_grows_with_depth_and_brakes_skiers() {
        // Ankle, waist and swim depth in metres above the bed.
        let depths = [("dry", None), ("ankle", Some(0.3)), ("waist", Some(1.1)), ("swim", Some(3.0))];
        let mut ski_after_1 = Vec::new();
        for (name, d) in depths {
            let (s1, s3) = (coast(d, 30.0, true, 1.0), coast(d, 30.0, true, 3.0));
            let (w1, w3) = (coast(d, 8.0, false, 1.0), coast(d, 8.0, false, 3.0));
            println!("{name}: ski 30 m/s -> {s1:.1} after 1 s, {s3:.1} after 3 s; coast 8 m/s -> {w1:.2}, {w3:.2}");
            ski_after_1.push(s1);
        }
        assert!(ski_after_1[1] < ski_after_1[0] * 0.8, "ankle water brakes a skier: {ski_after_1:?}");
        assert!(ski_after_1[2] < ski_after_1[1] && ski_after_1[3] < ski_after_1[2], "{ski_after_1:?}");
        assert!(ski_after_1[3] < 8.0, "swimming kills ski speed: {ski_after_1:?}");
    }

    #[test]
    fn wading_slows_walking() {
        let walk = |depth: Option<f32>| {
            let (mut world, spot) = solo_on_ground();
            if let Some(d) = depth { world.staged_water.push(pool(spot.y + d, spot)); }
            world.input.move_z = 1.0;
            for _ in 0..120 { world.step_players(STEP); }
            let v = world.players[0].vel;
            Vec2::new(v.x, v.z).length()
        };
        let (dry, waist) = (walk(None), walk(Some(1.1)));
        println!("walk top speed dry {dry:.2}, waist-deep {waist:.2}");
        assert!((dry - crate::defs::movement().walk_max).abs() < 0.8 && waist < dry * 0.8 && waist > 3.0, "{dry} {waist}");
    }

    #[test]
    fn reefbreak_water_is_a_surface_you_ski_across_never_under() {
        let mut world = World::new();
        world.set_map(MapId::Reefbreak);
        world.set_mode(crate::map_catalog::SupportedMode::Ctf);
        world.start_match(true);
        world.players.truncate(1);
        let sea = crate::water::volumes(MapId::Reefbreak)[0].surface;
        // Over a deep blue hole (its bed well under the surface).
        let (x, z) = (1024.0 + 420.0 * 40f32.to_radians().sin(), 1024.0 + 420.0 * 40f32.to_radians().cos());
        assert!(crate::terrain::height_on(MapId::Reefbreak, x, z) < sea - 4.0, "a deep spot");
        world.players[0].pos = Vec3::new(x, sea + 20.0, z);
        world.players[0].vel = Vec3::ZERO;
        for _ in 0..240 { world.step_players(STEP); }
        let p = &world.players[0];
        assert!(p.on_ground && (p.pos.y - (sea + PLAYER_RADIUS)).abs() < 0.3, "lands on the surface: {} vs {sea}", p.pos.y);
        assert_eq!(crate::water::immersion(world.map, &world.staged_water, p.pos).0, 0.0, "never under it");
        // Skiing across keeps its speed: no water drag.
        world.players[0].vel = Vec3::new(30.0, 0.0, 0.0);
        world.input.jump = true;
        for _ in 0..60 { world.step_players(STEP); }
        let speed = Vec2::new(world.players[0].vel.x, world.players[0].vel.z).length();
        // The same ski on flat dry ground: water must cost nothing extra.
        let (mut dry, spot) = solo_on_ground();
        dry.players[0].pos = spot;
        dry.players[0].vel = Vec3::new(30.0, 0.0, 0.0);
        dry.input.jump = true;
        for _ in 0..60 { dry.step_players(STEP); }
        let land = Vec2::new(dry.players[0].vel.x, dry.players[0].vel.z).length();
        println!("ski 1 s from 30 m/s: water {speed:.2}, dry flat ground {land:.2}");
        assert!(speed >= land - 0.5, "skis across the water no slower than on land: {speed} vs {land}");
        assert!(world.players[0].pos.y >= sea, "still on top");
    }

    #[test]
    fn a_swimmer_floats_and_can_jet_out() {
        let (mut world, spot) = solo_on_ground();
        let surface = spot.y + 6.0;
        world.staged_water.push(pool(surface, spot));
        for _ in 0..600 { world.step_players(STEP); }
        let (wet, _) = crate::water::immersion(world.map, &world.staged_water, world.players[0].pos);
        assert!(wet > 0.5 && wet < 1.0, "floats partly submerged, not on the bed: {wet}");
        let eye = world.players[0].pos.y + crate::terrain::EYE;
        assert!((eye - surface).abs() < 1.0, "eye near the surface: {eye} vs {surface}");
        world.players[0].energy = crate::defs::movement().energy_max;
        world.input.jet = true;
        let mut t = 0.0;
        while crate::water::immersion(world.map, &world.staged_water, world.players[0].pos).0 > 0.0 {
            world.step_players(STEP);
            t += STEP;
            assert!(t < 3.0, "could not jet out");
        }
        println!("jet out of deep water in {t:.2} s, energy left {:.1}", world.players[0].energy);
    }

    #[test]
    fn dry_movement_is_unchanged_by_distant_water() {
        let run = |water: bool| {
            let (mut world, spot) = solo_on_ground();
            if water { world.staged_water.push(pool(spot.y + 5.0, spot + Vec3::new(1400.0, 0.0, 1400.0))); }
            let mut trace = Vec::new();
            for tick in 0..900 {
                world.input.move_z = if tick % 200 < 150 { 1.0 } else { 0.0 };
                world.input.jump = tick % 300 > 150;
                world.input.jet = tick % 240 > 200;
                world.step_players(STEP);
                let p = &world.players[0];
                trace.push((p.pos, p.vel, p.energy.to_bits()));
            }
            trace
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn prediction_matches_the_server_in_water() {
        let run = |predicting: bool| {
            let (mut world, spot) = solo_on_ground();
            world.staged_water.push(Volume {flow: [2.0, -1.0], ..pool(spot.y + 1.4, spot)});
            world.predicting = predicting;
            world.players[0].vel = Vec3::new(25.0, 0.0, 3.0);
            world.input.jump = true;
            for tick in 0..300 {
                world.input.jet = tick > 200;
                world.step_players(STEP);
            }
            let p = &world.players[0];
            (p.pos, p.vel, p.energy.to_bits())
        };
        assert_eq!(run(true), run(false));
    }

    #[test]
    fn under_water_a_bullet_fizzles_and_a_disc_bursts_at_once() {
        let (mut world, spot) = solo_on_ground();
        world.staged_water.push(pool(spot.y + 40.0, spot));
        let start = spot + Vec3::new(0.0, 20.0, 0.0);
        world.discs.push(Disc {pos: start, vel: Vec3::new(0.0, 0.0, -95.0), team: Team::Ember, owner: 0, life: 3.0, kind: 0, spin: 0.0, target: 0, power: 1.0 });
        world.discs.push(Disc {pos: start, vel: Vec3::new(0.0, 0.0, -420.0), team: Team::Ember, owner: 0, life: 1.0, kind: 1, spin: 0.0, target: 0, power: 1.0 });
        let before = world.explosions.len();
        world.step_discs(STEP);
        assert!(world.discs.is_empty(), "neither carries on under water");
        assert_eq!(world.explosions.len(), before + 1, "the disc bursts; the bullet fizzles");
    }

    #[test]
    fn a_disc_bursts_where_it_meets_water() {
        // Fired straight down at a pool 30 m from the player: it bursts at
        // the surface instead of crawling on under it (it used to slow to a
        // stop and hang there until its fuse ran out).
        let (mut world, spot) = solo_on_ground();
        let surface = spot.y + 1.0;
        world.staged_water.push(pool(surface, spot));
        let start = spot + Vec3::new(30.0, 11.0, 0.0);
        world.discs.push(Disc {pos: start, vel: Vec3::new(0.0, -95.0, 0.0), team: Team::Glacier, owner: 0, life: 3.0, kind: 0, spin: 0.0, target: 0, power: 1.0 });
        let before = world.explosions.len();
        for _ in 0..30 { world.step_discs(STEP); if world.discs.is_empty() { break; } }
        assert!(world.discs.is_empty(), "it burst");
        assert_eq!(world.explosions.len(), before + 1);
        let at = world.explosions.last().unwrap().pos;
        assert!((at.y - surface).abs() < 0.05 && (at.x - start.x).abs() < 0.01, "at the surface: {at} (surface {surface})");
    }

    #[test]
    fn a_water_burst_does_half_the_damage_and_twice_the_push() {
        let hit = |wet: bool| {
            let (mut world, _) = solo_on_ground();
            let at = world.players[0].pos + Vec3::new(2.0, -0.4, 0.0);
            // Nobody's burst, from the other team.
            let (owner, team) = (99, world.players[0].team.other());
            if wet { world.explode_scaled(at, owner, 0, team, None, crate::water::BURST_DAMAGE, crate::water::BURST_PUSH); }
            else { world.explode_direct(at, owner, 0, team, None); }
            (100.0 - world.players[0].health, world.players[0].vel)
        };
        let ((dmg, push), (wet_dmg, wet_push)) = (hit(false), hit(true));
        assert!(dmg > 5.0 && push.length() > 1.0, "the burst reaches: {dmg} {push}");
        assert!((wet_dmg - dmg * 0.5).abs() < 1e-3, "half the damage: {wet_dmg} vs {dmg}");
        assert!((wet_push - push * 2.0).length() < 1e-3, "twice the push: {wet_push} vs {push}");
    }

    #[test]
    fn water_crossings_are_downward_through_a_surface_within_its_footprint() {
        use crate::water::water_crossing;
        let water = [pool(10.0, Vec3::ZERO)];
        let v = |x: f32, y: f32| Vec3::new(x, y, 0.0);
        assert_eq!(water_crossing(MapId::Valley, &water, v(0.0, 11.0), v(0.0, 9.0)), Some(0.5));
        assert_eq!(water_crossing(MapId::Valley, &water, v(0.0, 9.0), v(0.0, 11.0)), None, "coming up out of it");
        assert_eq!(water_crossing(MapId::Valley, &water, v(0.0, 11.0), v(0.0, 10.5)), None, "not reaching it");
        assert_eq!(water_crossing(MapId::Valley, &water, v(500.0, 11.0), v(500.0, 9.0)), None, "beside it");
        let lava = [Volume { lava: true, ..pool(10.0, Vec3::ZERO) }];
        assert_eq!(water_crossing(MapId::Valley, &lava, v(0.0, 11.0), v(0.0, 9.0)), None, "lava is a surface of its own");
    }

    /// Every map's declared water: a skier coming off the bank at 20 m/s is
    /// braked hard, floats instead of sinking, and can jet back out.
    #[test]
    fn every_maps_water_slows_skiers_who_can_jet_out() {
        let maps = [MapId::OldHoller, MapId::TowerComplex, MapId::Cairnhold,
                    MapId::Frostline, MapId::Dustreach];
        for map in maps {
            let volumes = crate::water::volumes(map);
            assert!(!volumes.is_empty(), "{map:?} declares water");
            for (i, v) in volumes.iter().enumerate() {
                // Centre and the direction a skier comes in from.
                let ground = |q: Vec2| crate::terrain::support_on(map, Vec3::new(q.x, 2000.0, q.y)).0;
                let (centre, dir) = match (&v.rect, &v.polygon) {
                    // A long channel: come in across it above its deepest point.
                    (Some(r), _) => {
                        let z = (r[1] + r[3]) / 2.0;
                        let x = (0..100).map(|k| 200.0 + k as f32 * 16.0)
                            .min_by(|a, b| ground(Vec2::new(*a, z)).total_cmp(&ground(Vec2::new(*b, z)))).unwrap();
                        (Vec2::new(x, z), Vec2::new(0.0, 1.0))
                    }
                    (_, Some(p)) => {
                        let n = p.len() as f32;
                        (Vec2::new(p.iter().map(|q| q[0]).sum::<f32>() / n, p.iter().map(|q| q[1]).sum::<f32>() / n),
                         Vec2::new(1.0, 0.0))
                    }
                    _ => unreachable!(),
                };
                let mut shore = centre;
                while ground(shore) < v.surface + 0.1 { shore += dir; }
                let start = shore + dir * 6.0;
                let mut world = World::new();
                world.set_map(map);
                world.start_match(true);
                world.players.truncate(1);
                world.player_id = 0;
                world.network_inputs.clear();
                let p = &mut world.players[0];
                p.is_bot = false; p.remote = false; p.alive = true; p.health = 100.0; p.energy = crate::defs::movement().energy_max;
                p.pos = Vec3::new(start.x, ground(start) + PLAYER_RADIUS, start.y);
                p.vel = Vec3::new(-dir.x * 20.0, 0.0, -dir.y * 20.0);
                p.on_ground = true;
                world.input.jump = true;
                world.input.jump_prev = true;
                let mut wettest: f32 = 0.0;
                for _ in 0..(2.5 / STEP) as usize {
                    world.step_players(STEP);
                    wettest = wettest.max(crate::water::immersion(map, &[], world.players[0].pos).0);
                }
                let v2 = world.players[0].vel;
                let speed = Vec2::new(v2.x, v2.z).length();
                assert!(wettest > 0.4, "{map:?} volume {i}: the skier reached deep water ({wettest:.2})");
                assert!(speed < 8.0, "{map:?} volume {i}: water braked the skier to {speed:.1} m/s");
                for _ in 0..(2.0 / STEP) as usize { world.step_players(STEP); }
                let (wet, _) = crate::water::immersion(map, &[], world.players[0].pos);
                assert!(wet < 1.0, "{map:?} volume {i}: floats, not sunk ({wet:.2})");
                world.input.jump = false;
                world.players[0].energy = crate::defs::movement().energy_max;
                world.input.jet = true;
                let mut t = 0.0;
                while crate::water::immersion(map, &[], world.players[0].pos).0 > 0.0 {
                    world.step_players(STEP);
                    t += STEP;
                    assert!(t < 3.0, "{map:?} volume {i}: could not jet out");
                }
                println!("{map:?} volume {i}: deepest immersion {wettest:.2}, speed {speed:.1} m/s after 2.5 s, jet out {t:.2} s");
            }
        }
    }
}

#[cfg(test)]
mod viewmodel_aim_tests {
    use super::*;

    /// The angle between a weapon's barrel and the line from its muzzle to
    /// the point its rounds fly to (80 m ahead on the crosshair), in camera space.
    fn barrel_error(id: &str) -> f32 {
        let w = crate::registry::registry().weapons.by_id(id).unwrap();
        let (yaw, pitch) = vm_turn(w.view.kind());
        let barrel = (Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(pitch)).transform_vector3(Vec3::NEG_Z);
        barrel.angle_between(Vec3::new(0.0, 0.0, -80.0) - muzzle_of(Vec3::ZERO, Vec3::NEG_Z, w, VM_FOV))
    }

    #[test]
    fn disc_and_chaingun_barrels_point_where_their_rounds_go() {
        for id in ["disc", "chaingun"] {
            let err = barrel_error(id).to_degrees();
            assert!(err < 0.5, "{id}: barrel is {err:.3} degrees off its rounds");
        }
    }
}

#[cfg(test)]
mod mode_field_tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// A one-player CTF variant changed by `change`.
    fn with(key: &str, change: impl Fn(&mut crate::modes::ModeDef)) -> World {
        let mut def = SupportedMode::Ctf.def().clone();
        change(&mut def);
        let mode = SupportedMode::from_index(crate::modes::add(key, def).unwrap());
        let mut w = World::new();
        w.set_map(MapId::OldHoller);
        w.set_mode(mode);
        w.start_match(true);
        w.players.truncate(1);
        w
    }

    #[test]
    fn a_dropped_flag_returns_after_the_mode_s_time() {
        let mut w = with("test-flag-return", |d| d.flag_return = 2.0);
        let home = w.flags[1].home;
        w.drop_flag(Team::Glacier, home + Vec3::new(40.0, 0.0, 0.0));
        assert_eq!(w.flags[1].drop_timer, 2.0);
        w.players[0].pos = home + Vec3::new(-200.0, 0.0, 0.0);
        for _ in 0..(1.9 / STEP) as usize { w.physics_step(); }
        assert!(w.flags[1].pos.distance(home) > 10.0, "still out at 1.9 s");
        for _ in 0..(0.3 / STEP) as usize { w.physics_step(); }
        assert!(w.flags[1].pos.distance(home) < 1.0, "home after 2 s");
    }

    #[test]
    fn team_fire_hurts_teammates_at_the_mode_s_share() {
        for (key, share) in [("test-no-team-fire", 0.0), ("test-half-team-fire", 0.5)] {
            let mut w = with(key, |d| d.friendly_fire = share);
            let ground = |x: f32, z: f32| Vec3::new(x, crate::terrain::support_on(MapId::OldHoller, Vec3::new(x, 400.0, z)).0 + PLAYER_RADIUS, z);
            w.players[0].pos = ground(1060.0, 700.0);
            let mut mate = w.players[0].clone();
            mate.pos = ground(1060.0, 680.0);
            let mut foe = mate.clone();
            foe.team = Team::Glacier;
            foe.pos = ground(1060.0, 660.0);
            w.players.push(mate);
            w.players.push(foe);
            for target in [1, 2] {
                let at = w.players[target].pos + Vec3::new(1.0, 0.3, 0.0);
                w.explode(at, 0, 0, Team::Ember);
            }
            let (mate_hurt, foe_hurt) = (100.0 - w.players[1].health, 100.0 - w.players[2].health);
            assert!(foe_hurt > 10.0);
            assert!((mate_hurt - foe_hurt * share).abs() < 0.01, "{key}: teammate {mate_hurt} of {foe_hurt}");
        }
    }

    #[test]
    fn random_spawn_armor_draws_from_what_stations_offer() {
        let mut w = with("test-random-armor", |d| d.spawn_armor = crate::modes::SpawnArmor::Random);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..30 {
            w.kill(0, None, "Fall");
            w.respawn(0);
            seen.insert(w.players[0].armor);
        }
        let medium = ArmorClass::by_id("medium").unwrap();
        assert_eq!(seen, [ArmorClass::Light, medium, ArmorClass::Heavy].into_iter().collect(), "never the football kit");
        // The setup armor where the mode keeps it.
        let mut w = with("test-setup-armor", |_| {});
        w.players[0].setup.armor = Some(ArmorClass::Heavy);
        w.kill(0, None, "Fall");
        w.respawn(0);
        assert_eq!(w.players[0].armor, ArmorClass::Heavy);
    }
}

#[cfg(test)]
mod body_fall_tests {
    use super::*;

    fn world_in_the_air(height: f32, vel: Vec3) -> (World, usize, Vec3) {
        // The open test valley: nothing overhead to land on.
        let mut w = World::new();
        w.set_map(MapId::Valley);
        w.start_match(true);
        w.players.truncate(1);
        let i = w.player_id;
        let at = crate::terrain::spawn_on(w.map, true);
        let floor = crate::terrain::support_on(w.map, at + Vec3::Y * 0.5).0;
        w.players[i].pos = Vec3::new(at.x, floor + PLAYER_RADIUS + height, at.z);
        w.players[i].vel = vel;
        w.players[i].on_ground = false;
        (w, i, Vec3::new(at.x, floor, at.z))
    }

    /// Killed in mid-air, a body falls with its momentum and comes to rest
    /// on the ground; it used to hang where it died until the respawn. The
    /// pack it drops rides it down and lies with it (`fall1`).
    #[test]
    fn a_body_killed_in_the_air_falls_and_its_pack_lands_with_it() {
        let (mut w, i, ground) = world_in_the_air(20.0, Vec3::new(5.0, 4.0, 0.0));
        let start = w.players[i].pos;
        w.kill(i, None, "Fall");
        assert!(!w.players[i].alive);
        let k = w.loot.iter().position(|l| l.carried).expect("the pack rides the body down");
        assert_eq!(w.loot[k].rider_of(), Some(i));
        for _ in 0..(2.4 / STEP) as usize { w.tick(STEP); }
        let p = &w.players[i];
        assert!(!p.alive, "still down before the respawn");
        let floor = crate::terrain::support_on(w.map, p.pos + Vec3::Y * 0.5).0;
        assert!((p.pos.y - PLAYER_RADIUS - floor).abs() < 0.15, "the body lies on the ground: {} over {floor}", p.pos.y);
        assert!(p.pos.x > start.x + 3.0, "it kept its momentum: {} from {}", p.pos.x, start.x);
        assert_eq!(p.vel, Vec3::ZERO, "it came to rest");
        let l = &w.loot[k];
        assert!(!l.carried, "the pack settled with the body");
        assert!((l.pos - (p.pos - Vec3::Y * PLAYER_RADIUS)).length() < 0.6, "the pack lies by the body: {:?} {:?}", l.pos, p.pos);
        assert!(l.pos.y < start.y - 10.0 && (l.pos.y - floor).abs() < 0.15, "on the ground, not where they died: {:?} {ground:?}", l.pos);
    }

    /// Dying on the ground the body stays put and the pack is down at once.
    #[test]
    fn a_body_killed_standing_stays_and_drops_its_pack_at_once() {
        let (mut w, i, _) = world_in_the_air(0.0, Vec3::ZERO);
        for _ in 0..20 { w.tick(STEP); }
        assert!(w.players[i].on_ground);
        let before = w.players[i].pos;
        w.kill(i, None, "Fall");
        let l = w.loot.last().expect("a pack");
        assert!(!l.carried && (l.pos.y - (before.y - PLAYER_RADIUS)).abs() < 0.2, "{:?}", l.pos);
        for _ in 0..60 { w.tick(STEP); }
        assert!(w.players[i].pos.distance(before) < 0.05, "a body at rest stays where it fell");
    }
}
