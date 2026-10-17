//! Map/mode votes (marker `vote1`). Any player presses V and picks one of the
//! server's rotation entries; everyone then has VOTE_TIME to vote. The server
//! owns the rules: the choices, who voted, the tally and the result. Clients
//! only send "call this entry" and "yes/no".
//!
//! - One vote at a time. The caller votes yes. A player may call again only
//!   CALL_COOLDOWN after their last call. A passed vote leaves GAP before the
//!   next; a failed one FAILED_GAP (5 minutes), so votes can't be spammed.
//! - Passes at once when more than half the players vote yes; fails at once
//!   when half or more vote no; otherwise, when time runs out, passes if yes
//!   outnumbers no.
//! - A passed vote switches the match to that entry (the server moves the
//!   rotation there too, so the cycle carries on from it).
//! - One ballot per network address: the server tells the votes which
//!   address group each player connected from (`set_address`; IPv6 by /64).
//!   Players sharing one cast one ballot between them, count once among the
//!   voters, and share the caller cooldown, which a reconnect (a new net ID)
//!   therefore doesn't reset. Players with no address (LAN) count alone.
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::map_catalog::SupportedMode;
use crate::terrain::MapId;

/// How long everyone has to vote (s).
pub const VOTE_TIME: f32 = 60.0;
/// Between one player's calls (ticks at 60 Hz: 2 minutes).
pub const CALL_COOLDOWN: u64 = 60 * 120;
/// After a vote passes, before the next can be called (ticks: 20 s).
pub const GAP: u64 = 60 * 20;
/// After a vote fails, before anyone can call another (ticks: 5 minutes),
/// so a rejected change can't be called again and again.
pub const FAILED_GAP: u64 = 60 * 300;
/// How long the result stays on screen (s).
pub const RESULT_TIME: f32 = 6.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vote {
    pub map: MapId,
    pub mode: SupportedMode,
    pub caller: String,
    pub yes: u32,
    pub no: u32,
    /// Players who can vote right now.
    pub voters: u32,
    /// Seconds left to vote, or of the result display once decided.
    pub left: f32,
    /// None while open; then whether it passed.
    pub passed: Option<bool>,
    /// Net IDs that have voted, or whose address has (so each client knows
    /// if it may still vote).
    pub voted: Vec<u32>,
}

/// Who a ballot or a call belongs to: an address group, or a lone player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Voter { Address(u128), Player(u32) }

/// Server-side vote state kept on the `Match`.
#[derive(Default)]
pub struct Votes {
    /// Rotation entries players may vote for (set by the server).
    pub choices: Vec<(MapId, SupportedMode)>,
    pub current: Option<Vote>,
    /// Ballots by net ID (true = yes), at most one per voter.
    ballots: Vec<(u32, bool)>,
    /// Per voter: tick before which they may not call again.
    next_call: Vec<(Voter, u64)>,
    /// Address group of each connected player's net ID (server-set).
    addresses: HashMap<u32, u128>,
    next_any: u64,
    /// A passed vote's entry, for the server to switch to.
    pub result: Option<(MapId, SupportedMode)>,
}

impl Votes {
    /// Record the address group `net_id` connected from (see the module notes).
    pub fn set_address(&mut self, net_id: u32, group: u128) { self.addresses.insert(net_id, group); }

    /// The address group `net_id` connected from, if known.
    pub fn group(&self, net_id: u32) -> Option<u128> { self.addresses.get(&net_id).copied() }

    /// Fresh votes that remember only the addresses (an emptied match whose
    /// spectators are still connected).
    pub fn carry_addresses(&mut self) -> Self { Self { addresses: std::mem::take(&mut self.addresses), ..Self::default() } }

    /// Forget a departed player's address.
    pub fn forget(&mut self, net_id: u32) { self.addresses.remove(&net_id); }

    fn voter(&self, net_id: u32) -> Voter {
        self.addresses.get(&net_id).map_or(Voter::Player(net_id), |&a| Voter::Address(a))
    }

