//! TLS-verified discovery/control transport, independent of gameplay packets.
//!
//! A client authenticates a server one of two ways (`Trust`):
//! - `WebPki`: a public CA certificate for the URL's host name (the official
//!   servers, and anyone with a domain and certificate).
//! - `Pinned`: exactly one end-entity certificate, named by the SHA-256 of
//!   its DER bytes. The fingerprint comes from somewhere the player already
//!   trusts: the directory's HTTPS listing (which records it only after a
//!   validated handshake at the announcer's own address) or an invite link
//!   the host shared. The handshake signature is still checked with rustls's
//!   standard TLS 1.3 verification, so only the holder of that certificate's
//!   private key can complete it. Nothing here accepts an unknown key.
use std::{io, net::SocketAddr, sync::{Arc, Mutex}, time::Duration};
use serde::Serialize;
use quinn::{Connection, Endpoint};
use rustls::{DigitallySignedStruct, SignatureScheme, client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::WebPkiSupportedAlgorithms, pki_types::{CertificateDer, ServerName, UnixTime}};
use tokio::time::timeout;
use crate::wire::invalid;
pub const ALPN: &[u8] = b"peakrunner/4";

/// SHA-256 of a certificate's DER bytes.
pub type Fingerprint = [u8; 32];
pub fn fingerprint(der: &[u8]) -> Fingerprint {
    ring::digest::digest(&ring::digest::SHA256, der).as_ref().try_into().expect("SHA-256 is 32 bytes")
}
/// Lowercase hex, as listings and invite links write it.
pub fn fingerprint_hex(fp: &Fingerprint) -> String { fp.iter().map(|b| format!("{b:02x}")).collect() }
/// Exactly 64 hex digits (either case).
pub fn parse_fingerprint(hex: &str) -> Option<Fingerprint> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() { *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?; }
    Some(out)
}

/// How a client decides it reached the right server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust { WebPki, Pinned(Fingerprint) }
/// What a server proved in a handshake: a CA certificate valid for the
/// URL's host, or possession of the key for a certificate with this
/// fingerprint. Only the directory's first probe asks; it then keeps
/// polling with the matching `Trust`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity { Ca, Key(Fingerprint) }
impl Identity {
    pub fn trust(self) -> Trust { match self { Identity::Ca => Trust::WebPki, Identity::Key(fp) => Trust::Pinned(fp) } }
}

/// Where to join and how to trust it. Accepts `quic://host:port`,
/// `quic://host:port#sha256=<hex>` and invite links
/// `peakrunner://join/host:port#sha256=<hex>`; returns the plain quic:// URL.
pub fn join_target(address: &str) -> io::Result<(String, Trust)> {
    let address = address.trim();
    let (base, fragment) = match address.split_once('#') { Some((b, f)) => (b, Some(f)), None => (address, None) };
    let base = match base.strip_prefix("peakrunner://join/") {
        Some(rest) => {
            if fragment.is_none() { return Err(invalid("an invite link needs its #sha256= fingerprint")); }
            format!("quic://{}", rest.trim_end_matches('/'))
        }
        None => base.to_string(),
    };
    endpoint_url(&base)?;
    let trust = match fragment {
        None => Trust::WebPki,
        Some(f) => Trust::Pinned(f.strip_prefix("sha256=").and_then(parse_fingerprint)
            .ok_or_else(|| invalid("the server fingerprint must be #sha256= and 64 hex digits"))?),
    };
    Ok((base, trust))
}
/// The one written form of a join target: `quic://host:port`, plus
/// `#sha256=<hex>` when pinned.
pub fn join_address(url: &str, trust: Trust) -> String {
    match trust { Trust::WebPki => url.to_string(), Trust::Pinned(fp) => format!("{url}#sha256={}", fingerprint_hex(&fp)) }
}
/// An invite link for a pinned server.
pub fn invite_link(host_port: &str, fp: &Fingerprint) -> String {
    format!("peakrunner://join/{host_port}#sha256={}", fingerprint_hex(fp))
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> { Arc::new(rustls::crypto::ring::default_provider()) }

/// Accepts exactly one end-entity certificate, then verifies the handshake
/// signature against it with rustls's standard TLS 1.3 check. Host names,
/// chains and validity dates don't apply: the pinned certificate is the
/// server's whole identity.
#[derive(Debug)]
pub struct PinnedCert { pin: Fingerprint, algorithms: WebPkiSupportedAlgorithms }
impl PinnedCert {
    pub fn new(pin: Fingerprint) -> Self { Self { pin, algorithms: provider().signature_verification_algorithms } }
}
impl ServerCertVerifier for PinnedCert {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _intermediates: &[CertificateDer<'_>], _name: &ServerName<'_>,
                          _ocsp: &[u8], _now: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity) == self.pin { Ok(ServerCertVerified::assertion()) }
        else { Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure)) }
    }
    fn verify_tls12_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &DigitallySignedStruct)
                              -> Result<HandshakeSignatureValid, rustls::Error> {
        // The configs here offer TLS 1.3 only.
        Err(rustls::Error::PeerIncompatible(rustls::PeerIncompatible::Tls13RequiredForQuic))
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct)
                              -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { self.algorithms.supported_schemes() }
}

