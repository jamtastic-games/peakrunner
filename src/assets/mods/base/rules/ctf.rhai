// Capture the Flag: a capture scores one; the first team to the score limit
// wins. (The engine runs the flags: grabs, returns and the capture itself.)

fn on_flag_captured(game, player) {
    let team = game.team(player);
    let mine = team == game.my_team;
    add_score(team, 1);
    sound(if mine { "capture_win" } else { "capture_loss" });
    shake(0.55);
    if game.score[team] + 1 >= game.limit {
        end_match(if mine { "VICTORY" } else { "DEFEAT" }, 8.0);
    }
}

fn on_clock_expired(game) {
    end_match("TIME — MATCH OVER", 4.0);
}
