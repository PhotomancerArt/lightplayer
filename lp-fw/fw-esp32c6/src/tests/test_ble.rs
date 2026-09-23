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

use bt_hci::cmd::le::{
    LeConnUpdate, LeReadLocalSupportedFeatures, LeReadPhy,
    LeRemoteConnectionParameterRequestNegativeReply, LeRemoteConnectionParameterRequestReply,
};
use bt_hci::cmd::status::ReadRssi;
use bt_hci::controller::{ControllerCmdAsync, ControllerCmdSync};
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

    // XIAO ESP32C6 RF switch (Seeed wiki): GPIO3 LOW powers the switch in
    // front of the antenna, GPIO14 LOW selects the on-board ceramic antenna
    // (HIGH = the U.FL connector). Nothing in fw-esp32c6 drives either pin, so
    // until now every XIAO build has run the radio into an unpowered switch.
    // The drivers are leaked so the pins stay driven for the life of the image.
    core::mem::forget(esp_hal::gpio::Output::new(
        peripherals.GPIO3,
        esp_hal::gpio::Level::Low,
        esp_hal::gpio::OutputConfig::default(),
    ));
    core::mem::forget(esp_hal::gpio::Output::new(
        peripherals.GPIO14,
        esp_hal::gpio::Level::Low,
        esp_hal::gpio::OutputConfig::default(),
    ));
    println!("[BLE] xiao rf switch: GPIO3=LOW (switch on), GPIO14=LOW (on-board antenna)");

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
                    reprint_boot_stages();
                    heap("connected");
                    select(gatt_events(&server, &conn, &stack), heartbeat(&conn, &stack)).await;
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

async fn gatt_events<C, P: PacketPool>(
    server: &Server<'_>,
    conn: &GattConnection<'_, '_, P>,
    stack: &Stack<'_, C, P>,
) where
    C: LabController,
{
    let rx = &server.uart.rx;
    let tx = &server.uart.tx;
    // Upload-direction meter: writes of exactly BURST_PACKET bytes are counted,
    // not echoed (echoing a flood of writes starves the packet pool), and a
    // `count` write reports the total back and resets it.
    let mut counted = 0usize;
    let mut first_at: Option<Instant> = None;
    // Live-pixel meter: any write whose first byte is FRAME_MARKER is a frame.
    let mut frames = FrameStats::new();
    let reason = loop {
        match conn.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                let mut echo: Option<Vec<u8, TX_MAX>> = None;
                if let GattEvent::Write(w) = &event {
                    if w.handle() == rx.handle {
                        let data = w.data();
                        if data.first() == Some(&FRAME_MARKER) {
                            frames.record(data.len());
                        } else if data.len() == BURST_PACKET {
                            first_at.get_or_insert_with(Instant::now);
                            counted += data.len();
                        } else {
                            println!("[BLE] rx {} B", data.len());
                            echo = Vec::from_slice(&data[..data.len().min(TX_MAX)]).ok();
                        }
                    }
                }
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => println!("[BLE] reply error: {e:?}"),
                }
                if let Some(bytes) = echo {
                    if let Some(n) = burst_request(&bytes) {
                        burst(tx, conn, n).await;
                    } else if bytes.as_slice() == b"frames" {
                        let mut line: heapless::String<200> = heapless::String::new();
                        frames.report(&mut line);
                        println!("[BLE] {line}");
                        reply_text(tx, conn, &line).await;
                        frames = FrameStats::new();
                    } else if bytes.as_slice() == b"params" {
                        let mut line: heapless::String<200> = heapless::String::new();
                        describe_link(conn, stack, &mut line).await;
                        println!("[BLE] {line}");
                        reply_text(tx, conn, &line).await;
                    } else if let Some((min_us, max_us)) = interval_request(&bytes) {
                        let params = RequestedConnParams {
                            min_connection_interval: Duration::from_micros(min_us),
                            max_connection_interval: Duration::from_micros(max_us),
                            max_latency: 0,
                            supervision_timeout: Duration::from_secs(4),
                            ..Default::default()
                        };
                        let result = conn.raw().update_connection_params(stack, &params).await;
                        let mut line: heapless::String<120> = heapless::String::new();
                        let _ = write!(
                            line,
                            "interval request {min_us}..{max_us} us: {}",
                            if result.is_ok() { "sent" } else { "FAILED" }
                        );
                        println!("[BLE] {line}");
                        reply_text(tx, conn, &line).await;
                    } else if bytes.as_slice() == b"count" {
                        let ms = first_at.map(|t| t.elapsed().as_millis()).unwrap_or(0);
                        println!("[BLE] count bytes={counted} ms={ms}");
                        let mut reply: Vec<u8, TX_MAX> = Vec::new();
                        let mut line: heapless::String<64> = heapless::String::new();
                        let _ = write!(line, "count bytes={counted} ms={ms}");
                        let _ = reply.extend_from_slice(line.as_bytes());
                        let _ = tx.notify(conn, &reply).await;
                        counted = 0;
                        first_at = None;
                    } else if tx.notify(conn, &bytes).await.is_err() {
                        println!("[BLE] echo notify failed");
                    }
                }
            }
            GattConnectionEvent::RequestConnectionParams(req) => {
                let p = req.params();
                println!(
                    "[BLE] central asks conn params interval={}..{} us latency={} timeout={} ms",
                    p.min_connection_interval.as_micros(),
                    p.max_connection_interval.as_micros(),
                    p.max_latency,
                    p.supervision_timeout.as_millis()
                );
                match req.accept(None, stack).await {
                    Ok(()) => println!("[BLE] conn params accepted"),
                    Err(_) => println!("[BLE] conn params accept FAILED"),
                }
            }
            GattConnectionEvent::ConnectionParamsUpdated {
                conn_interval,
                peripheral_latency,
                supervision_timeout,
            } => {
                println!(
                    "[BLE] conn params now interval={} us latency={} timeout={} ms",
                    conn_interval.as_micros(),
                    peripheral_latency,
                    supervision_timeout.as_millis()
                );
            }
            GattConnectionEvent::PhyUpdated { .. } => println!("[BLE] phy updated"),
            GattConnectionEvent::DataLengthUpdated {
                max_tx_octets,
                max_rx_octets,
                ..
            } => println!("[BLE] data length tx={max_tx_octets} rx={max_rx_octets}"),
        }
    };
    // `{:?}` formats to nothing in this build (`-Zfmt-debug=none`), so print
    // the HCI status code itself: 0x08 supervision timeout, 0x13 remote user
    // terminated, 0x16 local host terminated, 0x3e failed to establish.
    println!("[BLE] disconnected: reason=0x{:02x}", reason.into_inner());
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

