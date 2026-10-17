use std::io;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::mpsc::{self, Sender, error::TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use peakrunner_core::sim::{Command, Snapshot};
use crate::{directory, proto::{ClientMsg, ServerAdvert, ServerMsg}, transport::Transport};

/// Session state for the game loop. The snapshot is shared, so reading the
/// lobby every frame doesn't deep-copy a whole match.
#[derive(Clone, Debug, Default)]
pub struct Lobby {
    pub connected: bool, pub player_id: u32, pub match_name: String, pub map: String,
    pub players: Vec<String>, pub snapshot: Option<Arc<Snapshot>>, pub error: Option<String>,
    /// The server's latest notice (e.g. why a team switch was refused), and
    /// a count that goes up with each one.
    pub notice: Option<String>, pub notices: u32,
    /// Maps this server needs that the game lacks: waiting for the player's
    /// answer (`Session::answer_download`), then downloading.
    pub download: Option<Download>,
    /// A duel server's board for this player (`duel1`; none elsewhere).
    pub duels: Option<peakrunner_core::sim::duel::Board>,
}
/// A download of a server's maps before joining (docs/modding.md, P2).
#[derive(Clone, Debug, Default)]
pub struct Download {
    /// Each map's name and size.
    pub maps: Vec<(String, u64)>,
    pub total: u64, pub done: u64,
    /// Waiting for the player to say yes or no.
    pub asking: bool,
    /// The map being fetched.
    pub current: String,
}
/// How to join: watching instead of playing, the largest download that
/// starts without asking (0: always ask), and the game's anonymous ID for
/// counting how many people play (0: none sent; see `server_player`).
#[derive(Clone, Copy, Debug, Default)]
pub struct JoinOptions { pub spectate: bool, pub auto_download: u64, pub player: u128 }
impl Lobby {
    /// Apply a snapshot: this session must be in it, as a player or a spectator.
    pub(crate) fn accept(&mut self, state: Snapshot) -> bool {
        let id = self.player_id;
        if !state.players.iter().any(|p| p.net_id == id) && !state.spectators.iter().any(|s| s.net_id == id) { return false; }
        if self.snapshot.as_ref().is_some_and(|s| s.tick >= state.tick) { return true; }
        self.map = peakrunner_core::terrain::info(state.map).name.to_string();
        self.players = state.players.iter().filter(|p| p.net_id != 0).map(|p| p.name.clone()).collect();
        self.snapshot = Some(Arc::new(state));
        self.connected = true;
        true
    }
    pub(crate) fn notify(&mut self, message: String) { self.notice = Some(message); self.notices = self.notices.wrapping_add(1); }
    /// This session is watching, not playing.
    pub fn spectating(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| s.spectators.iter().any(|w| w.net_id == self.player_id))
    }
}
pub struct Session {
    lobby: Arc<Mutex<Lobby>>, outbound: Sender<ClientMsg>, stop: Arc<AtomicBool>,
    answer: Arc<Mutex<Option<bool>>>,
}
impl Drop for Session { fn drop(&mut self) { self.stop.store(true, Ordering::Relaxed); } }
impl Session {
    pub fn lobby(&self) -> Lobby { self.lobby.lock().expect("lobby").clone() }
    pub fn leave(self) { self.stop.store(true, Ordering::Relaxed); }
    pub fn send_input(&self, command: Command) -> bool { self.outbound.try_send(ClientMsg::Input { command }).is_ok() }
    pub fn send_chat(&self, text: String) -> bool { self.outbound.try_send(ClientMsg::Chat { text }).is_ok() }
    pub fn send_team_chat(&self, text: String) -> bool { self.outbound.try_send(ClientMsg::TeamChat { text }).is_ok() }
    pub fn rename(&self, name: String) -> bool { self.outbound.try_send(ClientMsg::Rename { name }).is_ok() }
    pub fn call_vote(&self, choice: u16) -> bool { self.outbound.try_send(ClientMsg::CallVote { choice }).is_ok() }
    pub fn vote(&self, yes: bool) -> bool { self.outbound.try_send(ClientMsg::Vote { yes }).is_ok() }
    /// Ask to play for the other team (a spectator: to join a team).
    pub fn switch_team(&self) -> bool { self.outbound.try_send(ClientMsg::SwitchTeam {}).is_ok() }
    /// Ask to watch (true) or to play again (false).
    pub fn spectate(&self, on: bool) -> bool { self.outbound.try_send(ClientMsg::Spectate { on }).is_ok() }
    /// Start the match now, with the key this client gave the server it hosts.
    pub fn start_match(&self, key: &str) -> bool { self.outbound.try_send(ClientMsg::StartMatch { key: key.into() }).is_ok() }
    /// The player's answer to `Lobby::download`: fetch the maps and join, or not.
    pub fn answer_download(&self, yes: bool) { *self.answer.lock().expect("answer") = Some(yes); }
    /// Duel servers (`duel1`): challenge a person, or a bot by archetype;
    /// answer a challenge; take yours back; watch a duel (0: the next);
    /// back to the lobby (conceding a duel being played).
    pub fn challenge(&self, player: u32) -> bool { self.outbound.try_send(ClientMsg::Challenge { player }).is_ok() }
    pub fn challenge_bot(&self, archetype: &str) -> bool { self.outbound.try_send(ClientMsg::ChallengeBot { archetype: archetype.into() }).is_ok() }
    pub fn answer_challenge(&self, player: u32, yes: bool) -> bool { self.outbound.try_send(ClientMsg::Answer { player, yes }).is_ok() }
    pub fn withdraw(&self) -> bool { self.outbound.try_send(ClientMsg::Withdraw {}).is_ok() }
    pub fn watch(&self, arena: u32) -> bool { self.outbound.try_send(ClientMsg::Watch { arena }).is_ok() }
    pub fn back_to_lobby(&self) -> bool { self.outbound.try_send(ClientMsg::BackToLobby {}).is_ok() }
}
pub fn browse(addr: &str) -> io::Result<Vec<ServerAdvert>> {
    if addr.starts_with("https://") { return peakrunner_discovery::http::browse_https(addr); }
    if addr.contains("://") { return Err(io::Error::other("public directories require https://")); }
    directory::list(addr)
}
/// What the server list shows beyond the directory's listing: ping, version,
/// whether this game can join, a password, mods and what joining downloads;
/// and, for a favorite that isn't listed, its name, map and players.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    pub ping_ms: u32, pub version: String, pub compatible: bool, pub password: bool,
    pub name: String, pub map: String, pub players: u32, pub max_players: u32,
    /// How many of `players` are bots, which make way for people.
    pub bots: u32,
    pub offer: crate::content::Offer,
}

