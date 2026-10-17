//! Getting players to the server: the invite link, announcing to the public
//! directory (and saying plainly whether the directory could reach us), and
//! optionally asking the router to forward the port (UPnP).
use std::{net::{IpAddr, SocketAddr, UdpSocket}, sync::{Arc, Mutex}, time::{Duration, Instant}};
use peakrunner_discovery::{AnnounceReply, quic::{Fingerprint, invite_link}};
use crate::setup::quic_url;

/// How often a listed server announces (the directory's ANNOUNCE_EVERY).
const ANNOUNCE_EVERY: Duration = Duration::from_secs(20);
/// A private server's one address lookup, per attempt.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(4);
/// UPnP leases last an hour and are renewed at half that.
const LEASE: Duration = Duration::from_secs(3600);

pub struct Reach {
    /// Where to announce (`https://.../announce`), if at all.
    pub directory: Option<String>,
    /// The announced URL (or a private server's known public address);
    /// None to ask the directory for our address.
    pub url: Option<String>,
    /// A private server run with `--publicip`: the directory to ask once
    /// for its public address. None: no directory is contacted.
    pub lookup: Option<String>,
    pub port: u16,
    pub upnp: bool,
    /// The generated key's fingerprint, for invite links (None with a CA certificate).
    pub pin: Option<Fingerprint>,
    /// Talking to a person at a terminal (plain messages on stdout) rather
    /// than a log collector.
    pub talk: bool,
}

/// A port mapping to remove at shutdown.
#[derive(Clone, Default)]
pub struct Mapping(Arc<Mutex<Option<(igd_next::Gateway, u16)>>>);
impl Mapping {
    pub fn release(&self) {
        if let Some((gateway, port)) = self.0.lock().unwrap().take() {
            let _ = gateway.remove_port(igd_next::PortMappingProtocol::UDP, port);
        }
    }
}

fn say(talk: bool, warn: bool, text: &str) {
    if talk { println!("{text}"); } else if warn { log::warn!("{}", text.replace('\n', " ")); } else { log::info!("{}", text.replace('\n', " ")); }
}

/// This computer's address on its local network (no packet is sent: a
/// connected UDP socket only picks a route).
pub fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

/// `host:port` as an invite link writes it.
fn host_port(ip: IpAddr, port: u16) -> String { SocketAddr::new(ip, port).to_string() }

/// Plain port-forwarding instructions.
pub fn forwarding_help(port: u16, lan: Option<IpAddr>) -> String {
    let lan = lan.map_or("this computer".to_string(), |ip| format!("this computer ({ip})"));
    format!("Players on the internet can't reach this server until UDP port {port} is forwarded to it:\n\
        \x20 1. Open your router's settings page (often http://192.168.1.1 or http://192.168.0.1).\n\
        \x20 2. Find \"Port forwarding\" (sometimes \"Virtual server\" or \"NAT\").\n\
        \x20 3. Forward UDP port {port} to {lan}, port {port}.\n\
        \x20 4. Allow peakrunner-server through this computer's firewall for UDP {port}.\n\
        The server keeps running and checks again every {} seconds. Players on your own network can join with the local invite link.",
        ANNOUNCE_EVERY.as_secs())
}

/// `https://dir.example/announce` → `https://dir.example/whoami`.
fn whoami_url(directory: &str) -> String {
    format!("{}/whoami", directory.strip_suffix("/announce").unwrap_or(directory).trim_end_matches('/'))
}

/// An address players on the internet could reach (not LAN, loopback,
/// link-local or carrier-grade NAT).
fn public(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(a) => !(a.is_private() || a.is_loopback() || a.is_link_local() || a.is_unspecified()
            || (a.octets()[0] == 100 && (a.octets()[1] & 0xc0) == 64)),
        IpAddr::V6(a) => !(a.is_loopback() || a.is_unspecified() || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80),
    }
}

/// What one announce told us, for the operator.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome { Listed(String), Refreshed, Unreachable(String), WrongAddress(String), Refused(u16, String), Failed(String) }

