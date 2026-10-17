//! Rule sets (docs/modding.md, P5): what scores, what wins and what the game
//! announces about it, apart from the engine's mechanics. The engine runs
//! flags, the ball, capture points, tackles and the clock; when something
//! happens that a rule set cares about it reports a `GameEvent`, and the rule
//! set answers with `Cmd`s the engine applies at once. Events are rare (a
//! capture, a touchdown, a frag, the clock running out), so a rule set never
//! runs per player per tick. The five built-in rule sets are Rust
//! (`builtin`); scripts implement the same trait (P6, `crates/script`): a
//! program that can run them registers a `Factory`, and a mode that names a
//! `script` gets one from it each round. Only servers register one, so a
//! player's game never runs a script.
use crate::map_catalog::SupportedMode;
use crate::modes::Rules;
use crate::sim::{MatchState, World};
use std::sync::OnceLock;

/// Something a rule set may score, end the match on, or announce.
#[derive(Clone, Debug, PartialEq)]
pub enum GameEvent {
    /// `player` brought the enemy flag home (the engine has returned it).
    FlagCaptured { player: usize },
    /// `player` carried the ball into the enemy end zone (the engine has
    /// called the ball dead and scheduled the next one).
    Touchdown { player: usize },
    /// `winner` tackled `loser`, who carried the ball.
    CarrierTackled { winner: usize, loser: usize },
    /// `killer` fragged `victim` (the same player: a suicide).
    Frag { killer: usize, victim: usize },
    /// Player `victim` died, killed by `killer` (None: the map or a turret;
    /// the victim: by their own hand). Every death that counts (none in a
    /// blade zone or a duel lobby), after any `Frag`.
    Died { victim: usize, killer: Option<usize> },
    /// `player` took the rabbit flag (`rabbit`).
    RabbitGrab { player: usize },
    /// `player`'s time holding the rabbit flag passed another whole
    /// `rabbit::HELD_POINT` seconds.
    RabbitHeld { player: usize },
    /// Held capture points earned these points this step, per team.
    ZonePoints { gained: [u32; 2] },
    /// The match clock ran out.
    ClockExpired,
}

/// What a rule set tells the engine to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    AddScore { team: usize, points: u32 },
    SetScore { team: usize, points: u32 },
    /// Frags (personal points) for a player; negative takes some away.
    AddFrags { player: usize, frags: i32 },
    /// The match is over: a centre message for `seconds`, and the end sound.
    EndMatch { message: String, seconds: f32 },
    /// A centre message for `seconds`.
    Message { text: String, seconds: f32 },
    /// A named sound or HUD cue (`sound::Cue::from_event`).
    Sound(&'static str),
    /// A duel's round is decided: the next counts down in `seconds` (`duel`).
    NextRound { seconds: f32 },
    /// A play-by-play line in the feed.
    Feed(String),
    /// Screen shake for this player (0..1).
    Shake(f32),
    // What hooks may do (`hooks`), each checked when applied:
    /// Damage a player (before armor) of a kind, for an attacker.
    Damage { player: usize, amount: f32, kind: String, attacker: Option<usize> },
    Heal { player: usize, amount: f32 },
    GiveEffect { player: usize, effect: String },
    ClearEffect { player: usize, effect: String },
    /// Add to a player's velocity (m/s), or put them somewhere.
    Impulse { player: usize, vel: glam::Vec3 },
    Teleport { player: usize, to: glam::Vec3 },
    SetArmor { player: usize, armor: String },
    GiveRounds { player: usize, weapon: String, rounds: i32 },
    SetEnergy { player: usize, energy: f32 },
    /// Credits for a player (their team's where shared); negative takes.
    Credits { player: usize, amount: i32 },
    /// A line in one team's feed.
    TeamFeed { team: usize, text: String },
    Waypoint { team: usize, at: glam::Vec3 },
    /// A deployable for a team, standing at `at`.
    Place { team: usize, deployable: String, at: glam::Vec3, yaw: f32 },
    /// A projectile from `owner` (a player slot; none: the world's).
    Projectile { owner: Option<usize>, projectile: String, at: glam::Vec3, vel: glam::Vec3 },
    /// Call the hook back with `on_timer(name)` in `seconds`.
    Timer { name: String, seconds: f32 },
}

/// A rule set: it reads the world and answers events with commands.
pub trait RuleSet {
    fn event(&mut self, world: &World, event: &GameEvent, out: &mut Vec<Cmd>);
}

