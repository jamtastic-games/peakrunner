//! Server-only QUIC admission, input validation and authoritative snapshots.
use std::{collections::{HashMap, VecDeque}, io, net::{IpAddr, SocketAddr}, sync::{Arc, Mutex, mpsc::TrySendError}, time::{Duration, Instant}};
use quinn::{Connection, Endpoint};
use tokio::{sync::Semaphore, time::timeout};
use crate::{GameHost, link::{address_group, Connector, Frame, Link, Outbound}, proto::{ClientMsg, ServerMsg}, wire::invalid};
use peakrunner_discovery::quic::{ALPN, STATUS_LIMIT, SERVER_CONTROL_LIMIT, transport, read_control, send_control, send_control_limited};
use peakrunner_protocol::packets::{decode_inputs, fragments, HISTORY, MIN_DATAGRAM};
#[cfg(test)]
use peakrunner_discovery::quic::{client_config, endpoint_url};
#[cfg(test)]
use peakrunner_protocol::packets::{Inputs, Receiver, inputs};
#[cfg(test)]
use peakrunner_core::sim::Command;
pub fn server_config(cert: &[u8], key: &[u8]) -> io::Result<quinn::ServerConfig> {
    let certs = rustls_pemfile::certs(&mut io::Cursor::new(cert)).collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut io::Cursor::new(key))?.ok_or_else(|| invalid("missing TLS key"))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider).with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?.with_no_client_auth().with_single_cert(certs, key).map_err(io::Error::other)?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    // No 0-RTT: joining and inputs must not be accepted as replayable early data.
    tls.max_early_data_size = 0;
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls).map_err(io::Error::other)?;
    let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    cfg.transport_config(Arc::new(transport())).migration(false).max_incoming(CONNECTION_SLOTS);
    Ok(cfg)
}
/// Players at once from one address (IPv6 by /64), unless configured.
pub const DEFAULT_SESSIONS_PER_IP: usize = 4;
/// Per-address limits. Loopback is exempt from the session cap, so local
/// multi-client tests work; a public client can't arrive from loopback.
#[derive(Clone, Copy, Debug)]
pub struct Limits { pub sessions_per_ip: usize }
impl Default for Limits { fn default() -> Self { Self { sessions_per_ip: DEFAULT_SESSIONS_PER_IP } } }
impl Limits {
    /// `PEAKRUNNER_SESSIONS_PER_IP`: player sessions at once from one address
    /// (1 to 28; default 4). Raise it for a LAN party behind one NAT.
    pub fn from_env() -> io::Result<Self> {
        let Ok(value) = std::env::var("PEAKRUNNER_SESSIONS_PER_IP") else { return Ok(Self::default()) };
        match value.trim().parse::<usize>() {
            Ok(n) if (1..=peakrunner_core::sim::MAX_PLAYERS).contains(&n) => Ok(Self { sessions_per_ip: n }),
            _ => Err(invalid("PEAKRUNNER_SESSIONS_PER_IP must be 1 to 28")),
        }
    }
    /// Connections at once from one address: its players plus room for a
    /// status query and a pending join.
    fn connections_per_ip(&self) -> usize { self.sessions_per_ip + 2 }
}
struct Admission { active: usize, players: usize, tokens: f32, seen: Instant }
type Admissions = Arc<Mutex<HashMap<IpAddr, Admission>>>;
struct Permit { peers: Admissions, ip: IpAddr }
impl Drop for Permit { fn drop(&mut self) {
    if let Some(p) = self.peers.lock().unwrap().get_mut(&self.ip) { p.active = p.active.saturating_sub(1); }
} }
fn admit(peers: &Admissions, ip: IpAddr, limits: &Limits) -> Option<Permit> {
    let local = ip.to_canonical().is_loopback();
    let ip = address_group(ip);
    let mut map = peers.lock().unwrap();
    map.retain(|_, p| p.active > 0 || p.seen.elapsed() < Duration::from_secs(300));
    if map.len() >= 4096 && !map.contains_key(&ip) { return None; }
    let p = map.entry(ip).or_insert(Admission { active: 0, players: 0, tokens: 18.0, seen: Instant::now() });
    p.tokens = (p.tokens + p.seen.elapsed().as_secs_f32() / 3.0).min(18.0); p.seen = Instant::now();
    // A full address still leaves room for directory status and a pending
    // admission; the match itself enforces its own capacity.
    if (p.active >= limits.connections_per_ip() && !local) || p.tokens < 1.0 { return None; }
    p.tokens -= 1.0; p.active += 1;
    Some(Permit { peers: peers.clone(), ip })
}
/// A player session held by an address (see `Limits`).
struct Seat { peers: Admissions, ip: IpAddr }
impl Drop for Seat { fn drop(&mut self) {
    if let Some(p) = self.peers.lock().unwrap().get_mut(&self.ip) { p.players = p.players.saturating_sub(1); }
} }
fn seat(peers: &Admissions, ip: IpAddr, limits: &Limits) -> Option<Seat> {
    let local = ip.to_canonical().is_loopback();
    let ip = address_group(ip);
    let mut map = peers.lock().unwrap();
    let p = map.get_mut(&ip)?;
    if p.players >= limits.sessions_per_ip && !local { return None; }
    p.players += 1;
    Some(Seat { peers: peers.clone(), ip })
}
/// Connections at once (handshaking, status queries and players), and how
/// many of them may be player sessions: the rest stay free, so a full match
/// never crowds out the directory's status probes.
const CONNECTION_SLOTS: usize = 48;
const SESSION_SLOTS: usize = peakrunner_core::sim::MAX_PLAYERS + peakrunner_core::sim::teams::MAX_SPECTATORS + 4;
/// Download connections (`ClientMsg::Fetch`) at once, and the longest one
/// may last. A download holds a connection slot for as long as its client
/// keeps asking, so without these a few addresses asking slowly could hold
/// every slot and keep players and the directory's status probes out.
const FETCH_SLOTS: usize = 6;
const FETCH_TIME: Duration = Duration::from_secs(15 * 60);
/// A file is sent in pieces this large, each within `FETCH_CHUNK_TIME`:
/// never the whole file in memory, and a client that stops reading is let go.
const FETCH_CHUNK: usize = 64 * 1024;
const FETCH_CHUNK_TIME: Duration = Duration::from_secs(30);
/// Chat, rename, vote and team messages: a burst of 8, then 2 a second. Beyond
/// that they are dropped; only a flood far past anyone's typing (FLOOD_RATE
/// a second beyond a FLOOD_BURST allowance) ends the session.
const CONTROL_BURST: f32 = 8.0;
const CONTROL_RATE: f32 = 2.0;
const FLOOD_BURST: f32 = 100.0;
const FLOOD_RATE: f32 = 20.0;

