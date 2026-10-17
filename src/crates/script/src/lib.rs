//! Rules scripts (docs/modding.md §7, P6): what scores, what wins and what
//! is announced in a mode, written in Rhai and run by servers only. A mode
//! names one (`"script": "spree"` for `rules/spree.rhai` in its mod); the
//! engine reports its events and applies the commands the script gives, as
//! it does for the built-in rules (`rules::RuleSet`). Players' games never
//! link this crate, and scripts are never sent to them.
//!
//! A script defines any of these handlers (a missing one does nothing):
//!
//! ```text
//! fn on_flag_captured(game, player)        // the engine has returned the flag
//! fn on_touchdown(game, player)            // the engine has called the ball dead
//! fn on_carrier_tackled(game, winner, loser)
//! fn on_frag(game, killer, victim)         // killer == victim: a suicide
//! fn on_died(game, victim, killer)         // every death that counts (none in a blade zone or a
//!                                          // duel lobby), after any on_frag; killer -1: none
//! fn on_rabbit_grab(game, player)          // took the rabbit flag
//! fn on_rabbit_held(game, player)          // held it 15 seconds more in all
//! fn on_zone_points(game, gained)          // gained: [ember, glacier] this step
//! fn on_clock_expired(game)                // unanswered, the engine ends the match
//! ```
//!
//! `game` reads the world (`register_game`); `this` is a map that lasts the
//! round, for the script's own state. Commands: `add_score(team, points)`,
//! `set_score(team, points)`, `add_frags(player, n)`, `end_match(text,
//! seconds)`, `message(text, seconds)`, `sound(name)`, `feed(text)`,
//! `shake(amount)`; each checks its arguments.
//!
//! Hooks (`hooks/<name>.rhai`, named by definitions, trigger zones and
//! modes; `sim::hooks`) define any of these, each given `game` and an event
//! record `ev` (a map), except `on_timer(game, name)`:
//!
//! ```text
//! on_spawn      ev.player
//! on_death      ev.victim, ev.killer (-1: none), ev.cause (deaths that count)
//! on_damage     ev.victim, ev.attacker (-1: none), ev.amount, ev.kind
//! on_fire       ev.player, ev.weapon
//! on_use        ev.player, ev.what (a pack's id, or "ability")
//! on_deploy     ev.player, ev.deployable, ev.at ([x, y, z])
//! on_destroyed  ev.deployable, ev.team, ev.at
//! on_touch      ev.toucher, ev.touched
//! on_enter      ev.player, ev.zone      on_leave  (the same)
//! on_pickup     ev.player, ev.item
//! on_buy        ev.player, ev.what (a purchase code)
//! on_effect     ev.player, ev.effect
//! ```
//!
//! Hooks read more of the game: `alive(i)`, `health(i)`, `energy(i)`,
//! `armor(i)` (an id), `pos(i)` ([x, y, z]), `carrying(i)`, `riding(i)`,
//! `credits(i)`, `distance(a, b)`, `distance_to(i, [x, y, z])`,
//! `flag_home(team)` and `flag_at(team)` ([x, y, z]), `has_effect(i, id)`, `steps` (the server's
//! tick) and `random(n)` (0 to n - 1, from a stream the server seeds). They
//! command the rules' commands and `damage(player, amount, kind[,
//! attacker])`, `heal(player, amount)`, `give_effect(player, id)`,
//! `clear_effect(player, id)`, `impulse(player, [x, y, z])`,
//! `teleport(player, [x, y, z])`, `set_armor(player, id)`,
//! `give_rounds(player, weapon, n)`, `set_energy(player, n)`,
//! `credits(player, n)`, `team_feed(team, text)`, `waypoint(team, [x, y,
//! z])`, `place(team, deployable, [x, y, z], yaw_degrees)`,
//! `projectile(owner, id, [x, y, z], [vx, vy, vz])` (owner -1: the
//! world's) and `after(seconds, name)` (an `on_timer` later).
//!
//! The sandbox: no modules or files, no clock, no `eval`; each call is
//! limited in operations, call depth and sizes, and hooks share a time
//! budget each step. A call that fails or runs over its limits has all its
//! commands dropped, and the server logs why.
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::hooks::{Hook, HookEvent};
use peakrunner_core::sim::rules::{Cmd, GameEvent, RuleSet};
use peakrunner_core::sim::{football, MatchState, Team, World};
use peakrunner_core::glam::Vec3;
use rhai::{Array, CallFnOptions, Dynamic, Engine, EvalAltResult, Scope, AST};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Fail = Box<EvalAltResult>;

