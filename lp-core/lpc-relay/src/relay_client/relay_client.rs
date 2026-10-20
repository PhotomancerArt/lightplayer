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
//!   `VersionTooOld` waits [`VERSION_REFUSED_RETRY_MS`] (an hour: the board
//!   needs an update); `VersionTooNew` waits [`VERSION_TOO_NEW_RETRY_MS`]
//!   (five minutes: the hub is behind the board, a deploy in progress or a
//!   rollback); the rest back off as a failure would. Each waits at least
//!   as long as the hub's `retry_after_s`.
//! - **New account entries while registered** re-register, so the hub
//!   learns them.
//! - **A leg silent** for [`SILENT_CLOSE_S`](crate::SILENT_CLOSE_S) is
//!   closed as dead. The edge reports pings as [`RelayEvent::Heard`].
//! - **Routes**: at most `max_routes` open; an `Open` past that is answered
//!   `Close { Busy }` at once and never reaches the board. The leg closing
//!   closes every route.
//! - **The hello is protocol 2** and carries the configured firmware
//!   version ([`RelayClientConfig::firmware`]).
//! - **The project** ([`RelayEvent::Project`]) goes to the hub after every
//!   `Registered` (once the board has said it) and on every change while
//!   registered: its name, cut to 32 bytes, and its uid and package hash
//!   only as tags ([`crate::relay_project`]) keyed by the first verified
//!   account (the lowest set bit of `accounts_ok`). The facts are kept
//!   across reconnects.
//! - **Pictures** follow the hub's [`PictureRate`](crate::PictureRate)
//!   (the schedule's rules are in `picture_schedule.rs`): none before the
//!   first rate after a `Registered`; each rate asks for one at once
//!   ([`RelayAction::TakePicture`]), then its cadence, watched then idle;
//!   at most one in flight. A [`RelayEvent::PictureReady`] while
//!   registered with a picture asked for is sent
//!   ([`RelayAction::SendPicture`]); any other is dropped
//!   ([`RelayAction::DropPicture`]). Every `Registered`, and the leg
//!   going, clears the schedule.
//! - **Never sent:** the project's uid and its package hash. Only their
//!   tags cross the leg.

use alloc::vec::Vec;

use alloc::string::String;

use super::picture_schedule::PictureSchedule;
use super::relay_account::RelayAccount;
use super::relay_action::RelayAction;
use super::relay_backoff::RelayBackoff;
use super::relay_client_config::RelayClientConfig;
use super::relay_event::RelayEvent;
use super::relay_project_facts::RelayProjectFacts;
use super::relay_routes::RelayRoutes;
use super::relay_state::RelayState;
use crate::lan_address::LanAddress;
use crate::refuse_reason::RefuseReason;
use crate::relay_frame::{RelayFrame, encode_route_frame};
use crate::relay_hello::{RelayHello, cut_utf8};
use crate::relay_limits::{MAX_HELLO_ACCOUNTS, MAX_PROJECT_NAME_BYTES, SILENT_CLOSE_S};
use crate::relay_project::{RelayProject, project_content_tag, project_tag_key, project_uid_tag};
use crate::relay_proof::{RELAY_NONCE_BYTES, relay_auth_key, relay_proof};
use crate::route_close_reason::RouteCloseReason;

