//! The cloud relay end to end on the host: an in-process `lp-cloud-server`
//! (mem store, sessions minted in-process) ↔ lp-cli's host board on the
//! relay (`serve --relay`'s own pieces: the `lpc-relay` client and a secure
//! responder per route, in front of a real `lpa-server`) ↔ lp-cli's
//! `relay:` client.
//!
//! What it proves:
//!
//! - an account the board holds the key of reaches it at that key's tier,
//!   with no password;
//! - anyone else gets in with the board's password, and with no password is
//!   refused by the board — even though the board is open at edit to anyone
//!   nearby (a relayed link never gets that);
//! - no session at all is refused at the hub, in words;
//! - the board's one session: a second client is told busy;
//! - a deploy (the server going away and coming back on the same port)
//!   brings the board back within 15 s;
//! - two boards under one account are both in `ListBoards`, on the same
//!   network as the caller (loopback);
//! - with 100 ms added each way on the device leg, the session still comes
//!   up well inside its login deadline and requests answer (numbers printed).

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use lp_cli::client::cli_connect::connect_relay;
use lp_cli::server::create_server::create_server_on;
use lp_cli::server::relay_host::relay_host_transport::NoLocalLinks;
use lp_cli::server::relay_host::{host_board_id, start_relay_host};
use lp_cli::server::run_server_loop_with;
use lp_cloud_domain::MetaStore as _;
use lp_cloud_server::app_state::AppState;
use lp_cloud_server::config::ServerConfig;
use lp_cloud_server::page::static_site::StaticSite;
use lp_cloud_server::ports::{AnyBlobStore, AnyMetaStore};
use lp_cloud_server::router::build_router;
use lp_cloud_store_mem::{MemBlobStore, MemMetaStore};
use lpa_client::LpClient;
use lpa_client::transport_lan::{BoardPassword, LanError, os_entropy};
use lpa_client::transport_relay::RelayTarget;
use lpc_access::{DeviceAccessFile, OpenTo, SecretEntry, SecretKind, Tier};
use lpc_cloud_api::{
    AccountAccessInfo, Actor, BoardList, CLOUD_API_VERSION, CloudCall, CloudReply, CloudRequest,
    CloudResponse,
};
use lpc_history::PrefixedUid;
use lpc_model::AsLpPath;
use lpc_relay::RelayBoardId;
use lpfs::{LpFs, LpFsMemory};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

const PASSWORD: &str = "camp fire";
/// Cheap for a test; a person's password is written with far more.
const PASSWORD_ITERATIONS: u32 = 4;

#[test]
fn an_account_the_board_holds_reaches_it_at_its_tier_with_no_password() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    run(async {
        let connection = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the account's key opens the board");
        assert_eq!(
            connection.hello().unwrap().auth.granted,
            Some(Tier::Edit),
            "the account entry's tier"
        );
        let mut client = LpClient::new(connection.client_io());
        client
            .network_status()
            .await
            .expect("an edit-tier request (wifi status) is answered through the relay");
        client
            .project_list_loaded()
            .await
            .expect("and a play-tier one");
        drop(client);
        connection.close().await;
    });
}

#[test]
fn anyone_else_needs_the_boards_password_even_on_a_board_open_to_anyone_nearby() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let bob = cloud.account("bob");
    let guest = cloud.guest();
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[password_entry()]);
    cloud.wait_for_boards(1);

    // No password: the anonymous session holds nothing, so the board is
    // locked to this client — though it is open at edit to anyone nearby.
    for session in [bob.session.clone(), guest.clone()] {
        let refused = run(async {
            match connect_relay(cloud.target(board), Some(session), None).await {
                Ok(_) => panic!("a visitor with no password got in"),
                Err(error) => error,
            }
        });
        assert_eq!(
            refused.downcast_ref::<LanError>(),
            Some(&LanError::Locked),
            "{refused}"
        );
    }

    // The board's password: in, at the password entry's tier.
    for session in [bob.session.clone(), guest] {
        run(async {
            let connection = connect_relay(
                cloud.target(board),
                Some(session),
                Some(BoardPassword::new(PASSWORD)),
            )
            .await
            .expect("the board's password opens it for a visitor");
            assert_eq!(connection.hello().unwrap().auth.granted, Some(Tier::Edit));
            let mut client = LpClient::new(connection.client_io());
            client
                .network_status()
                .await
                .expect("edit through the relay");
            drop(client);
            connection.close().await;
        });
    }

    let wrong = run(async {
        match connect_relay(
            cloud.target(board),
            Some(bob.session.clone()),
            Some(BoardPassword::new("camp-fire")),
        )
        .await
        {
            Ok(_) => panic!("a wrong password got in"),
            Err(error) => error,
        }
    });
    assert_eq!(
        wrong.downcast_ref::<LanError>(),
        Some(&LanError::WrongPassword)
    );
}

