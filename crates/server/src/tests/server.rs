use super::*;
use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode, header};
use battleship_core::opponent::{Opponent, OpponentEvent};
use battleship_core::protocol::Message as GameMessage;
use battleship_core::relay::CODE_LENGTH;
use battleship_core::relay_link::{Progress, RelayLink, SocketState, Transport};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Instant;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use tower::ServiceExt;

const TIMEOUT: Duration = Duration::from_secs(5);

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn rooms(capacity: usize) -> Arc<Rooms> {
    Arc::new(Rooms::new(capacity, Rng::new(7)))
}

/// A web folder with a page and a wasm file, unique to the calling test.
fn web_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "battleship-server-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>Battleship</title>",
    )
    .unwrap();
    std::fs::write(dir.join("battleship.wasm"), b"\0asm\x01\0\0\0").unwrap();
    dir
}

/// Starts the service on a free local port.
async fn start(rooms: Arc<Rooms>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app_with_rooms(web_dir(), rooms);
    tokio::spawn(async move { axum::serve(listener, app).await });
    addr
}

async fn connect(addr: SocketAddr) -> Client {
    connect_async(format!("ws://{addr}/ws")).await.unwrap().0
}

async fn send(client: &mut Client, text: &str) {
    client.send(WsMessage::Text(text.into())).await.unwrap();
}

/// The next text frame, or `None` once the server closed the connection.
async fn recv(client: &mut Client) -> Option<String> {
    let next = async {
        loop {
            match client.next().await {
                Some(Ok(WsMessage::Text(text))) => return Some(text.to_string()),
                Some(Ok(WsMessage::Close(_)) | Err(_)) | None => return None,
                Some(Ok(_)) => {}
            }
        }
    };
    tokio::time::timeout(TIMEOUT, next)
        .await
        .expect("timed out waiting for the server")
}

/// A host with an open room, and the room's code.
async fn hosted(addr: SocketAddr) -> (Client, String) {
    let mut host = connect(addr).await;
    send(&mut host, "HOST").await;
    let reply = recv(&mut host).await.unwrap();
    let Ok(Reply::Room(code)) = Reply::decode(&reply) else {
        panic!("expected a room, got {reply:?}");
    };
    (host, code)
}

/// A host and a guest that the server has paired.
async fn paired(addr: SocketAddr) -> (Client, Client) {
    let (mut host, code) = hosted(addr).await;
    let mut guest = connect(addr).await;
    send(&mut guest, &format!("JOIN {code}")).await;
    assert_eq!(recv(&mut guest).await.as_deref(), Some("PAIRED"));
    assert_eq!(recv(&mut host).await.as_deref(), Some("PAIRED"));
    (host, guest)
}

async fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn peer() -> (Peer, mpsc::UnboundedReceiver<Out>) {
    mpsc::unbounded_channel()
}

fn paired_text() -> Out {
    Out::Text("PAIRED".into())
}

// --- Rooms -----------------------------------------------------------------

#[test]
fn opening_a_room_gives_it_a_code() {
    let rooms = rooms(10);
    let (code, _) = rooms.open(peer().0).unwrap();
    assert_eq!(code.len(), CODE_LENGTH);
    assert_eq!(rooms.len(), 1);
}

#[test]
fn every_open_room_has_its_own_code() {
    let rooms = rooms(500);
    let mut keep = Vec::new();
    for _ in 0..500 {
        keep.push(rooms.open(peer().0).unwrap());
    }
    assert_eq!(rooms.len(), 500);
}

#[test]
fn joining_hands_the_guest_to_the_host_and_tells_both() {
    let rooms = rooms(10);
    let (host, mut host_outbox) = peer();
    let (guest, mut guest_outbox) = peer();
    let (code, mut arrives) = rooms.open(host).unwrap();
    rooms.join(&code, guest).unwrap();
    assert!(arrives.try_recv().is_ok());
    assert_eq!(host_outbox.try_recv(), Ok(paired_text()));
    assert_eq!(guest_outbox.try_recv(), Ok(paired_text()));
    assert!(rooms.is_empty());
}

#[test]
fn joining_returns_the_host() {
    let rooms = rooms(10);
    let (host, mut host_outbox) = peer();
    let (code, _arrives) = rooms.open(host).unwrap();
    let found = rooms.join(&code, peer().0).unwrap();
    found.send(Out::Text("READY".into())).unwrap();
    host_outbox.try_recv().unwrap();
    assert_eq!(host_outbox.try_recv(), Ok(Out::Text("READY".into())));
}

