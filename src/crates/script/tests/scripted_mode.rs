//! A scripted mode through the real engine: the example Spree mod, found in
//! a content folder and put in force as a server would, runs its script when
//! a player presses Ctrl+K; the base Team Deathmatch it varies doesn't.
use peakrunner_core::{catalog, map_catalog::SupportedMode, mods, sim::{MatchState, World}};

fn suicide_in(mode: SupportedMode) -> [u32; 2] {
    let mut w = World::new();
    w.set_mode(mode);
    w.start_match(true);
    while w.state != MatchState::Playing || w.countdown > 0.0 { w.tick(1.0 / 60.0); }
    let team = w.players[w.player_id].team.idx();
    w.score[team] = 5;
    w.input.suicide = true;
    for _ in 0..10 { w.tick(1.0 / 60.0); }
    assert!(!w.players[w.player_id].alive, "the player is dead");
    w.score
}

#[test]
fn a_scripted_mode_runs_its_script_inside_the_engine() {
    catalog::add_root(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples"));
    mods::activate_home(&["spree".to_string()]).unwrap();
    peakrunner_script::install();
    let spree = SupportedMode::parse("spree-tdm").expect("the example's mode is in force");
    assert_eq!(spree.def().script.as_deref(), Some("spree"));
    let scripted = suicide_in(spree);
    let base = suicide_in(SupportedMode::TeamDeathmatch);
    assert_eq!(scripted.iter().sum::<u32>(), 4, "Spree charges the suicide a point: {scripted:?}");
    assert_eq!(base.iter().sum::<u32>(), 5, "plain Team Deathmatch doesn't: {base:?}");
}