/// Most operations one handler call may take.
pub const MAX_OPERATIONS: u64 = 100_000;

/// Cues a script may play: announcements, not weapons or footsteps.
const SOUNDS: [&str; 16] = ["capture_win", "capture_loss", "touchdown_win", "touchdown_loss", "point", "contest", "whistle", "horn",
    "halftime", "go", "flag", "return", "tackle", "catch", "intercept", "end"];

/// The base game's rule sets as scripts, in `modes::BASE` order; the Rust
/// ones (`rules::builtin`) are their reference (`tests::ab_*`).
pub const BASE: [(&str, &str); 7] = [
    ("ctf", include_str!("../../../assets/mods/base/rules/ctf.rhai")),
    ("capture_and_hold", include_str!("../../../assets/mods/base/rules/capture_and_hold.rhai")),
    ("football", include_str!("../../../assets/mods/base/rules/football.rhai")),
    ("deathmatch", include_str!("../../../assets/mods/base/rules/deathmatch.rhai")),
    ("team_deathmatch", include_str!("../../../assets/mods/base/rules/team_deathmatch.rhai")),
    ("rabbit", include_str!("../../../assets/mods/base/rules/rabbit.rhai")),
    ("team_rabbit", include_str!("../../../assets/mods/base/rules/team_rabbit.rhai")),
];

/// Let scripted modes and hooks use scripts (the server, at startup).
pub fn install() {
    peakrunner_core::sim::rules::set_factory(factory);
    peakrunner_core::sim::hooks::set_hook_factory(hook_factory);
}

fn hook_factory(name: &str) -> Option<Box<dyn Hook + Send>> {
    let Some((id, source)) = peakrunner_core::mods::hook_script(name) else {
        log::error!("hook {name}: no hooks/{name}.rhai in the mods in force; its events go unanswered");
        return None;
    };
    match ScriptHook::new(&format!("{id}/hooks/{name}.rhai"), &source) {
        Ok(h) => Some(Box::new(h)),
        Err(e) => { log::error!("hook {name}: {e}; its events go unanswered"); None }
    }
}

fn factory(mode: SupportedMode) -> Option<Box<dyn RuleSet + Send>> {
    let name = mode.def().script.clone()?;
    let Some((id, source)) = peakrunner_core::mods::rules_script(&name) else {
        log::error!("mode {}: no rules/{name}.rhai in the mods in force; it plays its built-in rules", mode.key());
        return None;
    };
    match ScriptRules::new(&format!("{id}/rules/{name}.rhai"), &source) {
        Ok(rules) => Some(Box::new(rules)),
        Err(e) => { log::error!("mode {}: {e}; it plays its built-in rules", mode.key()); None }
    }
}

/// Compile every rules script of the mods in force: a server refuses to
/// start on one that doesn't compile.
pub fn check_active() -> Result<usize, String> {
    let mut n = 0;
    for m in peakrunner_core::mods::active() {
        let loaded = m.load()?;
        if let Some(missing) = loaded.missing_script() { return Err(format!("mod {}: {missing}", m.id)); }
        for (name, source) in loaded.scripts {
            ScriptRules::new(&format!("{}/rules/{name}.rhai", m.id), &source)?;
            n += 1;
        }
        for (name, source) in loaded.hooks {
            ScriptHook::new(&format!("{}/hooks/{name}.rhai", m.id), &source)?;
            n += 1;
        }
    }
    // Every hook something names has its script.
    if let Some((at, h)) = peakrunner_core::mods::named_hooks().into_iter().find(|(_, h)| peakrunner_core::mods::hook_script(h).is_none()) {
        return Err(format!("{at}: no hooks/{h}.rhai in the mods in force"));
    }
    Ok(n)
}

/// Compile a script to check it (`peakrunner-server --check`): a rules
/// script, or a hook (`hooks/` in its name).
pub fn check(name: &str, source: &str) -> Result<(), String> {
    if name.contains("hooks/") { ScriptHook::new(name, source).map(|_| ()) } else { ScriptRules::new(name, source).map(|_| ()) }
}

/// A mode's rules, from a script.
pub struct ScriptRules {
    name: String,
    engine: Engine,
    ast: AST,
    out: Arc<Mutex<Vec<Cmd>>>,
    /// `this` in the handlers: the script's state for the round.
    state: Dynamic,
}

