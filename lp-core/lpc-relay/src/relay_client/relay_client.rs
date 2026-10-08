//! The board's relay client: a sans-IO state machine.
//!
//! [`RelayClient`] is fed what the network, the settings, the access store
//! and the device-leg socket say ([`RelayEvent`]) with the time (`now_ms`,
//! monotonic, the caller's clock), and answers what the edge should do
//! ([`RelayAction`]). Every handler takes `now_ms`; there is no wait
//! action — the edge asks [`RelayClient::next_wake`] when to tick.
//!
//! The rules, each pinned by a test (`tests/relay_client_rules.rs`):
//!
//! - **It dials only when** the station is joined, Cloud relay is on, and
//!   the board holds at least one account key. Any of the three going away
//!   closes the leg and stops the dialling; the state says which
//!   ([`RelayState::Off`] before [`RelayState::NoAccount`] before
//!   [`RelayState::WaitingForInternet`]).
//! - **A dial** resolves the host, connects, sends the hello, answers the
//!   challenge with one proof per account, and is registered. Each step has
//!   a deadline; missing one is a failure.
//! - **Failures back off** ([`RelayBackoff`]): 1 s doubling to 60 s, ±50 %.
//!   A close with "going away" waits 2–12 s instead. Registering starts the
//!   doubling over. A failure before the leg opened says
//!   [`RelayState::WaitingForInternet`]; a drop after it says
//!   [`RelayState::Connecting`] while it waits to dial again.
//! - **Refusals:** `UnknownAccount` waits until the account entries change;
//!   a version refusal waits [`VERSION_REFUSED_RETRY_MS`] (an update, or the
//!   hub taking the version back); the rest back off as a failure would, at
//!   least as long as the hub's `retry_after_s`.
//! - **New account entries while registered** re-register, so the hub
//!   learns them.
//! - **A leg silent** for [`SILENT_CLOSE_S`](crate::SILENT_CLOSE_S) is
//!   closed as dead. The edge reports pings as [`RelayEvent::Heard`].
//! - **Routes**: at most `max_routes` open; an `Open` past that is answered
//!   `Close { Busy }` at once and never reaches the board. The leg closing
//!   closes every route.

use alloc::vec::Vec;

use super::relay_account::RelayAccount;
use super::relay_action::RelayAction;
use super::relay_backoff::RelayBackoff;
use super::relay_client_config::RelayClientConfig;
use super::relay_event::RelayEvent;
use super::relay_routes::RelayRoutes;
use super::relay_state::RelayState;
use crate::lan_address::LanAddress;
use crate::refuse_reason::RefuseReason;
use crate::relay_frame::{RelayFrame, encode_route_frame};
use crate::relay_hello::RelayHello;
use crate::relay_limits::{MAX_HELLO_ACCOUNTS, SILENT_CLOSE_S};
use crate::relay_proof::{RELAY_NONCE_BYTES, relay_auth_key, relay_proof};
use crate::route_close_reason::RouteCloseReason;

/// How long a name may take to resolve.
pub const RESOLVE_TIMEOUT_MS: u64 = 10_000;
/// How long the TCP connect and the WebSocket upgrade may take together.
pub const CONNECT_TIMEOUT_MS: u64 = 10_000;
/// How long the hub may take from the hello to `Registered`.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 10_000;
/// How long a board refused for its relay version waits before it asks
/// again: an hour.
pub const VERSION_REFUSED_RETRY_MS: u64 = 60 * 60 * 1000;

const SILENT_CLOSE_MS: u64 = SILENT_CLOSE_S as u64 * 1000;