pub async fn serve(endpoint: Endpoint, game: Connector, status: Arc<Mutex<peakrunner_discovery::MatchStatus>>, limits: Limits) {
    let slots = Arc::new(Semaphore::new(CONNECTION_SLOTS));
    let sessions = Arc::new(Semaphore::new(SESSION_SLOTS));
    let fetches = Arc::new(Semaphore::new(FETCH_SLOTS));
    let peers: Admissions = Arc::new(Mutex::new(HashMap::new()));
    while let Some(incoming) = endpoint.accept().await {
        if !incoming.remote_address_validated() { let _ = incoming.retry(); continue; }
        let Ok(slot) = slots.clone().try_acquire_owned() else { incoming.refuse(); continue; };
        let Some(admission) = admit(&peers, incoming.remote_address().ip(), &limits) else { incoming.refuse(); continue; };
        let status = status.clone();
        let game = game.clone();
        let peers = peers.clone();
        let sessions = sessions.clone();
        let fetches = fetches.clone();
        tokio::spawn(async move {
            let _slot = slot; let _admission = admission;
            if let Ok(Ok(conn)) = timeout(Duration::from_secs(4), incoming).await {
                let remote = conn.remote_address();
                match server_connection(&conn, game, status, peers, limits, sessions, fetches).await {
                    Ok(()) => log::debug!("QUIC session from {remote} ended"),
                    Err(e) => log::info!("QUIC session from {remote} closed: {e}"),
                }
                conn.close(0u32.into(), b"session ended");
            }
        });
    }
}
async fn next_control(mut recv: quinn::RecvStream) -> (quinn::RecvStream, io::Result<ClientMsg>) {
    let message = read_control(&mut recv).await;
    (recv, message)
}
/// Tell the client why its session ends and give the reason time to arrive:
/// closing the connection at once would discard unacknowledged stream data.
async fn reject(send: &mut quinn::SendStream, message: &str) -> io::Result<()> {
    let _ = timeout(Duration::from_secs(1), send_control(send, &ServerMsg::Reject { message: message.into() })).await;
    let _ = send.finish();
    let _ = timeout(Duration::from_secs(1), send.stopped()).await;
    Err(invalid(message))
}
/// Hand a client message to the game thread. A full queue (the game thread
/// is behind) drops it; a closed one means the match ended the session.
fn forward(link: &Link, message: ClientMsg) -> io::Result<()> {
    match link.inbound.try_send(message) {
        Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
        Err(TrySendError::Disconnected(_)) => Err(invalid("match session ended")),
    }
}
async fn server_connection(conn: &Connection, game: Connector, status: Arc<Mutex<peakrunner_discovery::MatchStatus>>, peers: Admissions,
                           limits: Limits, sessions: Arc<Semaphore>, fetches: Arc<Semaphore>) -> io::Result<()> {
    let empty_datagram_space = conn.datagram_send_buffer_space();
    let (mut send, mut recv) = timeout(Duration::from_secs(3), conn.accept_bi()).await.map_err(io::Error::other)?.map_err(io::Error::other)?;
    let hello = timeout(Duration::from_secs(3), read_control::<ClientMsg>(&mut recv)).await.map_err(io::Error::other)??;
    if matches!(hello, ClientMsg::Status) {
        let status = status.lock().unwrap().clone();
        timeout(Duration::from_secs(1), send_control_limited(&mut send, &ServerMsg::Status { status }, STATUS_LIMIT)).await.map_err(io::Error::other)??;
        send.finish().map_err(io::Error::other)?;
        let _ = timeout(Duration::from_secs(1), send.stopped()).await;
        return Ok(());
    }
    if let ClientMsg::Fetch { map, file } = hello {
        // One stream at a time: release this one's receiving half, or the
        // client can't open the next.
        drop(recv);
        let Ok(_fetch) = fetches.try_acquire_owned() else { return reject(&mut send, "the server is busy sending maps; try again in a minute").await; };
        return match timeout(FETCH_TIME, serve_fetch(conn, send, (map, file), status)).await {
            Ok(done) => done,
            Err(_) => Err(invalid("download time limit reached")),
        };
    }
    if !matches!(hello, ClientMsg::Hello { .. }) { return Err(invalid("expected hello")); }
    let Ok(_session) = sessions.try_acquire_owned() else { return reject(&mut send, "server is full").await; };
    let ip = conn.remote_address().ip();
    let Some(_seat) = seat(&peers, ip, &limits) else { return reject(&mut send, "too many players from your address").await; };
    let rtt = || conn.rtt().as_millis().min(u32::MAX as u128) as u32;
    let mut link = match game.connect(Some(address_group(ip)), rtt()) {
        Ok(link) => link,
        Err(e) => return reject(&mut send, &e.to_string()).await,
    };
    forward(&link, hello)?;
    // Event driven: inputs and controls are forwarded as they arrive, and
    // snapshots are sent as soon as the game thread hands them over.
    let mut watchdog = tokio::time::interval(Duration::from_millis(250));
    watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_seq = 0; let mut last_input = Instant::now();
    let (mut last_sent, mut skipped) = (Instant::now(), 0u32);
    // Snapshots sent (possible delta baselines), the newest of them the
    // client acknowledged, and its player ID for its exact own state.
    let (mut sent, mut acked, mut me): (VecDeque<Arc<Frame>>, u64, Option<u32>) = (VecDeque::new(), 0, None);
    let mut tokens = 32.0f32; let mut refill = Instant::now();
    let mut chat_tokens = CONTROL_BURST; let mut flood = FLOOD_BURST; let mut chat_refill = Instant::now();
    let control = next_control(recv); tokio::pin!(control);
    loop {
        tokio::select! {
            (stream, message) = &mut control => {
                let message = match message? {
                    ClientMsg::Chat { text } if text.len() <= peakrunner_core::feed::MAX_BYTES => ClientMsg::Chat { text },
                    ClientMsg::TeamChat { text } if text.len() <= peakrunner_core::feed::MAX_BYTES => ClientMsg::TeamChat { text },
                    ClientMsg::Rename { name } if name.len() <= peakrunner_core::names::MAX_LEN => ClientMsg::Rename { name },
                    ClientMsg::CallVote { choice } => ClientMsg::CallVote { choice },
                    ClientMsg::Vote { yes } => ClientMsg::Vote { yes },
                    ClientMsg::SwitchTeam {} => ClientMsg::SwitchTeam {},
                    ClientMsg::Spectate { on } => ClientMsg::Spectate { on },
                    ClientMsg::StartMatch { key } if key.len() <= crate::proto::HOST_KEY_LEN => ClientMsg::StartMatch { key },
                    ClientMsg::Challenge { player } => ClientMsg::Challenge { player },
                    ClientMsg::ChallengeBot { archetype } if archetype.len() <= 16 => ClientMsg::ChallengeBot { archetype },
                    ClientMsg::Answer { player, yes } => ClientMsg::Answer { player, yes },
                    ClientMsg::Withdraw {} => ClientMsg::Withdraw {},
                    ClientMsg::Watch { arena } => ClientMsg::Watch { arena },
                    ClientMsg::BackToLobby {} => ClientMsg::BackToLobby {},
                    ClientMsg::Leave => return Ok(()),
                    _ => return Err(invalid("unexpected control message")),
                };
                let elapsed = chat_refill.elapsed().as_secs_f32(); chat_refill = Instant::now();
                chat_tokens = (chat_tokens + elapsed * CONTROL_RATE).min(CONTROL_BURST);
                flood = (flood + elapsed * FLOOD_RATE).min(FLOOD_BURST);
                if chat_tokens >= 1.0 {
                    chat_tokens -= 1.0;
                    forward(&link, message)?;
                } else {
                    // Mashing chat or vote keys: drop the excess.
                    flood -= 1.0;
                    if flood < 0.0 { return Err(invalid("control message flood")); }
                }
                control.set(next_control(stream));
            },
            data = conn.read_datagram() => {
                tokens = (tokens + refill.elapsed().as_secs_f32() * 120.0).min(32.0); refill = Instant::now(); tokens -= 1.0;
                if tokens < 0.0 { return Err(invalid("input rate limit")); }
                let (commands, ack) = decode_inputs(&data.map_err(io::Error::other)?)?;
                if ack > acked && sent.iter().any(|f| f.tick == ack) { acked = ack; }
                for command in commands {
                    if command.seq <= last_seq { continue; }
                    if command.seq - last_seq > 1200 { return Err(invalid("input sequence jump")); }
                    forward(&link, ClientMsg::Input { command })?; last_seq = command.seq; last_input = Instant::now();
                }
            }
            out = link.outbound.recv() => {
                let Some(out) = out else { return Err(invalid("match session ended")) };
                // Controls in order; of several snapshots waiting, only the newest.
                let (mut controls, mut latest) = (Vec::new(), None);
                let mut next = Some(out);
                while let Some(out) = next.take().or_else(|| link.outbound.try_recv().ok()) {
                    match out { Outbound::Control(message) => controls.push(*message), Outbound::Snapshot(frame) => latest = Some(frame) }
                }
                for control in controls {
                    match control {
                        ServerMsg::Reject { message } => return reject(&mut send, &message).await,
                        ServerMsg::Welcome { player_id, .. } if me.is_none() => {
                            me = Some(player_id);
                            timeout(Duration::from_secs(1), send_control(&mut send, &control)).await.map_err(io::Error::other)??;
                        }
                        control @ ServerMsg::Duels { .. } => {
                            timeout(Duration::from_secs(1), send_control_limited(&mut send, &control, SERVER_CONTROL_LIMIT)).await.map_err(io::Error::other)??;
                        }
                        control => {
                            timeout(Duration::from_secs(1), send_control(&mut send, &control)).await.map_err(io::Error::other)??;
                        }
                    }
                }
                let Some(frame) = latest else { continue };
                link.rtt.store(rtt(), std::sync::atomic::Ordering::Relaxed);
                // Finish the one pending snapshot instead of mixing
                // fragments from multiple ticks or queuing old frames.
                // Under congestion, skip intermediate snapshots; once
                // drained, send the newest state on the next 20Hz tick.
                // ...but never for long: after 0.5 s of skips, send anyway
                // so a slow or lossy link isn't starved of state.
                if conn.datagram_send_buffer_space() < empty_datagram_space && last_sent.elapsed() < Duration::from_millis(500) {
                    skipped += 1;
                    continue;
                }
                if skipped > 20 { log::info!("{} snapshots skipped for a congested client before this one", skipped); }
                skipped = 0;
                last_sent = Instant::now();
                // A delta against the newest snapshot the client has, in
                // datagrams as large as the path allows.
                let message = frame.message(sent.iter().find(|f| f.tick == acked), me);
                for packet in fragments(frame.tick, &message, conn.max_datagram_size().unwrap_or(MIN_DATAGRAM))? {
                    conn.send_datagram(packet.into()).map_err(io::Error::other)?;
                }
                sent.push_back(frame);
                while sent.len() > HISTORY { sent.pop_front(); }
            }
            _ = watchdog.tick() => {
                if last_input.elapsed() > Duration::from_secs(5) { return Err(invalid("input timeout")); }
            }
        }
    }
}
/// A file a download connection sends: a mod's (small, in memory) or a
/// map's, from disk.
enum Download { Bytes(Vec<u8>), File { path: std::path::PathBuf, zlib: bool, size: u64 } }

/// Most files one download connection may ask for, and most bytes it may take.
const FETCH_FILES: usize = 1024;
const FETCH_BYTES: usize = 2 << 30;