/// Ask a server (`address` as `ServerAdvert::join_address` gives it, or a
/// saved favorite's) for its status.
pub fn probe(address: &str) -> io::Result<Probe> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let (status, rtt) = runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(5), peakrunner_discovery::quic::query_status_timed(address)).await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "no status reply"))?
    })?;
    crate::content::check(&status.content)?;
    let plain = |t: &str, max: usize| t.chars().filter(|c| !c.is_control()).take(max).collect::<String>();
    Ok(Probe {
        ping_ms: rtt.as_millis().min(9999) as u32,
        version: plain(&status.game_version, 48),
        compatible: status.game_protocol == crate::proto::game_protocol(),
        password: status.password_required,
        name: plain(&status.name, 64), map: plain(&status.map, 64), players: status.players.min(99), max_players: status.max_players.min(99),
        bots: status.bots.min(status.players).min(99),
        offer: crate::content::offer(&status.content),
    })
}
pub fn connect(host: &str, port: u16, name: &str) -> io::Result<Session> { connect_private(host, port, name, "") }
pub fn connect_private(host: &str, port: u16, name: &str, password: &str) -> io::Result<Session> {
    connect_as(host, port, name, password, false)
}
/// Join, or with `spectate` watch without a player slot. A server that
/// needs maps this game lacks ends the session with a message; to download
/// them, use `connect_with`.
pub fn connect_as(host: &str, port: u16, name: &str, password: &str, spectate: bool) -> io::Result<Session> {
    connect_with(host, port, name, password, JoinOptions { spectate, auto_download: 0, player: 0 })
}