/// The directory's first probe of an announced server: a CA certificate
/// valid for the URL's host counts as `Identity::Ca`; any other certificate
/// is recorded by fingerprint (`Identity::Key`). Either way the TLS 1.3
/// handshake signature must verify against the presented certificate, so
/// the fingerprint names a key the server really holds. The directory only
/// runs this against the announcer's own address.
#[derive(Debug)]
struct Identify { webpki: Arc<rustls::client::WebPkiServerVerifier>, algorithms: WebPkiSupportedAlgorithms, seen: Arc<Mutex<Option<Identity>>> }
impl ServerCertVerifier for Identify {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], name: &ServerName<'_>,
                          ocsp: &[u8], now: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        let identity = match self.webpki.verify_server_cert(end_entity, intermediates, name, ocsp, now) {
            Ok(_) => Identity::Ca,
            Err(_) => Identity::Key(fingerprint(end_entity)),
        };
        *self.seen.lock().unwrap() = Some(identity);
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &DigitallySignedStruct)
                              -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::PeerIncompatible(rustls::PeerIncompatible::Tls13RequiredForQuic))
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct)
                              -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { self.algorithms.supported_schemes() }
}
pub fn endpoint_url(address: &str) -> io::Result<url::Url> {
    let url = url::Url::parse(address).map_err(|_| invalid("invalid QUIC URL"))?;
    if url.scheme() != "quic" || url.host_str().is_none() || !url.username().is_empty()
        || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
        || !["", "/"].contains(&url.path()) { return Err(invalid("use quic://hostname:7777")); }
    Ok(url)
}
pub fn transport() -> quinn::TransportConfig {
    let mut cfg = quinn::TransportConfig::default();
    cfg.max_concurrent_bidi_streams(1u32.into()).max_concurrent_uni_streams(0u32.into())
        .stream_receive_window(8192u32.into()).receive_window(16384u32.into())
        .datagram_receive_buffer_size(Some(256 * 1024)).datagram_send_buffer_size(16 * 1024)
        .max_idle_timeout(Some(Duration::from_secs(10).try_into().unwrap()))
        .keep_alive_interval(Some(Duration::from_secs(1)));
    cfg
}
fn roots(extra_root: Option<CertificateDer<'static>>) -> io::Result<rustls::RootCertStore> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(root) = extra_root { roots.add(root).map_err(io::Error::other)?; }
    Ok(roots)
}
pub fn client_config(extra_root: Option<CertificateDer<'static>>) -> io::Result<quinn::ClientConfig> {
    let tls = rustls::ClientConfig::builder_with_provider(provider()).with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?.with_root_certificates(roots(extra_root)?).with_no_client_auth();
    quic_client(tls)
}
/// A client that accepts only the certificate with this fingerprint (see `PinnedCert`).
pub fn pinned_client_config(pin: Fingerprint) -> io::Result<quinn::ClientConfig> {
    // rustls files every custom verifier under `dangerous()`; this one is
    // strict: one exact certificate plus the standard signature check.
    let tls = rustls::ClientConfig::builder_with_provider(provider()).with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?.dangerous().with_custom_certificate_verifier(Arc::new(PinnedCert::new(pin))).with_no_client_auth();
    quic_client(tls)
}
pub fn trust_client_config(trust: Trust) -> io::Result<quinn::ClientConfig> {
    match trust { Trust::WebPki => client_config(None), Trust::Pinned(fp) => pinned_client_config(fp) }
}
fn identify_client_config() -> io::Result<(quinn::ClientConfig, Arc<Mutex<Option<Identity>>>)> {
    let provider = provider();
    let webpki = rustls::client::WebPkiServerVerifier::builder_with_provider(Arc::new(roots(None)?), provider.clone())
        .build().map_err(io::Error::other)?;
    let seen = Arc::new(Mutex::new(None));
    let verifier = Identify { webpki, algorithms: provider.signature_verification_algorithms, seen: seen.clone() };
    let tls = rustls::ClientConfig::builder_with_provider(provider).with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?.dangerous().with_custom_certificate_verifier(Arc::new(verifier)).with_no_client_auth();
    Ok((quic_client(tls)?, seen))
}
fn quic_client(tls: rustls::ClientConfig) -> io::Result<quinn::ClientConfig> {
    // Inputs are tiny and replaceable. A snapshot-sized transmit queue can hold
    // seconds of obsolete controls when congestion control reduces the rate.
    let mut input_transport = transport();
    input_transport.datagram_send_buffer_size(1024);
    quic_client_with(tls, input_transport)
}
fn quic_client_with(mut tls: rustls::ClientConfig, transport: quinn::TransportConfig) -> io::Result<quinn::ClientConfig> {
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls).map_err(io::Error::other)?;
    let mut cfg = quinn::ClientConfig::new(Arc::new(crypto)); cfg.transport_config(Arc::new(transport)); Ok(cfg)
}
/// A download connection's transport: the game's, with receive windows
/// large enough that files arrive at link speed (the game's 8/16 KiB windows
/// cap a download near 160 KB/s on a 50 ms path). Only the receiver's windows
/// matter, so the server's settings are unchanged.
pub fn download_transport() -> quinn::TransportConfig {
    let mut cfg = transport();
    cfg.stream_receive_window((4u32 << 20).into()).receive_window((8u32 << 20).into())
        .max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    cfg
}
/// Connect to download a server's content, trusting it exactly as joining
/// would (CA certificate, or the pinned key of the address or invite link).
pub async fn connect_for_download(address: &str) -> io::Result<(Endpoint, Connection)> {
    let (url, trust) = join_target(address)?;
    let tls = match trust {
        Trust::WebPki => rustls::ClientConfig::builder_with_provider(provider()).with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(io::Error::other)?.with_root_certificates(roots(None)?).with_no_client_auth(),
        Trust::Pinned(pin) => rustls::ClientConfig::builder_with_provider(provider()).with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(io::Error::other)?.dangerous().with_custom_certificate_verifier(Arc::new(PinnedCert::new(pin))).with_no_client_auth(),
    };
    connect_with(&url, None, quic_client_with(tls, download_transport())?, trust).await
}
/// Largest control message: joins, chat, votes, welcome and rejections.
pub const CONTROL_LIMIT: usize = 4096;
/// Largest message from a game server: a duel board (`ServerMsg::Duels`)
/// lists everyone on the server and their duels, past CONTROL_LIMIT once a
/// server fills up.
pub const SERVER_CONTROL_LIMIT: usize = 32 * 1024;
/// Largest status reply. A full 28-player roster with long names comes to
/// about 3.7 KB; this leaves room for more without being unbounded. Only
/// the server's status reply and the directory's status read use it.
pub const STATUS_LIMIT: usize = 16 * 1024;
pub async fn send_control<T: Serialize>(stream: &mut quinn::SendStream, data: &T) -> io::Result<()> {
    send_control_limited(stream, data, CONTROL_LIMIT).await
}
pub async fn send_control_limited<T: Serialize>(stream: &mut quinn::SendStream, data: &T, limit: usize) -> io::Result<()> {
    let bytes = serde_json::to_vec(data).map_err(io::Error::other)?;
    if bytes.len() > limit { return Err(invalid("control message too large")); }
    stream.write_all(&(bytes.len() as u32).to_le_bytes()).await.map_err(io::Error::other)?;
    stream.write_all(&bytes).await.map_err(io::Error::other)
}
pub async fn read_control<T: serde::de::DeserializeOwned>(stream: &mut quinn::RecvStream) -> io::Result<T> {
    read_control_limited(stream, CONTROL_LIMIT).await
}
pub async fn read_control_limited<T: serde::de::DeserializeOwned>(stream: &mut quinn::RecvStream, limit: usize) -> io::Result<T> {
    let mut len = [0u8; 4]; stream.read_exact(&mut len).await.map_err(io::Error::other)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > limit { return Err(invalid("control message too large")); }
    let mut bytes = vec![0; len]; stream.read_exact(&mut bytes).await.map_err(io::Error::other)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}