/// Makes the rule set for a mode that names a script (None: it can't, and
/// the mode plays its built-in rules).
pub type Factory = fn(SupportedMode) -> Option<Box<dyn RuleSet + Send>>;
static FACTORY: OnceLock<Factory> = OnceLock::new();

/// Let modes with scripts use `factory` (a server, once at startup).
pub fn set_factory(factory: Factory) { let _ = FACTORY.set(factory); }

/// The scripted rule set for `mode`, if it names one and this program runs scripts.
pub(crate) fn scripted(mode: SupportedMode) -> Option<Box<dyn RuleSet + Send>> {
    mode.def().script.as_ref()?;
    FACTORY.get()?(mode)
}

/// The built-in rule set for `rules`.
pub fn builtin(rules: Rules) -> Builtin { Builtin(rules) }

/// The engine's own rule sets, as the base modes play them.
pub struct Builtin(pub Rules);

impl RuleSet for Builtin {
    fn event(&mut self, w: &World, event: &GameEvent, out: &mut Vec<Cmd>) {
        let limit = w.mode.def().score_limit;
        let local = w.players.get(w.player_id).map(|p| p.team);
        match (self.0, event) {
            (Rules::Ctf, GameEvent::FlagCaptured { player }) => {
                let team = w.players[*player].team;
                out.push(Cmd::AddScore { team: team.idx(), points: 1 });
                out.push(Cmd::Sound(if Some(team) == local { "capture_win" } else { "capture_loss" }));
                out.push(Cmd::Shake(0.55));
                if w.score[team.idx()] + 1 >= limit {
                    out.push(Cmd::EndMatch { message: if Some(team) == local { "VICTORY" } else { "DEFEAT" }.into(), seconds: 8.0 });
                }
            }
            (Rules::CaptureAndHold, GameEvent::ZonePoints { gained }) => {
                let score = [w.score[0] + gained[0], w.score[1] + gained[1]];
                for t in 0..2 { if gained[t] > 0 { out.push(Cmd::AddScore { team: t, points: gained[t] }); } }
                if let Some(t) = (0..2).find(|&t| score[t] >= limit) {
                    out.push(Cmd::SetScore { team: t, points: limit });
                    out.push(Cmd::EndMatch { message: if t == 0 { "EMBER HOLDS THE FIELD" } else { "GLACIER HOLDS THE FIELD" }.into(), seconds: 4.0 });
                }
            }
            (Rules::Football, GameEvent::Touchdown { player }) => {
                let team = w.players[*player].team;
                out.push(Cmd::AddScore { team: team.idx(), points: 1 });
                out.push(Cmd::AddFrags { player: *player, frags: super::football::TOUCHDOWN_POINTS as i32 });
                out.push(Cmd::Feed(format!("{} {} a touchdown for {}!", w.display_name(*player), w.verb(*player, "scores", "score"), super::football::team_name(team))));
                out.push(Cmd::Shake(0.45));
                let mine = Some(team) == local;
                out.push(Cmd::Sound(if mine { "touchdown_win" } else { "touchdown_loss" }));
                if w.score[team.idx()] + 1 >= limit {
                    out.push(Cmd::EndMatch { message: if mine { "VICTORY" } else { "DEFEAT" }.into(), seconds: 8.0 });
                }
            }
            (Rules::Football, GameEvent::CarrierTackled { winner, .. }) => {
                out.push(Cmd::AddFrags { player: *winner, frags: super::football::TACKLE_POINTS as i32 });
            }
            (Rules::Deathmatch | Rules::TeamDeathmatch, GameEvent::Frag { killer, victim }) => {
                if w.state != MatchState::Playing { return; }
                let ffa = self.0 == Rules::Deathmatch;
                if killer == victim {
                    // Killing yourself costs a frag in Deathmatch.
                    if ffa { out.push(Cmd::AddFrags { player: *killer, frags: -1 }); }
                    return;
                }
                if !w.hostile(*killer, w.players[*killer].team, *victim) { return; }
                if ffa {
                    if w.players[*killer].frags >= limit { out.push(end_deathmatch(w, None)); }
                } else {
                    let t = w.players[*killer].team.idx();
                    out.push(Cmd::AddScore { team: t, points: 1 });
                    if w.score[t] + 1 >= limit { out.push(end_deathmatch(w, Some(t))); }
                }
            }
            (Rules::Deathmatch | Rules::TeamDeathmatch, GameEvent::ClockExpired) => out.push(end_deathmatch(w, None)),
            // Rabbit (the classic rules): a point for taking the flag, one each
            // HELD_POINT seconds held (all holds together), one for a hunter's
            // kill of the rabbit and two for the rabbit's kills (the engine
            // counts one; hunters can't hurt each other); a suicide costs
            // nothing. First to the limit, else the leader at the clock.
            (Rules::Rabbit, GameEvent::RabbitGrab { player } | GameEvent::RabbitHeld { player }) => {
                if w.state != MatchState::Playing { return; }
                if matches!(event, GameEvent::RabbitGrab { .. }) {
                    out.push(Cmd::Feed(format!("{} {} the rabbit", w.display_name(*player), w.verb(*player, "is", "are"))));
                }
                out.push(Cmd::AddFrags { player: *player, frags: 1 });
                if w.players[*player].frags + 1 >= limit { out.push(end_rabbit(w, Some(*player))); }
            }
            (Rules::Rabbit, GameEvent::Frag { killer, victim }) => {
                if w.state != MatchState::Playing || killer == victim { return; }
                let rabbit = w.rabbit.carrier.filter(|&c| w.rabbit.active && c < w.players.len());
                if rabbit == Some(*killer) {
                    out.push(Cmd::AddFrags { player: *killer, frags: 1 });
                    if w.players[*killer].frags + 1 >= limit { out.push(end_rabbit(w, Some(*killer))); }
                } else if rabbit == Some(*victim) && w.players[*killer].frags >= limit {
                    out.push(end_rabbit(w, None));
                }
            }
            (Rules::Rabbit, GameEvent::ClockExpired) => out.push(end_rabbit(w, None)),
            // Team Rabbit: every death costs the dead player's team a life
            // (the other team scores), two when a killer's team holds the flag.
            (Rules::TeamRabbit, GameEvent::Died { victim, killer }) => {
                if w.state != MatchState::Playing { return; }
                let gainer = w.players[*victim].team.other();
                let flag_team = w.rabbit.carrier.filter(|_| w.rabbit.active).and_then(|c| w.players.get(c)).map(|p| p.team);
                let bonus = killer.is_some_and(|k| k != *victim && w.players[k].team == gainer) && flag_team == Some(gainer);
                let points = if bonus { 2 } else { 1 };
                out.push(Cmd::AddScore { team: gainer.idx(), points });
                if w.score[gainer.idx()] + points >= limit {
                    out.push(Cmd::SetScore { team: gainer.idx(), points: limit });
                    out.push(end_team_rabbit(w, gainer.idx()));
                }
            }
            (Rules::TeamRabbit, GameEvent::ClockExpired) => out.push(end_deathmatch(w, None)),
            // Duel: a round goes to whoever is left standing (a death to the
            // map or by your own hand gives it away), once; the duel to the
            // first to the limit, or the leader when the clock runs out. A
            // duel server's lobby (no `duel` state) scores nothing.
            (Rules::Duel, GameEvent::Died { victim, .. }) => {
                let Some(duel) = w.duel.as_ref() else { return };
                if w.state != MatchState::Playing || duel.next_round.is_some() { return; }
                let team = w.players[*victim].team.other();
                let Some(winner) = duelist(w, team.idx()) else { return };
                let mut score = w.score;
                score[team.idx()] += 1;
                out.push(Cmd::AddScore { team: team.idx(), points: 1 });
                let name = w.display_name(winner);
                if score[team.idx()] >= limit {
                    out.push(Cmd::Feed(format!("{name} {} the duel {}–{}", w.verb(winner, "wins", "win"), score[team.idx()], score[1 - team.idx()])));
                    out.push(end_duel(w, Some(team.idx())));
                } else {
                    out.push(Cmd::Feed(format!("{name} {} round {}", w.verb(winner, "takes", "take"), score[0] + score[1])));
                    out.push(Cmd::NextRound { seconds: crate::sim::duel::ROUND_BREAK });
                }
            }
            (Rules::Duel, GameEvent::ClockExpired) => if w.duel.is_some() { out.push(end_duel(w, None)); },
            (Rules::Duel, _) => {}
            (_, GameEvent::ClockExpired) => out.push(Cmd::EndMatch { message: "TIME — MATCH OVER".into(), seconds: 4.0 }),
            _ => {}
        }
    }
}

