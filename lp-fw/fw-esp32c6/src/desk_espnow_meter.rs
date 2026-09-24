//! The desk's ESP-NOW loss meter, beside the product server (BLE M4).
//!
//! OFF by default, never shipped. M2's `test_ble_coex` harness measured
//! ESP-NOW receive loss with BLE up, but in a harness: no server, no link mux,
//! no render. M4's stop condition asks the same question of the **product
//! image** — BLE enabled by the device store, a phone connected and idle, ≥10
//! minutes — so this puts the same meter beside the whole server, the way the
//! P4 stress builds put their load generators there (`stress.rs`): the radio
//! comes up as the product's driver brings it up (`esp_radio::wifi::new`,
//! channel 11, broadcast) and becomes a counter instead of a driver, because
//! `wifi::new` runs once per boot.
//!
//! The frame and the `[COEX]` line are `test_ble_coex`'s byte for byte (`LPCX`
//! · seq · rx-from-peer · last-peer-seq · last RSSI), so a peer on either
//! image counts this one, and M2's `coex.py`/`series.py` read the console
//! unchanged. Loss over a window is Δ`rx_last_seq` − Δ`rx` (peer → this board)
//! and Δ`peer_last_seq` − Δ`peer_rx` (this board → peer).
//!
//! `LP_COEX_HZ` (build-time, default 50) sets frames per second each way.
//!
//! ```text
//! cd lp-fw/fw-esp32c6
//! cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 \
//!     --features esp32c6,server,desk_espnow_meter
//! ```

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Ticker};
use esp_hal::peripherals::WIFI;
use esp_radio::esp_now::{BROADCAST_ADDRESS, EspNow};

/// The product's default ESP-NOW channel (`DEFAULT_ESPNOW_CHANNEL`).
const ESPNOW_CHANNEL: u8 = 11;
const MAGIC: [u8; 4] = *b"LPCX";
const FRAME_LEN: usize = 32;
const REPORT_EVERY: Duration = Duration::from_secs(2);
/// A sequence this far below the high-water mark is a peer reboot; anything
/// closer is a duplicate or a reordered frame.
const REBOOT_GAP: u32 = 1000;
const HZ: u64 = parse_u64_or(option_env!("LP_COEX_HZ"), 50);

/// Bring Wi-Fi/ESP-NOW up the product's way and spawn the meter.
pub fn start(spawner: Spawner, wifi: WIFI<'static>) {
    let (controller, interfaces) =
        esp_radio::wifi::new(wifi, esp_radio::wifi::ControllerConfig::default())
            .expect("espnow meter: Wi-Fi init failed");
    // The controller must outlive the interface; dropping it deinitializes
    // Wi-Fi.
    core::mem::forget(controller);
    let esp_now = interfaces.esp_now;
    if esp_now.set_channel(ESPNOW_CHANNEL).is_err() {
        log::error!("[COEX] esp-now set_channel FAILED");
    }
    log::info!("[COEX] meter up: channel={ESPNOW_CHANNEL} hz={HZ} frame_len={FRAME_LEN}");
    spawner.spawn(meter_task(esp_now).expect("espnow meter: spawn"));
}

#[embassy_executor::task]
async fn meter_task(esp_now: EspNow<'static>) {
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
    rx_resets: u32,
    rx_dup: u32,
    rssi_sum: i32,
    rssi_n: i32,
    last_rssi: i8,
    peer_rx: u32,
    peer_last_seq: u32,
    peer_rssi: i8,
}

impl Stats {
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
            if self.rx_last_seq - seq < REBOOT_GAP {
                self.rx_dup += 1;
                return;
            }
            self.rx_resets += 1;
            self.rx = 0;
        }
        self.rx_last_seq = seq;
        self.rx += 1;
        self.peer_rx = word(8);
        self.peer_last_seq = word(12);
        self.peer_rssi = data[16] as i8;
        // The radio's signed byte arrives zero-extended; re-sign it.
        let rssi = i32::from(rssi as u8 as i8);
        self.rssi_sum += rssi;
        self.rssi_n += 1;
        self.last_rssi = rssi as i8;
    }

    fn report(&mut self, start: Instant) {
        let rssi_avg = if self.rssi_n > 0 {
            self.rssi_sum / self.rssi_n
        } else {
            0
        };
        log::info!(
            "[COEX] t_ms={} tx={} tx_err={} rx={} rx_last_seq={} rx_resets={} rx_dup={} rssi_avg={} peer_rx={} peer_last_seq={} peer_rssi={} heap_used={}",
            start.elapsed().as_millis(),
            self.tx,
            self.tx_err,
            self.rx,
            self.rx_last_seq,
            self.rx_resets,
            self.rx_dup,
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
