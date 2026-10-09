//! Peer to peer play over TCP. One player hosts, the other joins with `ip:port`.
//!
//! All blocking work (accepting, connecting, reading) happens on background
//! threads; the game loop only ever polls.

use crate::opponent::{Opponent, OpponentEvent};
use crate::protocol::{Message, PROTOCOL_VERSION};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 7777;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(8);

/// Normalises user input such as `192.168.1.20`, ` 10.0.0.5:9000 ` or `[::1]` to `host:port`.
pub fn parse_address(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter the host's IP address".into());
    }
    if input.contains(char::is_whitespace) {
        return Err(format!("{input:?} is not a valid address"));
    }
    let (host, port) = if let Some(rest) = input.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| format!("{input:?} is missing a closing ]"))?;
        (
            format!("[{host}]"),
            after.strip_prefix(':').or(after.is_empty().then_some("")),
        )
    } else if input.matches(':').count() > 1 {
        input
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| format!("{input:?} is not a valid address"))?;
        (format!("[{input}]"), Some(""))
    } else {
        match input.split_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (input.to_string(), Some("")),
        }
    };
    let port = match port {
        Some("") if !input.ends_with(':') => DEFAULT_PORT,
        Some(port) => port
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| format!("{port:?} is not a valid port"))?,
        None => return Err(format!("{input:?} is not a valid address")),
    };
    if host.is_empty() || host == "[]" {
        return Err(format!("{input:?} has no host"));
    }
    Ok(format!("{host}:{port}"))
}

/// Best effort guess of the addresses other players can use to reach this machine.
pub fn local_ip_addresses() -> Vec<IpAddr> {
    // Connecting a UDP socket sends nothing; it only asks the OS which
    // interface it would route through.
    let probe = |bind: &str, target: &str| -> Option<IpAddr> {
        let socket = std::net::UdpSocket::bind(bind).ok()?;
        socket.connect(target).ok()?;
        let ip = socket.local_addr().ok()?.ip();
        is_shareable(ip).then_some(ip)
    };
    [
        probe("0.0.0.0:0", "8.8.8.8:80"),
        probe("0.0.0.0:0", "192.168.0.1:80"),
        probe("[::]:0", "[2001:4860:4860::8888]:80"),
    ]
    .into_iter()
    .flatten()
    .fold(Vec::new(), |mut found, ip| {
        if !found.contains(&ip) {
            found.push(ip);
        }
        found
    })
}

/// Whether another machine could reach us on `ip`.
fn is_shareable(ip: IpAddr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified()
}

/// Exchanges HELLO with the peer and starts the background reader.
fn handshake(stream: TcpStream) -> Result<NetLink, String> {
    let io_error = |e: io::Error| format!("Connection failed: {e}");
    stream.set_nodelay(true).map_err(io_error)?;
    let peer = stream.peer_addr().map_err(io_error)?;
    let mut writer = stream.try_clone().map_err(io_error)?;
    writeln!(
        writer,
        "{}",
        Message::Hello {
            version: PROTOCOL_VERSION
        }
        .encode()
    )
    .map_err(io_error)?;

    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(io_error)?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    (&mut reader)
        .take(256)
        .read_line(&mut line)
        .map_err(|_| "The other side did not answer like a Battleship game".to_string())?;
    match Message::decode(&line) {
        Ok(Message::Hello { version }) if version == PROTOCOL_VERSION => {}
        Ok(Message::Hello { version }) => {
            return Err(format!(
                "Opponent speaks protocol version {version}, this game speaks version {PROTOCOL_VERSION}"
            ));
        }
        _ => return Err("The other side is not a Battleship game".into()),
    }
    reader.get_ref().set_read_timeout(None).map_err(io_error)?;

    let (events, receiver) = mpsc::channel();
    thread::spawn(move || read_loop(reader, events));
    Ok(NetLink {
        stream: writer,
        events: receiver,
        peer,
        closed: false,
    })
}

fn read_loop(mut reader: BufReader<TcpStream>, events: mpsc::Sender<OpponentEvent>) {
    let mut line = String::new();
    loop {
        line.clear();
        let farewell = match reader.read_line(&mut line) {
            Ok(0) | Err(_) => "Lost the connection to your opponent".to_string(),
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => match Message::decode(&line) {
                Ok(Message::Bye) => "Your opponent left the game".to_string(),
                Ok(message) => {
                    if events.send(OpponentEvent::Message(message)).is_err() {
                        return;
                    }
                    continue;
                }
                Err(error) => error.to_string(),
            },
        };
        let _ = events.send(OpponentEvent::Disconnected(farewell));
        return;
    }
}

/// An established, handshaken connection to the other player.
pub struct NetLink {
    stream: TcpStream,
    events: Receiver<OpponentEvent>,
    peer: SocketAddr,
    closed: bool,
}

impl NetLink {
    pub fn peer(&self) -> SocketAddr {
        self.peer
    }
}

impl Opponent for NetLink {
    fn send(&mut self, message: Message) {
        if self.closed {
            return;
        }
        if writeln!(self.stream, "{}", message.encode()).is_err() {
            // The reader thread notices the shutdown and reports the disconnect.
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn poll(&mut self) -> Option<OpponentEvent> {
        if self.closed {
            return None;
        }
        let event = match self.events.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                OpponentEvent::Disconnected("Lost the connection to your opponent".into())
            }
        };
        if matches!(event, OpponentEvent::Disconnected(_)) {
            self.closed = true;
        }
        Some(event)
    }
}

impl Drop for NetLink {
    fn drop(&mut self) {
        if !self.closed {
            let _ = writeln!(self.stream, "{}", Message::Bye.encode());
        }
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Waits for another player to connect.
pub struct Host {
    port: u16,
    connections: Receiver<NetLink>,
    stop: Arc<AtomicBool>,
}

impl Host {
    /// Listens on all interfaces. Port 0 picks a free port.
    pub fn start(port: u16) -> io::Result<Host> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (connections, receiver) = mpsc::channel();
        let stopped = Arc::clone(&stop);
        thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let connections = connections.clone();
                        thread::spawn(move || {
                            if stream.set_nonblocking(false).is_ok()
                                && let Ok(link) = handshake(stream)
                            {
                                let _ = connections.send(link);
                            }
                        });
                    }
                    Err(_) => thread::sleep(Duration::from_millis(25)),
                }
            }
        });
        Ok(Host {
            port,
            connections: receiver,
            stop,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The opponent, once someone has connected and completed the handshake.
    pub fn poll(&mut self) -> Option<NetLink> {
        self.connections.try_recv().ok()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// An outgoing connection attempt.
pub struct Joining {
    result: Receiver<Result<NetLink, String>>,
}

impl Joining {
    pub fn poll(&mut self) -> Option<Result<NetLink, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Connection attempt failed".into())),
        }
    }
}

pub fn join(address: &str) -> Joining {
    let address = address.to_string();
    let (sender, result) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(connect(&address));
    });
    Joining { result }
}

fn connect(input: &str) -> Result<NetLink, String> {
    let address = parse_address(input)?;
    let candidates: Vec<SocketAddr> = address
        .to_socket_addrs()
        .map_err(|e| format!("Could not find {address}: {e}"))?
        .collect();
    let mut last_error = format!("Could not find {address}");
    for candidate in candidates {
        match TcpStream::connect_timeout(&candidate, CONNECT_TIMEOUT) {
            Ok(stream) => return handshake(stream),
            Err(e) => last_error = format!("Could not connect to {candidate}: {e}"),
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
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
}