impl ScriptRules {
    pub fn new(name: &str, source: &str) -> Result<Self, String> {
        let out: Arc<Mutex<Vec<Cmd>>> = Arc::default();
        let engine = sandbox(name, &out);
        let ast = engine.compile(source).map_err(|e| format!("{name}: {e}"))?;
        let known = ["on_flag_captured", "on_touchdown", "on_carrier_tackled", "on_frag", "on_died", "on_rabbit_grab", "on_rabbit_held",
            "on_zone_points", "on_clock_expired"];
        if let Some(f) = ast.iter_functions().find(|f| f.name.starts_with("on_") && !known.contains(&f.name)) {
            return Err(format!("{name}: {} isn't an event (they are {})", f.name, known.join(", ")));
        }
        Ok(Self { name: name.into(), engine, ast, out, state: Dynamic::from_map(rhai::Map::new()) })
    }

    fn call(&mut self, handler: &str, args: Vec<Dynamic>) -> Result<Vec<Cmd>, String> {
        if !self.ast.iter_functions().any(|f| f.name == handler && f.params.len() == args.len()) { return Ok(Vec::new()); }
        self.out.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let options = CallFnOptions::new().eval_ast(false).rewind_scope(true).bind_this_ptr(&mut self.state);
        let result = self.engine.call_fn_with_options::<Dynamic>(options, &mut Scope::new(), &self.ast, handler, args);
        let cmds = std::mem::take(&mut *self.out.lock().unwrap_or_else(|e| e.into_inner()));
        result.map(|_| cmds).map_err(|e| e.to_string())
    }
}

impl RuleSet for ScriptRules {
    fn event(&mut self, world: &World, event: &GameEvent, out: &mut Vec<Cmd>) {
        let game = Dynamic::from(Game::from(world));
        let int = |i: usize| Dynamic::from(i as i64);
        let (handler, args) = match event {
            GameEvent::FlagCaptured { player } => ("on_flag_captured", vec![game, int(*player)]),
            GameEvent::Touchdown { player } => ("on_touchdown", vec![game, int(*player)]),
            GameEvent::CarrierTackled { winner, loser } => ("on_carrier_tackled", vec![game, int(*winner), int(*loser)]),
            GameEvent::Frag { killer, victim } => ("on_frag", vec![game, int(*killer), int(*victim)]),
            GameEvent::Died { victim, killer } => ("on_died", vec![game, int(*victim), killer.map_or(Dynamic::from(-1i64), int)]),
            GameEvent::RabbitGrab { player } => ("on_rabbit_grab", vec![game, int(*player)]),
            GameEvent::RabbitHeld { player } => ("on_rabbit_held", vec![game, int(*player)]),
            GameEvent::ZonePoints { gained } => ("on_zone_points", vec![game, Dynamic::from_array(gained.iter().map(|&g| Dynamic::from(g as i64)).collect())]),
            GameEvent::ClockExpired => ("on_clock_expired", vec![game]),
        };
        let started = Instant::now();
        match self.call(handler, args) {
            Ok(cmds) => out.extend(cmds),
            Err(e) => log::warn!("rules {}: {handler}: {e}; its commands were dropped", self.name),
        }
        if started.elapsed() > Duration::from_millis(2) {
            log::warn!("rules {}: {handler} took {:.1} ms", self.name, started.elapsed().as_secs_f32() * 1000.0);
        }
    }
}

/// An engine with the standard library, the game view and the commands, and
/// nothing that reaches outside the script.
fn sandbox(name: &str, out: &Arc<Mutex<Vec<Cmd>>>) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(4096)
        .set_max_array_size(1024)
        .set_max_map_size(256)
        .set_strict_variables(true);
    engine.disable_symbol("eval");
    let label = name.to_string();
    engine.on_print(move |text| log::info!("rules {label}: {text}"));
    let label = name.to_string();
    engine.on_debug(move |text, _, _| log::debug!("rules {label}: {text}"));
    register_game(&mut engine);
    register_commands(&mut engine, out);
    engine
}

// ---- What a script reads ----

/// A copy of what the rules may read, taken when the event happens.
#[derive(Clone)]
struct Game { score: [u32; 2], limit: u32, state: MatchState, ffa: bool, me: usize, players: Vec<Seat>, steps: u64, rng: u64, flags: [(Vec3, Vec3); 2], rabbit: Option<usize> }

#[derive(Clone)]
struct Seat {
    name: String, you: bool, team: Team, frags: u32, losses: u32, listed: bool,
    alive: bool, health: f32, energy: f32, armor: String, pos: Vec3, carrying: bool, riding: bool, credits: f32, effects: Vec<String>,
}

