//! [`EngineLoginGate`]: when an update may start on a new Bluetooth link
//! session, and the running engine's login that comes first.
//!
//! A radio link is never trusted. What a board lets it do over channel 3
//! depends on who answers there:
//!
//! - **A running engine** passes channel 3 to its update hook with the
//!   tier its **server's** login (channel 1) holds for that link, and takes
//!   no `L` itself (`lpc_update`'s `running_engine`): a core install needs
//!   edit, the backup's read-back play. Started first, the driver would be
//!   refused and stop `NeedsEngineLogin`. So with `--ota-password` the gate
//!   logs in on channel 1 first — the board's hello says an engine runs —
//!   and opens on the verdict.
//! - **Core-only** has no server: it says `M` on channel 3 unprompted the
//!   moment a link comes up, and that opens the gate at once. Its own login
//!   (`L`, D3a) is the driver's, on `N`/`A`, with the same password.
//!
//! Without a password the gate opens on the hello, and a locked board's
//! refusal is the run's answer (the refusal check of the silicon walk). A
//! board that says neither within [`SIGN_WAIT_MS`] is asked anyway.
//!
//! The gate is per link session: every new session — a new Bluetooth
//! connection, or lp-link restarting under one — closes it again, because a
//! login's tier belongs to the link that earned it.

use lpc_access::{LoginMac, LoginOutcome, derive_login_key};
use lpc_wire::{ClientMessage, ClientRequest, WireServerMsgBody};

/// First id of the gate's login requests: far from `--request`'s
/// (`REQUEST_ID_BASE`) and from any a client counts.
pub const ENGINE_LOGIN_ID_BASE: u64 = 2_000_000;

/// How long a new session waits for a hello or an unprompted `M` before the
/// update is asked to start anyway.
pub const SIGN_WAIT_MS: u64 = 3_000;

/// How long a login waits for the board's verdict before the update starts
/// without it (a locked board closes an unauthenticated radio link at 10 s).
pub const LOGIN_WAIT_MS: u64 = 10_000;

/// What the gate wants done.
#[derive(Debug)]
pub enum GateStep {
    Nothing,
    /// Send this on channel 1.
    Send(ClientMessage),
    /// The update may start on this session; the line says why.
    Open(String),
}

/// The gate. See the module docs.
pub struct EngineLoginGate {
    password: Option<Vec<u8>>,
    state: GateState,
    next_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GateState {
    /// No session.
    Closed,
    /// A session, and no word from the board yet.
    Waiting {
        since_ms: u64,
    },
    /// The engine's login is out.
    LoggingIn {
        since_ms: u64,
    },
    Open,
}

impl EngineLoginGate {
    /// A gate that logs in with `password`, if any.
    pub fn new(password: Option<&str>) -> Self {
        Self {
            password: password.map(|p| p.as_bytes().to_vec()),
            state: GateState::Closed,
            next_id: ENGINE_LOGIN_ID_BASE,
        }
    }

    /// A new link session came up: wait for the board to say what runs.
    pub fn session_up(&mut self, now_ms: u64) {
        self.state = GateState::Waiting { since_ms: now_ms };
    }

    /// The session went away.
    pub fn session_down(&mut self) {
        self.state = GateState::Closed;
    }

    /// The update may run on this session.
    pub fn is_open(&self) -> bool {
        self.state == GateState::Open
    }

    /// A channel-3 message arrived. Before anything else, it is core-only's
    /// unprompted `M`: there is no engine to log in to.
    pub fn on_update_message(&mut self) -> GateStep {
        match self.state {
            GateState::Waiting { .. } => {
                self.open("the board runs its core alone (an unprompted manifest)")
            }
            _ => GateStep::Nothing,
        }
    }

    /// A channel-1 message arrived.
    pub fn on_server(&mut self, now_ms: u64, body: &WireServerMsgBody) -> GateStep {
        match (self.state, body) {
            (GateState::Waiting { .. }, WireServerMsgBody::Hello(_)) => {
                if self.password.is_none() {
                    return self.open("the engine runs; no --ota-password, so no login");
                }
                self.state = GateState::LoggingIn { since_ms: now_ms };
                GateStep::Send(self.message(ClientRequest::LoginBegin))
            }
            (GateState::LoggingIn { .. }, WireServerMsgBody::LoginChallenge { nonce, offers }) => {
                let password = self.password.as_deref().unwrap_or_default();
                let macs = offers
                    .iter()
                    .map(|offer| {
                        let key = derive_login_key(password, &offer.salt, offer.iterations);
                        LoginMac::compute(&key, nonce)
                    })
                    .collect();
                GateStep::Send(self.message(ClientRequest::LoginAnswer { macs }))
            }
            (GateState::LoggingIn { .. }, WireServerMsgBody::LoginResult(outcome)) => match outcome
            {
                LoginOutcome::Granted { tier, .. } => {
                    self.open(&format!("the engine's login granted {tier:?}"))
                }
                LoginOutcome::Refused { retry_after_ms } => self.open(&format!(
                    "the engine's login was refused (retry after {retry_after_ms} ms); \
                         asking without it"
                )),
            },
            _ => GateStep::Nothing,
        }
    }

