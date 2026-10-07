//! The harness board on the cloud relay: the std twin of the C6's relay
//! task (`fw-esp32c6/src/net/relay_task.rs`). The board's own relay driver
//! and device-leg loop (`net::relay`) over a std socket; its routes land on
//! the same network slot the harness's LAN endpoint uses, so the two share
//! one session exactly as on a C6.

extern crate std;

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::future::poll_fn;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::Poll;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::Duration;

use lp_link::Micros;
use lpc_relay::{ROUTE_FRAME_OVERHEAD, RelayAccount, RelayClientConfig, RelayEvent, RelayState};

use super::harness_block_on::{block_on, until_micros};
use super::harness_entropy::harness_entropy;
use super::std_tcp_byte_stream::StdTcpByteStream;
use crate::net::relay::{
    RelayCounters, RelayDriver, RelayLegBuffers, RelayLegExit, RelayLegIo, run_relay_leg,
    wait_until_may_dial,
};
use crate::net::ws::RX_OVERHEAD;
use crate::radio_link::lan_link_config::LAN_MAX_FRAME;
use crate::radio_link::{RADIO_LINK_SLOTS, SharedPort, now_us};

/// Where the harness board's relay is, and who the board says it is.
#[derive(Debug, Clone)]
pub struct HarnessRelay {
    /// The relay's host (resolved by the board, sent as `Host`).
    pub host: String,
    /// Its plain-HTTP port.
    pub port: u16,
    /// The board's MAC at the relay.
    pub board_mac: [u8; 6],
    /// The board's name for people.
    pub label: String,
}

/// What the relay thread shares with the harness and its tests.
#[derive(Default)]
pub(super) struct RelayShared {
    inputs: Mutex<VecDeque<RelayEvent<'static>>>,
    state: Mutex<Option<(RelayState, RelayCounters)>>,
}

impl RelayShared {
    pub(super) fn push_input(&self, input: RelayEvent<'static>) {
        lock(&self.inputs).push_back(input);
    }

    pub(super) fn snapshot(&self) -> (RelayState, RelayCounters) {
        lock(&self.state).unwrap_or((RelayState::Off, RelayCounters::default()))
    }
}

/// Run the board's relay until `stop`: joined at `lan` (its LAN address),
/// Cloud relay on, holding `accounts`.
pub(super) fn run_relay(
    relay: HarnessRelay,
    accounts: Vec<RelayAccount>,
    lan: SocketAddr,
    port: SharedPort,
    shared: &RelayShared,
    stop: &AtomicBool,
) {
    let config = RelayClientConfig {
        host: relay.host,
        port: relay.port,
        board_mac: relay.board_mac,
        label: relay.label,
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
        max_routes: 1,
    };
    let index = RADIO_LINK_SLOTS;
    let mut driver = RelayDriver::new(config, harness_entropy, port.port(), index);
    let now = now_us();
    let lan = match lan {
        SocketAddr::V4(v4) => Some(lpc_relay::LanAddress {
            ip: v4.ip().octets(),
            port: v4.port(),
        }),
        SocketAddr::V6(_) => None,
    };
    driver.handle(now, RelayEvent::Lan(lan));
    driver.handle(now, RelayEvent::Accounts(accounts));
    driver.handle(now, RelayEvent::CloudRelay(true));
    driver.handle(now, RelayEvent::Network { joined: true });
    let io = StdRelayIo { shared, stop };
    let mut ws_rx = vec![0u8; ROUTE_FRAME_OVERHEAD + LAN_MAX_FRAME + RX_OVERHEAD];
    let mut frame_tx = vec![0u8; ROUTE_FRAME_OVERHEAD + LAN_MAX_FRAME];
    let mut bufs = RelayLegBuffers {
        tcp_rx: &mut [],
        tcp_tx: &mut [],
        ws_rx: &mut ws_rx,
        frame_tx: &mut frame_tx,
    };
    // The C6's loop, without giving the buffers back (they are the host's).
    while block_on(run_relay_leg(&mut driver, &io, &port, index, &mut bufs)) == RelayLegExit::Idle {
        if !block_on(wait_until_may_dial(&mut driver, &io, &port)) {
            return;
        }
    }
}

/// The relay leg's platform on the host: std sockets, the device clock, and
/// inputs a test pushes.
struct StdRelayIo<'a> {
    shared: &'a RelayShared,
    stop: &'a AtomicBool,
}

impl RelayLegIo for StdRelayIo<'_> {
    type Stream<'b>
        = StdTcpByteStream
    where
        Self: 'b;

    fn now_us(&self) -> Micros {
        now_us()
    }

    fn entropy(&self) -> fn(&mut [u8]) {
        harness_entropy
    }

    async fn resolve(&self, host: &str) -> Option<[u8; 4]> {
        (host, 0)
            .to_socket_addrs()
            .ok()?
            .find_map(|addr| match addr {
                SocketAddr::V4(v4) => Some(v4.ip().octets()),
                SocketAddr::V6(_) => None,
            })
    }

    async fn connect<'b>(
        &'b self,
        addr: [u8; 4],
        port: u16,
        _tcp_rx: &'b mut [u8],
        _tcp_tx: &'b mut [u8],
    ) -> Option<StdTcpByteStream> {
        let addr = SocketAddr::from((addr, port));
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).ok()?;
        StdTcpByteStream::new(stream).ok()
    }

    async fn sleep_until(&self, at: Option<Micros>) {
        let stopping = || self.stop.load(Ordering::SeqCst);
        until_micros(at.unwrap_or(u64::MAX), &stopping).await;
    }

    async fn next_input(&self) -> RelayEvent<'static> {
        poll_fn(|_| match lock(&self.shared.inputs).pop_front() {
            Some(input) => Poll::Ready(input),
            None => Poll::Pending,
        })
        .await
    }

    fn publish(&self, driver: &RelayDriver) {
        *lock(&self.shared.state) = Some((driver.state(), driver.counters()));
    }

    fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