/// Announce once. Transport failures are `Failed`; every status code maps
/// to an outcome, with the directory's explanation when it sent one.
pub fn announce(directory: &str, url: &str) -> Outcome {
    let body = peakrunner_discovery::Announce { url: url.into() };
    let https = directory.starts_with("https://");
    match peakrunner_discovery::http::http_post_json_reply(directory, &body, https) {
        Err(e) => Outcome::Failed(e.to_string()),
        Ok((code, bytes)) => {
            let why = serde_json::from_slice::<AnnounceReply>(&bytes).map(|r| r.message).unwrap_or_default();
            match code {
                201 => Outcome::Listed(if why.is_empty() { format!("listed {url}") } else { why }),
                200..=299 => Outcome::Refreshed,
                422 => Outcome::Unreachable(if why.is_empty() { "the directory couldn't query this server".into() } else { why }),
                403 => Outcome::WrongAddress(if why.is_empty() { format!("the directory didn't accept {url} as this computer's address") } else { why }),
                code => Outcome::Refused(code, why),
            }
        }
    }
}

/// Try to forward UDP `port` to this computer with UPnP; the router's
/// external address on success.
fn map_port(port: u16, mapping: &Mapping) -> Result<IpAddr, String> {
    let gateway = igd_next::search_gateway(igd_next::SearchOptions { timeout: Some(Duration::from_secs(3)), ..Default::default() })
        .map_err(|e| format!("no UPnP router answered ({e})"))?;
    let local = {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        socket.connect(gateway.addr).map_err(|e| e.to_string())?;
        socket.local_addr().map_err(|e| e.to_string())?.ip()
    };
    let add = |lease: u32| gateway.add_port(igd_next::PortMappingProtocol::UDP, port, SocketAddr::new(local, port), lease, "PeakRunner server");
    match add(LEASE.as_secs() as u32) {
        Ok(()) => {}
        Err(igd_next::AddPortError::OnlyPermanentLeasesSupported) => add(0).map_err(|e| format!("the router refused ({e})"))?,
        Err(e) => return Err(format!("the router refused ({e})")),
    }
    let external = gateway.get_external_ip().map_err(|e| format!("mapped, but the router wouldn't say its address ({e})"))?;
    *mapping.0.lock().unwrap() = Some((gateway, port));
    Ok(external)
}

/// Start the invite/announce/UPnP work on its own thread. Returns the
/// mapping to release at shutdown.
pub fn start(reach: Reach) -> Mapping {
    let mapping = Mapping::default();
    let held = mapping.clone();
    std::thread::Builder::new().name("peakrunner-reach".into()).spawn(move || run(reach, held)).expect("spawn reach thread");
    mapping
}