/// The end of a Rabbit match: the leader (`scored`: a player about to score
/// the winning point, already counted as the leader).
fn end_rabbit(w: &World, scored: Option<usize>) -> Cmd {
    let message = match scored.or_else(|| w.ffa_leader()) {
        Some(i) if i == w.player_id => "YOU WIN".to_string(),
        Some(i) => format!("{} WINS", w.display_name(i).to_uppercase()),
        None => "MATCH OVER".to_string(),
    };
    Cmd::EndMatch { message, seconds: 8.0 }
}

/// The end of a duel: `winner` took the deciding round, or the clock ran
/// out (the leader wins, or it's a draw).
fn end_duel(w: &World, winner: Option<usize>) -> Cmd {
    let team = winner.or_else(|| match w.score[0].cmp(&w.score[1]) {
        std::cmp::Ordering::Greater => Some(0),
        std::cmp::Ordering::Less => Some(1),
        std::cmp::Ordering::Equal => None,
    });
    let message = match team.and_then(|t| duelist(w, t)) {
        Some(i) if i == w.player_id => "YOU WIN THE DUEL".to_string(),
        Some(i) => format!("{} WINS THE DUEL", w.display_name(i).to_uppercase()),
        None => "THE DUEL IS A DRAW".to_string(),
    };
    Cmd::EndMatch { message, seconds: 8.0 }
}

