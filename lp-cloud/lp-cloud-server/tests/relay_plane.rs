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
    Actor, Base64Bytes, BoardList, BoardPictureList, BoardPictures, CLOUD_API_VERSION, CloudCall,
    CloudReply, CloudRequest, CloudResponse, KnownPicture,
};
use lpc_relay::{
    LanAddress, PictureRate, RELAY_PROTO_VERSION, RefuseReason, RelayAccount, RelayAction,
    RelayClient, RelayClientConfig, RelayEvent, RelayFrame, RelayHello, RelayPicture, RelayProject,
    RelayProjectFacts, RelayState, RouteCloseReason, frame_protocol, relay_auth_key, relay_proof,
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

/// Protocol 1, the bytes every fielded core speaks: a board registered by
/// hand (no `RelayClient`, which speaks protocol 2 now) registers, is
/// listed, routes a member's session, and is never sent a protocol 2
/// frame — not while its member watches it and asks for its picture, not
/// when another board registers beside it.
#[tokio::test]
async fn a_protocol_1_board_registers_and_routes_as_before() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let hello = RelayHello::new(MAC, "Old lamp", 39, None, vec![alice.salt]);
    let mut board = RawBoard::register(&server, &alice, hello).await;

    let listed = server.list_boards(&alice.cookie).await;
    assert_eq!(listed.boards.len(), 1);
    assert_eq!(listed.boards[0].label, "Old lamp");
    assert_eq!(listed.boards[0].relay_proto, 1);
    assert_eq!(listed.boards[0].firmware, None);
    assert_eq!(listed.boards[0].project, None);

    let mut session = server.browser(BOARD_ID, Some(&alice.cookie)).await;
    let RelayFrame::Open { route } = board.next_frame().await else {
        panic!("not an open");
    };
    session
        .send(Message::Binary(vec![0xa5, 1, 2]))
        .await
        .unwrap();
    let frame = board.next_frame().await;
    assert_eq!(
        frame,
        RelayFrame::Frame {
            route,
            bytes: vec![0xa5, 1, 2]
        }
    );
    board.send(&frame).await;
    assert_eq!(next_binary(&mut session).await, [0xa5, 1, 2]);

    // Its member watches it and asks for its picture: there is none, and
    // nothing of it reaches the board.
    for _ in 0..3 {
        let pictures = server
            .board_pictures(&alice.cookie, &[(BOARD_ID, None)], true)
            .await;
        assert!(pictures.pictures.is_empty());
    }
    session.close(None).await.unwrap();
    assert_eq!(
        board.next_frame().await,
        RelayFrame::Close {
            route,
            reason: RouteCloseReason::Gone
        }
    );

    // A protocol 2 board registers beside it, under the same account.
    let other = FakeBoard::spawn_as(
        server.port,
        [0x02, 0, 0, 0, 0, 9],
        vec![alice.relay_account()],
    );
    other.wait_for(RelayState::Connected).await;

    board.listen(Duration::from_secs(3)).await;
    board.assert_protocol_1_only();
}

#[tokio::test]
async fn a_protocol_2_board_gets_its_rate_and_its_picture_is_read_through_the_api() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let bob = server.account("bob").await;
    let guest = server.guest().await;
    let hello = RelayHello::new(MAC, "Lamp", 39, None, vec![alice.salt]).with_firmware("raw-2");
    let mut board = RawBoard::register(&server, &alice, hello).await;

    // `Registered`, then at once its rate: idle, nobody watching.
    assert_eq!(
        board.next_frame().await,
        RelayFrame::PictureRate(PictureRate {
            idle_s: 60,
            watched_ms: 500,
            watched_for_s: 0
        })
    );
    board
        .send(&RelayFrame::Project(Some(RelayProject {
            name: "Rocaille".into(),
            uid_tag: Some([0xa1; 16]),
            content_tag: None,
        })))
        .await;
    board.send(&RelayFrame::Picture(fake_picture())).await;

    let first = server.wait_for_picture(&alice.cookie, BOARD_ID).await;
    assert!(first.online);
    assert_eq!(first.outputs, [5, 3]);
    assert_eq!(first.colors, Some(Base64Bytes(fake_picture().colors)));
    let again = server
        .board_pictures(&alice.cookie, &[(BOARD_ID, Some(first.seq))], false)
        .await;
    assert_eq!(again.pictures.len(), 1);
    assert_eq!(again.pictures[0].seq, first.seq);
    assert_eq!(again.pictures[0].colors, None, "she has this one");

    // Bob and a guest read nothing, and their watching tells the board
    // nothing.
    for cookie in [&bob.cookie, &guest] {
        let pictures = server
            .board_pictures(cookie, &[(BOARD_ID, None)], true)
            .await;
        assert!(pictures.pictures.is_empty());
    }
    assert!(
        board.listen(Duration::from_millis(500)).await.is_empty(),
        "no rate for a non-member's watch"
    );

    let listed = &server.list_boards(&alice.cookie).await.boards[0];
    assert_eq!(
        (
            listed.relay_proto,
            listed.firmware.as_deref(),
            listed.project.as_deref()
        ),
        (2, Some("raw-2"), Some("Rocaille"))
    );

    // Alice watches: the board is told to be fast for the lease.
    server
        .board_pictures(&alice.cookie, &[(BOARD_ID, None)], true)
        .await;
    assert_eq!(
        board.next_frame().await,
        RelayFrame::PictureRate(PictureRate {
            idle_s: 60,
            watched_ms: 500,
            watched_for_s: 15
        })
    );
}

