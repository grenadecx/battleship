//! The browser's connection to the other player, through the relay server.
//!
//! [`RelayLink`] first runs the lobby of [`crate::relay`] (host or join a room),
//! then the same `HELLO` handshake as a direct TCP connection, and from then on
//! is an [`Opponent`] like any other. It only needs a message-based socket, the
//! [`Transport`], so the logic is tested here without a browser or a network.

use crate::opponent::{Opponent, OpponentEvent};
use crate::protocol::{Message, PROTOCOL_VERSION};
use crate::relay::{Reply, Request};
use std::time::Duration;

/// Seconds between latency measurements.
const PING_INTERVAL: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketState {
    Connecting,
    Open,
    /// Closed or failed. Messages that arrived before can still be received.
    Closed,
}

/// A WebSocket, or anything else that carries whole text messages.
pub trait Transport {
    fn state(&self) -> SocketState;
    fn send(&mut self, text: &str);
    /// The next received message, if any. Never blocks.
    fn recv(&mut self) -> Option<String>;
    fn close(&mut self);
    /// A clock in seconds, for measuring latency.
    fn now(&self) -> f64;
}

/// How far [`RelayLink::progress`] has come.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Still connecting, waiting in the lobby or shaking hands.
    Waiting,
    /// Connected to the other player: the link can be used as an [`Opponent`].
    Ready,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    Connecting,
    Lobby,
    Handshake,
    Ready,
    Failed(String),
}

pub struct RelayLink<T: Transport> {
    transport: T,
    request: Request,
    stage: Stage,
    room: Option<String>,
    closed: bool,
    /// The last ping sent: its id and when it went out.
    ping: Option<(u32, f64)>,
    latency: Option<Duration>,
}

impl<T: Transport> RelayLink<T> {
    /// Opens a room for someone else to join. The host fires first.
    pub fn host(transport: T) -> Self {
        Self::new(transport, Request::Host)
    }

    pub fn join(transport: T, code: &str) -> Self {
        Self::new(transport, Request::Join(code.to_string()))
    }

    fn new(transport: T, request: Request) -> Self {
        RelayLink {
            transport,
            request,
            stage: Stage::Connecting,
            room: None,
            closed: false,
            ping: None,
            latency: None,
        }
    }

    /// The code to share with the other player, once the server opened the room.
    pub fn room(&self) -> Option<&str> {
        self.room.as_deref()
    }

    pub fn i_go_first(&self) -> bool {
        self.request == Request::Host
    }

    /// Drives the lobby and the handshake. Call it every frame until it
    /// returns [`Progress::Ready`] or [`Progress::Failed`].
    pub fn progress(&mut self) -> Progress {
        loop {
            match self.stage.clone() {
                Stage::Connecting => match self.transport.state() {
                    SocketState::Connecting => return Progress::Waiting,
                    SocketState::Open => {
                        self.transport.send(&self.request.encode());
                        self.stage = Stage::Lobby;
                    }
                    SocketState::Closed => self.fail("Could not reach the game server"),
                },
                Stage::Lobby => match self.transport.recv() {
                    Some(line) => match Reply::decode(&line) {
                        Ok(Reply::Room(code)) if self.i_go_first() => self.room = Some(code),
                        Ok(Reply::Paired) => {
                            self.transport.send(
                                &Message::Hello {
                                    version: PROTOCOL_VERSION,
                                }
                                .encode(),
                            );
                            self.stage = Stage::Handshake;
                        }
                        Ok(Reply::Error(reason)) => self.fail(&reason),
                        _ => self.fail("The game server sent something unexpected"),
                    },
                    None if self.transport.state() == SocketState::Closed => {
                        self.fail("Lost the connection to the game server")
                    }
                    None => return Progress::Waiting,
                },
                Stage::Handshake => match self.transport.recv() {
                    Some(line) => match Message::decode(&line) {
                        Ok(Message::Hello { version }) if version == PROTOCOL_VERSION => {
                            self.stage = Stage::Ready;
                        }
                        Ok(Message::Hello { version }) => self.fail(&format!(
                            "Opponent speaks protocol version {version}, this game speaks version {PROTOCOL_VERSION}"
                        )),
                        _ => self.fail("The other side is not a Battleship game"),
                    },
                    None if self.transport.state() == SocketState::Closed => {
                        self.fail("Your opponent left before the game started")
                    }
                    None => return Progress::Waiting,
                },
                Stage::Ready => return Progress::Ready,
                Stage::Failed(reason) => return Progress::Failed(reason),
            }
        }
    }

    fn fail(&mut self, reason: &str) {
        self.stage = Stage::Failed(reason.to_string());
        self.transport.close();
    }

    fn ping_if_due(&mut self) {
        let now = self.transport.now();
        let id = match self.ping {
            Some((_, sent)) if now - sent < PING_INTERVAL => return,
            Some((id, _)) => id.wrapping_add(1),
            None => 0,
        };
        self.send(Message::Ping(id));
        self.ping = Some((id, now));
    }

    fn disconnect(&mut self, reason: String) -> Option<OpponentEvent> {
        self.closed = true;
        self.transport.close();
        Some(OpponentEvent::Disconnected(reason))
    }
}

impl<T: Transport> Opponent for RelayLink<T> {
    fn send(&mut self, message: Message) {
        if self.stage == Stage::Ready && !self.closed {
            self.transport.send(&message.encode());
        }
    }

    /// Answers pings on the spot, like the TCP link's reader thread does.
    fn poll(&mut self) -> Option<OpponentEvent> {
        if self.closed || self.stage != Stage::Ready {
            return None;
        }
        self.ping_if_due();
        loop {
            let Some(line) = self.transport.recv() else {
                if self.transport.state() == SocketState::Closed {
                    return self.disconnect("Lost the connection to your opponent".into());
                }
                return None;
            };
            match Message::decode(&line) {
                Ok(Message::Ping(id)) => self.send(Message::Pong(id)),
                Ok(Message::Pong(id)) => {
                    if let Some((sent_id, sent)) = self.ping
                        && sent_id == id
                    {
                        let rtt = (self.transport.now() - sent).max(0.0);
                        self.latency = Some(Duration::from_secs_f64(rtt));
                    }
                }
                Ok(Message::Bye) => return self.disconnect("Your opponent left the game".into()),
                Ok(message) => return Some(OpponentEvent::Message(message)),
                Err(error) => return self.disconnect(error.to_string()),
            }
        }
    }

    fn latency(&self) -> Option<Duration> {
        self.latency
    }
}

impl<T: Transport> Drop for RelayLink<T> {
    fn drop(&mut self) {
        if self.stage == Stage::Ready && !self.closed {
            self.transport.send(&Message::Bye.encode());
        }
        self.transport.close();
    }
}

#[cfg(test)]
#[path = "tests/relay_link.rs"]
mod tests;
