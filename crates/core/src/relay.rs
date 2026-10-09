//! Lobby protocol of the relay server, through which browser players meet.
//! Browsers can neither listen for connections nor open raw TCP sockets, so two
//! players each open a WebSocket to the server and pair up with a room code.
//!
//! ```text
//! client -> server   HOST | JOIN K7QD
//! server -> client   ROOM K7QD | PAIRED | ERROR No game with code K7QD
//! ```
//!
//! One frame carries one line. After `PAIRED` the server passes every frame on
//! to the other player unchanged, so from then on the two speak the game
//! protocol of [`crate::protocol`], starting with the `HELLO` handshake.

use crate::protocol::ProtocolError;
use crate::rng::Rng;

/// Letters and digits that cannot be mistaken for one another (no 0/O, 1/I).
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const CODE_LENGTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Open a room and wait for someone to join it.
    Host,
    /// Join the room with this code.
    Join(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// The room is open under this code.
    Room(String),
    /// Both players are here; the game protocol starts.
    Paired,
    /// Why the request failed. The server closes the connection after it.
    Error(String),
}

fn error(line: &str) -> ProtocolError {
    ProtocolError(format!("unexpected relay message {:?}", line.trim()))
}

impl Request {
    pub fn encode(&self) -> String {
        match self {
            Request::Host => "HOST".to_string(),
            Request::Join(code) => format!("JOIN {code}"),
        }
    }

    pub fn decode(line: &str) -> Result<Request, ProtocolError> {
        match line.split_whitespace().collect::<Vec<_>>().as_slice() {
            ["HOST"] => Ok(Request::Host),
            ["JOIN", code] => Ok(Request::Join(normalize_code(code))),
            _ => Err(error(line)),
        }
    }
}

impl Reply {
    pub fn encode(&self) -> String {
        match self {
            Reply::Room(code) => format!("ROOM {code}"),
            Reply::Paired => "PAIRED".to_string(),
            Reply::Error(reason) => format!("ERROR {reason}"),
        }
    }

    pub fn decode(line: &str) -> Result<Reply, ProtocolError> {
        let line = line.trim();
        match line.split_once(' ').unwrap_or((line, "")) {
            ("ROOM", code) if !code.is_empty() => Ok(Reply::Room(code.to_string())),
            ("PAIRED", "") => Ok(Reply::Paired),
            ("ERROR", reason) => Ok(Reply::Error(reason.to_string())),
            _ => Err(error(line)),
        }
    }
}

/// A fresh random room code.
pub fn new_code(rng: &mut Rng) -> String {
    (0..CODE_LENGTH)
        .map(|_| CODE_ALPHABET[rng.below(CODE_ALPHABET.len())] as char)
        .collect()
}

/// A code as a player typed it, in the form the server uses: upper case,
/// letters and digits only, at most [`CODE_LENGTH`] long.
pub fn normalize_code(input: &str) -> String {
    input
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .take(CODE_LENGTH)
        .collect()
}

#[cfg(test)]
#[path = "tests/relay.rs"]
mod tests;