impl Game {
    fn from(w: &World) -> Self {
        Game {
            score: w.score, limit: w.mode.def().score_limit, state: w.state, ffa: w.ffa(), me: w.player_id,
            players: w.players.iter().enumerate().map(|(i, p)| Seat {
                name: w.display_name(i), you: w.is_you(i), team: p.team, frags: p.frags, losses: p.losses, listed: p.net_id != 0 || !p.remote,
                alive: p.alive, health: p.health, energy: p.energy, armor: p.armor.id().to_string(), pos: p.pos,
                carrying: p.carrying.is_some(), riding: p.riding > 0, credits: w.credits_of(i),
                effects: p.effects.iter().filter_map(|(k, _)| peakrunner_core::registry::registry().effects.id(*k as usize).map(String::from)).collect(),
            }).collect(),
            steps: w.steps,
            // The server's own stream, moved on by the tick: the same events
            // in the same match draw the same numbers.
            rng: (w.rng_state() as u64) << 32 ^ w.steps.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            flags: [(w.flags[0].home, w.flags[0].pos), (w.flags[1].home, w.flags[1].pos)],
            rabbit: w.rabbit.carrier.filter(|&c| w.rabbit.active && c < w.players.len()),
        }
    }

    fn seat(&self, i: i64) -> Result<&Seat, Fail> {
        usize::try_from(i).ok().and_then(|i| self.players.get(i)).ok_or_else(|| format!("there is no player {i}").into())
    }
}

/// `game.score` ([ember, glacier]), `limit`, `playing`, `ffa`, `me` (this
/// program's player: on a server, nobody's in particular), `my_team` (-1:
/// none), `player_count`, `touchdown_points`, `tackle_points`, `rabbit` (who
/// holds the rabbit flag, -1: nobody); and for a
/// player index: `team(i)`, `frags(i)`, `losses(i)`, `name(i)`, `is_you(i)`
/// (the line is about the reader: "You score", not "You scores"),
/// `hostile(owner, victim)`, plus `leader()` (-1: none) and `team_name(t)`.
fn register_game(engine: &mut Engine) {
    engine.register_type_with_name::<Game>("Game")
        .register_get("score", |g: &mut Game| -> Array { g.score.iter().map(|&s| Dynamic::from(s as i64)).collect() })
        .register_get("limit", |g: &mut Game| g.limit as i64)
        .register_get("playing", |g: &mut Game| g.state == MatchState::Playing)
        .register_get("ffa", |g: &mut Game| g.ffa)
        .register_get("me", |g: &mut Game| g.me as i64)
        .register_get("my_team", |g: &mut Game| g.players.get(g.me).map_or(-1, |p| p.team.idx() as i64))
        .register_get("player_count", |g: &mut Game| g.players.len() as i64)
        .register_get("rabbit", |g: &mut Game| g.rabbit.map_or(-1, |c| c as i64))
        .register_get("touchdown_points", |_: &mut Game| football::TOUCHDOWN_POINTS as i64)
        .register_get("tackle_points", |_: &mut Game| football::TACKLE_POINTS as i64)
        .register_fn("team", |g: &mut Game, i: i64| -> Result<i64, Fail> { Ok(g.seat(i)?.team.idx() as i64) })
        .register_fn("frags", |g: &mut Game, i: i64| -> Result<i64, Fail> { Ok(g.seat(i)?.frags as i64) })
        .register_fn("losses", |g: &mut Game, i: i64| -> Result<i64, Fail> { Ok(g.seat(i)?.losses as i64) })
        .register_fn("name", |g: &mut Game, i: i64| -> Result<String, Fail> { Ok(g.seat(i)?.name.clone()) })
        .register_fn("is_you", |g: &mut Game, i: i64| -> Result<bool, Fail> { Ok(g.seat(i)?.you) })
        // As World::hostile for a player's shot: nobody is hostile to
        // themselves; in free-for-all everyone else is; otherwise the other team.
        .register_fn("hostile", |g: &mut Game, owner: i64, victim: i64| -> Result<bool, Fail> {
            let team = g.seat(owner)?.team;
            if owner == victim { return Ok(false); }
            if g.ffa { return Ok(true); }
            Ok(usize::try_from(victim).ok().and_then(|v| g.players.get(v)).is_some_and(|v| v.team != team))
        })
        // As World::ffa_leader: most frags, then fewest deaths.
        .register_fn("leader", |g: &mut Game| -> i64 {
            g.players.iter().enumerate().filter(|(_, p)| p.listed)
                .max_by_key(|(_, p)| (p.frags, std::cmp::Reverse(p.losses))).map_or(-1, |(i, _)| i as i64)
        })
        .register_fn("team_name", |_: &mut Game, t: i64| -> Result<String, Fail> { Ok(football::team_name(team(t)?).to_string()) })
        // What hooks read besides (`sim::hooks`).
        .register_get("steps", |g: &mut Game| g.steps as i64)
        .register_fn("flag_home", |g: &mut Game, t: i64| -> Result<Array, Fail> { Ok(vec_out(g.flags[team(t)?.idx()].0)) })
        .register_fn("flag_at", |g: &mut Game, t: i64| -> Result<Array, Fail> { Ok(vec_out(g.flags[team(t)?.idx()].1)) })
        .register_fn("distance_to", |g: &mut Game, i: i64, at: Array| -> Result<f64, Fail> { Ok(g.seat(i)?.pos.distance(vec_in("distance_to", at)?) as f64) })
        .register_fn("alive", |g: &mut Game, i: i64| -> Result<bool, Fail> { Ok(g.seat(i)?.alive) })
        .register_fn("health", |g: &mut Game, i: i64| -> Result<f64, Fail> { Ok(g.seat(i)?.health as f64) })
        .register_fn("energy", |g: &mut Game, i: i64| -> Result<f64, Fail> { Ok(g.seat(i)?.energy as f64) })
        .register_fn("armor", |g: &mut Game, i: i64| -> Result<String, Fail> { Ok(g.seat(i)?.armor.clone()) })
        .register_fn("pos", |g: &mut Game, i: i64| -> Result<Array, Fail> { Ok(vec_out(g.seat(i)?.pos)) })
        .register_fn("carrying", |g: &mut Game, i: i64| -> Result<bool, Fail> { Ok(g.seat(i)?.carrying) })
        .register_fn("riding", |g: &mut Game, i: i64| -> Result<bool, Fail> { Ok(g.seat(i)?.riding) })
        .register_fn("credits", |g: &mut Game, i: i64| -> Result<i64, Fail> { Ok(g.seat(i)?.credits as i64) })
        .register_fn("has_effect", |g: &mut Game, i: i64, id: &str| -> Result<bool, Fail> { Ok(g.seat(i)?.effects.iter().any(|e| e == id)) })
        .register_fn("distance", |g: &mut Game, a: i64, b: i64| -> Result<f64, Fail> { Ok(g.seat(a)?.pos.distance(g.seat(b)?.pos) as f64) })
        .register_fn("random", |g: &mut Game, n: i64| -> Result<i64, Fail> {
            if !(1..=1_000_000).contains(&n) { return Err(format!("random({n}): 1 to 1000000").into()); }
            // xorshift64*
            g.rng ^= g.rng >> 12; g.rng ^= g.rng << 25; g.rng ^= g.rng >> 27;
            Ok((g.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) as i64 % n)
        });
}

