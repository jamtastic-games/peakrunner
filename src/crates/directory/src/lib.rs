//! Standalone discovery service. No simulation, gameplay protocol, or client dependency.
//!
//! Two kinds of listing:
//! - The configured backend (`dellcon-north-spine`), always polled.
//! - Announced servers: a hosted server POSTs `{"url":"quic://host:port"}` to
//!   `/announce` every ANNOUNCE_EVERY seconds. The directory lists it only if
//!   the claimed host (a name, or an IP literal) is the address the request
//!   came from and a TLS 1.3 status query to that address succeeds;
//!   afterwards it polls the server itself like the backend. A server with a
//!   CA certificate for its host name is listed in `servers`. Any other
//!   server is listed in `vouched` with the SHA-256 fingerprint of the
//!   certificate it proved possession of in that handshake, and every later
//!   poll accepts only that certificate: a server whose key changes goes
//!   stale, and its next announce re-verifies it from scratch (a fresh probe
//!   records the new key). `GET /whoami` tells a host the address it must
//!   announce. Announce replies carry a JSON `AnnounceReply` explaining the
//!   status code (listed, or why not). Entries lapse when announces
//!   stop (ANNOUNCE_TTL) or status goes stale (STALE_EVICT). Announces are
//!   rate-limited per address (IPv6 per /64) and capped (MAX_ANNOUNCED in
//!   all, MAX_PER_ADDRESS from one address; the official host's address
//!   may fill the whole listing, `per_address`). A new server's verification (DNS + status probe) runs
//!   at most one at a time per address and VERIFY_CONCURRENCY at once; a
//!   listed server's refresh (DNS only) has its own REFRESH_CONCURRENCY, so
//!   a flood of new announces can't make listed servers lapse.
//! - Further official servers (`PEAKRUNNER_OFFICIAL_URLS`, e.g. the Football
//!   and Bots matches) are polled like the backend and listed without
//!   depending on announce; their own announces are acknowledged and ignored.
//!
//! Hardening:
//! - Probes go only to the IP verified for the entry (re-checked on every
//!   announce), never to whatever the name resolves to later, so DNS
//!   rebinding can't aim the directory elsewhere. Private, loopback and
//!   link-local addresses are refused.
//! - Every status is validated (name, map, counts) before it is listed, and
//!   names beginning with RESERVED_PREFIX are kept for the official host.
//! - The backend is polled on its own loop and every probe has
//!   PROBE_TIMEOUT (DNS alone DNS_TIMEOUT), so a stalling announced server
//!   can't make the official one go stale.
pub mod lan;
use std::{collections::{HashMap, HashSet}, io, net::{IpAddr, SocketAddr}, sync::{Arc, Mutex}, time::{Duration, Instant}};
use axum::{Router, Json, extract::{ConnectInfo, DefaultBodyLimit, State, Path}, http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response}, routing::{get, post}};
use peakrunner_discovery::{Announce, AnnounceReply, MatchStatus, ServerAdvert, PublicDirectory, WhoAmI, PROTOCOL, announce_id, canonical_url,
    quic::{Identity, endpoint_url, fingerprint_hex, identify_status_at, query_status_at, query_status_trusting}, http::status_fields_ok};

const BACKEND_ID: &str = "dellcon-north-spine";
const FRESH: Duration = Duration::from_secs(12);
/// Hosted servers announce this often; entries lapse after ANNOUNCE_TTL.
pub const ANNOUNCE_EVERY: Duration = Duration::from_secs(20);
const ANNOUNCE_TTL: Duration = Duration::from_secs(65);
const MIN_ANNOUNCE_GAP: Duration = Duration::from_secs(4);
/// Announced servers listed at once, and from one community address (the
/// official host's may list up to MAX_ANNOUNCED: `per_address`).
const MAX_ANNOUNCED: usize = 256;
const MAX_PER_ADDRESS: usize = 16;
const STALE_EVICT: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
/// Resolving an announced (or the advertised) host name.
const DNS_TIMEOUT: Duration = Duration::from_secs(1);
/// New servers being verified at once (each DNS + a status probe).
const VERIFY_CONCURRENCY: usize = 4;
/// Listed servers' refreshes at once (DNS only); a refresh waits up to
/// REFRESH_WAIT for a turn rather than failing.
const REFRESH_CONCURRENCY: usize = 8;
const REFRESH_WAIT: Duration = Duration::from_secs(2);
const MAX_RATE_KEYS: usize = 4096;
/// Names only the official host may use.
const RESERVED_PREFIX: &str = "springdale";

/// An announced server. `identity` is what it proved in the probe that
/// listed it; polls accept nothing else.
struct Announced { owner: IpAddr, ip: IpAddr, identity: Identity, announced: Instant, added: Instant, status: Option<(MatchStatus, Instant)> }
type Seen = Arc<Mutex<Option<(MatchStatus, Instant)>>>;
/// A further official server: its URL, listing ID and last status.
#[derive(Clone)]
struct Official { url: String, id: String, status: Seen }

#[derive(Clone)]
struct Directory {
    status: Arc<Mutex<Option<(MatchStatus, Instant)>>>,
    advertised: String,
    announced: Arc<Mutex<HashMap<String, Announced>>>,
    /// Last announce per (address, announced server): several servers on one
    /// host each announce on their own schedule.
    last_announce: Arc<Mutex<HashMap<(IpAddr, String), Instant>>>,
    /// Addresses of the official host (the backend's), refreshed by its poller.
    official: Arc<Mutex<Vec<IpAddr>>>,
    /// Official servers besides the backend, in configured order.
    extra: Vec<Official>,
    verify: Arc<tokio::sync::Semaphore>,
    refresh: Arc<tokio::sync::Semaphore>,
    /// Addresses (rate keys) with a new-server verification in flight.
    verifying: Arc<Mutex<HashSet<IpAddr>>>,
    trusted: bool,
    /// Integration tests only (`serve_loopback_for_tests`): accept announces
    /// from and probes to loopback.
    loopback: bool,
}

