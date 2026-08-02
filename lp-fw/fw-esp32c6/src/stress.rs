//! P4 stress-load generators: continuous radio load beside the running server.
//!
//! The stress matrix (plan `2026-08-01-1459-rmt-priority-hli`, P4) asks one
//! question of this chip: when the radio stack is genuinely busy, does the RMT
//! refill interrupt still make its deadline? These tasks are the "busy" —
//! ported from the experiment repo's `led-lab-esp32c6/src/stress.rs` scenarios,
//! but running beside the *shipping* server build driving the *shipping*
//! driver, because the deployment path is the thing being measured.
//!
//! * `stress_s2` — a WiFi scan loop (repeated active scans, station mode).
//!   A scan is the harshest radio load the product ever sees: the radio hops
//!   channels and holds long critical sections, and the experiment measured
//!   29 % frame truncation under it on the legacy driver.
//! * `stress_s3` — ESP-NOW broadcast chatter at the maximum payload,
//!   back-to-back. This is lp2025's actual radio usage, louder.
//!
//! Both features replace the ESP-NOW radio *driver* registration in
//! `boot_firmware`: `esp_radio::wifi::new` can run once per boot, and the
//! stress tasks need the controller / interface it returns. The rest of the
//! server (engine, project auto-load, WS281x output, telemetry) is unchanged —
//! a stress build is the server build with a load generator beside it, not a
//! separate harness. The load runs from boot, so a whole capture is one cell.
//!
//! The tasks are cooperative embassy tasks on the same executor as the server
//! loop. Both `await` genuinely (a scan completes in ~1.5–2 s, an ESP-NOW send
//! waits for its completion callback), so the server keeps producing frames
//! while the radio's own RTOS tasks and interrupts contend with the RMT ISR —
//! which is exactly the contention under test. Progress lines are deliberately
//! sparse (one per ~10 s) so serial output does not become an accidental
//! logging-load scenario on top.

use embassy_executor::Spawner;
use esp_hal::peripherals::WIFI;

/// ESP-NOW payload size for S3: the protocol maximum, i.e. the most airtime
/// per send (same constant the experiment harness used).
#[cfg(feature = "stress_s3")]
const ESP_NOW_PAYLOAD: usize = 250;

/// Bring the radio up and spawn the configured load task(s).
///
/// Mirrors `Esp32EspNowRadioDriver::new`'s bring-up: `wifi::new` applies the
/// default station config and starts it, so the station is running when this
/// returns. The controller must stay alive for the whole run — dropping it
/// deinitializes WiFi — so it is either moved into the scan task or leaked.
pub fn start(spawner: Spawner, wifi: WIFI<'static>) {
    let (controller, interfaces) =
        esp_radio::wifi::new(wifi, esp_radio::wifi::ControllerConfig::default())
            .expect("stress: Wi-Fi init failed");

    #[cfg(feature = "stress_s2")]
    spawner.spawn(wifi_scan_task(controller).expect("stress: spawn wifi_scan_task"));
    #[cfg(not(feature = "stress_s2"))]
    core::mem::forget(controller);

    #[cfg(feature = "stress_s3")]
    spawner.spawn(esp_now_task(interfaces.esp_now).expect("stress: spawn esp_now_task"));
    #[cfg(not(feature = "stress_s3"))]
    // The non-ESP-NOW interfaces are dropped in the production driver too;
    // only the controller keeps the stack alive.
    drop(interfaces);

    esp_println::println!(
        "[STRESS] radio up s2={} s3={}",
        cfg!(feature = "stress_s2"),
        cfg!(feature = "stress_s3"),
    );
}

/// S2: repeated active scans, awaited to completion and immediately restarted.
///
/// The scan future is created fresh each iteration: `scan_async` takes an
/// `EVENT_CHANNEL` subscriber (only two exist) and *panics* rather than
/// erroring when it cannot get one, so the subscriber must be released — by
/// the future completing or being dropped — before the next scan starts.
#[cfg(feature = "stress_s2")]
#[embassy_executor::task]
async fn wifi_scan_task(mut controller: esp_radio::wifi::WifiController<'static>) {
    let config = esp_radio::wifi::scan::ScanConfig::default();
    let mut scans: u32 = 0;
    let mut errors: u32 = 0;
    loop {
        match controller.scan_async(&config).await {
            Ok(aps) => {
                scans += 1;
                // A scan takes ~1.5-2 s; one line per ~5 scans keeps the
                // serial link quiet relative to the 10 s telemetry period.
                if scans % 5 == 0 {
                    esp_println::println!(
                        "[STRESS] s2 scans={scans} errors={errors} last_aps={}",
                        aps.len()
                    );
                }
            }
            Err(e) => {
                errors += 1;
                esp_println::println!("[STRESS] s2 scan_err {e:?}");
                embassy_time::Timer::after_millis(100).await;
            }
        }
    }
}

/// S3: ESP-NOW broadcast chatter — bursts of blocking sends at max payload.
///
/// The experiment harness's pattern, deliberately: a fully-async send loop
/// only manages ~30 sends/s here, because each `send_async` wake has to wait
/// for the server loop's ~23 ms frame tick before the executor re-polls the
/// task — the *load generator* gets starved, not the radio. A burst of
/// blocking send+wait pairs (~2 ms of airtime each, the radio's own RTOS
/// tasks preempt freely) keeps the wire genuinely busy; the 1 ms yield
/// between bursts is the server loop's window, exactly like the experiment
/// interleaving bursts between frame batches.
#[cfg(feature = "stress_s3")]
#[embassy_executor::task]
async fn esp_now_task(mut esp_now: esp_radio::esp_now::EspNow<'static>) {
    const BURST: usize = 8;
    let payload = [0xA5u8; ESP_NOW_PAYLOAD];
    let mut sends: u32 = 0;
    let mut errors: u32 = 0;
    loop {
        for _ in 0..BURST {
            let outcome = match esp_now.send(&esp_radio::esp_now::BROADCAST_ADDRESS, &payload) {
                Ok(waiter) => waiter.wait(),
                Err(e) => Err(e),
            };
            match outcome {
                Ok(()) => sends += 1,
                Err(e) => {
                    errors += 1;
                    if errors <= 3 || errors.is_multiple_of(1024) {
                        esp_println::println!("[STRESS] s3 send_err n={errors} {e:?}");
                    }
                }
            }
        }
        // ~500 sends/s in bursts of 8: one line per ~10 s.
        if (sends / BURST as u32).is_multiple_of(512) {
            esp_println::println!("[STRESS] s3 sends={sends} errors={errors}");
        }
        embassy_time::Timer::after_millis(1).await;
    }
}