fn vec_out(v: Vec3) -> Array { vec![Dynamic::from(v.x as f64), Dynamic::from(v.y as f64), Dynamic::from(v.z as f64)] }

fn vec_in(what: &str, a: Array) -> Result<Vec3, Fail> {
    let n: Vec<f64> = a.iter().map(|x| x.as_float().or_else(|_| x.as_int().map(|i| i as f64))).collect::<Result<_, _>>()
        .map_err(|_| -> Fail { format!("{what}: three numbers").into() })?;
    match n[..] {
        [x, y, z] if [x, y, z].iter().all(|v| v.is_finite() && v.abs() < 100_000.0) => Ok(Vec3::new(x as f32, y as f32, z as f32)),
        _ => Err(format!("{what}: three numbers").into()),
    }
}

// ---- What a script does ----

fn team(t: i64) -> Result<Team, Fail> {
    match t { 0 => Ok(Team::Ember), 1 => Ok(Team::Glacier), _ => Err(format!("team {t}: teams are 0 (Ember) and 1 (Glacier)").into()) }
}

fn ranged(what: &str, v: i64, lo: i64, hi: i64) -> Result<i64, Fail> {
    if (lo..=hi).contains(&v) { Ok(v) } else { Err(format!("{what} {v}: {lo} to {hi}").into()) }
}

fn seconds(v: f64) -> Result<f32, Fail> {
    if v.is_finite() && (0.0..=60.0).contains(&v) { Ok(v as f32) } else { Err(format!("{v} seconds: 0 to 60").into()) }
}

fn text(what: &str, t: &str, max: usize) -> Result<String, Fail> {
    if t.chars().count() <= max && !t.chars().any(char::is_control) { Ok(t.to_string()) }
    else { Err(format!("{what}: up to {max} plain characters").into()) }
}