impl Directory {
    fn new(advertised: String, trusted: bool) -> Self {
        Self { status: Arc::new(Mutex::new(None)), advertised, announced: Arc::default(), last_announce: Arc::default(),
            official: Arc::default(), extra: Vec::new(), verify: Arc::new(tokio::sync::Semaphore::new(VERIFY_CONCURRENCY)),
            refresh: Arc::new(tokio::sync::Semaphore::new(REFRESH_CONCURRENCY)), verifying: Arc::default(), trusted, loopback: false }
    }
    /// Add official servers besides the backend (quic:// URLs).
    fn with_official(mut self, urls: &[String]) -> io::Result<Self> {
        for url in urls {
            let url = canonical_url(url).ok_or_else(|| io::Error::other(format!("invalid official server URL {url}")))?;
            let id = announce_id(&url).ok_or_else(|| io::Error::other(format!("invalid official server URL {url}")))?;
            if canonical_url(&self.advertised).as_deref() == Some(url.as_str()) || self.extra.iter().any(|o| o.url == url) { continue; }
            self.extra.push(Official { url, id, status: Arc::default() });
        }
        Ok(self)
    }
    fn is_official_url(&self, url: &str) -> bool {
        canonical_url(&self.advertised).is_some_and(|a| a == url) || self.extra.iter().any(|o| o.url == url)
    }
}

/// One new-server verification in flight for an address; ends on drop.
struct Verifying { set: Arc<Mutex<HashSet<IpAddr>>>, key: IpAddr }
impl Drop for Verifying { fn drop(&mut self) { self.set.lock().unwrap().remove(&self.key); } }
fn begin_verify(directory: &Directory, from: IpAddr) -> Option<Verifying> {
    let key = rate_key(from);
    directory.verifying.lock().unwrap().insert(key).then(|| Verifying { set: directory.verifying.clone(), key })
}

/// How many servers address `from` may list, and announce in one gap
/// window: MAX_PER_ADDRESS, or the whole listing for the official host.
fn per_address(directory: &Directory, from: IpAddr) -> usize {
    if directory.official.lock().unwrap().iter().any(|o| rate_key(*o) == rate_key(from)) { MAX_ANNOUNCED } else { MAX_PER_ADDRESS }
}

/// The rate-limit key for an address: IPv6 by its /64, IPv4 as is.
fn rate_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => { let s = v6.segments(); IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0)) }
        v4 => v4,
    }
}

/// A globally routable address a public server could be on.
fn public_ip(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(a) => !(a.is_private() || a.is_loopback() || a.is_link_local() || a.is_unspecified()
            || a.is_broadcast() || a.is_multicast() || a.is_documentation()
            || (a.octets()[0] == 100 && (a.octets()[1] & 0xc0) == 64)),       // carrier-grade NAT
        IpAddr::V6(a) => !(a.is_loopback() || a.is_unspecified() || a.is_multicast()
            || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80),
    }
}

/// Whether a status may be listed; reserved names only from the official host.
fn status_ok(s: &MatchStatus, official: bool) -> bool {
    status_fields_ok(&s.name, &s.map, s.players, s.max_players) && peakrunner_discovery::valid_location(&s.location)
        && (official || !s.name.trim().to_ascii_lowercase().starts_with(RESERVED_PREFIX))
}

fn fresh(seen: &Seen) -> Option<MatchStatus> {
    seen.lock().unwrap().as_ref().filter(|(_, at)| at.elapsed() < FRESH).map(|(s, _)| s.clone())
}

fn live(directory: &Directory) -> Option<MatchStatus> { fresh(&directory.status) }

fn advert(id: String, url: &str, s: MatchStatus, identity: Identity) -> ServerAdvert {
    ServerAdvert { id, name: s.name, map: s.map, host: url.into(),
        port: endpoint_url(url).ok().and_then(|u| u.port()).unwrap_or(7777),
        players: s.players, max_players: s.max_players, bots: s.bots.min(s.players), location: s.location,
        sha256: match identity { Identity::Ca => None, Identity::Key(fp) => Some(fingerprint_hex(&fp)) } }
}

fn announced_live(directory: &Directory) -> Vec<(String, String, MatchStatus, Identity)> {
    let map = directory.announced.lock().unwrap();
    let mut out: Vec<_> = map.iter().filter_map(|(url, a)| {
        let (s, seen) = a.status.as_ref()?;
        if seen.elapsed() >= FRESH || a.announced.elapsed() >= ANNOUNCE_TTL { return None; }
        Some((announce_id(url)?, url.clone(), s.clone(), a.identity))
    }).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}

async fn health(State(directory): State<Directory>) -> Response {
    match live(&directory) { Some(s) => Json(s).into_response(), None => StatusCode::SERVICE_UNAVAILABLE.into_response() }
}

async fn listing(State(directory): State<Directory>) -> Response {
    let mut servers: Vec<ServerAdvert> = live(&directory)
        .map(|s| advert(BACKEND_ID.into(), &directory.advertised, s, Identity::Ca)).into_iter().collect();
    servers.extend(directory.extra.iter().filter_map(|o| fresh(&o.status).map(|s| advert(o.id.clone(), &o.url, s, Identity::Ca))));
    // Released clients read only `servers` and can join only CA-certified
    // hosts there; key-vouched hosts go in `vouched`, which they ignore.
    let (ca, vouched): (Vec<_>, Vec<_>) = announced_live(&directory).into_iter()
        .map(|(id, url, s, identity)| advert(id, &url, s, identity)).partition(|a| a.sha256.is_none());
    servers.extend(ca);
    no_store(Json(PublicDirectory { protocol: PROTOCOL.into(), servers, vouched }).into_response())
}

async fn details(Path(id): Path<String>, State(directory): State<Directory>) -> Response {
    let found = if id == BACKEND_ID { Some(live(&directory)) }
        else if let Some(o) = directory.extra.iter().find(|o| o.id == id) { Some(fresh(&o.status)) }
        else { announced_live(&directory).into_iter().find(|(i, ..)| *i == id).map(|(_, _, s, _)| Some(s)) };
    no_store(match found {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(Some(status)) => Json(status).into_response(),
        Some(None) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    })
}

