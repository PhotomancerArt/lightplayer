//! BLE spike, coexistence half (`test_ble_coex`): Wi-Fi/ESP-NOW up beside BLE,
//! with an ESP-NOW loss meter between two boards.
//!
//! Plan `ble-remote-control`, M2 (desk sitting 1, Run G and Run H). The radio
//! is brought up exactly as the product's `Esp32EspNowRadioDriver` does it —
//! `esp_radio::wifi::new(WIFI, ControllerConfig::default())`, then the ESP-NOW
//! interface on the product's default channel, broadcast — and then this
//! module broadcasts a sequence-numbered frame at a fixed rate and counts what
//! it receives from any other board running the same image.
//!
//! Every frame carries the sender's view of the other direction, so one
//! board's console line holds both directions' counters:
//!
//! - `tx` / `tx_err`: frames this board sent / whose send failed;
//! - `rx` / `rx_last_seq`: frames received from the peer, and the highest
//!   peer sequence seen. Loss over a window = Δ`rx_last_seq` − Δ`rx`;
//! - `peer_rx` / `peer_last_seq`: the same two counters, as the peer last
//!   reported them for THIS board's frames. Loss the other way = Δ`peer_last_seq`
//!   − Δ`peer_rx`;
//! - `rssi_avg` / `peer_rssi`: this board's mean RSSI of the peer's frames
//!   since the previous line, and the peer's last RSSI of this board's frames.
//!
//! Loss here is application-level: esp-radio keeps a 10-deep receive queue
//! and drops the oldest entry when it overflows, so a stalled executor counts
//! the same as a frame lost in the air.
//!
//! Build-time knobs (read with `option_env!`, so changing one rebuilds):
//!
//! - `LP_COEX_BLE=0`: do not bring BLE up at all (the ESP-NOW-alone control,
//!   and the peer board);
//! - `LP_COEX_RF_SWITCH=0`: do not drive the XIAO's RF switch (Run H's
//!   "without the quirk" leg);
//! - `LP_COEX_HZ`: frames per second each way (default 50).

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Ticker};
use esp_println::println;
use esp_radio::esp_now::{BROADCAST_ADDRESS, EspNow};
use esp_radio::wifi::{ControllerConfig, WifiController};

/// The product's default ESP-NOW channel (`DEFAULT_ESPNOW_CHANNEL`).
const ESPNOW_CHANNEL: u8 = 11;
const MAGIC: [u8; 4] = *b"LPCX";
const FRAME_LEN: usize = 32;
const REPORT_EVERY: Duration = Duration::from_secs(2);

/// `LP_COEX_BLE=0` → false.
pub const BLE_ENABLED: bool = !env_is_zero(option_env!("LP_COEX_BLE"));
/// `LP_COEX_RF_SWITCH=0` → false.
pub const RF_SWITCH_ENABLED: bool = !env_is_zero(option_env!("LP_COEX_RF_SWITCH"));
const HZ: u64 = parse_u64_or(option_env!("LP_COEX_HZ"), 50);

/// Bring Wi-Fi and ESP-NOW up the way the product's radio driver does. The
/// controller must outlive the interface, so the caller keeps both.
pub fn bring_up(
    wifi: esp_hal::peripherals::WIFI<'static>,
) -> (WifiController<'static>, EspNow<'static>) {
    let (controller, interfaces) = match esp_radio::wifi::new(wifi, ControllerConfig::default()) {
        Ok(pair) => pair,
        Err(e) => {
            // `{:?}` prints nothing in this build profile; the line still
            // proves where bring-up stopped.
            println!("[COEX] wifi init FAILED: {e:?}");
            panic!("wifi init failed");
        }
    };
    println!("[COEX] wifi up");
    let esp_now = interfaces.esp_now;
    match esp_now.set_channel(ESPNOW_CHANNEL) {
        Ok(()) => println!("[COEX] esp-now channel={ESPNOW_CHANNEL}"),
        Err(_) => println!("[COEX] esp-now set_channel FAILED"),
    }
    match esp_now.version() {
        Ok(v) => println!("[COEX] esp-now version={v}"),
        Err(_) => println!("[COEX] esp-now version query FAILED"),
    }
    println!(
        "[COEX] config ble={} rf_switch={} hz={HZ} frame_len={FRAME_LEN}",
        BLE_ENABLED as u8, RF_SWITCH_ENABLED as u8
    );
    (controller, esp_now)
}

