// Football: a touchdown scores one for the team and touchdown points for the
// scorer; a tackle on the carrier earns tackle points; the first team to the
// score limit wins. (The engine runs the ball, passes, tackles and kickoffs.)

fn on_touchdown(game, player) {
    let team = game.team(player);
    add_score(team, 1);
    add_frags(player, game.touchdown_points);
    let scores = if game.is_you(player) { "score" } else { "scores" };
    feed(`${game.name(player)} ${scores} a touchdown for ${game.team_name(team)}!`);
    shake(0.45);
    let mine = team == game.my_team;
    sound(if mine { "touchdown_win" } else { "touchdown_loss" });
    if game.score[team] + 1 >= game.limit {
        end_match(if mine { "VICTORY" } else { "DEFEAT" }, 8.0);
    }
}

fn on_carrier_tackled(game, winner, loser) {
    add_frags(winner, game.tackle_points);
}

fn on_clock_expired(game) {
    end_match("TIME — MATCH OVER", 4.0);
}