#[test]
fn no_session_is_refused_at_the_hub_in_words() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);
    let refused = run(async {
        match connect_relay(
            cloud.target(board),
            None,
            Some(BoardPassword::new(PASSWORD)),
        )
        .await
        {
            Ok(_) => panic!("a client with no session got through the hub"),
            Err(error) => error,
        }
    });
    assert!(
        refused.to_string().contains("Sign in to lightplayer.app"),
        "{refused}"
    );
}

#[test]
fn the_boards_one_session_tells_a_second_client_busy() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);
    run(async {
        let first = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the first session");
        let second = connect_relay(cloud.target(board), Some(alice.session.clone()), None).await;
        let error = match second {
            Ok(_) => panic!("a second session got in to a board that holds one"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error.downcast_ref::<LanError>(),
                Some(LanError::Busy { .. })
            ),
            "{error}"
        );
        assert!(error.to_string().contains("busy"), "{error}");
        first.close().await;
    });
}

#[test]
fn after_a_deploy_the_board_is_back_within_fifteen_seconds() {
    let mut cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    let restarted = Instant::now();
    cloud.restart();
    let session = cloud.session_for(alice.uid);
    let deadline = restarted + Duration::from_secs(15);
    loop {
        if cloud.list_boards(&session).boards.len() == 1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the board did not come back within 15 s"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    println!(
        "relay e2e: board back {:.1} s after the deploy",
        restarted.elapsed().as_secs_f64()
    );
    run(async {
        let connection = connect_relay(cloud.target(board), Some(session.clone()), None)
            .await
            .expect("a client reconnects after the deploy");
        connection.close().await;
    });
}

#[test]
fn two_boards_under_one_account_are_both_listed_on_the_same_network() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let one = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    let two = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(2);

    let list = cloud.list_boards(&alice.session);
    let mut ids: Vec<String> = list.boards.iter().map(|board| board.id.clone()).collect();
    ids.sort();
    let mut expected = vec![one.to_string(), two.to_string()];
    expected.sort();
    assert_eq!(ids, expected);
    for board in &list.boards {
        assert!(board.same_network, "loopback is one network");
        assert_eq!(board.lan, None, "a host board reports no LAN address");
        assert_eq!(board.wire_proto, lpc_wire::WIRE_PROTO_VERSION);
    }
    let bob = cloud.account("bob");
    assert!(cloud.list_boards(&bob.session).boards.is_empty());
}

/// R2: the relay adds round trip. With 100 ms added each way on the device
/// leg (so ≥ 200 ms per lp-link round trip), the session still comes up
/// inside the 10 s login deadline and requests answer, with no lp-link
/// timing changed. The numbers go in the phase's record.
#[test]
fn a_session_through_a_slow_device_leg_stays_up() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let slow = cloud.delayed_origin(Duration::from_millis(100));
    let board = HostBoard::start(&slow, vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    run(async {
        let opening = Instant::now();
        let connection = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the session comes up over a slow device leg");
        let opened = opening.elapsed();
        assert!(opened < Duration::from_secs(10), "{opened:?}");
        let mut client = LpClient::new(connection.client_io());
        let mut rtts = Vec::new();
        for _ in 0..5 {
            let asked = Instant::now();
            client.network_status().await.expect("a request answers");
            rtts.push(asked.elapsed().as_millis());
        }
        println!(
            "relay e2e, device leg +100 ms each way: session up in {} ms; request round trips {rtts:?} ms",
            opened.as_millis()
        );
        drop(client);
        connection.close().await;
    });
}

// ---- helpers ---------------------------------------------------------

/// An lp-cloud-server on a loopback port, on its own runtime.
struct Cloud {
    runtime: Arc<tokio::runtime::Runtime>,
    state: AppState,
    port: u16,
    serve: tokio::task::JoinHandle<()>,
}

