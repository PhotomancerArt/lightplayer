//! The cloud relay on `lp-net` (Wi-Fi relay plan P8): the board dials
//! lightplayer.app by itself and holds its device leg, so a browser signed
//! in to one of its accounts can reach it from anywhere.
//!
//! The work is `fw_esp32_common::net::relay`'s — the driver (the `lpc-relay`
//! client joined to the network slot it shares with the LAN endpoint) and
//! the device-leg loop — over this chip's platform ([`C6RelayIo`]): DNS and
//! TCP from embassy-net, the hardware RNG, the station's address
//! (`net_address`), and the settings the main thread hands over
//! (`relay_probes::RELAY_BOARD`).
//!
//! - **In the core.** `core_boot` starts it with the station, the LAN
//!   endpoint and mDNS, so an engine-less core reaches the relay too (OTA
//!   M8's rule).
//! - **It dials only when** the station has an address, Cloud relay is on,
//!   and the board holds an account key (the driver's rule, RD8).
//! - **Where.** `lightplayer.app:80`, always, on the product image; a desk
//!   image built with `LP_RELAY_HOST=<host>[:port]` dials that instead and
//!   says so at boot (RD14).
//! - **Memory.** The leg's TCP and WebSocket buffers (6,921 B) are
//!   allocated once: at boot with the LAN's when the board will join with
//!   Cloud relay on (`net_thread::NetBuffers`; whether it holds an account
//!   key or not, so they sit low in the heap), else the first time the
//!   driver dials. A board with Cloud relay off at boot does not pay for
//!   them until it is switched on and dials. On the emulated C6 with
//!   `projects/test/basic` loaded and a network session open they take the
//!   largest free block from 19,556 B to 13,448 B — under the read gate's
//!   16,384 B (Wi-Fi relay plan P9's measurement; A3, R1).

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use core::cell::RefCell;

use embassy_futures::select::{Either, select};
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, Ipv4Address, Stack};
use embassy_time::{Duration, Instant, Timer};
use fw_esp32_common::net::relay::{RelayDriver, RelayLegBuffers, RelayLegIo, run_relay_leg};
use fw_esp32_common::net::ws::RX_OVERHEAD;
use fw_esp32_common::radio_link::lan_link_config::LAN_MAX_FRAME;
use fw_esp32_common::radio_link::{RADIO_LINK_SLOTS, SharedPort};
use lpc_relay::{LanAddress, ROUTE_FRAME_OVERHEAD, RelayClientConfig, RelayEvent};

use super::lan_endpoint_task::LINK_PORT;
use super::net_address::{self, AddressWatch};
use super::relay_probes::RELAY_BOARD;
use super::tcp_byte_stream::TcpStream;

/// The device leg's TCP buffers: the LAN link's sizes (one network session
/// rides either path, at the same window).
const TCP_RX: usize = 2 * 1024;
const TCP_TX: usize = 2560;
/// The WebSocket's receive buffer: one route frame (3 bytes and one
/// network-link frame), with room for a control frame between fragments.
const WS_RX: usize = ROUTE_FRAME_OVERHEAD + LAN_MAX_FRAME + RX_OVERHEAD;
/// One outgoing route frame.
const FRAME_TX: usize = ROUTE_FRAME_OVERHEAD + LAN_MAX_FRAME;
/// A leg that hears nothing for this long is gone. The hub pings every
/// 25 s, and the driver closes a leg silent for 60 s itself first.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(75);

/// The relay's product address, or the desk image's (`LP_RELAY_HOST`).
pub const RELAY_HOST: &str = env!("LP_RELAY_HOST_NAME");

/// The relay's port (80: the device leg is plain HTTP).
pub fn relay_port() -> u16 {
    env!("LP_RELAY_HOST_PORT").parse().unwrap_or(80)
}

/// Whether this image was built to dial a desk relay (`LP_RELAY_HOST`).
pub fn relay_host_overridden() -> bool {
    env!("LP_RELAY_HOST_OVERRIDDEN") == "true"
}

/// The leg's buffers, allocated once (on the heap directly).
pub struct RelayBuffers {
    tcp_rx: &'static mut [u8],
    tcp_tx: &'static mut [u8],
    ws_rx: &'static mut [u8],
    frame_tx: &'static mut [u8],
}

impl RelayBuffers {
    pub fn leak() -> Self {
        let leak =
            |len: usize| -> &'static mut [u8] { Box::leak(vec![0u8; len].into_boxed_slice()) };
        Self {
            tcp_rx: leak(TCP_RX),
            tcp_tx: leak(TCP_TX),
            ws_rx: leak(WS_RX),
            frame_tx: leak(FRAME_TX),
        }
    }
}

/// The relay task: `config` is the board's fixed facts (MAC, name, wire
/// version) and where the relay is.
#[embassy_executor::task]
pub async fn relay_task(
    stack: Stack<'static>,
    port: SharedPort,
    buffers: Option<RelayBuffers>,
    config: RelayClientConfig,
) {
    let index = RADIO_LINK_SLOTS;
    let io = C6RelayIo {
        stack,
        address: RefCell::new(net_address::watch()),
        queue: RefCell::new(VecDeque::new()),
    };
    let mut driver = RelayDriver::new(config, fill_random, port.port(), index);
    io.prime(&mut driver);
    let buffers = match buffers {
        Some(buffers) => buffers,
        None => {
            // Nothing is spent until the driver first wants to dial.
            while !driver.has_actions() {
                RELAY_BOARD.publish(&driver);
                let input = io.next_input().await;
                driver.handle(io.now_us(), input);
            }
            RelayBuffers::leak()
        }
    };
    let RelayBuffers {
        tcp_rx,
        tcp_tx,
        ws_rx,
        frame_tx,
    } = buffers;
    let mut bufs = RelayLegBuffers {
        tcp_rx,
        tcp_tx,
        ws_rx,
        frame_tx,
    };
    run_relay_leg(&mut driver, &io, &port, index, &mut bufs).await;
}