/// A download connection (`ClientMsg::Fetch`): each stream asks for one file
/// of a map in this server's status content, and gets `ServerMsg::File` and
/// the bytes as stored (or `Reject` for a file it doesn't have). It holds no
/// player seat, serves nothing outside the listed maps' folders, and ends
/// when the client stops asking.
async fn serve_fetch(conn: &Connection, mut send: quinn::SendStream, first: (String, String),
                     status: Arc<Mutex<peakrunner_discovery::MatchStatus>>) -> io::Result<()> {
    let mut request = Some(first);
    let (mut files, mut bytes) = (0usize, 0usize);
    loop {
        let (map, file) = match request.take() {
            Some(first) => first,
            None => {
                let Ok(Ok((next, mut recv))) = timeout(Duration::from_secs(10), conn.accept_bi()).await else { return Ok(()) };
                send = next;
                match timeout(Duration::from_secs(3), read_control::<ClientMsg>(&mut recv)).await.map_err(io::Error::other)?? {
                    ClientMsg::Fetch { map, file } => (map, file),
                    _ => return Err(invalid("expected a file request")),
                }
            }
        };
        files += 1;
        if files > FETCH_FILES { return Err(invalid("too many file requests")); }
        let item = status.lock().unwrap().content.iter().find(|c| c.fingerprint == map).cloned();
        // A mod's listing and files (small, read whole), or a map's file as
        // stored (sent from disk in pieces).
        let found = tokio::task::spawn_blocking(move || -> Option<Download> {
            let c = item?;
            if c.kind == "mod" { return peakrunner_core::mods::by_fingerprint(&c.key, &c.fingerprint)?.download(&file).map(Download::Bytes); }
            let id = peakrunner_core::catalog::by_fingerprint(&c.key, &c.fingerprint)?;
            let (path, zlib) = peakrunner_core::catalog::entry(id).download_file(&file)?;
            let size = path.metadata().ok()?.len();
            Some(Download::File { path, zlib, size })
        }).await.map_err(io::Error::other)?;
        let Some(found) = found else {
            let _ = timeout(Duration::from_secs(1), send_control(&mut send, &ServerMsg::Reject { message: "no such file".into() })).await;
            let _ = send.finish();
            continue;
        };
        let (size, zlib) = match &found { Download::Bytes(d) => (d.len() as u64, false), Download::File { size, zlib, .. } => (*size, *zlib) };
        bytes += size as usize;
        if bytes > FETCH_BYTES { return Err(invalid("download limit reached")); }
        timeout(Duration::from_secs(5), send_control(&mut send, &ServerMsg::File { bytes: size, zlib })).await.map_err(io::Error::other)??;
        match found {
            Download::Bytes(data) => {
                timeout(FETCH_CHUNK_TIME, send.write_all(&data)).await.map_err(io::Error::other)?.map_err(io::Error::other)?;
            }
            Download::File { path, .. } => {
                let mut file = Some(std::fs::File::open(&path)?);
                let mut piece = vec![0u8; FETCH_CHUNK];
                let mut sent = 0u64;
                while sent < size {
                    let want = FETCH_CHUNK.min((size - sent) as usize);
                    let (f, buf, n) = tokio::task::spawn_blocking({
                        let (mut f, mut buf) = (file.take().expect("open file"), std::mem::take(&mut piece));
                        move || { let n = std::io::Read::read(&mut f, &mut buf[..want]); (f, buf, n) }
                    }).await.map_err(io::Error::other)?;
                    (file, piece) = (Some(f), buf);
                    let n = n?;
                    // The file changed under us: what was promised can't be sent.
                    if n == 0 { return Err(invalid("a map file changed while it was sent")); }
                    timeout(FETCH_CHUNK_TIME, send.write_all(&piece[..n])).await.map_err(io::Error::other)?.map_err(io::Error::other)?;
                    sent += n as u64;
                }
            }
        }
        send.finish().map_err(io::Error::other)?;
    }
}
/// Run a match configured by the environment alone (as the Docker
/// deployments are).
pub async fn run_server(bind: SocketAddr, cert: &[u8], key: &[u8]) -> io::Result<()> {
    run_match(bind, cert, key, crate::setup::plan(None, &|k| std::env::var(k).ok())?.settings).await
}
/// Run a match until SIGTERM/Ctrl-C.
pub async fn run_match(bind: SocketAddr, cert: &[u8], key: &[u8], settings: crate::setup::MatchSettings) -> io::Result<()> {
    // Mods first: the rotation may name their modes, and players get them.
    peakrunner_core::mods::activate_home(&settings.mods).map_err(|e| invalid(&format!("mods: {e}")))?;
    if !settings.mods.is_empty() { log::info!("Mods in force: {}", settings.mods.join(", ")); }
    if settings.forecast != Default::default() {
        use peakrunner_core::conditions::{TimeOfDay, Weather};
        log::info!("Time of day: {}; weather: {}", settings.forecast.time.name(&TimeOfDay::ALL), settings.forecast.weather.name(&Weather::ALL));
    }
    let scripts = peakrunner_script::check_active().map_err(|e| invalid(&format!("rules scripts and hooks: {e}")))?;
    if scripts > 0 { log::info!("Rules scripts compiled: {scripts}"); }
    log::info!("Content folders: {}", peakrunner_core::catalog::roots().iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join(", "));
    let cfg = server_config(cert, key)?;
    let endpoint = Endpoint::server(cfg, bind)?;
    let mut host = GameHost::bind("127.0.0.1:0", &settings.name, settings.max_players, &settings.map)?
        .with_password(settings.password)
        .with_bots(settings.bots)
        .with_forecast(settings.forecast)
        .with_spectators(crate::gameserver::spectators_from_env()?)?
        .with_location(&settings.location)?
        .with_host_key(settings.host_key.clone())?
        .with_stats(crate::stats::Stats::from_env(&settings.name));
    let hosted = settings.host_key.is_some();
    if let Some(rotation) = &settings.rotation {
        host = host.with_rotation(rotation)?;
    }
    if settings.random_order {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        host = host.with_random_order(now ^ (u64::from(std::process::id()) << 32));
        log::info!("Rotation in random order");
    }
    let limits = Limits::from_env()?;
    let connector = host.connector(); let game = host.spawn();
    let status = game.status.clone();
    let public_status = status.clone();
    let health_task = match &settings.health {
        Some(health_bind) => {
            let health = axum::Router::new().route("/status", axum::routing::get(move || {
                let status = status.clone(); async move { axum::Json(status.lock().unwrap().clone()) }
            }));
            match tokio::net::TcpListener::bind(health_bind.as_str()).await {
                Ok(listener) => Some(tokio::spawn(async move { let _ = axum::serve(listener, health).await; })),
                // The loopback page is only for --healthcheck: a taken port costs just that.
                Err(e) if health_bind.starts_with("127.0.0.1:") => {
                    log::warn!("No status page at {health_bind} ({e}); --healthcheck won't reach this server");
                    None
                }
                Err(e) => return Err(e),
            }
        }
        None => None,
    };
    log::info!("Encrypted UDP match listening on {bind}");
    // The client that started us joins pinned to this key: it reads the
    // fingerprint here, on its private pipe, never from the network.
    if hosted { println!("{} {}", crate::proto::HOST_READY, peakrunner_discovery::quic::fingerprint_hex(&crate::identity::leaf_fingerprint(cert)?)); }
    let stop = endpoint.clone();
    let host_left = async { if hosted { let _ = host_closed().await; } else { std::future::pending::<()>().await } };
    tokio::select! {
        _ = serve(endpoint, connector, public_status, limits) => {},
        _ = shutdown() => stop.close(0u32.into(), b"server restarting"),
        _ = host_left => stop.close(0u32.into(), b"the host closed the server"),
    }
    if let Some(task) = health_task { task.abort(); }
    drop(game); Ok(())
}
/// A hosted server runs while the client that started it holds our stdin
/// open: the client quitting, or crashing, closes it and stops the server.
fn host_closed() -> tokio::sync::oneshot::Receiver<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // A plain thread: a blocked read must not hold up the runtime's exit.
    let _ = std::thread::Builder::new().name("peakrunner-host".into()).spawn(move || {
        let mut sink = [0u8; 64];
        let mut stdin = std::io::stdin().lock();
        while matches!(std::io::Read::read(&mut stdin, &mut sink), Ok(n) if n > 0) {}
        let _ = tx.send(());
    });
    rx
}

