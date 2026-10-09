//! The relay over real sockets: a whole mem-backed server on a loopback
//! port, a fake board (`lpc-relay`'s own client, driven by a test loop over
//! tokio-tungstenite) and fake browsers.
//!
//! The board echoes every frame a session sends it, so "frames pass
//! byte-identical" is one round trip.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use futures_util::{SinkExt as _, StreamExt as _};
use lp_cloud_server::app_state::AppState;
use lp_cloud_server::config::ServerConfig;
use lp_cloud_server::page::static_site::StaticSite;
use lp_cloud_server::ports::{AnyBlobStore, AnyMetaStore};
use lp_cloud_server::router::build_router;
use lp_cloud_store_mem::{MemBlobStore, MemMetaStore};
use lpc_cloud_api::{
    Actor, BoardList, CLOUD_API_VERSION, CloudCall, CloudReply, CloudRequest, CloudResponse,
};
use lpc_relay::{
    LanAddress, RELAY_PROTO_VERSION, RefuseReason, RelayAccount, RelayAction, RelayClient,
    RelayClientConfig, RelayEvent, RelayFrame, RelayHello, RelayState,
};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const MAC: [u8; 6] = [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30];
const BOARD_ID: &str = "10bda3b08e30";

#[tokio::test]
async fn a_board_registers_and_its_account_lists_it_on_the_same_network() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let board = FakeBoard::spawn(server.port, vec![alice.relay_account()]);
    board.wait_for(RelayState::Connected).await;

    let boards = server.list_boards(&alice.cookie).await;
    assert_eq!(boards.boards.len(), 1);
    let listed = &boards.boards[0];
    assert_eq!(listed.id, BOARD_ID);
    assert_eq!(listed.label, "Lamp");
    assert_eq!(listed.wire_proto, 39);
    assert_eq!(listed.lan.as_deref(), Some("192.168.4.20:80"));
    assert!(listed.same_network, "both legs come from 127.0.0.1");

    let bob = server.account("bob").await;
    assert!(server.list_boards(&bob.cookie).await.boards.is_empty());
}

#[tokio::test]
async fn a_member_session_passes_frames_byte_identical() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let board = FakeBoard::spawn(server.port, vec![alice.relay_account()]);
    board.wait_for(RelayState::Connected).await;

    let mut session = server.browser(BOARD_ID, Some(&alice.cookie)).await;
    let frame = vec![0xa5, 0x00, 0xff, 0x10, 0x7e];
    session.send(Message::Binary(frame.clone())).await.unwrap();
    assert_eq!(next_binary(&mut session).await, frame);
    session.close(None).await.unwrap();
}

#[tokio::test]
async fn a_visitor_session_reaches_the_board_too() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let board = FakeBoard::spawn(server.port, vec![alice.relay_account()]);
    board.wait_for(RelayState::Connected).await;

    let guest = server.guest().await;
    let mut session = server.browser(BOARD_ID, Some(&guest)).await;
    session.send(Message::Binary(vec![1, 2, 3])).await.unwrap();
    assert_eq!(
        next_binary(&mut session).await,
        [1, 2, 3],
        "the relay passes it; what a visitor may do is the board's to decide"
    );
}

#[tokio::test]
async fn no_session_is_closed_sign_in_required_and_an_offline_board_board_offline() {
    let server = Server::start().await;
    let alice = server.account("alice").await;

    let mut nobody = server.browser(BOARD_ID, None).await;
    assert_eq!(close_code(&mut nobody).await, 4401);

    let mut offline = server.browser(BOARD_ID, Some(&alice.cookie)).await;
    assert_eq!(close_code(&mut offline).await, 4404);
}

#[tokio::test]
async fn a_board_that_holds_one_session_closes_the_second_busy() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let board = FakeBoard::spawn(server.port, vec![alice.relay_account()]);
    board.wait_for(RelayState::Connected).await;

    let mut first = server.browser(BOARD_ID, Some(&alice.cookie)).await;
    first.send(Message::Binary(vec![9])).await.unwrap();
    assert_eq!(next_binary(&mut first).await, [9]);
    let mut second = server.browser(BOARD_ID, Some(&alice.cookie)).await;
    assert_eq!(close_code(&mut second).await, 4429);
}

