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
