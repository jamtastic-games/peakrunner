//! Every field a class's JSON may hold, with its allowed values and a line
//! about it. One table serves two jobs: the loader checks every number and
//! text a mod writes against it (after serde has checked the shape), and
//! `peakrunner-server --schema` prints it as the modders' reference, so the
//! documentation can't drift from what the engine accepts.
use super::Class;
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// A number in this range.
    Num(f32, f32),
    /// A whole number in this range.
    Int(i64, i64),
    Bool,
    /// Plain text up to this many characters.
    Text(usize),
    /// A short name: 1 to 32 of a-z, 0-9, `_` and `-`.
    Key,
    /// Another entry's id, of this class.
    Ref(Class),
    /// One of these words.
    Choice(&'static [&'static str]),
    /// A list of these words.
    Choices(&'static [&'static str]),
    /// An object holding these fields.
    Object,
    /// An object keyed by ids of this class (its values are the `.*` row).
    Map(Class),
    /// An object keyed by names of the modder's choosing (its values are
    /// the `.*` row).
    Named,
    /// A list of up to this many ids of this class.
    List(Class, usize),
    /// A list of partial entries of the same class (a weapon's modes), each
    /// checked against the class's rows.
    Variants,
    /// Four numbers 0 to 1: red, green, blue, strength.
    Tint,
    /// Two numbers in this range.
    Pair(f32, f32),
    /// Three numbers in this range.
    Pair3(f32, f32),
    /// A list of up to this many objects, each holding the `.*` rows.
    Objects(usize),
}

pub struct Field { pub path: &'static str, pub kind: Kind, pub doc: &'static str }

const fn f(path: &'static str, kind: Kind, doc: &'static str) -> Field { Field { path, kind, doc } }

/// A parts list's rows at `$p`.
macro_rules! parts { ($p:literal) => { &[
    f($p, Objects(24), "Its look built from parts (up to 24), drawn instead of its base look (an armor's: on its body as well)."),
    f(concat!($p, ".*"), Object, "One part."),
    f(concat!($p, ".*.mesh"), Choice(&["cube", "bevel", "sphere", "disc", "capsule", "dome"]), "Its shape."),
    f(concat!($p, ".*.at"), Pair3(-50.0, 50.0), "Where it is (m: right, up, back) in its owner's frame."),
    f(concat!($p, ".*.size"), Pair3(0.001, 50.0), "Its size (m) along each axis."),
    f(concat!($p, ".*.turn"), Pair3(-360.0, 360.0), "Its turn (degrees: pitch, yaw, roll)."),
    f(concat!($p, ".*.color"), Pair3(0.0, 1.0), "Its color (red, green, blue)."),
    f(concat!($p, ".*.glow"), Num(0.0, 4.0), "How much it glows."),
    f(concat!($p, ".*.team"), Bool, "Its team's color instead."),
    f(concat!($p, ".*.aim"), Key, "A vehicle's part: turns with this seat's rider's view, about the upright line through the seat (a turret)."),
    f(concat!($p, ".*.elevates"), Bool, "With `aim`: tilts up and down with the view as well, about `pivot` (a gun barrel)."),
    f(concat!($p, ".*.pivot"), Pair3(-50.0, 50.0), "With `elevates`: the point it tilts about (m: right, up, back)."),
] } }
const PARTS: &[Field] = parts!("parts");
const MOUNT_PARTS: &[Field] = parts!("mounts.*.parts");

use Kind::*;

const MOVEMENT: &[Field] = &[
    f("movement", Object, "The skier movement model's numbers. Predicted by every client, so only data."),
    f("movement.jet", Num(0.0, 200.0), "Jet acceleration (m/s², before gravity): all upward with no movement key held."),
    f("movement.side", Num(0.0, 200.0), "Horizontal jet acceleration along the movement keys, taken out of the upward share."),
    f("movement.drain", Num(0.0, 1000.0), "Energy per second while jetting."),
    f("movement.regen", Num(0.0, 200.0), "Energy per second regained otherwise."),
    f("movement.min_jet", Num(0.0, 100.0), "Energy needed to start or keep jetting."),
    f("movement.jump", Num(0.0, 5.0), "Jump impulse as a share of the standard jump."),
    f("movement.fade_up", Bool, "Upward thrust fades as climb speed nears the cap."),
    f("movement.air_cap", Num(0.0, 200.0), "Horizontal speed (m/s) up to which the jet adds speed."),
    f("movement.ski_drag", Num(0.0, 100.0), "Skiing slowdown (m/s²) on flat ground."),
];

