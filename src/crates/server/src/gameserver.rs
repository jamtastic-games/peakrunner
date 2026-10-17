use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, TcpListener};
use std::sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, AtomicU32, Ordering}};
use std::thread;
use std::time::{Duration, Instant};
use peakrunner_core::{sim::{Match, MAX_PLAYERS, STEP, teams::MAX_SPECTATORS}, terrain::MapId};
use crate::input_buffer::InputBuffer;
use crate::link::{Connector, Frame, LocalPeer, Outbound};
use crate::proto::{ClientMsg, ServerMsg};
use crate::wire::{Framed, private_bind, invalid};

/// Longest client line on the loopback/LAN TCP link. Hello carries the
/// compatibility marker and a content digest (about 400 bytes), so this
/// leaves room while staying bounded.
const MAX_CLIENT_LINE: usize = 8192;
/// Connected and pending peers the game thread holds: every player and
/// spectator, and a few handshakes.
const MAX_PEERS: usize = MAX_PLAYERS + MAX_SPECTATORS + 8;

pub struct GameHost {
    listener: TcpListener, name: String, map: MapId, max_players: usize, password: String, bots: usize, forecast: peakrunner_core::conditions::Forecast, location: String,
    spectators: usize,
    /// The key of the client hosting this server (`with_host_key`).
    host_key: Option<String>,
    /// Counting the people who play (`with_stats`).
    stats: Option<crate::stats::Stats>,
    rotation: crate::rotation::Rotation,
    joins: mpsc::SyncSender<LocalPeer>, arrivals: mpsc::Receiver<LocalPeer>,
    /// The maps players need (`content`) and its digest, set by `spawn`.
    content: Vec<peakrunner_discovery::ContentItem>, content_digest: String,
    /// Where people who join first stand (`with_start`), and how many have.
    start: Option<(peakrunner_core::glam::Vec3, f32)>,
    started: u32,
    #[cfg(test)]
    pub(crate) accelerated_rounds: bool,
    /// Test staging: runs on the match just before each tick.
    #[cfg(test)]
    pub(crate) before_step: Option<Stage>,
}
#[cfg(test)]
pub(crate) type Stage = Box<dyn FnMut(&mut Match) + Send>;
pub struct GameHandle {
    stop: Arc<AtomicBool>, thread: Option<thread::JoinHandle<()>>,
    /// People in the match, bots aside (the status's `players` counts both).
    pub players: Arc<AtomicU32>,
    pub status: Arc<Mutex<peakrunner_discovery::MatchStatus>>,
}
impl Drop for GameHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}
/// A player connection: loopback/LAN TCP (JSON lines), or an in-process
/// QUIC relay session.
pub(crate) enum Wire { Tcp(Framed), Local(LocalPeer) }
impl Wire {
    fn receive(&mut self) -> io::Result<Vec<ClientMsg>> {
        match self {
            Wire::Tcp(framed) => framed.receive(),
            Wire::Local(peer) => {
                let mut messages = Vec::new();
                while messages.len() < 16 {
                    match peer.inbound.try_recv() {
                        Ok(message) => messages.push(message),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) if messages.is_empty() => return Err(io::ErrorKind::UnexpectedEof.into()),
                        Err(mpsc::TryRecvError::Disconnected) => break,
                    }
                }
                Ok(messages)
            }
        }
    }
    pub(crate) fn send(&mut self, message: ServerMsg) -> io::Result<()> {
        match self {
            Wire::Tcp(framed) => framed.send(&message),
            Wire::Local(peer) => peer.outbound.try_send(Outbound::Control(Box::new(message))).map_err(|e| match e {
                tokio::sync::mpsc::error::TrySendError::Full(_) => invalid("slow consumer"),
                tokio::sync::mpsc::error::TrySendError::Closed(_) => io::ErrorKind::UnexpectedEof.into(),
            }),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self { Wire::Tcp(framed) => framed.flush(), Wire::Local(_) => Ok(()) }
    }
    fn origin(&self) -> Option<IpAddr> {
        match self { Wire::Tcp(_) => None, Wire::Local(peer) => peer.origin }
    }
}
pub(crate) struct Peer {
    pub(crate) wire: Wire, pub(crate) slot: Option<usize>, accepted: Instant, last: Instant,
    pub(crate) inputs: InputBuffer, last_seq: u64, tokens: f32,
    /// Net ID once welcomed; a spectator has one but no slot.
    pub(crate) id: u32, pub(crate) watching: bool,
    /// The anonymous game ID it sent (`stats`), counted once it plays.
    player: String,
    /// On a duel server (`duels`): the arena it's in (0: the lobby), whether
    /// it chose to watch rather than play, and the board last sent.
    pub(crate) arena: u32, pub(crate) watch_only: bool, pub(crate) board: Option<Vec<u8>>,
}
impl Peer {
    pub(crate) fn new(wire: Wire, now: Instant) -> Self {
        Self { wire, slot: None, accepted: now, last: now, inputs: InputBuffer::new(), last_seq: 0, tokens: 32.0, id: 0, watching: false, player: String::new(),
            arena: 0, watch_only: false, board: None }
    }
    pub(crate) fn joined(&self) -> bool { self.slot.is_some() || self.watching }
}
/// Compare host keys in time independent of where they differ.
fn same_key(expected: &str, given: &str) -> bool {
    expected.len() == given.len() && expected.bytes().zip(given.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}
/// Snapshot views: team chat reaches only its team, so there are three
/// (Ember, Glacier, no team); every peer with a view shares its encoding.
fn view(game: &Match, slot: usize) -> usize {
    game.world.players.get(slot).filter(|p| p.net_id != 0).map_or(2, |p| p.team.idx())
}
impl GameHost {
    pub fn bind(addr: &str, name: &str, max_players: u32, map: &str) -> io::Result<Self> {
        let map = MapId::parse(map).ok_or_else(|| invalid(&crate::rotation::unknown_map(map)))?;
        if !cfg!(test) && map == MapId::Valley { return Err(invalid(&format!("Valley is retired; {}", crate::rotation::AVAILABLE))); }
        if !(2..=MAX_PLAYERS as u32).contains(&max_players) { return Err(invalid(&format!("capacity must be 2–{MAX_PLAYERS}"))); }
        if name.is_empty() || name.len() > 64 || name.chars().any(char::is_control) { return Err(invalid("invalid match name")); }
        let listener = TcpListener::bind(private_bind(addr)?)?;
        listener.set_nonblocking(true)?;
        let (joins, arrivals) = mpsc::sync_channel(16);
        Ok(Self { listener, name: name.into(), map, max_players: max_players as usize, password: String::new(), bots: 0, forecast: Default::default(), location: String::new(),
            spectators: MAX_SPECTATORS, host_key: None, stats: None, rotation: crate::rotation::Rotation::single(map),
            joins, arrivals, content: Vec::new(), content_digest: String::new(),
            start: None, started: 0,
            #[cfg(test)] accelerated_rounds: false, #[cfg(test)] before_step: None })
    }
    /// Opens in-process sessions for the QUIC relays (see `link`).
    pub fn connector(&self) -> Connector { Connector { joins: self.joins.clone() } }
    pub fn with_rotation(mut self, json: &str) -> io::Result<Self> {
        self.rotation = crate::rotation::Rotation::parse(json).map_err(|e| invalid(&e))?;
        let mut maps: Vec<MapId> = self.rotation.choices().into_iter().map(|(m, _)| m).collect();
        maps.dedup();
        maps.sort(); maps.dedup();
        if maps.len() > peakrunner_discovery::MAX_CONTENT {
            return Err(invalid(&format!("a rotation may use at most {} different maps", peakrunner_discovery::MAX_CONTENT)));
        }
        self.map = self.rotation.current();
        Ok(self)
    }
    /// Play the rotation in a random order, new each time through
    /// (`rotation_order: "random"`; `seed` drawn at start).
    pub fn with_random_order(mut self, seed: u64) -> Self {
        self.rotation.shuffle(seed);
        self.map = self.rotation.current();
        self
    }
    pub fn with_password(mut self, password: String) -> Self { self.password = password; self }
    /// Where the server is, for lists ("New York, US"; at most 40 plain characters).
    pub fn with_location(mut self, location: &str) -> io::Result<Self> {
        let location = location.trim();
        if !peakrunner_discovery::valid_location(location) { return Err(invalid("location: up to 40 letters, digits, spaces, commas, periods or hyphens")); }
        self.location = location.into();
        Ok(self)
    }
    /// QA captures (launch_smoke's `QA_START`): everyone who joins first
    /// stands at `at` (a body's centre), facing `yaw` degrees, a step apart
    /// in a row; then plays on as usual.
    pub fn with_start(mut self, at: [f32; 3], yaw: f32) -> Self { self.start = Some((peakrunner_core::glam::Vec3::from(at), yaw.to_radians())); self }
    /// Fill the match with bots up to `bots` players (at most the capacity);
    /// each person who joins takes a bot's place.
    pub fn with_bots(mut self, bots: usize) -> Self { self.bots = bots.min(self.max_players) & !1; self }
    /// Each round's time of day and weather, fixed or random, in every mode.
    pub fn with_forecast(mut self, forecast: peakrunner_core::conditions::Forecast) -> Self { self.forecast = forecast; self }
    /// Spectators at once (0 to `MAX_SPECTATORS`; default the most).
    pub fn with_spectators(mut self, spectators: usize) -> io::Result<Self> {
        if spectators > MAX_SPECTATORS { return Err(invalid(&format!("spectators must be 0–{MAX_SPECTATORS}"))); }
        self.spectators = spectators;
        Ok(self)
    }
    /// A server a client started to host: that client may start the match
    /// with one team empty, by sending this key (`HOST_KEY_LEN` hex digits).
    /// Count the people who play (`stats`; None: don't).
    pub fn with_stats(mut self, stats: Option<crate::stats::Stats>) -> Self { self.stats = stats; self }
    pub fn with_host_key(mut self, key: Option<String>) -> io::Result<Self> {
        if let Some(key) = &key {
            if key.len() != crate::proto::HOST_KEY_LEN || !key.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(invalid(&format!("PEAKRUNNER_HOST_KEY must be {} lowercase hex digits", crate::proto::HOST_KEY_LEN)));
            }
        }
        self.host_key = key;
        Ok(self)
    }
    pub fn local_addr(&self) -> std::net::SocketAddr { self.listener.local_addr().expect("bound") }
    /// Everything players need to join: the mods in force, then each map the
    /// rotation (and so every vote) can reach, in rotation order, with its
    /// fingerprint and download size (docs/modding.md, P2 and P4).
    pub fn content(&self) -> Vec<peakrunner_discovery::ContentItem> {
        let mut maps = vec![self.map];
        for (map, _) in self.rotation.choices() { if !maps.contains(&map) { maps.push(map); } }
        let mods = peakrunner_core::mods::active().into_iter().map(|m| {
            let listing = m.listing().ok();
            peakrunner_discovery::ContentItem { kind: "mod".into(), key: m.id.into(), name: m.name.into(),
                fingerprint: listing.map(|l| l.fingerprint.clone()).unwrap_or_default(), bytes: listing.map_or(0, |l| l.bytes) }
        });
        let mut items: Vec<peakrunner_discovery::ContentItem> = mods.chain(maps.into_iter().filter(|&m| m != MapId::Valley).map(|map| {
            let entry = peakrunner_core::catalog::entry(map);
            peakrunner_discovery::ContentItem { kind: "map".into(), key: map.key().into(), name: entry.name.into(),
                fingerprint: entry.fingerprint().unwrap_or_default().into(), bytes: entry.download_bytes() }
        })).take(peakrunner_discovery::MAX_CONTENT).collect();
        sizes_as_served(&mut items);
        items
    }
    pub fn spawn(mut self) -> GameHandle {
        // A duel server's lobby is Tenfold Tower when it's installed.
        let duel_server = self.rotation.duels().is_some();
        if duel_server { if let Some(lobby) = peakrunner_core::sim::duel::lobby_map() { self.map = lobby; } }
        self.content = self.content();
        // Bots find their way on graphs built per map: build every map the
        // rotation can reach now, before anyone plays it (on a duel server,
        // every arena's map: bots are opponents there, never in the lobby).
        if self.bots > 0 || duel_server {
            let mut maps = if duel_server { Vec::new() } else { vec![self.map] };
            for (map, _) in self.rotation.choices() { if !maps.contains(&map) { maps.push(map); } }
            peakrunner_core::bot_nav::prepare(&maps);
        }
        self.content_digest = crate::proto::content_digest(&self.content);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let players = Arc::new(AtomicU32::new(0));
        let count = players.clone();
        let status = Arc::new(Mutex::new(peakrunner_discovery::MatchStatus { content: self.content.clone(), ..Default::default() }));
        let stats = status.clone();
        let thread = thread::spawn(move || self.run(flag, count, stats));
        GameHandle { stop, thread: Some(thread), players, status }
    }
    fn run(mut self, stop: Arc<AtomicBool>, count: Arc<AtomicU32>, status: Arc<Mutex<peakrunner_discovery::MatchStatus>>) {
        let mut game = Match::new(self.map);
        game.world.set_mode(self.rotation.current_mode());
        game.bots = self.bots;
        game.spectator_cap = self.spectators;
        game.world.forecast = self.forecast;
        // Rounds may roll random conditions: seed them from the clock.
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() ^ d.as_secs() as u32);
        game.world.reseed(seed);
        game.world.roll_conditions();
        // A duel server: the match is its lobby, and duels play in arenas.
        let mut duels = self.rotation.duels().map(|(maps, mode)| crate::duels::Duels::new(maps, mode, seed as u64));
        if duels.is_some() {
            game.world.set_mode(peakrunner_core::map_catalog::SupportedMode::DuelLobby);
            // Bots spar with the blade in the lobby's tower (`blade::spar_wish`);
            // a lobby without blade zones has none.
            let sparring = peakrunner_core::sim::features::has_blade_zones(game.world.map);
            (game.open, game.bots) = (true, if sparring { self.bots } else { 0 });
        }
        let mut peers: Vec<Peer> = Vec::new();
        let mut id = 0u32;
        let step = Duration::from_secs_f64(STEP as f64);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            let now = Instant::now();
            if now < next { thread::sleep((next - now).min(Duration::from_millis(5))); continue; }
            next += step;
            // Never repay an unbounded wall-clock backlog after suspension.
            if now.duration_since(next.min(now)) > Duration::from_millis(100) { next = now + step; }
            for _ in 0..8 {
                match self.listener.accept() {
                    Ok((stream, _)) if peers.len() < MAX_PEERS => {
                        if let Ok(wire) = Framed::new(stream, MAX_CLIENT_LINE) { peers.push(Peer::new(Wire::Tcp(wire), now)); }
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
            for _ in 0..8 {
                let Ok(local) = self.arrivals.try_recv() else { break };
                if peers.len() < MAX_PEERS { peers.push(Peer::new(Wire::Local(local), now)); }
                else { let _ = local.outbound.try_send(Outbound::Control(Box::new(ServerMsg::Reject { message: "server is full".into() }))); }
            }
            let mut commands = vec![None; MAX_PLAYERS];
            // Whose command each slot holds: a duel request may seat someone
            // else there before the step.
            let mut sent_by = [0u32; MAX_PLAYERS];
            if let Some(d) = duels.as_mut() { d.begin_tick(); }
            // Duel requests, applied once every message is read.
            let mut asked: Vec<(u32, crate::duels::Action)> = Vec::new();
            peers.retain_mut(|peer| {
                peer.tokens = (peer.tokens + 2.0).min(32.0);
                let result = (|| -> io::Result<()> {
                    if !peer.joined() && now.duration_since(peer.accepted) > Duration::from_secs(3) {
                        return Err(invalid("handshake timeout"));
                    }
                    if now.duration_since(peer.last) > Duration::from_secs(5) { return Err(invalid("input timeout")); }
                    for msg in peer.wire.receive()? {
                        peer.tokens -= 1.0;
                        if peer.tokens < 0.0 { return Err(invalid("message rate exceeded")); }
                        // The match this connection is in: the lobby (or
                        // the only match), or its duel's arena.
                        let in_arena = peer.arena != 0;
                        match msg {
                            // Before joining, a LAN client asks what the match needs.
                            ClientMsg::Status if !peer.joined() => {
                                let status = status.lock().expect("match status").clone();
                                peer.wire.send(ServerMsg::Status { status })?;
                            }
                            ClientMsg::Hello { name, protocol, password, spectate, content, player } if !peer.joined() => {
                                let name = peakrunner_core::names::validate(&name)
                                    .ok_or_else(|| invalid(peakrunner_core::names::HELP))?;
                                if protocol != crate::proto::game_protocol() { return Err(invalid("incompatible game version: update PeakRunner to join this server")); }
                                if content != self.content_digest { return Err(invalid(CONTENT_MISMATCH)); }
                                if password != self.password { return Err(invalid("incorrect match password")); }
                                // A full match seats a would-be player as a queued
                                // spectator while a spectator place is free.
                                let full = !spectate && people(&game, &duels) >= self.max_players;
                                id = id.checked_add(1).ok_or_else(|| invalid("identity limit"))?;
                                if spectate || full {
                                    let seated = if watchers(&game, &duels) < game.spectator_cap { game.join_spectator(id, name) } else { Err("") };
                                    seated.map_err(|_| invalid(if full { "match is full (no spectator places free either)" } else { "no room for more spectators" }))?;
                                    peer.watching = true;
                                    peer.watch_only = true;
                                    if full { game.queue_spectator(id, true); }
                                    log::info!("spectator_join id={id} queued={full}");
                                } else {
                                    let slot = game.join(id, name).ok_or_else(|| invalid("match is full"))?;
                                    peer.slot = Some(slot);
                                    if let Some((at, yaw)) = self.start {
                                        let side = peakrunner_core::glam::Vec3::new(yaw.cos(), 0.0, -yaw.sin());
                                        let p = &mut game.world.players[slot];
                                        (p.pos, p.vel, p.yaw) = (at + side * 2.2 * self.started as f32, peakrunner_core::glam::Vec3::ZERO, yaw);
                                        self.started += 1;
                                    }
                                    log::info!("player_join id={id} slot={slot}");
                                    if let Some(stats) = &self.stats { stats.joined(&player); }
                                }
                                peer.player = player;
                                peer.id = id;
                                // One vote per address (IPv6 per /64); LAN peers count alone.
                                // Team-change cooldowns share the same groups.
                                if let Some(origin) = peer.wire.origin() { game.votes.set_address(id, crate::link::group_key(origin)); }
                                peer.wire.send(ServerMsg::Welcome { player_id: id, match_name: self.name.clone() })?;
                                if full { peer.wire.send(ServerMsg::Notice { message: FULL_NOTICE.into() })?; }
                            }
                            ClientMsg::Input { command } if peer.joined() => {
                                if !command.valid() || command.seq <= peer.last_seq || command.seq - peer.last_seq > 1200 {
                                    return Err(invalid("invalid or out-of-order input"));
                                }
                                peer.last_seq = command.seq;
                                peer.last = now;
                                // A spectator's inputs only keep the session alive.
                                if peer.slot.is_some() { peer.inputs.push(command); }
                            }
                            ClientMsg::Chat { text } if peer.joined() => { let room = room(&mut game, &mut duels, peer.arena); match peer.slot {
                                Some(slot) => { room.chat(slot, &text); }
                                None => { room.spectator_chat(peer.id, &text); }
                            } }
                            // Spectators have no team channel.
                            ClientMsg::TeamChat { text } if peer.joined() => { if let Some(slot) = peer.slot { room(&mut game, &mut duels, peer.arena).chat_channel(slot, &text, true); } }
                            ClientMsg::Rename { name } if peer.joined() => { let room = room(&mut game, &mut duels, peer.arena); match peer.slot {
                                Some(slot) => { room.rename(slot, &name); }
                                None => { room.rename_spectator(peer.id, &name); }
                            } }
                            ClientMsg::CallVote { choice } if peer.joined() => {
                                let result = match peer.slot {
                                    _ if duels.is_some() => Err("Duel servers have no map votes"),
                                    Some(slot) => game.call_vote(slot, choice as usize),
                                    None => Err("spectators can't vote"),
                                };
                                if let Err(why) = result { log::debug!("vote refused: {why}"); peer.wire.send(ServerMsg::Notice { message: why.into() })?; }
                            }
                            ClientMsg::Vote { yes } if peer.joined() => { if let (Some(slot), false) = (peer.slot, duels.is_some()) { game.cast_vote(slot, yes); } }
                            ClientMsg::SwitchTeam {} if peer.joined() => {
                                let result = match peer.slot {
                                    _ if in_arena => Err(in_arena_why(peer)),
                                    Some(slot) => game.switch_team(slot).map(|team| log::info!("team_switch id={} team={team:?}", peer.id)),
                                    None => { let n = people(&game, &duels); play(&mut game, peer, n, self.max_players, &mut commands, self.stats.as_ref()) }
                                };
                                if let Err(why) = result { peer.wire.send(ServerMsg::Notice { message: why.into() })?; }
                            }
                            ClientMsg::Spectate { on } if peer.joined() => {
                                let result = match (on, peer.slot) {
                                    _ if in_arena => Err(in_arena_why(peer)),
                                    (true, Some(_)) if watchers(&game, &duels) >= game.spectator_cap => Err("no room for more spectators"),
                                    (true, Some(slot)) => game.spectate(slot).map(|()| {
                                        log::info!("spectator_join id={} from slot={slot}", peer.id);
                                        commands[slot] = None;
                                        peer.inputs.clear();
                                        peer.slot = None; peer.watching = true; peer.watch_only = true;
                                    }),
                                    (false, None) => { let n = people(&game, &duels); play(&mut game, peer, n, self.max_players, &mut commands, self.stats.as_ref()) }
                                    // Watching by choice: stop waiting for a slot.
                                    (true, None) => { game.queue_spectator(peer.id, false); Ok(()) }
                                    // Already where they asked to be.
                                    _ => Ok(()),
                                };
                                if let Err(why) = result { peer.wire.send(ServerMsg::Notice { message: why.into() })?; }
                            }
                            ClientMsg::StartMatch { key } if peer.joined() => {
                                let host = self.host_key.as_deref().is_some_and(|host| same_key(host, &key));
                                let result = match peer.slot {
                                    Some(_) if host => game.host_start().map(|()| log::info!("host_start id={}", peer.id)),
                                    None if host => Err("Join a team to start the match"),
                                    _ => Err("Only the host can start this match"),
                                };
                                if let Err(why) = result { peer.wire.send(ServerMsg::Notice { message: why.into() })?; }
                            }
                            ClientMsg::Challenge { .. } | ClientMsg::ChallengeBot { .. } | ClientMsg::Answer { .. } | ClientMsg::Withdraw {}
                                | ClientMsg::Watch { .. } | ClientMsg::BackToLobby {} if peer.joined() => {
                                use crate::duels::Action;
                                if duels.is_none() { peer.wire.send(ServerMsg::Notice { message: "This isn't a duel server".into() })?; continue; }
                                asked.push((peer.id, match msg {
                                    ClientMsg::Challenge { player } => Action::Challenge(player),
                                    ClientMsg::ChallengeBot { archetype } => Action::ChallengeBot(archetype),
                                    ClientMsg::Answer { player, yes } => Action::Answer(player, yes),
                                    ClientMsg::Withdraw {} => Action::Withdraw,
                                    ClientMsg::Watch { arena } => Action::Watch(arena),
                                    _ => Action::BackToLobby,
                                }));
                            }
                            ClientMsg::Leave => return Err(invalid("left match")),
                            _ => return Err(invalid("unexpected message")),
                        }
                    }
                    if let Some(slot) = peer.slot {
                        // The client predicts one step per command, so simulate
                        // each command once, one per tick, from a small jitter
                        // buffer (input_buffer): a late tick repeats the held
                        // input, a standing backlog is trimmed without losing
                        // presses, and a delayed burst can't become permanent
                        // input lag or buy extra simulation steps.
                        let command = peer.inputs.next_command();
                        if now.duration_since(peer.last) <= Duration::from_millis(250) {
                            match duels.as_mut().and_then(|d| d.commands(peer.arena)) {
                                Some(arena) => arena[slot] = command,
                                None => (commands[slot], sent_by[slot]) = (command, peer.id),
                            }
                        }
                    }
                    peer.wire.flush()
                })();
                if let Err(e) = result {
                    if let Some(slot) = peer.slot { log::info!("player_leave id={} reason={e}", room(&mut game, &mut duels, peer.arena).world.players[slot].net_id); }
                    if peer.watching { log::info!("spectator_leave id={} reason={e}", peer.id); }
                    let _ = peer.wire.send(ServerMsg::Reject { message: e.to_string() });
                    if peer.arena == 0 { if let Some(slot) = peer.slot { commands[slot] = None; } }
                    depart(&mut game, &mut duels, peer);
                    false
                } else { true }
            });
            if let Some(d) = duels.as_mut() {
                for (who, action) in asked { d.act(&mut game, &mut peers, who, action); }
            }
            keep_senders(&game, &mut commands, &sent_by);
            // A freed player slot goes to the longest-waiting queued spectator.
            for waiting in game.queued_spectators() {
                if people(&game, &duels) >= self.max_players { break; }
                let Some(peer) = peers.iter_mut().find(|p| p.watching && p.arena == 0 && p.id == waiting) else { continue };
                // A spectator still cooling down after leaving play waits a little longer.
                let n = people(&game, &duels);
                if play(&mut game, peer, n, self.max_players, &mut commands, self.stats.as_ref()).is_ok() {
                    let team = peer.slot.map_or(peakrunner_core::sim::Team::Ember, |s| game.world.players[s].team);
                    let message = if duels.is_some() { "A place opened: you're in the lobby".to_string() } else { format!("A slot opened: you joined {}",
                        if team == peakrunner_core::sim::Team::Ember { "Ember" } else { "Glacier" }) };
                    let _ = peer.wire.send(ServerMsg::Notice { message });
                }
            }
            let previous_phase = game.phase;
            // An empty server keeps its current map; the rotation carries on
            // from it at the next round's end once players are back. (A duel
            // server's lobby is always in play: it never rotates.)
            if game.world.players.iter().all(|p| p.net_id == 0) {
            } else if game.phase == peakrunner_core::sim::Phase::Intermission && game.phase_left <= STEP {
                let next = self.rotation.advance();
                game.rotate_to_mode(next, self.rotation.current_mode());
                commands.fill(None);
                for peer in &mut peers { peer.inputs.clear(); }
            }
            // A passed vote switches now; the rotation carries on from there.
            if let Some(entry) = game.votes.result.take() {
                if self.rotation.jump_to(entry) {
                    game.rotate_to_mode(entry.0, entry.1);
                    commands.fill(None);
                    for peer in &mut peers { peer.inputs.clear(); }
                }
            }
            game.votes.choices = if duels.is_some() { Vec::new() } else { self.rotation.choices() };
            #[cfg(test)]
            if let Some(stage) = self.before_step.as_mut() { stage(&mut game); }
            game.step(&commands);
            tell(&mut game, &mut peers, 0);
            if let Some(d) = duels.as_mut() { d.step(&mut game, &mut peers); }
            #[cfg(test)]
            if self.accelerated_rounds && game.tick % 120 == 0 && game.world.players.iter().any(|p|p.net_id != 0) {
                game.phase = peakrunner_core::sim::Phase::Intermission;
                game.phase_left = STEP;
            }
            if game.phase != previous_phase {
                log::info!("match_phase round={} phase={:?} score={:?}", game.round, game.phase, game.world.score);
            }
            if game.tick % 3 == 0 {
                // Each view of each match is encoded at most once per round:
                // JSON for TCP peers, a prepared frame (with server RTTs)
                // for relays.
                let mut json: HashMap<(u32, usize), Vec<u8>> = HashMap::new();
                let mut frames: HashMap<(u32, usize), Option<Arc<Frame>>> = HashMap::new();
                let pings: Vec<(u32, u32)> = peers.iter().filter_map(|peer| match (&peer.wire, peer.slot) {
                    (Wire::Local(local), Some(_)) => Some((peer.id, local.rtt.load(Ordering::Relaxed))),
                    _ => None,
                }).collect();
                let mut closed = Vec::new();
                for (k, peer) in peers.iter_mut().enumerate() {
                    if !peer.joined() { continue; }
                    let room = match &duels { Some(d) => d.room_ref(&game, peer.arena), None => &game };
                    // Spectators get the no-team view (no team chat).
                    let slot = peer.slot.unwrap_or(usize::MAX);
                    let key = (peer.arena, view(room, slot));
                    let sent = match &mut peer.wire {
                        Wire::Tcp(framed) => {
                            let bytes = json.entry(key).or_insert_with(|| serde_json::to_vec(&ServerMsg::Snapshot { state: room.snapshot_for(slot) })
                                .expect("snapshots serialize"));
                            framed.send_json(bytes).is_ok()
                        }
                        Wire::Local(local) => match frames.entry(key).or_insert_with(|| relay_frame(room, slot, &pings)) {
                            // A full queue means the relay is behind: skip this
                            // one, it sends the newest when it catches up.
                            Some(frame) => !matches!(local.outbound.try_send(Outbound::Snapshot(frame.clone())),
                                Err(tokio::sync::mpsc::error::TrySendError::Closed(_))),
                            None => true,
                        },
                    };
                    if !sent { closed.push(k); }
                }
                for k in closed.into_iter().rev() {
                    let mut peer = peers.remove(k);
                    match peer.slot {
                        Some(_) => log::info!("player_leave id={} reason=connection closed", peer.id),
                        None => log::info!("spectator_leave id={} reason=connection closed", peer.id),
                    }
                    depart(&mut game, &mut duels, &mut peer);
                }
            }
            if game.tick % 15 == 0 { if let Some(d) = &duels { d.send_boards(&game, &mut peers); } }
            let people = people(&game, &duels);
            count.store(people as u32, Ordering::Relaxed);
            if game.tick % 3 == 0 {
                // Bots play, so they count as players. Balance can seat one
                // past the capacity, which a directory would refuse to list.
                let everyone: Vec<&peakrunner_core::sim::Player> = game.world.players.iter().filter(|p| p.net_id != 0)
                    .chain(duels.iter().flat_map(|d| d.players())).collect();
                // (People away watching a duel hold a player's place.)
                let playing = (everyone.len() + duels.as_ref().map_or(0, |d| d.away())).min(self.max_players);
                // The label carries the mode too: "Old Holler - CTF" (a
                // duel server: its lobby's map, "- DUEL").
                let mode = duels.as_ref().map_or(game.world.mode, |d| d.mode());
                *status.lock().expect("match status") = peakrunner_discovery::MatchStatus {
                    name: self.name.clone(), map: format!("{} - {}", peakrunner_core::terrain::info(game.world.map).name,
                        mode.def().short),
                    players: playing as u32, max_players: self.max_players as u32, bots: playing.saturating_sub(people) as u32,
                    tick: game.tick, round: game.round, phase: format!("{:?}", game.phase),
                    score: game.world.score, time_left: game.world.time_left,
                    password_required: !self.password.is_empty(),
                    game_version: crate::proto::GAME_VERSION.into(),
                    game_protocol: crate::proto::game_protocol(),
                    roster: everyone.iter().map(|p|peakrunner_discovery::PlayerStats {
                        id:p.net_id, name:p.name.clone(), team:format!("{:?}",p.team), frags:p.frags, deaths:p.losses, bot:p.is_bot,
                    }).collect(),
                    location: self.location.clone(),
                    content: self.content.clone(),
                };
            }
        }
    }
}

/// A client whose maps differ from the server's content list.
pub const CONTENT_MISMATCH: &str = "Your maps differ from this server's: join again from the game to download its maps";

/// Told to someone seated as a spectator because the match is full.
const FULL_NOTICE: &str = "Match is full: you're spectating, and you'll join a team when a slot opens";

fn humans(game: &Match) -> usize { game.world.players.iter().filter(|p| peakrunner_core::sim::human(p)).count() }

/// A spectator asks to play: they take a free slot on the team `join` picks,
/// or, while the server is full (`people` of `capacity`), wait in the queue
/// for one. (On a duel server, a place in the lobby.)
fn play(game: &mut Match, peer: &mut Peer, people: usize, capacity: usize, commands: &mut [Option<peakrunner_core::sim::Command>], stats: Option<&crate::stats::Stats>) -> Result<(), &'static str> {
    if people >= capacity {
        game.queue_spectator(peer.id, true);
        return Err("the match is full: you'll join a team when a slot opens");
    }
    let slot = game.unspectate(peer.id, capacity)?;
    log::info!("player_join id={} slot={slot} from=spectator", peer.id);
    if let Some(stats) = stats { stats.joined(&peer.player); }
    commands[slot] = None;
    peer.inputs.clear();
    peer.slot = Some(slot); peer.watching = false; peer.watch_only = false;
    Ok(())
}

/// People on the server: the match's, and on a duel server the arenas' and
/// those away watching duels.
fn people(game: &Match, duels: &Option<crate::duels::Duels>) -> usize { humans(game) + duels.as_ref().map_or(0, |d| d.people()) }

/// Drop each command whose slot changed hands since it came (`sent_by`, by
/// net ID; a duel request may move someone out and another in on one
/// tick): the newcomer must not move as the one who left, or take their acks.
fn keep_senders(game: &Match, commands: &mut [Option<peakrunner_core::sim::Command>], sent_by: &[u32]) {
    for (slot, command) in commands.iter_mut().enumerate() {
        if command.is_some() && game.world.players.get(slot).is_none_or(|p| p.net_id != sent_by[slot]) { *command = None; }
    }
}

/// List each map or mod at the size of the files sent for it: one under
/// two keys is sent as the first one's (`quic::serve_fetch` finds the first
/// with its fingerprint), which may be stored compressed where the other
/// isn't, and a player's game refuses more bytes than listed.
fn sizes_as_served(items: &mut [peakrunner_discovery::ContentItem]) {
    for i in 1..items.len() {
        if let Some(bytes) = items[..i].iter().find(|f| f.fingerprint == items[i].fingerprint).map(|f| f.bytes) { items[i].bytes = bytes; }
    }
}

/// Send what the match in `arena` (0: the server's only match, or a duel
/// server's lobby) said to one player (`World::tell`) to them, as a notice.
pub(crate) fn tell(game: &mut Match, peers: &mut [Peer], arena: u32) {
    for (slot, message) in std::mem::take(&mut game.world.notices) {
        if let Some(peer) = peers.iter_mut().find(|p| p.arena == arena && p.slot == Some(slot)) { let _ = peer.wire.send(ServerMsg::Notice { message }); }
    }
}

/// Why someone in a duel's arena can't change teams or watch from there.
fn in_arena_why(peer: &Peer) -> &'static str {
    if peer.slot.is_some() { "You're in a duel: go back to the lobby first" } else { "You're watching a duel: go back to the lobby first" }
}

/// Spectator places taken: the match's, and on a duel server those held by
/// spectators away watching duels (`Duels::held`).
fn watchers(game: &Match, duels: &Option<crate::duels::Duels>) -> usize { game.spectators.len() + duels.as_ref().map_or(0, |d| d.held()) }

/// The match a connection in `arena` plays or watches: the server's only
/// one, or on a duel server the lobby (0) or an arena.
fn room<'a>(game: &'a mut Match, duels: &'a mut Option<crate::duels::Duels>, arena: u32) -> &'a mut Match {
    match duels { Some(d) if arena != 0 => d.room(game, arena), _ => game }
}

/// A connection leaves: out of the match (or room) it's in.
fn depart(game: &mut Match, duels: &mut Option<crate::duels::Duels>, peer: &mut Peer) {
    match duels {
        Some(d) => d.depart(game, peer),
        None => match peer.slot { Some(slot) => game.leave(slot), None => game.leave_spectator(peer.id) },
    }
}

/// One view's snapshot prepared for the relays, with their RTTs.
fn relay_frame(game: &Match, slot: usize, pings: &[(u32, u32)]) -> Option<Arc<Frame>> {
    let mut state = game.snapshot_for(slot);
    state.pings = state.players.iter().filter_map(|p| pings.iter().find(|(id, _)| *id == p.net_id).copied()).collect();
    Frame::new(state).map(Arc::new).map_err(|e| log::warn!("snapshot skipped: {e}")).ok()
}

/// `PEAKRUNNER_MAX_SPECTATORS`: spectators at once (0 to 8; default 8).
pub(crate) fn spectators_from_env() -> io::Result<usize> {
    let Ok(value) = std::env::var("PEAKRUNNER_MAX_SPECTATORS") else { return Ok(MAX_SPECTATORS) };
    match value.trim().parse::<usize>() {
        Ok(n) if n <= MAX_SPECTATORS => Ok(n),
        _ => Err(invalid(&format!("PEAKRUNNER_MAX_SPECTATORS must be 0 to {MAX_SPECTATORS}"))),
    }
}

pub fn run_from_args() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut bind = "127.0.0.1".to_string();
    let mut port = 7781u16;
    let mut name = "Open rift".to_string();
    let mut map = "old-holler".to_string();
    let mut directory: Option<String> = None;
    let mut advertise: Option<String> = None;
    let mut players = 8u32;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let Some(value) = args.next() else { eprintln!("missing value for {arg}"); return; };
        match arg.as_str() {
            "--bind" => bind = value, "--port" => port = value.parse().expect("port"),
            "--name" => name = value, "--map" => map = value,
            "--directory" => directory = Some(value), "--advertise" => advertise = Some(value),
            "--players" => players = value.parse().expect("players"),
            _ => { eprintln!("unknown option {arg}; use --bind, --port, --name, --map, --players, --directory, --advertise"); return; }
        }
    }
    let password = std::env::var("PEAKRUNNER_MATCH_PASSWORD").unwrap_or_default();
    let bind_ip: std::net::IpAddr = bind.parse().expect("--bind must be a specific private IP");
    let host = GameHost::bind(&std::net::SocketAddr::new(bind_ip, port).to_string(), &name, players, &map)
        .expect("private game server bind").with_password(password)
        .with_spectators(spectators_from_env().expect("spectators")).expect("spectators");
    println!("PeakRunner: {name}, {map}, {}, {players} players; private LAN/VPN transport", host.local_addr());
    let handle = host.spawn();
    let mut lease: Option<peakrunner_discovery::lan::Lease> = None;
    loop {
        if let Some(dir) = directory.as_deref() {
            let players = handle.players.load(Ordering::Relaxed);
            let result = if let Some(l) = &lease {
                peakrunner_discovery::lan::heartbeat(dir, l, players).map(|_| ())
            } else {
                peakrunner_discovery::lan::register(dir, &name, advertise.as_deref().unwrap_or(&bind), port, players, 8, &map)
                    .map(|l| { lease = Some(l); })
            };
            if let Err(e) = result { log::warn!("directory: {e}"); lease = None; }
        }
        thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_core::sim::Command;

    /// A slot emptied and taken again on one tick (a duel request, then a
    /// return to the lobby) drops the one who left's command: applied to the
    /// newcomer, its sequence ran their acks ahead of their own inputs.
    #[test]
    fn a_command_stays_with_the_one_who_sent_it() {
        let mut game = Match::new(MapId::Valley);
        let (a, b) = (game.join(1, "Ada").unwrap(), game.join(2, "Bo").unwrap());
        let mut commands = vec![None; MAX_PLAYERS];
        let mut sent_by = [0u32; MAX_PLAYERS];
        (commands[a], sent_by[a]) = (Some(Command { seq: 500, ..Command::default() }), 1);
        (commands[b], sent_by[b]) = (Some(Command { seq: 7, ..Command::default() }), 2);
        game.leave(a);
        assert_eq!(game.join(3, "Cy"), Some(a), "Cy takes Ada's slot");
        keep_senders(&game, &mut commands, &sent_by);
        assert!(commands[a].is_none(), "Ada's command isn't Cy's");
        assert_eq!(commands[b].map(|c| c.seq), Some(7), "Bo's stays");
        game.step(&commands);
        assert_eq!((game.acks[a], game.acks[b]), (0, 7));
        // Emptied and not taken: dropped too.
        commands[b] = Some(Command { seq: 8, ..Command::default() });
        game.leave(b);
        keep_senders(&game, &mut commands, &sent_by);
        assert!(commands[b].is_none());
    }

    /// One map under two keys goes out as the first one's files, so both are
    /// listed at that size: a copy stored raw beside a compressed one was
    /// listed smaller than what came, and players' games refused it.
    #[test]
    fn a_map_under_two_keys_is_listed_as_sent() {
        let item = |key: &str, fingerprint: &str, bytes| peakrunner_discovery::ContentItem {
            kind: "map".into(), key: key.into(), name: key.into(), fingerprint: fingerprint.into(), bytes };
        let mut items = [item("raw", "aa", 900), item("other", "bb", 50), item("packed", "aa", 300)];
        sizes_as_served(&mut items);
        assert_eq!(items.map(|i| i.bytes), [900, 50, 900]);
    }
}