pub async fn connect_endpoint(address: &str) -> io::Result<(Endpoint, Connection)> {
    connect_endpoint_at(address, None).await
}

/// Connect to `address`, at `ip` when given instead of resolving its host.
/// A CA certificate is verified against the URL's hostname; an address
/// with `#sha256=` (or an invite link) accepts only that certificate.
pub async fn connect_endpoint_at(address: &str, ip: Option<std::net::IpAddr>) -> io::Result<(Endpoint, Connection)> {
    let (url, trust) = join_target(address)?;
    connect_with(&url, ip, trust_client_config(trust)?, trust).await
}

async fn connect_with(address: &str, ip: Option<std::net::IpAddr>, config: quinn::ClientConfig, trust: Trust) -> io::Result<(Endpoint, Connection)> {
    let url = endpoint_url(address)?;
    let host = url.host_str().unwrap().trim_matches(['[', ']']);
    // For pre-DNS deployment checks only. Hostname/certificate verification is
    // unchanged, even when a diagnostic explicitly selects the destination IP.
    let addr = if let Some(ip) = ip {
        SocketAddr::new(ip, url.port().unwrap_or(7777))
    } else if let Ok(ip) = std::env::var("PEAKRUNNER_QUIC_TEST_IP") {
        SocketAddr::new(ip.parse().map_err(|_| invalid("invalid diagnostic IP"))?, url.port().unwrap_or(7777))
    } else { tokio::net::lookup_host((host, url.port().unwrap_or(7777))).await?
        .next().ok_or_else(|| invalid("server address missing"))? };
    let mut endpoint = Endpoint::client(if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().unwrap())?;
    endpoint.set_default_client_config(config);
    let connecting = endpoint.connect(addr, host).map_err(io::Error::other)?;
    let conn = timeout(Duration::from_secs(6), connecting).await.map_err(|_| invalid("UDP connection timed out; check network/firewall"))?
        .map_err(|_| invalid(match trust {
            Trust::WebPki => "encrypted UDP connection failed; check server availability and certificate",
            Trust::Pinned(_) => "encrypted UDP connection failed; the server is down or its key doesn't match the fingerprint",
        }))?;
    Ok((endpoint, conn))
}
pub async fn query_status(address: &str) -> io::Result<crate::MatchStatus> {
    query_status_at(address, None).await
}

