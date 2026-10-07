//! The device leg, run: one async loop that carries the relay driver's
//! actions out over a socket and the socket's news back in, shared by the
//! C6's relay task and the host harness so the two cannot drift.
//!
//! What differs between them is behind [`RelayLegIo`]: the clock, name
//! resolution, opening a TCP connection (embassy-net on the C6, std on the
//! host), the timer, and where the driver's inputs come from (the station's
//! address and the settings the main thread hands over, on the C6). The
//! WebSocket on top ([`WsConnection::connect`]), the slot and the driver are
//! the same code on both.
//!
//! The loop, per connection:
//!
//! 1. **No leg.** Run the driver's actions (resolve, announce) and wait for
//!    an input or the driver's next deadline, until it asks to connect.
//! 2. **Connect.** TCP, then the upgrade on `/relay/device`, each bounded
//!    by [`CONNECT_TIMEOUT_US`]; a failure is `Closed`, and the driver backs
//!    off.
//! 3. **The leg.** Send what the driver asks and every outgoing route frame,
//!    then wait for the first of: a message (or a ping), the mux's doorbell
//!    or close request on the relay's edge of the network slot, the slot's
//!    verdict on a challenge, an input, or a deadline. A close from the hub
//!    with 1001 is "going away" (a deploy).
//!
//! **Memory** (Wi-Fi relay plan, round 2: Yona's 2026-10-07 ruling). The
//! loop allocates the leg's buffers ([`RelayLegSizes`]) when the driver
//! first asks to connect, keeps them across reconnects, and gives them back
//! whenever the board may no longer dial (Wi-Fi lost, Cloud relay off, no
//! account entry: RD8), so a board that will not dial holds none. The
//! route's outgoing frame exists only while a route holds the network slot:
//! while the LAN serves the one session it is not needed. Both are asked
//! for fallibly; a heap with no room is a failed dial (the driver backs
//! off) or a closed route, never a reset.

use core::future::Future;

use embassy_futures::select::{Either, Either3, select, select3};
use lp_link::Micros;
use lpc_relay::{RELAY_DEVICE_PATH, RelayEvent};

use alloc::boxed::Box;

use super::relay_driver::RelayDriver;
use super::relay_driver_action::RelayDriverAction;
use super::relay_route_link::{RelayRouteLink, RouteSlotState};
use crate::net::try_zeroed_bytes;
use crate::net::ws::{ByteStream, CloseCode, WsClosed, WsConnection, WsEvent};
use crate::radio_link::{RadioLinkPort, SlotEdge};

/// How long the TCP connect, and then the WebSocket upgrade, may each take.
pub const CONNECT_TIMEOUT_US: u64 = 10_000_000;

/// The WebSocket close code a hub that is shutting down sends.
const GOING_AWAY: u16 = 1001;

/// What the loop needs from its platform.
#[allow(
    async_fn_in_trait,
    reason = "single-executor firmware and test threads; no Send bound needed"
)]
pub trait RelayLegIo {
    /// The byte stream one TCP connection is, borrowing the caller's TCP
    /// buffers for its life.
    type Stream<'b>: ByteStream
    where
        Self: 'b;

    /// The device clock, µs (the one the links run on).
    fn now_us(&self) -> Micros;

    /// Fill `buf` with fresh random bytes (the WebSocket key and masks).
    fn entropy(&self) -> fn(&mut [u8]);

    /// Resolve `host` to one IPv4 address; `None` when it does not.
    async fn resolve(&self, host: &str) -> Option<[u8; 4]>;

    /// Open TCP to `addr:port`, over `tcp_rx`/`tcp_tx` where the platform
    /// needs buffers of its own; `None` on failure. Bounded by the caller.
    async fn connect<'b>(
        &'b self,
        addr: [u8; 4],
        port: u16,
        tcp_rx: &'b mut [u8],
        tcp_tx: &'b mut [u8],
    ) -> Option<Self::Stream<'b>>;

    /// Resolve at `at` µs on the device clock, or never with `None`.
    async fn sleep_until(&self, at: Option<Micros>);

    /// The driver's next input from the board (the network joined or lost,
    /// the LAN address, the Cloud relay switch, the account entries).
    /// Cancel-safe: it is raced and dropped every pass.
    async fn next_input(&self) -> RelayEvent<'static>;

    /// The driver's state and counters, after every pass (the status probe
    /// and the heartbeat read them).
    fn publish(&self, driver: &RelayDriver);

    /// The edge is shutting down (the host harness); never on a board.
    fn stopping(&self) -> bool {
        false
    }
}

