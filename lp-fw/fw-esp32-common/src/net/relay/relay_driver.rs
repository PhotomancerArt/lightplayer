//! The relay driver: the glue between `lpc-relay`'s board client (when to
//! dial, the registration, the route table) and the board's network slot
//! (the one secure session it shares with the LAN; Wi-Fi relay plan P6).
//!
//! Sans-IO. The edge — the C6's relay task on `lp-net`, the host harness's
//! relay thread — owns the device leg's socket and the clock; it feeds the
//! driver what the leg, the network and the settings say
//! ([`RelayDriver::handle`]), runs what it asks ([`RelayDriverAction`]), and
//! carries the route's outgoing frames ([`RelayDriver::route_frame`]). The
//! driver itself touches only the port's slot, under the port's lock.
//!
//! A route the hub opens becomes a network link trusted as
//! [`LinkTrust::Relayed`] (so the board's "Anyone" setting never applies to
//! it; `docs/adr/2026-10-06-cloud-relay.md` §5):
//!
//! - **the slot is free** → the route's link opens on it at once;
//! - **the slot is held** (a LAN session, say) → the route is a *challenge*:
//!   its first frame is parked and the mux decides
//!   (`radio_link::parked_handshake`). Taken over, the route opens on the
//!   slot with the parked frame first; refused, the route is closed `Busy`;
//! - **the slot's link is revoked** (the mux closed it: the login deadline,
//!   a reset session, a LAN client with the same key taking the slot over)
//!   → the route is closed and the hub told so;
//! - **the hub closes the route** (the browser left, the leg dropped) → the
//!   link is closed.
//!
//! One frame on a route is one lp-link frame; nothing here reads inside one.

use alloc::vec::Vec;

use lp_link::Micros;
use lpc_relay::{
    RelayAction, RelayClient, RelayClientConfig, RelayEvent, RelayState, RouteCloseReason,
    route_frame_header,
};
use lpc_shared::transport::LinkTrust;

use super::relay_driver_action::RelayDriverAction;
use super::relay_route_link::{RelayRouteLink, RouteSlotState};
use crate::radio_link::{ChallengeVerdict, RadioLinkEvent, RadioLinkPort, SlotEdge};

/// How long a route that found the slot held waits for its first frame and
/// the mux's verdict, µs: past lp-link's own 2 s key-lookup limit, with
/// room for the server loop's tick.
pub const CHALLENGE_WAIT_US: u64 = 5_000_000;

/// What the driver counted, for the heartbeat's `[relay]` line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayCounters {
    /// Bytes of device-leg messages received (the edge notes them).
    pub rx_bytes: u64,
    /// Bytes of device-leg messages sent (the edge notes them).
    pub tx_bytes: u64,
    /// Routes that held the network slot.
    pub routes: u32,
    /// Of them, routes that took it over from a LAN session.
    pub takeovers: u32,
    /// Routes turned away busy.
    pub busy: u32,
}

/// See the module doc.
pub struct RelayDriver {
    client: RelayClient,
    port: &'static RadioLinkPort,
    index: usize,
    entropy: fn(&mut [u8]),
    route: Option<RelayRouteLink>,
    actions: Vec<RelayDriverAction>,
    counters: RelayCounters,
}

impl RelayDriver {
    /// A driver for the board described by `config`, its routes on network
    /// slot `index` of `port`. `entropy` fills a buffer with fresh random
    /// bytes (the backoff's jitter, each link's nonce and handshakes).
    #[must_use]
    pub fn new(
        config: RelayClientConfig,
        entropy: fn(&mut [u8]),
        port: &'static RadioLinkPort,
        index: usize,
    ) -> Self {
        Self {
            client: RelayClient::new(config, entropy),
            port,
            index,
            entropy,
            route: None,
            actions: Vec::new(),
            counters: RelayCounters::default(),
        }
    }

