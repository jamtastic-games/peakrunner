use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "peakrunner-4";
/// Largest match size a listing may advertise.
/// Largest match capacity a listing may report (14 a side).
pub const MAX_LISTED_PLAYERS: u32 = 28;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerAdvert {
    pub id: String, pub name: String, pub host: String, pub port: u16,
    pub players: u32, pub max_players: u32, pub map: String,
    /// How many of `players` are bots (`MatchStatus::bots`); left out at
    /// zero, ignored by older clients.
    #[serde(default, skip_serializing_if = "no_bots")]
    pub bots: u32,
    /// Where the server says it is ("New York, US"); empty if it didn't say.
    #[serde(default)]
    pub location: String,
    /// For a server without a CA certificate: the SHA-256 (hex) of the
    /// certificate the directory saw it prove at its own address. Only in
    /// `PublicDirectory::vouched`, never in `servers` (older clients read
    /// `servers` and can only join CA-certified hosts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}
fn no_bots(bots: &u32) -> bool { *bots == 0 }
impl ServerAdvert {
    /// What to hand `connect`: the host URL, pinned when the listing has a
    /// fingerprint.
    pub fn join_address(&self) -> String {
        match &self.sha256 { Some(fp) => format!("{}#sha256={fp}", self.host), None => self.host.clone() }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum DirRequest {
    Register { name: String, host: String, port: u16, players: u32, max_players: u32, map: String },
    Heartbeat { id: String, token: String, players: u32 },
    List,
    Unregister { id: String, token: String },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DirResponse {
    Registered { id: String, token: String }, Servers { servers: Vec<ServerAdvert> },
    Ok, Error { message: String },
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MatchStatus {
    pub name: String, pub map: String,
    /// Everyone playing, bots included (at most `max_players`).
    pub players: u32, pub max_players: u32,
    /// How many of `players` are bots. A person who joins takes a bot's
    /// place, so the match is full only when people fill `max_players`.
    #[serde(default)]
    pub bots: u32,
    pub tick: u64, pub round: u32, pub phase: String, pub score: [u32; 2],
    pub time_left: f32, pub password_required: bool,
    #[serde(default)]
    pub game_version: String,
    #[serde(default)]
    pub game_protocol: String,
    #[serde(default)]
    pub roster: Vec<PlayerStats>,
    /// The server's stated location (PEAKRUNNER_LOCATION).
    #[serde(default)]
    pub location: String,
    /// Content a player needs to join: every map the rotation and its votes
    /// can reach (docs/modding.md, P2). A client fetches what it lacks, after
    /// the player approves, before it says hello.
    #[serde(default)]
    pub content: Vec<ContentItem>,
}
/// One map or mod a server needs players to hold, identified by its
/// fingerprint (a map: the SHA-256 of its `map.json`; a mod: of its
/// listing); `bytes` is its download size.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ContentItem {
    /// "map" or "mod".
    #[serde(default = "map_kind")]
    pub kind: String,
    pub key: String, pub name: String, pub fingerprint: String, pub bytes: u64,
}
fn map_kind() -> String { "map".into() }
/// Most maps a server may need: a status listing them all, with a full
/// roster, still fits the 16 KiB status limit directories read.
pub const MAX_CONTENT: usize = 32;
/// Public scoreboard only: no IP addresses, private chat, positions or credentials.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlayerStats {
    pub id: u32, pub name: String, pub team: String, pub frags: u32, pub deaths: u32,
    /// A bot, not a person (written only when true).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bot: bool,
}
/// The public listing. `servers` holds only hosts with a public CA
/// certificate, exactly as releases up to 0.1.0-playtest.20260926.5 expect;
/// those clients ignore unknown fields, so they never see `vouched`, the
/// community hosts whose keys the directory checked (each with `sha256`).
/// `vouched` is left out entirely when empty.
#[derive(Serialize, Deserialize)]
pub struct PublicDirectory {
    pub protocol: String, pub servers: Vec<ServerAdvert>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vouched: Vec<ServerAdvert>,
}

/// The directory's answer to `GET /whoami`: the address this request came
/// from, which is the address an announce from here must name.
#[derive(Debug, Serialize, Deserialize)]
pub struct WhoAmI { pub address: String }

/// The body of an announce reply (none with 204). Status codes are what
/// older servers read; this explains them. `result` is one of `listed`,
/// `unreachable`, `wrong_address`, `rate_limited`, `busy`, `invalid`.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnnounceReply {
    pub result: String, pub message: String,
    /// The fingerprint listed, for a server listed by its key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// A hosted server telling the public directory where it is (`POST
/// /announce`). Only the address: the directory learns everything else by
/// querying the server's status there, and lists it only after that works.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Announce { pub url: String }

/// The one spelling of an announced address: `quic://host:port`, host
/// lowercased, IPv6 in brackets, the default port written out. None for
/// anything that isn't a plain quic://host[:port] URL.
pub fn canonical_url(url: &str) -> Option<String> {
    let u = crate::quic::endpoint_url(url.trim()).ok()?;
    let host = u.host()?;
    let port = u.port().unwrap_or(7777);
    Some(match host {
        url::Host::Ipv6(ip) => format!("quic://[{ip}]:{port}"),
        url::Host::Ipv4(ip) => format!("quic://{ip}:{port}"),
        url::Host::Domain(d) => format!("quic://{}:{port}", d.to_ascii_lowercase()),
    })
}

/// Stable directory ID for an announced address: `play.peakrunner.net:7778`
/// becomes `play.peakrunner.net-7778`, `[2001:db8::1]:7777` becomes
/// `ip6_2001_db8__1-7777`. Distinct addresses always get distinct IDs: domain
/// hosts may use only letters, digits, dots and hyphens (no `_`), and the port
/// is always the last hyphen-separated part.
pub fn announce_id(url: &str) -> Option<String> {
    let u = crate::quic::endpoint_url(url.trim()).ok()?;
    let port = u.port().unwrap_or(7777);
    let host = match u.host()? {
        url::Host::Ipv6(ip) => format!("ip6_{}", ip.to_string().replace(':', "_")),
        url::Host::Ipv4(ip) => ip.to_string(),
        url::Host::Domain(d) => {
            let d = d.to_ascii_lowercase();
            if d.is_empty() || d.len() > 200 || !d.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') { return None; }
            d
        }
    };
    Some(format!("{host}-{port}"))
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum StatusRequest { Status }
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum StatusReply { Status { status: MatchStatus } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn announce_ids_are_stable_and_reject_other_schemes() {
        assert_eq!(announce_id("quic://play.peakrunner.net:7778").as_deref(), Some("play.peakrunner.net-7778"));
        assert_eq!(announce_id("quic://Play.PeakRunner.net").as_deref(), Some("play.peakrunner.net-7777"));
        // Once distinct addresses that used to share an ID.
        assert_ne!(announce_id("quic://a.b:7777"), announce_id("quic://a-b:7777"));
        assert_eq!(announce_id("quic://[2001:db8::1]:7780").as_deref(), Some("ip6_2001_db8__1-7780"));
        assert_eq!(canonical_url("quic://PLAY.peakrunner.net").as_deref(), Some("quic://play.peakrunner.net:7777"));
        assert_eq!(canonical_url("quic://[2001:DB8::1]:7778/").as_deref(), Some("quic://[2001:db8::1]:7778"));
        assert!(announce_id("https://play.peakrunner.net:7778").is_none());
        assert!(serde_json::from_str::<Announce>(r#"{"url":"quic://a:1","name":"x"}"#).is_err());
    }

    /// The listing types exactly as released clients (up to
    /// 0.1.0-playtest.20260926.5, commit 2781384) define them.
    mod released {
        #[derive(serde::Deserialize)]
        pub struct ServerAdvert {
            pub id: String, pub name: String, pub host: String, pub port: u16,
            pub players: u32, pub max_players: u32, pub map: String,
            #[serde(default)]
            pub location: String,
        }
        #[derive(serde::Deserialize)]
        pub struct PublicDirectory { pub protocol: String, pub servers: Vec<ServerAdvert> }
    }

    fn advert(id: &str, host: &str, sha256: Option<&str>) -> ServerAdvert {
        ServerAdvert { id: id.into(), name: format!("{id} server"), host: host.into(), port: 7777, players: 1, max_players: 8,
            bots: 0, map: "Old Holler - CTF".into(), location: String::new(), sha256: sha256.map(str::to_string) }
    }

    #[test]
    fn released_clients_see_exactly_the_ca_certified_servers() {
        // Without vouched servers the listing is byte-for-byte the old format.
        let mut ca = PublicDirectory { protocol: PROTOCOL.into(), servers: vec![advert("a", "quic://play.peakrunner.net:7777", None)], vouched: vec![] };
        assert_eq!(serde_json::to_string(&ca).unwrap(), r#"{"protocol":"peakrunner-4","servers":[{"id":"a","name":"a server","host":"quic://play.peakrunner.net:7777","port":7777,"players":1,"max_players":8,"map":"Old Holler - CTF","location":""}]}"#);
        // A bot count is extra: old clients read past it.
        ca.servers[0].bots = 1;
        // With vouched servers, an old client parses the listing and sees
        // only the CA-certified ones it can join.
        let pin = "ab".repeat(32);
        let both = PublicDirectory { vouched: vec![advert("198.51.100.7-7777", "quic://198.51.100.7:7777", Some(&pin))], ..ca };
        let json = serde_json::to_vec(&both).unwrap();
        let old: released::PublicDirectory = serde_json::from_slice(&json).unwrap();
        assert_eq!(old.protocol, PROTOCOL);
        assert_eq!(old.servers.iter().map(|s| (s.id.as_str(), s.host.as_str(), s.port, s.players, s.max_players, s.name.as_str(),
            s.map.as_str(), s.location.as_str())).collect::<Vec<_>>(),
            vec![("a", "quic://play.peakrunner.net:7777", 7777, 1, 8, "a server", "Old Holler - CTF", "")]);
        // A new client gets both, the vouched one pinned.
        let listed = crate::http::parse_listing(&json).unwrap();
        assert_eq!(listed[0].bots, 1);
        assert_eq!(listed.iter().map(|s| s.join_address()).collect::<Vec<_>>(),
            vec!["quic://play.peakrunner.net:7777".to_string(), format!("quic://198.51.100.7:7777#sha256={pin}")]);
        // Malformed or misplaced fingerprints are skipped, never guessed at.
        let odd = PublicDirectory { protocol: PROTOCOL.into(),
            servers: vec![advert("x", "quic://x.example:7777", Some(&pin))],
            vouched: vec![advert("y", "quic://198.51.100.8:7777", None), advert("z", "quic://198.51.100.9:7777", Some("AB")),
                advert("u", "quic://198.51.100.9:7778", Some(&pin.to_uppercase()))] };
        assert!(crate::http::parse_listing(&serde_json::to_vec(&odd).unwrap()).unwrap().is_empty());
    }

    #[test]
    fn discovery_cannot_decode_or_issue_gameplay_commands() {
        assert_eq!(serde_json::to_string(&StatusRequest::Status).unwrap(), r#"{"op":"status"}"#);
        for payload in [r#"{"op":"input","command":{}}"#, r#"{"op":"hello","name":"x"}"#] {
            assert!(serde_json::from_str::<StatusRequest>(payload).is_err());
        }
        assert!(serde_json::from_str::<StatusReply>(r#"{"op":"snapshot","state":{}}"#).is_err());
    }
}
