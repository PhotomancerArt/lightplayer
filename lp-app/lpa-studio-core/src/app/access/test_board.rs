//! A fake board for the access tests: the REAL `lpc_access::LoginState`
//! verifying Studio's answers, and the REAL `lpa-server` access store
//! (`access_store::access_*` over an in-memory fs) answering the device's
//! access requests — behind an `lpa-client` io.
//!
//! The emulated Bluetooth link (`?ble=emu`) is the board's trusted link, so
//! it can never refuse anything; this is where login is exercised against a
//! board that can (plan D19). It answers exactly the requests the access flow
//! sends — `Hello` (with this link's `auth`), `LoginBegin`/`LoginAnswer`, the
//! four access requests, and a filesystem write — and refuses anything else
//! below edit with `NotPermitted`, the way `lpa-server`'s classifier does.
//! [`FakeBoard::usb`] is a trusted link to the same board, the way USB is.

use core::future::Future;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use lpa_client::{ClientIo, LpClient};
use lpa_server::access_store;
use lpc_access::{BeginOutcome, DeviceAccessFile, LoginState, SecretEntry, Tier};
use lpc_wire::server::{FsRequest, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};
use lpfs::LpFsMemory;

thread_local! {
    static CLOCK_MS: Cell<u64> = const { Cell::new(0) };
}

/// Move the fake boards' shared clock forward.
pub fn advance_clock(ms: u64) {
    CLOCK_MS.with(|clock| clock.set(clock.get() + ms));
}

fn now_ms() -> u64 {
    CLOCK_MS.with(Cell::get)
}

struct BoardState {
    login: LoginState,
    fs: LpFsMemory,
    granted: Option<Tier>,
    nonce: u8,
    /// Login answers the board has heard.
    answers: u32,
}

/// One fake board; [`Self::client`] is an untrusted (Bluetooth) link to it
/// and [`Self::usb`] a trusted one.
#[derive(Clone)]
pub struct FakeBoard {
    state: Rc<RefCell<BoardState>>,
}

impl FakeBoard {
    /// A board with no device store at all (as it ships: Bluetooth on,
    /// locked, no keys).
    pub fn fresh() -> Self {
        Self {
            state: Rc::new(RefCell::new(BoardState {
                login: LoginState::new(),
                fs: LpFsMemory::new(),
                granted: None,
                nonce: 1,
                answers: 0,
            })),
        }
    }

    /// A board whose store holds `secrets` (label, tier, password), locked.
    pub fn locked(secrets: &[(&str, Tier, &str)]) -> Self {
        let entries = secrets
            .iter()
            .enumerate()
            .map(|(index, (label, tier, password))| {
                SecretEntry::from_password(*label, *tier, password.as_bytes(), [index as u8; 16], 2)
            })
            .collect();
        Self::with_entries(entries)
    }

    /// A board whose store holds exactly `entries`, locked.
    pub fn with_entries(entries: Vec<SecretEntry>) -> Self {
        let board = Self::fresh();
        let mut store = DeviceAccessFile::fresh();
        store.secrets = entries;
        board.write_store(&store);
        board
    }

    /// The same board, `open`: play without a login.
    pub fn open(secrets: &[(&str, Tier, &str)]) -> Self {
        let board = Self::locked(secrets);
        let mut store = board.store();
        store.open = true;
        board.write_store(&store);
        board
    }

    /// An `lpa-client` on an untrusted (Bluetooth) link to this board.
    pub fn client(&self) -> LpClient<FakeBoardIo> {
        LpClient::new(self.io())
    }

    /// An `lpa-client` on a trusted (USB) link to this board.
    pub fn usb(&self) -> LpClient<FakeBoardIo> {
        LpClient::new(FakeBoardIo {
            state: Rc::clone(&self.state),
            replies: VecDeque::new(),
            trusted: true,
        })
    }

    pub fn io(&self) -> FakeBoardIo {
        FakeBoardIo {
            state: Rc::clone(&self.state),
            replies: VecDeque::new(),
            trusted: false,
        }
    }

    /// The device store as the board reads it.
    pub fn store(&self) -> DeviceAccessFile {
        access_store::read_device_store(&self.state.borrow().fs)
    }

    fn write_store(&self, store: &DeviceAccessFile) {
        access_store::write_device_store(&self.state.borrow().fs, store)
            .expect("the fake board's fs takes the store");
    }

    /// Wrong answers already on the board's count (to reach its backoff).
    pub fn set_failures_before(&self, count: u32) {
        let mut state = self.state.borrow_mut();
        let secrets = access_store::installed_secrets(&state.fs, []);
        for _ in 0..count {
            let _ = state.login.begin(now_ms(), [0; 32], secrets.clone());
            let _ = state.login.answer(now_ms(), &[]);
        }
    }