#[tokio::test]
async fn a_picture_outlives_its_board_until_the_server_restarts() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    let hello = RelayHello::new(MAC, "Lamp", 39, None, vec![alice.salt]).with_firmware("raw-2");
    let mut board = RawBoard::register(&server, &alice, hello).await;
    board.next_frame().await; // its rate
    board.send(&RelayFrame::Picture(fake_picture())).await;
    let seen = server.wait_for_picture(&alice.cookie, BOARD_ID).await;

    board.ws.close(None).await.unwrap();
    drop(board);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !server.list_boards(&alice.cookie).await.boards.is_empty() {
        assert!(Instant::now() < deadline, "the board never left");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let kept = server
        .board_pictures(&alice.cookie, &[(BOARD_ID, None)], true)
        .await;
    assert_eq!(kept.pictures.len(), 1);
    assert!(!kept.pictures[0].online);
    assert_eq!(kept.pictures[0].seq, seen.seq);
    assert_eq!(
        kept.pictures[0].colors,
        Some(Base64Bytes(fake_picture().colors))
    );

    // A deploy: the process goes away, and every picture with it.
    server.state.relay().going_away();
    assert!(
        server
            .board_pictures(&alice.cookie, &[(BOARD_ID, None)], false)
            .await
            .pictures
            .is_empty()
    );
}