#[test]
fn joining_an_unknown_room_fails() {
    assert_eq!(
        rooms(10).join("ZZZZ", peer().0).unwrap_err(),
        "No game with code ZZZZ"
    );
}

#[test]
fn a_room_is_joined_only_once() {
    let rooms = rooms(10);
    let (code, _arrives) = rooms.open(peer().0).unwrap();
    rooms.join(&code, peer().0).unwrap();
    assert!(rooms.join(&code, peer().0).is_err());
}

#[test]
fn joining_a_room_whose_host_just_left_fails() {
    let rooms = rooms(10);
    let (code, arrives) = rooms.open(peer().0).unwrap();
    drop(arrives);
    assert!(rooms.join(&code, peer().0).is_err());
    assert!(rooms.is_empty());
}

#[test]
fn closing_a_room_removes_it() {
    let rooms = rooms(10);
    let (code, _arrives) = rooms.open(peer().0).unwrap();
    rooms.close(&code);
    assert!(rooms.is_empty());
    assert!(rooms.join(&code, peer().0).is_err());
}

#[test]
fn a_full_server_opens_no_more_rooms() {
    let rooms = rooms(2);
    let _first = rooms.open(peer().0).unwrap();
    let _second = rooms.open(peer().0).unwrap();
    assert!(rooms.open(peer().0).is_none());
}

// --- HTTP ------------------------------------------------------------------

async fn get(path: &str) -> axum::response::Response {
    let request = HttpRequest::get(path).body(Body::empty()).unwrap();
    app_with_rooms(web_dir(), rooms(1))
        .oneshot(request)
        .await
        .unwrap()
}

