use super::*;
use crate::domain::{Board, FLEET};
use crate::rng::Rng;

fn c(x: u8, y: u8) -> Coord {
    Coord::new(x, y)
}

fn round_trip(message: Message) {
    let line = message.encode();
    assert!(!line.contains('\n'), "{line:?}");
    assert_eq!(Message::decode(&line), Ok(message), "{line:?}");
}

#[test]
fn encodes_the_simple_messages() {
    assert_eq!(Message::Hello { version: 1 }.encode(), "HELLO BATTLESHIP 1");
    assert_eq!(Message::Ready.encode(), "READY");
    assert_eq!(Message::Fire(c(3, 7)).encode(), "FIRE 3 7");
    assert_eq!(Message::Result(ShotResult::Miss).encode(), "RESULT MISS");
    assert_eq!(Message::Result(ShotResult::Hit).encode(), "RESULT HIT");
    assert_eq!(Message::Bye.encode(), "BYE");
}

#[test]
fn encodes_a_sinking_with_the_ship_squares() {
    let sunk = ShotResult::Sunk {
        kind: ShipKind::Destroyer,
        cells: vec![c(3, 7), c(4, 7)],
    };
    assert_eq!(
        Message::Result(sunk).encode(),
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
    round_trip(Message::Result(ShotResult::Sunk {
        kind: ShipKind::Carrier,
        cells: (0..5).map(|y| c(9, y)).collect(),
    }));
    round_trip(Message::Reveal(
        Board::random(&mut Rng::new(4)).placements(),
    ));
    round_trip(Message::Reveal(vec![]));
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
    ] {
        assert!(Message::decode(line).is_err(), "accepted {line:?}");
    }
}

#[test]
fn protocol_errors_read_as_such() {
    assert_eq!(
        ProtocolError("bad line".into()).to_string(),
        "protocol error: bad line"
    );
}