/// See the module doc.
pub struct RelayClient {
    config: RelayClientConfig,
    entropy: fn(&mut [u8]),
    joined: bool,
    cloud_relay: bool,
    accounts: Vec<RelayAccount>,
    lan: Option<LanAddress>,
    phase: Phase,
    state: RelayState,
    backoff: RelayBackoff,
    routes: RelayRoutes,
    /// The account salts the open leg's hello named, in its order.
    registered_with: Vec<[u8; 16]>,
    /// Which of them verified, as the hub's bitmask.
    accounts_ok: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Not dialling: a precondition is missing.
    Idle,
    /// Waiting to dial at `until`.
    Waiting { until: u64 },
    /// Refused; waiting at `until` (if ever) or for the accounts to change.
    Blocked { until: Option<u64> },
    /// A name lookup is out, until `until`.
    Resolving { until: u64 },
    /// The socket is opening, until `until`.
    Connecting { until: u64 },
    /// The hello is sent; registered by `until`, or a failure.
    Registering { until: u64 },
    /// On the relay; last heard at `heard`.
    Registered { heard: u64 },
}

impl Phase {
    /// Whether the edge holds a device-leg socket (open or opening).
    const fn has_socket(self) -> bool {
        matches!(
            self,
            Self::Connecting { .. } | Self::Registering { .. } | Self::Registered { .. }
        )
    }
}

impl RelayClient {
    /// A client that does nothing until it hears it is joined, switched on
    /// and holds an account. `entropy` fills a buffer with random bytes (the
    /// board's RNG), for the backoff's jitter.
    #[must_use]
    pub fn new(config: RelayClientConfig, entropy: fn(&mut [u8])) -> Self {
        let routes = RelayRoutes::new(config.max_routes);
        Self {
            config,
            entropy,
            joined: false,
            cloud_relay: false,
            accounts: Vec::new(),
            lan: None,
            phase: Phase::Idle,
            state: RelayState::Off,
            backoff: RelayBackoff::new(),
            routes,
            registered_with: Vec::new(),
            accounts_ok: 0,
        }
    }

    /// Take one event at `now_ms`; the actions to run, in order.
    pub fn handle(&mut self, now_ms: u64, event: RelayEvent<'_>) -> Vec<RelayAction> {
        let mut actions = Vec::new();
        match event {
            RelayEvent::Network { joined } => {
                self.joined = joined;
                self.reconsider(now_ms, &mut actions);
            }
            RelayEvent::CloudRelay(on) => {
                self.cloud_relay = on;
                self.reconsider(now_ms, &mut actions);
            }
            RelayEvent::Accounts(accounts) => self.accounts_changed(now_ms, accounts, &mut actions),
            RelayEvent::Lan(lan) => {
                if lan != self.lan {
                    self.lan = lan;
                    if matches!(self.phase, Phase::Registered { .. }) {
                        actions.push(RelayAction::Send(RelayFrame::LanChanged { lan }.encode()));
                    }
                }
            }
            RelayEvent::Resolved(addr) => self.resolved(now_ms, addr, &mut actions),
            RelayEvent::Connected => self.connected(now_ms, &mut actions),
            RelayEvent::Closed { going_away } => self.closed(now_ms, going_away, &mut actions),
            RelayEvent::Message(bytes) => self.message(now_ms, bytes, &mut actions),
            RelayEvent::Heard => {
                if let Phase::Registered { heard } = &mut self.phase {
                    *heard = now_ms;
                }
            }
            RelayEvent::RouteSend { route, bytes } => self.route_send(route, bytes, &mut actions),
            RelayEvent::RouteClose { route, reason } => {
                if self.routes.close(route) && matches!(self.phase, Phase::Registered { .. }) {
                    actions.push(RelayAction::Send(
                        RelayFrame::Close { route, reason }.encode(),
                    ));
                }
            }
            RelayEvent::Tick => self.tick(now_ms, &mut actions),
        }
        actions
    }

    /// The relay's state, for status.
    #[must_use]
    pub fn state(&self) -> RelayState {
        self.state
    }

    /// When the client next needs a [`RelayEvent::Tick`]; `None` while it
    /// only waits on events.
    #[must_use]
    pub fn next_wake(&self) -> Option<u64> {
        match self.phase {
            Phase::Waiting { until }
            | Phase::Resolving { until }
            | Phase::Connecting { until }
            | Phase::Registering { until } => Some(until),
            Phase::Blocked { until } => until,
            Phase::Registered { heard } => Some(heard + SILENT_CLOSE_MS),
            Phase::Idle => None,
        }
    }

