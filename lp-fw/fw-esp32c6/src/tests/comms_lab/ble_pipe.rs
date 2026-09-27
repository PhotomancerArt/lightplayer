//! The BLE NUS pipe: a message transport, so each link frame is one
//! notification (board → host) or one write (host → board), with no COBS and
//! no delimiters ([`LinkConfig::ble`], datagram framing).
//!
//! `test_ble`'s bring-up (RF switch, controller, one NUS service), advertised
//! as `LP-LAB-xxxx`. A connection gets a fresh [`LabEdge`] (a fresh nonce, so
//! the host sees a new session per connection) once the central has written
//! its first frame: by then the MTU exchange is done, and the board's frame
//! payload is sized to it (the link then uses the smaller of the two ends').
//! The connection interval is asked down to 15 ms a second after connect, as
//! the product does.

use core::fmt::Write as _;

// `#[gatt_server]` names `embassy_sync::…` by relative path and means
// trouble-host's 0.7 (see `test_ble`).
use embassy_sync_07 as embassy_sync;

use bt_hci::cmd::le::{
    LeConnUpdate, LeReadLocalSupportedFeatures, LeRemoteConnectionParameterRequestNegativeReply,
    LeRemoteConnectionParameterRequestReply,
};
use bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync};
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::efuse;
use esp_hal::rng::Rng;
use esp_radio::ble::controller::BleConnector;
use heapless::Vec;
use lp_link::LinkConfig;
use trouble_host::prelude::*;

use super::lab_edge::{LabEdge, now_us};

/// One notification's value: 4 header + 236 payload + 4 CRC fills it.
const VALUE_MAX: usize = 244;
const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;
/// Longest the task sleeps with nothing to do.
const IDLE_CAP_US: u64 = 10_000;

#[gatt_server]
struct Server {
    uart: UartService,
}

#[gatt_service(uuid = "6e400001-b5a3-f393-e0a9-e50e24dcca9e")]
struct UartService {
    #[characteristic(
        uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e",
        write,
        write_without_response
    )]
    rx: Vec<u8, VALUE_MAX>,
    #[characteristic(uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e", notify)]
    tx: Vec<u8, VALUE_MAX>,
}

pub async fn run(
    bt: esp_hal::peripherals::BT<'static>,
    gpio3: esp_hal::peripherals::GPIO3<'static>,
    gpio14: esp_hal::peripherals::GPIO14<'static>,
    rng: Rng,
) {
    // XIAO ESP32C6 RF switch: GPIO3 LOW powers it, GPIO14 LOW selects the
    // on-board antenna (`test_ble`; docs/defects/2026-09-23-xiao-c6-rf-switch-never-powered.md).
    core::mem::forget(esp_hal::gpio::Output::new(
        gpio3,
        esp_hal::gpio::Level::Low,
        esp_hal::gpio::OutputConfig::default(),
    ));
    core::mem::forget(esp_hal::gpio::Output::new(
        gpio14,
        esp_hal::gpio::Level::Low,
        esp_hal::gpio::OutputConfig::default(),
    ));

    let connector = match BleConnector::new(bt, Default::default()) {
        Ok(c) => c,
        Err(_) => {
            log::error!("ble: controller init failed");
            return;
        }
    };
    let controller: ExternalController<_, 20> = ExternalController::new(connector);
    let mac = efuse::base_mac_address();
    let mac = mac.as_bytes();
    let mut name: heapless::String<16> = heapless::String::new();
    let _ = write!(name, "LP-LAB-{:02x}{:02x}", mac[4], mac[5]);
    let mut addr = [0u8; 6];
    for (i, b) in mac.iter().rev().enumerate() {
        addr[i] = *b;
    }
    addr[5] |= 0xC0;

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(controller, &mut resources).set_random_address(Address::random(addr));
    let Host {
        mut peripheral,
        mut runner,
        ..
    } = stack.build();
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: name.as_str(),
        appearance: &appearance::UNKNOWN,
    }))
    .expect("GATT server builds");
    log::info!("ble: up as {}, heap free {}", name.as_str(), esp_alloc::HEAP.free());

    let _ = join(
        async {
            loop {
                if runner.run().await.is_err() {
                    Timer::after_millis(100).await;
                }
            }
        },
        async {
            loop {
                match advertise(name.as_str(), &mut peripheral, &server).await {
                    Ok(conn) => {
                        log::info!("ble: connected, att mtu {}", conn.raw().att_mtu());
                        serve(&server, &conn, &stack, rng.random()).await;
                    }
                    Err(_) => Timer::after_secs(1).await,
                }
            }
        },
    )
    .await;
}

