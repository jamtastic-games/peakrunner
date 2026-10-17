use super::*;
use peakrunner_core::sim::rules;

/// A small deterministic random stream (xorshift), so failures replay.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
}

const NAMES: [&str; 4] = ["", "Ann", "bob smith", "Zed 9"];

/// A world in `mode` with a random roster, score, state and local player.
fn shuffle(w: &mut World, template: &peakrunner_core::sim::Player, rng: &mut Rng) {
    let limit = w.mode.def().score_limit as u64;
    let n = 2 + rng.below(8) as usize;
    w.players = (0..n).map(|_| {
        let mut p = template.clone();
        p.team = if rng.below(2) == 0 { Team::Ember } else { Team::Glacier };
        p.frags = rng.below(limit + 3) as u32;
        p.losses = rng.below(6) as u32;
        p.name = NAMES[rng.below(4) as usize].into();
        p.net_id = rng.below(3) as u32;
        p.remote = rng.below(2) == 0;
        p
    }).collect();
    w.score = [rng.below(limit + 2) as u32, rng.below(limit + 2) as u32];
    w.state = [MatchState::Playing, MatchState::Playing, MatchState::Ended, MatchState::Flyby][rng.below(4) as usize];
    // The rabbit flag: none, loose, or held by anyone (or a seat that's gone).
    w.rabbit.active = rng.below(4) != 0;
    w.rabbit.carrier = [None, Some(rng.below(n as u64 + 1) as usize)][rng.below(2) as usize];
    // Sometimes nobody: a server's view.
    w.player_id = rng.below(n as u64 + 2) as usize;
}

fn event(rng: &mut Rng, n: usize) -> GameEvent {
    let p = |rng: &mut Rng| rng.below(n as u64) as usize;
    match rng.below(9) {
        0 => GameEvent::FlagCaptured { player: p(rng) },
        1 => GameEvent::Touchdown { player: p(rng) },
        2 => GameEvent::CarrierTackled { winner: p(rng), loser: p(rng) },
        3 => { let killer = p(rng); GameEvent::Frag { killer, victim: if rng.below(4) == 0 { killer } else { p(rng) } } }
        4 => GameEvent::ZonePoints { gained: [rng.below(4) as u32 * rng.below(2) as u32, rng.below(4) as u32] },
        5 => { let victim = p(rng); GameEvent::Died { victim, killer: [None, Some(victim), Some(p(rng)), Some(p(rng))][rng.below(4) as usize] } }
        6 => GameEvent::RabbitGrab { player: p(rng) },
        7 => GameEvent::RabbitHeld { player: p(rng) },
        _ => GameEvent::ClockExpired,
    }
}

/// The P6 gate: each base rule set as a script gives exactly the commands
/// of the engine's Rust rule set, over thousands of random worlds and events.
#[test]
fn ab_every_base_rule_set_script_matches_the_engine_s() {
    for (i, (key, source)) in BASE.iter().enumerate() {
        let mode = SupportedMode::parse(key).unwrap();
        let mut script = ScriptRules::new(key, source).unwrap();
        let mut w = World::new();
        w.set_mode(mode);
        w.start_match(true);
        let template = w.players[0].clone();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1));
        let mut answered = 0;
        for case in 0..6000 {
            shuffle(&mut w, &template, &mut rng);
            let e = event(&mut rng, w.players.len());
            let mut want = Vec::new();
            rules::builtin(mode.rules()).event(&w, &e, &mut want);
            let mut got = Vec::new();
            script.event(&w, &e, &mut got);
            assert_eq!(got, want, "{key}, case {case}: {e:?}");
            if !want.is_empty() { answered += 1; }
        }
        assert!(answered > 500, "{key}: only {answered} cases did anything");
    }
}

fn run(source: &str, e: GameEvent) -> Vec<Cmd> {
    let mut w = World::new();
    w.set_mode(SupportedMode::TeamDeathmatch);
    w.start_match(true);
    let mut out = Vec::new();
    ScriptRules::new("test", source).unwrap().event(&w, &e, &mut out);
    out
}

