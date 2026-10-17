// Team Rabbit: each team has the score limit in lives. Every death costs the
// dead player's team one (the other team scores it), two when the killer's
// team holds the flag (`game.rabbit`). Taking their last life wins; at time,
// the team that took more.

fn on_died(game, victim, killer) {
    if !game.playing { return; }
    let gainer = 1 - game.team(victim);
    let flag = game.rabbit;
    let bonus = killer >= 0 && killer != victim && game.team(killer) == gainer && flag >= 0 && game.team(flag) == gainer;
    let points = if bonus { 2 } else { 1 };
    add_score(gainer, points);
    if game.score[gainer] + points >= game.limit {
        set_score(gainer, game.limit);
        let mine = if game.my_team < 0 { 0 } else { game.my_team };
        end_match(if mine == gainer { "VICTORY" } else { "DEFEAT" }, 8.0);
    }
}

fn on_clock_expired(game) {
    let score = game.score;
    let mine = if game.my_team < 0 { 0 } else { game.my_team };
    let message = if score[mine] > score[1 - mine] { "VICTORY" }
        else if score[mine] < score[1 - mine] { "DEFEAT" }
        else { "DRAW" };
    end_match(message, 8.0);
}