async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut adv = [0u8; 31];
    let adv_len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut adv[..],
    )?;
    let mut scan = [0u8; 31];
    let scan_len = AdStructure::encode_slice(
        &[AdStructure::ServiceUuids128(&[[
            0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0, 0x93, 0xf3, 0xa3, 0xb5, 0x01, 0x00,
            0x40, 0x6e,
        ]])],
        &mut scan[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &adv[..adv_len],
                scan_data: &scan[..scan_len],
            },
        )
        .await?;
    Ok(advertiser.accept().await?.with_attribute_server(server)?)
}

/// Every controller command the pipe uses, in one bound.
trait BleCtl:
    Controller
    + ControllerCmdAsync<LeConnUpdate>
    + ControllerCmdSync<LeReadLocalSupportedFeatures>
    + ControllerCmdAsync<LeRemoteConnectionParameterRequestReply>
    + ControllerCmdAsync<LeRemoteConnectionParameterRequestNegativeReply>
{
}
impl<T> BleCtl for T where
    T: Controller
        + ControllerCmdAsync<LeConnUpdate>
        + ControllerCmdSync<LeReadLocalSupportedFeatures>
        + ControllerCmdAsync<LeRemoteConnectionParameterRequestReply>
        + ControllerCmdAsync<LeRemoteConnectionParameterRequestNegativeReply>
{
}

/// One connection: a fresh link over its writes and notifications.
async fn serve<C: BleCtl, P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    stack: &Stack<'_, C, P>,
    nonce: u32,
) {
    let rx = &server.uart.rx;
    let tx = &server.uart.tx;
    let mut edge: Option<LabEdge> = None;
    let connected = Instant::now();
    let mut params_asked = false;
    loop {
        if let Some(e) = edge.as_mut() {
            e.service();
            while let Some(frame) = e.next_frame() {
                let Ok(value) = Vec::<u8, VALUE_MAX>::from_slice(frame) else {
                    e.write_errors += 1;
                    continue;
                };
                if tx.notify(conn, &value).await.is_err() {
                    e.write_errors += 1;
                    break;
                }
            }
        }
        if !params_asked && connected.elapsed() >= Duration::from_millis(1_000) {
            params_asked = true;
            let params = RequestedConnParams {
                min_connection_interval: Duration::from_micros(15_000),
                max_connection_interval: Duration::from_micros(15_000),
                max_latency: 0,
                supervision_timeout: Duration::from_secs(4),
                ..Default::default()
            };
            let ok = conn.raw().update_connection_params(stack, &params).await.is_ok();
            log::info!("ble: asked for a 15 ms interval: {}", if ok { "sent" } else { "refused" });
        }
        let wake = match edge.as_ref() {
            Some(e) => e.wake_at(IDLE_CAP_US),
            None => now_us() + IDLE_CAP_US,
        };
        match select(conn.next(), Timer::at(Instant::from_micros(wake))).await {
            Either::First(GattConnectionEvent::Disconnected { reason }) => {
                log::info!("ble: disconnected, reason 0x{:02x}", reason.into_inner());
                return;
            }
            Either::First(GattConnectionEvent::Gatt { event }) => {
                let mut frame: Option<Vec<u8, VALUE_MAX>> = None;
                if let GattEvent::Write(w) = &event
                    && w.handle() == rx.handle
                {
                    frame = Vec::from_slice(w.data()).ok();
                }
                if let Ok(reply) = event.accept() {
                    reply.send().await;
                }
                if let Some(f) = frame {
                    let e = edge.get_or_insert_with(|| {
                        let mtu = conn.raw().att_mtu() as usize;
                        let mut cfg = LinkConfig::ble();
                        cfg.max_payload = (mtu.saturating_sub(3 + 8)).clamp(20, 236) as u16;
                        log::info!("ble: link starts, frame payload {}", cfg.max_payload);
                        LabEdge::new("ble", cfg, nonce)
                    });
                    e.on_datagram(&f);
                }
            }
            Either::First(GattConnectionEvent::RequestConnectionParams(req)) => {
                let _ = req.accept(None, stack).await;
            }
            Either::First(GattConnectionEvent::ConnectionParamsUpdated { conn_interval, .. }) => {
                log::info!("ble: interval now {} us", conn_interval.as_micros());
            }
            _ => {}
        }
    }
}