/// Broadcast and count forever.
pub async fn run(esp_now: EspNow<'static>) -> ! {
    let (_manager, mut sender, mut receiver) = esp_now.split();
    let mut stats = Stats::default();
    let mut ticker = Ticker::every(Duration::from_micros(1_000_000 / HZ));
    let mut next_report = Instant::now() + REPORT_EVERY;
    let start = Instant::now();
    loop {
        match select(ticker.next(), receiver.receive_async()).await {
            Either::First(()) => {
                stats.tx_seq = stats.tx_seq.wrapping_add(1);
                let frame = stats.frame();
                match sender.send_async(&BROADCAST_ADDRESS, &frame).await {
                    Ok(()) => stats.tx += 1,
                    Err(_) => stats.tx_err += 1,
                }
                if Instant::now() >= next_report {
                    next_report += REPORT_EVERY;
                    stats.report(start);
                }
            }
            Either::Second(received) => {
                stats.record(received.data(), received.info.rx_control.rssi);
            }
        }
    }
}

#[derive(Default)]
struct Stats {
    tx_seq: u32,
    tx: u32,
    tx_err: u32,
    rx: u32,
    rx_last_seq: u32,
    /// Sequence resets seen (the peer rebooted).
    rx_resets: u32,
    rssi_sum: i32,
    rssi_n: i32,
    last_rssi: i8,
    peer_rx: u32,
    peer_last_seq: u32,
    peer_rssi: i8,
}

impl Stats {
    /// `LPCX` · seq · rx-from-peer · last-peer-seq · last RSSI of the peer.
    fn frame(&self) -> [u8; FRAME_LEN] {
        let mut f = [0u8; FRAME_LEN];
        f[..4].copy_from_slice(&MAGIC);
        f[4..8].copy_from_slice(&self.tx_seq.to_le_bytes());
        f[8..12].copy_from_slice(&self.rx.to_le_bytes());
        f[12..16].copy_from_slice(&self.rx_last_seq.to_le_bytes());
        f[16] = self.last_rssi as u8;
        f
    }

    fn record(&mut self, data: &[u8], rssi: i32) {
        if data.len() < 17 || data[..4] != MAGIC {
            return;
        }
        let word = |i: usize| u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        let seq = word(4);
        if seq <= self.rx_last_seq {
            // The peer restarted; start its counters over rather than
            // counting a negative gap.
            self.rx_resets += 1;
            self.rx = 0;
        }
        self.rx_last_seq = seq;
        self.rx += 1;
        self.peer_rx = word(8);
        self.peer_last_seq = word(12);
        self.peer_rssi = data[16] as i8;
        self.rssi_sum += rssi;
        self.rssi_n += 1;
        self.last_rssi = rssi.clamp(-128, 127) as i8;
    }

    fn report(&mut self, start: Instant) {
        let rssi_avg = if self.rssi_n > 0 {
            self.rssi_sum / self.rssi_n
        } else {
            0
        };
        println!(
            "[COEX] t_ms={} tx={} tx_err={} rx={} rx_last_seq={} rx_resets={} rssi_avg={} peer_rx={} peer_last_seq={} peer_rssi={} heap_used={}",
            start.elapsed().as_millis(),
            self.tx,
            self.tx_err,
            self.rx,
            self.rx_last_seq,
            self.rx_resets,
            rssi_avg,
            self.peer_rx,
            self.peer_last_seq,
            self.peer_rssi,
            esp_alloc::HEAP.used()
        );
        self.rssi_sum = 0;
        self.rssi_n = 0;
    }
}

const fn env_is_zero(v: Option<&str>) -> bool {
    match v {
        Some(s) => s.len() == 1 && s.as_bytes()[0] == b'0',
        None => false,
    }
}

const fn parse_u64_or(v: Option<&str>, default: u64) -> u64 {
    let Some(s) = v else { return default };
    let b = s.as_bytes();
    if b.is_empty() {
        return default;
    }
    let mut n = 0u64;
    let mut i = 0;
    while i < b.len() {
        let d = b[i];
        if d < b'0' || d > b'9' {
            return default;
        }
        n = n * 10 + (d - b'0') as u64;
        i += 1;
    }
    if n == 0 { default } else { n }
}
