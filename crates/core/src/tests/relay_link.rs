use super::*;
use crate::domain::Coord;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// The far end of a fake socket: what the link sent and what it receives next.
struct Wire {
    state: SocketState,
    sent: Vec<String>,
    incoming: VecDeque<String>,
    closed_by_link: bool,
    now: f64,
}

struct Fake(Rc<RefCell<Wire>>);

impl Transport for Fake {
    fn state(&self) -> SocketState {
        self.0.borrow().state
    }

    fn send(&mut self, text: &str) {
        self.0.borrow_mut().sent.push(text.to_string());
    }

    fn recv(&mut self) -> Option<String> {
        self.0.borrow_mut().incoming.pop_front()
    }

    fn close(&mut self) {
        let mut wire = self.0.borrow_mut();
        wire.state = SocketState::Closed;
        wire.closed_by_link = true;
    }

    fn now(&self) -> f64 {
        self.0.borrow().now
    }
}

fn wire(state: SocketState) -> Rc<RefCell<Wire>> {
    Rc::new(RefCell::new(Wire {
        state,
        sent: Vec::new(),
        incoming: VecDeque::new(),
        closed_by_link: false,
        now: 0.0,
    }))
}

fn say(wire: &Rc<RefCell<Wire>>, line: &str) {
    wire.borrow_mut().incoming.push_back(line.to_string());
}

fn take_sent(wire: &Rc<RefCell<Wire>>) -> Vec<String> {
    std::mem::take(&mut wire.borrow_mut().sent)
}

fn hello() -> String {
    Message::Hello {
        version: PROTOCOL_VERSION,
    }
    .encode()
}

/// A link that got through the lobby and the handshake, with the wire cleared.
fn ready(host: bool) -> (RelayLink<Fake>, Rc<RefCell<Wire>>) {
    let w = wire(SocketState::Open);
    let mut link = if host {
        RelayLink::host(Fake(w.clone()))
    } else {
        RelayLink::join(Fake(w.clone()), "K7QD")
    };
    if host {
        say(&w, "ROOM K7QD");
    }
    say(&w, "PAIRED");
    say(&w, &hello());
    assert_eq!(link.progress(), Progress::Ready);
    take_sent(&w);
    (link, w)
}

#[test]
fn waits_while_the_socket_connects() {
    let w = wire(SocketState::Connecting);
    let mut link = RelayLink::host(Fake(w.clone()));
    assert_eq!(link.progress(), Progress::Waiting);
    assert!(take_sent(&w).is_empty());
}

#[test]
fn hosting_asks_for_a_room_once_connected() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::host(Fake(w.clone()));
    assert_eq!(link.progress(), Progress::Waiting);
    assert_eq!(take_sent(&w), vec!["HOST"]);
}

#[test]
fn joining_asks_for_the_room() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    link.progress();
    assert_eq!(take_sent(&w), vec!["JOIN K7QD"]);
}

#[test]
fn the_host_learns_its_room_code() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::host(Fake(w.clone()));
    assert_eq!(link.room(), None);
    say(&w, "ROOM K7QD");
    assert_eq!(link.progress(), Progress::Waiting);
    assert_eq!(link.room(), Some("K7QD"));
}

#[test]
fn pairing_starts_the_handshake() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    link.progress();
    take_sent(&w);
    say(&w, "PAIRED");
    assert_eq!(link.progress(), Progress::Waiting);
    assert_eq!(take_sent(&w), vec![hello()]);
}

#[test]
fn the_handshake_completes_the_connection() {
    let (link, _) = ready(true);
    assert!(link.i_go_first());
    let (link, _) = ready(false);
    assert!(!link.i_go_first());
}

#[test]
fn messages_after_the_handshake_wait_for_the_game() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    say(&w, "PAIRED");
    say(&w, &hello());
    say(&w, "READY");
    assert_eq!(link.progress(), Progress::Ready);
    assert_eq!(link.poll(), Some(OpponentEvent::Message(Message::Ready)));
}

#[test]
fn a_server_error_fails_and_closes() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "ZZZZ");
    say(&w, "ERROR No game with code ZZZZ");
    assert_eq!(
        link.progress(),
        Progress::Failed("No game with code ZZZZ".into())
    );
    assert!(w.borrow().closed_by_link);
}

#[test]
fn failure_is_final() {
    let w = wire(SocketState::Closed);
    let mut link = RelayLink::host(Fake(w.clone()));
    let failed = Progress::Failed("Could not reach the game server".into());
    assert_eq!(link.progress(), failed);
    assert_eq!(link.progress(), failed);
}

#[test]
fn a_room_code_sent_to_a_guest_is_unexpected() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    say(&w, "ROOM ABCD");
    assert_eq!(
        link.progress(),
        Progress::Failed("The game server sent something unexpected".into())
    );
}

#[test]
fn losing_the_server_in_the_lobby_fails() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::host(Fake(w.clone()));
    link.progress();
    w.borrow_mut().state = SocketState::Closed;
    assert_eq!(
        link.progress(),
        Progress::Failed("Lost the connection to the game server".into())
    );
}

#[test]
fn another_protocol_version_fails_the_handshake() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    say(&w, "PAIRED");
    say(&w, "HELLO BATTLESHIP 999");
    let Progress::Failed(reason) = link.progress() else {
        panic!("expected a failure");
    };
    assert!(reason.contains("version 999"), "{reason}");
}