/// The relay's platform on the C6.
struct C6RelayIo {
    stack: Stack<'static>,
    address: RefCell<AddressWatch>,
    /// Inputs taken together (the address and its LAN address), handed out
    /// one at a time.
    queue: RefCell<VecDeque<RelayEvent<'static>>>,
}

impl C6RelayIo {
    /// What the board already knows at start: the station's address, the
    /// switch and the account entries `core_boot` handed over.
    fn prime(&self, driver: &mut RelayDriver) {
        let now = self.now_us();
        let address = self.address.borrow_mut().try_get().flatten();
        driver.handle(now, RelayEvent::Lan(lan_address(address)));
        if let Some(on) = RELAY_BOARD.take_cloud_relay() {
            driver.handle(now, RelayEvent::CloudRelay(on));
        }
        if let Some(accounts) = RELAY_BOARD.take_accounts() {
            driver.handle(now, RelayEvent::Accounts(accounts));
        }
        driver.handle(
            now,
            RelayEvent::Network {
                joined: address.is_some(),
            },
        );
    }

    fn push_address(&self, address: Option<[u8; 4]>) {
        let mut queue = self.queue.borrow_mut();
        queue.push_back(RelayEvent::Lan(lan_address(address)));
        queue.push_back(RelayEvent::Network {
            joined: address.is_some(),
        });
    }
}

impl RelayLegIo for C6RelayIo {
    type Stream<'b>
        = TcpStream<'b>
    where
        Self: 'b;

    fn now_us(&self) -> u64 {
        Instant::now().as_micros()
    }

    fn entropy(&self) -> fn(&mut [u8]) {
        fill_random
    }

    async fn resolve(&self, host: &str) -> Option<[u8; 4]> {
        match self.stack.dns_query(host, DnsQueryType::A).await {
            Ok(addresses) => addresses.first().map(|address| match address {
                IpAddress::Ipv4(v4) => v4.octets(),
            }),
            Err(_) => {
                log::info!("[relay] {host} did not resolve");
                None
            }
        }
    }

    async fn connect<'b>(
        &'b self,
        addr: [u8; 4],
        port: u16,
        tcp_rx: &'b mut [u8],
        tcp_tx: &'b mut [u8],
    ) -> Option<TcpStream<'b>> {
        let mut socket = TcpSocket::new(self.stack, tcp_rx, tcp_tx);
        socket.set_timeout(Some(SOCKET_TIMEOUT));
        // One write is one whole WebSocket message: send it now (the LAN
        // endpoint's reason).
        socket.set_nagle_enabled(false);
        match socket.connect((Ipv4Address::from(addr), port)).await {
            Ok(()) => Some(TcpStream(socket)),
            Err(_) => None,
        }
    }

    async fn sleep_until(&self, at: Option<u64>) {
        match at {
            Some(at) => Timer::at(Instant::from_micros(at)).await,
            None => core::future::pending().await,
        }
    }

    async fn next_input(&self) -> RelayEvent<'static> {
        loop {
            if let Some(input) = self.queue.borrow_mut().pop_front() {
                return input;
            }
            if let Some(on) = RELAY_BOARD.take_cloud_relay() {
                return RelayEvent::CloudRelay(on);
            }
            if let Some(accounts) = RELAY_BOARD.take_accounts() {
                return RelayEvent::Accounts(accounts);
            }
            let changed = {
                let mut address = self.address.borrow_mut();
                match select(RELAY_BOARD.wait(), address.changed()).await {
                    Either::First(()) => None,
                    Either::Second(address) => Some(address),
                }
            };
            if let Some(address) = changed {
                self.push_address(address);
            }
        }
    }

    fn publish(&self, driver: &RelayDriver) {
        RELAY_BOARD.publish(driver);
    }
}

/// The board's LAN address for the relay's hello: where its LAN link
/// listens.
fn lan_address(address: Option<[u8; 4]>) -> Option<LanAddress> {
    address.map(|ip| LanAddress {
        ip,
        port: LINK_PORT,
    })
}

/// The relay's `config` for this board: its MAC, its name (the stamped
/// `/.lp/device.json` name, else its `lp-xxxx` LAN label), this build's wire
/// version, and the relay's address (`lightplayer.app:80`, or the desk
/// image's `LP_RELAY_HOST`). One session at a time: the C6 holds one.
pub fn relay_config(board_mac: [u8; 6], name: Option<String>) -> RelayClientConfig {
    RelayClientConfig {
        host: String::from(RELAY_HOST),
        port: relay_port(),
        board_mac,
        label: name.unwrap_or_else(|| fw_esp32_common::net::mdns::mdns_label(board_mac)),
        wire_proto: lpc_wire::WIRE_PROTO_VERSION,
        max_routes: 1,
    }
}

fn fill_random(buf: &mut [u8]) {
    esp_hal::rng::Rng::new().read(buf);
}