async fn shutdown() {
    #[cfg(unix)] { let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} } }
    #[cfg(not(unix))] { let _ = tokio::signal::ctrl_c().await; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use peakrunner_protocol::packets::chunks;
    #[test] fn a_full_shared_nat_leaves_room_for_status_without_unbounded_admission() {
        let peers: Admissions = Arc::new(Mutex::new(HashMap::new()));
        let limits = Limits::default();
        let ip: IpAddr = "198.51.100.7".parse().unwrap();
        // Four players behind one NAT, each holding a connection and a seat.
        let players: Vec<_> = (0..4).map(|_| (admit(&peers, ip, &limits).unwrap(), seat(&peers, ip, &limits).unwrap())).collect();
        let status = admit(&peers, ip, &limits).expect("full address blocked directory status");
        let pending = admit(&peers, ip, &limits).unwrap();
        assert!(seat(&peers, ip, &limits).is_none(), "a fifth player from the address");
        assert!(admit(&peers, ip, &limits).is_none());
        drop(status); drop(pending); drop(players);
        let group = address_group(ip);
        let held = peers.lock().unwrap().get(&group).map(|p| (p.active, p.players));
        assert_eq!(held, Some((0, 0)));
        // IPv6 addresses count per /64: rotating the host part doesn't help.
        let seats: Vec<_> = (1..=4).map(|i| {
            let ip: IpAddr = format!("2001:db8:5:6::{i}").parse().unwrap();
            (admit(&peers, ip, &limits).unwrap(), seat(&peers, ip, &limits).unwrap())
        }).collect();
        let other: IpAddr = "2001:db8:5:6:ffff::1".parse().unwrap();
        let _held = admit(&peers, other, &limits).unwrap();
        assert!(seat(&peers, other, &limits).is_none());
        drop(seats);
        // A LAN party can raise the cap; loopback (local tests) is exempt.
        let party = Limits { sessions_per_ip: 14 };
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        let _party: Vec<_> = (0..14).map(|_| (admit(&peers, ip, &party).unwrap(), seat(&peers, ip, &party).unwrap())).collect();
        let local: IpAddr = "::1".parse().unwrap();
        let _local: Vec<_> = (0..12).map(|_| (admit(&peers, local, &limits).unwrap(), seat(&peers, local, &limits).unwrap())).collect();
    }
    #[test] fn the_session_cap_is_configurable_and_bounded() {
        for (value, ok) in [("1", Some(1)), (" 8 ", Some(8)), ("28", Some(28)), ("0", None), ("29", None), ("many", None)] {
            // Only this test reads or writes this variable.
            std::env::set_var("PEAKRUNNER_SESSIONS_PER_IP", value);
            assert_eq!(Limits::from_env().ok().map(|l| l.sessions_per_ip), ok, "{value}");
        }
        std::env::remove_var("PEAKRUNNER_SESSIONS_PER_IP");
        assert_eq!(Limits::from_env().unwrap().sessions_per_ip, DEFAULT_SESSIONS_PER_IP);
    }
    #[test] fn compressed_snapshot_limits_cover_eight_player_weapon_load() {
        let mut game = peakrunner_core::sim::Match::new(peakrunner_core::terrain::MapId::Valley);
        for id in 1..=8 { game.join(id, &format!("Player{id}")).unwrap(); }
        game.phase = peakrunner_core::sim::Phase::Playing;
        for n in 0..peakrunner_core::feed::CAPACITY {
            peakrunner_core::feed::push(&mut game.world.feed, peakrunner_core::feed::Entry::Chat {
                sender:format!("Player{n}"),text:(0..160).map(|i| char::from(b'!' + ((i*17+n*7)%90) as u8)).collect(),
            });
        }
        let mut largest = 0;
        for seq in 1..=900 {
            let commands: Vec<_> = (0..8).map(|i| Some(Command { seq, yaw:i as f32,
                fire:true, jet:true, jump:true, weapon:((seq/150)%3) as u8, ..Default::default() })).collect();
            game.step(&commands);
            if seq%3 == 0 {
                let packets = chunks(&game.snapshot()).unwrap();
                largest = largest.max(packets.len());
                assert!(packets.iter().map(Vec::len).sum::<usize>() < 16 * 1024);
            }
        }
        eprintln!("largest compressed eight-player snapshot: {largest} fragments");
        let mut malicious = 1u64.to_le_bytes().to_vec();
        malicious.extend_from_slice(&[0,1]); malicious.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Receiver::default().accept(&malicious).is_err());
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn encrypted_udp_verifies_identity_and_delivers_authoritative_snapshots() {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let server = Endpoint::server(server_config(cert.cert.pem().as_bytes(), cert.key_pair.serialize_pem().as_bytes()).unwrap(), "127.0.0.1:0".parse().unwrap()).unwrap();
        let address = server.local_addr().unwrap();
        let host = GameHost::bind("127.0.0.1:0", "QUIC test", 8, "Valley").unwrap();
        let connector = host.connector(); let game = host.spawn();
        timeout(Duration::from_secs(1), async {
            while game.status.lock().unwrap().tick == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }).await.expect("simulation did not publish its first status");
        let task = tokio::spawn(serve(server.clone(), connector, game.status.clone(), Limits::default()));
        let mut client = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(client_config(None).unwrap());
        assert!(timeout(Duration::from_secs(3), client.connect(address, "localhost").unwrap()).await.unwrap().is_err(), "untrusted certificate accepted");
        client.set_default_client_config(client_config(Some(cert.cert.der().clone())).unwrap());
        assert!(timeout(Duration::from_secs(3), client.connect(address, "wrong.example").unwrap()).await.unwrap().is_err(), "wrong hostname accepted");
        // The directory speaks only the discovery contract, not gameplay messages.
        let status_conn = client.connect(address, "localhost").unwrap().await.unwrap();
        let (mut status_send, mut status_recv) = status_conn.open_bi().await.unwrap();
        send_control(&mut status_send, &peakrunner_discovery::StatusRequest::Status).await.unwrap();
        let peakrunner_discovery::StatusReply::Status { status } =
            timeout(Duration::from_secs(3), read_control(&mut status_recv)).await.unwrap().unwrap();
        assert_eq!(status.name, "QUIC test");
        assert_eq!(status.map, "Valley - CTF");
        assert_eq!(status.players, 0);
        status_conn.close(0u32.into(), b"status test complete");
        let conn = timeout(Duration::from_secs(3), client.connect(address, "localhost").unwrap()).await.unwrap().unwrap();
        let (mut send, mut recv) = conn.open_bi().await.unwrap();
        send_control(&mut send, &ClientMsg::Hello { name: "UDP test".into(), protocol: crate::proto::game_protocol(), password: String::new(), spectate: false, content: crate::proto::content_digest(&[]) , player: String::new() }).await.unwrap();
        let welcome = timeout(Duration::from_secs(3), read_control::<ServerMsg>(&mut recv)).await.unwrap().unwrap();
        let ServerMsg::Welcome { player_id, .. } = welcome else { panic!("join rejected") };
        let mut assembly = Receiver::default();
        let state = timeout(Duration::from_secs(3), async {
            loop { if let Some(s) = assembly.accept(&conn.read_datagram().await.unwrap()).unwrap() { break s; } }
        }).await.unwrap();
        assert!(state.players.iter().any(|p| p.net_id == player_id));
        assert!(state.pings.iter().any(|(id,_)| *id == player_id), "server RTT missing");
        send_control(&mut send, &ClientMsg::Rename { name: "UDP Pilot 42".into() }).await.unwrap();
        timeout(Duration::from_secs(3), async {
            loop {
                if let Some(s) = assembly.accept(&conn.read_datagram().await.unwrap()).unwrap() {
                    if s.players.iter().any(|p| p.net_id == player_id && p.name == "UDP Pilot 42") { break; }
                }
            }
        }).await.expect("reliable rename missing from snapshots");
        // Multiple reliable control frames must not close the UDP session or
        // lose framing when snapshots arrive between partial stream reads.
        send_control(&mut send, &ClientMsg::Chat { text: "Ready for a match".into() }).await.unwrap();
        timeout(Duration::from_secs(3), async {
            loop {
                if let Some(s) = assembly.accept(&conn.read_datagram().await.unwrap()).unwrap() {
                    if s.feed.iter().any(|e| matches!(e, peakrunner_core::feed::Entry::Chat { sender, text } if sender == "UDP Pilot 42" && text == "Ready for a match")) { break; }
                }
            }
        }).await.expect("reliable chat missing from authoritative snapshots");
        conn.send_datagram(vec![0u8; 300].into()).unwrap();
        timeout(Duration::from_secs(3), conn.closed()).await.expect("malformed input was not disconnected");
        server.close(0u32.into(), b"test complete"); client.close(0u32.into(), b"test complete");
        task.await.unwrap(); drop(game);
    }
    /// A QUIC match server on loopback with a self-signed certificate, and a
    /// client endpoint trusting it.
    struct Rig { server: Endpoint, client: Endpoint, address: SocketAddr, task: tokio::task::JoinHandle<()>,
        status: Arc<Mutex<peakrunner_discovery::MatchStatus>> }
    impl Rig {
        fn start(status: Arc<Mutex<peakrunner_discovery::MatchStatus>>, connector: Connector) -> Self {
            let listed = status.clone();
            let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let server = Endpoint::server(server_config(cert.cert.pem().as_bytes(), cert.key_pair.serialize_pem().as_bytes()).unwrap(), "127.0.0.1:0".parse().unwrap()).unwrap();
            let address = server.local_addr().unwrap();
            let task = tokio::spawn(serve(server.clone(), connector, status, Limits::default()));
            let mut client = Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
            client.set_default_client_config(client_config(Some(cert.cert.der().clone())).unwrap());
            Self { server, client, address, task, status: listed }
        }
        /// The digest a client holding the server's maps says hello with.
        fn content(&self) -> String { crate::proto::content_digest(&self.status.lock().unwrap().content) }
        async fn open(&self) -> (Connection, quinn::SendStream, quinn::RecvStream) {
            let conn = timeout(Duration::from_secs(3), self.client.connect(self.address, "localhost").unwrap()).await.unwrap().unwrap();
            let (send, recv) = conn.open_bi().await.unwrap();
            (conn, send, recv)
        }
        async fn join(&self, name: &str, password: &str) -> (Connection, quinn::SendStream, quinn::RecvStream, ServerMsg) {
            let (conn, mut send, mut recv) = self.open().await;
            send_control(&mut send, &ClientMsg::Hello { name: name.into(), protocol: crate::proto::game_protocol(), password: password.into(), spectate: false, content: self.content() , player: String::new() }).await.unwrap();
            let reply = timeout(Duration::from_secs(3), read_control::<ServerMsg>(&mut recv)).await.unwrap().unwrap();
            (conn, send, recv, reply)
        }
        async fn stop(self) {
            self.server.close(0u32.into(), b"test complete"); self.client.close(0u32.into(), b"test complete");
            self.task.await.unwrap();
        }
    }
    /// Next complete snapshot that satisfies `pred`, sending a neutral input
    /// with each datagram so the session stays alive.
    async fn snapshot_where(conn: &Connection, assembly: &mut Receiver, seq: &mut u64,
                            mut pred: impl FnMut(&peakrunner_core::sim::Snapshot) -> bool) -> peakrunner_core::sim::Snapshot {
        timeout(Duration::from_secs(5), async {
            loop {
                let data = conn.read_datagram().await.unwrap();
                *seq += 1;
                let _ = conn.send_datagram(inputs(&[Command { seq: *seq, ..Command::default() }].into_iter().collect(), assembly.acked()).into());
                if let Some(s) = assembly.accept(&data).unwrap() { if pred(&s) { break s; } }
            }
        }).await.expect("snapshot condition timed out")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_full_roster_status_fits_and_rejections_reach_the_client() {
        use peakrunner_discovery::{MatchStatus, PlayerStats, StatusReply, StatusRequest, quic::read_control_limited};
        // No spectator places: a full match refuses outright.
        let host = GameHost::bind("127.0.0.1:0", "Reject test", 2, "Valley").unwrap().with_password("invite".into())
            .with_spectators(0).unwrap();
        let connector = host.connector(); let game = host.spawn();
        // 28 bots with 24-character names and large counters (about 4 KB),
        // and the most maps a server may list, each with a long name and key.
        let content = (0..peakrunner_discovery::MAX_CONTENT).map(|i| peakrunner_discovery::ContentItem { kind: "map".into(),
            key: format!("{}{i:02}", "k".repeat(30)), name: "N".repeat(48), fingerprint: "f".repeat(64), bytes: u64::MAX }).collect();
        let full = MatchStatus { name: "Springdale Central".into(), map: "Ozarktic Blast - CTF".into(), players: 28, max_players: 28, bots: 28,
            tick: 100_000_000, round: 500, phase: "Intermission".into(), score: [3, 2], time_left: 123.456,
            game_version: crate::proto::GAME_VERSION.into(), game_protocol: crate::proto::game_protocol(),
            roster: (0..28).map(|i| PlayerStats { id: 4_000_000 + i, name: "W".repeat(peakrunner_core::names::MAX_LEN), team: "Glacier".into(),
                frags: 1500, deaths: 1500, bot: true }).collect(),
            location: "New York, US".into(), content, ..Default::default() };
        let size = serde_json::to_vec(&ServerMsg::Status { status: full.clone() }).unwrap().len();
        assert!(size > 3500 && size < STATUS_LIMIT, "{size}");
        let rig = Rig::start(Arc::new(Mutex::new(full)), connector);
        let (conn, mut send, mut recv) = rig.open().await;
        send_control(&mut send, &StatusRequest::Status).await.unwrap();
        let StatusReply::Status { status } = timeout(Duration::from_secs(3), read_control_limited(&mut recv, STATUS_LIMIT)).await.unwrap().unwrap();
        assert_eq!((status.roster.len(), status.roster[27].name.len()), (28, peakrunner_core::names::MAX_LEN));
        assert_eq!(status.content.len(), peakrunner_discovery::MAX_CONTENT);
        conn.close(0u32.into(), b"done");
        // Joins prove the real match's maps (Valley: none).
        rig.status.lock().unwrap().content = game.status.lock().unwrap().content.clone();
        // The game's rejection reason arrives before the connection closes.
        let (_c, _s, _r, reply) = rig.join("Wrong", "nope").await;
        assert!(matches!(reply, ServerMsg::Reject { ref message } if message.contains("password")), "{reply:?}");
        let (_a, _as, _ar, a) = rig.join("A", "invite").await;
        let (_b, _bs, _br, b) = rig.join("B", "invite").await;
        assert!(matches!((a, b), (ServerMsg::Welcome { .. }, ServerMsg::Welcome { .. })));
        let (_c, _s, _r, reply) = rig.join("Third", "invite").await;
        assert!(matches!(reply, ServerMsg::Reject { ref message } if message.contains("full")), "{reply:?}");
        rig.stop().await; drop(game);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mashing_chat_drops_messages_but_a_flood_disconnects() {
        let host = GameHost::bind("127.0.0.1:0", "Mash test", 8, "Valley").unwrap();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (conn, mut send, _recv, welcome) = rig.join("Masher", "").await;
        assert!(matches!(welcome, ServerMsg::Welcome { .. }));
        let (mut assembly, mut seq) = (Receiver::default(), 0);
        snapshot_where(&conn, &mut assembly, &mut seq, |_| true).await;
        // Forty vote and chat presses at once: the excess is dropped, the
        // player stays.
        for i in 0..40 { send_control(&mut send, &if i % 2 == 0 { ClientMsg::Vote { yes: true } } else { ClientMsg::Chat { text: format!("gg {i}") } }).await.unwrap(); }
        let s = snapshot_where(&conn, &mut assembly, &mut seq, |s| s.feed.iter().any(|e| e.line().contains("gg 1"))).await;
        assert!(!s.feed.iter().any(|e| e.line().contains("gg 39")), "chat stays rate-limited");
        assert!(conn.close_reason().is_none());
        // Hundreds more, far beyond typing speed: disconnected.
        for i in 0..300 { if send_control(&mut send, &ClientMsg::Chat { text: format!("flood {i}") }).await.is_err() { break; } }
        timeout(Duration::from_secs(3), conn.closed()).await.expect("a control flood was not disconnected");
        rig.stop().await; drop(game);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn only_the_hosts_key_starts_a_match_with_one_team_empty() {
        use peakrunner_core::sim::Phase;
        let key = "0123456789abcdef".repeat(4);
        for bad in ["short", &key.to_uppercase(), &format!("{key}0")] {
            assert!(GameHost::bind("127.0.0.1:0", "Host test", 8, "Valley").unwrap().with_host_key(Some(bad.into())).is_err(), "{bad}");
        }
        let host = GameHost::bind("127.0.0.1:0", "Host test", 8, "Valley").unwrap().with_host_key(Some(key.clone())).unwrap();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (conn, mut send, mut recv, welcome) = rig.join("Host", "").await;
        assert!(matches!(welcome, ServerMsg::Welcome { .. }));
        let (mut assembly, mut seq) = (Receiver::default(), 0);
        snapshot_where(&conn, &mut assembly, &mut seq, |s| s.phase == Phase::Waiting).await;
        // Anyone else's key is refused with a notice.
        send_control(&mut send, &ClientMsg::StartMatch { key: "f".repeat(crate::proto::HOST_KEY_LEN) }).await.unwrap();
        let notice = timeout(Duration::from_secs(3), read_control::<ServerMsg>(&mut recv)).await.unwrap().unwrap();
        assert!(matches!(notice, ServerMsg::Notice { ref message } if message.contains("Only the host")), "{notice:?}");
        assert_eq!(snapshot_where(&conn, &mut assembly, &mut seq, |_| true).await.phase, Phase::Waiting);
        // The host's key counts the match down with the other team empty.
        send_control(&mut send, &ClientMsg::StartMatch { key }).await.unwrap();
        snapshot_where(&conn, &mut assembly, &mut seq, |s| s.phase == Phase::Countdown).await;
        // An oversized key never reaches the game: the session ends.
        send_control(&mut send, &ClientMsg::StartMatch { key: "a".repeat(crate::proto::HOST_KEY_LEN + 1) }).await.unwrap();
        timeout(Duration::from_secs(3), conn.closed()).await.expect("an oversized key was not disconnected");
        rig.stop().await; drop(game);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sessions_from_one_address_share_one_vote() {
        let host = GameHost::bind("127.0.0.1:0", "Vote test", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"ctf"},{"map":"reefbreak","mode":"team_deathmatch"}]"#).unwrap();
        let port = host.local_addr().port();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        // Two QUIC sessions from one address, and one LAN (TCP) player.
        let (a, mut a_send, _ar, _) = rig.join("Caller", "").await;
        let (b, mut b_send, _br, _) = rig.join("Same Address", "").await;
        let lan = tokio::task::spawn_blocking(move || peakrunner_net::connect("127.0.0.1", port, "Elsewhere").unwrap()).await.unwrap();
        let (mut assembly, mut seq) = (Receiver::default(), 0);
        let (mut b_assembly, mut b_seq) = (Receiver::default(), 0);
        let s = snapshot_where(&a, &mut assembly, &mut seq, |s| s.players.iter().filter(|p| p.net_id != 0).count() == 3).await;
        let reef = s.vote_choices.iter().position(|e| e.0 == peakrunner_core::terrain::MapId::Reefbreak).unwrap();
        send_control(&mut a_send, &ClientMsg::CallVote { choice: reef as u16 }).await.unwrap();
        snapshot_where(&b, &mut b_assembly, &mut b_seq, |s| s.vote.is_some()).await;
        send_control(&mut b_send, &ClientMsg::Vote { yes: true }).await.unwrap();
        for _ in 0..10 { snapshot_where(&b, &mut b_assembly, &mut b_seq, |_| true).await; }
        let v = snapshot_where(&a, &mut assembly, &mut seq, |_| true).await.vote.unwrap();
        assert_eq!((v.yes, v.voters, v.passed), (1, 2, None), "one address, one ballot, one voter");
        assert!(lan.vote(true));
        let s = snapshot_where(&a, &mut assembly, &mut seq, |s| s.map == peakrunner_core::terrain::MapId::Reefbreak).await;
        assert_eq!(s.vote.and_then(|v| v.passed), Some(true));
        lan.leave(); rig.stop().await; drop(game);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_self_signed_server_is_vouched_by_a_local_directory_and_joined_by_its_pin() {
        use peakrunner_discovery::{http, quic::{fingerprint_hex, invite_link}};
        // A generated identity, as a first run makes it.
        let dir = std::env::temp_dir().join(format!("peakrunner-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let tls = crate::identity::self_signed(&dir).unwrap();
        let pin = fingerprint_hex(&tls.fingerprint);
        let server = Endpoint::server(server_config(&tls.cert, &tls.key).unwrap(), "127.0.0.1:0".parse().unwrap()).unwrap();
        let port = server.local_addr().unwrap().port();
        let host = GameHost::bind("127.0.0.1:0", "Garage Server", 8, "Valley").unwrap();
        let connector = host.connector(); let game = host.spawn();
        timeout(Duration::from_secs(1), async {
            while game.status.lock().unwrap().tick == 0 { tokio::time::sleep(Duration::from_millis(5)).await; }
        }).await.expect("simulation did not publish its first status");
        let task = tokio::spawn(serve(server.clone(), connector, game.status.clone(), Limits::default()));
        // A directory on loopback (the real one refuses loopback announces).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let directory_at = listener.local_addr().unwrap();
        let directory = tokio::spawn(peakrunner_directory::serve_loopback_for_tests(listener,
            "quic://127.0.0.1:9".into(), "quic://127.0.0.1:9".into()));
        // The server learns its address from the directory and announces it.
        let (me, outcome) = tokio::task::spawn_blocking(move || {
            let ip = http::whoami(&format!("http://{directory_at}/whoami"), false, Duration::from_secs(4)).unwrap();
            let me = crate::setup::quic_url(&ip.to_string(), port);
            let outcome = crate::reach::announce(&format!("http://{directory_at}/announce"), &me);
            (me, outcome)
        }).await.unwrap();
        assert_eq!(me, format!("quic://127.0.0.1:{port}"));
        assert!(matches!(&outcome, crate::reach::Outcome::Listed(m) if m.contains(&pin)), "{outcome:?}");
        // A client reads the listing and finds it vouched, with the key.
        let listed = tokio::task::spawn_blocking(move ||
            http::parse_listing(&http::http_get(&format!("http://{directory_at}/servers"), false).unwrap()).unwrap()).await.unwrap();
        let entry = listed.iter().find(|s| s.host == me).expect("listed").clone();
        assert_eq!((entry.name.as_str(), entry.sha256.as_deref()), ("Garage Server", Some(pin.as_str())));
        // It joins by the pin, and by the invite link the server prints.
        let invite = invite_link(&format!("127.0.0.1:{port}"), &tls.fingerprint);
        for address in [entry.join_address(), invite] {
            let joined = tokio::task::spawn_blocking(move || {
                let session = peakrunner_net::connect(&address, 0, "Pinned Pilot").unwrap();
                let start = Instant::now();
                while !session.lobby().connected {
                    assert!(session.lobby().error.is_none(), "{address}: {:?}", session.lobby().error);
                    assert!(start.elapsed() < Duration::from_secs(5), "{address}: no snapshot");
                    std::thread::sleep(Duration::from_millis(10));
                }
                let lobby = session.lobby();
                session.leave();
                lobby
            }).await.unwrap();
            assert_eq!(joined.match_name, "Garage Server");
        }
        // Without the pin (WebPKI) or with another one, it doesn't.
        for address in [me.clone(), format!("{me}#sha256={}", "0".repeat(64))] {
            let error = tokio::task::spawn_blocking(move || {
                let session = peakrunner_net::connect(&address, 0, "Wrong Pin").unwrap();
                let start = Instant::now();
                loop {
                    let lobby = session.lobby();
                    assert!(!lobby.connected, "{address} joined");
                    if let Some(error) = lobby.error { break error; }
                    assert!(start.elapsed() < Duration::from_secs(8), "{address}: no error");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }).await.unwrap();
            assert!(error.contains("encrypted UDP connection failed"), "{error}");
        }
        directory.abort();
        server.close(0u32.into(), b"test complete"); task.await.unwrap(); drop(game);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Over a simulated 100 ms round trip, a laser aimed dead on a skier where
    /// the client draws them (100 ms behind its estimate of the server clock,
    /// as `src/online.rs` does) lands when the command carries that view
    /// time, and the same aim without it misses (sim::lagcomp).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_laser_aimed_where_the_target_is_drawn_lands_over_a_100_ms_round_trip() {
        use peakrunner_core::sim::{human, lagcomp::view_at, rifles::RIFLE_SLOT, ArmorClass, Match, Phase, STEP};
        use peakrunner_core::terrain::{spawn_on, MapId, EYE};
        use std::collections::VecDeque;
        const ONE_WAY: Duration = Duration::from_millis(50);
        const DRAWN_BEHIND: f64 = 0.1;
        const SPEED: f32 = 20.0;
        let mut hold = spawn_on(MapId::Valley, true);
        hold.y += 60.0;
        let mut start = hold;
        start.x -= 40.0; start.z += 60.0;
        let mut host = GameHost::bind("127.0.0.1:0", "Lag test", 8, "Valley").unwrap();
        // The first to join hovers with a laser; the second skis sideways in
        // a straight line 60 m in front of them.
        let mut first_tick = None;
        host.before_step = Some(Box::new(move |game: &mut Match| {
            let mut people: Vec<usize> = (0..game.world.players.len()).filter(|&i| human(&game.world.players[i])).collect();
            if people.len() < 2 { return; }
            people.sort_by_key(|&i| game.world.players[i].net_id);
            game.phase = Phase::Playing;
            let t = (game.tick - *first_tick.get_or_insert(game.tick)) as f32 * STEP;
            let s = &mut game.world.players[people[0]];
            s.pos = hold; s.vel *= 0.0; s.loadout.push(RIFLE_SLOT); s.armor = ArmorClass::Light; s.energy = peakrunner_core::defs::movement().energy_max;
            let v = &mut game.world.players[people[1]];
            v.pos = start; v.pos.x += SPEED * t; v.vel *= 0.0; v.vel.x = SPEED; v.health = 100.0;
        }));
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (shooter, _send, _recv, welcome) = rig.join("Shooter", "").await;
        let ServerMsg::Welcome { player_id: me, .. } = welcome else { panic!("join rejected") };
        let (target, _target_send, _target_recv, _) = rig.join("Target", "").await;
        let keepalive = tokio::spawn(async move {
            for seq in 1.. {
                if target.send_datagram(inputs(&[Command { seq, ..Command::default() }].into_iter().collect(), 0).into()).is_err() { break; }
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        });
        // Snapshots reach the shooter 50 ms late, and its inputs the server.
        let (arrived, mut arrivals) = tokio::sync::mpsc::unbounded_channel();
        let reader = { let conn = shooter.clone(); tokio::spawn(async move {
            while let Ok(data) = conn.read_datagram().await { if arrived.send((Instant::now() + ONE_WAY, data)).is_err() { break; } }
        }) };
        let (mut assembly, mut inbound, mut outbound) = (Receiver::default(), VecDeque::new(), VecDeque::new());
        // The target's snapshot positions, and the estimated server clock.
        let mut frames = VecDeque::new();
        let mut clock: Option<(f64, Instant)> = None;
        let (mut seq, mut hits, mut next_command, begun) = (0u64, 0u32, Instant::now(), Instant::now());
        // (when, send the view), and after each shot: whether it hit.
        let mut plan = VecDeque::from([(Duration::from_millis(2000), false), (Duration::from_millis(3500), true)]);
        let mut shots: Vec<(bool, u32, Instant, f64)> = Vec::new();
        while begun.elapsed() < Duration::from_secs(6) {
            while let Ok(packet) = arrivals.try_recv() { inbound.push_back(packet); }
            while inbound.front().is_some_and(|(due, _)| *due <= Instant::now()) {
                let (_, data) = inbound.pop_front().unwrap();
                let Some(state) = assembly.accept(&data).unwrap() else { continue };
                let time = state.tick as f64 * STEP as f64;
                clock = Some((time, Instant::now()));
                if let Some(p) = state.players.iter().find(|p| p.net_id == me) { hits = p.hits; }
                if let Some(p) = state.players.iter().find(|p| human(p) && p.net_id != me) {
                    frames.push_back((time, p.pos));
                    if frames.len() > 40 { frames.pop_front(); }
                }
            }
            if Instant::now() >= next_command {
                next_command += Duration::from_secs_f32(STEP);
                seq += 1;
                let render = clock.map(|(time, at)| time + at.elapsed().as_secs_f64() - DRAWN_BEHIND);
                let drawn = render.and_then(|r| {
                    let after = frames.iter().position(|(t, _)| *t > r).filter(|&i| i > 0)?;
                    let ((t0, a), (t1, b)) = (frames[after - 1], frames[after]);
                    Some(a.lerp(b, ((r - t0) / (t1 - t0)) as f32))
                });
                let mut command = Command { seq, weapon: RIFLE_SLOT, ..Command::default() };
                if let (Some(r), Some(mut chest)) = (render, drawn) {
                    chest.y += 0.9;
                    let mut eye = hold;
                    eye.y += EYE;
                    let d = chest - eye;
                    command.yaw = (-d.x).atan2(-d.z).rem_euclid(std::f32::consts::TAU);
                    command.pitch = (d.y / d.length()).asin();
                    if plan.front().is_some_and(|(at, _)| begun.elapsed() >= *at) {
                        let (_, with_view) = plan.pop_front().unwrap();
                        command.fire = true;
                        if with_view { command.view = view_at(r / STEP as f64); }
                        shots.push((with_view, hits, Instant::now(), clock.unwrap().0 - r));
                    }
                }
                outbound.push_back((Instant::now() + ONE_WAY, inputs(&[command].into_iter().collect(), assembly.acked())));
            }
            while outbound.front().is_some_and(|(due, _)| *due <= Instant::now()) {
                let (_, bytes) = outbound.pop_front().unwrap();
                shooter.send_datagram(bytes.into()).unwrap();
            }
            // Done once the last shot's outcome has had time to come back.
            if plan.is_empty() && shots.last().is_some_and(|(_, _, at, _)| at.elapsed() > Duration::from_millis(600)) { break; }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(shots.len(), 2, "both shots fired");
        // The hit count as the shooter last saw it: before the second shot
        // for the first, now for the second.
        let (with_view, before, _, behind) = shots[1];
        assert!(with_view && hits > before, "the shot with its view (drawn {:.0} ms behind the newest snapshot) missed", behind * 1000.0);
        let (with_view, first_before, _, _) = shots[0];
        assert!(!with_view && before == first_before, "the same aim without the view hit");
        keepalive.abort(); reader.abort();
        rig.stop().await; drop(game);
    }

    /// Football over a simulated 100 ms round trip, a person chasing a bot
    /// carrier: every two seconds the bot runs up the field at 12 m/s, then
    /// cuts sideways at 30, and 3 to 6 ticks after the cut the person is put
    /// on the line it would have run straight on, just behind where it
    /// would be. Their game, coasting the bot on from snapshots that left
    /// before the cut (`src/online.rs`), drew it there. Claiming that drawing
    /// as the game does (the newest snapshot, and as far past it as commands
    /// wait unanswered, less one), the bot is tackled; without the claim it
    /// isn't, since on the server they never met (`bump::seen_hit`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_bot_is_tackled_where_the_chasers_game_drew_it_over_a_100_ms_round_trip() {
        use peakrunner_core::glam::Vec3;
        use peakrunner_core::sim::{football::Ball, human, lagcomp::{view_at, VIEW_SUBTICKS}, Match, Phase, STEP};
        use std::collections::VecDeque;
        const ONE_WAY: Duration = Duration::from_millis(50);
        const CLAIMS_FROM: Duration = Duration::from_millis(2000);
        const LAP: u64 = 120;
        const CUT: u64 = 60;
        let map = peakrunner_core::terrain::MapId::Longfield;
        let floor = peakrunner_core::terrain::support_on(map, Vec3::new(1024.0, 120.0, 960.0)).0 + 1.2;
        let start = Vec3::new(1024.0, floor, 960.0);
        let host = GameHost::bind("127.0.0.1:0", "Tackle lag test", 8, "longfield").unwrap()
            .with_rotation(r#"[{"map":"longfield","mode":"football"}]"#).unwrap().with_bots(2);
        let mut host = host;
        // The laps in which the bot went down.
        let tackled: Arc<Mutex<Vec<u64>>> = Arc::default();
        let (seen, mut first_tick) = (tackled.clone(), None);
        host.before_step = Some(Box::new(move |game: &mut Match| {
            let person = (0..game.world.players.len()).find(|&i| human(&game.world.players[i]));
            let bot = (0..game.world.players.len()).find(|&i| game.world.players[i].is_bot && game.world.players[i].net_id != 0);
            let (Some(person), Some(bot)) = (person, bot) else { return };
            game.phase = Phase::Playing;
            let ticks = game.tick - *first_tick.get_or_insert(game.tick);
            let (lap, k) = (ticks / LAP, ticks % LAP);
            if game.world.players[bot].stun > 0.0 { seen.lock().unwrap().push(lap); }
            let (run, side) = (Vec3::Z * 12.0, Vec3::X * 30.0);
            let straight = |k: u64| start + run * (k as f32 * STEP);
            let (at, vel) = if k <= CUT { (straight(k), run) } else { (straight(CUT) + side * ((k - CUT) as f32 * STEP), side) };
            let gap = if (CUT + 3..=CUT + 6).contains(&k) { 1.2 } else { 10.0 };
            for (i, pos, vel) in [(person, straight(k) - Vec3::Z * gap, Vec3::Z * 30.0), (bot, at, vel)] {
                let p = &mut game.world.players[i];
                (p.alive, p.pos, p.vel, p.stun, p.health, p.on_ground) = (true, pos, vel, 0.0, 100.0, true);
            }
            game.world.ball = Ball { active: true, in_play: true, carrier: Some(bot), pos: at, ..Ball::default() };
        }));
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (chaser, _send, _recv, welcome) = rig.join("Chaser", "").await;
        let ServerMsg::Welcome { player_id: me, .. } = welcome else { panic!("join rejected: {welcome:?}") };
        // Snapshots reach the chaser 50 ms late, and its inputs the server.
        let (arrived, mut arrivals) = tokio::sync::mpsc::unbounded_channel();
        let reader = { let conn = chaser.clone(); tokio::spawn(async move {
            while let Ok(data) = conn.read_datagram().await { if arrived.send((Instant::now() + ONE_WAY, data)).is_err() { break; } }
        }) };
        let (mut assembly, mut inbound, mut outbound) = (Receiver::default(), VecDeque::new(), VecDeque::new());
        // The newest snapshot's tick and the last command it answered.
        let mut newest: Option<(u64, u64)> = None;
        let (mut seq, mut next_command, begun) = (0u64, Instant::now(), Instant::now());
        while begun.elapsed() < Duration::from_millis(4600) {
            while let Ok(packet) = arrivals.try_recv() { inbound.push_back(packet); }
            while inbound.front().is_some_and(|(due, _)| *due <= Instant::now()) {
                let (_, data) = inbound.pop_front().unwrap();
                let Some(state) = assembly.accept(&data).unwrap() else { continue };
                if let Some(slot) = state.players.iter().position(|p| p.net_id == me) { newest = Some((state.tick, state.acks[slot])); }
            }
            if Instant::now() >= next_command {
                next_command += Duration::from_secs_f32(STEP);
                seq += 1;
                let mut command = Command { seq, ..Command::default() };
                if let Some((tick, ack)) = newest.filter(|_| begun.elapsed() >= CLAIMS_FROM) {
                    let lead = seq.saturating_sub(ack).saturating_sub(1).min(15);
                    (command.view, command.ahead) = (view_at(tick as f64), (lead * VIEW_SUBTICKS) as u16);
                }
                outbound.push_back((Instant::now() + ONE_WAY, inputs(&[command].into_iter().collect(), assembly.acked())));
            }
            while outbound.front().is_some_and(|(due, _)| *due <= Instant::now()) {
                let (_, bytes) = outbound.pop_front().unwrap();
                chaser.send_datagram(bytes.into()).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let laps = tackled.lock().unwrap().clone();
        println!("laps with a tackle: {laps:?}");
        assert!(!laps.contains(&0), "tackled without a claim: {laps:?}");
        assert!(laps.contains(&1), "the claim's drawing never tackled the bot: {laps:?}");
        reader.abort();
        rig.stop().await; drop(game);
    }

    /// A duel server over QUIC: the board comes on the control stream (past
    /// the plain control limit once a server fills), a challenge and its
    /// answer go through the relay, and a concession ends the duel.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duels_travel_over_quic() {
        use peakrunner_core::sim::duel::{Board, Live, Person, Place};
        use peakrunner_discovery::quic::{read_control_limited, CONTROL_LIMIT};
        let host = GameHost::bind("127.0.0.1:0", "Duel QUIC", 8, "old-holler").unwrap()
            .with_rotation(r#"[{"map":"old-holler","mode":"duel"}]"#).unwrap();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (a, mut a_send, mut a_recv, welcome) = rig.join("Alpha", "").await;
        let ServerMsg::Welcome { player_id: a_id, .. } = welcome else { panic!("{welcome:?}") };
        let (b, mut b_send, mut b_recv, welcome) = rig.join("Bravo", "").await;
        let ServerMsg::Welcome { player_id: b_id, .. } = welcome else { panic!("{welcome:?}") };
        // Inputs keep both sessions alive, every snapshot (a delta, across
        // moves between the lobby and an arena too) must decode, and the
        // newest one's mode is kept; boards come on the control stream.
        let seen: Arc<Mutex<[Option<peakrunner_core::map_catalog::SupportedMode>; 2]>> = Arc::default();
        let alive = |conn: Connection, k: usize| { let seen = seen.clone(); tokio::spawn(async move {
            let (mut assembly, mut seq) = (Receiver::default(), 0u64);
            while let Ok(data) = conn.read_datagram().await {
                seq += 1;
                let _ = conn.send_datagram(inputs(&[Command { seq, ..Command::default() }].into_iter().collect(), assembly.acked()).into());
                if let Some(s) = assembly.accept(&data).expect("every snapshot decodes") { seen.lock().unwrap()[k] = Some(s.mode); }
            }
        }) };
        let mode = |k: usize| seen.lock().unwrap()[k];
        let (ka, kb) = (alive(a.clone(), 0), alive(b.clone(), 1));
        async fn board(recv: &mut quinn::RecvStream, pred: impl Fn(&Board) -> bool) -> Board {
            timeout(Duration::from_secs(5), async {
                loop {
                    if let ServerMsg::Duels { board } = read_control_limited::<ServerMsg>(recv, SERVER_CONTROL_LIMIT).await.unwrap() {
                        if pred(&board) { break board; }
                    }
                }
            }).await.expect("board condition timed out")
        }
        let first = board(&mut a_recv, |x| x.people.len() == 2).await;
        assert_eq!((first.bots.len(), first.rounds), (peakrunner_core::sim::duel::BOTS.len(), 3));
        send_control(&mut a_send, &ClientMsg::Challenge { player: b_id }).await.unwrap();
        board(&mut b_recv, |x| x.invites.iter().any(|(from, _)| *from == a_id)).await;
        send_control(&mut b_send, &ClientMsg::Answer { player: a_id, yes: true }).await.unwrap();
        let dueling = board(&mut a_recv, |x| x.duels.len() == 1 && x.people.iter().all(|p| p.place == Place::Dueling)).await;
        assert_eq!(dueling.duels[0].names, ["Alpha".to_string(), "Bravo".to_string()]);
        async fn until(what: &str, pred: impl Fn() -> bool) {
            timeout(Duration::from_secs(5), async { while !pred() { tokio::time::sleep(Duration::from_millis(10)).await; } }).await.expect(what);
        }
        use peakrunner_core::map_catalog::SupportedMode;
        until("the arena's snapshots", || mode(0) == Some(SupportedMode::Duel) && mode(1) == Some(SupportedMode::Duel)).await;
        send_control(&mut b_send, &ClientMsg::BackToLobby {}).await.unwrap();
        board(&mut a_recv, |x| x.duels.is_empty() && x.people.iter().any(|p| p.id == a_id && p.wins == 1 && p.place == Place::Lobby)).await;
        until("the lobby's snapshots again", || mode(0) == Some(SupportedMode::DuelLobby) && mode(1) == Some(SupportedMode::DuelLobby)).await;
        assert!(!ka.is_finished() && !kb.is_finished(), "no snapshot failed to decode");
        // A full server's board: past the plain control limit, within the server's.
        let name = "W".repeat(peakrunner_core::names::MAX_LEN);
        let full = Board { people: (0..28).map(|id| Person { id: 4_000_000 + id, name: name.clone(), place: Place::Dueling, arena: 1_000 + id / 2, wins: 999, losses: 999 }).collect(),
            duels: (0..14).map(|k| Live { arena: 1_000 + k, map: peakrunner_core::terrain::MapId::OldHoller, names: [name.clone(), name.clone()], score: [2, 2], bot: false, watchers: 8 }).collect(),
            bots: peakrunner_core::sim::duel::BOTS.iter().map(|b| b.to_string()).collect(), invites: (0..27).map(|i| (4_000_000 + i, 20)).collect(), challenge: Some((4_000_000, 20)), rounds: 3 };
        let size = serde_json::to_vec(&ServerMsg::Duels { board: full }).unwrap().len();
        assert!(size > CONTROL_LIMIT && size < SERVER_CONTROL_LIMIT, "{size}");
        ka.abort(); kb.abort();
        rig.stop().await; drop(game);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_spectator_watches_chats_and_joins_and_a_player_switches_teams() {
        use peakrunner_core::sim::Team;
        let host = GameHost::bind("127.0.0.1:0", "Teams test", 8, "Valley").unwrap();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (a, mut a_send, mut a_recv, welcome) = rig.join("Alpha", "").await;
        let ServerMsg::Welcome { player_id: a_id, .. } = welcome else { panic!("{welcome:?}") };
        let (b, _bs, _br, _) = rig.join("Bravo", "").await;
        let (s, mut s_send, mut s_recv) = rig.open().await;
        send_control(&mut s_send, &ClientMsg::Hello { name: "Watcher".into(), protocol: crate::proto::game_protocol(), password: String::new(), spectate: true, content: rig.content() , player: String::new() }).await.unwrap();
        let ServerMsg::Welcome { player_id: s_id, .. } = timeout(Duration::from_secs(3), read_control::<ServerMsg>(&mut s_recv)).await.unwrap().unwrap() else { panic!() };
        async fn notice(recv: &mut quinn::RecvStream) -> String {
            match timeout(Duration::from_secs(3), read_control::<ServerMsg>(recv)).await.unwrap().unwrap() {
                ServerMsg::Notice { message } => message, other => panic!("{other:?}"),
            }
        }
        let (mut sa, mut sseq) = (Receiver::default(), 0);
        let (mut aa, mut aseq) = (Receiver::default(), 0);
        let (mut ba, mut bseq) = (Receiver::default(), 0);
        // The spectator is listed apart from the player slots.
        let snap = snapshot_where(&s, &mut sa, &mut sseq, |x| x.spectators.iter().any(|w| w.net_id == s_id)).await;
        assert!(!snap.players.iter().any(|p| p.net_id == s_id));
        assert_eq!(snap.players.iter().filter(|p| p.net_id != 0).count(), 2);
        // Team chat never reaches them; their public chat reaches players.
        send_control(&mut a_send, &ClientMsg::TeamChat { text: "Secret route".into() }).await.unwrap();
        send_control(&mut s_send, &ClientMsg::Chat { text: "Hello from the stands".into() }).await.unwrap();
        let seen = snapshot_where(&a, &mut aa, &mut aseq, |x| x.feed.iter().any(|e| e.line().contains("Hello from the stands"))
            && x.feed.iter().any(|e| e.line().contains("Secret route"))).await;
        let later = snapshot_where(&s, &mut sa, &mut sseq, |x| x.tick >= seen.tick).await;
        assert!(!later.feed.iter().any(|e| e.line().contains("Secret route")));
        // Spectators can't vote: they're told why, and the session stays.
        send_control(&mut s_send, &ClientMsg::CallVote { choice: 0 }).await.unwrap();
        assert!(notice(&mut s_recv).await.contains("vote"));
        // 1 v 1: a switch would be uneven; refused with the reason.
        send_control(&mut a_send, &ClientMsg::SwitchTeam {}).await.unwrap();
        assert!(notice(&mut a_recv).await.contains("uneven"));
        // The spectator plays (2 v 1), then Alpha may cross to 1 v 2.
        send_control(&mut s_send, &ClientMsg::Spectate { on: false }).await.unwrap();
        let joined = snapshot_where(&b, &mut ba, &mut bseq, |x| x.players.iter().any(|p| p.net_id == s_id)).await;
        assert!(joined.spectators.is_empty());
        let team_of = |x: &peakrunner_core::sim::Snapshot, id| x.players.iter().find(|p| p.net_id == id).unwrap().team;
        let before = team_of(&joined, a_id);
        let size = |x: &peakrunner_core::sim::Snapshot, t: Team| x.players.iter().filter(|p| p.net_id != 0 && p.team == t).count();
        assert_eq!(size(&joined, before), 2, "the spectator joined Alpha's smaller side");
        send_control(&mut a_send, &ClientMsg::SwitchTeam {}).await.unwrap();
        let switched = snapshot_where(&a, &mut aa, &mut aseq, |x| team_of(x, a_id) != before).await;
        let me = switched.players.iter().find(|p| p.net_id == a_id).unwrap();
        assert!(me.alive && me.losses == 0, "respawned without a death");
        // Stays switched (auto-balance doesn't undo it); another switch cools down.
        for _ in 0..20 { snapshot_where(&a, &mut aa, &mut aseq, |_| true).await; }
        assert_eq!(team_of(&snapshot_where(&a, &mut aa, &mut aseq, |_| true).await, a_id), before.other());
        send_control(&mut a_send, &ClientMsg::SwitchTeam {}).await.unwrap();
        assert!(notice(&mut a_recv).await.contains("moment ago"));
        // A player may become a spectator again.
        send_control(&mut a_send, &ClientMsg::Spectate { on: true }).await.unwrap();
        snapshot_where(&a, &mut aa, &mut aseq, |x| x.spectators.iter().any(|w| w.net_id == a_id) && !x.players.iter().any(|p| p.net_id == a_id)).await;
        rig.stop().await; drop(game);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_full_match_seats_a_waiting_spectator_who_joins_when_a_slot_frees() {
        let host = GameHost::bind("127.0.0.1:0", "Full test", 2, "Valley").unwrap();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (a, _as, _ar, _) = rig.join("Alpha", "").await;
        let (_b, _bs, _br, _) = rig.join("Bravo", "").await;
        // A voluntary spectator, who never joins by themselves.
        let (v, mut v_send, mut v_recv) = rig.open().await;
        send_control(&mut v_send, &ClientMsg::Hello { name: "Watcher".into(), protocol: crate::proto::game_protocol(), password: String::new(), spectate: true, content: rig.content() , player: String::new() }).await.unwrap();
        let ServerMsg::Welcome { player_id: v_id, .. } = timeout(Duration::from_secs(3), read_control::<ServerMsg>(&mut v_recv)).await.unwrap().unwrap() else { panic!() };
        // A third player: seated as a waiting spectator, and told so.
        let (c, _cs, mut c_recv, welcome) = rig.join("Charlie", "").await;
        let ServerMsg::Welcome { player_id: c_id, .. } = welcome else { panic!("{welcome:?}") };
        async fn notice(recv: &mut quinn::RecvStream) -> String {
            match timeout(Duration::from_secs(3), read_control::<ServerMsg>(recv)).await.unwrap().unwrap() {
                ServerMsg::Notice { message } => message, other => panic!("{other:?}"),
            }
        }
        assert!(notice(&mut c_recv).await.contains("Match is full"));
        let (mut ca, mut cseq) = (Receiver::default(), 0);
        let s = snapshot_where(&c, &mut ca, &mut cseq, |x| x.spectators.len() == 2).await;
        assert!(s.spectators.iter().any(|w| w.net_id == c_id && w.queued));
        assert!(s.spectators.iter().any(|w| w.net_id == v_id && !w.queued));
        assert!(!s.players.iter().any(|p| p.net_id == c_id));
        // Alpha leaves: the waiting spectator gets the slot; the voluntary one stays.
        a.close(0u32.into(), b"bye");
        let s = snapshot_where(&c, &mut ca, &mut cseq, |x| x.players.iter().any(|p| p.net_id == c_id)).await;
        assert!(s.spectators.iter().any(|w| w.net_id == v_id), "the voluntary spectator keeps watching");
        assert!(notice(&mut c_recv).await.contains("slot opened"));
        let (mut va, mut vseq) = (Receiver::default(), 0);
        snapshot_where(&v, &mut va, &mut vseq, |x| x.spectators.iter().any(|w| w.net_id == v_id)).await;
        rig.stop().await; drop(game);
    }

    /// Acknowledged snapshots become baselines: deltas after the first full
    /// snapshot, a full one again for a client that stops acknowledging, and
    /// every datagram within the path's size.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deltas_follow_acknowledgements_over_quic() {
        let host = GameHost::bind("127.0.0.1:0", "Delta test", 8, "old-holler").unwrap();
        let connector = host.connector(); let game = host.spawn();
        let rig = Rig::start(game.status.clone(), connector);
        let (conn, _send, _recv, welcome) = rig.join("Delta", "").await;
        let ServerMsg::Welcome { player_id, .. } = welcome else { panic!("join rejected") };
        let (mut rx, mut seq) = (Receiver::default(), 0u64);
        let (mut deltas, mut fulls) = (0, 0);
        let mut acking = true;
        timeout(Duration::from_secs(10), async {
            while deltas < 20 || (!acking && fulls < 25) {
                let data = conn.read_datagram().await.unwrap();
                assert!(data.len() <= conn.max_datagram_size().unwrap().max(1060));
                if data[8] == 0 { if data[10] == 0 { fulls += 1 } else { deltas += 1 } }
                seq += 1;
                let ack = if acking { rx.acked() } else { 0 };
                let _ = conn.send_datagram(inputs(&[Command { seq, ..Command::default() }].into_iter().collect(), ack).into());
                if let Some(s) = rx.accept(&data).unwrap() { assert!(s.players.iter().any(|p| p.net_id == player_id)); }
                if deltas == 20 && acking { acking = false; fulls = 0; }
            }
        }).await.expect("deltas and fallback");
        assert_eq!(rx.unmatched, 0, "every delta's baseline was at hand");
        rig.stop().await; drop(game);
    }

    #[test] fn lost_reordered_duplicate_and_oversized_datagrams_are_bounded() {
        let mut game = peakrunner_core::sim::Match::new(peakrunner_core::terrain::MapId::Valley);
        game.join(1, "One"); game.join(2, "Two"); game.step(&[]);
        let first = chunks(&game.snapshot()).unwrap();
        let mut reassembly = Receiver::default();
        // Lose the first fragment; a newer snapshot must still complete.
        for part in first.iter().skip(1) { assert!(reassembly.accept(part).unwrap().is_none()); }
        game.step(&[]); let second = chunks(&game.snapshot()).unwrap();
        let mut decoded = None;
        for part in second.iter().rev() { if let Some(s) = reassembly.accept(part).unwrap() { decoded = Some(s); } }
        assert_eq!(decoded.unwrap().tick, game.tick);
        for part in &first { assert!(reassembly.accept(part).unwrap().is_none()); }
        for part in &second { assert!(reassembly.accept(part).unwrap().is_none()); }
        assert!(reassembly.accept(&[0; 2000]).is_err());
        for _ in 0..12 { game.step(&[]); let parts = chunks(&game.snapshot()).unwrap(); reassembly.accept(&parts[0]).unwrap(); }
        assert!(reassembly.pending_frames() <= 4);
    }
    #[test] fn input_redundancy_preserves_sequence_and_rejects_forgery() {
        let history = (7..10).map(|seq| Command { seq, ..Command::default() }).collect();
        let bytes = inputs(&history, u64::MAX); assert!(bytes.len() <= 256);
        let (commands, ack) = decode_inputs(&bytes).unwrap();
        assert_eq!((commands.iter().map(|c| c.seq).collect::<Vec<_>>(), ack), (vec![7,8,9], u64::MAX));
        assert!(decode_inputs(&[0; 300]).is_err());
        let mut bad = Inputs { count: 1, frames: [Command::default(); 3], ack: 0 };
        bad.frames[0] = Command { seq: 1, move_x: 2.0, ..Command::default() };
        assert!(decode_inputs(&postcard::to_allocvec(&bad).unwrap()).is_err());
        assert!(endpoint_url("quic://play.peakrunner.net:7777").is_ok());
        assert!(endpoint_url("quic://user:pass@play.peakrunner.net:7777").is_err());
    }
}
