//! An in-process `lp-cloud-server` (mem store, sessions minted in-process)
//! on a loopback port, for the relay's tests: `relay_link.rs` (the host
//! board and the board's own relay driver on the host harness) and
//! `emu_relay_link.rs` (an emulated C6 through the virtual LAN's uplink).

#![allow(dead_code, reason = "each test file uses part of this")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use lp_cloud_domain::MetaStore as _;
use lp_cloud_server::app_state::AppState;
use lp_cloud_server::config::ServerConfig;
use lp_cloud_server::page::static_site::StaticSite;
use lp_cloud_server::ports::{AnyBlobStore, AnyMetaStore};
use lp_cloud_server::router::build_router;
use lp_cloud_store_mem::{MemBlobStore, MemMetaStore};
use lpa_client::transport_relay::RelayTarget;
use lpc_access::{SecretEntry, SecretKind, Tier};
use lpc_cloud_api::{
    AccountAccessInfo, Actor, BoardList, CLOUD_API_VERSION, CloudCall, CloudReply, CloudRequest,
    CloudResponse,
};
use lpc_history::PrefixedUid;
use lpc_relay::RelayBoardId;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// An lp-cloud-server on a loopback port, on its own runtime.
pub struct Cloud {
    pub runtime: Arc<tokio::runtime::Runtime>,
    pub state: AppState,
    pub port: u16,
    serve: tokio::task::JoinHandle<()>,
}

pub struct Account {
    pub uid: PrefixedUid,
    pub session: String,
    pub access: AccountAccessInfo,
}

impl Account {
    /// The entry Studio installs on a board for this account.
    pub fn entry(&self) -> SecretEntry {
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
    pub fn start(port: Option<u16>) -> Self {
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

    pub fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn target(&self, board: RelayBoardId) -> RelayTarget {
        RelayTarget::new(board, self.origin())
    }

    /// A signed-in account with its account key minted, and a session.
    pub fn account(&self, name: &str) -> Account {
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
    pub fn guest(&self) -> String {
        let token = self.runtime.block_on(self.state.with_service(|core| {
            let user = core.service.begin_guest_user();
            core.service.open_session(user.uid, 3600.0, None)
        }));
        URL_SAFE_NO_PAD.encode(token)
    }

    /// A fresh session for an account already in the store.
    pub fn session_for(&self, uid: PrefixedUid) -> String {
        let token = self.runtime.block_on(
            self.state
                .with_service(move |core| core.service.open_session(uid, 3600.0, None)),
        );
        URL_SAFE_NO_PAD.encode(token)
    }

    /// Boards registered with the hub now.
    pub fn board_count(&self) -> usize {
        self.state.relay().board_count()
    }

    /// Wait up to `wait` for exactly `count` boards; whether it came.
    pub fn boards_within(&self, count: usize, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        while self.board_count() != count {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }

    /// The account resets its key (`ResetAccountKey`): every board holding
    /// the old one is refused at its next registration.
    pub fn reset_account_key(&self, uid: PrefixedUid) {
        self.runtime.block_on(self.state.with_service(move |core| {
            core.service
                .handle(Actor::User(uid), CloudRequest::ResetAccountKey)
                .expect("the key resets");
        }));
    }

    pub fn wait_for_boards(&self, count: usize) {
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

    pub fn list_boards(&self, session: &str) -> BoardList {
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
    pub fn restart(&mut self) {
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
    pub fn delayed_origin(&self, delay: Duration) -> String {
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

pub fn fresh_state() -> AppState {
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