/// How long a name may take to resolve.
pub const RESOLVE_TIMEOUT_MS: u64 = 10_000;
/// How long the TCP connect and the WebSocket upgrade may take together.
pub const CONNECT_TIMEOUT_MS: u64 = 10_000;
/// How long the hub may take from the hello to `Registered`.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 10_000;
/// How long a board refused `VersionTooOld` waits before it asks again: an
/// hour. The board needs an update.
pub const VERSION_REFUSED_RETRY_MS: u64 = 60 * 60 * 1000;
/// How long a board refused `VersionTooNew` waits before it asks again:
/// five minutes. A hub that does not know the board's protocol is behind
/// it (a deploy in progress, a rollback), which passes.
pub const VERSION_TOO_NEW_RETRY_MS: u64 = 5 * 60 * 1000;

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
    /// The board's project as the edge last said; `None` until it says.
    /// Kept across reconnects.
    project: Option<Option<RelayProjectFacts>>,
    /// When the next picture is due.
    pictures: PictureSchedule,
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
            project: None,
            pictures: PictureSchedule::new(),
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
                        actions.push(RelayAction::Send(
                            RelayFrame::LanChanged { lan }.encode_to_hub(),
                        ));
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
            RelayEvent::Project(facts) => {
                self.project = Some(facts);
                self.send_project(&mut actions);
            }
            RelayEvent::PictureReady => {
                let wanted = self.pictures.ready();
                actions.push(
                    if wanted && matches!(self.phase, Phase::Registered { .. }) {
                        RelayAction::SendPicture
                    } else {
                        RelayAction::DropPicture
                    },
                );
            }
            RelayEvent::RouteClose { route, reason } => {
                if self.routes.close(route) && matches!(self.phase, Phase::Registered { .. }) {
                    actions.push(RelayAction::Send(
                        RelayFrame::Close { route, reason }.encode_to_hub(),
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
            Phase::Registered { heard } => {
                let silent = heard + SILENT_CLOSE_MS;
                Some(
                    self.pictures
                        .next_wake()
                        .map_or(silent, |at| at.min(silent)),
                )
            }
            Phase::Idle => None,
        }
    }

    /// When the next picture is due, while the hub has asked for pictures.
    #[must_use]
    pub fn next_picture_due(&self) -> Option<u64> {
        self.pictures.next_due()
    }

    /// Whether the board counts as watched at `now_ms`: the hub's last rate
    /// asked for watched pictures, and its watch has not run out.
    #[must_use]
    pub fn pictures_watched(&self, now_ms: u64) -> bool {
        self.pictures.is_watched(now_ms)
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
        )
        .with_firmware(self.config.firmware);
        self.registered_with = salts;
        self.phase = Phase::Registering {
            until: now + HANDSHAKE_TIMEOUT_MS,
        };
        self.state = RelayState::Connecting;
        actions.push(RelayAction::Send(RelayFrame::Hello(hello).encode_to_hub()));
    }

    fn closed(&mut self, now: u64, going_away: bool, actions: &mut Vec<RelayAction>) {
        if !self.phase.has_socket() {
            return;
        }
        let was_open = !matches!(self.phase, Phase::Connecting { .. });
        self.leg_gone(actions);
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
        let frame = match RelayFrame::decode_from_hub(bytes) {
            Ok(frame) => frame,
            Err(_) if self.phase.has_socket() => {
                self.protocol_error(now, actions);
                return;
            }
            Err(_) => return,
        };
        match (self.phase, frame) {
            (Phase::Registering { .. }, RelayFrame::Challenge { nonce }) => {
                actions.push(RelayAction::Send(self.proof(&nonce).encode_to_hub()));
            }
            (Phase::Registering { .. }, RelayFrame::Registered { accounts_ok, .. }) => {
                self.accounts_ok = accounts_ok;
                self.backoff.reset();
                self.phase = Phase::Registered { heard: now };
                self.state = RelayState::Connected;
                // No pictures until this hub asks.
                self.pictures.clear();
                self.send_project(actions);
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
                        .encode_to_hub(),
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
            RelayFrame::PictureRate(rate) => {
                if self.pictures.rate(now, rate) {
                    actions.push(RelayAction::TakePicture);
                }
            }
            // `Project` and `Picture` only travel board → hub.
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
        self.leg_gone(actions);
        self.state = RelayState::Refused { reason };
        let hub_wait = u64::from(retry_after_s) * 1000;
        self.phase = match reason {
            RefuseReason::UnknownAccount => Phase::Blocked { until: None },
            RefuseReason::VersionTooOld => Phase::Blocked {
                until: Some(now + VERSION_REFUSED_RETRY_MS.max(hub_wait)),
            },
            RefuseReason::VersionTooNew => Phase::Blocked {
                until: Some(now + VERSION_TOO_NEW_RETRY_MS.max(hub_wait)),
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
                .encode_to_hub(),
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
                self.leg_gone(actions);
                self.fail_dropped(now);
            }
            Phase::Registered { .. } => {
                if self.pictures.tick(now) {
                    actions.push(RelayAction::TakePicture);
                }
            }
            _ => {}
        }
    }

    /// A frame out of turn, or bytes that are not a frame: close and back
    /// off.
    fn protocol_error(&mut self, now: u64, actions: &mut Vec<RelayAction>) {
        actions.push(RelayAction::Close);
        self.leg_gone(actions);
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
        self.leg_gone(actions);
    }

    /// The leg is gone (or going): every route closes, and no picture is
    /// due until the next hub asks.
    fn leg_gone(&mut self, actions: &mut Vec<RelayAction>) {
        for route in self.routes.take_all() {
            actions.push(RelayAction::RouteClosed(route));
        }
        self.pictures.clear();
    }

    /// Tell the hub the board's project, if the board has said it and the
    /// leg is registered.
    fn send_project(&self, actions: &mut Vec<RelayAction>) {
        if !matches!(self.phase, Phase::Registered { .. }) {
            return;
        }
        let Some(facts) = &self.project else {
            return;
        };
        let project = facts.as_ref().map(|facts| self.project_frame(facts));
        actions.push(RelayAction::Send(
            RelayFrame::Project(project).encode_to_hub(),
        ));
    }

    /// The facts as the hub may see them: the name, and the uid and the
    /// hash only as tags.
    fn project_frame(&self, facts: &RelayProjectFacts) -> RelayProject {
        let tag_key = self
            .tag_account()
            .map(|account| project_tag_key(&account.k));
        RelayProject {
            name: String::from(cut_utf8(&facts.name, MAX_PROJECT_NAME_BYTES)),
            uid_tag: tag_key
                .zip(facts.uid.as_deref())
                .map(|(key, uid)| project_uid_tag(&key, uid)),
            content_tag: tag_key
                .zip(facts.content_hash.as_ref())
                .map(|(key, hash)| project_content_tag(&key, hash)),
        }
    }

    /// The account the project tags are keyed with: the first verified one
    /// (the lowest set bit of `accounts_ok`, indexing the hello's salts),
    /// which the hub knows as the first of the board's proven accounts.
    fn tag_account(&self) -> Option<&RelayAccount> {
        let first = self.accounts_ok.trailing_zeros() as usize;
        let salt = self.registered_with.get(first)?;
        self.accounts.iter().find(|account| account.salt == *salt)
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