/// The sizes of the leg's buffers; the loop allocates them.
#[derive(Debug, Clone, Copy)]
pub struct RelayLegSizes {
    /// The TCP socket's receive and send buffers (0 on the host).
    pub tcp_rx: usize,
    pub tcp_tx: usize,
    /// The WebSocket's receive buffer: the largest relay frame the board
    /// takes (a route frame: 3 bytes and one network link frame) plus
    /// `ws::RX_OVERHEAD`; the upgrade request is also written from it.
    pub ws_rx: usize,
    /// One outgoing route frame (held only while a route holds the slot).
    pub frame_tx: usize,
}

/// The leg's buffers, while the board may dial.
struct LegBuffers {
    tcp_rx: Box<[u8]>,
    tcp_tx: Box<[u8]>,
    ws_rx: Box<[u8]>,
}

impl LegBuffers {
    fn try_new(sizes: RelayLegSizes) -> Option<Self> {
        Some(Self {
            tcp_rx: try_zeroed_bytes(sizes.tcp_rx)?,
            tcp_tx: try_zeroed_bytes(sizes.tcp_tx)?,
            ws_rx: try_zeroed_bytes(sizes.ws_rx)?,
        })
    }
}

/// How a connected leg ended.
enum LegEnd {
    /// The driver closed it.
    Ours,
    /// The hub closed it, or the socket failed; `true` for "going away".
    Theirs(bool),
    /// The edge is stopping.
    Stop,
}

/// Run the device leg for `driver`, its routes on network slot `index` of
/// `port`, until the edge stops (never, on a board).
pub async fn run_relay_leg<I: RelayLegIo>(
    driver: &mut RelayDriver,
    io: &I,
    port: &RadioLinkPort,
    index: usize,
    sizes: RelayLegSizes,
) {
    let mut held: Option<LegBuffers> = None;
    loop {
        let Some((addr, tcp_port)) = wait_for_dial(driver, io, port, &mut held).await else {
            return;
        };
        if held.is_none() {
            held = LegBuffers::try_new(sizes);
        }
        let Some(bufs) = held.as_mut() else {
            log::warn!("[relay] no room for the leg's buffers");
            driver.handle(io.now_us(), RelayEvent::Closed { going_away: false });
            continue;
        };
        let host = host_header(&driver.config().host, tcp_port);
        let connected = bounded(io, async {
            let stream = io
                .connect(addr, tcp_port, &mut bufs.tcp_rx, &mut bufs.tcp_tx)
                .await?;
            WsConnection::connect(
                stream,
                &mut bufs.ws_rx,
                &host,
                RELAY_DEVICE_PATH,
                io.entropy(),
            )
            .await
            .ok()
        })
        .await;
        let Some(mut ws) = connected else {
            log::info!("[relay] could not open the leg to {host}");
            driver.handle(io.now_us(), RelayEvent::Closed { going_away: false });
            continue;
        };
        log::info!("[relay] leg open to {host}");
        driver.handle(io.now_us(), RelayEvent::Connected);
        match serve_leg(driver, io, port, index, &mut ws, sizes.frame_tx).await {
            LegEnd::Ours => ws.close(CloseCode::NORMAL).await,
            LegEnd::Theirs(going_away) => {
                log::info!(
                    "[relay] leg closed by the hub{}",
                    if going_away { " (going away)" } else { "" }
                );
                driver.handle(io.now_us(), RelayEvent::Closed { going_away });
            }
            LegEnd::Stop => {
                ws.close(CloseCode::NORMAL).await;
                return;
            }
        }
    }
}

/// With no leg open: run the driver's actions and wait on its inputs and
/// deadlines until it asks to connect (`None`: the edge is stopping).
/// `held` is given back whenever the board may not dial (RD8).
async fn wait_for_dial<I: RelayLegIo>(
    driver: &mut RelayDriver,
    io: &I,
    port: &RadioLinkPort,
    held: &mut Option<LegBuffers>,
) -> Option<([u8; 4], u16)> {
    loop {
        if io.stopping() {
            return None;
        }
        if !driver.may_dial() {
            *held = None;
        }
        let mut dial = None;
        for action in driver.take_actions() {
            match action {
                RelayDriverAction::Resolve { host } => {
                    let addr = bounded(io, io.resolve(&host)).await;
                    driver.handle(io.now_us(), RelayEvent::Resolved(addr));
                }
                RelayDriverAction::Connect { addr, port } => dial = Some((addr, port)),
                RelayDriverAction::Announce(event) => port.announce(event).await,
                // No leg: nothing to send or close.
                RelayDriverAction::Send(_) | RelayDriverAction::Close => {}
            }
        }
        if dial.is_some() {
            return dial;
        }
        if driver.has_actions() {
            continue;
        }
        io.publish(driver);
        match select(io.next_input(), io.sleep_until(driver.next_wake_us())).await {
            Either::First(input) => driver.handle(io.now_us(), input),
            Either::Second(()) => driver.tick(io.now_us()),
        }
    }
}

