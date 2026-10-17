use std::collections::VecDeque;
use std::io::{self, BufRead, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use serde::{de::DeserializeOwned, Serialize};

pub fn resolve(addr: &str) -> io::Result<SocketAddr> {
    addr.to_socket_addrs()?.next().ok_or_else(|| invalid("no address"))
}
pub fn invalid(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message) }

pub fn private_bind(addr: &str) -> io::Result<SocketAddr> {
    let addr = resolve(addr)?;
    let private = match addr.ip() {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private()
            || (u32::from(ip) & 0xffc00000) == 0x64400000, // VPN shared-address space
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
    };
    if !private { return Err(invalid("bind to a specific LAN/VPN or loopback IP; public/plaintext hosting is disabled")); }
    Ok(addr)
}
pub fn write_msg(stream: &mut impl Write, msg: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(msg).map_err(io::Error::other)?;
    bytes.push(b'\n');
    stream.write_all(&bytes)
}
pub fn read_msg<T: DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<T> {
    let mut bytes = Vec::new();
    reader.take(262_145).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() { return Err(io::ErrorKind::UnexpectedEof.into()); }
    if bytes.len() > 262_144 || bytes.last() != Some(&b'\n') { return Err(invalid("invalid frame size")); }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// Incremental framing: partial lines survive polls, reads/writes never block a tick.
pub struct Framed {
    pub stream: TcpStream,
    incoming: Vec<u8>,
    outgoing: VecDeque<Vec<u8>>,
    offset: usize,
    max_line: usize,
}
impl Framed {
    pub fn new(stream: TcpStream, max_line: usize) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self { stream, incoming: Vec::new(), outgoing: VecDeque::new(), offset: 0, max_line })
    }
    pub fn send(&mut self, msg: &impl Serialize) -> io::Result<()> {
        self.send_json(&serde_json::to_vec(msg).map_err(io::Error::other)?)
    }
    /// Send one already-encoded JSON message (the same bytes `send` makes),
    /// so one encoding can go to many connections.
    pub fn send_json(&mut self, json: &[u8]) -> io::Result<()> {
        if self.outgoing.len() >= 4 { return Err(invalid("slow consumer")); }
        if json.len() > 262_144 { return Err(invalid("snapshot too large")); }
        let mut bytes = Vec::with_capacity(json.len() + 1);
        bytes.extend_from_slice(json);
        bytes.push(b'\n');
        self.outgoing.push_back(bytes);
        self.flush()
    }
    pub fn flush(&mut self) -> io::Result<()> {
        while let Some(front) = self.outgoing.front() {
            match self.stream.write(&front[self.offset..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => {
                    self.offset += n;
                    if self.offset == front.len() { self.outgoing.pop_front(); self.offset = 0; }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
    pub fn receive<T: DeserializeOwned>(&mut self) -> io::Result<Vec<T>> {
        let mut buf = [0u8; 4096];
        // Bound the amount of work per peer per poll as well as the frame size.
        let budget = (self.max_line * 2).min(524_288);
        let mut read = 0;
        loop {
            if read >= budget { break; }
            match self.stream.read(&mut buf) {
                Ok(0) if read > 0 => break,
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => {
                    read += n;
                    self.incoming.extend_from_slice(&buf[..n]);
                    if self.incoming.len() > budget + 4096 { return Err(invalid("receive overflow")); }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        let mut messages = Vec::new();
        let mut consumed = 0;
        for line in self.incoming.split_inclusive(|b| *b == b'\n') {
            if line.len() > self.max_line { return Err(invalid("frame too large")); }
            if line.last() != Some(&b'\n') { break; }
            messages.push(serde_json::from_slice(line).map_err(io::Error::other)?);
            consumed += line.len();
            if messages.len() >= 8 { break; }
        }
        self.incoming.drain(..consumed);
        if self.incoming.len() > self.max_line && !self.incoming.contains(&b'\n') {
            return Err(invalid("frame too large"));
        }
        Ok(messages)
    }
}