    /// Time passed: a board that said nothing is asked anyway.
    pub fn tick(&mut self, now_ms: u64) -> GateStep {
        match self.state {
            GateState::Waiting { since_ms } if now_ms.saturating_sub(since_ms) >= SIGN_WAIT_MS => {
                self.open(&format!(
                    "no hello and no manifest in {} s; asking anyway",
                    SIGN_WAIT_MS / 1000
                ))
            }
            GateState::LoggingIn { since_ms }
                if now_ms.saturating_sub(since_ms) >= LOGIN_WAIT_MS =>
            {
                self.open(&format!(
                    "no login verdict in {} s; asking without it",
                    LOGIN_WAIT_MS / 1000
                ))
            }
            _ => GateStep::Nothing,
        }
    }

    fn open(&mut self, why: &str) -> GateStep {
        self.state = GateState::Open;
        GateStep::Open(why.to_string())
    }

    fn message(&mut self, msg: ClientRequest) -> ClientMessage {
        self.next_id += 1;
        ClientMessage {
            id: self.next_id,
            msg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_access::{BeginOutcome, LoginState, SecretEntry, Tier};

    #[test]
    fn a_running_engine_is_logged_in_to_before_the_update_starts() {
        let mut gate = EngineLoginGate::new(Some("desk-lab"));
        gate.session_up(0);
        assert!(!gate.is_open());
        let GateStep::Send(begin) = gate.on_server(10, &hello()) else {
            panic!("a hello with a password begins the engine's login");
        };
        assert!(matches!(begin.msg, ClientRequest::LoginBegin));
        assert!(!gate.is_open(), "not before the verdict");

        // The board's side: the real login machine, holding the password.
        let mut board = LoginState::new();
        let secret = SecretEntry::from_password("lab", Tier::Edit, b"desk-lab", [3; 16], 1_000);
        let BeginOutcome::Challenge(challenge) = board.begin(10, [9; 32], vec![secret]) else {
            panic!("the board challenges");
        };
        let GateStep::Send(answer) = gate.on_server(
            20,
            &WireServerMsgBody::LoginChallenge {
                nonce: challenge.nonce,
                offers: challenge.offers,
            },
        ) else {
            panic!("a challenge is answered");
        };
        let ClientRequest::LoginAnswer { macs } = answer.msg else {
            panic!("an answer");
        };
        let verdict = board.answer(30, &macs);
        assert!(matches!(
            verdict,
            LoginOutcome::Granted {
                tier: Tier::Edit,
                ..
            }
        ));
        let step = gate.on_server(30, &WireServerMsgBody::LoginResult(verdict));
        assert!(matches!(step, GateStep::Open(why) if why.contains("granted Edit")));
        assert!(gate.is_open());
    }

    #[test]
    fn core_only_opens_on_its_unprompted_manifest_with_no_engine_login() {
        let mut gate = EngineLoginGate::new(Some("desk-lab"));
        gate.session_up(0);
        assert!(matches!(gate.on_update_message(), GateStep::Open(_)));
        assert!(gate.is_open());
        // A later hello (a `--request hello`) starts nothing.
        assert!(matches!(gate.on_server(5, &hello()), GateStep::Nothing));
    }

    #[test]
    fn without_a_password_the_hello_opens_it() {
        let mut gate = EngineLoginGate::new(None);
        gate.session_up(0);
        assert!(matches!(gate.on_server(1, &hello()), GateStep::Open(_)));
    }

    #[test]
    fn a_refused_login_still_opens_it_so_the_refusal_is_reported() {
        let mut gate = EngineLoginGate::new(Some("wrong"));
        gate.session_up(0);
        gate.on_server(1, &hello());
        let step = gate.on_server(
            2,
            &WireServerMsgBody::LoginResult(LoginOutcome::Refused {
                retry_after_ms: 500,
            }),
        );
        assert!(matches!(step, GateStep::Open(why) if why.contains("refused")));
    }

    #[test]
    fn a_silent_board_is_asked_anyway_and_every_session_starts_closed() {
        let mut gate = EngineLoginGate::new(Some("desk-lab"));
        gate.session_up(1_000);
        assert!(matches!(
            gate.tick(1_000 + SIGN_WAIT_MS - 1),
            GateStep::Nothing
        ));
        assert!(matches!(gate.tick(1_000 + SIGN_WAIT_MS), GateStep::Open(_)));
        gate.session_down();
        assert!(!gate.is_open());
        gate.session_up(9_000);
        assert!(!gate.is_open(), "a new session is a new link: closed again");
        gate.on_server(9_001, &hello());
        assert!(matches!(
            gate.tick(9_001 + LOGIN_WAIT_MS - 1),
            GateStep::Nothing
        ));
        assert!(matches!(
            gate.tick(9_001 + LOGIN_WAIT_MS),
            GateStep::Open(_)
        ));
    }

    fn hello() -> WireServerMsgBody {
        crate::commands::link::ble_pipe_fake_board::hello_body()
    }
}