/// Carry the open leg until either end closes it.
async fn serve_leg<I: RelayLegIo, S: ByteStream>(
    driver: &mut RelayDriver,
    io: &I,
    port: &RadioLinkPort,
    index: usize,
    ws: &mut WsConnection<'_, S>,
    frame_tx_len: usize,
) -> LegEnd {
    let slot = port.slot(index);
    let mut frame_tx: Option<Box<[u8]>> = None;
    loop {
        if io.stopping() {
            return LegEnd::Stop;
        }
        let mut closing = false;
        for action in driver.take_actions() {
            match action {
                RelayDriverAction::Send(bytes) if !closing => {
                    if ws.send(&bytes).await.is_err() {
                        return LegEnd::Theirs(false);
                    }
                    driver.note_sent(bytes.len());
                }
                RelayDriverAction::Close => closing = true,
                RelayDriverAction::Announce(event) => port.announce(event).await,
                RelayDriverAction::Send(_)
                | RelayDriverAction::Resolve { .. }
                | RelayDriverAction::Connect { .. } => {}
            }
        }
        if closing {
            return LegEnd::Ours;
        }
        // The route's frame buffer, only while a route holds the slot.
        let holding = matches!(
            driver.route(),
            Some(RelayRouteLink {
                state: RouteSlotState::Holding,
                ..
            })
        );
        if !holding {
            frame_tx = None;
        } else if frame_tx.is_none() {
            frame_tx = try_zeroed_bytes(frame_tx_len);
            if frame_tx.is_none() {
                // Logged by the driver: "route N: closed (no room …)".
                driver.on_close_request(io.now_us(), "no room for its frame buffer");
                continue;
            }
        }
        if let Some(frame_tx) = frame_tx.as_deref_mut() {
            while let Some(len) = driver.route_frame(io.now_us(), frame_tx) {
                if ws.send(&frame_tx[..len]).await.is_err() {
                    return LegEnd::Theirs(false);
                }
                driver.note_sent(len);
            }
        }
        if driver.has_actions() {
            continue;
        }
        io.publish(driver);
        let awaits_verdict = driver.awaits_verdict();
        let wake = driver.next_wake_us();
        let slot_news = select3(
            slot.doorbell_for(SlotEdge::Relay),
            slot.close_requested_for(SlotEdge::Relay),
            async {
                if awaits_verdict {
                    slot.verdict().await
                } else {
                    core::future::pending().await
                }
            },
        );
        let others = select(io.next_input(), io.sleep_until(wake));
        match select3(ws.recv_event(), slot_news, others).await {
            Either3::First(Ok(WsEvent::Message(bytes))) => {
                driver.handle(io.now_us(), RelayEvent::Message(bytes));
            }
            Either3::First(Ok(WsEvent::Control)) => driver.handle(io.now_us(), RelayEvent::Heard),
            Either3::First(Err(WsClosed::Peer(code))) => {
                return LegEnd::Theirs(code.0 == GOING_AWAY);
            }
            Either3::First(Err(_)) => return LegEnd::Theirs(false),
            Either3::Second(Either3::First(())) => {}
            Either3::Second(Either3::Second(reason)) => {
                driver.on_close_request(io.now_us(), reason);
            }
            Either3::Second(Either3::Third(verdict)) => driver.on_verdict(io.now_us(), verdict),
            Either3::Third(Either::First(input)) => driver.handle(io.now_us(), input),
            Either3::Third(Either::Second(())) => driver.tick(io.now_us()),
        }
    }
}

/// `work`, or `None` past [`CONNECT_TIMEOUT_US`].
async fn bounded<I: RelayLegIo, T>(io: &I, work: impl Future<Output = Option<T>>) -> Option<T> {
    let deadline = io.now_us() + CONNECT_TIMEOUT_US;
    match select(work, io.sleep_until(Some(deadline))).await {
        Either::First(out) => out,
        Either::Second(()) => None,
    }
}

/// The `Host` header for `host` on `port`: the bare name on 80.
fn host_header(host: &str, port: u16) -> alloc::string::String {
    if port == 80 {
        alloc::string::String::from(host)
    } else {
        alloc::format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_header_names_a_port_only_off_80() {
        assert_eq!(host_header("lightplayer.app", 80), "lightplayer.app");
        assert_eq!(host_header("127.0.0.1", 2812), "127.0.0.1:2812");
    }
}
