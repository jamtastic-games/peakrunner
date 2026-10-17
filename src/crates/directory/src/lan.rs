use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, TcpListener};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use peakrunner_discovery::{DirRequest, DirResponse, ServerAdvert};
use peakrunner_discovery::wire::{Framed, private_bind};

const STALE: Duration = Duration::from_secs(8);
struct Listing { advert: ServerAdvert, token: String, seen: Instant, owner: IpAddr }
pub struct DirectoryHandle {
    pub addr: std::net::SocketAddr, stop: Arc<AtomicBool>, thread: Option<JoinHandle<()>>,
}
impl Drop for DirectoryHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}
pub fn serve(bind: &str) -> io::Result<DirectoryHandle> {
    let listener = TcpListener::bind(private_bind(bind)?)?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = thread::spawn(move || {
        let mut listings = HashMap::<String, Listing>::new();
        let mut peers: Vec<(Framed, Instant, bool)> = Vec::new();
        while !flag.load(Ordering::Relaxed) {
            for _ in 0..8 {
                match listener.accept() {
                    Ok((stream, _)) if peers.len() < 32 => {
                        if let Ok(wire) = Framed::new(stream, 2048) { peers.push((wire, Instant::now(), false)); }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            listings.retain(|_, l| l.seen.elapsed() < STALE);
            peers.retain_mut(|(wire, start, replied)| {
                if start.elapsed() > Duration::from_secs(2) { return false; }
                if *replied { return wire.flush().is_ok(); }
                match wire.receive::<DirRequest>() {
                    Ok(requests) => {
                        if let Some(request) = requests.into_iter().next() {
                            let Ok(peer) = wire.stream.peer_addr() else { return false; };
                            let response = handle(request, peer.ip(), &mut listings);
                            *replied = true;
                            wire.send(&response).is_ok()
                        } else { true }
                    }
                    Err(_) => false,
                }
            });
            thread::sleep(Duration::from_millis(10));
        }
    });
    Ok(DirectoryHandle { addr, stop, thread: Some(thread) })
}
fn handle(request: DirRequest, peer: IpAddr, listings: &mut HashMap<String, Listing>) -> DirResponse {
    let error = |message: &str| DirResponse::Error { message: message.into() };
    match request {
        DirRequest::Register { name, host, port, players, max_players, map } => {
            if listings.len() >= 128 || listings.values().filter(|l| l.owner == peer).count() >= 8 {
                return error("listing limit");
            }
            if host.parse::<IpAddr>().ok() != Some(peer) || name.is_empty() || name.len() > 64
                || name.chars().any(char::is_control) || port == 0 || players > max_players
                || !(2..=8).contains(&max_players) || !peakrunner_discovery::valid_map_label(&map) {
                return error("invalid advert; advertised IP must match registering peer");
            }
            let mut random = [0u8; 32];
            if getrandom::fill(&mut random).is_err() { return error("entropy unavailable"); }
            let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            let id = hex(&random[..8]);
            let token = hex(&random[8..]);
            listings.insert(id.clone(), Listing { advert: ServerAdvert {
                id: id.clone(), name, host, port, players, max_players, bots: 0, map, location: String::new(), sha256: None },
                token: token.clone(), seen: Instant::now(), owner: peer });
            DirResponse::Registered { id, token }
        }
        DirRequest::Heartbeat { id, token, players } => {
            if let Some(l) = listings.get_mut(&id) {
                if l.token != token || l.owner != peer || players > l.advert.max_players { return error("not authorized"); }
                l.seen = Instant::now(); l.advert.players = players; DirResponse::Ok
            } else { error("unknown server") }
        }
        DirRequest::Unregister { id, token } => {
            if !listings.get(&id).is_some_and(|l| l.token == token && l.owner == peer) { return error("not authorized"); }
            listings.remove(&id); DirResponse::Ok
        }
        DirRequest::List => DirResponse::Servers { servers: listings.values().map(|l| l.advert.clone()).collect() },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listings_require_owner_token_and_cannot_redirect_clients() {
        let ip = "127.0.0.1".parse().unwrap();
        let mut state = HashMap::new();
        let reg = |host: &str| DirRequest::Register { name: "Test".into(), host: host.into(),
            port: 7781, players: 0, max_players: 8, map: "Valley".into() };
        assert!(matches!(handle(reg("1.2.3.4"), ip, &mut state), DirResponse::Error { .. }));
        let DirResponse::Registered { id, token } = handle(reg("127.0.0.1"), ip, &mut state) else { panic!() };
        assert!(matches!(handle(DirRequest::Unregister { id: id.clone(), token: "wrong".into() }, ip, &mut state), DirResponse::Error { .. }));
        assert_eq!(state.len(), 1);
        assert!(matches!(handle(DirRequest::Heartbeat { id: id.clone(), token: token.clone(), players: 2 }, ip, &mut state), DirResponse::Ok));
        assert!(matches!(handle(DirRequest::Unregister { id, token }, ip, &mut state), DirResponse::Ok));
        assert!(state.is_empty());
    }
    #[test]
    fn every_current_and_legacy_map_can_be_advertised() {
        let ip = "127.0.0.1".parse().unwrap();
        let mut state = HashMap::new();
        let reg = |map: &str| DirRequest::Register { name: "Test".into(), host: "127.0.0.1".into(),
            port: 7781, players: 0, max_players: 8, map: map.into() };
        for map in ["old-holler", "tower-complex", "cairnhold", "frostline",
            "dustreach", "Old Holler", "Tower Complex", "Cairnhold", "Frostline",
            "Dustreach", "Valley", "Some Future Map", "some-future-map"] {
            state.clear();
            assert!(matches!(handle(reg(map), ip, &mut state), DirResponse::Registered { .. }), "{map}");
            let DirResponse::Servers { servers } = handle(DirRequest::List, ip, &mut state) else { panic!() };
            assert_eq!(servers[0].map, map);
        }
        for map in ["", "<script>x</script>", "a\u{202e}b", "x\ny", &"m".repeat(40)] {
            assert!(matches!(handle(reg(map), ip, &mut state), DirResponse::Error { .. }), "{map:?}");
        }
    }
}
