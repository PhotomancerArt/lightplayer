//! The radio boot probe (feature `radio_boot_probe`) — **scaffolding, never
//! shipped**.
//!
//! The shipped boot with the radio started beside it, and nothing else: the
//! server, the link, the filesystem and the LED output are the product's,
//! untouched. What it adds is enough radio to make the esp-radio blobs touch
//! every window they own on this chip, so the S3 emulator (`lp-emu-esp32s3`)
//! has an image that reaches them (S3 radio roadmap M1, plan
//! `lp2025/2026-10-20-2335-s3-radio-and-ota`):
//!
//! - **Wi‑Fi** — `esp_radio::wifi::new` (the PHY calibration, the MAC, the
//!   baseband), a station configuration (which starts the station), and one
//!   passive-default scan on a task of the main executor;
//! - **Bluetooth** — `BleConnector::new` (the BTDM controller's init and
//!   enable, with `coex` arbitrating the one radio) and one HCI `Reset`
//!   command, whose Command Complete event is read back.
//!
//! Each stage prints one `[RADIO]` line with the heap beside it, so a wedged
//! boot names the stage it wedged in. The scan's and the reset's answers come
//! from the air and the controller, so they are printed after the server is
//! already up and the emulator need not produce them (raising the Wi‑Fi MAC's
//! interrupts is the emulated Wi‑Fi milestone's, M5).
//!
//! # Every line twice: raw, and as a log record
//!
//! The stages run before the link exists, so they print **raw** (esp-println
//! text a host attached from power-on reads, as the emulator's tests do) and
//! are **kept** — the figures are taken where they happen, before anything
//! the link does can move the heap — then re-emitted as `log::info!` records
//! by [`replay_stages_to_log`] once the log ring is installed. That second
//! copy is what a silicon capture sees: `lp-cli validate`'s link host opens
//! the port only after espflash has reset the board and exited, so raw boot
//! text is gone by then, while log records wait in the ring for the link
//! (`radio-boot`'s payload in `lp-emu-validate`). The answers task prints
//! both ways for the same reason.
//!
//! It is not the S3's product radio (M3/M4 own `radio`, `ble` and `wifi`), and
//! it takes no product decision: no radio node, no ESP-NOW, no link over the
//! air. M3 may delete it, or keep it as the emulator's radio image.

use esp_hal::peripherals::{BT, WIFI};

/// The two radio peripherals, taken from the singleton in `init_board` and
/// handed to [`start`] once the scheduler runs (esp-radio needs esp-rtos).
static PERIPHERALS: critical_section::Mutex<
    core::cell::RefCell<Option<(WIFI<'static>, BT<'static>)>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Up to this many stages are kept for [`replay_stages_to_log`]; the probe
/// has six.
const KEPT_STAGES: usize = 8;

/// The stages so far: `(name, heap_free, heap_used)`, as printed.
static STAGES: critical_section::Mutex<
    core::cell::RefCell<([(&str, usize, usize); KEPT_STAGES], usize)>,
> = critical_section::Mutex::new(core::cell::RefCell::new(([("", 0, 0); KEPT_STAGES], 0)));

fn stage(name: &'static str) {
    let (free, used) = (esp_alloc::HEAP.free(), esp_alloc::HEAP.used());
    esp_println::println!("[RADIO] stage={name} heap_free={free} heap_used={used}");
    critical_section::with(|cs| {
        let mut kept = STAGES.borrow_ref_mut(cs);
        let n = kept.1;
        if n < KEPT_STAGES {
            kept.0[n] = (name, free, used);
            kept.1 = n + 1;
        }
    });
}

/// Re-emit every stage kept so far as a log record (module docs). Call once,
/// right after the log ring is installed.
pub fn replay_stages_to_log() {
    let (kept, n) = critical_section::with(|cs| *STAGES.borrow_ref(cs));
    for (name, free, used) in &kept[..n] {
        log::info!("[RADIO] stage={name} heap_free={free} heap_used={used}");
    }
}

/// One answer line, raw and as a log record (module docs).
macro_rules! say {
    ($($arg:tt)*) => {{
        esp_println::println!($($arg)*);
        log::info!($($arg)*);
    }};
}

/// Keep the radio's two peripherals for [`start`]. Called from `init_board`,
/// the one place the peripheral singleton is split.
pub fn stash(wifi: WIFI<'static>, bt: BT<'static>) {
    critical_section::with(|cs| PERIPHERALS.borrow_ref_mut(cs).replace((wifi, bt)));
}

/// Start Wi‑Fi (station + one scan) and the Bluetooth controller (HCI reset).
/// After `esp_rtos::start`; before the server is built, so the heap the radio
/// takes is taken before a project loads, as the C6's order has it.
pub fn start(spawner: embassy_executor::Spawner) {
    let Some((wifi, bt)) = critical_section::with(|cs| PERIPHERALS.borrow_ref_mut(cs).take())
    else {
        esp_println::println!("[RADIO] probe: peripherals were not stashed");
        return;
    };
    stage("start");

    let (mut controller, interfaces) = match esp_radio::wifi::new(wifi, Default::default()) {
        Ok(parts) => parts,
        Err(error) => {
            esp_println::println!("[RADIO] wifi init FAILED: {error:?}");
            return;
        }
    };
    // Dropping an interface stops nothing, but keep them: a later stage that
    // wants the station's frame device finds it alive.
    core::mem::forget(interfaces);
    stage("wifi_new");

    let station = esp_radio::wifi::Config::Station(esp_radio::wifi::sta::StationConfig::default());
    if let Err(error) = controller.set_config(&station) {
        esp_println::println!("[RADIO] station config FAILED: {error:?}");
        return;
    }
    stage("station_started");

    let mut connector = match esp_radio::ble::controller::BleConnector::new(bt, Default::default())
    {
        Ok(connector) => connector,
        Err(error) => {
            esp_println::println!("[RADIO] ble init FAILED: {error:?}");
            return;
        }
    };
    stage("ble_new");

    // HCI Reset (OGF 0x03, OCF 0x0003): an H4 command packet, no parameters.
    match connector.write(&[0x01, 0x03, 0x0c, 0x00]) {
        Ok(_) => stage("ble_hci_reset_sent"),
        Err(error) => esp_println::println!("[RADIO] ble HCI reset FAILED: {error:?}"),
    }

    match radio_answers(controller, connector) {
        Ok(token) => {
            spawner.spawn(token);
            stage("answers_task_spawned");
        }
        Err(_) => esp_println::println!("[RADIO] answers task spawn FAILED"),
    }
}

/// The parts of the probe whose answer comes from the air or the controller:
/// the HCI reset's Command Complete, then one scan. Neither blocks the boot.
#[embassy_executor::task]
async fn radio_answers(
    mut controller: esp_radio::wifi::WifiController<'static>,
    mut connector: esp_radio::ble::controller::BleConnector<'static>,
) {
    let mut event = [0u8; 64];
    match embedded_io_async::Read::read(&mut connector, &mut event).await {
        Ok(n) => say!("[RADIO] ble HCI event: {:02x?}", &event[..n.min(8)]),
        Err(error) => say!("[RADIO] ble HCI read FAILED: {error:?}"),
    }
    // The controller stays up for the boot; dropping the connector would
    // deinit it.
    core::mem::forget(connector);

    let config = esp_radio::wifi::scan::ScanConfig::default().with_max(8);
    match controller.scan_async(&config).await {
        Ok(found) => say!("[RADIO] wifi scan done: {} access points", found.len()),
        Err(error) => say!("[RADIO] wifi scan FAILED: {error:?}"),
    }
    core::mem::forget(controller);
}
