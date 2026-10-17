//! Duels (`duel1`; docs/duels.md): one on one, a life a round, first to the
//! mode's `score_limit` rounds. A duel server keeps a lobby (mode
//! `duel_lobby`: no weapons, no clock, always open, `Match::open`) and plays
//! each accepted challenge as its own small match, an arena
//! (`crates/server/src/duels.rs`). This module is the shared part: where a
//! round starts (a pair of facing duel spots), how a round ends (`rules.rs`
//! and `Match::next_duel_round`), and the board a duel server sends each
//! player (`Board`).
use super::*;
use serde::{Deserialize, Serialize};

/// Seconds from the blow that decides a round to the next round's countdown.
pub const ROUND_BREAK: f32 = 3.0;
/// The countdown before each later round (the first counts the mode's warmup).
pub const ROUND_COUNTDOWN: f32 = 3.0;
/// How far apart a random pair of spots is (m).
pub const SPOT_DISTANCE: [f32; 2] = [90.0, 140.0];
/// The map a duel server's lobby plays on when it's installed: Tenfold
/// Tower, ten floors of blade zones (`sim::blade`). Without it, the lobby is
/// the rotation's first map.
pub const LOBBY_MAP: &str = "tenfold";

/// The lobby map, if it's installed and loads.
pub fn lobby_map() -> Option<MapId> {
    MapId::parse(LOBBY_MAP).filter(|&m| crate::map_pack::on(m).is_some_and(|p| p.manifest.lobby))
}

/// The bot opponents a duel board offers, by archetype name
/// (`bot_nav::ARCHETYPES`; the Anchor holds ground, so it isn't offered).
pub const BOTS: [&str; 6] = ["Rookie", "Grunt", "Rider", "Skirmisher", "Ace", "Hawk"];

/// An arena's duel state (server and offline only, never in snapshots):
/// where this round starts, and the round's end once it's decided.
#[derive(Clone, Debug, Default)]
pub struct Duel {
    /// Team 0's spot and team 1's, each a position and a yaw (none: the
    /// map had no clear pair, and the teams' own spawns serve).
    pub spots: Option<[(Vec3, f32); 2]>,
    /// The round is decided: seconds left until the next one counts down.
    pub next_round: Option<f32>,
}

/// Where someone on a duel server is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place { Lobby, Dueling, Watching }

/// One person on a duel server, as the board lists them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Person {
    pub id: u32,
    pub name: String,
    pub place: Place,
    /// The duel they play or watch (0: none).
    pub arena: u32,
    /// Duels won and lost since they joined.
    pub wins: u32,
    pub losses: u32,
}

/// A duel being played.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Live {
    pub arena: u32,
    pub map: MapId,
    /// The challenger's name and the other's (a bot's archetype for a bot).
    pub names: [String; 2],
    pub score: [u32; 2],
    /// The second is a bot.
    pub bot: bool,
    pub watchers: u32,
}

/// A duel server's board for one player (`ServerMsg::Duels`), sent when it
/// changes: who's there and where, the duels being played, the bots on
/// offer, and the challenges to and from this player.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub people: Vec<Person>,
    pub duels: Vec<Live>,
    pub bots: Vec<String>,
    /// Who challenges you, by ID, with whole seconds left to answer.
    pub invites: Vec<(u32, u8)>,
    /// The challenge you made and is waiting: whom, and whole seconds left.
    pub challenge: Option<(u32, u8)>,
    /// Rounds that win a duel.
    pub rounds: u32,
}

impl World {
    /// Make this world an arena: duel rules in force, spots picked.
    pub fn begin_duel(&mut self) {
        self.duel = Some(Duel::default());
        self.pick_duel_spots();
    }

