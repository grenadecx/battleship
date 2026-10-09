//! Line based text protocol spoken between two peers (and the computer opponent).
//!
//! ```text
//! HELLO BATTLESHIP 2
//! READY
//! FIRE 3 7
//! RESULT MISS | RESULT HIT | RESULT SUNK Cruiser 3,7 4,7 5,7
//! REVEAL Carrier,0,0,H Battleship,2,4,V ...
//! PING 42 | PONG 42
//! BYE
//! ```

use crate::domain::{Coord, Orientation, Placement, ShipKind, ShotResult};

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Hello {
        version: u32,
    },
    /// The sender has placed their fleet and is ready to play.
    Ready,
    Fire(Coord),
    Result(ShotResult),
    /// The sender's fleet, shared once the game is over.
    Reveal(Vec<Placement>),
    /// Asks the peer to echo the id back in a `Pong`, to measure latency.
    Ping(u32),
    Pong(u32),
    Bye,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError(pub String);

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "protocol error: {}", self.0)
    }
}

impl Message {
    /// Encodes as a single line, without the trailing newline.
    pub fn encode(&self) -> String {
        match self {
            Message::Hello { version } => format!("HELLO BATTLESHIP {version}"),
            Message::Ready => "READY".to_string(),
            Message::Fire(c) => format!("FIRE {} {}", c.x, c.y),
            Message::Result(ShotResult::Miss) => "RESULT MISS".to_string(),
            Message::Result(ShotResult::Hit) => "RESULT HIT".to_string(),
            Message::Result(ShotResult::Sunk { kind, cells }) => {
                let mut line = format!("RESULT SUNK {kind}");
                for c in cells {
                    line.push_str(&format!(" {},{}", c.x, c.y));
                }
                line
            }
            Message::Reveal(placements) => {
                let mut line = "REVEAL".to_string();
                for p in placements {
                    let o = match p.orientation {
                        Orientation::Horizontal => 'H',
                        Orientation::Vertical => 'V',
                    };
                    line.push_str(&format!(" {},{},{},{o}", p.kind, p.origin.x, p.origin.y));
                }
                line
            }
            Message::Ping(id) => format!("PING {id}"),
            Message::Pong(id) => format!("PONG {id}"),
            Message::Bye => "BYE".to_string(),
        }
    }

    pub fn decode(line: &str) -> Result<Message, ProtocolError> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let message = match words.as_slice() {
            ["HELLO", "BATTLESHIP", version] => Message::Hello {
                version: version.parse().map_err(|_| error(line))?,
            },
            ["READY"] => Message::Ready,
            ["BYE"] => Message::Bye,
            ["PING", id] => Message::Ping(id.parse().map_err(|_| error(line))?),
            ["PONG", id] => Message::Pong(id.parse().map_err(|_| error(line))?),
            ["FIRE", x, y] => Message::Fire(coord(x, y).ok_or_else(|| error(line))?),
            ["RESULT", "MISS"] => Message::Result(ShotResult::Miss),
            ["RESULT", "HIT"] => Message::Result(ShotResult::Hit),
            ["RESULT", "SUNK", kind, cells @ ..] => {
                let kind = ShipKind::from_name(kind).ok_or_else(|| error(line))?;
                let cells = cells
                    .iter()
                    .map(
                        |cell| match cell.split(',').collect::<Vec<_>>().as_slice() {
                            [x, y] => coord(x, y),
                            _ => None,
                        },
                    )
                    .collect::<Option<Vec<Coord>>>()
                    .ok_or_else(|| error(line))?;
                if cells.len() != kind.size() as usize {
                    return Err(error(line));
                }
                Message::Result(ShotResult::Sunk { kind, cells })
            }
            ["REVEAL", ships @ ..] => Message::Reveal(
                ships
                    .iter()
                    .map(|ship| placement(ship))
                    .collect::<Option<Vec<Placement>>>()
                    .ok_or_else(|| error(line))?,
            ),
            _ => return Err(error(line)),
        };
        Ok(message)
    }
}

fn error(line: &str) -> ProtocolError {
    ProtocolError(format!("unexpected message {:?}", line.trim()))
}

