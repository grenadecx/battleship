use super::*;
use crate::domain::{Coord, ShotResult};
use std::time::Instant;

const TIMEOUT: Duration = Duration::from_secs(10);

fn wait_for<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

fn connected_pair() -> (NetLink, NetLink) {
    let mut host = Host::start(0).unwrap();
    let mut joining = join(&format!("127.0.0.1:{}", host.port()));
    let guest = wait_for(|| joining.poll()).expect("join failed");
    let hosted = wait_for(|| host.poll());
    (hosted, guest)
}

/// Connects to the host, sends `greeting` and blocks until the host hangs up.
fn greet_host(host: &Host, greeting: &[u8]) {
    let mut stranger = TcpStream::connect(("127.0.0.1", host.port())).unwrap();
    stranger.set_read_timeout(Some(TIMEOUT)).unwrap();
    stranger.write_all(greeting).unwrap();
    stranger.read_to_end(&mut Vec::new()).unwrap();
}

/// A link hosted by us, with the raw socket of the peer on the other end.
/// The peer has already read our HELLO.
fn link_with_raw_peer() -> (NetLink, BufReader<TcpStream>) {
    let mut host = Host::start(0).unwrap();
    let mut peer = TcpStream::connect(("127.0.0.1", host.port())).unwrap();
    peer.set_read_timeout(Some(TIMEOUT)).unwrap();
    let hello = Message::Hello {
        version: PROTOCOL_VERSION,
    };
    writeln!(peer, "{}", hello.encode()).unwrap();
    let link = wait_for(|| host.poll());
    let mut peer = BufReader::new(peer);
    peer.read_line(&mut String::new()).unwrap();
    (link, peer)
}

/// Lines the peer receives up to and including `last`.
fn lines_until(peer: &mut BufReader<TcpStream>, last: &str) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        assert_ne!(
            peer.read_line(&mut line).unwrap(),
            0,
            "closed before {last}"
        );
        let line = line.trim().to_string();
        lines.push(line.clone());
        if line == last {
            return lines;
        }
    }
}

#[test]
fn parses_valid_addresses() {
    for (input, expected) in [
        ("192.168.1.20", "192.168.1.20:7777"),
        (" 10.0.0.5:9000 ", "10.0.0.5:9000"),
        ("localhost", "localhost:7777"),
        ("example.com:80", "example.com:80"),
        ("[::1]", "[::1]:7777"),
        ("::1", "[::1]:7777"),
        ("[::1]:8000", "[::1]:8000"),
    ] {
        assert_eq!(parse_address(input), Ok(expected.into()), "{input:?}");
    }
}

#[test]
fn rejects_invalid_addresses() {
    for input in [
        "",
        "1.2.3.4:",
        "1.2.3.4:99999",
        "1.2.3.4:abc",
        "has space:80",
        "[::1",
        "[::1]8000",
    ] {
        assert!(parse_address(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn loopback_addresses_are_not_shared() {
    assert!(!is_shareable("127.0.0.1".parse().unwrap()));
    assert!(!is_shareable("::1".parse().unwrap()));
}

#[test]
fn unspecified_addresses_are_not_shared() {
    assert!(!is_shareable("0.0.0.0".parse().unwrap()));
    assert!(!is_shareable("::".parse().unwrap()));
}

#[test]
fn lan_addresses_are_shared() {
    assert!(is_shareable("192.168.1.20".parse().unwrap()));
}

#[test]
fn host_receives_what_the_guest_sends() {
    let (mut host, mut guest) = connected_pair();
    guest.send(Message::Fire(Coord::new(3, 4)));
    assert_eq!(
        wait_for(|| host.poll()),
        OpponentEvent::Message(Message::Fire(Coord::new(3, 4)))
    );
}

#[test]
fn guest_receives_what_the_host_sends_in_order() {
    let (mut host, mut guest) = connected_pair();
    host.send(Message::Result(ShotResult::Hit));
    host.send(Message::Ready);
    assert_eq!(
        [wait_for(|| guest.poll()), wait_for(|| guest.poll())],
        [
            OpponentEvent::Message(Message::Result(ShotResult::Hit)),
            OpponentEvent::Message(Message::Ready)
        ]
    );
}

#[test]
fn host_does_not_see_the_handshake() {
    let (mut host, mut guest) = connected_pair();
    guest.send(Message::Ready);
    assert_eq!(
        wait_for(|| host.poll()),
        OpponentEvent::Message(Message::Ready)
    );
}

#[test]
fn guest_does_not_see_the_handshake() {
    let (mut host, mut guest) = connected_pair();
    host.send(Message::Ready);
    assert_eq!(
        wait_for(|| guest.poll()),
        OpponentEvent::Message(Message::Ready)
    );
}

#[test]
fn latency_is_unknown_right_after_connecting() {
    let (_host, guest) = connected_pair();
    assert_eq!(guest.latency(), None);
}

#[test]
fn latency_is_measured_once_the_peer_answers_a_ping() {
    let (_host, mut guest) = connected_pair();
    wait_for(|| {
        guest.poll();
        guest.latency()
    });
}

#[test]
fn pings_are_not_passed_on_to_the_game() {
    let (mut host, mut guest) = connected_pair();
    host.poll();
    host.send(Message::Ready);
    assert_eq!(
        wait_for(|| guest.poll()),
        OpponentEvent::Message(Message::Ready)
    );
}

#[test]
fn pongs_are_not_passed_on_to_the_game() {
    let (_host, mut guest) = connected_pair();
    wait_for(|| match guest.poll() {
        Some(event) => panic!("unexpected {event:?}"),
        None => guest.latency(),
    });
}

#[test]
fn leaving_is_reported_as_a_disconnect() {
    let (host, mut guest) = connected_pair();
    drop(host);
    assert!(matches!(
        wait_for(|| guest.poll()),
        OpponentEvent::Disconnected(_)
    ));
}

#[test]
fn disconnect_is_reported_only_once() {
    let (host, mut guest) = connected_pair();
    drop(host);
    wait_for(|| guest.poll());
    assert_eq!(guest.poll(), None);
}

#[test]
fn sending_after_a_disconnect_is_harmless() {
    let (host, mut guest) = connected_pair();
    drop(host);
    wait_for(|| guest.poll());
    guest.send(Message::Ready);
    assert_eq!(guest.poll(), None);
}

#[test]
fn joining_a_closed_port_fails() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let mut joining = join(&format!("127.0.0.1:{port}"));
    assert!(wait_for(|| joining.poll()).is_err());
}

#[test]
fn joining_an_unparseable_address_fails() {
    let mut joining = join("not an address");
    assert!(wait_for(|| joining.poll()).is_err());
}

#[test]
fn host_turns_away_a_client_that_is_not_a_battleship_game() {
    let mut host = Host::start(0).unwrap();
    greet_host(&host, b"GET / HTTP/1.1\r\n\r\n");
    assert!(host.poll().is_none());
}

#[test]
fn host_turns_away_a_client_with_another_protocol_version() {
    let mut host = Host::start(0).unwrap();
    greet_host(&host, b"HELLO BATTLESHIP 999\n");
    assert!(host.poll().is_none());
}

#[test]
fn host_keeps_waiting_after_turning_a_stranger_away() {
    let host = Host::start(0).unwrap();
    greet_host(&host, b"GET / HTTP/1.1\r\n\r\n");
    let mut joining = join(&format!("127.0.0.1:{}", host.port()));
    assert!(wait_for(|| joining.poll()).is_ok());
}

#[test]
fn guest_rejects_a_host_with_another_protocol_version() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.write_all(b"HELLO BATTLESHIP 999\n").unwrap();
        let _ = socket.read_to_end(&mut Vec::new());
    });
    let mut joining = join(&format!("127.0.0.1:{port}"));
    let error = wait_for(|| joining.poll())
        .err()
        .expect("should be rejected");
    assert!(error.contains("version"), "{error}");
}