    /// The configuration the client was built with.
    #[must_use]
    pub fn config(&self) -> &RelayClientConfig {
        &self.config
    }

    /// The routes open now.
    #[must_use]
    pub fn routes(&self) -> &RelayRoutes {
        &self.routes
    }

    /// Whether the board may dial now: joined, Cloud relay on, and holding
    /// an account entry (RD8). While it is false the client never asks for
    /// a socket, so an edge may give the device leg's buffers back.
    #[must_use]
    pub fn may_dial(&self) -> bool {
        self.precondition().is_none()
    }

    /// The hub's verdict on each account of the last registration: bit `i`
    /// set when the hello's account `i` verified.
    #[must_use]
    pub fn accounts_ok(&self) -> u8 {
        self.accounts_ok
    }

    /// The state the preconditions alone allow, if they forbid dialling.
    fn precondition(&self) -> Option<RelayState> {
        if !self.cloud_relay {
            Some(RelayState::Off)
        } else if self.accounts.is_empty() {
            Some(RelayState::NoAccount)
        } else if !self.joined {
            Some(RelayState::WaitingForInternet)
        } else {
            None
        }
    }

    /// A precondition changed: stop, or start dialling if idle.
    fn reconsider(&mut self, now: u64, actions: &mut Vec<RelayAction>) {
        if let Some(state) = self.precondition() {
            self.drop_leg(actions);
            self.phase = Phase::Idle;
            self.state = state;
            return;
        }
        if self.phase == Phase::Idle {
            self.backoff.reset();
            self.state = RelayState::Connecting;
            self.dial(now, actions);
        }
    }

    fn accounts_changed(
        &mut self,
        now: u64,
        accounts: Vec<RelayAccount>,
        actions: &mut Vec<RelayAction>,
    ) {
        if accounts == self.accounts {
            return;
        }
        self.accounts = accounts;
        if self.precondition().is_some() {
            self.reconsider(now, actions);
            return;
        }
        // New entries: whatever the leg was doing, register again with them
        // (an `UnknownAccount` refusal was waiting for exactly this).
        let salts = self.hello_salts();
        let current = matches!(
            self.phase,
            Phase::Registering { .. } | Phase::Registered { .. }
        ) && salts == self.registered_with;
        if current {
            return;
        }
        self.drop_leg(actions);
        self.backoff.reset();
        self.dial(now, actions);
    }

    fn dial(&mut self, now: u64, actions: &mut Vec<RelayAction>) {
        self.phase = Phase::Resolving {
            until: now + RESOLVE_TIMEOUT_MS,
        };
        if !matches!(self.state, RelayState::WaitingForInternet) {
            self.state = RelayState::Connecting;
        }
        actions.push(RelayAction::Resolve {
            host: self.config.host.clone(),
        });
    }

    fn resolved(&mut self, now: u64, addr: Option<[u8; 4]>, actions: &mut Vec<RelayAction>) {
        if !matches!(self.phase, Phase::Resolving { .. }) {
            return;
        }
        match addr {
            Some(addr) => {
                self.phase = Phase::Connecting {
                    until: now + CONNECT_TIMEOUT_MS,
                };
                actions.push(RelayAction::Connect {
                    addr,
                    port: self.config.port,
                });
            }
            None => self.fail_unreached(now),
        }
    }

    fn connected(&mut self, now: u64, actions: &mut Vec<RelayAction>) {
        if !matches!(self.phase, Phase::Connecting { .. }) {
            // A socket nobody asks for any more.
            actions.push(RelayAction::Close);
            return;
        }
        let salts = self.hello_salts();
        let hello = RelayHello::new(
            self.config.board_mac,
            &self.config.label,
            self.config.wire_proto,
            self.lan,
            salts.clone(),
        );
        self.registered_with = salts;
        self.phase = Phase::Registering {
            until: now + HANDSHAKE_TIMEOUT_MS,
        };
        self.state = RelayState::Connecting;
        actions.push(RelayAction::Send(RelayFrame::Hello(hello).encode()));
    }