fn register_commands(engine: &mut Engine, out: &Arc<Mutex<Vec<Cmd>>>) {
    let push = |out: &Arc<Mutex<Vec<Cmd>>>| { let out = out.clone(); move |cmd: Cmd| out.lock().unwrap_or_else(|e| e.into_inner()).push(cmd) };
    let p = push(out);
    engine.register_fn("add_score", move |t: i64, points: i64| -> Result<(), Fail> {
        p(Cmd::AddScore { team: team(t)?.idx(), points: ranged("points", points, 0, 100_000)? as u32 }); Ok(())
    });
    let p = push(out);
    engine.register_fn("set_score", move |t: i64, points: i64| -> Result<(), Fail> {
        p(Cmd::SetScore { team: team(t)?.idx(), points: ranged("points", points, 0, 100_000)? as u32 }); Ok(())
    });
    let p = push(out);
    engine.register_fn("add_frags", move |player: i64, frags: i64| -> Result<(), Fail> {
        p(Cmd::AddFrags { player: ranged("player", player, 0, 255)? as usize, frags: ranged("frags", frags, -1000, 1000)? as i32 }); Ok(())
    });
    for integer in [false, true] {
        let p = push(out);
        let end = move |message: &str, s: f64| -> Result<(), Fail> {
            p(Cmd::EndMatch { message: text("end_match", message, 80)?, seconds: seconds(s)? }); Ok(())
        };
        let p = push(out);
        let msg = move |message: &str, s: f64| -> Result<(), Fail> {
            p(Cmd::Message { text: text("message", message, 80)?, seconds: seconds(s)? }); Ok(())
        };
        // Seconds as 8 or 8.0 alike.
        if integer {
            engine.register_fn("end_match", move |m: &str, s: i64| end(m, s as f64));
            engine.register_fn("message", move |m: &str, s: i64| msg(m, s as f64));
        } else {
            engine.register_fn("end_match", end);
            engine.register_fn("message", msg);
        }
    }
    let p = push(out);
    engine.register_fn("sound", move |name: &str| -> Result<(), Fail> {
        let cue = SOUNDS.iter().find(|s| **s == name).ok_or_else(|| -> Fail { format!("sound {name:?}: one of {}", SOUNDS.join(", ")).into() })?;
        p(Cmd::Sound(cue)); Ok(())
    });
    let p = push(out);
    engine.register_fn("feed", move |line: &str| -> Result<(), Fail> { p(Cmd::Feed(text("feed", line, 160)?)); Ok(()) });
    let p = push(out);
    engine.register_fn("shake", move |amount: f64| -> Result<(), Fail> {
        if !(0.0..=1.0).contains(&amount) { return Err(format!("shake {amount}: 0 to 1").into()); }
        p(Cmd::Shake(amount as f32)); Ok(())
    });
}

// ---- Hooks ----

/// The handlers a hook may define.
const HOOK_EVENTS: [&str; 14] = ["on_spawn", "on_death", "on_damage", "on_fire", "on_use", "on_deploy", "on_destroyed", "on_touch",
    "on_enter", "on_leave", "on_pickup", "on_buy", "on_effect", "on_timer"];

/// A definition's or mode's hook, from a script.
pub struct ScriptHook {
    name: String,
    engine: Engine,
    ast: AST,
    out: Arc<Mutex<Vec<Cmd>>>,
    /// `this` in the handlers: the hook's state for the match.
    state: Dynamic,
}

impl ScriptHook {
    pub fn new(name: &str, source: &str) -> Result<Self, String> {
        let out: Arc<Mutex<Vec<Cmd>>> = Arc::default();
        let mut engine = sandbox(name, &out);
        register_hook_commands(&mut engine, &out);
        let ast = engine.compile(source).map_err(|e| format!("{name}: {e}"))?;
        if let Some(f) = ast.iter_functions().find(|f| f.name.starts_with("on_") && !HOOK_EVENTS.contains(&f.name)) {
            return Err(format!("{name}: {} isn't a hook event (they are {})", f.name, HOOK_EVENTS.join(", ")));
        }
        Ok(Self { name: name.into(), engine, ast, out, state: Dynamic::from_map(rhai::Map::new()) })
    }