/// The address an announce came from: Cloudflare's client IP header when the
/// directory sits behind the tunnel (trusted mode), else the socket peer.
fn requester(directory: &Directory, headers: &HeaderMap, peer: SocketAddr) -> Option<IpAddr> {
    if directory.trusted {
        headers.get("cf-connecting-ip")?.to_str().ok()?.trim().parse().ok()
    } else { Some(peer.ip()) }
}

fn same_ip(a: IpAddr, b: IpAddr) -> bool { a.to_canonical() == b.to_canonical() }

/// The caller's own address: what a host behind NAT announces.
async fn whoami(State(directory): State<Directory>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Response {
    match requester(&directory, &headers, peer) {
        Some(ip) => no_store(Json(WhoAmI { address: ip.to_canonical().to_string() }).into_response()),
        None => no_store(StatusCode::BAD_REQUEST.into_response()),
    }
}

/// An announce reply: the status code older servers read, and a body saying why.
fn reply(code: StatusCode, message: impl Into<String>) -> Response {
    if code == StatusCode::NO_CONTENT { return no_store(code.into_response()); }
    let result = match code {
        StatusCode::CREATED => "listed", StatusCode::UNPROCESSABLE_ENTITY => "unreachable", StatusCode::FORBIDDEN => "wrong_address",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited", StatusCode::SERVICE_UNAVAILABLE => "busy", _ => "invalid",
    };
    no_store((code, Json(AnnounceReply { result: result.into(), message: message.into(), sha256: None })).into_response())
}
fn refusal(code: StatusCode) -> Response {
    reply(code, match code {
        StatusCode::BAD_REQUEST => "announce {\"url\":\"quic://host:port\"} with a port of 1024 or above",
        StatusCode::FORBIDDEN => "another address already announced this server",
        StatusCode::TOO_MANY_REQUESTS => "announcing too often from this address; try again shortly",
        StatusCode::SERVICE_UNAVAILABLE => "the directory is busy verifying other servers; try again shortly",
        _ => "",
    })
}

/// Admission before any network work: the URL shape, the per-address rate
/// and the caps. Ok(true) to verify (a new entry, or a listed one with no
/// fresh status, whose key may have changed), Ok(false) to refresh one (the
/// refresh itself happens only after its address is re-verified).
fn admit(directory: &Directory, url: &str, from: IpAddr, now: Instant) -> Result<bool, StatusCode> {
    if directory.is_official_url(url) { return Err(StatusCode::NO_CONTENT); }
    let u = endpoint_url(url).map_err(|_| StatusCode::BAD_REQUEST)?;
    if announce_id(url).is_none() || u.port().is_some_and(|p| p < 1024) { return Err(StatusCode::BAD_REQUEST); }
    let key = (rate_key(from), url.to_string());
    let cap = per_address(directory, from);
    {
        let mut last = directory.last_announce.lock().unwrap();
        if last.get(&key).is_some_and(|t| now.duration_since(*t) < MIN_ANNOUNCE_GAP) { return Err(StatusCode::TOO_MANY_REQUESTS); }
        // Per address, at most its cap of announces in any gap window.
        let recent = last.iter().filter(|((ip, _), t)| *ip == key.0 && now.duration_since(**t) < MIN_ANNOUNCE_GAP).count();
        if recent >= cap { return Err(StatusCode::TOO_MANY_REQUESTS); }
        if last.len() >= MAX_RATE_KEYS { last.retain(|_, t| now.duration_since(*t) < ANNOUNCE_TTL); }
        if last.len() >= MAX_RATE_KEYS && !last.contains_key(&key) { return Err(StatusCode::SERVICE_UNAVAILABLE); }
        last.insert(key.clone(), now);
    }
    let mut map = directory.announced.lock().unwrap();
    map.retain(|_, a| now.duration_since(a.announced) < ANNOUNCE_TTL);
    if let Some(a) = map.get(url) {
        if rate_key(a.owner) != key.0 { return Err(StatusCode::FORBIDDEN); }
        // Polls accept only the recorded identity, so a server whose key
        // changed stops answering them; it re-verifies with a fresh probe.
        return Ok(a.status.as_ref().is_none_or(|(_, seen)| now.saturating_duration_since(*seen) >= FRESH));
    }
    if map.len() >= MAX_ANNOUNCED { return Err(StatusCode::SERVICE_UNAVAILABLE); }
    if map.values().filter(|a| rate_key(a.owner) == key.0).count() >= cap { return Err(StatusCode::TOO_MANY_REQUESTS); }
    Ok(true)
}

/// The announcer's own address among those the URL's host resolves to (with
/// a timeout), if it's a public one. An IP literal resolves to itself, so it
/// must be the requester's address exactly.
async fn verified_ip(directory: &Directory, url: &str, from: IpAddr) -> Result<IpAddr, String> {
    let u = endpoint_url(url).map_err(|e| e.to_string())?;
    let host = u.host_str().unwrap_or("").trim_matches(['[', ']']).to_string();
    let port = u.port().unwrap_or(7777);
    let from_shown = from.to_canonical();
    let addrs = match tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((host.as_str(), port))).await {
        Ok(Ok(addrs)) => addrs,
        _ => return Err(format!("{host} did not resolve")),
    };
    let ip = addrs.map(|a| a.ip()).find(|ip| same_ip(*ip, from))
        .ok_or_else(|| format!("{host} is not {from_shown}, the address this announce came from; announce your own public address (GET /whoami)"))?;
    if public_ip(ip) || (directory.loopback && ip.to_canonical().is_loopback()) { Ok(ip) }
    else { Err(format!("{from_shown} is not a public internet address")) }
}

/// A status probe of `url` at exactly `ip` with the identity recorded for
/// it, bounded in time.
async fn probe(url: &str, ip: IpAddr, identity: Identity) -> Option<MatchStatus> {
    tokio::time::timeout(PROBE_TIMEOUT, query_status_trusting(url, Some(ip), identity.trust())).await.ok()?.ok()
}