    /// Pick the spots this round starts from: one of the map's own pairs
    /// (`duel_spots` in its manifest) where it has any, else a random pair
    /// on open ground. The pair's ends are drawn at random for each team.
    pub fn pick_duel_spots(&mut self) {
        let authored = crate::map_pack::on(self.map).map_or(&[][..], |p| p.manifest.duel_spots.as_slice());
        let mut spots = if authored.is_empty() {
            let map = self.map;
            random_spots(map, || self.rng())
        } else {
            let [a, b] = authored[((self.rng() * authored.len() as f32) as usize).min(authored.len() - 1)];
            Some([(Vec3::new(a[0], a[1], a[2]), a[3]), (Vec3::new(b[0], b[1], b[2]), b[3])])
        };
        if self.rng() < 0.5 { if let Some(s) = spots.as_mut() { s.swap(0, 1); } }
        if let Some(duel) = self.duel.as_mut() { duel.spots = spots; }
    }

    /// Player `i`'s spot this round, in an arena that has one.
    pub(super) fn duel_spot(&self, i: usize) -> Option<(Vec3, f32)> {
        let spots = self.duel.as_ref()?.spots?;
        Some(spots[self.players.get(i)?.team.idx()])
    }

    /// Count down a decided round's break; true when the next round is due.
    pub fn duel_round_due(&mut self, dt: f32) -> bool {
        let Some(left) = self.duel.as_mut().and_then(|d| d.next_round.as_mut()) else { return false };
        *left -= dt;
        *left <= 0.0
    }
}

/// A random pair of facing spots on `map`, SPOT_DISTANCE apart: around
/// the middle of its two bases, on ground a player can stand on, under
/// open sky, clear of walls, water and lava. Pairs that can see each other
/// are tried first. None if nothing fits.
pub fn random_spots(map: MapId, mut roll: impl FnMut() -> f32) -> Option<[(Vec3, f32); 2]> {
    let info = crate::terrain::info(map);
    let middle = (info.ember + info.glacier) * 0.5;
    let reach = (info.ember.distance(info.glacier) * 0.55).max(120.0);
    for attempt in 0..900 {
        // Around the middle first, then anywhere on the map; in sight of
        // each other first, then not.
        let centre = if attempt < 300 {
            let (r, a) = (reach * roll().sqrt(), roll() * std::f32::consts::TAU);
            middle + Vec3::new(a.cos(), 0.0, a.sin()) * r
        } else { Vec3::new(roll() * info.size, 0.0, roll() * info.size) };
        let d = SPOT_DISTANCE[0] + (SPOT_DISTANCE[1] - SPOT_DISTANCE[0]) * roll();
        let t = roll() * std::f32::consts::TAU;
        let half = Vec3::new(t.cos(), 0.0, t.sin()) * (d * 0.5);
        let (Some(p), Some(q)) = (open_spot(map, centre + half), open_spot(map, centre - half)) else { continue };
        if attempt < 600 && !sight(map, p, q) { continue; }
        return Some([(p, facing(p, q)), (q, facing(q, p))]);
    }
    None
}

