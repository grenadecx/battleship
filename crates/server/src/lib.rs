//! Web service for the browser version of Battleship. It serves the files from
//! `scripts/build-web.sh` and relays games between browser players: one player
//! opens a room, the other joins it with the room code, and from then on every
//! WebSocket frame from one is passed to the other unchanged. The lobby protocol
//! is in `battleship_core::relay`; the server does not look at the game itself.

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use battleship_core::relay::{Reply, Request, new_code};
use battleship_core::rng::Rng;
use futures_util::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tower_http::services::ServeDir;

/// Rooms waiting for a guest at the same time.
pub const MAX_ROOMS: usize = 1000;
/// Game messages are short lines; anything bigger is not a Battleship client.
const MAX_FRAME_BYTES: usize = 4096;
/// How long a new connection may take to say HOST or JOIN.
const LOBBY_TIMEOUT: Duration = Duration::from_secs(30);

/// What a connection's writer sends to its browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    Text(String),
    Close,
}

/// The way to reach one connected player.
pub type Peer = mpsc::UnboundedSender<Out>;

struct Waiting {
    host: Peer,
    guest: oneshot::Sender<Peer>,
}

/// Rooms whose host is waiting for a guest.
pub struct Rooms {
    waiting: Mutex<HashMap<String, Waiting>>,
    rng: Mutex<Rng>,
    capacity: usize,
}

impl Rooms {
    pub fn new(capacity: usize, rng: Rng) -> Self {
        Rooms {
            waiting: Mutex::new(HashMap::new()),
            rng: Mutex::new(rng),
            capacity,
        }
    }

    /// Opens a room for `host`. Returns its code and where the guest will arrive,
    /// or `None` when the server is full.
    pub fn open(&self, host: Peer) -> Option<(String, oneshot::Receiver<Peer>)> {
        let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
        if waiting.len() >= self.capacity {
            return None;
        }
        let mut rng = self.rng.lock().unwrap_or_else(|e| e.into_inner());
        let code = loop {
            let code = new_code(&mut rng);
            if !waiting.contains_key(&code) {
                break code;
            }
        };
        let (guest, arrives) = oneshot::channel();
        waiting.insert(code.clone(), Waiting { host, guest });
        Some((code, arrives))
    }

    /// Pairs `guest` with the host of room `code` and tells both. Returns the host.
    pub fn join(&self, code: &str, guest: Peer) -> Result<Peer, String> {
        let room = self
            .waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(code);
        let missing = || format!("No game with code {code}");
        let room = room.ok_or_else(missing)?;
        // The host may have left a moment ago without closing the room yet.
        room.guest.send(guest.clone()).map_err(|_| missing())?;
        let paired = Out::Text(Reply::Paired.encode());
        let _ = room.host.send(paired.clone());
        let _ = guest.send(paired);
        Ok(room.host)
    }

    /// Closes a room nobody joined.
    pub fn close(&self, code: &str) {
        self.waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(code);
    }

    /// Rooms currently waiting for a guest.
    pub fn len(&self) -> usize {
        self.waiting.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The whole service: the relay at `/ws`, a health check at `/healthz`, and
/// everything else from `web_dir`.
pub fn app(web_dir: impl AsRef<Path>) -> Router {
    app_with_rooms(web_dir, Arc::new(Rooms::new(MAX_ROOMS, Rng::from_time())))
}

pub fn app_with_rooms(web_dir: impl AsRef<Path>, rooms: Arc<Rooms>) -> Router {
    Router::new()
        .route("/ws", get(upgrade))
        .route("/healthz", get(|| async { "ok" }))
        .fallback_service(ServeDir::new(web_dir.as_ref()))
        .with_state(rooms)
}

async fn upgrade(socket: WebSocketUpgrade, State(rooms): State<Arc<Rooms>>) -> Response {
    socket
        .max_message_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| session(socket, rooms))
}

/// One browser's connection, from the lobby to the end of its game.
async fn session(socket: WebSocket, rooms: Arc<Rooms>) {
    let (mut sink, mut stream) = socket.split();
    let (me, mut outbox) = mpsc::unbounded_channel::<Out>();
    let writer = tokio::spawn(async move {
        while let Some(Out::Text(text)) = outbox.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    if let Some(peer) = lobby(&mut stream, &me, &rooms).await {
        relay(&mut stream, &peer).await;
        let _ = peer.send(Out::Close);
    }
    let _ = me.send(Out::Close);
    let _ = writer.await;
}

/// Handles HOST or JOIN. Returns the other player once paired.
async fn lobby<S>(stream: &mut S, me: &Peer, rooms: &Rooms) -> Option<Peer>
where
    S: Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    let reply = |reply: Reply| {
        let _ = me.send(Out::Text(reply.encode()));
    };
    let first = tokio::time::timeout(LOBBY_TIMEOUT, next_text(stream))
        .await
        .ok()??;
    match Request::decode(&first) {
        Ok(Request::Host) => {
            let Some((code, mut guest)) = rooms.open(me.clone()) else {
                reply(Reply::Error("The server is full, try again later".into()));
                return None;
            };
            reply(Reply::Room(code.clone()));
            loop {
                tokio::select! {
                    guest = &mut guest => return guest.ok(),
                    frame = stream.next() => match frame {
                        Some(Ok(Message::Close(_)) | Err(_)) | None => {
                            rooms.close(&code);
                            return None;
                        }
                        // Nobody to pass anything to yet.
                        Some(Ok(_)) => {}
                    },
                }
            }
        }
        Ok(Request::Join(code)) => match rooms.join(&code, me.clone()) {
            Ok(host) => Some(host),
            Err(reason) => {
                reply(Reply::Error(reason));
                None
            }
        },
        Err(_) => {
            reply(Reply::Error("Expected HOST or JOIN".into()));
            None
        }
    }
}

/// Passes this player's frames to the other until either side leaves.
async fn relay<S>(stream: &mut S, peer: &Peer)
where
    S: Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    while let Some(Ok(frame)) = stream.next().await {
        match frame {
            Message::Text(text) => {
                if peer.send(Out::Text(text.to_string())).is_err() {
                    return;
                }
            }
            Message::Close(_) => return,
            // Pings are answered by axum; the game never sends binary frames.
            _ => {}
        }
    }
}

async fn next_text<S>(stream: &mut S) -> Option<String>
where
    S: Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    loop {
        match stream.next().await? {
            Ok(Message::Text(text)) => return Some(text.to_string()),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

#[cfg(test)]
#[path = "tests/server.rs"]
mod tests;