    pub fn failures(&self) -> u32 {
        self.state.borrow().login.rate_limit().failures()
    }

    /// Login answers the board has heard, right or wrong.
    pub fn answers(&self) -> u32 {
        self.state.borrow().answers
    }

    /// The tier the untrusted link holds right now.
    pub fn granted(&self) -> Option<Tier> {
        let state = self.state.borrow();
        let open = access_store::read_device_store(&state.fs).open;
        state.granted.or(open.then_some(Tier::Play))
    }
}

/// The client side of one fake link.
pub struct FakeBoardIo {
    state: Rc<RefCell<BoardState>>,
    replies: VecDeque<WireServerMessage>,
    trusted: bool,
}

impl FakeBoardIo {
    fn answer(&self, request: ClientRequest) -> WireServerMsgBody {
        let mut state = self.state.borrow_mut();
        let open = access_store::read_device_store(&state.fs).open;
        let held = if self.trusted {
            Some(Tier::Edit)
        } else {
            state.granted.or(open.then_some(Tier::Play))
        };
        match request {
            ClientRequest::Hello => WireServerMsgBody::Hello(lpc_wire::ServerHello {
                proto: lpc_wire::WIRE_PROTO_VERSION,
                build: lpc_wire::BuildFacts {
                    features: Vec::new(),
                    package: "fw-esp32c6".to_string(),
                    commit: "abc1234".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                hardware: lpc_wire::HardwareFacts::default(),
                device_uid: None,
                // This fake speaks JSON only: 0 names no dictionary.
                pack_format: 0,
                auth: lpc_wire::HelloAuth {
                    required: !self.trusted,
                    granted: held,
                },
            }),
            ClientRequest::LoginBegin => {
                state.nonce = state.nonce.wrapping_add(1);
                let nonce = [state.nonce; 32];
                let secrets = access_store::installed_secrets(&state.fs, []);
                match state.login.begin(now_ms(), nonce, secrets) {
                    BeginOutcome::Challenge(challenge) => WireServerMsgBody::LoginChallenge {
                        nonce: challenge.nonce,
                        offers: challenge.offers,
                    },
                    BeginOutcome::Refused { retry_after_ms } => {
                        WireServerMsgBody::LoginResult(lpc_access::LoginOutcome::Refused {
                            retry_after_ms,
                        })
                    }
                }
            }
            ClientRequest::LoginAnswer { macs } => {
                state.answers += 1;
                let outcome = state.login.answer(now_ms(), &macs);
                if let lpc_access::LoginOutcome::Granted { tier, .. } = &outcome {
                    state.granted = Some(*tier);
                }
                WireServerMsgBody::LoginResult(outcome)
            }
            request @ (ClientRequest::AccessList
            | ClientRequest::AccessAdd { .. }
            | ClientRequest::AccessRemove { .. }
            | ClientRequest::AccessSetSwitches { .. }) => {
                if held != Some(Tier::Edit) {
                    return WireServerMsgBody::NotPermitted { needs: Tier::Edit };
                }
                match request {
                    ClientRequest::AccessList => access_store::access_list(&state.fs),
                    ClientRequest::AccessAdd { entry } => {
                        access_store::access_add(&state.fs, entry)
                    }
                    ClientRequest::AccessRemove { salt } => {
                        access_store::access_remove(&state.fs, &salt)
                    }
                    ClientRequest::AccessSetSwitches { ble_enabled, open } => {
                        access_store::access_set_switches(&state.fs, ble_enabled, open)
                    }
                    _ => unreachable!("matched above"),
                }
            }
            ClientRequest::Filesystem(FsRequest::Write { path, data }) => {
                if held != Some(Tier::Edit) {
                    return WireServerMsgBody::NotPermitted { needs: Tier::Edit };
                }
                let _ = lpfs::LpFs::write_file(&state.fs, path.as_path(), &data);
                WireServerMsgBody::Filesystem(FsResponse::Write { path, error: None })
            }
            _ => WireServerMsgBody::NotPermitted { needs: Tier::Edit },
        }
    }
}

#[async_trait::async_trait(?Send)]
impl ClientIo for FakeBoardIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let body = self.answer(msg.msg);
        self.replies.push_back(WireServerMessage::new(msg.id, body));
        Ok(())
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        self.replies
            .pop_front()
            .ok_or_else(|| TransportError::Other("nothing was asked".to_string()))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Drive an immediately-ready future (tests are an edge; see AGENTS.md).
pub fn block_on<F: Future>(future: F) -> F::Output {
    use core::task::{Context, Poll};
    use std::sync::Arc;
    use std::task::Wake;

    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = core::task::Waker::from(Arc::new(Noop));
    let mut cx = Context::from_waker(&waker);
    let mut future = core::pin::pin!(future);
    for _ in 0..10_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
    }
    panic!("an access test future did not complete");
}
