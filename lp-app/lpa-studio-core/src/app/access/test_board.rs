//! A fake board for the access tests: the REAL `lpc_access::LoginState`
//! verifying Studio's answers, behind an `lpa-client` io.
//!
//! The emulated Bluetooth link (`?ble=emu`) is the board's trusted link, so
//! it can never refuse anything; this is where login is exercised against a
//! board that can. It answers exactly the requests the access flow sends —
//! `Hello` (with this link's `auth`), `LoginBegin`/`LoginAnswer`, and a
//! filesystem write — and refuses anything else below edit with
//! `NotPermitted`, the way `lpa-server`'s classifier does.

use core::future::Future;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use lpa_client::{ClientIo, LpClient};
use lpc_access::{BeginOutcome, LoginState, SecretEntry, Tier};
use lpc_wire::server::{FsRequest, FsResponse};
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};

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
    secrets: Vec<SecretEntry>,
    open: bool,
    granted: Option<Tier>,
    files: BTreeMap<String, Vec<u8>>,
    nonce: u8,
}

/// One untrusted link to a fake board.
#[derive(Clone)]
pub struct FakeBoard {
    state: Rc<RefCell<BoardState>>,
}

impl FakeBoard {
    /// A board whose store holds `secrets` (label, tier, password), locked.
    pub fn locked(secrets: &[(&str, Tier, &str)]) -> Self {
        let secrets = secrets
            .iter()
            .enumerate()
            .map(|(index, (label, tier, password))| {
                SecretEntry::from_password(*label, *tier, password.as_bytes(), [index as u8; 16], 2)
            })
            .collect();
        Self {
            state: Rc::new(RefCell::new(BoardState {
                login: LoginState::new(),
                secrets,
                open: false,
                granted: None,
                files: BTreeMap::new(),
                nonce: 1,
            })),
        }
    }

    /// The same board, `open`: play without a login.
    pub fn open(secrets: &[(&str, Tier, &str)]) -> Self {
        let board = Self::locked(secrets);
        board.state.borrow_mut().open = true;
        board
    }

    /// An `lpa-client` on this link.
    pub fn client(&self) -> LpClient<FakeBoardIo> {
        LpClient::new(self.io())
    }

    pub fn io(&self) -> FakeBoardIo {
        FakeBoardIo {
            state: Rc::clone(&self.state),
            replies: VecDeque::new(),
        }
    }

    /// Wrong answers already on the board's count (to reach its backoff).
    pub fn set_failures_before(&self, count: u32) {
        let mut state = self.state.borrow_mut();
        let secrets = state.secrets.clone();
        for _ in 0..count {
            let _ = state.login.begin(now_ms(), [0; 32], secrets.clone());
            let _ = state.login.answer(now_ms(), &[]);
        }
    }

    pub fn failures(&self) -> u32 {
        self.state.borrow().login.rate_limit().failures()
    }

    /// The tier this link holds right now.
    pub fn granted(&self) -> Option<Tier> {
        let state = self.state.borrow();
        state.granted.or(state.open.then_some(Tier::Play))
    }
}

/// The client side of one fake link.
pub struct FakeBoardIo {
    state: Rc<RefCell<BoardState>>,
    replies: VecDeque<WireServerMessage>,
}

impl FakeBoardIo {
    fn answer(&self, request: ClientRequest) -> WireServerMsgBody {
        let mut state = self.state.borrow_mut();
        let held = state.granted.or(state.open.then_some(Tier::Play));
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
                pack_dictionary: 0,
                auth: lpc_wire::HelloAuth {
                    required: true,
                    granted: held,
                },
            }),
            ClientRequest::LoginBegin => {
                state.nonce = state.nonce.wrapping_add(1);
                let nonce = [state.nonce; 32];
                let secrets = state.secrets.clone();
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
                let outcome = state.login.answer(now_ms(), &macs);
                if let lpc_access::LoginOutcome::Granted { tier, .. } = &outcome {
                    state.granted = Some(*tier);
                }
                WireServerMsgBody::LoginResult(outcome)
            }
            ClientRequest::Filesystem(FsRequest::Write { path, data }) => {
                if held != Some(Tier::Edit) {
                    return WireServerMsgBody::NotPermitted { needs: Tier::Edit };
                }
                state.files.insert(path.as_str().to_string(), data);
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
