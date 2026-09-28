//! The WiFi pipe: UDP datagrams on port [`UDP_PORT`], one link frame per
//! datagram ([`LinkConfig::udp`]: selective repeat with a reordering
//! allowance, 1 KB frames).
//!
//! The board joins the network named at BUILD time by `LP_LAB_WIFI_SSID` /
//! `LP_LAB_WIFI_PASS` (read with `option_env!`, never logged, never in the
//! repo), takes an address by DHCP, and answers whoever writes to it: the
//! first datagram's sender becomes the peer, and a datagram from anyone else
//! later replaces it (a restarted host on a new port). Its address is printed
//! raw on USB (`[LAB] wifi ip=…`) every few seconds until a peer appears, and
//! goes into the `hello` identity.

use core::fmt::Write as _;

use embassy_futures::select::{Either, select};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Runner, StackResources};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::rng::Rng;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config, Interface};
use lp_link::LinkConfig;
use static_cell::StaticCell;

use super::lab_edge::{LabEdge, now_us};

pub const UDP_PORT: u16 = 5555;
/// Longest the task sleeps with nothing to do.
const IDLE_CAP_US: u64 = 10_000;
const DATAGRAM_MAX: usize = 1_100;

static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
static RX_META: StaticCell<[PacketMetadata; 32]> = StaticCell::new();
static TX_META: StaticCell<[PacketMetadata; 32]> = StaticCell::new();
static RX_BUF: StaticCell<[u8; 24 * 1024]> = StaticCell::new();
static TX_BUF: StaticCell<[u8; 24 * 1024]> = StaticCell::new();

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

pub async fn run(
    spawner: embassy_executor::Spawner,
    wifi: esp_hal::peripherals::WIFI<'static>,
    rng: Rng,
) {
    let (Some(ssid), Some(pass)) = (
        option_env!("LP_LAB_WIFI_SSID"),
        option_env!("LP_LAB_WIFI_PASS"),
    ) else {
        esp_println::println!("[LAB] wifi: built without LP_LAB_WIFI_SSID / LP_LAB_WIFI_PASS");
        return;
    };
    let (mut controller, interfaces) = match esp_radio::wifi::new(wifi, Default::default()) {
        Ok(pair) => pair,
        Err(_) => {
            esp_println::println!("[LAB] wifi: init failed");
            return;
        }
    };
    let conf = Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_password(pass.into()),
    );
    if controller.set_config(&conf).is_err() {
        esp_println::println!("[LAB] wifi: set_config failed");
        return;
    }
    let seed = u64::from(rng.random()) << 32 | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.spawn(net_task(runner).unwrap());

    loop {
        match controller.connect_async().await {
            Ok(_) => break,
            Err(_) => {
                esp_println::println!("[LAB] wifi: connect failed, retrying");
                Timer::after(Duration::from_secs(2)).await;
            }
        }
    }
    stack.wait_config_up().await;
    let mut ip: heapless::String<24> = heapless::String::new();
    if let Some(cfg) = stack.config_v4() {
        let _ = write!(ip, "{}", cfg.address.address());
    }
    log::info!("wifi: up, ip {} udp {}", ip.as_str(), UDP_PORT);

    let mut socket = UdpSocket::new(
        stack,
        RX_META.init([PacketMetadata::EMPTY; 32]),
        RX_BUF.init([0; 24 * 1024]),
        TX_META.init([PacketMetadata::EMPTY; 32]),
        TX_BUF.init([0; 24 * 1024]),
    );
    if socket.bind(UDP_PORT).is_err() {
        esp_println::println!("[LAB] wifi: bind failed");
        return;
    }

    let mut edge = LabEdge::new("udp", LinkConfig::udp(), rng.random());
    let mut identity: heapless::String<96> = heapless::String::new();
    let _ = write!(identity, "wifi-ip={} udp={}", ip.as_str(), UDP_PORT);
    edge.board.set_identity(alloc::format!(
        "lab=comms pipe=udp chip=esp32c6 fw={} link=selective-repeat {}",
        env!("LP_BUILD_COMMIT"),
        identity.as_str()
    ));
    let mut peer: Option<IpEndpoint> = None;
    let mut announce = Instant::now();
    let mut buf = [0u8; DATAGRAM_MAX];
    loop {
        if peer.is_none() && Instant::now() >= announce {
            esp_println::println!("[LAB] wifi ip={} udp={}", ip.as_str(), UDP_PORT);
            announce = Instant::now() + Duration::from_secs(3);
        }
        edge.service();
        if let Some(p) = peer {
            while let Some(frame) = edge.next_frame() {
                if socket.send_to(frame, p).await.is_err() {
                    edge.write_errors += 1;
                    break;
                }
            }
        }
        let wake = edge.wake_at(IDLE_CAP_US).max(now_us());
        match select(
            socket.recv_from(&mut buf),
            Timer::at(Instant::from_micros(wake)),
        )
        .await
        {
            Either::First(Ok((n, meta))) => {
                if peer != Some(meta.endpoint) {
                    log::info!("wifi: peer is now {}", meta.endpoint);
                    peer = Some(meta.endpoint);
                }
                edge.on_datagram(&buf[..n]);
            }
            _ => {}
        }
    }
}