/// A new server's first probe: its status and the identity it proved.
async fn verify(url: &str, ip: IpAddr) -> Result<(MatchStatus, Identity), String> {
    match tokio::time::timeout(PROBE_TIMEOUT, identify_status_at(url, ip)).await {
        Err(_) => Err(format!("no answer from {} within {} s (is the UDP port forwarded to the server?)",
            ip.to_canonical(), PROBE_TIMEOUT.as_secs())),
        Ok(Err(e)) => Err(e.to_string()),
        Ok(Ok(found)) => Ok(found),
    }
}

async fn announce(State(directory): State<Directory>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap,
                  Json(body): Json<Announce>) -> Response {
    let Some(from) = requester(&directory, &headers, peer) else { return refusal(StatusCode::BAD_REQUEST) };
    // One spelling per address, so case or a written-out default port can't
    // list the same server twice (or the official one as a community entry).
    let Some(url) = canonical_url(&body.url) else { return refusal(StatusCode::BAD_REQUEST) };
    let fresh = match admit(&directory, &url, from, Instant::now()) {
        Ok(fresh) => fresh,
        Err(code) => return refusal(code),
    };
    // A listed server's refresh has its own pool, so new announces can't
    // starve it; a new server's verification runs one at a time per address.
    let (_permit, _verifying) = if fresh {
        let Some(verifying) = begin_verify(&directory, from) else { return refusal(StatusCode::TOO_MANY_REQUESTS) };
        let Ok(permit) = directory.verify.clone().try_acquire_owned() else { return refusal(StatusCode::SERVICE_UNAVAILABLE) };
        (permit, Some(verifying))
    } else {
        match tokio::time::timeout(REFRESH_WAIT, directory.refresh.clone().acquire_owned()).await {
            Ok(Ok(permit)) => (permit, None),
            _ => return refusal(StatusCode::SERVICE_UNAVAILABLE),
        }
    };
    // The claimed host must be the announcer's own public address, checked
    // on every announce; probes then go to that exact address.
    let ip = match verified_ip(&directory, &url, from).await { Ok(ip) => ip, Err(why) => return reply(StatusCode::FORBIDDEN, why) };
    if !fresh {
        let mut map = directory.announced.lock().unwrap();
        if let Some(a) = map.get_mut(&url) { a.announced = Instant::now(); a.ip = ip; a.owner = from; }
        return no_store(StatusCode::NO_CONTENT.into_response());
    }
    // And it must complete a TLS 1.3 handshake there and answer a status
    // query fit to list. The identity it proves is what later polls accept.
    let official = directory.official.lock().unwrap().iter().any(|o| same_ip(*o, ip));
    let (status, identity) = match verify(&url, ip).await {
        Ok(found) => found,
        Err(why) => return reply(StatusCode::UNPROCESSABLE_ENTITY, why),
    };
    if !status_ok(&status, official) {
        return reply(StatusCode::UNPROCESSABLE_ENTITY, format!("the server answered, but its status can't be listed \
            (name 1-64 plain characters not starting with \"{RESERVED_PREFIX}\", known map, 2-28 players, plain location)"));
    }
    let cap = per_address(&directory, from);
    let mut map = directory.announced.lock().unwrap();
    let key = rate_key(from);
    if !map.contains_key(&url) && (map.len() >= MAX_ANNOUNCED
        || map.values().filter(|a| rate_key(a.owner) == key).count() >= cap) {
        return refusal(StatusCode::SERVICE_UNAVAILABLE);
    }
    if let Some(old) = map.get(&url).filter(|a| a.identity != identity) {
        log::info!("{url} re-verified with a different identity ({:?} -> {:?})", old.identity, identity);
    }
    let now = Instant::now();
    let name = status.name.clone();
    map.insert(url.clone(), Announced { owner: from, ip, identity, announced: now, added: now, status: Some((status, now)) });
    log::info!("listed announced server {url}");
    let sha256 = match identity { Identity::Ca => None, Identity::Key(fp) => Some(fingerprint_hex(&fp)) };
    let message = match &sha256 {
        None => format!("listed \"{name}\" at {url} (CA certificate)"),
        Some(fp) => format!("listed \"{name}\" at {url} with key sha256={fp}"),
    };
    no_store((StatusCode::CREATED, Json(AnnounceReply { result: "listed".into(), message, sha256 })).into_response())
}

pub async fn serve(bind: &str, backend: String, advertised: String) -> io::Result<()> {
    serve_with(bind, backend, advertised, false).await
}

pub async fn serve_with(bind: &str, backend: String, advertised: String, trusted: bool) -> io::Result<()> {
    serve_official(bind, backend, advertised, &[], trusted).await
}

/// `serve_with`, also listing the `official` servers (quic:// URLs besides
/// the backend), each polled on its own loop like the backend.
pub async fn serve_official(bind: &str, backend: String, advertised: String, official: &[String], trusted: bool) -> io::Result<()> {
    endpoint_url(&advertised)?;
    endpoint_url(&backend)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let directory = Directory::new(advertised.clone(), trusted).with_official(official)?;
    run(listener, directory, backend, advertised).await
}

/// For integration tests on one machine: a directory on `listener` that
/// accepts announces from, and probes, loopback addresses (which a real
/// directory refuses). Never used by the directory binaries.
pub async fn serve_loopback_for_tests(listener: tokio::net::TcpListener, backend: String, advertised: String) -> io::Result<()> {
    let mut directory = Directory::new(advertised.clone(), false);
    directory.loopback = true;
    run(listener, directory, backend, advertised).await
}