    /// Start a vote for `choice` (an index into `choices`), called by `net_id`.
    pub fn call(&mut self, net_id: u32, name: &str, choice: usize, now: u64, players: u32,
                current: (MapId, SupportedMode)) -> Result<(), &'static str> {
        if self.current.as_ref().is_some_and(|v| v.passed.is_none()) { return Err("a vote is already running"); }
        let &(map, mode) = self.choices.get(choice).ok_or("not in this server's rotation")?;
        if (map, mode) == current { return Err("that is the current match"); }
        if now < self.next_any { return Err("wait a moment before the next vote"); }
        let voter = self.voter(net_id);
        if self.next_call.iter().any(|&(v, t)| v == voter && now < t) { return Err("you called a vote recently"); }
        self.next_call.retain(|&(v, t)| v != voter && now < t);
        self.next_call.push((voter, now + CALL_COOLDOWN));
        self.ballots = vec![(net_id, true)];
        self.current = Some(Vote { map, mode, caller: name.into(), yes: 1, no: 0, voters: players.max(1),
            left: VOTE_TIME, passed: None, voted: vec![net_id] });
        Ok(())
    }

    /// Seconds until anyone may call the next vote (0 when one can be).
    pub fn wait(&self, now: u64) -> f32 { self.next_any.saturating_sub(now) as f32 / 60.0 }

    /// Record or change `net_id`'s ballot; refused when someone else from
    /// the same address has already voted.
    pub fn cast(&mut self, net_id: u32, yes: bool) -> bool {
        if !self.current.as_ref().is_some_and(|v| v.passed.is_none()) { return false; }
        let voter = self.voter(net_id);
        if self.ballots.iter().any(|&(id, _)| id != net_id && self.voter(id) == voter) { return false; }
        self.ballots.retain(|&(id, _)| id != net_id);
        self.ballots.push((net_id, yes));
        true
    }

    /// Advance by `dt` with the net IDs currently in the match; decides the
    /// vote when it's settled. A departed player's ballot no longer counts.
    pub fn step(&mut self, dt: f32, present: &[u32], now: u64) {
        let Some(v) = self.current.as_mut() else { return };
        if v.passed.is_some() {
            v.left -= dt;
            if v.left <= 0.0 { self.current = None; }
            return;
        }
        self.ballots.retain(|(id, _)| present.contains(id));
        v.yes = self.ballots.iter().filter(|b| b.1).count() as u32;
        v.no = self.ballots.iter().filter(|b| !b.1).count() as u32;
        let addresses = &self.addresses;
        let voter = |id: u32| addresses.get(&id).map_or(Voter::Player(id), |&a| Voter::Address(a));
        let mut voters: Vec<Voter> = present.iter().map(|&id| voter(id)).collect();
        voters.sort_by_key(|v| match *v { Voter::Address(a) => (0, a), Voter::Player(id) => (1, id as u128) });
        voters.dedup();
        v.voters = voters.len().max(1) as u32;
        let cast: Vec<Voter> = self.ballots.iter().map(|b| voter(b.0)).collect();
        v.voted = present.iter().copied().filter(|&id| cast.contains(&voter(id))).collect();
        v.left -= dt;
        let decided = if v.yes * 2 > v.voters { Some(true) }
            else if v.no * 2 >= v.voters { Some(false) }
            else if v.left <= 0.0 { Some(v.yes > v.no) }
            else { None };
        if let Some(passed) = decided {
            v.passed = Some(passed);
            v.left = RESULT_TIME;
            self.next_any = now + if passed { GAP } else { FAILED_GAP };
            if passed { self.result = Some((v.map, v.mode)); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn votes() -> Votes {
        Votes { choices: vec![(MapId::OldHoller, SupportedMode::Ctf), (MapId::Reefbreak, SupportedMode::TeamDeathmatch)], ..Default::default() }
    }
    const NOW: (MapId, SupportedMode) = (MapId::OldHoller, SupportedMode::Ctf);

    #[test]
    fn only_rotation_entries_one_at_a_time_and_callers_cool_down() {
        let mut v = votes();
        assert!(v.call(1, "A", 0, 0, 4, NOW).is_err(), "not the current match");
        assert!(v.call(1, "A", 7, 0, 4, NOW).is_err(), "not in the rotation");
        assert!(v.call(1, "A", 1, 0, 4, NOW).is_ok());
        assert!(v.call(2, "B", 1, 0, 4, NOW).is_err(), "one at a time");
        v.cast(2, false); v.cast(3, false);
        v.step(0.1, &[1, 2, 3, 4], 10);
        assert_eq!(v.current.as_ref().unwrap().passed, Some(false), "half said no");
        assert!(v.result.is_none());
        assert!(v.call(2, "B", 1, 20, 4, NOW).is_err(), "gap after a vote");
        assert!(v.call(2, "B", 1, 10 + GAP, 4, NOW).is_err(), "a failed vote locks voting for longer");
        assert!((v.wait(10) - FAILED_GAP as f32 / 60.0).abs() < 0.1);
        assert!(v.call(2, "B", 1, 10 + FAILED_GAP, 4, NOW).is_ok(), "votes open again after the lock");
    }

    #[test]
    fn a_majority_passes_at_once_and_time_decides_otherwise() {
        let mut v = votes();
        v.call(1, "A", 1, 0, 5, NOW).unwrap();
        v.cast(2, true);
        v.step(0.1, &[1, 2, 3, 4, 5], 1);
        assert!(v.current.as_ref().unwrap().passed.is_none(), "2 of 5 isn't a majority yet");
        v.cast(3, true);
        v.step(0.1, &[1, 2, 3, 4, 5], 2);
        assert_eq!(v.current.as_ref().unwrap().passed, Some(true));
        assert_eq!(v.result, Some((MapId::Reefbreak, SupportedMode::TeamDeathmatch)));
        // Time runs out: yes beats no among those who voted.
        let mut v = votes();
        v.call(1, "A", 1, 0, 10, NOW).unwrap();
        v.cast(2, false);
        v.cast(3, true);
        let everyone: Vec<u32> = (1..=10).collect();
        for t in 0..=700 { v.step(0.1, &everyone, t); }
        assert!(v.result.is_some(), "2 yes, 1 no after a minute passes");
        assert!(v.current.is_none(), "result display ends");
    }

    #[test]
    fn one_ballot_one_voter_and_one_cooldown_per_address() {
        let mut v = votes();
        // Players 1-3 share an address; 4 and 5 are elsewhere; 6 has none (LAN).
        for id in 1..=3 { v.set_address(id, 7); }
        v.set_address(4, 8); v.set_address(5, 9);
        let everyone = [1, 2, 3, 4, 5, 6];
        v.call(1, "A", 1, 0, 6, NOW).unwrap();
        assert!(!v.cast(2, true) && !v.cast(3, true), "the caller's address has voted");
        assert!(v.cast(1, true), "the caller may still change their own ballot");
        v.step(0.1, &everyone, 1);
        let vote = v.current.as_ref().unwrap();
        assert_eq!((vote.yes, vote.voters, vote.passed), (1, 4, None), "four addresses, one yes");
        assert_eq!(vote.voted, vec![1, 2, 3], "the whole address shows as voted");
        v.cast(4, true);
        v.step(0.1, &everyone, 2);
        assert!(v.current.as_ref().unwrap().passed.is_none(), "2 of 4 addresses isn't a majority");
        v.cast(6, true);
        v.step(0.1, &everyone, 3);
        assert_eq!(v.current.as_ref().unwrap().passed, Some(true));
        // A reconnect from the caller's address (a new net ID) still waits.
        v.set_address(10, 7);
        assert_eq!(v.call(10, "A again", 1, 3 + GAP, 6, NOW), Err("you called a vote recently"));
        // The address's ballot holder leaves: someone else there may vote.
        let mut v = votes();
        for id in 1..=2 { v.set_address(id, 7); }
        v.call(3, "C", 1, 0, 3, NOW).unwrap();
        assert!(v.cast(1, false));
        assert!(!v.cast(2, false));
        v.forget(1);
        v.step(0.1, &[2, 3], 1);
        assert!(v.cast(2, false), "the departed player's ballot no longer holds the address");
    }

    #[test]
    fn departed_players_ballots_drop_out() {
        let mut v = votes();
        v.call(1, "A", 1, 0, 3, NOW).unwrap();
        v.cast(2, true);
        v.step(0.1, &[1, 3, 4], 1);   // player 2 left before it counted
        let vote = v.current.as_ref().unwrap();
        assert_eq!((vote.yes, vote.voters, vote.passed), (1, 3, None));
    }
}