const ARMOR: &[Field] = &[
    f("name", Text(32), "The armor's name at stations and on the HUD."),
    f("about", Text(200), "A line about it at stations."),
    f("offered", Bool, "Offered at stations by modes that don't list their armors (default true); a mode's `armors` list wins."),
    f("build", Choice(&["light", "medium", "heavy"]), "The body's size: its look and its armor model."),
    f("walk_scale", Num(0.1, 3.0), "Walking speed relative to the base light armor."),
    f("damage_taken", Num(0.05, 5.0), "Share of damage taken."),
    f("mass_scale", Num(0.1, 10.0), "Mass relative to the base light armor (blasts kick heavier bodies less)."),
    f("weapon_slots", Int(1, 6), "Weapons carried at once (the six slot keys)."),
    f("weapons", Map(Class::Weapon), "Weapons this armor may carry, by id."),
    f("weapons.*", Int(0, 10_000), "Full rounds for that weapon (0 for one paid with energy)."),
    f("kit", List(Class::Weapon, 8), "What it carries until its wearer chooses (bots too), slot 1 first."),
    f("grenade", Object, "What the grenade key throws."),
    f("grenade.throwable", Ref(Class::Throwable), "The throwable."),
    f("grenade.count", Int(0, 50), "How many are carried."),
    f("mine", Object, "What the mine key throws."),
    f("mine.throwable", Ref(Class::Throwable), "The throwable."),
    f("mine.count", Int(0, 50), "How many are carried."),
    f("resist", Map(Class::Damage), "Damage taken of each kind, by id, as a share."),
    f("resist.*", Num(0.0, 5.0), "The share of that kind taken (1: as usual)."),
    f("energy_max", Num(1.0, 1000.0), "Its energy tank (default: the world's movement.energy_max)."),
    f("team_cap", Int(0, 64), "Most of a team that may wear it at once (0: no limit)."),
    f("ability", Object, "The class key: what its wearer can do with it."),
    f("ability.name", Text(32), "Its name on the HUD."),
    f("ability.about", Text(200), "A line about it at stations."),
    f("ability.use", Choice(&["toggle", "use"]), "Switch it on and off, or use it for its action."),
    f("ability.effects", List(Class::Effect, 8), "Toggle: effects held on while it's on."),
    f("ability.drain", Num(0.0, 200.0), "Toggle: energy a second while it's on."),
    f("ability.min_energy", Num(0.0, 1000.0), "Toggle: energy it needs to stay on."),
    f("ability.energy", Num(0.0, 1000.0), "Use: energy each use costs."),
    f("ability.cooldown", Num(0.0, 600.0), "Use: seconds before it can be used again."),
    f("ability.charges", Int(0, 100), "Use: uses a life holds (0: no limit)."),
    f("ability.strain", Num(0.0, 100.0), "Strain each use (or each second it's on) builds."),
    f("ability.strain_decay", Num(0.0, 100.0), "Strain lost each second."),
    f("ability.strain_limit", Num(0.0, 1000.0), "At this strain or more, the strain effect is given (0: never)."),
    f("ability.strain_effect", Ref(Class::Effect), "What too much strain gives."),
    f("ability.action", Object, "Use: what it does (exactly one of fire, throw, place, recall, effect, impulse, armor)."),
    f("ability.action.fire", Ref(Class::Weapon), "Fire this weapon's projectile at the crosshair, from the shoulder."),
    f("ability.action.throw", Ref(Class::Throwable), "Throw this throwable."),
    f("ability.action.place", Ref(Class::Deployable), "Put this deployable down where the player aims."),
    f("ability.action.recall", Ref(Class::Deployable), "Put this deployable down where the player aims; used again, go to it."),
    f("ability.action.effect", Ref(Class::Effect), "Give its user this effect."),
    f("ability.action.impulse", Pair(0.0, 200.0), "A shove along the view, m/s forward and up (heavier bodies move less). Predicted."),
    f("ability.action.armor", Ref(Class::Armor), "Wear this armor for action.seconds, then go back."),
    f("ability.action.seconds", Num(0.0, 120.0), "How long action.armor lasts."),
    f("touch", Object, "What touching another player does, at most once a cooldown per pair."),
    f("touch.heal", Num(0.0, 100.0), "Health given to a teammate touched."),
    f("touch.energy", Num(0.0, 1000.0), "Energy given to a teammate touched."),
    f("touch.ally_effect", Ref(Class::Effect), "An effect given to a teammate touched."),
    f("touch.damage", Num(0.0, 100.0), "Damage to an enemy touched."),
    f("touch.damage_kind", Ref(Class::Damage), "Its kind (default impact)."),
    f("touch.steal_energy", Num(0.0, 1.0), "Share of an enemy's energy taken (and given to the toucher)."),
    f("touch.effect", Ref(Class::Effect), "An effect given to an enemy touched."),
    f("touch.cooldown", Num(0.0, 60.0), "Seconds before the same two touch again."),
    f("mounts", Named, "Things fixed to the body, by a name of your choosing (at most 4)."),
    f("mounts.*", Object, "One mount."),
    f("mounts.*.at", Choice(&["shoulder", "back", "front", "belt"]), "Where it sits."),
    f("mounts.*.weapon", Ref(Class::Weapon), "A weapon that fires by itself at the nearest enemy in sight."),
    f("mounts.*.range", Num(0.0, 300.0), "How far it reaches (m)."),
    f("mounts.*.cooldown", Num(0.05, 60.0), "Seconds between its shots (default: the weapon's reload)."),
    f("mounts.*.blocker", Object, "A plate stopping shots and blasts from the front."),
    f("mounts.*.blocker.arc", Num(0.0, 90.0), "Degrees either side of straight ahead it covers."),
    f("mounts.*.blocker.share", Num(0.0, 1.0), "Share of a blocked hit's damage that still gets through."),
    f("price", Int(0, 1_000_000), "Credits it costs where the mode has an economy."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
    f("scale", Num(0.5, 2.0), "Its body's size (looks only: collisions stay shared)."),
    f("tint", Pair3(0.0, 1.0), "Its body's color (red, green, blue)."),
    f("bot_roles", Choices(&["offense", "defense", "hunter"]), "The bot roles that wear it (none named by any armor: bots wear what they always have)."),
    f("ability.bot", Choice(&["combat", "defense", "always"]), "When bots use it."),
    f("packs", List(Class::Pack, 32), "The packs it may wear, by id (empty: any its station offers)."),
];

const WEAPON: &[Field] = &[
    f("name", Text(32), "The weapon's name at stations, on the HUD and in the kill feed."),
    f("about", Text(200), "A line about it at stations."),
    f("family", Key, "Versions of a weapon across armors share a family; favorites name families."),
    f("offered", Bool, "Offered at stations by modes that don't list their packs (default true); a mode's `packs` list wins."),
    f("projectile", Ref(Class::Projectile), "What it fires."),
    f("energy", Num(0.0, 1000.0), "Energy a shot costs; 0: a round instead."),
    f("reload", Num(0.016, 30.0), "Seconds between shots."),
    f("spread", Num(0.0, 100.0), "Random speed added to a shot (m/s)."),
    f("recoil", Num(0.0, 1.0), "View kick up per shot (radians)."),
    f("shake", Num(0.0, 1.0), "Screen shake per shot."),
    f("aim", Num(1.0, 5000.0), "Metres ahead a shot from the muzzle steers onto the crosshair (default 80)."),
    f("bot_spread", Num(0.0, 50.0), "Random speed (m/s) added to a bot's shot."),
    f("view", Choice(&["disc", "chaingun", "launcher", "mortar", "rifle", "railgun", "plasma", "blaster"]), "The view model held; it also places the muzzle."),
    f("muzzle", Pair3(-2.0, 2.0), "Where its model's barrel ends, if not at its view's muzzle: metres right, up and back in the view model's frame; its rounds, beams and flash leave there."),
    f("sound", Key, "The firing sound cue."),
    f("zoom", Bool, "The zoom key zooms while it's held (the wheel picks one of two levels)."),
    f("elevation", Bool, "Its sight shows an elevation gauge: the aim's angle above the horizon, for lobbing shells."),
    f("empties_energy", Bool, "Each shot also empties its holder's energy, so they can't jet right after one."),
    f("loot", Choice(&["none", "roll", "all"]), "What a dying player's ammo pack holds of its rounds: none, 1 to 5, or all."),
    f("lost_on_death", Bool, "Lost when its carrier dies (back at respawn if it's in their setup)."),
    f("fire", Choice(&["single", "spin", "charge", "burst", "sustained"]), "How the trigger works (default single)."),
    f("spin_up", Num(0.0, 10.0), "Spin: seconds the barrel takes to spin up before it fires."),
    f("spin_down", Num(0.0, 10.0), "Spin: seconds it takes to spin down once released."),
    f("charge", Num(0.0, 10.0), "Charge: seconds held to reach full power."),
    f("charge_scale", Num(0.1, 10.0), "Charge: damage and speed at full power, as a multiple (default 1)."),
    f("overload", Num(0.0, 10.0), "Charge: seconds held past full before it hurts the shooter (0: never)."),
    f("overload_damage", Num(0.0, 1000.0), "Charge: what an overload does to the shooter."),
    f("burst", Int(0, 20), "Burst: shots per trigger pull."),
    f("burst_gap", Num(0.0, 5.0), "Burst: seconds between the shots of a burst."),
    f("lock", Num(0.0, 10.0), "Seconds a target must stay in the crosshair to lock on (0: no lock)."),
    f("lock_cone", Num(0.0, 90.0), "Degrees from the crosshair a target may be to lock on."),
    f("lock_range", Num(0.0, 5000.0), "Metres away a target may be to lock on."),
    f("remote", Bool, "Firing again sets off this weapon's rounds still out."),
    f("modes", Variants, "Other ways to fire it (the fire-mode key cycles them): each changes some of these fields."),
    f("price", Int(0, 1_000_000), "Credits it costs where the mode has an economy."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
    f("bot_range", Pair(0.0, 5000.0), "The distances (m) bots fire it at, nearest and farthest."),
];

const PROJECTILE: &[Field] = &[
    f("class", Choice(&["straight", "shell", "thrown", "beam", "stream"]), "How it moves and what contact does (stream: a held beam)."),
    f("speed", Num(0.0, 5000.0), "Muzzle speed (m/s)."),
    f("inherit", Num(0.0, 2.0), "Share of the shooter's velocity kept."),
    f("life", Num(0.01, 1000.0), "Seconds it lasts (a grenade's fuse, a beam's glow)."),
    f("gravity", Num(0.0, 5.0), "Share of gravity it falls with."),
    f("radius", Num(0.0, 8.0), "Collision radius against the map (default 0.12)."),
    f("reach", Num(0.0, 5.0), "Extra reach against equipment."),
    f("water", Choice(&["drag", "fizzle", "burst"]), "In water: slowed hard, put out, or burst where it meets the water at half damage and twice the push."),
    f("expire", Choice(&["explode", "vanish"]), "When its life runs out: bursts, or is gone."),
    f("blast", Ref(Class::Blast), "Its burst against bodies."),
    f("structure_blast", Ref(Class::Blast), "Its burst against equipment and deployables (default: blast)."),
    f("damage", Num(0.0, 10_000.0), "A hit's damage when it doesn't burst; a beam's damage."),
    f("burn", Num(0.0, 2000.0), "Damage a second to a body it overlaps in flight. It keeps flying (it still bursts on the ground)."),
    f("flash", Num(0.0, 1.0), "How hard a hit that doesn't burst flashes the victim's screen (default 0.5)."),
    f("self_damage", Num(0.0, 2.0), "Share of its own blast the shooter takes (default 1)."),
    f("arm", Num(0.0, 60.0), "Shells: seconds of flight before contact sets it off."),
    f("elasticity", Num(0.0, 1.0), "Bounces: share of the speed into a surface kept."),
    f("slide", Num(0.0, 1.0), "Thrown: share of the speed along a surface kept."),
    f("keep", Num(0.0, 1.0), "Shells: speed kept after a bounce. Straight: glances off the map keeping this share."),
    f("rest_speed", Num(0.0, 50.0), "Thrown: under this speed it comes to rest on a floor."),
    f("damage_kind", Ref(Class::Damage), "What its hits and burst are made of (default blast)."),
    f("smoke", Bool, "Trails smoke."),
    f("clears_mines", Bool, "Sets off resting mines along its path."),
    f("range", Num(0.0, 5000.0), "Beams: how far it reaches."),
    f("trigger", Object, "Thrown ones that lie in wait (mines)."),
    f("trigger.rest_life", Num(0.1, 3600.0), "Seconds it lies at rest."),
    f("trigger.arm", Num(0.0, 60.0), "Seconds at rest before it arms."),
    f("trigger.radius", Num(0.0, 50.0), "An enemy this near sets it off."),
    f("trigger.sympathy", Num(0.0, 50.0), "A blast this near sets it off too."),
    f("trigger.shot_radius", Num(0.0, 5.0), "A shot passing this close hits it."),
    f("accel", Num(0.0, 1000.0), "Speeding up along its flight (m/s²)."),
    f("max_speed", Num(0.0, 5000.0), "The speed it speeds up to (0: no limit)."),
    f("seek", Object, "Homing on a target."),
    f("seek.turn", Num(0.0, 1080.0), "Degrees a second it turns toward its target."),
    f("seek.cone", Num(0.0, 180.0), "Degrees from the aim a target may be, chosen at launch."),
    f("seek.range", Num(0.0, 5000.0), "Metres away a target may be, chosen at launch."),
    f("seek.targets", Choices(&["players", "jetting", "marked"]), "What it homes on: players, players jetting, or what teammates marked (default players)."),
    f("seek.proximity", Num(0.0, 50.0), "It bursts this near its target (0: only on contact)."),
    f("guided", Num(0.0, 1080.0), "Degrees a second it turns to follow the shooter's aim (0: not guided)."),
    f("pellets", Int(1, 32), "Fired as this many pellets (default 1)."),
    f("cone", Num(0.0, 45.0), "Degrees the pellets spread over."),
    f("marks", Num(0.0, 120.0), "Beams and streams: seconds what it hits stays marked (shown) for the shooter's team; a stream keeps one mark, on what it touches while held."),
    f("stream", Object, "Streams: what the held beam does each second."),
    f("stream.range", Num(0.0, 1000.0), "How far it reaches (m)."),
    f("stream.damage", Num(0.0, 1000.0), "Damage a second to enemies."),
    f("stream.drain", Num(0.0, 1000.0), "Energy a second drained from enemies."),
    f("stream.heal", Num(0.0, 1000.0), "Health a second restored to teammates."),
    f("stream.repair", Num(0.0, 1000.0), "Hull a second restored to the team's equipment and deployables."),
    f("stream.claim", Num(0.0, 120.0), "Seconds of beam that turn an enemy deployable to the shooter's team (0: it can't)."),
    f("kill", Text(32), "The kill feed's word for a kill by it."),
    f("kill_turret", Text(32), "The kill feed's word when a turret fired it (default: kill)."),
    f("look", Choice(&["disc", "tracer", "grenade", "turret_plasma", "mortar", "hand_grenade", "mine", "laser", "rail", "plasma", "blaster"]),
        "How it and its burst are drawn: a base projectile's look (default by class)."),
    f("effect", Ref(Class::Effect), "An effect for whoever it hits, catches in its blast, or holds in its stream."),
    f("pull", Num(-200.0, 200.0), "A direct hit drags its victim toward the shooter at this speed (m/s; negative: away, and up by 0.4 of it)."),
    f("grapple", Num(0.0, 200.0), "Any hit drags the shooter toward it at this speed (m/s)."),
    f("becomes", Ref(Class::Deployable), "Come to rest (thrown), it becomes this deployable."),
    f("drops", Ref(Class::Item), "Come to rest (thrown), it becomes this item."),
    f("model", Key, "Its authored model (models/projectiles/<model>/), drawn in flight in place of its look."),
    f("size", Num(0.25, 8.0), "Its look's glow drawn this many times as big (tracers, bolts, beams; default 1)."),
    f("color", Pair3(0.0, 1.0), "Its look's glow color, or its model's paint all over (red, green, blue)."),
    f("trail", Object, "A trail behind it in flight, with its look or its model (looks only: it changes nothing in play)."),
    f("trail.style", Choice(&["ribbon", "smoke"]), "A glowing ribbon that follows it, or smoke left along its path that spreads and fades."),
    f("trail.color", Pair3(0.0, 1.0), "Its color (red, green, blue)."),
    f("trail.width", Num(0.0, 5.0), "The ribbon's width, or a fresh puff's size (m; 0: ribbon 0.25, smoke 0.2)."),
    f("trail.life", Num(0.0, 10.0), "Seconds of flight the ribbon spans, or a puff lingers (0: ribbon 0.14, smoke 1.4)."),
];

const BLAST: &[Field] = &[
    f("falloff", Choice(&["linear", "quadratic"]), "The damage curve (linear, or concentrated with core)."),
    f("radius", Num(0.0, 200.0), "Reach (m)."),
    f("max_damage", Num(0.0, 10_000.0), "Damage at the centre (a direct hit for quadratic)."),
    f("edge_fraction", Num(0.0, 1.0), "Linear: share of max_damage left at the radius."),
    f("impulse", Num(0.0, 100_000.0), "Kick, divided by the body's mass."),
    f("core", Num(0.0, 200.0), "Above 0: concentrated, half damage at core metres."),
    f("edge_damage", Num(0.0, 10_000.0), "Quadratic: damage term rising to the centre."),
    f("core_damage", Num(0.0, 10_000.0), "Quadratic: damage term rising fast near the centre."),
    f("kick", Num(0.0, 100.0), "Quadratic: kick speed (m/s) at the centre."),
];

const THROWABLE: &[Field] = &[
    f("name", Text(32), "Its name on the HUD and at stations."),
    f("about", Text(200), "A line about it."),
    f("projectile", Ref(Class::Projectile), "What it becomes once thrown."),
    f("speed", Num(0.0, 200.0), "Full-strength throw speed (m/s)."),
    f("inherit", Num(0.0, 2.0), "Share of the thrower's velocity kept (default 1)."),
    f("time", Num(0.0, 10.0), "Seconds before the next throw."),
    f("min_strength", Num(0.0, 1.0), "The weakest throw's share of full."),
    f("team_max", Int(0, 500), "Most a team may have out (0: no limit)."),
    f("match_limit", Int(0, 10_000), "Most a team may throw in a match (0: no limit)."),
];

const DEPLOYABLE: &[Field] = &[
    f("name", Text(32), "Its name at stations and on the HUD."),
    f("about", Text(200), "A line about it at stations."),
    f("behavior", Choice(&["turret", "wall", "field", "ammo", "inventory", "post", "pad", "gate", "command"]),
        "Its body: a turret's post and gun, a plain post, a solid wall, a field its team passes, an ammo crate, an inventory kiosk, a low pad to stand on, a gate nobody bumps into, or a command kiosk."),
    f("hull", Num(1.0, 100_000.0), "Its health."),
    f("team_limit", Int(0, 200), "Most a team may have standing."),
    f("per_teammate", Num(0.0, 10.0), "More a team may have standing per player on it."),
    f("per_player", Int(0, 50), "Most one player may have standing (0: no limit)."),
    f("match_limit", Int(0, 10_000), "Most a team may place in a match (0: no limit)."),
    f("turret", Object, "A turret's targeting and fire."),
    f("turret.range", Num(0.0, 2000.0), "Detection range (m)."),
    f("turret.sensed_range", Num(0.0, 2000.0), "Detection range while a team sensor is powered."),
    f("turret.speed", Num(0.0, 5000.0), "Its rounds' speed (m/s)."),
    f("turret.life", Num(0.01, 60.0), "Its rounds' life (s)."),
    f("turret.cooldown", Num(0.016, 60.0), "Seconds between shots."),
    f("turret.leads", Bool, "Aims where a target will be."),
    f("turret.fires", Bool, "Fires (false: only detects)."),
    f("turret.projectile", Ref(Class::Projectile), "What it fires."),
    f("turret.jetting_only", Bool, "Engages only enemies using their jets (a homing round follows the one it picked)."),
    f("size", Pair3(0.05, 20.0), "Half its width, height and depth (m) for a box-shaped body (default: its behavior's)."),
    f("look", Choice(&["turret", "wall", "field", "ammo", "inventory", "pylon", "sensor", "camera", "pad", "gate", "command", "beacon"]),
        "The base look it is drawn with (default: its behavior's)."),
    f("pad", Object, "Whoever stands on it is thrown. Predicted."),
    f("pad.forward", Num(0.0, 200.0), "Speed added along `along` (m/s; heavier bodies less)."),
    f("pad.up", Num(0.0, 200.0), "Upward speed it gives (m/s; heavier bodies less)."),
    f("pad.along", Choice(&["view", "facing", "up"]), "Forward along the player's view, the pad's facing, or none."),
    f("teleport", Object, "A team's units of this kind link in pairs, in placement order; step into one, come out at the other."),
    f("teleport.cooldown", Num(0.0, 60.0), "Seconds before the same player can go through again."),
    f("teleport.carrier", Bool, "A flag carrier may go through."),
    f("aura", Object, "An effect held on players within range."),
    f("aura.effect", Ref(Class::Effect), "The effect."),
    f("aura.range", Num(0.0, 200.0), "How far it reaches (m)."),
    f("aura.targets", Choice(&["team", "enemies", "all"]), "Whom it touches."),
    f("sensor", Object, "Senses enemies for its team: turrets reach their sensed range, and the team sees them marked."),
    f("sensor.range", Num(0.0, 1000.0), "How far it senses (m)."),
    f("sensor.moving", Bool, "Senses only those moving."),
    f("sensor.alarm", Bool, "Sounds an alarm for its team when an enemy comes in."),
    f("camera", Bool, "Its team can look through it from a command station."),
    f("shock", Object, "What touching it does to an enemy."),
    f("shock.damage", Num(0.0, 200.0), "Damage."),
    f("shock.damage_kind", Ref(Class::Damage), "Its kind (default shock)."),
    f("shock.effect", Ref(Class::Effect), "An effect given."),
    f("shock.cooldown", Num(0.05, 60.0), "Seconds before it shocks the same player again."),
    f("vehicles", List(Class::Vehicle, 8), "A vehicle station: the vehicles bought standing on it."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
    f("life", Num(0.0, 3600.0), "Seconds it stands before it's gone by itself (0: until destroyed)."),
];

const VEHICLE: &[Field] = &[
    f("name", Text(32), "Its name."),
    f("about", Text(200), "A line about it at stations."),
    f("hull", Num(1.0, 100_000.0), "Its health."),
    f("energy_max", Num(1.0, 10_000.0), "Its energy tank."),
    f("regen", Num(0.0, 1000.0), "Energy a second regained."),
    f("drive", Choice(&["fly", "ground"]), "How it moves: `fly` (default), or `ground`: kept to the ground, turned by the sideways keys."),
    f("flight", Object, "How it moves. Predicted for the pilot."),
    f("flight.thrust", Num(0.0, 500.0), "Acceleration along its nose (m/s²)."),
    f("flight.reverse", Num(0.0, 500.0), "Acceleration backward (m/s²)."),
    f("flight.strafe", Num(0.0, 500.0), "A flier's acceleration sideways (m/s²)."),
    f("flight.lift", Num(0.0, 500.0), "A flier's acceleration up with the jet key (m/s²)."),
    f("flight.hover", Num(0.0, 1.0), "A flier's share of gravity it cancels while piloted."),
    f("flight.drag", Num(0.0, 20.0), "Speed lost a second, as a share."),
    f("flight.top_speed", Num(1.0, 1000.0), "Its top speed (m/s)."),
    f("flight.turn", Num(1.0, 1000.0), "Degrees a second it turns: a flier toward where its pilot looks, a ground vehicle with the sideways keys."),
    f("flight.drain", Num(0.0, 1000.0), "Energy a second while thrusting."),
    f("flight.climb", Num(0.0, 89.0), "Ground: the steepest slope it drives up (degrees, default 35)."),
    f("flight.grip", Num(0.0, 100.0), "Ground: how hard its tracks hold, sideways always and every way while it isn't driven (per second, default 8)."),
    f("size", Pair3(0.2, 20.0), "Half its width, height and length (m)."),
    f("mass", Num(0.1, 100.0), "Mass relative to a light-armored player."),
    f("seats", Named, "Its seats by name (at most 6): `pilot` flies it; the rest ride, in name order."),
    f("seats.*", Object, "One seat."),
    f("seats.*.at", Pair3(-20.0, 20.0), "Where it is (right, up, forward m)."),
    f("seats.*.weapon", Ref(Class::Weapon), "The weapon fired from it along its rider's view."),
    f("seats.*.exposed", Bool, "Its rider sits in the open: seen, and hit by shots and blasts (a bike's)."),
    f("blast", Ref(Class::Blast), "The blast it bursts in when destroyed."),
    f("ram", Num(0.0, 1000.0), "Damage it deals for each m/s past 10 that it runs into an enemy on foot or another vehicle at."),
    f("carries_flag", Bool, "A flag carrier may ride it."),
    f("team_limit", Int(0, 50), "Most a team may have at once (0: no limit)."),
    f("price", Int(0, 1_000_000), "Credits it costs where the mode has an economy."),
    f("abandon", Num(0.0, 3600.0), "Seconds it stays with nobody aboard before it's gone (0: until destroyed)."),
    f("look", Choice(&["skiff", "transport"]), "How it looks."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
];

const DAMAGE: &[Field] = &[
    f("name", Text(32), "The kind's name."),
    f("head", Num(0.0, 10.0), "Direct hits in the head: damage times this (default 1)."),
    f("back", Num(0.0, 10.0), "Direct hits in the back: damage times this (default 1)."),
    f("knock_flag", Num(0.0, 10_000.0), "A direct hit from behind this hard knocks the flag off its carrier (0: never)."),
    f("drop_pack", Num(0.0, 10_000.0), "A direct hit this hard drops the victim's pack (0: never)."),
    f("disarm", Num(0.0, 10_000.0), "A direct hit this hard knocks the weapon out of the victim's hand (0: never)."),
];

const ACTION: &[Field] = &[
    f("action", Object, "What using it does (exactly one of fire, throw, place, recall, effect, impulse, armor)."),
    f("action.fire", Ref(Class::Weapon), "Fire this weapon's projectile at the crosshair."),
    f("action.throw", Ref(Class::Throwable), "Throw this throwable."),
    f("action.place", Ref(Class::Deployable), "Put this deployable down where the player aims."),
    f("action.recall", Ref(Class::Deployable), "Put this deployable down where the player aims; used again, go to it (it's used up; a flag carrier drops the flag)."),
    f("action.effect", Ref(Class::Effect), "Give its user this effect."),
    f("action.impulse", Pair(0.0, 200.0), "A shove along the view, m/s forward and up (heavier bodies move less). Predicted."),
    f("action.armor", Ref(Class::Armor), "Wear this armor for action.seconds, then go back."),
    f("action.seconds", Num(0.0, 120.0), "How long action.armor lasts."),
];

const PACK: &[Field] = &[
    f("name", Text(32), "Its name at stations and on the HUD."),
    f("about", Text(200), "A line about it at stations."),
    f("offered", Bool, "Offered at stations (default true)."),
    f("base_station_only", Bool, "Only a base inventory station hands it out."),
    f("use", Choice(&["place", "always", "toggle", "use"]), "What the deploy key does: place its deployable, nothing (it always works), switch it on and off, or use it."),
    f("deployable", Ref(Class::Deployable), "Place: what it puts down."),
    f("effects", List(Class::Effect, 8), "Effects held on while it works."),
    f("drain", Num(0.0, 200.0), "Energy a second while it works."),
    f("drain_regen", Num(0.0, 10.0), "Energy a second while it works, as a multiple of the wearer's recharge (`movement.regen`), on top of `drain`."),
    f("min_energy", Num(0.0, 1000.0), "Energy it needs to keep working."),
    f("energy", Num(0.0, 1000.0), "Use: energy each use costs."),
    f("cooldown", Num(0.0, 600.0), "Use: seconds before it can be used again."),
    f("charges", Int(0, 100), "Use: uses it holds (0: no limit)."),
    f("ammo_scale", Num(0.0, 5.0), "The wearer carries this multiple of their rounds (default 1)."),
    f("price", Int(0, 1_000_000), "Credits it costs where the mode has an economy."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
    f("bot", Choice(&["combat", "defense", "always"]), "When bots use it (a pack that places: defenders put it down near their flag)."),
];

const ITEM: &[Field] = &[
    f("name", Text(32), "Its name."),
    f("about", Text(200), "A line about it."),
    f("health", Num(0.0, 100.0), "Health it gives."),
    f("energy", Num(0.0, 1000.0), "Energy it gives."),
    f("rounds", Num(0.0, 1.0), "Rounds it gives each weapon carried, as a share of full."),
    f("grenades", Int(0, 50), "Grenades it gives."),
    f("mines", Int(0, 50), "Mines it gives."),
    f("effect", Ref(Class::Effect), "An effect it gives."),
    f("respawn", Num(1.0, 600.0), "Seconds before one a map places comes back once taken."),
    f("look", Choice(&["kit", "box", "cell"]), "How it looks."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
];

const EFFECT: &[Field] = &[
    f("name", Text(32), "Its name on the HUD."),
    f("about", Text(200), "A line about it."),
    f("seconds", Num(0.0, 600.0), "How long it lasts once given (a pack or ability holding it on renews it)."),
    f("damage", Num(0.0, 1000.0), "Damage a second (of damage_kind)."),
    f("damage_kind", Ref(Class::Damage), "What that damage is made of (default blast)."),
    f("heal", Num(0.0, 1000.0), "Health restored a second."),
    f("energy", Num(-1000.0, 1000.0), "Energy gained a second (negative: drained)."),
    f("regen_scale", Num(0.0, 10.0), "Energy regained, as a multiple of the armor's (default 1)."),
    f("movement", Object, "Movement as multiples (default 1). Predicted by every client."),
    f("movement.jet", Num(0.0, 5.0), "Jet thrust."),
    f("movement.side", Num(0.0, 5.0), "Sideways jet thrust."),
    f("movement.air_cap", Num(0.0, 5.0), "The speed the jet adds speed up to."),
    f("movement.walk", Num(0.0, 5.0), "Walking speed."),
    f("movement.jump", Num(0.0, 5.0), "The jump."),
    f("absorb", Num(0.0, 100.0), "A shield: energy spent per point of damage taken instead, counted before armor (0: none)."),
    f("absorb_kinds", List(Class::Damage, 16), "The damage kinds the shield takes (empty: all)."),
    f("visibility", Num(0.0, 1.0), "How plainly enemies see its bearer (1: plainly; 0: not at all)."),
    f("hide_name", Bool, "No name tag for enemies."),
    f("sensor_hidden", Bool, "Sensors, turrets and homing rounds don't see its bearer."),
    f("shimmer", Num(0.0, 200.0), "Enemies this near see a shimmer anyway."),
    f("reveal", Bool, "Firing or being hit ends it."),
    f("blocks", Object, "What it stops while it lasts."),
    f("blocks.fire", Bool, "Firing."),
    f("blocks.jet", Bool, "Jetting."),
    f("blocks.flag", Bool, "Taking a flag."),
    f("stack", Choice(&["refresh", "extend", "keep"]), "Given again while it lasts: start over, add the time, or ignore."),
    f("cleansed", Bool, "An inventory or ammo station ends it."),
    f("immune", List(Class::Armor, 32), "Armors it never takes hold on."),
    f("tint", Tint, "The screen tint its bearer sees: red, green, blue, strength (0 to 1)."),
    f("hook", Key, "The hook (hooks/<name>.rhai, server only) that hears its events."),
    f("glow", Tint, "A glow round its bearer's body: color and strength."),
];

/// The rows for a class.
pub fn table(class: Class) -> Vec<&'static Field> {
    match class {
        Class::Armor => MOVEMENT.iter().chain(ARMOR).chain(PARTS).chain(MOUNT_PARTS).collect(),
        Class::Weapon => WEAPON.iter().chain(PARTS).collect(),
        Class::Projectile => PROJECTILE.iter().chain(PARTS).collect(),
        Class::Blast => BLAST.iter().collect(),
        Class::Throwable => THROWABLE.iter().collect(),
        Class::Deployable => DEPLOYABLE.iter().chain(PARTS).collect(),
        Class::Damage => DAMAGE.iter().collect(),
        Class::Effect => EFFECT.iter().collect(),
        Class::Pack => PACK.iter().chain(ACTION).chain(PARTS).collect(),
        Class::Item => ITEM.iter().chain(PARTS).collect(),
        Class::Vehicle => VEHICLE.iter().chain(PARTS).collect(),
    }
}

/// A short name: 1 to 32 of a-z, 0-9, `_` and `-`.
pub fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}

/// Check one entry's values against its class's table: `at` names it in
/// errors ("weapons/disc"). Shapes were checked by serde already.
pub fn check(class: Class, value: &Value, at: &str) -> Result<(), String> {
    let rows = table(class);
    walk(&rows, value, "", at)
}

fn walk(rows: &[&Field], value: &Value, prefix: &str, at: &str) -> Result<(), String> {
    let Value::Object(map) = value else { return Ok(()) };
    let parent = rows.iter().find(|r| r.path == prefix.trim_end_matches('.')).map(|r| r.kind);
    for (k, v) in map {
        let path = if matches!(parent, Some(Map(_) | Named)) { format!("{prefix}*") } else { format!("{prefix}{k}") };
        let shown = format!("{prefix}{k}");
        let Some(row) = rows.iter().find(|r| r.path == path) else {
            return Err(format!("{at}: unknown field {shown}"));
        };
        let bad = |what: String| Err(format!("{at}: {shown} {what}"));
        match (row.kind, v) {
            (Num(lo, hi), Value::Number(n)) => {
                let x = n.as_f64().unwrap_or(f64::NAN);
                if !x.is_finite() || x < lo as f64 - 1e-9 || x > hi as f64 + 1e-9 { return bad(format!("must be {lo} to {hi}")); }
            }
            (Int(lo, hi), Value::Number(n)) => {
                if !n.as_i64().is_some_and(|x| (lo..=hi).contains(&x)) { return bad(format!("must be a whole number {lo} to {hi}")); }
            }
            (Text(max), Value::String(s)) => {
                if s.chars().count() > max || s.chars().any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) {
                    return bad(format!("must be plain text up to {max} characters"));
                }
            }
            (Key, Value::String(s)) | (Ref(_), Value::String(s)) => {
                if !valid_id(s) { return bad("must be 1 to 32 of a-z, 0-9, _ and -".into()); }
            }
            (Map(_) | Named, Value::Object(m)) => {
                if let Some(bad_key) = m.keys().find(|k| !valid_id(k)) { return Err(format!("{at}: {shown}: {bad_key:?} isn't an id")); }
                walk(rows, v, &format!("{shown}."), at)?;
            }
            (Object, Value::Object(_)) => walk(rows, v, &format!("{path}."), at)?,
            (Variants, Value::Array(a)) => {
                if a.len() > 8 { return bad("holds at most 8".into()); }
                for (n, v) in a.iter().enumerate() {
                    if !v.is_object() { return bad("holds objects".into()); }
                    walk(rows, v, "", &format!("{at} {shown}[{n}]"))?;
                }
            }
            (Pair3(lo, hi), Value::Array(a)) => {
                if a.len() != 3 || !a.iter().all(|x| x.as_f64().is_some_and(|x| x >= lo as f64 && x <= hi as f64)) {
                    return bad(format!("must be three numbers {lo} to {hi}"));
                }
            }
            (Pair(lo, hi), Value::Array(a)) => {
                if a.len() != 2 || !a.iter().all(|x| x.as_f64().is_some_and(|x| x >= lo as f64 && x <= hi as f64)) {
                    return bad(format!("must be two numbers {lo} to {hi}"));
                }
            }
            (Tint, Value::Array(a)) => {
                if a.len() != 4 || !a.iter().all(|x| x.as_f64().is_some_and(|x| (0.0..=1.0).contains(&x))) { return bad("must be four numbers 0 to 1".into()); }
            }
            (Objects(max), Value::Array(a)) => {
                if a.len() > max { return bad(format!("holds at most {max}")); }
                for v in a {
                    if !v.is_object() { return bad("holds objects".into()); }
                    walk(rows, v, &format!("{path}.*."), at)?;
                }
            }
            (List(_, max), Value::Array(a)) => {
                if a.len() > max { return bad(format!("holds at most {max}")); }
                if let Some(x) = a.iter().find(|x| !x.as_str().is_some_and(valid_id)) { return bad(format!("{x} isn't an id")); }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The reference: every class's fields, as Markdown.
pub fn markdown() -> String {
    let mut s = String::new();
    for class in Class::ALL {
        s.push_str(&format!("## {} (`{}/`)\n\n| Field | Values | About |\n|---|---|---|\n", class.title(), class.folder()));
        for r in table(class) {
            let values = match r.kind {
                Num(lo, hi) => format!("{lo} to {hi}"),
                Int(lo, hi) => format!("whole number {lo} to {hi}"),
                Bool => "true or false".into(),
                Text(n) => format!("text, up to {n}"),
                Key => "a name: a-z, 0-9, _ and -".into(),
                Ref(c) => format!("a {} id", c.title().to_lowercase()),
                Choice(w) => w.join(", "),
                Choices(w) => format!("a list of: {}", w.join(", ")),
                Object => "an object".into(),
                Map(c) => format!("an object keyed by {} ids", c.title().to_lowercase()),
                Named => "an object keyed by names (a-z, 0-9, _ and -)".into(),
                List(c, n) => format!("a list of up to {n} {} ids", c.title().to_lowercase()),
                Variants => "a list of up to 8 objects of these fields".into(),
                Tint => "four numbers 0 to 1".into(),
                Pair(lo, hi) => format!("two numbers {lo} to {hi}"),
                Pair3(lo, hi) => format!("three numbers {lo} to {hi}"),
                Objects(n) => format!("a list of up to {n} objects of these fields"),
            };
            s.push_str(&format!("| `{}` | {} | {} |\n", r.path, values, r.doc));
        }
        s.push('\n');
    }
    s
}
