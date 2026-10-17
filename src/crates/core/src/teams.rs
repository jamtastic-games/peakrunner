//! Team switching and spectators (marker `teams1`). The server owns every
//! rule here; clients only ask ("switch me", "let me watch", "let me play").
//!
//! - **Switching** moves a person to the other team when team sizes stay
//!   within the balance rule (at most one apart). On a bot server a team's
//!   size counts its bots, so a person may always take a bot's place on the
//!   other side: the bots move over to fill the gap. People may be uneven
//!   among themselves (friends can play on one side). Free-for-all has no
//!   teams; in Football a switch waits while a ball is in play. A carried
//!   flag or ball drops, and the player respawns on the new team with no
//!   death, frag or score counted.
//! - **Spectators** watch without a player slot: they are listed apart from
//!   the 28 player slots, up to `spectator_cap` of them (MAX_SPECTATORS at
//!   most). They see the public snapshot (no team chat), may use public
//!   chat, can't vote and don't count as voters. A spectator plays again
//!   when a player slot is free, on the team `join` picks. Someone who joined
//!   a full match, or asked to play while it was full, is *queued*: the
//!   server moves the longest-waiting queued spectator in when a slot frees.
//! - **Cooldown:** SWITCH_COOLDOWN between switches, and between becoming a
//!   spectator and playing again, per player and per address group (as
//!   votes are, so a reconnect doesn't reset it). Becoming a spectator is
//!   always allowed.
use serde::{Deserialize, Serialize};

use super::*;

/// Between one player's (or address's) team changes: 10 s at 60 Hz.
pub const SWITCH_COOLDOWN: u64 = 60 * 10;
/// Most spectators a match takes (a server may allow fewer).
pub const MAX_SPECTATORS: usize = 8;

/// A connected person watching instead of playing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spectator {
    pub net_id: u32,
    pub name: String,
    /// Server-only rate limits (ticks).
    #[serde(skip)]
    chat_next: u64,
    #[serde(skip)]
    rename_next: u64,
    /// Waiting to join a team as soon as a player slot is free.
    #[serde(default)]
    pub queued: bool,
    /// Tick they started waiting (the longest wait goes first).
    #[serde(skip)]
    queued_at: u64,
}

/// What a person takes from room to room on a duel server, whose rooms
/// share one clock (`Match::carried`, `Match::carry_in`): their chat and
/// rename limits, their address group, and a queued spectator's place in
/// the queue. Joining afresh would reset them all.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Carried { chat_next: u64, rename_next: u64, group: Option<u128>, queued_at: Option<u64> }

/// Who a cooldown belongs to: an address group, or a lone player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key { Address(u128), Player(u32) }

/// Team-change cooldowns, kept on the `Match` across maps and empty resets.
#[derive(Clone, Debug, Default)]
pub struct Switches { next: Vec<(Key, u64)> }

impl Match {
    fn switch_key(&self, net_id: u32) -> Key {
        self.votes.group(net_id).map_or(Key::Player(net_id), Key::Address)
    }