#[tokio::test]
async fn an_unknown_account_is_refused_by_name() {
    let server = Server::start().await;
    let board = FakeBoard::spawn(
        server.port,
        vec![RelayAccount {
            salt: [0xee; 16],
            k: [0xee; 32],
        }],
    );
    board
        .wait_for(RelayState::Refused {
            reason: RefuseReason::UnknownAccount,
        })
        .await;
}

#[tokio::test]
async fn a_version_the_hub_does_not_list_is_refused_by_name() {
    let server = Server::start().await;
    let mut ws = server.device_socket().await;
    let mut hello = RelayHello::new(MAC, "Lamp", 39, None, vec![[1; 16]]);
    hello.relay_proto = RELAY_PROTO_VERSION + 1;
    ws.send(Message::Binary(RelayFrame::Hello(hello).encode()))
        .await
        .unwrap();
    assert_eq!(
        RelayFrame::decode(&next_binary(&mut ws).await).unwrap(),
        RelayFrame::Refused {
            reason: RefuseReason::VersionTooNew,
            retry_after_s: 0
        }
    );
}

#[tokio::test]
async fn going_away_sends_boards_to_the_short_backoff_and_ends_sessions() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let board = FakeBoard::spawn(server.port, vec![alice.relay_account()]);
    board.wait_for(RelayState::Connected).await;
    let mut session = server.browser(BOARD_ID, Some(&alice.cookie)).await;
    session.send(Message::Binary(vec![1])).await.unwrap();
    next_binary(&mut session).await;

    server.state.relay().going_away();
    assert_eq!(close_code(&mut session).await, 1001);
    board.wait_for(RelayState::Connecting).await;
    let wait = board.next_wake_in().await;
    assert!(
        (Duration::from_millis(1_500)..=Duration::from_secs(12)).contains(&wait),
        "the 2–12 s going-away backoff: {wait:?}"
    );
}

/// R5: a deploy's reconnect storm. After a deploy every board dials the new
/// machine inside one jitter window; here fifty boards (five accounts of
/// ten) dial at the same instant — the worst case, no jitter at all — and
/// `/api` keeps answering in under a second while they register. Each
/// registration touches the store once; nothing else on the relay's path
/// does. (The jitter itself is `lpc-relay`'s, tested there; a going-away
/// leg closes for good on this process, which is the one being replaced.)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reconnect_storm_of_fifty_boards_does_not_starve_the_api() {
    let server = Server::start().await;
    let mut accounts = Vec::new();
    for n in 0..5 {
        accounts.push(server.account(&format!("owner{n}")).await);
    }
    let storm = Instant::now();
    let mut boards = Vec::new();
    for n in 0..50u8 {
        let account = &accounts[usize::from(n) % accounts.len()];
        boards.push(FakeBoard::spawn_as(
            server.port,
            [0x02, 0, 0, 0, 0, n],
            vec![account.relay_account()],
        ));
    }
    let mut slowest = Duration::ZERO;
    let mut calls = 0;
    while server.state.relay().board_count() < 50 {
        assert!(
            storm.elapsed() < Duration::from_secs(20),
            "only {} boards registered",
            server.state.relay().board_count()
        );
        let asked = Instant::now();
        server.list_boards(&accounts[0].cookie).await;
        slowest = slowest.max(asked.elapsed());
        calls += 1;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    println!(
        "relay storm: 50 boards registered in {} ms; slowest of {calls} /api calls meanwhile {} ms",
        storm.elapsed().as_millis(),
        slowest.as_millis()
    );
    assert!(calls > 0, "the storm finished before /api was asked");
    assert!(slowest < Duration::from_secs(1), "{slowest:?}");
    for board in &boards {
        board.wait_for(RelayState::Connected).await;
    }
    assert_eq!(
        server.list_boards(&accounts[0].cookie).await.boards.len(),
        10
    );
}

// ---- harness ---------------------------------------------------------

struct Server {
    state: AppState,
    port: u16,
}

struct Account {
    cookie: String,
    k: [u8; 32],
    salt: [u8; 16],
}

impl Account {
    fn relay_account(&self) -> RelayAccount {
        RelayAccount {
            salt: self.salt,
            k: self.k,
        }
    }
}

