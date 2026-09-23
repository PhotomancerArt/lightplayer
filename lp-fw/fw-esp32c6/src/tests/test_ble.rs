//! BLE spike: can this chip, on this esp-radio, talk BLE at all?
//!
//! Vision session `ble-remote-control` (2026-09-23). Back to basics: no
//! server, no WiFi init, no ESP-NOW. The board advertises as `LP-BLE-xxxx`
//! (the last two base-MAC bytes) and serves one Nordic-UART-shaped GATT
//! service — the shape a future "BLE is just another transport" link would
//! carry the `M!` line framing over:
//!
//! - `6E400002-…` RX: the central writes bytes here;
//! - `6E400003-…` TX: the board notifies bytes back.
//!
//! Every write is echoed back on TX. A write of `burst` (optionally
//! `burst <n>`) instead notifies `n` 180-byte packets back to back and prints
//! how long they took, which is the first honest number for "how fast can a
//! project upload go over this".
//!
//! Heap is printed at each bring-up stage with the PRODUCT's heap layout
//! (the same two regions `init_board` declares), so the deltas read straight
//! against the shipped image's heap ledger.
//!
//! Build (from `lp-fw/fw-esp32c6/`, `touch src/main.rs` after flipping):
//!
//! ```text
//! cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 \
//!     --no-default-features --features esp32c6,test_ble
//! ```

use core::fmt::Write as _;

// `#[gatt_server]` names `embassy_sync::…` by relative path and means
// trouble-host's 0.7, not the firmware's 0.8; a module-scope `use` shadows
// the extern-prelude name for this file only.
use embassy_sync_07 as embassy_sync;

use embassy_futures::join::join;
use embassy_futures::select::select;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::efuse;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::ble::controller::BleConnector;
use heapless::Vec;
use trouble_host::prelude::*;

/// Largest payload one notification carries. With the 255-byte packet pool
/// the negotiated ATT MTU tops out near 247, so 244 is the ceiling; the
/// burst uses 180 to stay clear of centrals that settle lower.
const TX_MAX: usize = 244;
const BURST_PACKET: usize = 180;
const BURST_DEFAULT: usize = 100;

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2; // signal + ATT

#[gatt_server]
struct Server {
    uart: UartService,
}

/// Nordic UART Service UUIDs — what every generic BLE terminal app (nRF
/// Connect, Bluefy's demos, Web Bluetooth samples) already knows.
#[gatt_service(uuid = "6e400001-b5a3-f393-e0a9-e50e24dcca9e")]
struct UartService {
    #[characteristic(
        uuid = "6e400002-b5a3-f393-e0a9-e50e24dcca9e",
        write,
        write_without_response
    )]
    rx: Vec<u8, TX_MAX>,
    #[characteristic(uuid = "6e400003-b5a3-f393-e0a9-e50e24dcca9e", notify)]
    tx: Vec<u8, TX_MAX>,
}

pub async fn run_ble_test(_: embassy_executor::Spawner) -> ! {
    let _ = esp_println::logger::init_logger(log::LevelFilter::Info);
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    // The product's heap, region for region (`board::esp32c6::init`).
    esp_alloc::heap_allocator!(size: 260_000);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65_536);
    heap("heap-init");

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    heap("rtos-started");

    let connector = match BleConnector::new(peripherals.BT, Default::default()) {
        Ok(c) => c,
        Err(e) => {
            println!("[BLE] controller init FAILED: {e:?}");
            loop {
                Timer::after_secs(1).await;
            }
        }
    };
    heap("controller-up");
    let controller: ExternalController<_, 20> = ExternalController::new(connector);

    let mac = efuse::base_mac_address();
    let mac = mac.as_bytes();
    let mut name: heapless::String<16> = heapless::String::new();
    let _ = write!(name, "LP-BLE-{:02x}{:02x}", mac[4], mac[5]);
    // A random *static* address derived from the MAC: bt-hci stores it
    // little-endian, and the top two bits of the most significant byte must
    // be 1 for a static address.
    let mut addr = [0u8; 6];
    for (i, b) in mac.iter().rev().enumerate() {
        addr[i] = *b;
    }
    addr[5] |= 0xC0;
    let address = Address::random(addr);
    println!(
        "[BLE] name={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        name, mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    let mut resources: HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX> =
        HostResources::new();
    let stack = trouble_host::new(controller, &mut resources).set_random_address(address);
    let Host {
        mut peripheral,
        runner,
        ..
    } = stack.build();

    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: name.as_str(),
        appearance: &appearance::UNKNOWN,
    }))
    .expect("GATT server builds");
    heap("host-built");

    let _ = join(ble_task(runner), async {
        loop {
            match advertise(name.as_str(), &mut peripheral, &server).await {
                Ok(conn) => {
                    heap("connected");
                    select(gatt_events(&server, &conn), heartbeat()).await;
                    heap("disconnected");
                }
                Err(e) => {
                    println!("[BLE] advertise error: {e:?}");
                    Timer::after_secs(1).await;
                }
            }
        }
    })
    .await;
    unreachable!("the BLE runner never returns")
}

