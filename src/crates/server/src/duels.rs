//! Duel servers (`duel1`; docs/duels.md). A rotation of `duel` entries
//! makes one: everyone joins a lobby (`Match::open`, mode `duel_lobby`), and
//! each accepted challenge plays in an arena, its own small `Match` on a map
//! drawn from the rotation, stepped beside the lobby on the server's one
//! clock (so a move between them never takes a client back in time). The
//! server moves people between them: into an arena when a challenge is
//! accepted or a bot picked, back when the duel ends or is conceded, and
//! watchers from duel to duel. The board (`duel::Board`) tells each player
//! who's where. Clients only ask; every move is the server's.
use std::collections::{HashMap, HashSet};
use peakrunner_core::{map_catalog::SupportedMode, sim::{duel::{self, Board, Live, Person, Place}, human, teams::Carried, Command, Match, Phase, Player, Team, MAX_PLAYERS, STEP},
    terrain::MapId};
use crate::gameserver::Peer;
use crate::proto::ServerMsg;

/// Seconds a challenge waits for its answer.
pub const ANSWER_TIME: f32 = 20.0;
/// Seconds a finished duel's result stays up before its people go back
/// (under the intermission's 10 s, after which a match would start again).
pub const RESULT_TIME: f32 = 6.0;
/// Told to someone asking to watch a duel with every watcher's place taken.
const FULL_ARENA: &str = "That duel has no room for more watchers";
/// Duels at once: everyone paired off.
pub const MAX_ARENAS: usize = MAX_PLAYERS / 2;
/// An arena's rounds are numbered from its ID times this (`Match::round`),
/// so a move between rooms is always a new round to the client, which
/// resets its prediction there.
const ROUNDS_PER_ARENA: u32 = 1000;

/// What a person on a duel server asked for (`ClientMsg`'s duel messages).
pub(crate) enum Action { Challenge(u32), ChallengeBot(String), Answer(u32, bool), Withdraw, Watch(u32), BackToLobby }

pub struct Arena {
    pub id: u32,
    pub game: Match,
    /// This tick's commands, by slot, as the lobby's are.
    pub commands: Vec<Option<Command>>,
    /// The challenger's ID (Ember) and the other's (Glacier; 0: a bot),
    /// and their names as the duel began.
    duelists: [u32; 2],
    names: [String; 2],
    bot: bool,
    /// The result is in: seconds left before its people go back.
    over: Option<f32>,
}

struct Invite { from: u32, to: u32, left: f32 }

pub struct Duels {
    /// The arenas' maps, and their mode (`duel`, or a mod's variant).
    maps: Vec<MapId>,
    mode: SupportedMode,
    pub arenas: Vec<Arena>,
    invites: Vec<Invite>,
    /// Duels won and lost by ID, since each joined.
    records: HashMap<u32, (u32, u32)>,
    /// People watching a duel who hold a player's place (they'll come back
    /// to the lobby as players), for the server's capacity.
    away: HashSet<u32>,
    /// People who came to watch, watching a duel: each holds a spectator's
    /// place in the lobby to come back to, for the server's spectator cap.
    held: HashSet<u32>,
    next_id: u32,
    rng: u64,
}

/// How a duel was decided.
#[derive(Clone, Copy, PartialEq)]
enum How { Played, Conceded, Left }

impl Duels {
    pub fn new(maps: Vec<MapId>, mode: SupportedMode, seed: u64) -> Self {
        Self { maps, mode, arenas: Vec::new(), invites: Vec::new(), records: HashMap::new(), away: HashSet::new(), held: HashSet::new(), next_id: 0, rng: seed | 1 }
    }

    fn roll(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        self.rng = x;
        x
    }

    /// People in the arenas and lobby players away watching: with the
    /// lobby's, the server's head count against its capacity.
    pub fn people(&self) -> usize {
        self.arenas.iter().map(|a| a.game.world.players.iter().filter(|p| human(p)).count()).sum::<usize>() + self.away.len()
    }

    /// The arenas' mode.
    pub fn mode(&self) -> SupportedMode { self.mode }

    /// Lobby players away watching a duel.
    pub fn away(&self) -> usize { self.away.len() }

    /// Spectators away watching a duel: with the lobby's, the server's
    /// spectators against its cap.
    pub fn held(&self) -> usize { self.held.len() }

