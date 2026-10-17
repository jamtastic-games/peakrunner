// Sparkfall's advanced scoring (server only). A kill scores frags for:
// - defense: the victim near the killer's own flag (+2 within 30 m, +1 within 80 m);
// - the carrier: killing whoever carries a flag (+3);
// - the underdog: a lighter class killing a heavier one (+1 per tier between);
// - range: farther than 150 m (+1).
// The team purse gets 20 credits per bonus frag.

fn tier(armor) {
    switch armor {
        "courier" | "longshot" | "veil" => 1,
        "striker" | "bombard" | "rigger" | "warden" => 2,
        "bulwark" => 3,
        "siegeframe" => 4,
        _ => 2,
    }
}

fn on_death(game, ev) {
    let killer = ev.killer;
    let victim = ev.victim;
    if killer < 0 || killer == victim || !game.hostile(killer, victim) { return; }
    let bonus = 0;
    let home = game.flag_home(game.team(killer));
    let near = game.distance_to(victim, home);
    if near < 30.0 { bonus += 2; } else if near < 80.0 { bonus += 1; }
    if game.carrying(victim) { bonus += 3; }
    let gap = tier(game.armor(victim)) - tier(game.armor(killer));
    if gap > 0 { bonus += gap; }
    if game.distance(killer, victim) > 150.0 { bonus += 1; }
    if bonus > 0 {
        add_frags(killer, bonus);
        credits(killer, bonus * 20);
        if bonus >= 3 { team_feed(game.team(killer), game.name(killer) + " earned " + bonus + " bonus for that kill"); }
    }
}
