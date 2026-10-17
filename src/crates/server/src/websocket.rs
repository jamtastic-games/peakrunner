//! Optional legacy WebSocket match endpoint. Never a directory service.
use std::{collections::HashMap, io, net::{IpAddr, SocketAddr, TcpStream}, sync::{Arc, Mutex}, time::{Duration, Instant}};
use axum::{Router, Json, extract::{State, ConnectInfo, WebSocketUpgrade, ws::{Message, WebSocket}}, http::{HeaderMap, StatusCode, header}, response::{IntoResponse, Response}, routing::get};
use tokio::{sync::Semaphore, time::timeout};
use peakrunner_discovery::MatchStatus;
use crate::{GameHost, proto::{ClientMsg, ServerMsg}, wire::Framed};
struct Limit { active: usize, tokens: f32, seen: Instant }
#[derive(Default)]
struct Limits { ips: HashMap<IpAddr, Limit> }
impl Limits {
    fn enter(&mut self, ip: IpAddr) -> bool {
        self.ips.retain(|_, v| v.active > 0 || v.seen.elapsed() < Duration::from_secs(300));
        if self.ips.len() >= 4096 && !self.ips.contains_key(&ip) { return false; }
        let now = Instant::now();
        let l = self.ips.entry(ip).or_insert(Limit { active: 0, tokens: 12.0, seen: now });
        l.tokens = (l.tokens + l.seen.elapsed().as_secs_f32() / 3.0).min(12.0);
        l.seen = now;
        if l.active >= 8 || l.tokens < 1.0 { return false; }
        l.tokens -= 1.0; l.active += 1; true
    }
}
struct Admission { limits: Arc<Mutex<Limits>>, ip: IpAddr }
impl Drop for Admission { fn drop(&mut self) {
    if let Some(l) = self.limits.lock().unwrap().ips.get_mut(&self.ip) { l.active = l.active.saturating_sub(1); }
} }
#[derive(Clone)]
struct Edge {
    backend: Option<SocketAddr>, trusted_proxy: bool,
    slots: Arc<Semaphore>, limits: Arc<Mutex<Limits>>,
    status: Arc<Mutex<Option<(MatchStatus, Instant)>>>,
}
fn client_ip(headers: &HeaderMap, peer: IpAddr, trusted_proxy: bool) -> Option<IpAddr> {
    if !trusted_proxy { return Some(peer); }
    // This mode must only be reachable from cloudflared on the isolated network.
    if headers.get("x-forwarded-proto")?.to_str().ok()? != "https" { return None; }
    headers.get("cf-connecting-ip")?.to_str().ok()?.parse().ok()
}
async fn upgrade(State(edge): State<Edge>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let Some(ip) = client_ip(&headers, peer.ip(), edge.trusted_proxy) else { return StatusCode::FORBIDDEN.into_response(); };
    if let Some(origin) = headers.get(header::ORIGIN) {
        if !["https://peakrunner.net", "https://www.peakrunner.net"].iter().any(|s| origin == *s) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let Ok(permit) = edge.slots.clone().try_acquire_owned() else { return StatusCode::SERVICE_UNAVAILABLE.into_response(); };
    if !edge.limits.lock().unwrap().enter(ip) { return StatusCode::TOO_MANY_REQUESTS.into_response(); }
    let admission = Admission { limits: edge.limits.clone(), ip };
    ws.max_message_size(8192).max_frame_size(8192).write_buffer_size(0).max_write_buffer_size(524_288)
        .on_upgrade(move |socket| async move {
            let _permit = permit; let _admission = admission;
            if let Some(addr) = edge.backend { bridge(socket, addr).await; }
        })
}
async fn bridge(mut socket: WebSocket, addr: SocketAddr) {
    let Ok(Ok(stream)) = tokio::task::spawn_blocking(move || TcpStream::connect_timeout(&addr, Duration::from_secs(1))).await else { return; };
    let Ok(mut wire) = Framed::new(stream, 262_144) else { return; };
    let mut poll = tokio::time::interval(Duration::from_millis(2));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last = Instant::now();
    let mut tokens = 16.0f32;
    let mut refill = Instant::now();
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                tokens = (tokens + refill.elapsed().as_secs_f32() * 120.0).min(16.0);
                refill = Instant::now(); tokens -= 1.0;
                if tokens < 0.0 { break; }
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let Ok(message) = serde_json::from_str::<ClientMsg>(&text) else { break; };
                        if wire.send(&message).is_err() { break; }
                        last = Instant::now();
                    }
                    Some(Ok(Message::Pong(_))) => {},
                    Some(Ok(Message::Ping(bytes))) => { if !matches!(timeout(Duration::from_secs(1), socket.send(Message::Pong(bytes))).await, Ok(Ok(()))) { break; } },
                    _ => break,
                }
            }
            _ = poll.tick() => {
                if last.elapsed() > Duration::from_secs(5) || wire.flush().is_err() { break; }
                let Ok(messages) = wire.receive::<ServerMsg>() else { break; };
                for msg in messages {
                    let Ok(text) = serde_json::to_string(&msg) else { return; };
                    if !matches!(timeout(Duration::from_secs(1), socket.send(Message::Text(text.into()))).await, Ok(Ok(()))) { return; }
                }
            }
        }
    }
    let _ = timeout(Duration::from_millis(200), socket.send(Message::Close(None))).await;
}
fn live(edge: &Edge) -> Option<MatchStatus> {
    edge.status.lock().unwrap().as_ref().filter(|(_, seen)| seen.elapsed() < Duration::from_secs(12)).map(|(s, _)| s.clone())
}
async fn status(State(edge): State<Edge>) -> Response {
    match live(&edge) { Some(status) => Json(status).into_response(), None => StatusCode::SERVICE_UNAVAILABLE.into_response() }
}
pub async fn serve(bind: &str, trusted_proxy: bool) -> io::Result<()> {
    let mut edge = Edge { backend: None, trusted_proxy, slots: Arc::new(Semaphore::new(16)),
        limits: Arc::new(Mutex::new(Limits::default())), status: Arc::new(Mutex::new(None)) };
        let host = GameHost::bind("127.0.0.1:0", "Springdale Central", 8, "old-holler")?
            .with_password(std::env::var("PEAKRUNNER_MATCH_PASSWORD").unwrap_or_default());
        edge.backend = Some(host.local_addr());
        let handle = host.spawn();
        let input = handle.status.clone(); let output = edge.status.clone();
        tokio::spawn(async move {
            let mut tick = 0;
            loop {
                let status = input.lock().unwrap().clone();
                if status.tick > tick { tick = status.tick; *output.lock().unwrap() = Some((status, Instant::now())); }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        let game_handle = handle;
    let app = Router::new().route("/match", get(upgrade)).route("/status", get(status));
    let app = app.route("/healthz", get(status)).with_state(edge);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    log::info!("WebSocket match listening on {}", listener.local_addr()?);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async {
        #[cfg(unix)] {
            let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
            tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
        }
        #[cfg(not(unix))] { let _ = tokio::signal::ctrl_c().await; }
    }).await?;
    drop(game_handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn bounded_admission_and_proxy_trust() {
        let ip = "127.0.0.1".parse().unwrap(); let mut limits = Limits::default();
        for _ in 0..8 { assert!(limits.enter(ip)); } assert!(!limits.enter(ip));
        let mut headers = HeaderMap::new();
        assert_eq!(client_ip(&headers, ip, false), Some(ip));
        assert_eq!(client_ip(&headers, ip, true), None);
        headers.insert("cf-connecting-ip", "1.2.3.4".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert_eq!(client_ip(&headers, ip, true), Some("1.2.3.4".parse().unwrap()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn websocket_match_rejects_forgery_and_releases_disconnected_slots() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap(); drop(listener);
        let server = tokio::spawn(async move { serve(&addr.to_string(), false).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let endpoint = format!("ws://{addr}/match");
        let a = peakrunner_net::connect(&endpoint, 0, "A").unwrap();
        let b = peakrunner_net::connect(&endpoint, 0, "B").unwrap();
        for _ in 0..200 {
            if a.lobby().players.len() == 2 && b.lobby().connected { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(a.lobby().connected && b.lobby().connected);
        assert_ne!(a.lobby().player_id, b.lobby().player_id);
        // Malformed and oversized frames must close without affecting valid peers.
        for payload in [r#"{"op":"pose","position":[999,999,999]}"#.to_string(), "x".repeat(4096)] {
            let endpoint = endpoint.clone();
            tokio::task::spawn_blocking(move || {
                let (mut bad, _) = tungstenite::connect(&endpoint).unwrap();
                if let tungstenite::stream::MaybeTlsStream::Plain(s) = bad.get_mut() { s.set_read_timeout(Some(Duration::from_secs(2))).unwrap(); }
                let _ = bad.send(tungstenite::Message::Text(payload.into()));
                assert!(matches!(bad.read(), Err(_) | Ok(tungstenite::Message::Close(_))));
            }).await.unwrap();
        }
        assert!(a.lobby().connected);
        drop(b);
        for _ in 0..200 {
            if a.lobby().players.len() == 1 { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(a.lobby().players, vec!["A"]);
        drop(a); server.abort(); let _ = server.await;
    }
}
