//! Line based text protocol spoken between two peers (and the computer opponent).
//!
//! ```text
//! HELLO BATTLESHIP 1
//! READY
//! FIRE 3 7
//! RESULT MISS | RESULT HIT | RESULT SUNK Cruiser 3,7 4,7 5,7
//! REVEAL Carrier,0,0,H Battleship,2,4,V ...
//! BYE
//! ```

use crate::domain::{Coord, Orientation, Placement, ShipKind, ShotResult};

pub const PROTOCOL_VERSION: u32 = 1;

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
#[path = "tests/protocol.rs"]
mod tests;