impl Server {
    async fn start() -> Self {
        let config = ServerConfig::from_vars(|name| match name {
            "LP_CLOUD_STORE" | "LP_CLOUD_BLOBS" => Some("mem".to_string()),
            _ => None,
        })
        .expect("the test configuration parses");
        let state = AppState::new(
            config,
            AnyMetaStore::new(MemMetaStore::new()),
            AnyBlobStore::new(MemBlobStore::new()),
            StaticSite::open(None),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = build_router(state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { state, port }
    }

    /// A signed-in account with its account key minted.
    async fn account(&self, name: &str) -> Account {
        let name = name.to_string();
        let (token, info) = self
            .state
            .with_service(move |core| {
                let email = format!("{name}@example.com");
                let user = core
                    .service
                    .upsert_user(&name, &email, &name, "google", None, None, None);
                let token = core.service.open_session(user.uid, 3600.0, None);
                let info = match core
                    .service
                    .handle(Actor::User(user.uid), CloudRequest::GetAccountAccess)
                    .unwrap()
                {
                    CloudResponse::AccountAccessInfo(info) => info,
                    other => panic!("{other:?}"),
                };
                (token, info)
            })
            .await;
        Account {
            cookie: cookie(&token),
            k: lpc_access::derive_login_key(&info.key_secret, &info.key_salt, 1),
            salt: info.key_salt,
        }
    }

    /// A guest session's cookie.
    async fn guest(&self) -> String {
        let token = self
            .state
            .with_service(|core| {
                let user = core.service.begin_guest_user();
                core.service.open_session(user.uid, 3600.0, None)
            })
            .await;
        cookie(&token)
    }

    async fn list_boards(&self, cookie: &str) -> BoardList {
        let reply: CloudReply = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/api", self.port))
            .header("cookie", cookie)
            .json(&CloudCall {
                version: CLOUD_API_VERSION,
                request: CloudRequest::ListBoards,
            })
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        match reply.result.unwrap() {
            CloudResponse::BoardList(list) => list,
            other => panic!("{other:?}"),
        }
    }

    async fn browser(&self, id: &str, cookie: Option<&str>) -> Ws {
        let mut request = format!("ws://127.0.0.1:{}/relay/board/{id}", self.port)
            .into_client_request()
            .unwrap();
        if let Some(cookie) = cookie {
            request
                .headers_mut()
                .insert("cookie", cookie.parse().unwrap());
        }
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    async fn device_socket(&self) -> Ws {
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/relay/device", self.port))
            .await
            .unwrap()
            .0
    }
}

fn cookie(token: &[u8]) -> String {
    format!("lp_session={}", URL_SAFE_NO_PAD.encode(token))
}

async fn next_binary(ws: &mut Ws) -> Vec<u8> {
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            message = ws.next() => match message {
                Some(Ok(Message::Binary(bytes))) => return bytes,
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                other => panic!("expected a binary message, got {other:?}"),
            },
            () = &mut deadline => panic!("no binary message within 5 s"),
        }
    }
}

async fn close_code(ws: &mut Ws) -> u16 {
    let deadline = tokio::time::sleep(Duration::from_secs(5));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            message = ws.next() => match message {
                Some(Ok(Message::Close(Some(frame)))) => return u16::from(frame.code),
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))) => {}
                other => panic!("expected a close frame, got {other:?}"),
            },
            () = &mut deadline => panic!("no close within 5 s"),
        }
    }
}

/// A board on the relay: `lpc-relay`'s client, driven by a task over
/// tokio-tungstenite, echoing every session frame back.
struct FakeBoard {
    shared: Arc<Mutex<BoardShared>>,
}

struct BoardShared {
    state: RelayState,
    next_wake_in: Option<Duration>,
}

impl FakeBoard {
    fn spawn(port: u16, accounts: Vec<RelayAccount>) -> Self {
        Self::spawn_as(port, MAC, accounts)
    }

    fn spawn_as(port: u16, mac: [u8; 6], accounts: Vec<RelayAccount>) -> Self {
        let shared = Arc::new(Mutex::new(BoardShared {
            state: RelayState::Off,
            next_wake_in: None,
        }));
        tokio::spawn(drive_board(port, mac, accounts, Arc::clone(&shared)));
        Self { shared }
    }

