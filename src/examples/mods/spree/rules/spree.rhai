// Spree TDM: Team Deathmatch where streaks pay. Each enemy frag scores one
// for the team; every third frag in a row, without dying, scores one more
// and is called out in the feed. Killing yourself costs your team a point.
// The first team to the score limit wins.
//
// `this` lasts the round: here, each player's current streak.

fn on_frag(game, killer, victim) {
    if !game.playing { return; }
    if type_of(this.streaks) != "map" { this.streaks = #{}; }
    this.streaks.remove(`${victim}`);  // a death ends a streak
    let team = game.team(killer);
    if killer == victim {
        let score = game.score[team];
        if score > 0 { set_score(team, score - 1); }
        return;
    }
    if !game.hostile(killer, victim) { return; }
    let streak = (this.streaks[`${killer}`] ?? 0) + 1;
    this.streaks[`${killer}`] = streak;
    let points = if streak % 3 == 0 { 2 } else { 1 };
    add_score(team, points);
    if streak % 3 == 0 {
        feed(`${game.name(killer)} is on a spree: ${streak} in a row!`);
        sound("point");
    }
    if game.score[team] + points >= game.limit {
        end_match(`${game.team_name(team)} WINS THE SPREE`, 8.0);
    }
}

fn on_clock_expired(game) {
    let score = game.score;
    end_match(if score[0] > score[1] { "EMBER WINS THE SPREE" }
        else if score[1] > score[0] { "GLACIER WINS THE SPREE" }
        else { "DRAW" }, 8.0);
}
