use super::*;
use crate::domain::{Coord, ShotResult};
use std::time::Instant;

fn wait_for<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
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

#[test]
fn parses_addresses() {
    assert_eq!(
        parse_address("192.168.1.20"),
        Ok("192.168.1.20:7777".into())
    );
    assert_eq!(parse_address(" 10.0.0.5:9000 "), Ok("10.0.0.5:9000".into()));
    assert_eq!(parse_address("localhost"), Ok("localhost:7777".into()));
    assert_eq!(parse_address("example.com:80"), Ok("example.com:80".into()));
    assert_eq!(parse_address("[::1]"), Ok("[::1]:7777".into()));
    assert_eq!(parse_address("::1"), Ok("[::1]:7777".into()));
    assert_eq!(parse_address("[::1]:8000"), Ok("[::1]:8000".into()));
    assert!(parse_address("").is_err());
    assert!(parse_address("1.2.3.4:").is_err());
    assert!(parse_address("1.2.3.4:99999").is_err());
    assert!(parse_address("1.2.3.4:abc").is_err());
    assert!(parse_address("has space:80").is_err());
    assert!(parse_address("[::1").is_err());
    assert!(parse_address("[::1]8000").is_err());
    assert_eq!(parse_address(":80"), Err("\":80\" has no host".into()));
    assert_eq!(parse_address("[]"), Err("\"[]\" has no host".into()));
}

#[test]
fn local_addresses_are_not_loopback() {
    assert!(local_ip_addresses().iter().all(|ip| !ip.is_loopback()));
}

#[test]
fn host_and_guest_exchange_messages_both_ways() {
    let (mut host, mut guest) = connected_pair();
    guest.send(Message::Fire(Coord::new(3, 4)));
    assert_eq!(
        wait_for(|| host.poll()),
        OpponentEvent::Message(Message::Fire(Coord::new(3, 4)))
    );
    host.send(Message::Result(ShotResult::Hit));
    host.send(Message::Ready);
    assert_eq!(
        wait_for(|| guest.poll()),
        OpponentEvent::Message(Message::Result(ShotResult::Hit))
    );
    assert_eq!(
        wait_for(|| guest.poll()),
        OpponentEvent::Message(Message::Ready)
    );
    assert_eq!(guest.poll(), None);
}

#[test]
fn handshake_is_not_passed_on_to_the_game() {
    let (mut host, mut guest) = connected_pair();
    thread::sleep(Duration::from_millis(50));
    assert_eq!(host.poll(), None);
    assert_eq!(guest.poll(), None);
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
fn disconnect_is_reported_once_and_sending_afterwards_is_harmless() {
    let (host, mut guest) = connected_pair();
    drop(host);
    wait_for(|| guest.poll());
    guest.send(Message::Ready);
    thread::sleep(Duration::from_millis(50));
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
fn host_ignores_strangers_and_keeps_waiting() {
    let mut host = Host::start(0).unwrap();
    let mut stranger = TcpStream::connect(("127.0.0.1", host.port())).unwrap();
    stranger.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    let mut wrong_version = TcpStream::connect(("127.0.0.1", host.port())).unwrap();
    wrong_version.write_all(b"HELLO BATTLESHIP 999\n").unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(host.poll().is_none());

    let mut joining = join(&format!("127.0.0.1:{}", host.port()));
    assert!(wait_for(|| joining.poll()).is_ok());
    assert!(wait_for(|| host.poll()).peer().ip().is_loopback());
}

#[test]
fn guest_rejects_a_host_with_another_protocol_version() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket.write_all(b"HELLO BATTLESHIP 999\n").unwrap();
        thread::sleep(Duration::from_millis(500));
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while Host::start(port).is_err() {
        assert!(Instant::now() < deadline, "port {port} still in use");
        thread::sleep(Duration::from_millis(20));
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
