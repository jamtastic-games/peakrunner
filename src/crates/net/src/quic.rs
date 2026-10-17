//! Client-only encrypted UDP session.
use std::{collections::VecDeque, io, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use tokio::sync::mpsc;
use quinn::Connection;
use tokio::time::timeout;
use crate::{client::Lobby, proto::{ClientMsg, ServerMsg}};
use peakrunner_discovery::{wire::invalid, quic::{connect_endpoint, send_control, read_control_limited, SERVER_CONTROL_LIMIT}};
use peakrunner_protocol::packets::{inputs, Receiver};
fn apply(lobby: &Arc<Mutex<Lobby>>, message: ServerMsg) -> io::Result<()> {
    let mut l = lobby.lock().unwrap();
    match message {
        ServerMsg::Welcome { player_id, match_name } => { l.player_id = player_id; l.match_name = match_name; },
        ServerMsg::Reject { message } => return Err(io::Error::other(message)),
        ServerMsg::Status { .. } | ServerMsg::File { .. } => return Err(invalid("unexpected status")),
        ServerMsg::Snapshot { state } => {
            if !l.accept(state) { return Err(invalid("player missing from snapshot")); }
        }
        ServerMsg::Notice { message } => l.notify(message),
        ServerMsg::Duels { board } => l.duels = Some(board),
    }
    Ok(())
}
async fn next_control(mut recv: quinn::RecvStream) -> (quinn::RecvStream, io::Result<ServerMsg>) {
    let message = read_control_limited(&mut recv, SERVER_CONTROL_LIMIT).await;
    (recv, message)
}
pub(crate) async fn run_client(address: &str, hello: ClientMsg, outbound: &mut mpsc::Receiver<ClientMsg>,
    lobby: Arc<Mutex<Lobby>>, stop: Arc<AtomicBool>) -> io::Result<()> {
    let (endpoint, conn) = connect_endpoint(address).await?;
    let result = client_connection(&conn, hello, outbound, lobby, stop).await;
    conn.close(0u32.into(), b"client left"); endpoint.wait_idle().await;
    result
}
async fn client_connection(conn: &Connection, hello: ClientMsg, outbound: &mut mpsc::Receiver<ClientMsg>,
    lobby: Arc<Mutex<Lobby>>, stop: Arc<AtomicBool>) -> io::Result<()> {
    let (mut send, mut recv) = conn.open_bi().await.map_err(io::Error::other)?;
    timeout(Duration::from_secs(3), send_control(&mut send, &hello)).await.map_err(io::Error::other)??;
    let welcome = timeout(Duration::from_secs(4), read_control_limited::<ServerMsg>(&mut recv, SERVER_CONTROL_LIMIT)).await.map_err(io::Error::other)??;
    apply(&lobby, welcome)?;
    // Snapshots arrive as deltas against the newest one acknowledged with
    // each input.
    let mut assembly = Receiver::default(); let mut history = VecDeque::new();
    let mut last_snapshot = Instant::now();
    // Inputs go out the moment the game queues them; this only checks for
    // leaving and for a silent server.
    let mut poll = tokio::time::interval(Duration::from_millis(50));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Keep this future pinned: cancel/restarting a partial stream read loses
    // framing. It owns the stream and hands it back for the next message.
    let control = next_control(recv); tokio::pin!(control);
    loop {
        tokio::select! {
            data = conn.read_datagram() => {
                if let Some(state) = assembly.accept(&data.map_err(io::Error::other)?)? {
                    apply(&lobby, ServerMsg::Snapshot { state })?; last_snapshot = Instant::now();
                }
            }
            (stream, message) = &mut control => {
                // Notices and a duel server's board keep the session;
                // anything else ends it.
                let message = message?;
                let keeps = matches!(message, ServerMsg::Notice { .. } | ServerMsg::Duels { .. });
                apply(&lobby, message)?;
                if !keeps { return Err(invalid("unexpected server control")); }
                control.set(next_control(stream));
            }
            message = outbound.recv() => {
                if stop.load(Ordering::Relaxed) { return Ok(()); }
                match message {
                    Some(ClientMsg::Input { command }) => {
                        // Inputs that queued up while this thread was held up
                        // (the game in the background, a hitch) go as one
                        // datagram of the newest, not a burst the server's
                        // flood limit would end the session for.
                        history.push_back(command);
                        let mut queued_control = None;
                        while let Ok(next) = outbound.try_recv() {
                            match next { ClientMsg::Input { command } => history.push_back(command), other => { queued_control = Some(other); break; } }
                        }
                        while history.len() > 3 { history.pop_front(); }
                        conn.send_datagram(inputs(&history, assembly.acked()).into()).map_err(io::Error::other)?;
                        if let Some(message) = queued_control {
                            timeout(Duration::from_secs(1), send_control(&mut send, &message)).await.map_err(io::Error::other)??;
                        }
                    }
                    Some(message) => timeout(Duration::from_secs(1), send_control(&mut send, &message)).await.map_err(io::Error::other)??,
                    None => return Ok(()),
                }
            }
            _ = poll.tick() => {
                if stop.load(Ordering::Relaxed) { return Ok(()); }
                if last_snapshot.elapsed() > Duration::from_secs(5) { return Err(invalid("server snapshots timed out")); }
            }
        }
    }
}