fn run(reach: Reach, mapping: Mapping) {
    let Reach { directory, mut url, lookup, port, upnp, pin, talk } = reach;
    let lan = lan_ip();
    if let (Some(pin), Some(lan), Some(_)) = (pin, lan, &directory) {
        say(talk, false, &format!("Invite link for players on your network:\n  {}", invite_link(&host_port(lan, port), &pin)));
    }
    let mut external = None;
    let mut mapped_at = None;
    let mut upnp_result = None;
    if upnp {
        match map_port(port, &mapping) {
            Ok(ip) => {
                say(talk, false, &format!("Your router forwarded UDP port {port} to this computer (UPnP)."));
                external = Some(ip);
                mapped_at = Some(Instant::now());
                upnp_result = Some(Ok(ip));
            }
            Err(why) => {
                say(talk, false, &format!("Automatic port forwarding (UPnP) didn't work: {why}."));
                upnp_result = Some(Err(why));
            }
        }
    }
    let Some(directory) = directory else {
        // Not announcing: at most one address lookup at the directory.
        if let Some(pin) = pin {
            for (warn, text) in private_invites(&pin, port, lan, url.as_deref(), lookup.as_deref(), upnp_result.as_ref()) {
                say(talk, warn, &text);
            }
        }
        loop {
            std::thread::sleep(LEASE / 2);
            if mapped_at.is_some() { let _ = map_port(port, &mapping); }
        }
    };
    // The first announce waits for the match to be up (as before).
    std::thread::sleep(Duration::from_secs(if talk { 2 } else { 5 }));
    let detect = url.is_none();
    let mut last: Option<Outcome> = None;
    let mut invited = false;
    loop {
        if url.is_none() {
            match peakrunner_discovery::http::whoami(&whoami_url(&directory), directory.starts_with("https://"), Duration::from_secs(6)) {
                Ok(ip) => {
                    if let Some(ext) = external.filter(|e| *e != ip) {
                        say(talk, true, &format!("Your router says its address is {ext}, but the directory sees {ip}: there may be a second router \
                            or carrier-grade NAT between you and the internet, which port forwarding can't get through."));
                    }
                    if ip.is_ipv6() {
                        say(talk, true, &format!("This computer reached the directory only over IPv6 ({ip}). The server listens on IPv4 \
                            unless PEAKRUNNER_UDP_BIND=[::]:{port} is set, and home routers usually block incoming IPv6 until you allow UDP {port}."));
                    }
                    url = Some(quic_url(&ip.to_string(), port));
                }
                Err(e) => {
                    if last != Some(Outcome::Failed(String::new())) {
                        say(talk, true, &format!("Couldn't ask the directory for this computer's public address ({e}); retrying. \
                            Set public_address in server.json to skip this."));
                    }
                    last = Some(Outcome::Failed(String::new()));
                    std::thread::sleep(ANNOUNCE_EVERY);
                    continue;
                }
            }
        }
        let me = url.clone().unwrap();
        if !invited {
            invited = true;
            if let Some(pin) = pin {
                let host = me.trim_start_matches("quic://");
                say(talk, false, &format!("Invite link for players on the internet:\n  {}", invite_link(host, &pin)));
            }
        }
        let outcome = announce(&directory, &me);
        let changed = match (&last, &outcome) {
            (Some(Outcome::Listed(_)), Outcome::Refreshed) => false,
            (Some(a), b) => std::mem::discriminant(a) != std::mem::discriminant(b),
            (None, _) => true,
        };
        match &outcome {
            Outcome::Listed(why) => say(talk, false, &format!("Public server list: {why}.\nThe directory reached this server over the internet, \
                so players can find it under Find match.")),
            Outcome::Refreshed => log::debug!("announced {me}"),
            Outcome::Unreachable(why) if changed => say(talk, true, &format!("The directory couldn't reach this server at {me}: {why}.\n{}",
                forwarding_help(port, lan))),
            Outcome::WrongAddress(why) => {
                if changed { say(talk, true, &format!("The directory didn't accept {me}: {why}.")); }
                // Our address may have changed: ask again next time.
                if detect { url = None; invited = false; }
            }
            Outcome::Refused(code, why) if changed => say(talk, true, &format!("The directory refused the announce (HTTP {code}){}{why}",
                if why.is_empty() { "" } else { ": " })),
            Outcome::Failed(why) if changed => say(talk, true, &format!("Couldn't reach the directory ({why}); retrying.")),
            _ => {}
        }
        if !matches!(outcome, Outcome::Refreshed) || last.is_none() { last = Some(outcome); }
        if mapped_at.is_some_and(|t: Instant| t.elapsed() >= LEASE / 2) && map_port(port, &mapping).is_ok() {
            mapped_at = Some(Instant::now());
        }
        std::thread::sleep(ANNOUNCE_EVERY);
    }
}