struct Account {
    uid: PrefixedUid,
    session: String,
    access: AccountAccessInfo,
}

impl Account {
    /// The entry Studio installs on a board for this account.
    fn entry(&self) -> SecretEntry {
        SecretEntry::from_password(
            "test's account",
            Tier::Edit,
            &self.access.key_secret,
            self.access.key_salt,
            1,
        )
        .with_kind(SecretKind::Account)
    }
}

impl Cloud {
    fn start(port: Option<u16>) -> Self {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
        );
        let state = fresh_state();
        let (serve, port) = Self::serve(&runtime, state.clone(), port);
        Self {
            runtime,
            state,
            port,
            serve,
        }
    }

    fn serve(
        runtime: &tokio::runtime::Runtime,
        state: AppState,
        port: Option<u16>,
    ) -> (tokio::task::JoinHandle<()>, u16) {
        let listener = runtime.block_on(async {
            let address = format!("127.0.0.1:{}", port.unwrap_or(0));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match TcpListener::bind(&address).await {
                    Ok(listener) => break listener,
                    Err(error) if Instant::now() < deadline => {
                        let _ = error;
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(error) => panic!("could not bind {address}: {error}"),
                }
            }
        });
        let port = listener.local_addr().unwrap().port();
        let app = build_router(state).into_make_service_with_connect_info::<SocketAddr>();
        let serve = runtime.spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (serve, port)
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn target(&self, board: RelayBoardId) -> RelayTarget {
        RelayTarget::new(board, self.origin())
    }

    /// A signed-in account with its account key minted, and a session.
    fn account(&self, name: &str) -> Account {
        let name = name.to_string();
        let (uid, token, access) = self.runtime.block_on(self.state.with_service(move |core| {
            let email = format!("{name}@example.com");
            let user = core
                .service
                .upsert_user(&name, &email, &name, "google", None, None, None);
            let token = core.service.open_session(user.uid, 3600.0, None);
            let access = match core
                .service
                .handle(Actor::User(user.uid), CloudRequest::GetAccountAccess)
                .unwrap()
            {
                CloudResponse::AccountAccessInfo(access) => access,
                other => panic!("{other:?}"),
            };
            (user.uid, token, access)
        }));
        Account {
            uid,
            session: URL_SAFE_NO_PAD.encode(token),
            access,
        }
    }

    /// A guest session (no account key).
    fn guest(&self) -> String {
        let token = self.runtime.block_on(self.state.with_service(|core| {
            let user = core.service.begin_guest_user();
            core.service.open_session(user.uid, 3600.0, None)
        }));
        URL_SAFE_NO_PAD.encode(token)
    }

    /// A fresh session for an account already in the store.
    fn session_for(&self, uid: PrefixedUid) -> String {
        let token = self.runtime.block_on(
            self.state
                .with_service(move |core| core.service.open_session(uid, 3600.0, None)),
        );
        URL_SAFE_NO_PAD.encode(token)
    }

    fn wait_for_boards(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.state.relay().board_count() < count {
            assert!(
                Instant::now() < deadline,
                "{count} board(s) did not register; {} did",
                self.state.relay().board_count()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn list_boards(&self, session: &str) -> BoardList {
        let url = format!("{}/api", self.origin());
        let cookie = format!("lp_session={session}");
        let reply: CloudReply = self.runtime.block_on(async move {
            reqwest::Client::new()
                .post(url)
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
                .unwrap()
        });
        match reply.result.unwrap() {
            CloudResponse::BoardList(list) => list,
            other => panic!("{other:?}"),
        }
    }

    /// A deploy: every leg told "going away", the process gone, a new one
    /// on the same port with the same accounts (the store survives a
    /// deploy; presence does not).
    fn restart(&mut self) {
        self.state.relay().going_away();
        self.serve.abort();
        let _ = self.runtime.block_on(&mut self.serve);
        let old = self.state.clone();
        let new = fresh_state();
        let rows = self.runtime.block_on(old.with_service(|core| {
            let store = core.service.store();
            store
                .users(100)
                .into_iter()
                .map(|user| {
                    let access = store.account_access(user.uid);
                    (user, access)
                })
                .collect::<Vec<_>>()
        }));
        self.runtime.block_on(new.with_service(move |core| {
            for (user, access) in rows {
                core.service.store_mut().put_user(user);
                if let Some(access) = access {
                    core.service.store_mut().put_account_access(access);
                }
            }
        }));
        let (serve, port) = Self::serve(&self.runtime, new.clone(), Some(self.port));
        assert_eq!(port, self.port);
        self.state = new;
        self.serve = serve;
    }

    /// An origin whose traffic reaches this server `delay` late each way
    /// (a loopback proxy): what a slow home uplink does to the device leg.
    fn delayed_origin(&self, delay: Duration) -> String {
        let target: SocketAddr = format!("127.0.0.1:{}", self.port).parse().unwrap();
        let listener = self
            .runtime
            .block_on(TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        self.runtime.spawn(async move {
            while let Ok((inbound, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let Ok(outbound) = TcpStream::connect(target).await else {
                        return;
                    };
                    let (in_read, in_write) = inbound.into_split();
                    let (out_read, out_write) = outbound.into_split();
                    tokio::spawn(delayed_copy(in_read, out_write, delay));
                    tokio::spawn(delayed_copy(out_read, in_write, delay));
                });
            }
        });
        format!("http://127.0.0.1:{port}")
    }
}

/// Copy `from` to `to`, each chunk arriving `delay` after it was read, in
/// order.
async fn delayed_copy(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    delay: Duration,
) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(tokio::time::Instant, Vec<u8>)>();
    let writer = tokio::spawn(async move {
        while let Some((due, chunk)) = rx.recv().await {
            tokio::time::sleep_until(due).await;
            if to.write_all(&chunk).await.is_err() {
                return;
            }
        }
        let _ = to.shutdown().await;
    });
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        match from.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let due = tokio::time::Instant::now() + delay;
                if tx.send((due, buffer[..n].to_vec())).is_err() {
                    break;
                }
            }
        }
    }
    drop(tx);
    let _ = writer.await;
}

