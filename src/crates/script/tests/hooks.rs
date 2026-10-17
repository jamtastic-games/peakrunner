//! Hooks named by an armor and a mode (crates/script, `sim::hooks`): the
//! armor's hook shields its wearers as they spawn and pays for blast damage
//! dealt, its death handler heals the killer, tells their team and sets a
//! timer that scores; a hook's own damage never sets off more hooks.
//! Its own test binary: activating a mod changes the process's definitions.
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{Team, World, STEP};
use peakrunner_core::terrain::MapId;
use peakrunner_core::{catalog, mods, registry};

fn write(root: &std::path::Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

#[test]
fn hooks_hear_events_and_their_commands_apply() {
    let root = std::env::temp_dir().join(format!("peakrunner-hooks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = "mods/hooked";
    write(&root, &format!("{m}/mod.json"), r#"{"name": "Hooked", "note": "Hooks.", "api": 1}"#);
    write(&root, &format!("{m}/armors/hook.json"), r#"{ "light": { "hook": "watch" } }"#);
    write(&root, &format!("{m}/modes/hooked-ctf.json"), r#"{ "base": "ctf", "label": "Hooked CTF", "short": "HCTF", "note": "Hooks.",
        "goal": "Capture", "start_message": "HOOKED", "economy": { "start": 0 }, "hooks": ["referee"] }"#);
    write(&root, &format!("{m}/hooks/watch.rhai"), r#"
        fn on_spawn(game, ev) { give_effect(ev.player, "shielded"); }
        fn on_damage(game, ev) {
            if ev.kind == "blast" && ev.attacker >= 0 {
                credits(ev.attacker, 5);
                // Hooks' own commands never set off more hooks.
                damage(ev.attacker, 1, "impact");
            }
        }
        fn on_death(game, ev) {
            if ev.killer >= 0 {
                heal(ev.killer, 25);
                team_feed(game.team(ev.killer), "A kill for " + game.name(ev.killer));
            }
            after(1.5, "revenge");
        }
        fn on_timer(game, name) { if name == "revenge" { add_score(0, 1); } }
    "#);
    write(&root, &format!("{m}/hooks/referee.rhai"), r#"
        fn on_death(game, ev) { this.deaths = if "deaths" in this { this.deaths + 1 } else { 1 }; if this.deaths == 2 { add_score(1, 7); } }
    "#);
    catalog::add_root(root.clone());
    mods::activate_home(&["hooked".to_string()]).unwrap();
    peakrunner_script::install();
    assert_eq!(peakrunner_script::check_active().map(|n| n >= 2), Ok(true));
    let mode = SupportedMode::parse("hooked-ctf").unwrap();

    let mut w = World::new();
    w.set_map(MapId::OldHoller);
    w.set_mode(mode);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.name = "Foe".into();
    w.players.push(foe);
    let shielded = registry::registry().effects.index("shielded").unwrap() as u8;

    // Damage: the attacker is paid, and the hook's own damage to them
    // didn't call the hook again (one payment, one point of damage).
    w.players[0].health = 50.0;
    w.damage(1, 10.0, "blast", Some(0), "Test");
    assert_eq!(w.credits_of(0), 5.0);
    assert!(w.players[0].health < 50.0 && w.players[0].health > 48.0, "{}", w.players[0].health);

    // Death: the killer is healed, their team told, and a timer scores.
    let before = w.feed.len();
    w.damage(1, 1000.0, "blast", Some(0), "Test");
    assert!(!w.players[1].alive);
    assert!(w.players[0].health > 70.0, "healed: {}", w.players[0].health);
    assert!(w.feed.len() > before, "told");
    let score = w.score[0];
    for _ in 0..(1.6 / STEP) as usize { w.tick(STEP); }
    assert_eq!(w.score[0], score + 1, "the timer scored");

    // Spawn: shielded at once (their armor's hook).
    w.players[1].respawn = 0.0;
    w.tick(STEP);
    assert!(w.players[1].alive && w.has_effect(1, shielded));

    // The mode's hook hears every death, and keeps its own state.
    w.damage(1, 1000.0, "blast", Some(0), "Test");
    assert_eq!(w.score[1], 7, "the referee's second death");

    // A hook that doesn't compile is refused by the check.
    assert!(peakrunner_script::check("x/hooks/bad.rhai", "fn on_dmg(game, ev) {}").is_err());
    assert!(peakrunner_script::check("x/hooks/bad.rhai", "fn on_spawn(game, ev) { ").is_err());
    let _ = std::fs::remove_dir_all(&root);
}