/// The player on team `team` of a duel (anyone in the slot: a person, a
/// bot, or an offline world's own player).
fn duelist(w: &World, team: usize) -> Option<usize> {
    w.players.iter().position(|p| (p.net_id != 0 || !p.remote) && p.team.idx() == team)
}

/// The end of a Team Rabbit match: team `winner` took the other's last life.
fn end_team_rabbit(w: &World, winner: usize) -> Cmd {
    let mine = w.players.get(w.player_id).map_or(0, |p| p.team.idx());
    Cmd::EndMatch { message: if mine == winner { "VICTORY" } else { "DEFEAT" }.into(), seconds: 8.0 }
}

/// The end of a deathmatch: the leader (free-for-all), or victory, defeat or
/// a draw for this player's team (`scored`: a team frag about to be added).
fn end_deathmatch(w: &World, scored: Option<usize>) -> Cmd {
    let message = if w.ffa() {
        match w.ffa_leader() {
            Some(i) if i == w.player_id => "YOU WIN THE DEATHMATCH".to_string(),
            Some(i) => format!("{} WINS THE DEATHMATCH", w.display_name(i).to_uppercase()),
            None => "MATCH OVER".to_string(),
        }
    } else {
        let mut score = w.score;
        if let Some(t) = scored { score[t] += 1; }
        let mine = w.players.get(w.player_id).map_or(0, |p| p.team.idx());
        if score[mine] > score[1 - mine] { "VICTORY".into() } else if score[mine] < score[1 - mine] { "DEFEAT".into() } else { "DRAW".into() }
    };
    Cmd::EndMatch { message, seconds: 8.0 }
}

impl World {
    /// Report `event` to the mode's rule set and apply what it answers. Never
    /// during prediction: rules are the server's (or an offline match's).
    pub(crate) fn rule_event(&mut self, event: GameEvent) {
        if self.predicting { return; }
        let mut out = Vec::new();
        match self.rules.take() {
            Some(mut rules) => { rules.event(self, &event, &mut out); self.rules = Some(rules); }
            None => builtin(self.mode.rules()).event(self, &event, &mut out),
        }
        for cmd in out { self.apply_rule(cmd); }
        // Whatever a script did, a match whose clock ran out is over.
        if event == GameEvent::ClockExpired && self.state != MatchState::Ended {
            let mut out = Vec::new();
            builtin(self.mode.rules()).event(self, &event, &mut out);
            for cmd in out { self.apply_rule(cmd); }
        }
    }

    pub(crate) fn apply_rule(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::AddScore { team, points } => if let Some(s) = self.score.get_mut(team) { *s += points; },
            Cmd::SetScore { team, points } => if let Some(s) = self.score.get_mut(team) { *s = points; },
            Cmd::AddFrags { player, frags } => if let Some(p) = self.players.get_mut(player) {
                p.frags = if frags < 0 { p.frags.saturating_sub(frags.unsigned_abs()) } else { p.frags + frags as u32 };
            },
            Cmd::EndMatch { message, seconds } => {
                self.state = MatchState::Ended;
                self.msg(&message, seconds);
                self.push_event("end");
            }
            Cmd::Message { text, seconds } => self.msg(&text, seconds),
            Cmd::Sound(name) => self.push_event(name),
            Cmd::Feed(text) => self.play(text),
            Cmd::Shake(amount) => self.trauma = amount,
            Cmd::NextRound { seconds } => if let Some(d) = self.duel.as_mut() { d.next_round = Some(seconds); },
            other => self.apply_hook_cmd(other),
        }
    }
}