    fn closed(&mut self, now: u64, going_away: bool, actions: &mut Vec<RelayAction>) {
        if !self.phase.has_socket() {
            return;
        }
        let was_open = !matches!(self.phase, Phase::Connecting { .. });
        self.close_routes(actions);
        if going_away {
            let wait = self.backoff.after_going_away(self.random());
            self.phase = Phase::Waiting { until: now + wait };
            self.state = RelayState::Connecting;
        } else if was_open {
            self.fail_dropped(now);
        } else {
            self.fail_unreached(now);
        }
    }

    fn message(&mut self, now: u64, bytes: &[u8], actions: &mut Vec<RelayAction>) {
        let frame = match RelayFrame::decode(bytes) {
            Ok(frame) => frame,
            Err(_) if self.phase.has_socket() => {
                self.protocol_error(now, actions);
                return;
            }
            Err(_) => return,
        };
        match (self.phase, frame) {
            (Phase::Registering { .. }, RelayFrame::Challenge { nonce }) => {
                actions.push(RelayAction::Send(self.proof(&nonce).encode()));
            }
            (Phase::Registering { .. }, RelayFrame::Registered { accounts_ok, .. }) => {
                self.accounts_ok = accounts_ok;
                self.backoff.reset();
                self.phase = Phase::Registered { heard: now };
                self.state = RelayState::Connected;
            }
            (
                Phase::Registering { .. } | Phase::Registered { .. },
                RelayFrame::Refused {
                    reason,
                    retry_after_s,
                },
            ) => self.refused(now, reason, retry_after_s, actions),
            (Phase::Registered { .. }, frame) => {
                self.phase = Phase::Registered { heard: now };
                self.registered_frame(now, frame, actions);
            }
            (phase, _) if phase.has_socket() => self.protocol_error(now, actions),
            _ => {}
        }
    }

    fn registered_frame(&mut self, now: u64, frame: RelayFrame, actions: &mut Vec<RelayAction>) {
        match frame {
            RelayFrame::Open { route } => {
                if self.routes.open(route) {
                    actions.push(RelayAction::RouteOpened(route));
                } else {
                    actions.push(RelayAction::Send(
                        RelayFrame::Close {
                            route,
                            reason: RouteCloseReason::Busy,
                        }
                        .encode(),
                    ));
                }
            }
            RelayFrame::Frame { route, bytes } => {
                if self.routes.contains(route) {
                    actions.push(RelayAction::RouteFrame { route, bytes });
                }
                // A frame for a route the board does not hold raced its
                // close; the close already said so.
            }
            RelayFrame::Close { route, .. } => {
                if self.routes.close(route) {
                    actions.push(RelayAction::RouteClosed(route));
                }
            }
            _ => self.protocol_error(now, actions),
        }
    }

    fn refused(
        &mut self,
        now: u64,
        reason: RefuseReason,
        retry_after_s: u16,
        actions: &mut Vec<RelayAction>,
    ) {
        actions.push(RelayAction::Close);
        self.close_routes(actions);
        self.state = RelayState::Refused { reason };
        let hub_wait = u64::from(retry_after_s) * 1000;
        self.phase = match reason {
            RefuseReason::UnknownAccount => Phase::Blocked { until: None },
            RefuseReason::VersionTooOld | RefuseReason::VersionTooNew => Phase::Blocked {
                until: Some(now + VERSION_REFUSED_RETRY_MS.max(hub_wait)),
            },
            RefuseReason::TooManyBoards | RefuseReason::Malformed | RefuseReason::Busy => {
                let wait = self.backoff.after_failure(self.random()).max(hub_wait);
                Phase::Waiting { until: now + wait }
            }
        };
    }