/// Settle the server's content before saying hello: bind the maps it needs,
/// downloading missing ones once the player approves (encrypted servers
/// only; LAN servers can't send maps). Returns the content digest.
async fn prepare(address: Option<&str>, items: &[peakrunner_discovery::ContentItem], lobby: &Arc<Mutex<Lobby>>,
                 answer: &Arc<Mutex<Option<bool>>>, stop: &Arc<AtomicBool>, options: JoinOptions) -> io::Result<String> {
    crate::content::check(items)?;
    let missing = match crate::content::resolve(items) {
        Ok(digest) => return Ok(digest),
        Err(crate::content::Unresolved::Invalid(why)) => return Err(io::Error::other(format!("This server's mod can't be used: {why}"))),
        Err(crate::content::Unresolved::Missing(missing)) => missing,
    };
    let names = missing.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ");
    let Some(address) = address else {
        return Err(io::Error::other(format!("This server needs maps you don't have ({names}); servers on the LAN can't send them.")));
    };
    let total = crate::content::total(&missing);
    let asking = total > options.auto_download;
    lobby.lock().expect("lobby").download = Some(Download {
        maps: missing.iter().map(|m| (if m.kind == "mod" { format!("{} (mod)", m.name) } else { m.name.clone() }, m.bytes)).collect(),
        total, done: 0, asking, current: String::new() });
    if asking {
        loop {
            if stop.load(Ordering::Relaxed) { return Err(io::Error::other("left before downloading")); }
            match answer.lock().expect("answer").take() {
                Some(true) => break,
                Some(false) => { lobby.lock().expect("lobby").download = None; return Err(io::Error::other(format!("You chose not to download {names}."))); }
                None => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
        if let Some(d) = lobby.lock().expect("lobby").download.as_mut() { d.asking = false; }
    }
    let progress = |done: u64, total: u64, name: &str| {
        if let Some(d) = lobby.lock().expect("lobby").download.as_mut() { d.done = done; d.total = total.max(done); d.current = name.into(); }
    };
    crate::content::download(address, &missing, items, &progress).await?;
    lobby.lock().expect("lobby").download = None;
    crate::content::resolve(items).map_err(|e| io::Error::other(match e {
        crate::content::Unresolved::Invalid(why) => format!("This server's mod can't be used: {why}"),
        crate::content::Unresolved::Missing(_) => "the downloads don't match the server's list".into(),
    }))
}

/// The anonymous ID a server at `host` is sent: the game's own ID mixed
/// with the server's host name (no scheme, port or pin), so servers at one
/// address (the official ones share play.peakrunner.net) see the same
/// player, and servers at different addresses can't match players up.
/// Empty without an ID.
pub fn server_player(id: u128, host: &str) -> String {
    use sha2::{Digest, Sha256};
    if id == 0 { return String::new(); }
    let rest = host.split_once("://").map_or(host, |(_, r)| r);
    let rest = rest.strip_prefix("join/").unwrap_or(rest);
    let rest = rest.split(['#', '/']).next().unwrap_or("");
    let name = match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => rest.rsplit_once(':').filter(|(_, port)| port.parse::<u16>().is_ok()).map_or(rest, |(name, _)| name),
    };
    let digest = Sha256::new().chain_update(format!("{id:032x}")).chain_update(b":").chain_update(name.to_ascii_lowercase()).finalize();
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Join with options (see `JoinOptions`).
pub fn connect_with(host: &str, port: u16, name: &str, password: &str, options: JoinOptions) -> io::Result<Session> {
    let spectate = options.spectate;
    let player = server_player(options.player, host);
    let lobby = Arc::new(Mutex::new(Lobby::default()));
    let answer = Arc::new(Mutex::new(None));
    // A tokio channel: the QUIC session wakes as soon as an input is queued.
    let (outbound, mut rx) = mpsc::channel(32);
    let stop = Arc::new(AtomicBool::new(false));
    let state = lobby.clone();
    let flag = stop.clone();
    let address = if host.contains("://") { host.to_string() } else { match host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
        Ok(ip) => std::net::SocketAddr::new(ip, port).to_string(),
        Err(_) => format!("{host}:{port}"),
    }};
    // quic://host:port, optionally pinned with #sha256=, or an invite link
    // (peakrunner://join/host:port#sha256=...): checked before connecting.
    let encrypted = address.starts_with("quic://") || address.starts_with("peakrunner://");
    if encrypted { peakrunner_discovery::quic::join_target(&address)?; }
    let (name, password) = (name.to_string(), password.to_string());
    let asked = answer.clone();
    thread::Builder::new().name("peakrunner-session".into()).spawn(move || {
        let hello = |content: String| ClientMsg::Hello { name: name.clone(), protocol: crate::proto::game_protocol(),
            password: password.clone(), spectate, content, player: player.clone() };
        let result = (|| -> io::Result<()> {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
            if encrypted {
                return runtime.block_on(async {
                    let status = peakrunner_discovery::quic::query_status(&address).await?;
                    let digest = prepare(Some(&address), &status.content, &state, &asked, &flag, options).await?;
                    crate::quic::run_client(&address, hello(digest), &mut rx, state.clone(), flag.clone()).await
                });
            }
            let mut wire = Transport::connect(&address)?;
            // What the match needs, on the same connection, before hello.
            wire.send(&ClientMsg::Status)?;
            wire.flush()?;
            let asked_at = Instant::now();
            let status = loop {
                if let Some(ServerMsg::Status { status }) = wire.receive::<ServerMsg>()?.into_iter().find(|m| matches!(m, ServerMsg::Status { .. })) {
                    break status;
                }
                if asked_at.elapsed() > Duration::from_secs(3) { return Err(io::Error::other("the server didn't say what maps it needs")); }
                thread::sleep(Duration::from_millis(5));
            };
            let digest = runtime.block_on(prepare(None, &status.content, &state, &asked, &flag, options))?;
            wire.send(&hello(digest))?;
            let mut last = Instant::now();
            loop {
                if flag.load(Ordering::Relaxed) { let _ = wire.send(&ClientMsg::Leave); return Ok(()); }
                for _ in 0..8 {
                    match rx.try_recv() {
                        Ok(message) => wire.send(&message)?,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return Ok(()),
                    }
                }
                for msg in wire.receive::<ServerMsg>()? {
                    last = Instant::now();
                    let mut lobby = state.lock().expect("lobby");
                    match msg {
                        ServerMsg::Welcome { player_id, match_name } => {
                            lobby.player_id = player_id; lobby.match_name = match_name;
                        }
                        ServerMsg::Snapshot { state } => {
                            if state.players.len() != peakrunner_core::sim::MAX_PLAYERS
                                || state.acks.len() != state.players.len()
                                || !lobby.accept(state) {
                                return Err(io::Error::other("invalid server snapshot"));
                            }
                        }
                        ServerMsg::Notice { message } => lobby.notify(message),
                        ServerMsg::Duels { board } => lobby.duels = Some(board),
                        ServerMsg::Reject { message } => return Err(io::Error::other(message)),
                        ServerMsg::Status { .. } => {}
                        ServerMsg::File { .. } => return Err(io::Error::other("unexpected file from the server")),
                    }
                }
                if last.elapsed() > Duration::from_secs(5) { return Err(io::Error::other("server timed out")); }
                wire.flush()?;
                thread::sleep(Duration::from_millis(2));
            }
        })();
        // Keys mean the installed maps again, and the home mods are back.
        peakrunner_core::catalog::end_session();
        peakrunner_core::mods::end_session();
        let mut lobby = state.lock().expect("lobby");
        lobby.connected = false;
        lobby.download = None;
        if let Err(e) = result {
            lobby.error = Some(match e.kind() {
                io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted | io::ErrorKind::BrokenPipe =>
                    "The host closed the match or the connection was lost.".into(),
                io::ErrorKind::ConnectionRefused => "No match server is listening at that address.".into(),
                _ => e.to_string(),
            });
            lobby.players.clear();
            lobby.snapshot = None;
        }
    })?;
    Ok(Session { lobby, outbound, stop, answer })
}

#[cfg(test)]
mod tests {
    use super::server_player;

    #[test]
    fn servers_at_one_address_see_one_player_and_others_cannot_match_them() {
        let id = 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef;
        let official = server_player(id, "quic://play.peakrunner.net:7777");
        assert_eq!(official.len(), 32);
        assert!(official.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        // The four official servers share the host name.
        for host in ["quic://play.peakrunner.net:7778", "quic://play.peakrunner.net:7780", "play.peakrunner.net", "PLAY.peakrunner.net"] {
            assert_eq!(server_player(id, host), official, "{host}");
        }
        // A community server, pinned or by invite, sees someone else; never the ID itself.
        let community = server_player(id, "quic://203.0.113.9:7777#sha256=abc");
        assert_ne!(community, official);
        assert_eq!(server_player(id, "peakrunner://join/203.0.113.9:7777#sha256=abc"), community);
        assert_eq!(server_player(id, "[2001:db8::1]"), server_player(id, "quic://[2001:db8::1]:7777"));
        assert_ne!(official, format!("{id:032x}"));
        // No ID, nothing sent.
        assert_eq!(server_player(0, "play.peakrunner.net"), "");
    }
}
