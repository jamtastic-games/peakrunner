// Run by the server only. When a Guardian falls, its team hears who did it.
fn on_death(game, ev) {
    if ev.killer >= 0 && ev.killer != ev.victim {
        team_feed(game.team(ev.victim), "Guardian " + game.name(ev.victim) + " fell to " + game.name(ev.killer));
    }
}