async fn ble_task<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) {
    loop {
        if let Err(e) = runner.run().await {
            println!("[BLE] runner error: {e:?}");
            Timer::after_millis(100).await;
        }
    }
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
    // The 128-bit service UUID goes in the scan response: a Web Bluetooth
    // `requestDevice({filters:[{services:[NUS]}]})` matches on it, and it does
    // not fit beside the name in 31 bytes. Little-endian on air.
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
    println!("[BLE] advertising");
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    println!("[BLE] connected");
    Ok(conn)
}

async fn gatt_events<P: PacketPool>(server: &Server<'_>, conn: &GattConnection<'_, '_, P>) {
    let rx = &server.uart.rx;
    let tx = &server.uart.tx;
    let reason = loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                let mut echo: Option<Vec<u8, TX_MAX>> = None;
                if let GattEvent::Write(w) = &event {
                    if w.handle() == rx.handle {
                        let data = w.data();
                        println!("[BLE] rx {} B", data.len());
                        echo = Vec::from_slice(&data[..data.len().min(TX_MAX)]).ok();
                    }
                }
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => println!("[BLE] reply error: {e:?}"),
                }
                if let Some(bytes) = echo {
                    if let Some(n) = burst_request(&bytes) {
                        burst(tx, conn, n).await;
                    } else if tx.notify(conn, &bytes).await.is_err() {
                        println!("[BLE] echo notify failed");
                    }
                }
            }
            GattConnectionEvent::ConnectionParamsUpdated { conn_interval, .. } => {
                println!("[BLE] conn interval {} us", conn_interval.as_micros());
            }
            _ => {}
        }
    };
    println!("[BLE] disconnected: {reason:?}");
}

/// `burst` or `burst <n>` → Some(n).
fn burst_request(bytes: &[u8]) -> Option<usize> {
    let s = core::str::from_utf8(bytes).ok()?.trim();
    let rest = s.strip_prefix("burst")?.trim();
    if rest.is_empty() {
        Some(BURST_DEFAULT)
    } else {
        rest.parse().ok()
    }
}

async fn burst<P: PacketPool>(
    tx: &Characteristic<Vec<u8, TX_MAX>>,
    conn: &GattConnection<'_, '_, P>,
    n: usize,
) {
    let mut packet: Vec<u8, TX_MAX> = Vec::new();
    for i in 0..BURST_PACKET {
        let _ = packet.push(b'a' + (i % 26) as u8);
    }
    let start = Instant::now();
    let mut sent = 0usize;
    for _ in 0..n {
        if tx.notify(conn, &packet).await.is_err() {
            break;
        }
        sent += 1;
    }
    let ms = start.elapsed().as_millis().max(1);
    let bytes = sent * BURST_PACKET;
    println!(
        "[BLE] burst sent={sent}/{n} bytes={bytes} ms={ms} rate={} B/s",
        bytes as u64 * 1000 / ms
    );
}

async fn heartbeat() {
    loop {
        Timer::after(Duration::from_secs(5)).await;
        heap("heartbeat");
    }
}

fn heap(stage: &str) {
    println!(
        "[BLE] heap stage={stage} free={} used={}",
        esp_alloc::HEAP.free(),
        esp_alloc::HEAP.used()
    );
}