async fn run(listener: tokio::net::TcpListener, directory: Directory, backend: String, advertised: String) -> io::Result<()> {
    let extra_pollers: Vec<_> = directory.extra.iter().cloned().map(|o| tokio::spawn(async move {
        loop {
            let status = tokio::time::timeout(PROBE_TIMEOUT, query_status_at(&o.url, None)).await;
            if let Ok(Ok(status)) = status { *o.status.lock().unwrap() = Some((status, Instant::now())); }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })).collect();
    // The official backend on its own loop, so nothing announced can delay it.
    let output = directory.status.clone();
    let official = directory.official.clone();
    let backend_poller = tokio::spawn(async move {
        loop {
            if let Ok(u) = endpoint_url(&advertised) {
                let host = u.host_str().unwrap_or("").trim_matches(['[', ']']).to_string();
                let port = u.port().unwrap_or(7777);
                let found = tokio::time::timeout(DNS_TIMEOUT, tokio::net::lookup_host((host.as_str(), port))).await;
                if let Ok(Ok(addrs)) = found {
                    let ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
                    *official.lock().unwrap() = ips;
                }
            }
            let status = tokio::time::timeout(PROBE_TIMEOUT, query_status_at(&backend, None)).await;
            if let Ok(Ok(status)) = status { *output.lock().unwrap() = Some((status, Instant::now())); }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    // Announced servers: each probed at its verified address, in parallel,
    // each bounded by PROBE_TIMEOUT; entries that stop answering are evicted.
    let announced = directory.announced.clone();
    let official = directory.official.clone();
    let poller = tokio::spawn(async move {
        loop {
            let targets: Vec<(String, IpAddr, Identity)> = {
                let mut map = announced.lock().unwrap();
                map.retain(|_, a| a.announced.elapsed() < ANNOUNCE_TTL
                    && a.status.as_ref().map_or(a.added.elapsed(), |(_, seen)| seen.elapsed()) < STALE_EVICT);
                map.iter().map(|(u, a)| (u.clone(), a.ip, a.identity)).collect()
            };
            let mut set = tokio::task::JoinSet::new();
            for (url, ip, identity) in targets { set.spawn(async move { let s = probe(&url, ip, identity).await; (url, ip, identity, s) }); }
            while let Some(done) = set.join_next().await {
                let Ok((url, ip, identity, Some(status))) = done else { continue };
                let is_official = official.lock().unwrap().iter().any(|o| same_ip(*o, ip));
                if !status_ok(&status, is_official) { continue; }
                // Only if it's still the entry that was probed: a re-verified
                // entry with a new identity keeps its own status.
                if let Some(a) = announced.lock().unwrap().get_mut(&url).filter(|a| a.identity == identity) {
                    a.status = Some((status, Instant::now()));
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
    let app = Router::new().route("/", get(listing)).route("/servers", get(listing))
        .route("/servers/{id}", get(details))
        .route("/announce", post(announce)).layer(DefaultBodyLimit::max(1024)).route("/whoami", get(whoami))
        .route("/healthz", get(health)).with_state(directory);
    log::info!("Directory listening on {}", listener.local_addr()?);
    let result = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async {
        #[cfg(unix)] {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
            tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        }
        #[cfg(not(unix))] { let _ = tokio::signal::ctrl_c().await; }
    }).await;
    poller.abort();
    backend_poller.abort();
    for task in extra_pollers { task.abort(); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn listings_expire_and_never_expose_stale_matches() {
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        assert_eq!(health(State(d.clone())).await.status(), StatusCode::SERVICE_UNAVAILABLE);
        *d.status.lock().unwrap() = Some((MatchStatus { name: "North Spine".into(), map: "Old Holler".into(), players: 6, max_players: 8,
            bots: 5, ..Default::default() }, Instant::now()));
        let response = listing(State(d.clone())).await;
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 4096).await.unwrap();
        let catalog: PublicDirectory = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(catalog.servers.len(), 1);
        assert_eq!(catalog.servers[0].map, "Old Holler");
        // Bots count as players; the listing says how many are bots.
        assert_eq!((catalog.servers[0].players, catalog.servers[0].bots), (6, 5));
        assert_eq!(details(Path("unknown".into()),State(d.clone())).await.status(),StatusCode::NOT_FOUND);
        assert_eq!(details(Path("dellcon-north-spine".into()),State(d.clone())).await.status(),StatusCode::OK);
        d.status.lock().unwrap().as_mut().unwrap().1 = Instant::now() - Duration::from_secs(13);
        assert!(live(&d).is_none());
        assert_eq!(details(Path("dellcon-north-spine".into()),State(d.clone())).await.status(),StatusCode::SERVICE_UNAVAILABLE);
        let bytes = axum::body::to_bytes(listing(State(d)).await.into_body(), 4096).await.unwrap();
        assert!(serde_json::from_slice::<PublicDirectory>(&bytes).unwrap().servers.is_empty());
    }

    #[tokio::test]
    async fn announced_servers_are_listed_by_port_and_lapse() {
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        let url = "quic://play.peakrunner.net:7778".to_string();
        let status = MatchStatus { name: "Football".into(), map: "Longfield - Football".into(), max_players: 8, ..Default::default() };
        d.announced.lock().unwrap().insert(url.clone(), Announced { owner: "198.51.100.7".parse().unwrap(), ip: "198.51.100.7".parse().unwrap(), identity: Identity::Ca,
            announced: Instant::now(), added: Instant::now(), status: Some((status, Instant::now())) });
        let bytes = axum::body::to_bytes(listing(State(d.clone())).await.into_body(), 4096).await.unwrap();
        let catalog: PublicDirectory = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(catalog.servers.len(), 1, "the backend is down; the announced one is up");
        assert_eq!((catalog.servers[0].id.as_str(), catalog.servers[0].port), ("play.peakrunner.net-7778", 7778));
        assert_eq!(details(Path("play.peakrunner.net-7778".into()), State(d.clone())).await.status(), StatusCode::OK);
        // Announces stop: it lapses.
        d.announced.lock().unwrap().get_mut(&url).unwrap().announced = Instant::now() - ANNOUNCE_TTL;
        assert!(announced_live(&d).is_empty());
    }

    #[test]
    fn announces_are_rate_limited_capped_and_owned() {
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        let a: IpAddr = "198.51.100.7".parse().unwrap();
        let b: IpAddr = "203.0.113.9".parse().unwrap();
        let t0 = Instant::now();
        assert_eq!(admit(&d, "quic://play.peakrunner.net:7777", a, t0), Err(StatusCode::NO_CONTENT), "the backend is already listed");
        assert_eq!(admit(&d, "https://x.example:7778", a, t0), Err(StatusCode::BAD_REQUEST));
        assert_eq!(admit(&d, "quic://x.example:80", a, t0), Err(StatusCode::BAD_REQUEST), "privileged ports refused");
        assert_eq!(admit(&d, "quic://x.example:7778", a, t0), Ok(true));
        assert_eq!(admit(&d, "quic://x.example:7778", a, t0), Err(StatusCode::TOO_MANY_REQUESTS), "too soon for the same server");
        for port in 7779..7778 + MAX_PER_ADDRESS as u16 {
            assert_eq!(admit(&d, &format!("quic://x.example:{port}"), a, t0), Ok(true), "another server on the same host announces on its own");
        }
        assert_eq!(admit(&d, &format!("quic://x.example:{}", 7778 + MAX_PER_ADDRESS), a, t0), Err(StatusCode::TOO_MANY_REQUESTS),
            "a burst of addresses is capped");
        let t1 = t0 + MIN_ANNOUNCE_GAP;
        d.announced.lock().unwrap().insert("quic://x.example:7778".into(), Announced { owner: a, ip: a, identity: Identity::Ca,
            announced: t0, added: t0, status: Some((MatchStatus::default(), t1)) });
        assert_eq!(admit(&d, "quic://x.example:7778", a, t1), Ok(false), "a heartbeat refreshes (after re-verifying)");
        assert_eq!(admit(&d, "quic://x.example:7778", b, t1), Err(StatusCode::FORBIDDEN), "someone else can't take it over");
        for port in 7779..7778 + MAX_PER_ADDRESS as u16 {
            d.announced.lock().unwrap().insert(format!("quic://x.example:{port}"), Announced { owner: a, ip: a, identity: Identity::Ca, announced: t1, added: t1, status: None });
        }
        let t2 = t1 + MIN_ANNOUNCE_GAP;
        assert_eq!(admit(&d, "quic://x.example:9000", a, t2), Err(StatusCode::TOO_MANY_REQUESTS), "per-address cap");
        assert_eq!(admit(&d, "quic://y.example:9000", b, t2), Ok(true), "another address has its own");
    }

    #[test]
    fn the_official_host_lists_a_hundred_servers_and_more() {
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        let official: IpAddr = "198.51.100.20".parse().unwrap();
        *d.official.lock().unwrap() = vec![official];
        let mut t = Instant::now();
        // Announced in bursts, as many servers on one host start up.
        for port in 7778..7778 + 120u16 {
            if (port - 7778) % 50 == 0 { t += MIN_ANNOUNCE_GAP; }
            let url = format!("quic://play.peakrunner.net:{port}");
            assert_eq!(admit(&d, &url, official, t), Ok(true), "{url}");
            d.announced.lock().unwrap().insert(url, Announced { owner: official, ip: official, identity: Identity::Ca, announced: t, added: t, status: None });
        }
        assert_eq!(d.announced.lock().unwrap().len(), 120);
        // The whole listing is the cap, whoever announces.
        for port in 0..(MAX_ANNOUNCED - 120) as u16 {
            d.announced.lock().unwrap().insert(format!("quic://z{port}.example:7777"), Announced { owner: official, ip: official, identity: Identity::Ca, announced: t, added: t, status: None });
        }
        let other: IpAddr = "203.0.113.30".parse().unwrap();
        assert_eq!(admit(&d, "quic://w.example:7777", other, t + MIN_ANNOUNCE_GAP), Err(StatusCode::SERVICE_UNAVAILABLE), "listing full");
    }

    #[tokio::test]
    async fn official_servers_are_listed_without_announcing() {
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false)
            .with_official(&["quic://PLAY.peakrunner.net:7778".into(), "quic://play.peakrunner.net:7779".into(),
                "quic://play.peakrunner.net:7777".into()]).unwrap();
        assert_eq!(d.extra.len(), 2, "the backend isn't listed twice");
        assert!(Directory::new("quic://a.example:7777".into(), false).with_official(&["https://x.example".into()]).is_err());
        let a: IpAddr = "198.51.100.7".parse().unwrap();
        assert_eq!(admit(&d, "quic://play.peakrunner.net:7778", a, Instant::now()), Err(StatusCode::NO_CONTENT), "already official");
        *d.extra[0].status.lock().unwrap() = Some((MatchStatus { name: "Springdale Football".into(), map: "Longfield - Football".into(),
            max_players: 16, ..Default::default() }, Instant::now()));
        let bytes = axum::body::to_bytes(listing(State(d.clone())).await.into_body(), 4096).await.unwrap();
        let catalog: PublicDirectory = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(catalog.servers.len(), 1, "only the answering official server");
        assert_eq!((catalog.servers[0].id.as_str(), catalog.servers[0].port, catalog.servers[0].name.as_str()),
            ("play.peakrunner.net-7778", 7778, "Springdale Football"));
        assert_eq!(details(Path("play.peakrunner.net-7778".into()), State(d.clone())).await.status(), StatusCode::OK);
        assert_eq!(details(Path("play.peakrunner.net-7779".into()), State(d.clone())).await.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn new_announces_cannot_starve_refreshes_and_verify_one_at_a_time_per_address() {
        // 198.18.0.0/15 is a benchmarking range: counted as public here, never
        // routed, and a refresh needs no probe (an IP literal needs no DNS).
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        let owner: IpAddr = "198.18.0.7".parse().unwrap();
        let peer = SocketAddr::new(owner, 50_000);
        let listed = "quic://198.18.0.7:7778".to_string();
        let t0 = Instant::now() - MIN_ANNOUNCE_GAP;
        d.announced.lock().unwrap().insert(listed.clone(), Announced { owner, ip: owner, identity: Identity::Ca, announced: t0, added: t0,
            status: Some((MatchStatus::default(), Instant::now())) });
        let post = |url: &str| announce(State(d.clone()), ConnectInfo(peer), HeaderMap::new(), Json(Announce { url: url.into() }));
        // Every new-server verification slot is busy, and this address has
        // one of its own in flight.
        let _busy: Vec<_> = (0..VERIFY_CONCURRENCY).map(|_| d.verify.clone().try_acquire_owned().unwrap()).collect();
        let held = begin_verify(&d, owner).unwrap();
        assert!(begin_verify(&d, "198.18.0.7".parse().unwrap()).is_none());
        assert!(begin_verify(&d, "198.18.0.8".parse().unwrap()).is_some(), "other addresses verify on their own");
        assert_eq!(post("quic://198.18.0.7:7790").await.status(), StatusCode::TOO_MANY_REQUESTS, "one new verification per address");
        let before = d.announced.lock().unwrap()[&listed].announced;
        assert_eq!(post(&listed).await.status(), StatusCode::NO_CONTENT, "the listed server still refreshes");
        assert!(d.announced.lock().unwrap()[&listed].announced > before);
        drop(held);
        assert_eq!(post("quic://198.18.0.7:7791").await.status(), StatusCode::SERVICE_UNAVAILABLE, "new verifications are capped");
        assert!(d.verifying.lock().unwrap().is_empty(), "the refused verification released its address");
    }

    #[test]
    fn announce_hardening_rules() {
        // IPv6 is limited per /64; IPv4 per address.
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::9".parse().unwrap();
        assert_eq!(rate_key(a), rate_key(b));
        assert_ne!(rate_key("198.51.100.7".parse().unwrap()), rate_key("198.51.100.8".parse().unwrap()));
        // Only public addresses can be listed (no LAN, loopback, CGNAT, link-local).
        for bad in ["127.0.0.1", "10.1.2.3", "192.168.1.20", "172.20.0.5", "100.64.1.1", "169.254.1.1", "::1", "fe80::1", "fd00::1"] {
            assert!(!public_ip(bad.parse().unwrap()), "{bad}");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        // Status validation and reserved names.
        let good = MatchStatus { name: "Community CTF".into(), map: "Old Holler - CTF".into(), players: 3, max_players: 8, ..Default::default() };
        assert!(status_ok(&good, false));
        assert!(status_ok(&MatchStatus { location: "New York, US".into(), ..good.clone() }, false));
        let fake = MatchStatus { name: "Springdale Central".into(), ..good.clone() };
        assert!(!status_ok(&fake, false) && status_ok(&fake, true), "official names only from the official host");
        for bad in [MatchStatus { max_players: 29, ..good.clone() }, MatchStatus { players: 9, ..good.clone() },
                    MatchStatus { name: "x".repeat(65), ..good.clone() }, MatchStatus { name: "bad\u{7}name".into(), ..good.clone() },
                    MatchStatus { map: "<script>".into(), ..good.clone() },
                    MatchStatus { location: "<b>NYC</b>".into(), ..good.clone() },
                    MatchStatus { location: "x".repeat(41), ..good.clone() }] {
            assert!(!status_ok(&bad, true), "{:?}", bad.name);
        }
    }

    /// A status-only QUIC server on loopback. `config` presents `cert` and
    /// signs with `key` (which may not match); the endpoint's config and the
    /// name it reports can change while it runs.
    mod status_server {
        use std::sync::{Arc, Mutex};
        use rustls::{pki_types::{CertificateDer, PrivateKeyDer}, server::{ClientHello, ResolvesServerCert}, sign::{CertifiedKey, SigningKey}};
        use peakrunner_discovery::{MatchStatus, StatusReply, StatusRequest, quic::{ALPN, read_control, send_control}};
        pub fn keypair() -> (CertificateDer<'static>, Arc<dyn SigningKey>) {
            let c = rcgen::generate_simple_self_signed(vec!["peakrunner-server.invalid".into()]).unwrap();
            let key = rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(c.key_pair.serialize_der().into())).unwrap();
            (c.cert.der().clone(), key)
        }
        #[derive(Debug)]
        struct Fixed(Arc<CertifiedKey>);
        impl ResolvesServerCert for Fixed {
            fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> { Some(self.0.clone()) }
        }
        pub fn config(cert: CertificateDer<'static>, key: Arc<dyn SigningKey>) -> quinn::ServerConfig {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let mut tls = rustls::ServerConfig::builder_with_provider(provider).with_protocol_versions(&[&rustls::version::TLS13]).unwrap()
                .with_no_client_auth().with_cert_resolver(Arc::new(Fixed(Arc::new(CertifiedKey::new(vec![cert], key)))));
            tls.alpn_protocols = vec![ALPN.to_vec()];
            quinn::ServerConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap()))
        }
        pub fn start(config: quinn::ServerConfig, name: Arc<Mutex<String>>) -> quinn::Endpoint {
            let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
            let server = endpoint.clone();
            tokio::spawn(async move {
                while let Some(incoming) = server.accept().await {
                    let status = MatchStatus { name: name.lock().unwrap().clone(), map: "Old Holler - CTF".into(), max_players: 8, ..Default::default() };
                    tokio::spawn(async move {
                        let Ok(conn) = incoming.await else { return };
                        let Ok((mut send, mut recv)) = conn.accept_bi().await else { return };
                        if read_control::<StatusRequest>(&mut recv).await.is_err() { return; }
                        let _ = send_control(&mut send, &StatusReply::Status { status }).await;
                        let _ = send.finish();
                        let _ = send.stopped().await;
                    });
                }
            });
            endpoint
        }
    }

    #[tokio::test]
    async fn an_ip_literal_must_be_the_requesters_own_public_address() {
        let d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        let me: IpAddr = "198.18.0.7".parse().unwrap();
        assert_eq!(verified_ip(&d, "quic://198.18.0.7:7778", me).await, Ok(me));
        let other = verified_ip(&d, "quic://198.18.0.9:7778", me).await.unwrap_err();
        assert!(other.contains("is not 198.18.0.7"), "{other}");
        let v6: IpAddr = "2001:4860::7".parse().unwrap();
        assert_eq!(verified_ip(&d, "quic://[2001:4860::7]:7778", v6).await, Ok(v6));
        assert!(verified_ip(&d, "quic://[2001:4860::8]:7778", v6).await.is_err(), "IPv6 must match exactly too, not per /64");
        // An IPv4 request seen as an IPv4-mapped IPv6 peer still matches.
        assert!(verified_ip(&d, "quic://198.18.0.7:7778", "::ffff:198.18.0.7".parse().unwrap()).await.is_ok());
        for private in ["127.0.0.1", "192.168.1.20", "10.0.0.2"] {
            let ip: IpAddr = private.parse().unwrap();
            assert!(verified_ip(&d, &format!("quic://{ip}:7778"), ip).await.unwrap_err().contains("not a public"), "{private}");
        }
        // Loopback only for the integration-test directory.
        let mut local = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        local.loopback = true;
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        assert_eq!(verified_ip(&local, "quic://127.0.0.1:7778", lo).await, Ok(lo));
    }

    async fn body(response: Response) -> (StatusCode, serde_json::Value) {
        let code = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 8192).await.unwrap();
        (code, if bytes.is_empty() { serde_json::Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_key_is_vouched_only_after_a_validated_handshake_and_a_new_key_needs_a_fresh_probe() {
        use peakrunner_discovery::quic::fingerprint;
        let mut d = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        d.loopback = true;
        let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
        let post = |d: &Directory, url: &str| {
            d.last_announce.lock().unwrap().clear();   // the tests' announces needn't wait out MIN_ANNOUNCE_GAP
            announce(State(d.clone()), ConnectInfo(peer), HeaderMap::new(), Json(Announce { url: url.into() }))
        };
        let (cert_a, key_a) = status_server::keypair();
        let (cert_b, key_b) = status_server::keypair();
        let (fp_a, fp_b) = (fingerprint(&cert_a), fingerprint(&cert_b));
        // Nothing answers at a closed port.
        let closed = std::net::UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let (code, reply) = body(post(&d, &format!("quic://127.0.0.1:{closed}")).await).await;
        assert_eq!((code, reply["result"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("unreachable")), "{reply}");
        assert!(d.announced.lock().unwrap().is_empty());
        // Certificate A without its key: no validated handshake, no listing.
        let name = std::sync::Arc::new(std::sync::Mutex::new("Springdale Central".to_string()));
        let server = status_server::start(status_server::config(cert_a.clone(), key_b.clone()), name.clone());
        let at = server.local_addr().unwrap();
        let url = format!("quic://127.0.0.1:{}", at.port());
        assert_eq!(body(post(&d, &url).await).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(d.announced.lock().unwrap().is_empty());
        // A reserved name is refused even with a good handshake.
        server.set_server_config(Some(status_server::config(cert_a.clone(), key_a.clone())));
        assert_eq!(body(post(&d, &url).await).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(d.announced.lock().unwrap().is_empty());
        // The real server A: listed by its key, in `vouched` only.
        *name.lock().unwrap() = "Home Server".into();
        let (code, reply) = body(post(&d, &url).await).await;
        assert_eq!((code, reply["sha256"].as_str()), (StatusCode::CREATED, Some(fingerprint_hex(&fp_a).as_str())), "{reply}");
        assert_eq!(d.announced.lock().unwrap()[&url].identity, Identity::Key(fp_a));
        let (_, listed) = body(listing(State(d.clone())).await).await;
        assert_eq!(listed["servers"], serde_json::json!([]), "released clients never see it");
        assert_eq!(listed["vouched"][0]["sha256"].as_str(), Some(fingerprint_hex(&fp_a).as_str()));
        assert_eq!(listed["vouched"][0]["host"].as_str(), Some(url.as_str()));
        assert!(probe(&url, at.ip(), Identity::Key(fp_a)).await.is_some(), "polls accept key A");
        // The same address comes back with key B.
        server.set_server_config(Some(status_server::config(cert_b, key_b)));
        assert!(probe(&url, at.ip(), Identity::Key(fp_a)).await.is_none(), "polls pinned to A refuse B");
        // While A's status is fresh an announce is only a refresh: B doesn't
        // replace A without a probe.
        assert_eq!(body(post(&d, &url).await).await.0, StatusCode::NO_CONTENT);
        assert_eq!(d.announced.lock().unwrap()[&url].identity, Identity::Key(fp_a));
        // Once A's status goes stale (the pinned polls failing), the next
        // announce re-verifies from scratch and records B.
        d.announced.lock().unwrap().get_mut(&url).unwrap().status.as_mut().unwrap().1 = Instant::now() - FRESH;
        let (_, listed) = body(listing(State(d.clone())).await).await;
        assert!(listed.get("vouched").is_none(), "a stale entry isn't listed with its old key");
        let (code, reply) = body(post(&d, &url).await).await;
        assert_eq!((code, reply["sha256"].as_str()), (StatusCode::CREATED, Some(fingerprint_hex(&fp_b).as_str())), "{reply}");
        assert_eq!(d.announced.lock().unwrap()[&url].identity, Identity::Key(fp_b));
        server.close(0u32.into(), b"done");
    }

    #[tokio::test]
    async fn whoami_reports_the_requester_and_trusts_cloudflare_only_when_configured() {
        let peer: SocketAddr = "198.18.0.7:40000".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", "203.0.113.50".parse().unwrap());
        let direct = Directory::new("quic://play.peakrunner.net:7777".into(), false);
        let (_, me) = body(whoami(State(direct), ConnectInfo(peer), headers.clone()).await).await;
        assert_eq!(me["address"], "198.18.0.7");
        let tunnel = Directory::new("quic://play.peakrunner.net:7777".into(), true);
        let (_, me) = body(whoami(State(tunnel.clone()), ConnectInfo(peer), headers).await).await;
        assert_eq!(me["address"], "203.0.113.50");
        assert_eq!(whoami(State(tunnel), ConnectInfo(peer), HeaderMap::new()).await.status(), StatusCode::BAD_REQUEST);
    }
}