    async fn wait_for(&self, wanted: RelayState) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if self.shared.lock().unwrap().state == wanted {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "board never reached {wanted}; it is {}",
                self.shared.lock().unwrap().state
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn next_wake_in(&self) -> Duration {
        self.shared
            .lock()
            .unwrap()
            .next_wake_in
            .expect("a wake is set")
    }
}

async fn drive_board(
    port: u16,
    mac: [u8; 6],
    accounts: Vec<RelayAccount>,
    shared: Arc<Mutex<BoardShared>>,
) {
    let started = Instant::now();
    let now = || started.elapsed().as_millis() as u64;
    let mut client = RelayClient::new(
        RelayClientConfig {
            host: "127.0.0.1".into(),
            port,
            board_mac: mac,
            label: "Lamp".into(),
            wire_proto: 39,
            max_routes: 1,
            firmware: "fake-board-1".into(),
        },
        |bytes| bytes.fill(7),
    );
    let mut ws: Option<Ws> = None;
    let mut pending = Vec::new();
    pending.extend(client.handle(
        now(),
        RelayEvent::Lan(Some(LanAddress {
            ip: [192, 168, 4, 20],
            port: 80,
        })),
    ));
    pending.extend(client.handle(now(), RelayEvent::Network { joined: true }));
    pending.extend(client.handle(now(), RelayEvent::CloudRelay(true)));
    pending.extend(client.handle(now(), RelayEvent::Accounts(accounts)));
    loop {
        while !pending.is_empty() {
            let actions = std::mem::take(&mut pending);
            for action in actions {
                match action {
                    RelayAction::Resolve { .. } => {
                        pending.extend(
                            client.handle(now(), RelayEvent::Resolved(Some([127, 0, 0, 1]))),
                        );
                    }
                    RelayAction::Connect { port, .. } => {
                        match tokio_tungstenite::connect_async(format!(
                            "ws://127.0.0.1:{port}/relay/device"
                        ))
                        .await
                        {
                            Ok((socket, _)) => {
                                ws = Some(socket);
                                pending.extend(client.handle(now(), RelayEvent::Connected));
                            }
                            Err(_) => pending.extend(
                                client.handle(now(), RelayEvent::Closed { going_away: false }),
                            ),
                        }
                    }
                    RelayAction::Send(bytes) => {
                        if let Some(socket) = ws.as_mut() {
                            let _ = socket.send(Message::Binary(bytes)).await;
                        }
                    }
                    RelayAction::Close => {
                        if let Some(mut socket) = ws.take() {
                            let _ = socket.close(None).await;
                        }
                    }
                    RelayAction::RouteFrame { route, bytes } => pending.extend(client.handle(
                        now(),
                        RelayEvent::RouteSend {
                            route,
                            bytes: &bytes,
                        },
                    )),
                    RelayAction::RouteOpened(_)
                    | RelayAction::RouteClosed(_)
                    | RelayAction::TakePicture
                    | RelayAction::SendPicture
                    | RelayAction::DropPicture => {}
                }
            }
        }
        {
            let mut shared = shared.lock().unwrap();
            shared.state = client.state();
            shared.next_wake_in = client
                .next_wake()
                .map(|at| Duration::from_millis(at.saturating_sub(now())));
        }
        let wake = client.next_wake().map_or(Duration::from_secs(3600), |at| {
            Duration::from_millis(at.saturating_sub(now()))
        });
        let message = async {
            match ws.as_mut() {
                Some(socket) => socket.next().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            message = message => match message {
                Some(Ok(Message::Binary(bytes))) => {
                    pending.extend(client.handle(now(), RelayEvent::Message(&bytes)));
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
                    pending.extend(client.handle(now(), RelayEvent::Heard));
                }
                Some(Ok(Message::Close(frame))) => {
                    let going_away = frame.is_some_and(|frame| u16::from(frame.code) == 1001);
                    ws = None;
                    pending.extend(client.handle(now(), RelayEvent::Closed { going_away }));
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    ws = None;
                    pending.extend(client.handle(now(), RelayEvent::Closed { going_away: false }));
                }
            },
            () = tokio::time::sleep(wake) => {
                pending.extend(client.handle(now(), RelayEvent::Tick));
            }
        }
    }
}