#[test]
fn a_stranger_fails_the_handshake() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    say(&w, "PAIRED");
    say(&w, "GET / HTTP/1.1");
    assert_eq!(
        link.progress(),
        Progress::Failed("The other side is not a Battleship game".into())
    );
}

#[test]
fn the_opponent_leaving_during_the_handshake_fails() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::join(Fake(w.clone()), "K7QD");
    say(&w, "PAIRED");
    link.progress();
    w.borrow_mut().state = SocketState::Closed;
    assert_eq!(
        link.progress(),
        Progress::Failed("Your opponent left before the game started".into())
    );
}

#[test]
fn nothing_is_sent_or_received_before_the_link_is_ready() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::host(Fake(w.clone()));
    say(&w, "ROOM K7QD");
    link.send(Message::Ready);
    assert_eq!(link.poll(), None);
    assert!(take_sent(&w).is_empty());
    assert_eq!(w.borrow().incoming.len(), 1, "ROOM is left for progress");
}

#[test]
fn sends_game_messages_as_lines() {
    let (mut link, w) = ready(true);
    link.send(Message::Fire(Coord::new(3, 4)));
    assert_eq!(take_sent(&w), vec!["FIRE 3 4"]);
}

#[test]
fn passes_game_messages_on() {
    let (mut link, w) = ready(true);
    say(&w, "FIRE 3 4");
    assert_eq!(
        link.poll(),
        Some(OpponentEvent::Message(Message::Fire(Coord::new(3, 4))))
    );
}

#[test]
fn pings_on_the_first_poll() {
    let (mut link, w) = ready(true);
    assert_eq!(link.poll(), None);
    assert_eq!(take_sent(&w), vec!["PING 0"]);
}

#[test]
fn pings_again_once_the_interval_has_passed() {
    let (mut link, w) = ready(true);
    link.poll();
    w.borrow_mut().now = 0.5;
    link.poll();
    w.borrow_mut().now = 1.0;
    link.poll();
    assert_eq!(take_sent(&w), vec!["PING 0", "PING 1"]);
}

#[test]
fn answers_pings_without_passing_them_on() {
    let (mut link, w) = ready(false);
    link.poll();
    take_sent(&w);
    say(&w, "PING 7");
    assert_eq!(link.poll(), None);
    assert_eq!(take_sent(&w), vec!["PONG 7"]);
}

#[test]
fn latency_is_unknown_until_a_pong_arrives() {
    let (mut link, _) = ready(true);
    link.poll();
    assert_eq!(link.latency(), None);
}

#[test]
fn a_pong_measures_the_round_trip() {
    let (mut link, w) = ready(true);
    link.poll();
    w.borrow_mut().now = 0.25;
    say(&w, "PONG 0");
    assert_eq!(link.poll(), None);
    assert_eq!(link.latency(), Some(Duration::from_millis(250)));
}

#[test]
fn a_pong_for_an_older_ping_is_ignored() {
    let (mut link, w) = ready(true);
    link.poll();
    say(&w, "PONG 5");
    link.poll();
    assert_eq!(link.latency(), None);
}

#[test]
fn bye_is_reported_as_the_opponent_leaving() {
    let (mut link, w) = ready(true);
    say(&w, "BYE");
    assert_eq!(
        link.poll(),
        Some(OpponentEvent::Disconnected(
            "Your opponent left the game".into()
        ))
    );
    assert!(w.borrow().closed_by_link);
}

#[test]
fn a_closed_socket_is_reported_once() {
    let (mut link, w) = ready(true);
    w.borrow_mut().state = SocketState::Closed;
    assert_eq!(
        link.poll(),
        Some(OpponentEvent::Disconnected(
            "Lost the connection to your opponent".into()
        ))
    );
    assert_eq!(link.poll(), None);
}

#[test]
fn messages_received_before_the_close_are_still_delivered() {
    let (mut link, w) = ready(true);
    say(&w, "READY");
    w.borrow_mut().state = SocketState::Closed;
    assert_eq!(link.poll(), Some(OpponentEvent::Message(Message::Ready)));
    assert!(matches!(link.poll(), Some(OpponentEvent::Disconnected(_))));
}

#[test]
fn a_garbled_message_ends_the_game_with_a_protocol_error() {
    let (mut link, w) = ready(true);
    say(&w, "NONSENSE");
    let Some(OpponentEvent::Disconnected(why)) = link.poll() else {
        panic!("expected a disconnect");
    };
    assert!(why.starts_with("protocol error"), "{why}");
}

#[test]
fn nothing_is_sent_after_a_disconnect() {
    let (mut link, w) = ready(true);
    say(&w, "BYE");
    link.poll();
    take_sent(&w);
    link.send(Message::Ready);
    assert!(take_sent(&w).is_empty());
}

#[test]
fn dropping_a_connected_link_says_goodbye() {
    let (link, w) = ready(true);
    drop(link);
    assert_eq!(take_sent(&w), vec!["BYE"]);
    assert!(w.borrow().closed_by_link);
}

#[test]
fn dropping_a_link_in_the_lobby_just_closes_it() {
    let w = wire(SocketState::Open);
    let mut link = RelayLink::host(Fake(w.clone()));
    link.progress();
    take_sent(&w);
    drop(link);
    assert!(take_sent(&w).is_empty());
    assert!(w.borrow().closed_by_link);
}
