//! Sparkfall's scoring hook (src/examples/mods/sparkfall/hooks/scoring.rhai):
//! a light class killing a heavy one near its own flag scores the frag, a
//! defense bonus and an underdog bonus, and pays the team purse for them.
//! Its own test binary: activating a mod changes the process's definitions.
use peakrunner_core::map_catalog::SupportedMode;
use peakrunner_core::sim::{ArmorClass, Team, World};
use peakrunner_core::terrain::MapId;
use peakrunner_core::{catalog, mods};

#[test]
fn bonuses_for_defense_and_the_underdog() {
    catalog::add_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples"));
    mods::activate_home(&["sparkfall".to_string()]).unwrap();
    peakrunner_script::install();
    peakrunner_script::check_active().unwrap();
    let mode = SupportedMode::parse("sparkfall-ctf").unwrap();
    let mut w = World::new();
    w.set_map(MapId::OldHoller);
    w.set_mode(mode);
    w.start_match(true);
    w.countdown = 0.0;
    w.players.truncate(1);
    w.players[0].team = Team::Ember;
    w.players[0].armor = ArmorClass::by_id("courier").unwrap();
    let mut foe = w.players[0].clone();
    foe.team = Team::Glacier;
    foe.armor = ArmorClass::by_id("bulwark").unwrap();
    // At the killer's own flag.
    foe.pos = w.flags[0].home + peakrunner_core::glam::Vec3::new(5.0, 0.0, 0.0);
    w.players.push(foe);
    let (frags, purse) = (w.players[0].frags, w.credits_of(0));
    w.damage(1, 1000.0, "blast", Some(0), "Test");
    assert!(!w.players[1].alive);
    // 1 for the kill, 2 for defense, 2 for the underdog (tier 1 over tier 3).
    assert_eq!(w.players[0].frags, frags + 5);
    // 60 for the frag (the mode's economy), 20 a bonus frag (the hook).
    assert_eq!(w.credits_of(0), purse + 60.0 + 80.0);
}