#[test]
fn stopping_the_host_frees_the_port() {
    let host = Host::start(0).unwrap();
    let port = host.port();
    drop(host);
    let deadline = Instant::now() + TIMEOUT;
    while Host::start(port).is_err() {
        assert!(Instant::now() < deadline, "port {port} still in use");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn addresses_need_a_host() {
    for input in [":80", "[]"] {
        assert_eq!(parse_address(input), Err(format!("{input:?} has no host")));
    }
}

#[test]
fn sending_to_a_vanished_peer_reports_the_disconnect() {
    let (host, mut guest) = connected_pair();
    drop(host);
    thread::sleep(Duration::from_millis(50));
    // The first write after the peer closed may still be accepted; later
    // ones fail, and the link shuts itself down.
    for _ in 0..10 {
        guest.send(Message::Ready);
    }
    assert!(matches!(
        wait_for(|| guest.poll()),
        OpponentEvent::Disconnected(_)
    ));
}

#[test]
fn local_addresses_are_shareable() {
    assert!(local_ip_addresses().into_iter().all(is_shareable));
}

#[test]
fn each_end_knows_the_address_of_the_other() {
    let (host, guest) = connected_pair();
    assert!(host.peer().ip().is_loopback());
    assert!(guest.peer().ip().is_loopback());
}

#[test]
fn pings_again_once_the_interval_has_passed() {
    let (_host, mut guest) = connected_pair();
    wait_for(|| {
        guest.poll();
        guest.ping.filter(|(id, _)| *id > 0)
    });
}

#[test]
fn a_garbled_message_ends_the_game_with_a_protocol_error() {
    let mut host = Host::start(0).unwrap();
    let mut peer = TcpStream::connect(("127.0.0.1", host.port())).unwrap();
    let hello = Message::Hello {
        version: PROTOCOL_VERSION,
    };
    writeln!(peer, "{}\nNONSENSE", hello.encode()).unwrap();
    let mut link = wait_for(|| host.poll());
    let OpponentEvent::Disconnected(why) = wait_for(|| link.poll()) else {
        panic!("expected a disconnect");
    };
    assert!(why.starts_with("protocol error"), "{why}");
}

#[test]
fn blank_lines_from_the_peer_are_ignored() {
    let (mut link, mut peer) = link_with_raw_peer();
    peer.get_mut().write_all(b"\nREADY\n").unwrap();
    assert_eq!(
        wait_for(|| link.poll()),
        OpponentEvent::Message(Message::Ready)
    );
}

#[test]
fn pings_at_most_once_per_interval() {
    let (mut link, mut peer) = link_with_raw_peer();
    link.poll();
    link.poll();
    link.send(Message::Ready);
    let pings = lines_until(&mut peer, "READY")
        .iter()
        .filter(|line| line.starts_with("PING"))
        .count();
    assert_eq!(pings, 1);
}

#[test]
fn leaving_says_goodbye() {
    let (link, mut peer) = link_with_raw_peer();
    drop(link);
    lines_until(&mut peer, "BYE");
}

#[test]
fn leaving_closes_the_connection() {
    let (link, mut peer) = link_with_raw_peer();
    drop(link);
    peer.read_to_end(&mut Vec::new())
        .expect("connection should close");
}
