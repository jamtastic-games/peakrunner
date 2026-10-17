// Capture & Hold: held points earn points; the first team to the score limit
// holds the field. (The engine runs capturing, decay and the drain.)

fn on_zone_points(game, gained) {
    let score = [game.score[0] + gained[0], game.score[1] + gained[1]];
    for t in 0..2 {
        if gained[t] > 0 { add_score(t, gained[t]); }
    }
    for t in 0..2 {
        if score[t] >= game.limit {
            set_score(t, game.limit);
            end_match(if t == 0 { "EMBER HOLDS THE FIELD" } else { "GLACIER HOLDS THE FIELD" }, 4.0);
            return;
        }
    }
}

fn on_clock_expired(game) {
    end_match("TIME — MATCH OVER", 4.0);
}