#[test]
fn a_failing_call_drops_all_its_commands() {
    let frag = GameEvent::Frag { killer: 0, victim: 0 };
    // Over the operation limit, a bad argument, a player who isn't there.
    for body in ["add_score(0, 1); loop { }", "add_score(0, 1); add_score(5, 1);", "add_score(0, 1); game.team(99);",
        "add_score(0, 1); sound(\"disc\");", "add_score(0, 1); end_match(\"x\", 999);", "add_score(0, 1); feed(\"a\\nb\");", "add_score(0, 1); timestamp();"] {
        assert_eq!(run(&format!("fn on_frag(game, killer, victim) {{ {body} }}"), frag.clone()), vec![], "{body}");
    }
    assert_eq!(run("fn on_frag(game, killer, victim) { add_score(1, 2); message(\"FIRST BLOOD\", 3); }", frag),
        vec![Cmd::AddScore { team: 1, points: 2 }, Cmd::Message { text: "FIRST BLOOD".into(), seconds: 3.0 }]);
}

#[test]
fn scripts_keep_state_for_the_round_and_can_t_reach_outside() {
    let mut w = World::new();
    w.set_mode(SupportedMode::TeamDeathmatch);
    w.start_match(true);
    let mut rules = ScriptRules::new("count", "fn on_clock_expired(game) { this.n = (this.n ?? 0) + 1; message(`call ${this.n}`, 1); }").unwrap();
    for n in 1..=3 {
        let mut out = Vec::new();
        rules.event(&w, &GameEvent::ClockExpired, &mut out);
        assert_eq!(out, vec![Cmd::Message { text: format!("call {n}"), seconds: 1.0 }]);
    }
    for bad in ["fn on_frag(game, k, v) { eval(\"1\") }", "import \"x\" as y;",
        "fn on_frags(game, k, v) { }", "fn on_frag(game, k, v) { undeclared + 1 }", "fn on_frag(game, k, v) {"] {
        assert!(ScriptRules::new("bad", bad).is_err(), "{bad}");
    }
}

#[test]
fn the_spree_example_pays_streaks_and_charges_suicides() {
    let mut w = World::new();
    w.set_mode(SupportedMode::TeamDeathmatch);
    w.start_match(true);
    let template = w.players[0].clone();
    w.players = [Team::Ember, Team::Ember, Team::Glacier, Team::Glacier].iter().enumerate().map(|(i, &team)| {
        let mut p = template.clone();
        p.team = team;
        p.name = ["Ann", "Bea", "Cy", "Di"][i].into();
        p
    }).collect();
    w.state = MatchState::Playing;
    let mut spree = ScriptRules::new("spree", include_str!("../../../examples/mods/spree/rules/spree.rhai")).unwrap();
    let mut frag = |w: &World, killer, victim| { let mut out = Vec::new(); spree.event(w, &GameEvent::Frag { killer, victim }, &mut out); out };
    let one = vec![Cmd::AddScore { team: 0, points: 1 }];
    assert_eq!(frag(&w, 0, 2), one);
    assert_eq!(frag(&w, 0, 3), one);
    assert_eq!(frag(&w, 0, 2), vec![Cmd::AddScore { team: 0, points: 2 }, Cmd::Feed("Ann is on a spree: 3 in a row!".into()), Cmd::Sound("point")]);
    // Cy ends Ann's streak; her next frag starts a new one.
    assert_eq!(frag(&w, 2, 0), vec![Cmd::AddScore { team: 1, points: 1 }]);
    assert_eq!(frag(&w, 0, 3), one);
    assert_eq!(frag(&w, 1, 0), vec![], "a teammate isn't an enemy");
    w.score = [3, 0];
    assert_eq!(frag(&w, 1, 1), vec![Cmd::SetScore { team: 0, points: 2 }], "a suicide costs a point");
    w.score = [0, 39];
    assert_eq!(frag(&w, 3, 0), vec![Cmd::AddScore { team: 1, points: 1 }, Cmd::EndMatch { message: "GLACIER WINS THE SPREE".into(), seconds: 8.0 }]);
}