async fn heartbeat<C: LabController, P: PacketPool>(
    conn: &GattConnection<'_, '_, P>,
    stack: &Stack<'_, C, P>,
) {
    loop {
        Timer::after(Duration::from_secs(5)).await;
        heap("heartbeat");
        let mut line: heapless::String<200> = heapless::String::new();
        describe_link(conn, stack, &mut line).await;
        println!("[BLE] {line}");
    }
}

/// Every controller command the lab uses, in one bound.
trait LabController:
    Controller
    + ControllerCmdAsync<LeRemoteConnectionParameterRequestReply>
    + ControllerCmdAsync<LeRemoteConnectionParameterRequestNegativeReply>
    + ControllerCmdAsync<LeConnUpdate>
    + ControllerCmdSync<LeReadLocalSupportedFeatures>
    + ControllerCmdSync<LeReadPhy>
    + ControllerCmdSync<ReadRssi>
{
}
impl<T> LabController for T where
    T: Controller
        + ControllerCmdAsync<LeRemoteConnectionParameterRequestReply>
        + ControllerCmdAsync<LeRemoteConnectionParameterRequestNegativeReply>
        + ControllerCmdAsync<LeConnUpdate>
        + ControllerCmdSync<LeReadLocalSupportedFeatures>
        + ControllerCmdSync<LeReadPhy>
        + ControllerCmdSync<ReadRssi>
{
}

/// First byte of a live-pixel frame write.
const FRAME_MARKER: u8 = 0xF0;

/// Arrival statistics for marker-tagged frame writes: count, bytes, span,
/// and the gaps between consecutive arrivals, which is what jitter on the
/// LEDs would be.
struct FrameStats {
    count: u32,
    bytes: u32,
    first: Option<Instant>,
    last: Option<Instant>,
    gap_min_us: u64,
    gap_max_us: u64,
    gap_sum_us: u64,
    /// Gap histogram, upper bounds in ms: 10, 20, 35, 50, 75, 100, 150, 250, inf.
    buckets: [u32; 9],
}

const GAP_BOUNDS_MS: [u64; 8] = [10, 20, 35, 50, 75, 100, 150, 250];

impl FrameStats {
    fn new() -> Self {
        Self {
            count: 0,
            bytes: 0,
            first: None,
            last: None,
            gap_min_us: u64::MAX,
            gap_max_us: 0,
            gap_sum_us: 0,
            buckets: [0; 9],
        }
    }