/// `query_status` and the connection's measured round trip (the server
/// list's ping).
pub async fn query_status_timed(address: &str) -> io::Result<(crate::MatchStatus, Duration)> {
    let (endpoint, conn) = connect_endpoint_at(address, None).await?;
    let mut rtt = Duration::ZERO;
    let status = status_timed(endpoint, conn, &mut rtt).await?;
    Ok((status, rtt))
}

/// `query_status` against a known IP (the directory probes only the address
/// it verified, so a DNS change after listing can't redirect it).
pub async fn query_status_at(address: &str, ip: Option<std::net::IpAddr>) -> io::Result<crate::MatchStatus> {
    let (endpoint, conn) = connect_endpoint_at(address, ip).await?;
    status_over(endpoint, conn).await
}

/// `query_status_at` with an explicit trust: how the directory keeps polling
/// a server with the identity it recorded when listing it.
pub async fn query_status_trusting(address: &str, ip: Option<std::net::IpAddr>, trust: Trust) -> io::Result<crate::MatchStatus> {
    let (endpoint, conn) = connect_with(address, ip, trust_client_config(trust)?, trust).await?;
    status_over(endpoint, conn).await
}

/// The directory's first probe: the status, and what the server proved
/// about its identity (see `Identify`).
pub async fn identify_status_at(address: &str, ip: std::net::IpAddr) -> io::Result<(crate::MatchStatus, Identity)> {
    let (config, seen) = identify_client_config()?;
    let (endpoint, conn) = connect_with(address, Some(ip), config, Trust::WebPki).await
        .map_err(|_| invalid("no encrypted QUIC handshake completed (UDP port closed, not forwarded, or not a PeakRunner server)"))?;
    let identity = seen.lock().unwrap().ok_or_else(|| invalid("no certificate presented"))?;
    Ok((status_over(endpoint, conn).await?, identity))
}