fn coord(x: &str, y: &str) -> Option<Coord> {
    Coord::try_new(x.parse().ok()?, y.parse().ok()?)
}

fn placement(text: &str) -> Option<Placement> {
    let [kind, x, y, orientation] = text.split(',').collect::<Vec<_>>()[..] else {
        return None;
    };
    let orientation = match orientation {
        "H" => Orientation::Horizontal,
        "V" => Orientation::Vertical,
        _ => return None,
    };
    Some(Placement::new(
        ShipKind::from_name(kind)?,
        coord(x, y)?,
        orientation,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Board, FLEET};
    use crate::rng::Rng;
    use crate::test_support::{c, sunk};

    fn round_trip(message: Message) {
        let line = message.encode();
        assert!(!line.contains('\n'), "{line:?}");
        assert_eq!(Message::decode(&line), Ok(message), "{line:?}");
    }

    #[test]
    fn encodes_the_simple_messages() {
        for (message, line) in [
            (Message::Hello { version: 1 }, "HELLO BATTLESHIP 1"),
            (Message::Ready, "READY"),
            (Message::Fire(c(3, 7)), "FIRE 3 7"),
            (Message::Result(ShotResult::Miss), "RESULT MISS"),
            (Message::Result(ShotResult::Hit), "RESULT HIT"),
            (Message::Ping(42), "PING 42"),
            (Message::Pong(42), "PONG 42"),
            (Message::Bye, "BYE"),
        ] {
            assert_eq!(message.encode(), line, "{message:?}");
        }
    }

    #[test]
    fn encodes_a_sinking_with_the_ship_squares() {
        assert_eq!(
            Message::Result(sunk(ShipKind::Destroyer, &[c(3, 7), c(4, 7)])).encode(),
            "RESULT SUNK Destroyer 3,7 4,7"
        );
    }

    #[test]
    fn every_message_round_trips() {
        round_trip(Message::Hello {
            version: PROTOCOL_VERSION,
        });
        round_trip(Message::Ready);
        round_trip(Message::Fire(c(0, 0)));
        round_trip(Message::Fire(c(9, 9)));
        round_trip(Message::Result(ShotResult::Miss));
        round_trip(Message::Result(ShotResult::Hit));
        round_trip(Message::Result(sunk(
            ShipKind::Carrier,
            &[c(9, 0), c(9, 1), c(9, 2), c(9, 3), c(9, 4)],
        )));
        round_trip(Message::Reveal(
            Board::random(&mut Rng::new(4)).placements(),
        ));
        round_trip(Message::Reveal(vec![]));
        round_trip(Message::Ping(u32::MAX));
        round_trip(Message::Pong(0));
        round_trip(Message::Bye);
    }

    #[test]
    fn reveal_contains_every_ship() {
        let placements = Board::random(&mut Rng::new(8)).placements();
        let Ok(Message::Reveal(decoded)) = Message::decode(&Message::Reveal(placements).encode())
        else {
            panic!("not a reveal");
        };
        let mut kinds: Vec<ShipKind> = decoded.iter().map(|pl| pl.kind).collect();
        kinds.sort();
        assert_eq!(kinds, FLEET.to_vec());
    }

    #[test]
    fn tolerates_surrounding_whitespace_and_carriage_returns() {
        assert_eq!(Message::decode("  FIRE 1 2\r"), Ok(Message::Fire(c(1, 2))));
    }

    #[test]
    fn rejects_garbage() {
        for line in [
            "",
            "HELLO",
            "HELLO CHESS 1",
            "LAUNCH 1 2",
            "FIRE 1",
            "FIRE 1 2 3",
            "FIRE a b",
            "FIRE 10 0",
            "FIRE -1 0",
            "RESULT",
            "RESULT MAYBE",
            "RESULT SUNK Dinghy 1,1",
            "RESULT SUNK Destroyer",
            "RESULT SUNK Destroyer 1,1",
            "RESULT SUNK Destroyer 1;1 2,1",
            "REVEAL Carrier,0,0,X",
            "REVEAL Carrier,0,0",
            "PING",
            "PING -1",
            "PONG x",
        ] {
            assert!(Message::decode(line).is_err(), "accepted {line:?}");
        }
    }
}