    /// Take one event at `now_us` (the device clock, µs): the network, the
    /// settings, the access store, or the device leg.
    pub fn handle(&mut self, now_us: Micros, event: RelayEvent<'_>) {
        if let RelayEvent::Message(bytes) = &event {
            self.counters.rx_bytes += bytes.len() as u64;
        }
        let actions = self.client.handle(ms(now_us), event);
        self.apply(now_us, actions);
    }

    /// What to do now, in order.
    pub fn take_actions(&mut self) -> Vec<RelayDriverAction> {
        core::mem::take(&mut self.actions)
    }

    /// Whether there is anything to do.
    #[must_use]
    pub fn has_actions(&self) -> bool {
        !self.actions.is_empty()
    }

    /// The edge sent `len` bytes on the device leg.
    pub fn note_sent(&mut self, len: usize) {
        self.counters.tx_bytes += len as u64;
    }

    /// The route's next outgoing frame as one device-leg message, written
    /// into `out` (at least `ROUTE_FRAME_OVERHEAD` + the largest network
    /// frame); its length, or `None` when there is nothing to send.
    pub fn route_frame(&mut self, now_us: Micros, out: &mut [u8]) -> Option<usize> {
        let route = self.route?;
        if route.state != RouteSlotState::Holding || self.client.state() != RelayState::Connected {
            return None;
        }
        let polled = self
            .port
            .slot(self.index)
            .poll_frame_for(route.id, now_us, |frame| {
                let header = route_frame_header(route.route);
                let len = header.len() + frame.len();
                let out = out.get_mut(..len)?;
                out[..header.len()].copy_from_slice(&header);
                out[header.len()..].copy_from_slice(frame);
                Some(len)
            });
        match polled {
            Ok(Some(Some(len))) => Some(len),
            Ok(Some(None)) => {
                log::error!(
                    "[relay] route {}: a frame larger than the leg's buffer was dropped",
                    route.route
                );
                None
            }
            Ok(None) => None,
            Err(_) => {
                self.lost(now_us, "the board closed it");
                None
            }
        }
    }

    /// The mux asked the relay's edge to drop its link (`reason`).
    pub fn on_close_request(&mut self, now_us: Micros, reason: &str) {
        let Some(route) = self.route else {
            return;
        };
        if route.state != RouteSlotState::Holding {
            return;
        }
        self.port.slot(self.index).close_link(route.id);
        log::info!("[relay] route {}: closed ({reason})", route.route);
        self.route = None;
        self.close_route(now_us, route.route, RouteCloseReason::Normal);
        self.actions
            .push(RelayDriverAction::Announce(RadioLinkEvent::Closed {
                link: route.id,
            }));
    }

    /// Whether the edge should wait on the network slot's verdict: the
    /// route is a challenge whose first frame is parked.
    #[must_use]
    pub fn awaits_verdict(&self) -> bool {
        matches!(
            self.route,
            Some(RelayRouteLink {
                state: RouteSlotState::Challenging { parked: true, .. },
                ..
            })
        )
    }

    /// The mux's verdict on the route's challenge.
    pub fn on_verdict(&mut self, now_us: Micros, verdict: ChallengeVerdict) {
        if !self.awaits_verdict() {
            return;
        }
        let Some(mut route) = self.route else {
            return;
        };
        if verdict == ChallengeVerdict::TakeOver {
            let taken = self.port.slot(self.index).take_over(
                route.id,
                now_us,
                self.nonce(),
                self.entropy,
                LinkTrust::Relayed,
            );
            if taken.is_ok() {
                log::info!(
                    "[relay] route {}: took the network link over (the same key)",
                    route.route
                );
                route.state = RouteSlotState::Holding;
                self.route = Some(route);
                self.counters.routes += 1;
                self.counters.takeovers += 1;
                self.actions
                    .push(RelayDriverAction::Announce(RadioLinkEvent::Opened {
                        link: route.id,
                        slot: self.index,
                    }));
                return;
            }
        }
        self.busy(now_us);
    }