async fn status_over(endpoint: Endpoint, conn: Connection) -> io::Result<crate::MatchStatus> {
    let mut rtt = Duration::ZERO;
    status_timed(endpoint, conn, &mut rtt).await
}

/// The status over `conn`, recording its round-trip estimate once the reply is in.
async fn status_timed(endpoint: Endpoint, conn: Connection, rtt: &mut Duration) -> io::Result<crate::MatchStatus> {
    let result = timeout(Duration::from_secs(3), async {
        let (mut send, mut recv) = conn.open_bi().await.map_err(io::Error::other)?;
        send_control(&mut send, &crate::StatusRequest::Status).await?;
        match read_control_limited::<crate::StatusReply>(&mut recv, STATUS_LIMIT).await? {
            crate::StatusReply::Status { status } => Ok(status),
        }
    }).await.map_err(io::Error::other)?;
    *rtt = conn.rtt();
    conn.close(0u32.into(), b"status complete"); endpoint.wait_idle().await; result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{pki_types::PrivateKeyDer, server::{ClientHello, ResolvesServerCert}, sign::{CertifiedKey, SigningKey}};

    fn keypair() -> (CertificateDer<'static>, Arc<dyn SigningKey>) {
        let c = rcgen::generate_simple_self_signed(vec!["peakrunner-server.invalid".into()]).unwrap();
        let key = rustls::crypto::ring::sign::any_supported_type(&PrivateKeyDer::Pkcs8(c.key_pair.serialize_der().into())).unwrap();
        (c.cert.der().clone(), key)
    }
    /// Presents `cert` and signs handshakes with `key`, whether or not they belong together.
    #[derive(Debug)]
    struct Fixed(Arc<CertifiedKey>);
    impl ResolvesServerCert for Fixed {
        fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> { Some(self.0.clone()) }
    }
    /// A status-only QUIC server on loopback; its URL.
    fn status_server(cert: CertificateDer<'static>, key: Arc<dyn SigningKey>) -> (Endpoint, String) {
        let mut tls = rustls::ServerConfig::builder_with_provider(provider()).with_protocol_versions(&[&rustls::version::TLS13]).unwrap()
            .with_no_client_auth().with_cert_resolver(Arc::new(Fixed(Arc::new(CertifiedKey::new(vec![cert], key)))));
        tls.alpn_protocols = vec![ALPN.to_vec()];
        let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls).unwrap();
        let endpoint = Endpoint::server(quinn::ServerConfig::with_crypto(Arc::new(crypto)), "127.0.0.1:0".parse().unwrap()).unwrap();
        let url = format!("quic://127.0.0.1:{}", endpoint.local_addr().unwrap().port());
        let server = endpoint.clone();
        tokio::spawn(async move {
            while let Some(incoming) = server.accept().await {
                tokio::spawn(async move {
                    let Ok(conn) = incoming.await else { return };
                    let Ok((mut send, mut recv)) = conn.accept_bi().await else { return };
                    if read_control::<crate::StatusRequest>(&mut recv).await.is_err() { return; }
                    let status = crate::MatchStatus { name: "Pinned".into(), map: "Old Holler - CTF".into(), max_players: 8, ..Default::default() };
                    let _ = send_control(&mut send, &crate::StatusReply::Status { status }).await;
                    let _ = send.finish();
                    let _ = send.stopped().await;
                });
            }
        });
        (endpoint, url)
    }

    #[test]
    fn join_targets_accept_urls_pins_and_invite_links() {
        let fp = [0xabu8; 32];
        let hex = fingerprint_hex(&fp);
        assert_eq!(join_target("quic://play.peakrunner.net:7777").unwrap(), ("quic://play.peakrunner.net:7777".into(), Trust::WebPki));
        assert_eq!(join_target(&format!(" quic://203.0.113.5:7777#sha256={hex} ")).unwrap(), ("quic://203.0.113.5:7777".into(), Trust::Pinned(fp)));
        assert_eq!(join_target(&format!("peakrunner://join/203.0.113.5:7777#sha256={}", hex.to_uppercase())).unwrap(),
            ("quic://203.0.113.5:7777".into(), Trust::Pinned(fp)));
        assert_eq!(join_target(&invite_link("[2001:db8::5]:27015", &fp)).unwrap(), ("quic://[2001:db8::5]:27015".into(), Trust::Pinned(fp)));
        assert_eq!(join_address("quic://203.0.113.5:7777", Trust::Pinned(fp)), format!("quic://203.0.113.5:7777#sha256={hex}"));
        for bad in ["peakrunner://join/203.0.113.5:7777".to_string(),        // an invite without a pin: no trust on first use
            format!("quic://h:7777#sha256={}", &hex[..63]), format!("quic://h:7777#sha1={hex}"), format!("quic://h:7777#sha256={hex}00"),
            format!("quic://u:p@h:7777#sha256={hex}"), format!("https://h/#sha256={hex}"), format!("peakrunner://join/h:7777/x#sha256={hex}")] {
            assert!(join_target(&bad).is_err(), "{bad}");
        }
        assert_eq!(parse_fingerprint(&hex), Some(fp));
        assert_eq!(parse_fingerprint("zz"), None);
    }

    #[test]
    fn the_pinned_verifier_accepts_exactly_one_certificate() {
        let (a, _) = keypair();
        let (b, _) = keypair();
        let pinned = PinnedCert::new(fingerprint(&a));
        let name = ServerName::try_from("203.0.113.5").unwrap();
        assert!(pinned.verify_server_cert(&a, &[], &name, &[], UnixTime::now()).is_ok());
        assert!(pinned.verify_server_cert(&b, &[], &name, &[], UnixTime::now()).is_err(), "a different certificate");
        assert!(PinnedCert::new([0; 32]).verify_server_cert(&a, &[], &name, &[], UnixTime::now()).is_err(), "a wrong fingerprint");
        assert!(!pinned.supported_verify_schemes().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pinned_handshakes_need_the_certificate_and_its_key() {
        let (cert_a, key_a) = keypair();
        let (cert_b, key_b) = keypair();
        let (fp_a, fp_b) = (fingerprint(&cert_a), fingerprint(&cert_b));
        let (server, url) = status_server(cert_a.clone(), key_a);
        let pin = |fp: &Fingerprint| format!("{url}#sha256={}", fingerprint_hex(fp));
        // The pinned certificate, signed by its own key: joins.
        assert_eq!(query_status(&pin(&fp_a)).await.unwrap().name, "Pinned");
        let host = url.trim_start_matches("quic://");
        assert_eq!(query_status(&invite_link(host, &fp_a)).await.unwrap().name, "Pinned", "the invite-link form");
        // Anything else: refused.
        assert!(query_status(&url).await.is_err(), "no CA vouches for a self-signed certificate");
        assert!(query_status(&pin(&fp_b)).await.is_err(), "a different fingerprint");
        // The directory's first probe records the key the handshake proved.
        let (status, identity) = identify_status_at(&url, "127.0.0.1".parse().unwrap()).await.unwrap();
        assert_eq!((status.name.as_str(), identity), ("Pinned", Identity::Key(fp_a)));
        assert_eq!(query_status_trusting(&url, None, identity.trust()).await.unwrap().name, "Pinned");
        server.close(0u32.into(), b"done");

        // A server presenting certificate A but signing with key B (someone
        // who copied A without its private key): the pin matches, the
        // handshake signature doesn't, so both the client and the
        // directory's probe refuse it.
        let (impostor, url) = status_server(cert_a, key_b);
        assert!(query_status(&format!("{url}#sha256={}", fingerprint_hex(&fp_a))).await.is_err(), "mismatched handshake signature");
        assert!(identify_status_at(&url, "127.0.0.1".parse().unwrap()).await.is_err(), "the directory records no key it wasn't shown to hold");
        impostor.close(0u32.into(), b"done");
    }
}