fn fresh_state() -> AppState {
    let config = ServerConfig::from_vars(|name| match name {
        "LP_CLOUD_STORE" | "LP_CLOUD_BLOBS" => Some("mem".to_string()),
        _ => None,
    })
    .expect("the test configuration parses");
    AppState::new(
        config,
        AnyMetaStore::new(MemMetaStore::new()),
        AnyBlobStore::new(MemBlobStore::new()),
        StaticSite::open(None),
    )
}

/// The board's own password, at edit (stretched, as a person's is).
fn password_entry() -> SecretEntry {
    SecretEntry::from_password(
        "desk",
        Tier::Edit,
        PASSWORD.as_bytes(),
        [5; 16],
        PASSWORD_ITERATIONS,
    )
}

/// lp-cli's host board on the relay, on its own thread (the server is
/// single-threaded): a memory filesystem whose access store holds
/// `accounts` and `passwords` and is open at EDIT to anyone nearby — the
/// worst case for the relay's second lock.
struct HostBoard;

impl HostBoard {
    fn start(origin: &str, accounts: Vec<SecretEntry>, passwords: &[SecretEntry]) -> RelayBoardId {
        let mut seed = [0u8; 16];
        os_entropy(&mut seed);
        let board = host_board_id(&URL_SAFE_NO_PAD.encode(seed));
        let origin = origin.to_string();
        let mut secrets = accounts;
        secrets.extend_from_slice(passwords);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let fs = LpFsMemory::new();
            let store = DeviceAccessFile {
                version: DeviceAccessFile::VERSION,
                secrets,
                ble_enabled: true,
                open: OpenTo::Edit,
            };
            fs.write_file(
                DeviceAccessFile::PATH.as_path(),
                store.to_json().unwrap().as_bytes(),
            )
            .unwrap();
            let accounts = lp_cli::server::relay_host::relay_accounts(&fs);
            let mut server = create_server_on(Box::new(fs), None, true, None).unwrap();
            server.set_entropy_source(Some(os_entropy));
            runtime.block_on(async move {
                let transport =
                    start_relay_host(NoLocalLinks, &origin, board, "test board".into(), accounts)
                        .unwrap();
                let _ = run_server_loop_with(server, transport, |server, transport| {
                    transport.send_hellos(server);
                })
                .await;
            });
        });
        board
    }
}

/// Run `future` the way lp-cli's commands do: a current-thread runtime and
/// a `LocalSet` (the CLI connection is single-actor).
fn run<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, future)
}
