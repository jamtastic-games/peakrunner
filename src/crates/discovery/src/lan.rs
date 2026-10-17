use std::{io::{self, BufReader}, net::TcpStream, time::Duration};
use crate::{DirRequest, DirResponse, ServerAdvert, wire::{private_bind, read_msg, write_msg, invalid}};
#[derive(Clone, Debug)]
pub struct Lease { pub id: String, pub token: String }
pub fn register(directory: &str, name: &str, host: &str, port: u16, players: u32, max_players: u32, map: &str) -> io::Result<Lease> {
    match exchange(directory, &DirRequest::Register { name: name.into(), host: host.into(), port,
        players, max_players, map: map.into() })? {
        DirResponse::Registered { id, token } => Ok(Lease { id, token }),
        DirResponse::Error { message } => Err(io::Error::other(message)),
        _ => Err(invalid("unexpected reply")),
    }
}
pub fn heartbeat(directory: &str, lease: &Lease, players: u32) -> io::Result<()> {
    match exchange(directory, &DirRequest::Heartbeat { id: lease.id.clone(), token: lease.token.clone(), players })? {
        DirResponse::Ok => Ok(()), DirResponse::Error { message } => Err(io::Error::other(message)),
        _ => Err(invalid("unexpected reply")),
    }
}
pub fn list(directory: &str) -> io::Result<Vec<ServerAdvert>> {
    match exchange(directory, &DirRequest::List)? {
        DirResponse::Servers { servers } => Ok(servers),
        _ => Err(invalid("unexpected directory reply")),
    }
}
fn exchange(directory: &str, request: &DirRequest) -> io::Result<DirResponse> {
    let mut stream = TcpStream::connect_timeout(&private_bind(directory)?, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write_msg(&mut stream, request)?;
    read_msg(&mut BufReader::new(stream))
}
