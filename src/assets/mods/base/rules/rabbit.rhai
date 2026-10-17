// Rabbit, the classic rules: everyone else hunts whoever holds the one flag.
// A point for taking it, one each time a player's time holding it (all their
// holds together) passes another 15 seconds, one for a hunter's kill of the
// rabbit and two for the rabbit's kills (the engine counts one; hunters
// can't hurt each other); killing yourself costs nothing. The first to the
// score limit wins, and at time the leader.

fn on_rabbit_grab(game, player) {
    if !game.playing { return; }
    let verb = if game.is_you(player) { "are" } else { "is" };
    feed(`${game.name(player)} ${verb} the rabbit`);
    point(game, player);
}

fn on_rabbit_held(game, player) {
    if !game.playing { return; }
    point(game, player);
}

fn on_frag(game, killer, victim) {
    if !game.playing || killer == victim { return; }
    if killer == game.rabbit {
        point(game, killer);
    } else if victim == game.rabbit && game.frags(killer) >= game.limit {
        end_rabbit(game, -1);
    }
}

fn on_clock_expired(game) {
    end_rabbit(game, -1);
}

fn point(game, player) {
    add_frags(player, 1);
    if game.frags(player) + 1 >= game.limit { end_rabbit(game, player); }
}

// The leader wins (`scored`: a player whose winning point isn't counted yet).
fn end_rabbit(game, scored) {
    let winner = if scored >= 0 { scored } else { game.leader() };
    let message = if winner < 0 { "MATCH OVER" }
        else if winner == game.me { "YOU WIN" }
        else { `${game.name(winner).to_upper()} WINS` };
    end_match(message, 8.0);
}