    fn record(&mut self, len: usize) {
        let now = Instant::now();
        if let Some(last) = self.last {
            let gap = (now - last).as_micros();
            self.gap_min_us = self.gap_min_us.min(gap);
            self.gap_max_us = self.gap_max_us.max(gap);
            self.gap_sum_us += gap;
            let ms = gap / 1000;
            let i = GAP_BOUNDS_MS.iter().position(|b| ms < *b).unwrap_or(8);
            self.buckets[i] += 1;
        } else {
            self.first = Some(now);
        }
        self.last = Some(now);
        self.count += 1;
        self.bytes += len as u32;
    }

    fn report(&self, out: &mut heapless::String<200>) {
        let span_ms = match (self.first, self.last) {
            (Some(a), Some(b)) => (b - a).as_millis(),
            _ => 0,
        };
        let gaps = self.count.saturating_sub(1) as u64;
        let mean = if gaps > 0 { self.gap_sum_us / gaps } else { 0 };
        let min = if gaps > 0 { self.gap_min_us } else { 0 };
        let b = &self.buckets;
        let _ = write!(
            out,
            "frames n={} bytes={} span_ms={} gap_us min={} mean={} max={} hist10/20/35/50/75/100/150/250/+={}/{}/{}/{}/{}/{}/{}/{}/{}",
            self.count, self.bytes, span_ms, min, mean, self.gap_max_us,
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8]
        );
    }
}

/// The link's current parameters, PHY and signal, as one line.
async fn describe_link<C: LabController, P: PacketPool>(
    conn: &GattConnection<'_, '_, P>,
    stack: &Stack<'_, C, P>,
    out: &mut heapless::String<200>,
) {
    let raw = conn.raw();
    let p = raw.params();
    let rssi = raw.rssi(stack).await.ok();
    let phy = raw.read_phy(stack).await.ok();
    let phy_name = |k: PhyKind| match k {
        PhyKind::Le1M => "1M",
        PhyKind::Le2M => "2M",
        _ => "coded",
    };
    let _ = write!(
        out,
        "link interval_us={} latency={} timeout_ms={} mtu={} rssi={} phy_tx={} phy_rx={}",
        p.conn_interval.as_micros(),
        p.peripheral_latency,
        p.supervision_timeout.as_millis(),
        raw.att_mtu(),
        rssi.map(|r| r as i32).unwrap_or(i32::MIN),
        phy.map(|(t, _)| phy_name(t)).unwrap_or("?"),
        phy.map(|(_, r)| phy_name(r)).unwrap_or("?"),
    );
}

async fn reply_text<P: PacketPool>(
    tx: &Characteristic<Vec<u8, TX_MAX>>,
    conn: &GattConnection<'_, '_, P>,
    text: &str,
) {
    let mut reply: Vec<u8, TX_MAX> = Vec::new();
    let _ = reply.extend_from_slice(&text.as_bytes()[..text.len().min(TX_MAX)]);
    let _ = tx.notify(conn, &reply).await;
}

/// `interval <min_us> <max_us>` → Some((min, max)).
fn interval_request(bytes: &[u8]) -> Option<(u64, u64)> {
    let s = core::str::from_utf8(bytes).ok()?.trim();
    let mut it = s.strip_prefix("interval")?.split_whitespace();
    let min = it.next()?.parse().ok()?;
    let max = it.next().map_or(Some(min), |v| v.parse().ok())?;
    Some((min, max))
}

/// Boot-stage heap figures, kept so they can be re-printed on every connect:
/// a non-resetting serial reader attaches after the boot has already scrolled.
static BOOT_STAGES: critical_section::Mutex<
    core::cell::RefCell<heapless::Vec<(&'static str, usize, usize), 8>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(heapless::Vec::new()));

fn heap(stage: &'static str) {
    let (free, used) = (esp_alloc::HEAP.free(), esp_alloc::HEAP.used());
    println!("[BLE] heap stage={stage} free={free} used={used}");
    if matches!(
        stage,
        "heap-init" | "rtos-started" | "controller-up" | "host-built"
    ) {
        critical_section::with(|cs| {
            let _ = BOOT_STAGES.borrow_ref_mut(cs).push((stage, free, used));
        });
    }
}

fn reprint_boot_stages() {
    critical_section::with(|cs| {
        for (stage, free, used) in BOOT_STAGES.borrow_ref(cs).iter() {
            println!("[BLE] boot stage={stage} free={free} used={used}");
        }
    });
}
