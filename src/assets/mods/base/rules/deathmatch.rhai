// Deathmatch, every player for themselves: the first to the score limit in
// frags wins; killing yourself costs a frag. (The engine counts the frags.)

fn on_frag(game, killer, victim) {
    if !game.playing { return; }
    if killer == victim {
        add_frags(killer, -1);
        return;
    }
    if !game.hostile(killer, victim) { return; }
    if game.frags(killer) >= game.limit { end_deathmatch(game); }
}

fn on_clock_expired(game) {
    end_deathmatch(game);
}

// The leader wins, by frags and then fewest deaths.
fn end_deathmatch(game) {
    let leader = game.leader();
    let message = if leader < 0 { "MATCH OVER" }
        else if leader == game.me { "YOU WIN THE DEATHMATCH" }
        else { `${game.name(leader).to_upper()} WINS THE DEATHMATCH` };
    end_match(message, 8.0);
}