    /// Everyone playing in an arena (people and bots), for the status roster.
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.arenas.iter().flat_map(|a| a.game.world.players.iter().filter(|p| p.net_id != 0))
    }

    fn index(&self, arena: u32) -> Option<usize> { self.arenas.iter().position(|a| a.id == arena) }

    /// The match a connection in `arena` is in (0, or an arena gone: the lobby).
    pub fn room<'a>(&'a mut self, lobby: &'a mut Match, arena: u32) -> &'a mut Match {
        match self.index(arena) { Some(k) => &mut self.arenas[k].game, None => lobby }
    }
    pub fn room_ref<'a>(&'a self, lobby: &'a Match, arena: u32) -> &'a Match {
        match self.index(arena) { Some(k) => &self.arenas[k].game, None => lobby }
    }
    /// The commands of `arena`'s tick (None: the lobby's).
    pub fn commands(&mut self, arena: u32) -> Option<&mut Vec<Option<Command>>> {
        let k = self.index(arena)?;
        Some(&mut self.arenas[k].commands)
    }

    /// A new tick: no arena command yet.
    pub fn begin_tick(&mut self) {
        for a in &mut self.arenas { a.commands.fill(None); }
    }

    /// A connection leaves the server: out of its room, its challenges
    /// gone. An arena it played in is settled at the next `step`.
    pub fn depart(&mut self, lobby: &mut Match, peer: &mut Peer) {
        self.unseat(lobby, peer);
        self.invites.retain(|i| i.from != peer.id && i.to != peer.id);
        self.records.remove(&peer.id);
    }

    /// `peer`'s name in the room it's in.
    fn name(&self, lobby: &Match, peer: &Peer) -> String {
        let room = self.room_ref(lobby, peer.arena);
        match peer.slot {
            Some(slot) => room.world.players[slot].name.clone(),
            None => room.spectators.iter().find(|w| w.net_id == peer.id).map(|w| w.name.clone()).unwrap_or_default(),
        }
    }

    /// Take `peer` out of the room it's in, returning their name and what
    /// they carry to the next (`Carried`).
    fn unseat(&mut self, lobby: &mut Match, peer: &mut Peer) -> (String, Carried) {
        let name = self.name(lobby, peer);
        let room = self.room(lobby, peer.arena);
        let carried = room.carried(peer.slot, peer.id);
        match peer.slot.take() { Some(slot) => room.leave(slot), None => room.leave_spectator(peer.id) }
        peer.watching = false;
        peer.inputs.clear();
        self.away.remove(&peer.id);
        self.held.remove(&peer.id);
        (name, carried)
    }

    /// Seat `peer` in the lobby: as a player, unless they came to watch
    /// (their place held while they watched a duel).
    fn to_lobby(&mut self, lobby: &mut Match, peer: &mut Peer, (name, carried): &(String, Carried)) {
        peer.arena = 0;
        if !peer.watch_only {
            if let Some(slot) = lobby.join(peer.id, name) {
                lobby.carry_in(Some(slot), peer.id, *carried);
                peer.slot = Some(slot);
                return;
            }
        }
        match lobby.join_spectator(peer.id, name) {
            Ok(()) => { lobby.carry_in(None, peer.id, *carried); peer.watching = true; }
            Err(why) => log::warn!("duel: id={} has no place in the lobby: {why}", peer.id),
        }
    }

    /// Seat `peer` in arena `k`: on `team`, or watching (None).
    fn to_arena(&mut self, k: usize, peer: &mut Peer, (name, carried): &(String, Carried), team: Option<Team>) -> bool {
        let a = &mut self.arenas[k];
        match team {
            Some(team) => match a.game.join_on(peer.id, name, team) {
                Some(slot) => { a.game.carry_in(Some(slot), peer.id, *carried); peer.slot = Some(slot); a.commands[slot] = None; }
                None => return false,
            },
            None => {
                if a.game.join_spectator(peer.id, name).is_err() { return false; }
                a.game.carry_in(None, peer.id, *carried);
                peer.watching = true;
                if peer.watch_only { self.held.insert(peer.id); } else { self.away.insert(peer.id); }
            }
        }
        peer.arena = a.id;
        true
    }

    /// A fresh arena on a map drawn from the rotation, on the lobby's clock.
    fn open(&mut self, lobby: &Match) -> Option<usize> {
        if self.arenas.len() >= MAX_ARENAS || self.maps.is_empty() { return None; }
        self.next_id += 1;
        let id = self.next_id;
        let pick = self.roll() % self.maps.len() as u64;
        let map = self.maps[pick as usize];
        let mut game = Match::new(map);
        // Watchers per arena: as many as the server takes spectators.
        game.spectator_cap = lobby.spectator_cap;
        game.world.set_mode(self.mode);
        game.world.forecast = lobby.world.forecast;
        let seed = self.roll() as u32;
        game.world.reseed(seed);
        game.world.roll_conditions();
        game.world.begin_duel();
        game.tick = lobby.tick;
        game.round = id.wrapping_mul(ROUNDS_PER_ARENA);
        self.arenas.push(Arena { id, game, commands: vec![None; MAX_PLAYERS], duelists: [0, 0], names: Default::default(), bot: false, over: None });
        Some(self.arenas.len() - 1)
    }

    /// Start a duel: `a` (the challenger, on Ember) against `b` or a bot of
    /// `archetype`, both from the lobby.
    fn start(&mut self, lobby: &mut Match, peers: &mut [Peer], a: usize, b: Option<usize>, archetype: Option<&str>) -> Result<(), &'static str> {
        let k = self.open(lobby).ok_or("Every arena is busy: try again in a moment")?;
        let ids = [peers[a].id, b.map_or(0, |b| peers[b].id)];
        let mut names = vec![(a, self.unseat(lobby, &mut peers[a]))];
        let mut seated = self.to_arena(k, &mut peers[a], &names[0].1, Some(Team::Ember));
        match (b, archetype) {
            (Some(b), _) => {
                names.push((b, self.unseat(lobby, &mut peers[b])));
                seated &= self.to_arena(k, &mut peers[b], &names[1].1, Some(Team::Glacier));
            }
            (None, Some(archetype)) => seated &= self.arenas[k].game.add_duel_bot(archetype).is_some(),
            (None, None) => seated = false,
        }
        if !seated {
            // Undo: everyone back, the arena gone.
            self.close(lobby, peers, k);
            for (i, seat) in names { if !peers[i].joined() { self.to_lobby(lobby, &mut peers[i], &seat); } }
            return Err("The duel couldn't start: try again");
        }
        let arena = &mut self.arenas[k];
        let name = |t: usize| arena.game.world.players.iter().find(|p| p.net_id != 0 && p.team.idx() == t).map(|p| p.name.clone()).unwrap_or_default();
        arena.names = [name(0), name(1)];
        (arena.duelists, arena.bot) = (ids, b.is_none());
        self.invites.retain(|i| !ids.contains(&i.from) && !ids.contains(&i.to));
        log::info!("duel_start arena={} map={} ids={ids:?} bot={:?}", arena.id, arena.game.world.map.key(), archetype);
        Ok(())
    }

    /// Everyone in arena `k` back to the lobby, and the arena gone.
    fn close(&mut self, lobby: &mut Match, peers: &mut [Peer], k: usize) {
        let id = self.arenas[k].id;
        for peer in peers.iter_mut().filter(|p| p.arena == id && p.joined()) {
            let seat = self.unseat(lobby, peer);
            self.to_lobby(lobby, peer, &seat);
        }
        self.arenas.remove(k);
    }

    /// The duel in arena `k` is decided: `winner` (a team, None: a draw).
    fn settle(&mut self, lobby: &mut Match, peers: &mut [Peer], k: usize, winner: Option<usize>, how: How) {
        let a = &self.arenas[k];
        let names = a.names.clone();
        let score = a.game.world.score;
        let map = peakrunner_core::terrain::info(a.game.world.map).name;
        let line = match (winner, how) {
            (Some(w), How::Played) => format!("{} beat {} {}–{} on {map}", names[w], names[1 - w], score[w], score[1 - w]),
            (Some(w), How::Conceded) => format!("{} conceded to {} on {map}", names[1 - w], names[w]),
            (Some(w), How::Left) => format!("{} left: {} wins the duel on {map}", names[1 - w], names[w]),
            (None, _) => format!("{} and {} drew {}–{} on {map}", names[0], names[1], score[0], score[1]),
        };
        if let Some(w) = winner {
            let (won, lost) = (a.duelists[w], a.duelists[1 - w]);
            if won != 0 { self.records.entry(won).or_default().0 += 1; }
            if lost != 0 { self.records.entry(lost).or_default().1 += 1; }
            // The one still here hears why it ended early.
            let notice = match how { How::Conceded => Some(format!("{} conceded: you win", names[1 - w])), How::Left => Some(format!("{} left the duel: you win", names[1 - w])), How::Played => None };
            if let (Some(notice), Some(peer)) = (notice, peers.iter_mut().find(|p| p.id == won && won != 0)) {
                let _ = peer.wire.send(ServerMsg::Notice { message: notice });
            }
        }
        log::info!("duel_end arena={} winner={winner:?} score={score:?}", a.id);
        self.arenas[k].game.announce(line.clone());
        lobby.announce(line);
        self.arenas[k].over = Some(if how == How::Played { RESULT_TIME } else { 0.0 });
    }

    /// Apply what person `id` asked (the server, once per tick, in order).
    pub(crate) fn act(&mut self, lobby: &mut Match, peers: &mut [Peer], id: u32, action: Action) {
        let Some(me) = peers.iter().position(|p| p.id == id && p.joined()) else { return };
        let in_lobby = |p: &Peer| p.arena == 0 && p.slot.is_some();
        let result: Result<(), String> = match action {
            Action::Challenge(target) => {
                let found = peers.iter().position(|p| p.id == target && p.id != id && p.joined());
                match found {
                    _ if !in_lobby(&peers[me]) => Err("Challenge from the lobby: go back to it first".into()),
                    Some(t) if in_lobby(&peers[t]) => {
                        if self.invites.iter().any(|i| i.from == target && i.to == id) {
                            // They challenged you too: that's a yes.
                            self.start(lobby, peers, t, Some(me), None).map_err(str::to_string)
                        } else {
                            self.invites.retain(|i| i.from != id);
                            self.invites.push(Invite { from: id, to: target, left: ANSWER_TIME });
                            let from = lobby.world.players[peers[me].slot.unwrap_or(0)].name.clone();
                            let _ = peers[t].wire.send(ServerMsg::Notice { message: format!("{from} challenges you to a duel: open the duel board to answer") });
                            Ok(())
                        }
                    }
                    _ => Err("They can't take a challenge now".into()),
                }
            }
            Action::ChallengeBot(archetype) => match duel::BOTS.iter().find(|b| **b == archetype) {
                _ if !in_lobby(&peers[me]) => Err("Challenge from the lobby: go back to it first".into()),
                Some(archetype) => self.start(lobby, peers, me, None, Some(archetype)).map_err(str::to_string),
                None => Err("No such bot".into()),
            },
            Action::Answer(from, yes) => {
                let Some(at) = self.invites.iter().position(|i| i.from == from && i.to == id) else { return };
                let invite = self.invites.remove(at);
                let challenger = peers.iter().position(|p| p.id == invite.from && p.joined());
                match challenger {
                    Some(c) if !yes => {
                        // From wherever they are (watching, too).
                        let name = self.name(lobby, &peers[me]);
                        let _ = peers[c].wire.send(ServerMsg::Notice { message: format!("{name} declined your challenge") });
                        Ok(())
                    }
                    Some(c) if in_lobby(&peers[c]) && in_lobby(&peers[me]) => self.start(lobby, peers, c, Some(me), None).map_err(str::to_string),
                    _ if !yes => Ok(()),
                    _ => Err("They're busy now".into()),
                }
            }
            Action::Withdraw => { self.invites.retain(|i| i.from != id); Ok(()) }
            Action::Watch(arena) => {
                let peer = &peers[me];
                let order: Vec<u32> = self.arenas.iter().filter(|a| a.over.is_none()).map(|a| a.id).collect();
                // The next duel (round to the first) with room to watch; the
                // next one, full, if none has (a full one is never skipped
                // into by asking again).
                let room = |a: u32| a != peer.arena && self.index(a).is_some_and(|k| self.arenas[k].game.spectator_place());
                let target = if arena != 0 { Some(arena) } else {
                    let next = order.iter().copied().filter(|&a| a > peer.arena).chain(order.iter().copied().filter(|&a| a <= peer.arena));
                    next.clone().find(|&a| room(a)).or(next.clone().next())
                };
                match target.and_then(|t| self.index(t)) {
                    _ if peer.arena != 0 && peer.slot.is_some() => Err("Finish your duel first".into()),
                    None => Err(if order.is_empty() { "No duels are being played".into() } else { "That duel is over".into() }),
                    Some(k) if self.arenas[k].id == peer.arena => Ok(()),
                    // Full: they stay where they are.
                    Some(k) if !self.arenas[k].game.spectator_place() => Err(FULL_ARENA.into()),
                    Some(k) => {
                        let seat = self.unseat(lobby, &mut peers[me]);
                        if self.to_arena(k, &mut peers[me], &seat, None) { Ok(()) } else {
                            self.to_lobby(lobby, &mut peers[me], &seat);
                            Err(FULL_ARENA.into())
                        }
                    }
                }
            }
            Action::BackToLobby => match (self.index(peers[me].arena), peers[me].slot) {
                (None, _) => Ok(()),
                (Some(k), Some(slot)) => {
                    if self.arenas[k].over.is_none() {
                        let team = self.arenas[k].game.world.players[slot].team.idx();
                        self.settle(lobby, peers, k, Some(1 - team), How::Conceded);
                    }
                    self.close(lobby, peers, k);
                    Ok(())
                }
                (Some(_), None) => {
                    let seat = self.unseat(lobby, &mut peers[me]);
                    self.to_lobby(lobby, &mut peers[me], &seat);
                    Ok(())
                }
            },
        };
        if let Err(why) = result { let _ = peers[me].wire.send(ServerMsg::Notice { message: why }); }
    }

    /// Once per tick, after the lobby's step: challenges run out, every
    /// arena steps, and finished duels are settled and closed.
    pub fn step(&mut self, lobby: &mut Match, peers: &mut [Peer]) {
        for i in &mut self.invites { i.left -= STEP; }
        self.invites.retain(|i| i.left > 0.0);
        for a in &mut self.arenas {
            a.game.step(&a.commands);
            crate::gameserver::tell(&mut a.game, peers, a.id);
        }
        for k in (0..self.arenas.len()).rev() {
            let a = &self.arenas[k];
            let here = |id: u32| id == 0 || a.game.world.players.iter().any(|p| p.net_id == id);
            let gone: Vec<usize> = (0..2).filter(|&t| !here(a.duelists[t])).collect();
            if let Some(left) = a.over {
                if left <= 0.0 { self.close(lobby, peers, k); } else { self.arenas[k].over = Some(left - STEP); }
            } else if !gone.is_empty() {
                // A duelist left the server: the other wins (both gone: nobody).
                let winner = (gone.len() == 1).then(|| 1 - gone[0]);
                self.settle(lobby, peers, k, winner, How::Left);
                self.close(lobby, peers, k);
            } else if a.game.phase == Phase::Intermission {
                let s = a.game.world.score;
                let winner = match s[0].cmp(&s[1]) { std::cmp::Ordering::Greater => Some(0), std::cmp::Ordering::Less => Some(1), _ => None };
                self.settle(lobby, peers, k, winner, How::Played);
            }
        }
    }

    /// Send each person their board where it changed since the last.
    pub fn send_boards(&self, lobby: &Match, peers: &mut [Peer]) {
        let people: Vec<Person> = peers.iter().filter(|p| p.joined()).map(|p| {
            let name = self.name(lobby, p);
            let place = if p.slot.is_none() { Place::Watching } else if p.arena == 0 { Place::Lobby } else { Place::Dueling };
            let (wins, losses) = self.records.get(&p.id).copied().unwrap_or_default();
            Person { id: p.id, name, place, arena: self.index(p.arena).map_or(0, |_| p.arena), wins, losses }
        }).collect();
        let duels: Vec<Live> = self.arenas.iter().map(|a| {
            let name = |t: usize| a.game.world.players.iter().find(|p| p.net_id != 0 && p.team.idx() == t).map(|p| p.name.clone()).unwrap_or_default();
            Live { arena: a.id, map: a.game.world.map, names: [name(0), name(1)], score: a.game.world.score, bot: a.bot,
                watchers: a.game.spectators.len() as u32 }
        }).collect();
        let bots: Vec<String> = duel::BOTS.iter().map(|b| b.to_string()).collect();
        let rounds = self.mode.def().score_limit;
        for peer in peers.iter_mut().filter(|p| p.joined()) {
            let board = Board { people: people.clone(), duels: duels.clone(), bots: bots.clone(),
                invites: self.invites.iter().filter(|i| i.to == peer.id).map(|i| (i.from, i.left.ceil() as u8)).collect(),
                challenge: self.invites.iter().find(|i| i.from == peer.id).map(|i| (i.to, i.left.ceil() as u8)), rounds };
            let Ok(bytes) = serde_json::to_vec(&board) else { continue };
            if peer.board.as_deref() == Some(bytes.as_slice()) { continue; }
            if peer.wire.send(ServerMsg::Duels { board }).is_ok() { peer.board = Some(bytes); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gameserver::Wire, link::{LocalPeer, Outbound}, proto::ClientMsg};
    use std::{sync::{atomic::AtomicU32, mpsc, Arc}, time::Instant};

    /// A person's far end: what the server sent them.
    struct Ear { _inbound: mpsc::SyncSender<ClientMsg>, outbound: tokio::sync::mpsc::Receiver<Outbound> }
    impl Ear {
        fn notices(&mut self) -> Vec<String> {
            let mut said = Vec::new();
            while let Ok(message) = self.outbound.try_recv() {
                if let Outbound::Control(message) = message { if let ServerMsg::Notice { message } = *message { said.push(message); } }
            }
            said
        }
    }
    fn person(id: u32) -> (Peer, Ear) {
        let (in_tx, in_rx) = mpsc::sync_channel(4);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel(64);
        let mut peer = Peer::new(Wire::Local(LocalPeer { inbound: in_rx, outbound: out_tx, origin: None, rtt: Arc::new(AtomicU32::new(0)) }), Instant::now());
        peer.id = id;
        (peer, Ear { _inbound: in_tx, outbound: out_rx })
    }
    fn server() -> (Match, Duels) {
        let mut lobby = Match::new(MapId::OldHoller);
        lobby.world.set_mode(SupportedMode::DuelLobby);
        lobby.open = true;
        (lobby, Duels::new(vec![MapId::OldHoller], SupportedMode::Duel, 7))
    }
    /// `names[k]` joins the lobby as a player, or (`watch`) a spectator.
    fn join(lobby: &mut Match, names: &[(&str, bool)]) -> (Vec<Peer>, Vec<Ear>) {
        names.iter().enumerate().map(|(k, &(name, watch))| {
            let (mut peer, ear) = person(k as u32 + 1);
            if watch {
                lobby.join_spectator(peer.id, name).unwrap();
                (peer.watching, peer.watch_only) = (true, true);
            } else { peer.slot = lobby.join(peer.id, name); }
            (peer, ear)
        }).unzip()
    }

    /// Watching a duel and coming back, a person keeps their place in the
    /// spectator queue, their chat and rename limits and their address
    /// group, and so does a player going to a duel: the move used to rejoin
    /// them afresh, at the back, free to chat and rename again.
    #[test]
    fn a_watcher_comes_back_as_they_left() {
        let (mut lobby, mut d) = server();
        let (mut peers, _ears) = join(&mut lobby, &[("Ada", false), ("Bo", false), ("Cy", true)]);
        lobby.queue_spectator(3, true);
        lobby.votes.set_address(3, 77);
        assert!(lobby.spectator_chat(3, "hi"));
        assert!(lobby.chat(peers[0].slot.unwrap(), "good luck"));
        assert!(lobby.rename(peers[0].slot.unwrap(), "Ada Two") && lobby.rename_spectator(3, "Cyd"));
        lobby.votes.set_address(1, 55);
        // Di waits too, later.
        lobby.tick = 10;
        let (mut di, _ear) = person(4);
        lobby.join_spectator(4, "Di").unwrap();
        (di.watching, di.watch_only) = (true, true);
        lobby.queue_spectator(4, true);
        peers.push(di);
        d.act(&mut lobby, &mut peers, 1, Action::Challenge(2));
        d.act(&mut lobby, &mut peers, 2, Action::Answer(1, true));
        assert_eq!(d.arenas.len(), 1);
        let arena = d.arenas[0].id;
        assert!(!d.arenas[0].game.chat(peers[0].slot.unwrap(), "have fun"), "Ada's chat limit came along");
        assert!(!d.arenas[0].game.rename(peers[0].slot.unwrap(), "Ada Three"), "and the rename limit");
        assert_eq!(d.arenas[0].game.votes.group(1), Some(55), "and the address group");
        d.act(&mut lobby, &mut peers, 3, Action::Watch(0));
        assert_eq!((peers[2].arena, d.held()), (arena, 1), "Cy watches, holding a spectator's place");
        assert!(!d.arenas[0].game.spectator_chat(3, "again"), "so did Cy's");
        assert!(!d.arenas[0].game.rename_spectator(3, "Cyx"));
        d.act(&mut lobby, &mut peers, 3, Action::BackToLobby);
        assert_eq!((peers[2].arena, peers[2].watching, d.held()), (0, true, 0));
        assert_eq!(lobby.queued_spectators(), [3, 4], "Cy is still ahead of Di");
        assert_eq!(lobby.votes.group(3), Some(77));
        assert!(!lobby.rename_spectator(3, "Cyz"), "the rename limit came back too");
        assert!(!lobby.spectator_chat(3, "back"));
    }

    /// A duel with every watcher's place taken (as many as the server takes
    /// spectators) turns a watcher away where they stand: the lobby player
    /// asking used to leave and rejoin at a fresh spawn.
    #[test]
    fn a_full_duel_leaves_a_would_be_watcher_where_they_stand() {
        let (mut lobby, mut d) = server();
        lobby.spectator_cap = 1;
        let (mut peers, mut ears) = join(&mut lobby, &[("Ada", false), ("Bo", false), ("Cy", false), ("Di", false)]);
        d.act(&mut lobby, &mut peers, 1, Action::Challenge(2));
        d.act(&mut lobby, &mut peers, 2, Action::Answer(1, true));
        assert_eq!(d.arenas[0].game.spectator_cap, 1);
        d.act(&mut lobby, &mut peers, 3, Action::Watch(0));
        assert!(peers[2].watching && peers[2].arena != 0);
        let slot = peers[3].slot.unwrap();
        lobby.world.players[slot].pos.x += 3.0;
        let at = lobby.world.players[slot].pos;
        d.act(&mut lobby, &mut peers, 4, Action::Watch(0));
        assert_eq!((peers[3].arena, peers[3].slot), (0, Some(slot)));
        assert_eq!(lobby.world.players[slot].pos, at, "not respawned");
        assert_eq!(ears[3].notices(), [FULL_ARENA]);
    }

    /// "The next duel" passes over a full one: asking again kept landing on
    /// it, so a duel after it could be watched only from the board.
    #[test]
    fn the_next_duel_to_watch_is_one_with_room() {
        let (mut lobby, mut d) = server();
        lobby.spectator_cap = 1;
        let names = [("Ada", false), ("Bo", false), ("Cy", false), ("Di", false), ("Eve", false), ("Flo", false), ("Gus", false)];
        let (mut peers, mut ears) = join(&mut lobby, &names);
        for (from, to) in [(1, 2), (3, 4)] {
            d.act(&mut lobby, &mut peers, from, Action::Challenge(to));
            d.act(&mut lobby, &mut peers, to, Action::Answer(from, true));
        }
        let (first, second) = (d.arenas[0].id, d.arenas[1].id);
        d.act(&mut lobby, &mut peers, 5, Action::Watch(first));
        assert_eq!(peers[4].arena, first, "Eve fills the first duel");
        d.act(&mut lobby, &mut peers, 6, Action::Watch(0));
        assert_eq!(peers[5].arena, second, "Flo watches the one with room");
        assert!(ears[5].notices().is_empty());
        // Every duel full: turned away where they stand.
        d.act(&mut lobby, &mut peers, 7, Action::Watch(0));
        assert_eq!((peers[6].arena, peers[6].slot.is_some()), (0, true));
        assert_eq!(ears[6].notices(), [FULL_ARENA]);
    }

    /// Declining a challenge while watching a duel names you: the name was
    /// looked up in the lobby's player slots, which a watcher hasn't.
    #[test]
    fn a_watcher_who_declines_is_named() {
        let (mut lobby, mut d) = server();
        let (mut peers, mut ears) = join(&mut lobby, &[("Ada", false), ("Bo", false), ("Cy", false), ("Di", false)]);
        d.act(&mut lobby, &mut peers, 1, Action::Challenge(3));
        d.act(&mut lobby, &mut peers, 2, Action::Challenge(4));
        d.act(&mut lobby, &mut peers, 4, Action::Answer(2, true));
        d.act(&mut lobby, &mut peers, 3, Action::Watch(0));
        assert!(peers[2].slot.is_none() && peers[2].arena != 0, "Cy watches Bo and Di");
        d.act(&mut lobby, &mut peers, 3, Action::Answer(1, false));
        assert_eq!(ears[0].notices(), ["Cy declined your challenge"]);
    }
}