#[tokio::test]
async fn protocol_1_and_2_boards_share_one_hub() {
    let server = Server::start().await;
    let alice = server.account("alice").await;
    const OLD_ID: &str = "020000000001";
    let hello = RelayHello::new(
        [0x02, 0, 0, 0, 0, 1],
        "Old lamp",
        39,
        None,
        vec![alice.salt],
    );
    let mut old = RawBoard::register(&server, &alice, hello).await;
    let new = FakeBoard::spawn(server.port, vec![alice.relay_account()]);
    new.wait_for(RelayState::Connected).await;

    let picture = server.wait_for_picture(&alice.cookie, BOARD_ID).await;
    assert_eq!(picture.colors, Some(Base64Bytes(fake_picture().colors)));
    let mut listed = server.list_boards(&alice.cookie).await.boards;
    listed.sort_by(|a, b| a.id.cmp(&b.id));
    let seen: Vec<(&str, u16, Option<&str>, Option<&str>)> = listed
        .iter()
        .map(|board| {
            (
                board.id.as_str(),
                board.relay_proto,
                board.firmware.as_deref(),
                board.project.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (OLD_ID, 1, None, None),
            (BOARD_ID, 2, Some("fake-board-1"), Some("Rocaille")),
        ]
    );

    // Alice watches both: the new board goes fast, the old hears nothing
    // new.
    let before = new.pictures_sent();
    for _ in 0..6 {
        let pictures = server
            .board_pictures(&alice.cookie, &[(BOARD_ID, None), (OLD_ID, None)], true)
            .await;
        assert_eq!(pictures.pictures.len(), 1, "the old board has no picture");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert!(
        new.pictures_sent() >= before + 2,
        "watched pictures every 500 ms: {} → {}",
        before,
        new.pictures_sent()
    );
    old.listen(Duration::from_millis(500)).await;
    old.assert_protocol_1_only();
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

    /// One `/api` call with `cookie`.
    async fn call(&self, cookie: &str, request: CloudRequest) -> CloudResponse {
        let reply: CloudReply = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/api", self.port))
            .header("cookie", cookie)
            .json(&CloudCall {
                version: CLOUD_API_VERSION,
                request,
            })
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        reply.result.unwrap()
    }

    /// `BoardPictures` through `/api`: each board with the `seq` the caller
    /// holds.
    async fn board_pictures(
        &self,
        cookie: &str,
        boards: &[(&str, Option<u64>)],
        watch: bool,
    ) -> BoardPictureList {
        let request = CloudRequest::BoardPictures(BoardPictures {
            boards: boards
                .iter()
                .map(|(id, seq)| KnownPicture {
                    id: (*id).to_string(),
                    seq: *seq,
                })
                .collect(),
            watch,
        });
        match self.call(cookie, request).await {
            CloudResponse::BoardPictureList(list) => list,
            other => panic!("{other:?}"),
        }
    }

    /// Board `id`'s picture, once the hub has one (within 5 s).
    async fn wait_for_picture(&self, cookie: &str, id: &str) -> lpc_cloud_api::BoardPicture {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let list = self.board_pictures(cookie, &[(id, None)], false).await;
            if let Some(picture) = list.pictures.into_iter().next() {
                return picture;
            }
            assert!(Instant::now() < deadline, "no picture of {id} within 5 s");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// A board registered by hand, frame by frame, at whatever protocol its
/// hello says: what a fielded protocol 1 core does, with every byte the hub
/// sends it kept.
struct RawBoard {
    ws: Ws,
    received: Vec<Vec<u8>>,
}

impl RawBoard {
    /// Hello, challenge, proof (for `account`), `Registered`.
    async fn register(server: &Server, account: &Account, hello: RelayHello) -> Self {
        let mac = hello.board_mac;
        let mut board = Self {
            ws: server.device_socket().await,
            received: Vec::new(),
        };
        board.send(&RelayFrame::Hello(hello)).await;
        let RelayFrame::Challenge { nonce } = board.next_frame().await else {
            panic!("no challenge");
        };
        let proof = relay_proof(&relay_auth_key(&account.k), &nonce, &mac);
        board
            .send(&RelayFrame::Proof {
                proofs: vec![proof],
            })
            .await;
        assert_eq!(
            board.next_frame().await,
            RelayFrame::Registered {
                accounts_ok: 1,
                ping_s: 25
            }
        );
        board
    }

    async fn send(&mut self, frame: &RelayFrame) {
        self.ws.send(Message::Binary(frame.encode())).await.unwrap();
    }

    /// The next frame the hub sends (within 5 s).
    async fn next_frame(&mut self) -> RelayFrame {
        let bytes = next_binary(&mut self.ws).await;
        self.received.push(bytes.clone());
        RelayFrame::decode(&bytes).expect("a relay frame")
    }

    /// Whatever the hub sends for `period`; kept, and returned.
    async fn listen(&mut self, period: Duration) -> Vec<Vec<u8>> {
        let mut heard = Vec::new();
        let deadline = tokio::time::sleep(period);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                message = self.ws.next() => match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        self.received.push(bytes.clone());
                        heard.push(bytes);
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                    other => panic!("the leg ended: {other:?}"),
                },
                () = &mut deadline => return heard,
            }
        }
    }

    /// Every message the hub ever sent this board is a protocol 1 frame.
    fn assert_protocol_1_only(&self) {
        assert!(!self.received.is_empty());
        for bytes in &self.received {
            assert_eq!(
                frame_protocol(bytes),
                Some(1),
                "a protocol 1 board was sent {bytes:02x?}"
            );
        }
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

/// A board on the relay: `lpc-relay`'s client (relay protocol 2), driven by
/// a task over tokio-tungstenite, echoing every session frame back,
/// reporting the project "Rocaille", and sending [`fake_picture`] whenever
/// the client asks for a picture.
struct FakeBoard {
    shared: Arc<Mutex<BoardShared>>,
}

struct BoardShared {
    state: RelayState,
    next_wake_in: Option<Duration>,
    pictures_sent: usize,
}

/// The fake board's project uid: a capability, which must never reach the
/// hub (only its tag does).
const FAKE_PROJECT_UID: &str = "prj7m3qk2x9z4w8v6t5r1n0p2a4c";

/// What the fake board's lamps always show: 5 + 3 lamps, 4 samples.
fn fake_picture() -> RelayPicture {
    RelayPicture {
        outputs: vec![5, 3],
        colors: vec![0xff, 0, 0, 0, 0xff, 0, 0, 0, 0xff, 0x10, 0x20, 0x30],
    }
}

impl FakeBoard {
    fn spawn(port: u16, accounts: Vec<RelayAccount>) -> Self {
        Self::spawn_as(port, MAC, accounts)
    }

    fn spawn_as(port: u16, mac: [u8; 6], accounts: Vec<RelayAccount>) -> Self {
        let shared = Arc::new(Mutex::new(BoardShared {
            state: RelayState::Off,
            next_wake_in: None,
            pictures_sent: 0,
        }));
        tokio::spawn(drive_board(port, mac, accounts, Arc::clone(&shared)));
        Self { shared }
    }

    fn pictures_sent(&self) -> usize {
        self.shared.lock().unwrap().pictures_sent
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
            firmware: "fake-board-1",
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
    pending.extend(client.handle(
        now(),
        RelayEvent::Project(Some(RelayProjectFacts {
            name: "Rocaille".into(),
            uid: Some(FAKE_PROJECT_UID.into()),
            content_hash: None,
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
                    // The fake's lamps always show the same picture, taken
                    // at once.
                    RelayAction::TakePicture => {
                        pending.extend(client.handle(now(), RelayEvent::PictureReady));
                    }
                    RelayAction::SendPicture => {
                        if let Some(socket) = ws.as_mut() {
                            let picture = RelayFrame::Picture(fake_picture()).encode();
                            let _ = socket.send(Message::Binary(picture)).await;
                        }
                        shared.lock().unwrap().pictures_sent += 1;
                    }
                    RelayAction::RouteOpened(_)
                    | RelayAction::RouteClosed(_)
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