    fn switch_ready(&self, net_id: u32) -> Result<(), &'static str> {
        let key = self.switch_key(net_id);
        if self.switches.next.iter().any(|&(k, t)| k == key && self.tick < t) { return Err("you changed teams a moment ago; wait a few seconds"); }
        Ok(())
    }

    fn switched(&mut self, net_id: u32) {
        let (key, now) = (self.switch_key(net_id), self.tick);
        self.switches.next.retain(|&(k, t)| k != key && now < t);
        self.switches.next.push((key, now + SWITCH_COOLDOWN));
    }

    /// Team sizes for the balance rule, with one person from `moving` on the
    /// other side. People count; on a bot server so do the bots, which keep
    /// each side at `bots / 2` or at its people when there are more.
    pub(super) fn team_sizes(&self, moving: Option<Team>) -> [usize; 2] {
        let mut people = [0usize; 2];
        for p in self.world.players.iter().filter(|p| human(p)) { people[p.team.idx()] += 1; }
        if let Some(t) = moving {
            people[t.idx()] = people[t.idx()].saturating_sub(1);
            people[t.other().idx()] += 1;
        }
        let half = self.bots / 2;
        if self.bots > 0 { people.map(|n| n.max(half)) } else { people }
    }

    /// Put the player in `slot` on `team`: drop what they carry, clear their
    /// projectiles and respawn them there. Nothing is counted.
    pub(super) fn move_to_team(&mut self, slot: usize, team: Team) {
        if let Some(flag) = self.world.players[slot].carrying.take() {
            self.world.drop_flag(flag, self.world.players[slot].pos);
        }
        self.world.ball_lost(slot);
        // Out of any vehicle first: a seat kept would carry them back into
        // it on the other team at the next step.
        self.world.unseat(slot);
        self.world.discs.retain(|d| d.owner != slot);
        self.world.players[slot].team = team;
        self.world.respawn(slot);
        self.world.network_inputs[slot] = Input::default();
    }

    /// The person in `slot` asks to play for the other team.
    pub fn switch_team(&mut self, slot: usize) -> Result<Team, &'static str> {
        let Some(p) = self.world.players.get(slot).filter(|p| human(p)) else { return Err("not in the match") };
        let (id, from) = (p.net_id, p.team);
        if self.world.ffa() { return Err("free-for-all has no teams"); }
        if self.world.football() && self.phase == Phase::Playing && self.world.ball.in_play {
            return Err("switch between plays: wait for the next ball or round");
        }
        self.switch_ready(id)?;
        let [a, b] = self.team_sizes(Some(from));
        if a.abs_diff(b) > 1 { return Err("that would make the teams uneven"); }
        let to = from.other();
        self.move_to_team(slot, to);
        self.switched(id);
        let name = self.world.players[slot].name.clone();
        crate::feed::push(&mut self.world.feed, crate::feed::Entry::Play {
            text: format!("{name} switched to {}", if to == Team::Ember { "Ember" } else { "Glacier" }) });
        Ok(to)
    }

    /// Whether another spectator fits.
    pub fn spectator_place(&self) -> bool { self.spectators.len() < self.spectator_cap.min(MAX_SPECTATORS) }

    /// What person `id`, in `slot` or watching, would carry to another room.
    pub fn carried(&self, slot: Option<usize>, id: u32) -> Carried {
        let group = self.votes.group(id);
        match slot {
            Some(slot) => Carried { chat_next: self.chat_next[slot], rename_next: self.rename_next[slot], group, queued_at: None },
            None => self.spectators.iter().find(|s| s.net_id == id).map_or(Carried { group, ..Default::default() }, |s|
                Carried { chat_next: s.chat_next, rename_next: s.rename_next, group, queued_at: s.queued.then_some(s.queued_at) }),
        }
    }

    /// Person `id`, just seated in `slot` or watching, keeps what they
    /// carried in. A player's place in the spectator queue ends there.
    pub fn carry_in(&mut self, slot: Option<usize>, id: u32, c: Carried) {
        if let Some(group) = c.group { self.votes.set_address(id, group); }
        match slot {
            Some(slot) => { self.chat_next[slot] = c.chat_next; self.rename_next[slot] = c.rename_next; }
            None => if let Some(s) = self.spectators.iter_mut().find(|s| s.net_id == id) {
                (s.chat_next, s.rename_next) = (c.chat_next, c.rename_next);
                (s.queued, s.queued_at) = (c.queued_at.is_some(), c.queued_at.unwrap_or(0));
            },
        }
    }

    /// A new connection joins as a spectator.
    pub fn join_spectator(&mut self, id: u32, name: &str) -> Result<(), &'static str> {
        let name = crate::names::validate(name).ok_or("invalid name")?;
        if id == 0 || self.world.players.iter().any(|p| p.net_id == id) || self.spectating(id) { return Err("already here"); }
        if !self.spectator_place() { return Err("no room for more spectators"); }
        self.spectators.push(Spectator { net_id: id, name: name.into(), chat_next: 0, rename_next: 0, queued: false, queued_at: 0 });
        Ok(())
    }

    /// Spectator `id` waits for a player slot (`on`), or stops waiting.
    pub fn queue_spectator(&mut self, id: u32, on: bool) {
        let tick = self.tick;
        if let Some(s) = self.spectators.iter_mut().find(|s| s.net_id == id) {
            if on && !s.queued { s.queued_at = tick; }
            s.queued = on;
        }
    }

    /// Queued spectators, longest-waiting first.
    pub fn queued_spectators(&self) -> Vec<u32> {
        let mut queue: Vec<&Spectator> = self.spectators.iter().filter(|s| s.queued).collect();
        queue.sort_by_key(|s| s.queued_at);
        queue.iter().map(|s| s.net_id).collect()
    }

    /// The person in `slot` stops playing and watches. Their slot empties
    /// (a carried flag or ball drops); identity and address stay.
    pub fn spectate(&mut self, slot: usize) -> Result<(), &'static str> {
        let Some(p) = self.world.players.get(slot).filter(|p| human(p)) else { return Err("not in the match") };
        if !self.spectator_place() { return Err("no room for more spectators"); }
        let (id, name) = (p.net_id, p.name.clone());
        self.switched(id);
        self.spectators.push(Spectator { net_id: id, name, chat_next: self.chat_next[slot], rename_next: self.rename_next[slot],
            queued: false, queued_at: 0 });
        self.vacate(slot);
        Ok(())
    }

    /// Spectator `id` asks to play, while the match holds fewer than
    /// `capacity` people. Returns their new slot.
    pub fn unspectate(&mut self, id: u32, capacity: usize) -> Result<usize, &'static str> {
        let Some(at) = self.spectators.iter().position(|s| s.net_id == id) else { return Err("not spectating") };
        self.switch_ready(id)?;
        if self.world.players.iter().filter(|p| human(p)).count() >= capacity { return Err("the match is full; wait for a free slot"); }
        let s = self.spectators.remove(at);
        let Some(slot) = self.join(id, &s.name) else {
            self.spectators.insert(at, s);
            return Err("the match is full; wait for a free slot");
        };
        self.chat_next[slot] = s.chat_next;
        self.rename_next[slot] = s.rename_next;
        Ok(slot)
    }

    /// Spectator `id` disconnected.
    pub fn leave_spectator(&mut self, id: u32) {
        self.spectators.retain(|s| s.net_id != id);
        self.votes.forget(id);
    }

    pub fn spectating(&self, id: u32) -> bool { self.spectators.iter().any(|s| s.net_id == id) }

    /// Public chat from spectator `id` (rate-limited as players are).
    pub fn spectator_chat(&mut self, id: u32, text: &str) -> bool {
        let tick = self.tick;
        let Some(s) = self.spectators.iter_mut().find(|s| s.net_id == id) else { return false };
        let Some(text) = crate::feed::message(text) else { return false };
        if tick < s.chat_next { return false; }
        s.chat_next = tick.saturating_add(60);
        let sender = s.name.clone();
        crate::feed::push(&mut self.world.feed, crate::feed::Entry::Chat { sender, text });
        true
    }

    pub fn rename_spectator(&mut self, id: u32, raw: &str) -> bool {
        let Some(name) = crate::names::validate(raw) else { return false };
        let tick = self.tick;
        let Some(s) = self.spectators.iter_mut().find(|s| s.net_id == id) else { return false };
        if tick < s.rename_next { return false; }
        if s.name == name { return true; }
        s.name = name.into();
        s.rename_next = tick.saturating_add(600);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map_catalog::SupportedMode;

    fn game(people: u32) -> Match {
        let mut m = Match::new(MapId::Valley);
        for id in 1..=people { m.join(id, "Skier").unwrap(); }
        m.phase = Phase::Playing;
        m
    }
    fn on(m: &Match, team: Team) -> usize { m.world.players.iter().filter(|p| human(p) && p.team == team).count() }
    fn slot(m: &Match, id: u32) -> usize { m.world.players.iter().position(|p| p.net_id == id).unwrap() }

    /// A rider who switches teams leaves the vehicle: a kept seat used to
    /// carry them back into it, on the other team, at the next step.
    #[test]
    fn switching_teams_leaves_the_vehicle_ridden() {
        use crate::sim::vehicles::{Craft, MAX_SEATS};
        let mut m = game(4);
        let a = slot(&m, 1);
        let from = m.world.players[a].team;
        let kind = crate::registry::registry().vehicles.index("skiff").expect("the stock skiff");
        let def = crate::registry::registry().vehicles.get(kind).unwrap();
        let mut riders = [0u8; MAX_SEATS];
        riders[0] = a as u8 + 1;
        let at = m.world.players[a].pos + Vec3::Y * 4.0;
        m.world.vehicles.push(Craft { kind: kind as u8, team: from, pos: at, vel: Vec3::ZERO, yaw: 0.0, pitch: 0.0,
            hull: def.hull, energy: def.energy_max, riders, cooldowns: [0.0; MAX_SEATS], idle: 0.0 });
        m.world.players[a].riding = 1;
        m.move_to_team(a, from.other());
        let commands = vec![None; m.world.players.len()];
        m.step(&commands);
        let p = &m.world.players[a];
        assert_eq!((p.team, p.riding), (from.other(), 0));
        assert!(m.world.vehicles.iter().all(|c| c.riders.iter().all(|&r| r as usize != a + 1)), "the seat is free");
        assert!(p.pos.distance(at) > 2.0, "they respawned, not back in the seat");
    }

    #[test]
    fn switches_keep_teams_within_one_and_are_not_undone() {
        // 2 v 2: a switch would make it 1 v 3.
        let mut m = game(4);
        assert_eq!(m.switch_team(slot(&m, 1)), Err("that would make the teams uneven"));
        // 3 v 2: someone on the bigger team may cross (to 2 v 3).
        m.join(5, "Fifth").unwrap();
        let bigger = if on(&m, Team::Ember) > on(&m, Team::Glacier) { Team::Ember } else { Team::Glacier };
        let mover = m.world.players.iter().position(|p| human(p) && p.team == bigger).unwrap();
        assert_eq!(m.switch_team(mover), Ok(bigger.other()));
        assert_eq!(m.world.players[mover].team, bigger.other());
        // Auto-balance leaves a legal switch alone.
        for _ in 0..30 { m.step(&[]); }
        assert_eq!(m.world.players[mover].team, bigger.other());
        assert_eq!((on(&m, bigger), on(&m, bigger.other())), (2, 3));
        // And the smaller side can't cross back.
        let small = m.world.players.iter().position(|p| human(p) && p.team == bigger).unwrap();
        m.tick += SWITCH_COOLDOWN;
        assert!(m.switch_team(small).is_err());
    }

    #[test]
    fn switching_counts_nothing_and_drops_the_flag() {
        let mut m = game(3);
        let s = m.world.players.iter().position(|p| human(p) && on(&m, p.team) == 2).unwrap();
        let team = m.world.players[s].team;
        let enemy_flag = team.other();
        m.world.players[s].carrying = Some(enemy_flag);
        m.world.flags[enemy_flag.idx()].carrier = Some(s);
        m.world.players[s].frags = 4; m.world.players[s].losses = 2;
        m.world.score = [1, 2];
        m.world.players[s].health = 20.0;
        m.switch_team(s).unwrap();
        let p = &m.world.players[s];
        assert_eq!((p.team, p.carrying, p.frags, p.losses, p.alive, p.health), (enemy_flag, None, 4, 2, true, 100.0));
        assert_eq!(m.world.flags[enemy_flag.idx()].carrier, None, "the flag dropped");
        assert_eq!(m.world.score, [1, 2]);
        assert!(!m.world.feed.iter().any(|e| matches!(e, crate::feed::Entry::Frag { .. })), "no death in the feed");
    }

    #[test]
    fn cooldown_is_per_player_and_per_address_through_a_reconnect() {
        let mut m = game(5);
        let set = |m: &mut Match, ember: &[u32]| for p in m.world.players.iter_mut().filter(|p| human(p)) {
            p.team = if ember.contains(&p.net_id) { Team::Ember } else { Team::Glacier };
        };
        set(&mut m, &[1, 2, 3]);
        m.votes.set_address(1, 7);
        m.switch_team(slot(&m, 1)).unwrap();
        // 3 v 2 again with player 1 on the bigger side: still cooling down.
        set(&mut m, &[2, 3]);
        assert_eq!(m.switch_team(slot(&m, 1)), Err("you changed teams a moment ago; wait a few seconds"));
        assert!(m.switch_team(slot(&m, 4)).is_ok(), "others aren't held up");
        // Reconnecting from the same address (a new net ID) doesn't reset it.
        let s = slot(&m, 1);
        m.leave(s);
        m.join(9, "Back").unwrap();
        m.votes.set_address(9, 7);
        set(&mut m, &[2, 3, 9]);
        assert!(m.switch_team(slot(&m, 9)).is_err(), "same address, still cooling down");
        m.tick += SWITCH_COOLDOWN;
        assert!(m.switch_team(slot(&m, 9)).is_ok());
    }

    fn bot_server(people: &[(u32, Team)]) -> Match {
        let mut m = Match::new(MapId::OldHoller);
        m.bots = 8;
        let none = vec![None; MAX_PLAYERS];
        m.step(&none);
        for &(id, team) in people {
            let s = m.join(id, "Person").unwrap();
            if m.world.players[s].team != team { m.move_to_team(s, team); }
        }
        m
    }
    fn size(m: &Match, t: Team) -> usize { m.world.players.iter().filter(|p| p.net_id != 0 && p.team == t).count() }

    #[test]
    fn a_bot_server_lets_a_person_take_a_bots_place_on_the_other_side() {
        // 1 v 1 people: moving A makes 0 v 2 people, but bots fill 4 v 4.
        let mut m = bot_server(&[(1, Team::Ember), (2, Team::Glacier)]);
        let none = vec![None; MAX_PLAYERS];
        m.step(&none);
        let a = slot(&m, 1);
        assert_eq!(m.switch_team(a), Ok(Team::Glacier));
        for _ in 0..30 { m.step(&none); }
        assert_eq!((size(&m, Team::Ember), size(&m, Team::Glacier)), (4, 4), "a bot crossed back the other way");
        assert_eq!(m.world.players[a].team, Team::Glacier, "balancing kept the swap");
        // 2 v 1 people may cross to 1 v 2, and that stands too.
        let mut m = bot_server(&[(1, Team::Ember), (2, Team::Ember), (3, Team::Glacier)]);
        m.step(&none);
        let b = slot(&m, 2);
        assert_eq!(m.switch_team(b), Ok(Team::Glacier));
        for _ in 0..30 { m.step(&none); }
        assert_eq!(m.world.players[b].team, Team::Glacier);
        assert_eq!((size(&m, Team::Ember), size(&m, Team::Glacier)), (4, 4));
    }

    #[test]
    fn friends_on_a_bot_server_stay_on_one_team() {
        // Two people with two bots against four bots: team sizes are even,
        // so nobody is balanced apart, however long they play.
        let mut m = bot_server(&[(1, Team::Ember), (2, Team::Ember)]);
        let none = vec![None; MAX_PLAYERS];
        for _ in 0..600 { m.step(&none); }
        assert_eq!((m.world.players[slot(&m, 1)].team, m.world.players[slot(&m, 2)].team), (Team::Ember, Team::Ember));
        assert_eq!((size(&m, Team::Ember), size(&m, Team::Glacier)), (4, 4));
        let bots_on = |t: Team| m.world.players.iter().filter(|p| p.is_bot && p.net_id != 0 && p.team == t).count();
        assert_eq!((bots_on(Team::Ember), bots_on(Team::Glacier)), (2, 4));
    }

    #[test]
    fn queued_spectators_wait_in_order() {
        let mut m = game(2);
        for id in [10, 11, 12] { m.join_spectator(id, "Watcher").unwrap(); }
        m.queue_spectator(11, true);
        m.tick += 5;
        m.queue_spectator(10, true);
        m.queue_spectator(11, true); // asking again keeps their place
        assert_eq!(m.queued_spectators(), vec![11, 10]);
        m.queue_spectator(11, false);
        assert_eq!(m.queued_spectators(), vec![10]);
        assert!(m.snapshot().spectators.iter().find(|s| s.net_id == 10).unwrap().queued, "clients see who is waiting");
        let s = m.unspectate(10, 8).unwrap();
        assert_eq!(m.world.players[s].net_id, 10);
        assert!(m.queued_spectators().is_empty());
    }

    #[test]
    fn football_blocks_a_switch_while_a_ball_is_in_play() {
        let mut m = game(3);
        m.world.set_mode(SupportedMode::Football);
        let s = m.world.players.iter().position(|p| human(p) && on(&m, p.team) == 2).unwrap();
        m.world.ball.active = true; m.world.ball.in_play = true;
        assert!(m.switch_team(s).unwrap_err().contains("between plays"));
        m.phase = Phase::Countdown;
        assert!(m.switch_team(s).is_ok(), "allowed before the round starts");
        // Between a score and the next ball, too.
        let mut m = game(3);
        m.world.set_mode(SupportedMode::Football);
        let s = m.world.players.iter().position(|p| human(p) && on(&m, p.team) == 2).unwrap();
        m.world.ball.active = true; m.world.ball.in_play = false;
        assert!(m.switch_team(s).is_ok());
        // Free-for-all has no teams to switch.
        let mut m = game(3);
        m.world.set_mode(SupportedMode::Deathmatch);
        assert!(m.switch_team(0).is_err());
    }

    #[test]
    fn spectators_take_no_player_slot_no_vote_and_no_team_chat() {
        let mut m = game(2);
        m.votes.choices = vec![(MapId::Valley, SupportedMode::Ctf), (MapId::OldHoller, SupportedMode::Ctf)];
        m.spectator_cap = 2;
        m.join_spectator(10, "Watcher").unwrap();
        m.join_spectator(11, "Other").unwrap();
        assert_eq!(m.join_spectator(12, "Third"), Err("no room for more spectators"));
        assert!(!m.world.players.iter().any(|p| p.net_id == 10), "not in a player slot");
        let snap = m.snapshot();
        assert_eq!(snap.spectators.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Watcher", "Other"]);
        // No votes: a spectator has no slot to vote from, and isn't a voter.
        assert!(m.call_vote(usize::MAX, 1).is_err());
        m.call_vote(0, 1).unwrap();
        m.step(&[]);
        assert_eq!(m.votes.current.as_ref().unwrap().voters, 2);
        // Team chat never reaches them; public chat goes both ways.
        assert!(m.chat_channel(0, "Secret", true));
        assert!(m.spectator_chat(10, "Nice shot"));
        assert!(!m.spectator_chat(10, "Spam"), "rate-limited");
        let feed = m.snapshot().feed;
        assert!(feed.iter().any(|e| e.line().contains("Nice shot")));
        assert!(!feed.iter().any(|e| e.line().contains("Secret")));
        // A spectator leaving empties their place.
        m.leave_spectator(11);
        assert!(m.join_spectator(12, "Third").is_ok());
    }

    #[test]
    fn players_become_spectators_and_back_keeping_identity() {
        let mut m = game(3);
        let s = slot(&m, 3);
        let team = m.world.players[s].team;
        m.world.players[s].carrying = Some(team.other());
        m.world.flags[team.other().idx()].carrier = Some(s);
        m.spectate(s).unwrap();
        assert!(m.spectating(3));
        assert_eq!(m.world.players[s].net_id, 0);
        assert_eq!(m.world.flags[team.other().idx()].carrier, None, "the flag dropped");
        assert_eq!(m.unspectate(3, 8), Err("you changed teams a moment ago; wait a few seconds"));
        m.tick += SWITCH_COOLDOWN;
        // A full match makes them wait.
        assert!(m.unspectate(3, 2).is_err());
        let back = m.unspectate(3, 8).unwrap();
        assert_eq!(m.world.players[back].net_id, 3);
        assert!(!m.spectating(3) && m.world.players[back].alive);
        // Spectators alone don't keep a match: everyone else leaving resets
        // it, but the spectators stay.
        let mut m = game(2);
        m.join_spectator(10, "Watcher").unwrap();
        m.leave(0); m.leave(1);
        assert!(m.spectating(10));
        m.rotate_to_mode(MapId::OldHoller, SupportedMode::Ctf);
        assert!(m.spectating(10), "spectators stay through a map change");
    }
}