    fn call(&mut self, handler: &str, args: Vec<Dynamic>) -> Result<Vec<Cmd>, String> {
        if !self.ast.iter_functions().any(|f| f.name == handler && f.params.len() == args.len()) { return Ok(Vec::new()); }
        self.out.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let options = CallFnOptions::new().eval_ast(false).rewind_scope(true).bind_this_ptr(&mut self.state);
        let result = self.engine.call_fn_with_options::<Dynamic>(options, &mut Scope::new(), &self.ast, handler, args);
        let cmds = std::mem::take(&mut *self.out.lock().unwrap_or_else(|e| e.into_inner()));
        result.map(|_| cmds).map_err(|e| e.to_string())
    }
}

/// An event as the record its handler gets.
fn record(event: &HookEvent) -> (&'static str, rhai::Map) {
    let mut m = rhai::Map::new();
    let int = |i: usize| Dynamic::from(i as i64);
    let opt = |i: Option<usize>| Dynamic::from(i.map_or(-1, |i| i as i64));
    let s = |t: &str| Dynamic::from(t.to_string());
    let handler = match event {
        HookEvent::Spawn { player } => { m.insert("player".into(), int(*player)); "on_spawn" }
        HookEvent::Death { victim, killer, cause } => {
            m.insert("victim".into(), int(*victim)); m.insert("killer".into(), opt(*killer)); m.insert("cause".into(), s(cause)); "on_death"
        }
        HookEvent::Damage { victim, attacker, amount, kind } => {
            m.insert("victim".into(), int(*victim)); m.insert("attacker".into(), opt(*attacker));
            m.insert("amount".into(), Dynamic::from(*amount as f64)); m.insert("kind".into(), s(kind)); "on_damage"
        }
        HookEvent::Fire { player, weapon } => { m.insert("player".into(), int(*player)); m.insert("weapon".into(), s(weapon)); "on_fire" }
        HookEvent::Use { player, what } => { m.insert("player".into(), int(*player)); m.insert("what".into(), s(what)); "on_use" }
        HookEvent::Deploy { player, deployable, at } => {
            m.insert("player".into(), int(*player)); m.insert("deployable".into(), s(deployable)); m.insert("at".into(), Dynamic::from_array(vec_out(*at))); "on_deploy"
        }
        HookEvent::Destroyed { deployable, team, at } => {
            m.insert("deployable".into(), s(deployable)); m.insert("team".into(), int(team.idx())); m.insert("at".into(), Dynamic::from_array(vec_out(*at))); "on_destroyed"
        }
        HookEvent::Touch { toucher, touched } => { m.insert("toucher".into(), int(*toucher)); m.insert("touched".into(), int(*touched)); "on_touch" }
        HookEvent::Enter { player, zone } => { m.insert("player".into(), int(*player)); m.insert("zone".into(), s(zone)); "on_enter" }
        HookEvent::Leave { player, zone } => { m.insert("player".into(), int(*player)); m.insert("zone".into(), s(zone)); "on_leave" }
        HookEvent::Pickup { player, item } => { m.insert("player".into(), int(*player)); m.insert("item".into(), s(item)); "on_pickup" }
        HookEvent::Buy { player, what } => { m.insert("player".into(), int(*player)); m.insert("what".into(), Dynamic::from(*what as i64)); "on_buy" }
        HookEvent::Effect { player, effect } => { m.insert("player".into(), int(*player)); m.insert("effect".into(), s(effect)); "on_effect" }
        HookEvent::Timer { name } => { m.insert("name".into(), s(name)); "on_timer" }
    };
    (handler, m)
}

impl Hook for ScriptHook {
    fn event(&mut self, world: &World, event: &HookEvent, out: &mut Vec<Cmd>) {
        let game = Dynamic::from(Game::from(world));
        let (handler, record) = record(event);
        let args = match event {
            HookEvent::Timer { name } => vec![game, Dynamic::from(name.clone())],
            _ => vec![game, Dynamic::from_map(record)],
        };
        match self.call(handler, args) {
            Ok(cmds) => out.extend(cmds),
            Err(e) => log::warn!("hook {}: {handler}: {e}; its commands were dropped", self.name),
        }
    }
}

