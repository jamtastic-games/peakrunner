// Team Deathmatch: each frag of an enemy scores one for the team; the first
// team to the score limit wins, and at time the higher score.

fn on_frag(game, killer, victim) {
    if !game.playing { return; }
    if killer == victim { return; }
    if !game.hostile(killer, victim) { return; }
    let team = game.team(killer);
    add_score(team, 1);
    if game.score[team] + 1 >= game.limit { end_deathmatch(game, team); }
}

fn on_clock_expired(game) {
    end_deathmatch(game, -1);
}

// Victory, defeat or a draw, counting `scored` (a team frag not yet added).
fn end_deathmatch(game, scored) {
    let score = game.score;
    if scored >= 0 { score[scored] += 1; }
    let mine = if game.my_team < 0 { 0 } else { game.my_team };
    let message = if score[mine] > score[1 - mine] { "VICTORY" }
        else if score[mine] < score[1 - mine] { "DEFEAT" }
        else { "DRAW" };
    end_match(message, 8.0);
}
