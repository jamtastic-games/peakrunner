use std::{io::{self, Read}, time::Duration};
use crate::{PublicDirectory, ServerAdvert, PROTOCOL, wire::invalid};
pub fn http_get(url: &str, https_only: bool) -> io::Result<Vec<u8>> {
    let config = ureq::Agent::config_builder().https_only(https_only)
        .max_redirects(0).timeout_global(Some(Duration::from_secs(4))).build();
    let mut response = ureq::Agent::new_with_config(config).get(url).call().map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    response.body_mut().as_reader().take(262_145).read_to_end(&mut bytes)?;
    if bytes.len() > 262_144 { return Err(invalid("directory response too large")); }
    Ok(bytes)
}
/// POST a small JSON body; returns the HTTP status (errors only for
/// transport failures). Used by hosted servers to announce themselves.
pub fn http_post_json<T: serde::Serialize>(url: &str, body: &T, https_only: bool) -> io::Result<u16> {
    let config = ureq::Agent::config_builder().https_only(https_only).http_status_as_error(false)
        .max_redirects(0).timeout_global(Some(Duration::from_secs(12))).build();
    let json = serde_json::to_string(body).map_err(io::Error::other)?;
    let response = ureq::Agent::new_with_config(config).post(url)
        .header("content-type", "application/json").send(json).map_err(io::Error::other)?;
    Ok(response.status().as_u16())
}
/// POST a small JSON body; the HTTP status and up to 4 KiB of the reply body.
pub fn http_post_json_reply<T: serde::Serialize>(url: &str, body: &T, https_only: bool) -> io::Result<(u16, Vec<u8>)> {
    let config = ureq::Agent::config_builder().https_only(https_only).http_status_as_error(false)
        .max_redirects(0).timeout_global(Some(Duration::from_secs(12))).build();
    let json = serde_json::to_string(body).map_err(io::Error::other)?;
    let mut response = ureq::Agent::new_with_config(config).post(url)
        .header("content-type", "application/json").send(json).map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    response.body_mut().as_reader().take(4096).read_to_end(&mut bytes)?;
    Ok((response.status().as_u16(), bytes))
}
/// The public address the directory sees this host at (`GET /whoami`),
/// over IPv4 when possible: a home server's forwarded port is IPv4. Each
/// attempt (IPv4, then any family) is bounded by `timeout`.
pub fn whoami(url: &str, https_only: bool, timeout: Duration) -> io::Result<std::net::IpAddr> {
    let get = |family| -> io::Result<std::net::IpAddr> {
        let config = ureq::Agent::config_builder().https_only(https_only).ip_family(family)
            .max_redirects(0).timeout_global(Some(timeout)).build();
        let mut response = ureq::Agent::new_with_config(config).get(url).call().map_err(io::Error::other)?;
        let mut bytes = Vec::new();
        response.body_mut().as_reader().take(1024).read_to_end(&mut bytes)?;
        let me: crate::WhoAmI = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        me.address.parse().map_err(|_| invalid("directory sent an invalid address"))
    };
    get(ureq::config::IpFamily::Ipv4Only).or_else(|_| get(ureq::config::IpFamily::Any))
}
pub fn browse_https(url: &str) -> io::Result<Vec<ServerAdvert>> {
    parse_listing(&http_get(url, true)?)
}
/// A directory listing's joinable servers: CA-certified ones, then the
/// vouched ones with their fingerprints.
/// Most servers a listing may hold in each of `servers` and `vouched` (the
/// directory lists up to 256 announced in all; games before
/// `.20261015` refuse a listing of more than 128).
pub const MAX_LISTING: usize = 512;

pub fn parse_listing(bytes: &[u8]) -> io::Result<Vec<ServerAdvert>> {
    let data: PublicDirectory = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    if data.protocol != PROTOCOL || data.servers.len() > MAX_LISTING || data.vouched.len() > MAX_LISTING { return Err(invalid("incompatible directory")); }
    // One bad entry is skipped, never allowed to hide every other server.
    Ok(data.servers.into_iter().filter(|s| s.sha256.is_none())
        .chain(data.vouched.into_iter().filter(|s| s.sha256.is_some()))
        .filter(advert_ok).collect())
}

/// Whether a listed server is safe to show and join.
pub fn advert_ok(s: &ServerAdvert) -> bool {
    s.host.starts_with("quic://") && crate::quic::endpoint_url(&s.host).is_ok()
        && s.sha256.as_deref().is_none_or(|fp| fp.len() == 64 && fp.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        && status_fields_ok(&s.name, &s.map, s.players, s.max_players) && crate::valid_location(&s.location)
}

/// Shared bounds for a server's advertised name, map and counts.
pub fn status_fields_ok(name: &str, map: &str, players: u32, max_players: u32) -> bool {
    !name.trim().is_empty() && name.len() <= 64 && !name.chars().any(char::is_control)
        && crate::valid_map_label(map.split(" - ").next().unwrap_or(map)) && map.len() <= 64
        && players <= max_players && (2..=crate::MAX_LISTED_PLAYERS).contains(&max_players)
}
