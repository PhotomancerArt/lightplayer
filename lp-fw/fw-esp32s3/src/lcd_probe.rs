//! The LED-channels spike's app image (feature `lcd_ws281x_probe`, **never
//! shipped**): the shipped boot with Bluetooth on (this chip's default), plus
//!
//! - **Wi‑Fi scanning without pause** — esp-radio's station started beside
//!   BLE (`coex`), and one scan after another on a task of the main executor;
//! - **the LCD_CAM five-lane check** of [`crate::output::lcd_spike`], one
//!   frame every 20 ms: five ragged lanes (124, 124, 100, 124, 60 LEDs),
//!   encoded, sent, captured by the CAM sampler and decoded on every lane.
//!
//! Every 250 frames it logs `[LCDPROBE] frames=… failed=… scans=… aps=…`,
//! so a transcript says how many frames were checked while the radios ran.
//! A central connected over BLE (the desk's `nus-hold.py`) supplies the
//! connection events; this image does not drive them.
//!
//! The peripherals are stolen: `init_board` hands out neither the LCD_CAM,
//! two GDMA channels, the ten pads, nor (with `ble`) the Wi‑Fi peripheral,
//! and nothing else in this image uses them.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::{Duration, Timer};

use crate::output::lcd_spike::{Pads, Rig, Stress};

static SCANS: AtomicU32 = AtomicU32::new(0);
static APS: AtomicU32 = AtomicU32::new(0);
static SCAN_FAILS: AtomicU32 = AtomicU32::new(0);

const SKIRT: [usize; 5] = [124, 124, 100, 124, 60];

pub fn start(spawner: embassy_executor::Spawner) {
    match wifi_scans() {
        Ok(token) => spawner.spawn(token),
        Err(_) => log::error!("[LCDPROBE] wifi task spawn failed"),
    }
    match lcd_frames() {
        Ok(token) => spawner.spawn(token),
        Err(_) => log::error!("[LCDPROBE] lcd task spawn failed"),
    }
}

#[embassy_executor::task]
async fn wifi_scans() {
    // Let the boot and BLE settle first.
    Timer::after(Duration::from_secs(3)).await;
    // SAFETY: `init_board` drops the Wi‑Fi peripheral in a `ble` image and
    // nothing else in this image claims it.
    let wifi = unsafe { esp_hal::peripherals::WIFI::steal() };
    let (mut controller, interfaces) = match esp_radio::wifi::new(wifi, Default::default()) {
        Ok(parts) => parts,
        Err(error) => {
            log::error!("[LCDPROBE] wifi init FAILED: {error:?}");
            return;
        }
    };
    core::mem::forget(interfaces);
    let station = esp_radio::wifi::Config::Station(esp_radio::wifi::sta::StationConfig::default());
    if let Err(error) = controller.set_config(&station) {
        log::error!("[LCDPROBE] station config FAILED: {error:?}");
        return;
    }
    log::info!(
        "[LCDPROBE] wifi up, scanning (heap_free={})",
        esp_alloc::HEAP.free()
    );
    let config = esp_radio::wifi::scan::ScanConfig::default().with_max(16);
    loop {
        match controller.scan_async(&config).await {
            Ok(found) => {
                SCANS.fetch_add(1, Ordering::Relaxed);
                APS.store(found.len() as u32, Ordering::Relaxed);
            }
            Err(error) => {
                if SCAN_FAILS.fetch_add(1, Ordering::Relaxed) < 3 {
                    log::warn!("[LCDPROBE] scan FAILED: {error:?}");
                }
                Timer::after(Duration::from_millis(500)).await;
            }
        }
    }
}

#[embassy_executor::task]
async fn lcd_frames() {
    Timer::after(Duration::from_secs(4)).await;
    // SAFETY: none of these is handed out by `init_board` or claimed by any
    // other driver in this image (the WS281x endpoints bind their pads only
    // when a project opens them, and the probe's pads are not the panel's).
    let mut rig = unsafe {
        use esp_hal::peripherals::*;
        let pads = Pads {
            lanes: [
                GPIO1::steal().into(),
                GPIO2::steal().into(),
                GPIO3::steal().into(),
                GPIO4::steal().into(),
                GPIO5::steal().into(),
                GPIO6::steal().into(),
                GPIO7::steal().into(),
                GPIO10::steal().into(),
            ],
            clk: GPIO11::steal().into(),
            vsync: GPIO12::steal().into(),
        };
        Rig::new(
            LCD_CAM::steal(),
            DMA_CH3::steal(),
            DMA_CH4::steal(),
            pads,
            None,
            124 * 3,
        )
    };
    log::info!(
        "[LCDPROBE] lcd rig up (heap_free={})",
        esp_alloc::HEAP.free()
    );
    let (mut frames, mut failed, mut worst_frame_us, mut worst_encode_us) = (0u32, 0u32, 0, 0);
    loop {
        let report = rig.frame(&SKIRT, frames, Stress::None);
        frames += 1;
        if !report.ok {
            failed += 1;
        }
        worst_frame_us = worst_frame_us.max(report.frame_us);
        worst_encode_us = worst_encode_us.max(report.encode_us);
        if frames % 250 == 0 {
            log::info!(
                "[LCDPROBE] frames={frames} failed={failed} worst_frame_us={worst_frame_us} \
                 worst_encode_us={worst_encode_us} scans={} aps={} scan_fails={} heap_free={}",
                SCANS.load(Ordering::Relaxed),
                APS.load(Ordering::Relaxed),
                SCAN_FAILS.load(Ordering::Relaxed),
                esp_alloc::HEAP.free(),
            );
        }
        Timer::after(Duration::from_millis(20)).await;
    }
}