/// What hooks may do besides the rules' commands; each checks its
/// arguments (the engine checks them again against the world).
fn register_hook_commands(engine: &mut Engine, out: &Arc<Mutex<Vec<Cmd>>>) {
    let push = |out: &Arc<Mutex<Vec<Cmd>>>| { let out = out.clone(); move |cmd: Cmd| out.lock().unwrap_or_else(|e| e.into_inner()).push(cmd) };
    let player = |i: i64| ranged("player", i, 0, 255).map(|i| i as usize);
    let id = |what: &str, s: &str| -> Result<String, Fail> {
        if !s.is_empty() && s.len() <= 32 && s.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-') { Ok(s.to_string()) }
        else { Err(format!("{what} {s:?}: an id").into()) }
    };
    // Amounts as 20 or 20.0 alike.
    let amount = |what: &str, v: Dynamic, hi: f64| -> Result<f32, Fail> {
        let v = v.as_float().or_else(|_| v.as_int().map(|i| i as f64)).map_err(|_| -> Fail { format!("{what}: a number").into() })?;
        if v.is_finite() && (0.0..=hi).contains(&v) { Ok(v as f32) } else { Err(format!("{what} {v}: 0 to {hi}").into()) }
    };
    let p = push(out);
    engine.register_fn("damage", move |i: i64, n: Dynamic, kind: &str| -> Result<(), Fail> {
        p(Cmd::Damage { player: player(i)?, amount: amount("damage", n, 10_000.0)?, kind: id("kind", kind)?, attacker: None }); Ok(())
    });
    let p = push(out);
    engine.register_fn("damage", move |i: i64, n: Dynamic, kind: &str, by: i64| -> Result<(), Fail> {
        let attacker = if by < 0 { None } else { Some(player(by)?) };
        p(Cmd::Damage { player: player(i)?, amount: amount("damage", n, 10_000.0)?, kind: id("kind", kind)?, attacker }); Ok(())
    });
    let p = push(out);
    engine.register_fn("heal", move |i: i64, n: Dynamic| -> Result<(), Fail> { p(Cmd::Heal { player: player(i)?, amount: amount("heal", n, 1000.0)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("give_effect", move |i: i64, e: &str| -> Result<(), Fail> { p(Cmd::GiveEffect { player: player(i)?, effect: id("effect", e)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("clear_effect", move |i: i64, e: &str| -> Result<(), Fail> { p(Cmd::ClearEffect { player: player(i)?, effect: id("effect", e)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("impulse", move |i: i64, v: Array| -> Result<(), Fail> { p(Cmd::Impulse { player: player(i)?, vel: vec_in("impulse", v)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("teleport", move |i: i64, v: Array| -> Result<(), Fail> { p(Cmd::Teleport { player: player(i)?, to: vec_in("teleport", v)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("set_armor", move |i: i64, a: &str| -> Result<(), Fail> { p(Cmd::SetArmor { player: player(i)?, armor: id("armor", a)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("give_rounds", move |i: i64, w: &str, n: i64| -> Result<(), Fail> {
        p(Cmd::GiveRounds { player: player(i)?, weapon: id("weapon", w)?, rounds: ranged("rounds", n, -10_000, 10_000)? as i32 }); Ok(())
    });
    let p = push(out);
    engine.register_fn("set_energy", move |i: i64, n: Dynamic| -> Result<(), Fail> { p(Cmd::SetEnergy { player: player(i)?, energy: amount("energy", n, 10_000.0)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("credits", move |i: i64, n: i64| -> Result<(), Fail> {
        p(Cmd::Credits { player: player(i)?, amount: ranged("credits", n, -1_000_000, 1_000_000)? as i32 }); Ok(())
    });
    let p = push(out);
    engine.register_fn("team_feed", move |t: i64, line: &str| -> Result<(), Fail> { p(Cmd::TeamFeed { team: team(t)?.idx(), text: text("team_feed", line, 160)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("waypoint", move |t: i64, v: Array| -> Result<(), Fail> { p(Cmd::Waypoint { team: team(t)?.idx(), at: vec_in("waypoint", v)? }); Ok(()) });
    let p = push(out);
    engine.register_fn("place", move |t: i64, d: &str, v: Array, yaw: Dynamic| -> Result<(), Fail> {
        let yaw = amount("place yaw", yaw, 360.0)?;
        p(Cmd::Place { team: team(t)?.idx(), deployable: id("deployable", d)?, at: vec_in("place", v)?, yaw: yaw.to_radians() }); Ok(())
    });
    let p = push(out);
    engine.register_fn("projectile", move |owner: i64, k: &str, at: Array, vel: Array| -> Result<(), Fail> {
        let owner = if owner < 0 { None } else { Some(player(owner)?) };
        p(Cmd::Projectile { owner, projectile: id("projectile", k)?, at: vec_in("projectile", at)?, vel: vec_in("projectile", vel)? }); Ok(())
    });
    let p = push(out);
    engine.register_fn("after", move |s: Dynamic, name: &str| -> Result<(), Fail> {
        p(Cmd::Timer { name: text("after", name, 32)?, seconds: amount("after", s, 600.0)? }); Ok(())
    });
}

#[cfg(test)]
mod tests;
