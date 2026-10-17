//! TLS-verified public WebSockets; plaintext is allowed only on private networks.
use std::{io, net::{TcpStream, ToSocketAddrs}, time::Duration};
use serde::{de::DeserializeOwned, Serialize};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream, protocol::WebSocketConfig};
use crate::wire::{Framed, invalid, private_bind};

pub enum Transport { Private(Framed), Public(Box<WebSocket<MaybeTlsStream<TcpStream>>>) }

pub fn endpoint(address: &str) -> io::Result<url::Url> {
    let url = url::Url::parse(address).map_err(|_| invalid("use a wss:// match URL"))?;
    let local_test = url.scheme() == "ws" && url.host_str().is_some_and(|h|
        h.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback()));
    if (url.scheme() != "wss" && !local_test) || !url.username().is_empty()
        || url.password().is_some() || url.fragment().is_some() || url.host_str().is_none() {
        return Err(invalid("public matches require wss:// with a verified certificate; ws:// is loopback-only"));
    }
    Ok(url)
}

fn ws_error(error: tungstenite::Error) -> io::Error {
    match error {
        tungstenite::Error::Io(e) => e,
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => io::ErrorKind::UnexpectedEof.into(),
        other => io::Error::other(other.to_string()),
    }
}
fn pending(result: Result<(), tungstenite::Error>) -> io::Result<()> {
    match result { Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => Ok(()), other => other.map_err(ws_error) }
}
impl Transport {
    pub fn connect(address: &str) -> io::Result<Self> {
        if !address.contains("://") {
            let socket = TcpStream::connect_timeout(&private_bind(address)?, Duration::from_secs(3))?;
            return Ok(Self::Private(Framed::new(socket, 262_144)?));
        }
        let url = endpoint(address)?;
        let host = url.host_str().unwrap().trim_matches(['[', ']']);
        let port = url.port_or_known_default().ok_or_else(|| invalid("invalid port"))?;
        let mut connected = None;
        for addr in (host, port).to_socket_addrs()?.take(4) {
            if let Ok(stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(3)) { connected = Some(stream); break; }
        }
        let stream = connected.ok_or_else(|| io::Error::new(io::ErrorKind::ConnectionRefused, "cannot reach match server"))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.set_nodelay(true)?;
        let config = WebSocketConfig::default().max_message_size(Some(262_144))
            .max_frame_size(Some(262_144)).write_buffer_size(0).max_write_buffer_size(524_288);
        // rustls uses WebPKI roots and validates the certificate and hostname.
        let (mut ws, _) = tungstenite::client_tls_with_config(url.as_str(), stream, Some(config), None)
            .map_err(|_| io::Error::other("Secure match handshake failed (check URL, certificate, or server availability)."))?;
        match ws.get_mut() {
            MaybeTlsStream::Plain(s) => s.set_nonblocking(true)?,
            MaybeTlsStream::Rustls(s) => s.sock.set_nonblocking(true)?,
            _ => return Err(invalid("unsupported TLS transport")),
        }
        Ok(Self::Public(Box::new(ws)))
    }
    pub fn send(&mut self, msg: &impl Serialize) -> io::Result<()> {
        match self {
            Self::Private(wire) => wire.send(msg),
            Self::Public(ws) => pending(ws.send(Message::Text(serde_json::to_string(msg).map_err(io::Error::other)?.into()))),
        }
    }
    pub fn flush(&mut self) -> io::Result<()> {
        match self { Self::Private(wire) => wire.flush(), Self::Public(ws) => pending(ws.flush()) }
    }
    pub fn receive<T: DeserializeOwned>(&mut self) -> io::Result<Vec<T>> {
        let Self::Public(ws) = self else { if let Self::Private(wire) = self { return wire.receive(); } unreachable!() };
        let mut messages = Vec::new();
        for _ in 0..8 {
            match ws.read() {
                Ok(Message::Text(text)) => messages.push(serde_json::from_str(&text).map_err(io::Error::other)?),
                Ok(Message::Ping(_)|Message::Pong(_)) => {},
                Ok(Message::Close(_)) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(_) => return Err(invalid("unexpected binary message")),
                Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(ws_error(e)),
            }
        }
        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn reject_insecure_public_transport() {
        assert!(endpoint("wss://play.peakrunner.net/match").is_ok());
        assert!(endpoint("ws://127.0.0.1:7782/match").is_ok());
        for url in ["ws://play.peakrunner.net/match", "http://example.com", "wss://a:b@example.com", "wss://example.com/#bad"] { assert!(endpoint(url).is_err()); }
        assert!(Transport::connect("1.1.1.1:7781").is_err());
    }
}