/// A private server's invite messages (warning?, text), in order: the
/// public link and what it needs, then the LAN link. The public address
/// is `known` (`public_address`), else one `GET /whoami` at `lookup` (only
/// with `--publicip`, announced first), else the router's (UPnP). If none
/// works, the LAN link is the fallback; without `--publicip` a hint says
/// how to get the public link.
pub fn private_invites(pin: &Fingerprint, port: u16, lan: Option<IpAddr>, known: Option<&str>, lookup: Option<&str>,
                       upnp: Option<&Result<IpAddr, String>>) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let found: Result<String, String> = match (known, lookup) {
        (Some(url), _) => Ok(url.trim_start_matches("quic://").to_string()),
        (None, Some(directory)) => {
            let at = whoami_url(directory);
            out.push((false, format!("Private server: asking {at} once for this computer's public address, for the invite link \
                (set public_address in server.json to skip this).")));
            peakrunner_discovery::http::whoami(&at, directory.starts_with("https://"), LOOKUP_TIMEOUT)
                .map(|ip| host_port(ip, port)).map_err(|e| e.to_string())
        }
        (None, None) => Err(String::new()),
    };
    let found = found.or_else(|why| match upnp {
        Some(Ok(ip)) if public(*ip) => Ok(host_port(*ip, port)),
        _ => Err(why),
    });
    let lan_link = lan.map(|ip| invite_link(&host_port(ip, port), pin));
    match found {
        Ok(host) => {
            let forwarding = match upnp {
                Some(Ok(_)) => "UPnP has set this up on your router",
                Some(Err(_)) => "UPnP didn't work, so forward it on your router",
                None => "UPnP is off, so forward it on your router",
            };
            out.push((false, format!("Invite link for players on the internet:\n  {}\n  It works once UDP port {port} reaches this computer \
                ({forwarding}).", invite_link(&host, pin))));
        }
        // Without --publicip: nothing was asked.
        Err(why) if why.is_empty() => {}
        Err(why) => out.push((true, format!("Couldn't learn this computer's public address ({why}). Share the local invite link below; \
            players outside your network can use it with your public IP address in place of the local one, once UDP port {port} \
            is forwarded to this computer."))),
    }
    if let Some(link) = lan_link { out.push((false, format!("Invite link for players on your network:\n  {link}"))); }
    if known.is_none() && lookup.is_none() {
        out.push((false, "Run with --publicip to also show your public invite link (asks the server list for your address once).".into()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helpers_derive_addresses_and_explain_forwarding() {
        assert_eq!(whoami_url("https://dir.peakrunner.net/announce"), "https://dir.peakrunner.net/whoami");
        assert_eq!(whoami_url("http://127.0.0.1:9000/announce"), "http://127.0.0.1:9000/whoami");
        assert_eq!(whoami_url("https://dir.example"), "https://dir.example/whoami");
        assert_eq!(host_port("2001:db8::5".parse().unwrap(), 7777), "[2001:db8::5]:7777");
        let help = forwarding_help(27015, Some("192.168.1.23".parse().unwrap()));
        assert!(help.contains("Forward UDP port 27015 to this computer (192.168.1.23), port 27015"), "{help}");
        // Refused outright: nothing listens there (loopback, a closed port).
        assert!(matches!(announce("http://127.0.0.1:9/announce", "quic://203.0.113.5:7777"), Outcome::Failed(_)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn publicip_asks_the_directory_once_and_falls_back_to_the_lan_link() {
        let pin = [7u8; 32];
        let lan: Option<IpAddr> = Some("192.168.1.23".parse().unwrap());
        let lan_link = invite_link("192.168.1.23:27015", &pin);
        // A directory on loopback answers /whoami with 127.0.0.1.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = listener.local_addr().unwrap();
        let directory = tokio::spawn(peakrunner_directory::serve_loopback_for_tests(listener, "quic://127.0.0.1:9".into(), "quic://127.0.0.1:9".into()));
        let texts = |m: Vec<(bool, String)>| m.into_iter().map(|(warn, t)| format!("{warn} {t}")).collect::<Vec<_>>();
        let found = tokio::task::spawn_blocking(move || private_invites(&pin, 27015, lan, None, Some(&format!("http://{at}/announce")), None)).await.unwrap();
        let found = texts(found);
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found[0].starts_with("false Private server: asking http://") && found[0].contains("/whoami once"), "{found:?}");
        assert!(found[1].contains(&invite_link("127.0.0.1:27015", &pin)) && found[1].contains("UPnP is off"), "{found:?}");
        assert!(found[2].contains(&lan_link));
        directory.abort();
        // Nothing answers: a warning, and the LAN link still.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let failed = texts(tokio::task::spawn_blocking(move ||
            private_invites(&pin, 27015, lan, None, Some(&format!("http://{closed}/announce")), Some(&Err("no router".into())))).await.unwrap());
        assert!(failed[1].starts_with("true Couldn't learn this computer's public address"), "{failed:?}");
        assert!(failed[2].contains(&lan_link));
        // A given address needs no lookup; a public UPnP address is the next best thing.
        let known = texts(private_invites(&pin, 27015, lan, Some("quic://198.51.100.4:27015"), None, Some(&Ok("198.51.100.4".parse().unwrap()))));
        assert_eq!(known.len(), 2);
        assert!(known[0].contains(&invite_link("198.51.100.4:27015", &pin)) && known[0].contains("UPnP has set this up"), "{known:?}");
        let router = texts(private_invites(&pin, 27015, None, None, None, Some(&Ok("198.51.100.9".parse().unwrap()))));
        assert!(router[0].contains(&invite_link("198.51.100.9:27015", &pin)), "{router:?}");
    }

    #[test]
    fn without_publicip_nothing_is_asked_and_a_hint_says_how() {
        let pin = [7u8; 32];
        let lan: Option<IpAddr> = Some("192.168.1.23".parse().unwrap());
        // No lookup directory is even known here, so no request can be made;
        // the output is the LAN link and the hint, no warning.
        let out = private_invites(&pin, 27015, lan, None, None, None);
        assert_eq!(out, vec![
            (false, format!("Invite link for players on your network:\n  {}", invite_link("192.168.1.23:27015", &pin))),
            (false, "Run with --publicip to also show your public invite link (asks the server list for your address once).".to_string()),
        ]);
        // A carrier-grade NAT address from the router is no use to players.
        let cgnat = private_invites(&pin, 27015, lan, None, None, Some(&Ok("100.64.3.4".parse().unwrap())));
        assert!(!cgnat.iter().any(|(_, t)| t.contains("100.64.3.4")), "{cgnat:?}");
    }
}