/// The yaw that looks from `from` toward `to` (`look_dir`'s convention).
fn facing(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

/// A standing point (1.2 m over the ground, or over skim water, as spawns
/// stand) at `at`'s x and z, if a duel can start there.
fn open_spot(map: MapId, at: Vec3) -> Option<Vec3> {
    let size = crate::terrain::info(map).size;
    const MARGIN: f32 = 48.0;
    if !(MARGIN..=size - MARGIN).contains(&at.x) || !(MARGIN..=size - MARGIN).contains(&at.z) { return None; }
    let ground = crate::terrain::height_on(map, at.x, at.z);
    let top = crate::water::skim_surface(map, at.x, at.z).filter(|&s| s > ground);
    if top.is_none() && crate::terrain::normal_on(map, at.x, at.z).y < 0.85 { return None; }
    let p = Vec3::new(at.x, top.unwrap_or(ground) + 1.2, at.z);
    // Never on lava, never under water.
    if crate::water::lava_burn(map, p - Vec3::Y * 1.5).is_some() || crate::water::lava_burn(map, p).is_some()
        || crate::water::at(map, &[], p).is_some() { return None; }
    if let Some(pack) = crate::map_pack::on(map) {
        if top.is_none() && pack.hole(p.x, p.z) { return None; }
        // Open sky above, and room to move off the spot every way.
        if pack.sweep(p - Vec3::Y * 0.3, p + Vec3::Y * 100.0, crate::terrain::PLAYER_RADIUS).is_some() { return None; }
        if [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z].into_iter().any(|axis| pack.sweep(p, p + axis * 5.0, crate::terrain::PLAYER_RADIUS).is_some()) {
            return None;
        }
    }
    Some(p)
}

/// Whether eyes at `p` and `q` see each other past the ground and the map's structures.
fn sight(map: MapId, p: Vec3, q: Vec3) -> bool {
    let (a, b) = (p + Vec3::Y * 0.6, q + Vec3::Y * 0.6);
    crate::terrain::segment_hit(map, a, b, 0.0).is_none()
        && crate::map_pack::on(map).is_none_or(|pack| pack.sweep(a, b, 0.05).is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    /// Every stock map but the stadiums has random spots, apart, facing,
    /// standing on the ground.
    #[test]
    fn every_duel_map_has_facing_spots_apart() {
        let maps = crate::map_catalog::maps_for(SupportedMode::Duel);
        assert!(maps.len() >= 2, "duel maps: {maps:?}");
        for map in maps {
            let mut w = World::new();
            w.set_map(map);
            w.reseed(7);
            for _ in 0..6 {
                let [(p, yp), (q, yq)] = random_spots(map, || w.rng()).unwrap_or_else(|| panic!("{map:?}: no duel spots"));
                let d = (p - q).with_y(0.0).length();
                assert!((SPOT_DISTANCE[0] - 0.1..=SPOT_DISTANCE[1] + 0.1).contains(&d), "{map:?}: {d} m apart");
                for (spot, yaw, other) in [(p, yp, q), (q, yq, p)] {
                    let ground = crate::terrain::support_on(map, spot - Vec3::Y * 0.6).0;
                    assert!((spot.y - ground - 1.2).abs() < 0.05, "{map:?}: {spot} stands 1.2 m over the ground ({ground})");
                    let look = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
                    assert!(look.dot((other - spot).with_y(0.0).normalize()) > 0.999, "{map:?}: faces the other spot");
                }
            }
        }
    }

    fn arena(map: MapId) -> Match {
        let mut m = Match::new(map);
        m.world.set_mode(SupportedMode::Duel);
        m.world.begin_duel();
        assert_eq!(m.join_on(1, "Ada", Team::Ember), Some(0));
        assert_eq!(m.join_on(2, "Bo", Team::Glacier), Some(1));
        m
    }

    fn run(m: &mut Match, seconds: f32) {
        for _ in 0..(seconds / STEP).round() as usize { m.step(&[None, None]); }
    }

    fn at_spots(m: &Match) {
        let spots = m.world.duel.as_ref().and_then(|d| d.spots).expect("spots");
        for i in 0..2 {
            let p = &m.world.players[i];
            assert!(p.alive && p.pos.with_y(0.0).distance(spots[p.team.idx()].0.with_y(0.0)) < 0.5, "{} at {} not its spot", p.name, p.pos);
        }
    }

    /// A duel: each round from a pair of spots, a life each, the score
    /// carried through the next round's countdown, light armor's kit, the
    /// end at the third round won.
    #[test]
    fn a_duel_plays_rounds_to_the_limit() {
        let mut m = arena(MapId::OldHoller);
        run(&mut m, STEP);
        assert_eq!(m.phase, Phase::Countdown);
        at_spots(&m);
        assert!(m.world.players.iter().take(2).all(|p| p.armor == ArmorClass::Light && p.pack.is_none()), "light's default kit");
        run(&mut m, SupportedMode::Duel.def().warmup + 0.1);
        assert_eq!(m.phase, Phase::Playing);
        let first = m.world.duel.as_ref().and_then(|d| d.spots).unwrap();
        for (round, (victim, killer)) in [(1usize, 0usize), (0, 1), (1, 0)].into_iter().enumerate() {
            m.world.damage(victim, 500.0, "blast", Some(killer), "Disc launcher");
            run(&mut m, STEP);
            m.world.damage(killer, 500.0, "blast", Some(victim), "Disc launcher");
            run(&mut m, STEP);
            let ember = [1, 1, 2][round];
            let glacier = [0, 1, 1][round];
            assert_eq!(m.world.score, [ember, glacier], "round {}: one point, the trade after it doesn't count", round + 1);
            run(&mut m, ROUND_BREAK + 0.1);
            assert_eq!(m.phase, Phase::Countdown, "round {} counts down", round + 2);
            assert_eq!(m.world.score, [ember, glacier], "the score carries into the countdown");
            at_spots(&m);
            run(&mut m, ROUND_COUNTDOWN + 0.1);
            assert_eq!((m.phase, m.world.score), (Phase::Playing, [ember, glacier]));
        }
        assert_ne!(m.world.duel.as_ref().and_then(|d| d.spots).unwrap()[0].0, first[0].0, "later rounds start elsewhere");
        // The fourth is the decider.
        m.world.damage(1, 500.0, "blast", Some(0), "Disc launcher");
        run(&mut m, STEP * 2.0);
        assert_eq!(m.world.score, [3, 1]);
        assert_eq!(m.phase, Phase::Intermission, "the duel is over");
        assert!(m.world.feed.iter().any(|e| matches!(e, crate::feed::Entry::Play { text } if text.contains("Ada wins the duel 3–1"))),
            "{:?}", m.world.feed);
    }

    /// The lobby never counts down, scores or runs out of time, and its
    /// people carry no guns.
    #[test]
    fn the_lobby_stays_open() {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::DuelLobby);
        m.open = true;
        m.join(1, "Ada").unwrap();
        run(&mut m, 2.0);
        assert_eq!(m.phase, Phase::Playing, "no waiting for a second team");
        m.world.time_left = 0.5;
        run(&mut m, 1.0);
        assert_eq!((m.phase, m.world.state, m.world.score), (Phase::Playing, MatchState::Playing, [0, 0]));
        m.world.damage(0, 500.0, "blast", None, "Fall");
        run(&mut m, 4.0);
        assert!(m.world.players[0].alive && m.world.score == [0, 0], "a fall in the lobby scores nothing");
        assert!(!SupportedMode::DuelLobby.def().allows(0), "no disc launcher");
        // Everyone leaving resets it, still open.
        m.leave(0);
        assert!(m.open);
    }

    /// A bot opponent fights: against someone standing still, it takes
    /// the round, on open ground and from a map's middle.
    #[test]
    fn a_duel_bot_hunts_its_opponent_down() {
        for map in [MapId::OldHoller, MapId::Dustreach] {
            let mut m = Match::new(map);
            m.world.set_mode(SupportedMode::Duel);
            m.world.begin_duel();
            m.join_on(1, "Ada", Team::Ember).unwrap();
            m.add_duel_bot("Ace").unwrap();
            let mut seconds = 0.0;
            while m.world.score[1] == 0 && seconds < 90.0 { run(&mut m, 1.0); seconds += 1.0; }
            assert_eq!(m.world.score, [0, 1], "{map:?}: the Ace took the round ({seconds} s)");
            println!("{map:?}: the Ace won the round in {seconds} s");
        }
    }

    #[test]
    fn a_duel_bot_is_the_archetype_asked_for() {
        let mut m = Match::new(MapId::OldHoller);
        m.world.set_mode(SupportedMode::Duel);
        m.world.begin_duel();
        m.join_on(1, "Ada", Team::Ember).unwrap();
        let bot = m.add_duel_bot("Ace").unwrap();
        run(&mut m, 1.0);
        let p = &m.world.players[bot];
        assert!(p.is_bot && p.net_id != 0 && p.team == Team::Glacier && p.name == "Ace" && p.bot_profile.name == "Ace");
        assert_eq!(m.world.players.iter().filter(|p| p.net_id != 0).count(), 2, "the bot stays, alone");
        assert!(m.add_duel_bot("Nobody").is_none());
        assert!(BOTS.iter().all(|b| crate::bot_nav::ARCHETYPES.iter().any(|a| a.name == *b)));
    }
}