    /// Time passed: the client's deadlines, and a challenge's.
    pub fn tick(&mut self, now_us: Micros) {
        self.handle(now_us, RelayEvent::Tick);
        if let Some(RelayRouteLink {
            state: RouteSlotState::Challenging { until, .. },
            ..
        }) = self.route
            && now_us >= until
        {
            self.busy(now_us);
        }
    }

    /// When the driver next needs [`Self::tick`] or [`Self::route_frame`],
    /// µs; `None` while it only waits on events.
    #[must_use]
    pub fn next_wake_us(&self) -> Option<Micros> {
        let client = self.client.next_wake().map(|at| at.saturating_mul(1_000));
        let route = self.route.and_then(|route| match route.state {
            RouteSlotState::Holding => self.port.slot(self.index).poll_timeout_for(route.id),
            RouteSlotState::Challenging { until, .. } => Some(until),
        });
        match (client, route) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Whether the board may dial now (joined, Cloud relay on, an account
    /// entry held: RD8). While it is false the edge holds no device-leg
    /// buffers ([`super::relay_leg::run_relay_leg`] returns `Idle`).
    #[must_use]
    pub fn may_dial(&self) -> bool {
        self.client.may_dial()
    }

    /// The relay's state, for status.
    #[must_use]
    pub fn state(&self) -> RelayState {
        self.client.state()
    }

    /// The route the board holds (or is challenging with), if any.
    #[must_use]
    pub fn route(&self) -> Option<RelayRouteLink> {
        self.route
    }

    /// What the driver counted.
    #[must_use]
    pub fn counters(&self) -> RelayCounters {
        self.counters
    }

    /// The client's configuration.
    #[must_use]
    pub fn config(&self) -> &RelayClientConfig {
        self.client.config()
    }

    fn apply(&mut self, now_us: Micros, actions: Vec<RelayAction>) {
        for action in actions {
            match action {
                RelayAction::Resolve { host } => {
                    self.actions.push(RelayDriverAction::Resolve { host });
                }
                RelayAction::Connect { addr, port } => {
                    self.actions.push(RelayDriverAction::Connect { addr, port });
                }
                RelayAction::Send(bytes) => self.actions.push(RelayDriverAction::Send(bytes)),
                RelayAction::Close => self.actions.push(RelayDriverAction::Close),
                RelayAction::RouteOpened(route) => self.open_route(now_us, route),
                RelayAction::RouteFrame { route, bytes } => self.route_in(now_us, route, &bytes),
                RelayAction::RouteClosed(route) => self.route_gone(route),
                // This board has no picture source yet: it never answers a
                // `TakePicture`, so the client never asks it to send one.
                RelayAction::TakePicture | RelayAction::SendPicture | RelayAction::DropPicture => {}
            }
        }
    }

    /// The hub opened `route`: the slot's, or a challenge for it.
    fn open_route(&mut self, now_us: Micros, route: u16) {
        if self.route.is_some() {
            // The client holds one route (`max_routes` 1), so the hub
            // never gets this far; turn it away if it does.
            self.close_route(now_us, route, RouteCloseReason::Busy);
            return;
        }
        let id = self.port.mint_link();
        let opened = self.port.slot(self.index).open_network(
            id,
            self.nonce(),
            self.entropy,
            LinkTrust::Relayed,
            SlotEdge::Relay,
        );
        match opened {
            Ok(payload) => {
                log::info!(
                    "[relay] route {route}: link {id}, secure session opening ({payload} B frames)"
                );
                self.route = Some(RelayRouteLink {
                    route,
                    id,
                    state: RouteSlotState::Holding,
                });
                self.counters.routes += 1;
                self.actions
                    .push(RelayDriverAction::Announce(RadioLinkEvent::Opened {
                        link: id,
                        slot: self.index,
                    }));
            }
            Err(_) => {
                log::info!("[relay] route {route}: the network link is held — asking for it");
                self.route = Some(RelayRouteLink {
                    route,
                    id,
                    state: RouteSlotState::Challenging {
                        parked: false,
                        until: now_us + CHALLENGE_WAIT_US,
                    },
                });
            }
        }
    }

    /// One lp-link frame from the browser on `route`.
    fn route_in(&mut self, now_us: Micros, route: u16, bytes: &[u8]) {
        let Some(mut held) = self.route.filter(|held| held.route == route) else {
            return;
        };
        let slot = self.port.slot(self.index);
        match held.state {
            RouteSlotState::Holding => {
                if slot.on_datagram_for(held.id, now_us, bytes).is_err() {
                    self.lost(now_us, "the board closed it");
                }
            }
            RouteSlotState::Challenging {
                parked: false,
                until,
            } => {
                if slot.park_challenge(held.id, SlotEdge::Relay, bytes).is_ok() {
                    held.state = RouteSlotState::Challenging {
                        parked: true,
                        until,
                    };
                    self.route = Some(held);
                    self.actions
                        .push(RelayDriverAction::Announce(RadioLinkEvent::Challenged {
                            link: held.id,
                            slot: self.index,
                        }));
                } else {
                    self.busy(now_us);
                }
            }
            // A resent SYN: the parked one stands for it.
            RouteSlotState::Challenging { parked: true, .. } => {}
        }
    }

    /// The hub closed `route` (or the leg dropped).
    fn route_gone(&mut self, route: u16) {
        let Some(held) = self.route.filter(|held| held.route == route) else {
            return;
        };
        self.route = None;
        let slot = self.port.slot(self.index);
        match held.state {
            RouteSlotState::Holding => {
                slot.close_link(held.id);
            }
            RouteSlotState::Challenging { parked, .. } => {
                slot.withdraw_challenge(held.id);
                if !parked {
                    return;
                }
            }
        }
        log::info!("[relay] route {route}: closed by the hub");
        self.actions
            .push(RelayDriverAction::Announce(RadioLinkEvent::Closed {
                link: held.id,
            }));
    }

    /// The slot no longer holds the route's link (the mux revoked it, or a
    /// LAN client with the same key took it over): close the route.
    fn lost(&mut self, now_us: Micros, why: &str) {
        let Some(held) = self.route.take() else {
            return;
        };
        let reason: &str = match self
            .port
            .slot(self.index)
            .try_close_request_for(SlotEdge::Relay)
        {
            Some(reason) => reason,
            None => why,
        };
        log::info!("[relay] route {}: closed ({reason})", held.route);
        self.close_route(now_us, held.route, RouteCloseReason::Normal);
        self.actions
            .push(RelayDriverAction::Announce(RadioLinkEvent::Closed {
                link: held.id,
            }));
    }

    /// Turn the route's challenge away: busy.
    fn busy(&mut self, now_us: Micros) {
        let Some(held) = self.route.take() else {
            return;
        };
        self.counters.busy += 1;
        if let RouteSlotState::Challenging { parked, .. } = held.state {
            self.port.slot(self.index).withdraw_challenge(held.id);
            if parked {
                self.actions
                    .push(RelayDriverAction::Announce(RadioLinkEvent::Closed {
                        link: held.id,
                    }));
            }
        }
        log::info!(
            "[relay] route {}: the network link is in use — busy",
            held.route
        );
        self.close_route(now_us, held.route, RouteCloseReason::Busy);
    }

    /// Tell the hub the board closed `route`.
    fn close_route(&mut self, now_us: Micros, route: u16, reason: RouteCloseReason) {
        let actions = self
            .client
            .handle(ms(now_us), RelayEvent::RouteClose { route, reason });
        self.apply(now_us, actions);
    }

    fn nonce(&self) -> u32 {
        let mut bytes = [0u8; 4];
        (self.entropy)(&mut bytes);
        u32::from_le_bytes(bytes) | 1
    }
}

fn ms(now_us: Micros) -> u64 {
    now_us / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use lpc_relay::{RelayAccount, RelayFrame, RouteCloseReason};

    use crate::radio_link::RADIO_LINK_SLOTS;

    /// It dials only when joined, Cloud relay on and an account held — the
    /// C6's rule (RD8), whichever order they arrive in — and says why not.
    #[test]
    fn it_dials_only_when_joined_switched_on_and_holding_an_account() {
        let mut driver = driver();
        driver.handle(0, RelayEvent::Network { joined: true });
        driver.handle(0, RelayEvent::Accounts(vec![account()]));
        assert!(!driver.has_actions());
        assert_eq!(driver.state(), RelayState::Off, "Cloud relay off");

        let mut driver = self::driver();
        driver.handle(0, RelayEvent::CloudRelay(true));
        driver.handle(0, RelayEvent::Network { joined: true });
        assert!(!driver.has_actions());
        assert_eq!(driver.state(), RelayState::NoAccount);

        let mut driver = self::driver();
        driver.handle(0, RelayEvent::CloudRelay(true));
        driver.handle(0, RelayEvent::Accounts(vec![account()]));
        assert!(!driver.has_actions());
        assert_eq!(driver.state(), RelayState::WaitingForInternet, "not joined");
        assert!(!driver.may_dial());
        driver.handle(0, RelayEvent::Network { joined: true });
        assert!(driver.may_dial());
        assert_eq!(
            driver.take_actions(),
            [RelayDriverAction::Resolve {
                host: String::from("relay.test")
            }]
        );
        assert_eq!(driver.state(), RelayState::Connecting);
    }

    /// A route opened on a free slot is a relayed link on it, announced;
    /// the hub closing it frees the slot and says so.
    #[test]
    fn a_route_on_a_free_slot_is_a_relayed_link_and_its_close_frees_it() {
        let mut driver = driver();
        register(&mut driver);
        let opened = RelayFrame::Open { route: 9 }.encode();
        driver.handle(1_000, RelayEvent::Message(&opened));
        let actions = driver.take_actions();
        let [RelayDriverAction::Announce(RadioLinkEvent::Opened { link, slot })] = actions[..]
        else {
            panic!("{actions:?}");
        };
        assert_eq!(slot, RADIO_LINK_SLOTS);
        assert_eq!(
            driver.port.link_on(slot, link).trust,
            LinkTrust::Relayed,
            "the board's Anyone never applies through the relay"
        );
        let closed = RelayFrame::Close {
            route: 9,
            reason: RouteCloseReason::Gone,
        }
        .encode();
        driver.handle(2_000, RelayEvent::Message(&closed));
        assert_eq!(
            driver.take_actions(),
            [RelayDriverAction::Announce(RadioLinkEvent::Closed { link })]
        );
        assert_eq!(driver.port.slot(slot).link_id(), None);
    }

    fn driver() -> RelayDriver {
        fn entropy(buf: &mut [u8]) {
            buf.fill(3);
        }
        RelayDriver::new(
            RelayClientConfig {
                host: String::from("relay.test"),
                port: 80,
                board_mac: [2, 0, 0, 0, 0, 1],
                label: String::from("test"),
                wire_proto: 1,
                max_routes: 1,
                firmware: String::from("test-1"),
            },
            entropy,
            RadioLinkPort::leak(),
            RADIO_LINK_SLOTS,
        )
    }

    fn account() -> RelayAccount {
        RelayAccount {
            salt: [1; 16],
            k: [2; 32],
        }
    }

    /// Joined, on, an account, the leg up and the hub's welcome.
    fn register(driver: &mut RelayDriver) {
        driver.handle(0, RelayEvent::CloudRelay(true));
        driver.handle(0, RelayEvent::Accounts(vec![account()]));
        driver.handle(0, RelayEvent::Network { joined: true });
        driver.handle(0, RelayEvent::Resolved(Some([127, 0, 0, 1])));
        driver.handle(0, RelayEvent::Connected);
        let challenge = RelayFrame::Challenge { nonce: [5; 32] }.encode();
        driver.handle(0, RelayEvent::Message(&challenge));
        let registered = RelayFrame::Registered {
            accounts_ok: 1,
            ping_s: 25,
        }
        .encode();
        driver.handle(0, RelayEvent::Message(&registered));
        assert_eq!(driver.state(), RelayState::Connected);
        driver.take_actions();
    }
}