#[tokio::test]
async fn serves_the_page() {
    let response = get("/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test]
async fn serves_wasm_with_the_type_browsers_require() {
    let response = get("/battleship.wasm").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/wasm");
}

#[tokio::test]
async fn unknown_files_are_not_found() {
    assert_eq!(get("/nope.js").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reports_health() {
    assert_eq!(get("/healthz").await.status(), StatusCode::OK);
}

#[tokio::test]
async fn the_default_service_has_the_same_routes() {
    let request = HttpRequest::get("/healthz").body(Body::empty()).unwrap();
    let response = app(web_dir()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// --- Relay -----------------------------------------------------------------

#[tokio::test]
async fn hosting_opens_a_room() {
    let addr = start(rooms(10)).await;
    let (_host, code) = hosted(addr).await;
    assert_eq!(code.len(), CODE_LENGTH);
}

#[tokio::test]
async fn a_guest_with_the_code_is_paired_with_the_host() {
    let addr = start(rooms(10)).await;
    paired(addr).await;
}

#[tokio::test]
async fn codes_are_accepted_in_lower_case() {
    let addr = start(rooms(10)).await;
    let (_host, code) = hosted(addr).await;
    let mut guest = connect(addr).await;
    send(&mut guest, &format!("JOIN {}", code.to_lowercase())).await;
    assert_eq!(recv(&mut guest).await.as_deref(), Some("PAIRED"));
}

#[tokio::test]
async fn paired_players_reach_each_other_both_ways() {
    let addr = start(rooms(10)).await;
    let (mut host, mut guest) = paired(addr).await;
    send(&mut host, "HELLO BATTLESHIP 2").await;
    assert_eq!(
        recv(&mut guest).await.as_deref(),
        Some("HELLO BATTLESHIP 2")
    );
    send(&mut guest, "FIRE 3 4").await;
    assert_eq!(recv(&mut host).await.as_deref(), Some("FIRE 3 4"));
}

#[tokio::test]
async fn frames_keep_their_order() {
    let addr = start(rooms(10)).await;
    let (mut host, mut guest) = paired(addr).await;
    for i in 0..20 {
        send(&mut host, &format!("PING {i}")).await;
    }
    for i in 0..20 {
        assert_eq!(recv(&mut guest).await, Some(format!("PING {i}")));
    }
}

#[tokio::test]
async fn non_text_frames_before_the_request_are_ignored() {
    let addr = start(rooms(10)).await;
    let mut host = connect(addr).await;
    host.send(WsMessage::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    host.send(WsMessage::Ping(vec![].into())).await.unwrap();
    send(&mut host, "HOST").await;
    assert!(recv(&mut host).await.unwrap().starts_with("ROOM "));
}

#[tokio::test]
async fn non_text_frames_are_not_passed_on() {
    let addr = start(rooms(10)).await;
    let (mut host, mut guest) = paired(addr).await;
    host.send(WsMessage::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    send(&mut host, "READY").await;
    assert_eq!(recv(&mut guest).await.as_deref(), Some("READY"));
}

#[tokio::test]
async fn frames_sent_before_the_guest_arrives_are_dropped() {
    let addr = start(rooms(10)).await;
    let (mut host, code) = hosted(addr).await;
    send(&mut host, "READY").await;
    let mut guest = connect(addr).await;
    send(&mut guest, &format!("JOIN {code}")).await;
    assert_eq!(recv(&mut guest).await.as_deref(), Some("PAIRED"));
    send(&mut host, "FIRE 0 0").await;
    assert_eq!(recv(&mut guest).await.as_deref(), Some("FIRE 0 0"));
}

#[tokio::test]
async fn joining_an_unknown_room_is_refused_and_closed() {
    let addr = start(rooms(10)).await;
    let mut guest = connect(addr).await;
    send(&mut guest, "JOIN ZZZZ").await;
    assert_eq!(
        recv(&mut guest).await.as_deref(),
        Some("ERROR No game with code ZZZZ")
    );
    assert_eq!(recv(&mut guest).await, None);
}

#[tokio::test]
async fn anything_but_host_or_join_is_refused() {
    let addr = start(rooms(10)).await;
    let mut stranger = connect(addr).await;
    send(&mut stranger, "HELLO BATTLESHIP 2").await;
    assert_eq!(
        recv(&mut stranger).await.as_deref(),
        Some("ERROR Expected HOST or JOIN")
    );
    assert_eq!(recv(&mut stranger).await, None);
}

#[tokio::test]
async fn a_room_closes_when_its_host_leaves() {
    let rooms = rooms(10);
    let addr = start(rooms.clone()).await;
    let (mut host, code) = hosted(addr).await;
    host.close(None).await.unwrap();
    wait_until(|| rooms.is_empty()).await;
    let mut guest = connect(addr).await;
    send(&mut guest, &format!("JOIN {code}")).await;
    assert!(recv(&mut guest).await.unwrap().starts_with("ERROR"));
}

#[tokio::test]
async fn a_room_can_only_be_joined_once() {
    let addr = start(rooms(10)).await;
    let (mut host, code) = hosted(addr).await;
    let mut guest = connect(addr).await;
    send(&mut guest, &format!("JOIN {code}")).await;
    recv(&mut guest).await;
    recv(&mut host).await;
    let mut late = connect(addr).await;
    send(&mut late, &format!("JOIN {code}")).await;
    assert!(recv(&mut late).await.unwrap().starts_with("ERROR"));
}

#[tokio::test]
async fn a_full_server_turns_new_hosts_away() {
    let addr = start(rooms(1)).await;
    let (_host, _code) = hosted(addr).await;
    let mut second = connect(addr).await;
    send(&mut second, "HOST").await;
    assert_eq!(
        recv(&mut second).await.as_deref(),
        Some("ERROR The server is full, try again later")
    );
}

#[tokio::test]
async fn when_one_player_leaves_the_other_hears_it_and_is_disconnected() {
    let addr = start(rooms(10)).await;
    let (mut host, mut guest) = paired(addr).await;
    send(&mut guest, "BYE").await;
    guest.close(None).await.unwrap();
    assert_eq!(recv(&mut host).await.as_deref(), Some("BYE"));
    assert_eq!(recv(&mut host).await, None);
}

#[tokio::test]
async fn a_lost_connection_disconnects_the_other_player() {
    let addr = start(rooms(10)).await;
    let (mut host, guest) = paired(addr).await;
    drop(guest);
    assert_eq!(recv(&mut host).await, None);
}

// --- The browser's client logic against the real server ----------------------

/// A [`Transport`] over a real WebSocket, as the browser's socket behaves:
/// connecting in the background, queueing messages until they are polled.
struct TestSocket {
    state: Arc<AtomicU8>,
    inbox: std::sync::mpsc::Receiver<String>,
    outbox: mpsc::UnboundedSender<Option<String>>,
    started: Instant,
}

impl TestSocket {
    fn open(addr: SocketAddr) -> Self {
        let state = Arc::new(AtomicU8::new(0));
        let (inbox_tx, inbox) = std::sync::mpsc::channel();
        let (outbox, mut outgoing) = mpsc::unbounded_channel::<Option<String>>();
        let shared = state.clone();
        tokio::spawn(async move {
            let Ok((mut ws, _)) = connect_async(format!("ws://{addr}/ws")).await else {
                shared.store(2, Ordering::SeqCst);
                return;
            };
            shared.store(1, Ordering::SeqCst);
            loop {
                tokio::select! {
                    frame = ws.next() => match frame {
                        Some(Ok(WsMessage::Text(text))) => {
                            let _ = inbox_tx.send(text.to_string());
                        }
                        Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break,
                        Some(Ok(_)) => {}
                    },
                    out = outgoing.recv() => match out {
                        Some(Some(text)) => {
                            if ws.send(WsMessage::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                        Some(None) | None => {
                            let _ = ws.close(None).await;
                            break;
                        }
                    },
                }
            }
            shared.store(2, Ordering::SeqCst);
        });
        TestSocket {
            state,
            inbox,
            outbox,
            started: Instant::now(),
        }
    }
}

impl Transport for TestSocket {
    fn state(&self) -> SocketState {
        match self.state.load(Ordering::SeqCst) {
            0 => SocketState::Connecting,
            1 => SocketState::Open,
            _ => SocketState::Closed,
        }
    }

    fn send(&mut self, text: &str) {
        let _ = self.outbox.send(Some(text.to_string()));
    }

    fn recv(&mut self) -> Option<String> {
        self.inbox.try_recv().ok()
    }

    fn close(&mut self) {
        let _ = self.outbox.send(None);
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

/// Polls `poll` every few milliseconds until it returns something.
async fn eventually<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn is_ready(link: &mut RelayLink<TestSocket>) -> bool {
    match link.progress() {
        Progress::Waiting => false,
        Progress::Ready => true,
        Progress::Failed(e) => panic!("{e}"),
    }
}

/// Host and guest links, connected through the server like two browsers.
async fn linked(addr: SocketAddr) -> (RelayLink<TestSocket>, RelayLink<TestSocket>) {
    let mut host = RelayLink::host(TestSocket::open(addr));
    let code = eventually(|| {
        host.progress();
        host.room().map(str::to_string)
    })
    .await;
    let mut guest = RelayLink::join(TestSocket::open(addr), &code);
    // Like two browsers, both sides make progress every frame.
    eventually(|| (is_ready(&mut host) & is_ready(&mut guest)).then_some(())).await;
    (host, guest)
}

#[tokio::test]
async fn browser_links_meet_through_the_server() {
    let addr = start(rooms(10)).await;
    let (host, guest) = linked(addr).await;
    assert!(host.i_go_first());
    assert!(!guest.i_go_first());
}

#[tokio::test]
async fn browser_links_exchange_game_messages() {
    let addr = start(rooms(10)).await;
    let (mut host, mut guest) = linked(addr).await;
    host.send(GameMessage::Ready);
    assert_eq!(
        eventually(|| guest.poll()).await,
        OpponentEvent::Message(GameMessage::Ready)
    );
}

#[tokio::test]
async fn browser_links_measure_latency_through_the_server() {
    let addr = start(rooms(10)).await;
    let (mut host, mut guest) = linked(addr).await;
    eventually(|| {
        guest.poll();
        host.poll();
        host.latency()
    })
    .await;
}

#[tokio::test]
async fn a_browser_leaving_is_reported_to_the_other() {
    let addr = start(rooms(10)).await;
    let (host, mut guest) = linked(addr).await;
    drop(host);
    assert_eq!(
        eventually(|| guest.poll()).await,
        OpponentEvent::Disconnected("Your opponent left the game".into())
    );
}

#[tokio::test]
async fn joining_a_room_that_does_not_exist_fails_in_the_browser() {
    let addr = start(rooms(10)).await;
    let mut guest = RelayLink::join(TestSocket::open(addr), "ZZZZ");
    let failure = eventually(|| match guest.progress() {
        Progress::Failed(e) => Some(e),
        _ => None,
    })
    .await;
    assert_eq!(failure, "No game with code ZZZZ");
}