    fn route_send(&mut self, route: u16, bytes: &[u8], actions: &mut Vec<RelayAction>) {
        if !matches!(self.phase, Phase::Registered { .. }) || !self.routes.contains(route) {
            return;
        }
        if RelayFrame::fits(bytes.len()) {
            actions.push(RelayAction::Send(encode_route_frame(route, bytes)));
        } else {
            // No lp-link frame is this big; a link that sends one is broken.
            self.routes.close(route);
            actions.push(RelayAction::Send(
                RelayFrame::Close {
                    route,
                    reason: RouteCloseReason::Normal,
                }
                .encode(),
            ));
            actions.push(RelayAction::RouteClosed(route));
        }
    }

    fn tick(&mut self, now: u64, actions: &mut Vec<RelayAction>) {
        match self.phase {
            Phase::Waiting { until } if now >= until => self.dial(now, actions),
            Phase::Blocked { until: Some(until) } if now >= until => self.dial(now, actions),
            Phase::Resolving { until } if now >= until => self.fail_unreached(now),
            Phase::Connecting { until } if now >= until => {
                actions.push(RelayAction::Close);
                self.fail_unreached(now);
            }
            Phase::Registering { until } if now >= until => {
                actions.push(RelayAction::Close);
                self.fail_dropped(now);
            }
            Phase::Registered { heard } if now >= heard + SILENT_CLOSE_MS => {
                actions.push(RelayAction::Close);
                self.close_routes(actions);
                self.fail_dropped(now);
            }
            _ => {}
        }
    }

    /// A frame out of turn, or bytes that are not a frame: close and back
    /// off.
    fn protocol_error(&mut self, now: u64, actions: &mut Vec<RelayAction>) {
        actions.push(RelayAction::Close);
        self.close_routes(actions);
        self.fail_dropped(now);
    }

    /// The relay could not be reached: back off, "waiting for internet".
    fn fail_unreached(&mut self, now: u64) {
        let wait = self.backoff.after_failure(self.random());
        self.phase = Phase::Waiting { until: now + wait };
        self.state = RelayState::WaitingForInternet;
    }

    /// The leg dropped after it opened: back off, "connecting".
    fn fail_dropped(&mut self, now: u64) {
        let wait = self.backoff.after_failure(self.random());
        self.phase = Phase::Waiting { until: now + wait };
        self.state = RelayState::Connecting;
    }

    /// Close the leg if there is one, and every route with it.
    fn drop_leg(&mut self, actions: &mut Vec<RelayAction>) {
        if self.phase.has_socket() {
            actions.push(RelayAction::Close);
        }
        self.close_routes(actions);
    }

    fn close_routes(&mut self, actions: &mut Vec<RelayAction>) {
        for route in self.routes.take_all() {
            actions.push(RelayAction::RouteClosed(route));
        }
    }

    /// The salts the next hello names: the first
    /// [`MAX_HELLO_ACCOUNTS`] account entries.
    fn hello_salts(&self) -> Vec<[u8; 16]> {
        self.accounts
            .iter()
            .take(MAX_HELLO_ACCOUNTS)
            .map(|account| account.salt)
            .collect()
    }

    /// One proof per salt of the hello that opened this leg, in its order.
    fn proof(&self, nonce: &[u8; RELAY_NONCE_BYTES]) -> RelayFrame {
        let proofs = self
            .registered_with
            .iter()
            .map(|salt| {
                // The accounts have not changed since the hello, or the leg
                // would have been closed and dialled again.
                self.accounts
                    .iter()
                    .find(|account| account.salt == *salt)
                    .map_or([0; 32], |account| {
                        relay_proof(&relay_auth_key(&account.k), nonce, &self.config.board_mac)
                    })
            })
            .collect();
        RelayFrame::Proof { proofs }
    }

    fn random(&self) -> u32 {
        let mut bytes = [0u8; 4];
        (self.entropy)(&mut bytes);
        u32::from_le_bytes(bytes)
    }
}
