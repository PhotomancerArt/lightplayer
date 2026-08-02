//! The HLI experiment's radio-linked stress harness: level-3 vs level-4
//! refill, head to head, same board, same boot.
//!
//! The shipping server cannot boot beside the radio on this chip (M6's RAM
//! ledger), so this harness replaces the entrypoint the way `radio_ram_probe`
//! does: no server, no filesystem — just the two refill paths under test,
//! four RMT channels driving the desk classic's strips, a WiFi scan loop for
//! load (the P4/S2 condition), and the `[WS281X]` telemetry both paths
//! print in the same format (`src=hli4` marks the level-4 lines).
//!
//! Cell schedule (one boot, markers on serial):
//!
//! | cell | refill path | load        | duration |
//! |------|-------------|-------------|----------|
//! | 1    | level-3     | radio idle  | 130 s    |
//! | 2    | level-3     | S2 scan     | 150 s    |
//! | —    | *switch: matrix remapped to CPU interrupt 24*        |
//! | 3    | level-4     | radio idle  | 130 s    |
//! | 4    | level-4     | S2 scan     | 150 s    |
//!
//! then a quiet heartbeat forever (the board is left measurable). Counters
//! are cumulative per driver (each path has its own), so per-cell deltas come
//! from the 10 s telemetry lines nearest the `[CELL]` markers, exactly like
//! P4's captures.
//!
//! The frame loop mirrors the app path deliberately: sequential
//! `send_blocking` per channel with the endpoint layer's 50 ms hang detector,
//! paced to ~21 fps — the P4 classic baseline's shape (quad-strips-v3,
//! 4 × 30 LEDs), so cells are comparable to that table.
//!
//! Desk facts (DOM-Z-102): LED1-4 = GPIO 18/16/14/2, driven from RMT slots
//! 0/2/4/6 (two blocks each). The scan loop is `fw-esp32c6/src/stress.rs`'s
//! S2 generator, gated so idle cells keep the radio up but quiet.

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use esp_hal::gpio::Level;
use esp_hal::rmt::{Rmt, TxChannelConfig, TxChannelCreator};
use esp_hal::time::{Duration, Instant};
use lp_ws281x::ChannelTiming;

use crate::output::rmt::hli::app as hli_app;
use crate::output::rmt::hli::app::FRAME_TIMEOUT;
use crate::output::rmt::shared_driver::{self, RMT_CLOCK};
use crate::output::rmt::v3_rmt::{self, BLOCKS_PER_CHANNEL, TX_BLOCKS};

/// The four transmitting slots (two-block windows) and their strip lengths —
/// quad-strips-v3's load: 4 × 30 LEDs.
const SLOTS: [u8; 4] = [0, 2, 4, 6];
const LEDS_PER_STRIP: usize = 30;
const FRAME_BYTES: usize = LEDS_PER_STRIP * 3;

/// ~21 fps, the P4 baseline's app-path rate.
const FRAME_PERIOD_MS: u64 = 48;

const IDLE_CELL_SECS: u64 = 130;
const SCAN_CELL_SECS: u64 = 150;

/// Scan-task gate: idle cells keep the radio associated-but-quiet.
static SCAN_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Which refill path a cell runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Level3,
    Level4,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Level3 => "l3",
            Mode::Level4 => "hli4",
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));

    // Baud-divisor fix, as the probe entrypoint — but at the desk workflow's
    // 921600 (the probe's 115200 default cost this harness its first capture:
    // the CH340 at the wrong baud reads as pure 0x80/0x00 framing garbage).
    // 921_600 = lpc_model::DEFAULT_SERIAL_BAUD_RATE, spelled literally: the
    // harness build does not link lpc-model.
    let uart_config = esp_hal::uart::Config::default().with_baudrate(921_600);
    let _uart0 = esp_hal::uart::Uart::new(peripherals.UART0, uart_config)
        .expect("uart0 config")
        .with_tx(peripherals.GPIO1)
        .with_rx(peripherals.GPIO3);

    esp_alloc::heap_allocator!(size: crate::HEAP_SIZE);
    esp_println::println!("[INIT] fw-esp32v3 hli_stress harness");
    esp_println::println!(
        "[INIT] chip=esp32 heap={} slots={:?} leds/strip={} frame_period_ms={}",
        crate::HEAP_SIZE,
        SLOTS,
        LEDS_PER_STRIP,
        FRAME_PERIOD_MS,
    );

    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    let sw_int =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    // ---- RMT bring-up, phase A on the shipping level-3 path ----
    let mut rmt = Rmt::new(peripherals.RMT, RMT_CLOCK).expect("rmt init");
    shared_driver::install_isr(&mut rmt);

    let config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output(true)
        .with_idle_output_level(Level::Low)
        .with_carrier_modulation(false)
        .with_memsize(BLOCKS_PER_CHANNEL);
    // Slot → pin: LED1-4 silkscreen order on the DOM-Z-102.
    let _ch0 = rmt
        .channel0
        .configure_tx(&config)
        .expect("slot 0")
        .with_pin(peripherals.GPIO18);
    let _ch2 = rmt
        .channel2
        .configure_tx(&config)
        .expect("slot 2")
        .with_pin(peripherals.GPIO16);
    let _ch4 = rmt
        .channel4
        .configure_tx(&config)
        .expect("slot 4")
        .with_pin(peripherals.GPIO14);
    let _ch6 = rmt
        .channel6
        .configure_tx(&config)
        .expect("slot 6")
        .with_pin(peripherals.GPIO2);
    v3_rmt::init_tx();
    for slot in SLOTS {
        v3_rmt::enable_tx_interrupts(slot);
        v3_rmt::clear_ram(&TX_BLOCKS, slot);
        shared_driver::DRIVER
            .configure_default_clock(slot, &ChannelTiming::WS2812)
            .expect("l3 configure");
    }

    // ---- radio up (station started), scan task spawned but gated off ----
    let (controller, _interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, esp_radio::wifi::ControllerConfig::default())
            .expect("wifi init");
    spawner.spawn(scan_task(controller).expect("stress: scan_task token"));
    esp_println::println!("[STRESS] radio up (station, idle)");

    // ---- the cells ----
    let mut frame_no: u32 = 0;
    run_cell(Mode::Level3, false, IDLE_CELL_SECS, &mut frame_no).await;
    run_cell(Mode::Level3, true, SCAN_CELL_SECS, &mut frame_no).await;

    // Drain: everything idle (sends are sequential+blocking), scans off.
    SCAN_ACTIVE.store(false, Ordering::Relaxed);
    embassy_time::Timer::after_millis(2_000).await;

    // ---- the switch: RMT interrupt re-routed to the level-4 vector ----
    hli_app::install_isr_raw();
    for slot in SLOTS {
        hli_app::DRIVER
            .configure_default_clock(slot, &ChannelTiming::WS2812)
            .expect("hli configure");
    }
    esp_println::println!("[SWITCH] rmt -> level-4 vector (cpu int 24)");

    run_cell(Mode::Level4, false, IDLE_CELL_SECS, &mut frame_no).await;
    run_cell(Mode::Level4, true, SCAN_CELL_SECS, &mut frame_no).await;

    SCAN_ACTIVE.store(false, Ordering::Relaxed);
    esp_println::println!("[STRESS] schedule complete; heartbeat only");
    let mut beats = 0u32;
    loop {
        embassy_time::Timer::after_millis(10_000).await;
        beats += 1;
        esp_println::println!(
            "[HEARTBEAT] t_ms={} beats={} heap_free={}",
            Instant::now().duration_since_epoch().as_millis(),
            beats,
            esp_alloc::HEAP.free(),
        );
    }
}

/// Run one cell: frames at the app-path pace on the given refill path, with
/// the scan generator on or off, for `secs` seconds.
async fn run_cell(mode: Mode, scan: bool, secs: u64, frame_no: &mut u32) {
    SCAN_ACTIVE.store(scan, Ordering::Relaxed);
    let load = if scan { "scan" } else { "idle" };
    esp_println::println!(
        "[CELL] start mode={} load={} t_ms={} secs={}",
        mode.label(),
        load,
        Instant::now().duration_since_epoch().as_millis(),
        secs,
    );

    let cell_end = Instant::now() + Duration::from_secs(secs);
    let mut send_errors: u32 = 0;
    let mut timeouts: u32 = 0;
    let mut frame = [0u8; FRAME_BYTES];
    while Instant::now() < cell_end {
        *frame_no = frame_no.wrapping_add(1);
        for (strip, slot) in SLOTS.into_iter().enumerate() {
            fill_pattern(&mut frame, *frame_no, strip);
            let started = Instant::now();
            let mut timed_out = false;
            // The endpoint layer's exact hang-detector shape, on either path.
            let result = match mode {
                Mode::Level3 => shared_driver::DRIVER.send_blocking(slot, &frame, || {
                    if !timed_out && started.elapsed() > FRAME_TIMEOUT {
                        timed_out = true;
                        shared_driver::DRIVER.abort(slot);
                    }
                }),
                Mode::Level4 => hli_app::DRIVER.send_blocking(slot, &frame, || {
                    if !timed_out && started.elapsed() > FRAME_TIMEOUT {
                        timed_out = true;
                        hli_app::DRIVER.abort(slot);
                    }
                }),
            };
            if result.is_err() {
                send_errors += 1;
            }
            if timed_out {
                timeouts += 1;
            }
        }
        match mode {
            Mode::Level3 => shared_driver::report_telemetry_if_due(),
            Mode::Level4 => hli_app::report_telemetry_if_due(),
        }
        embassy_time::Timer::after_millis(FRAME_PERIOD_MS).await;
    }

    esp_println::println!(
        "[CELL] end mode={} load={} t_ms={} frames_total={} send_errors={} timeouts={}",
        mode.label(),
        load,
        Instant::now().duration_since_epoch().as_millis(),
        frame_no,
        send_errors,
        timeouts,
    );
}

/// A cheap moving pattern: distinct per strip, changes every frame, and —
/// unlike a solid color — exercises both pulse codes in every byte position.
fn fill_pattern(frame: &mut [u8; FRAME_BYTES], frame_no: u32, strip: usize) {
    for led in 0..LEDS_PER_STRIP {
        let phase = (frame_no as usize).wrapping_add(led * 8).wrapping_add(strip * 64) as u8;
        frame[led * 3] = phase;
        frame[led * 3 + 1] = phase.wrapping_add(85);
        frame[led * 3 + 2] = phase.wrapping_add(170);
    }
}

/// S2: repeated active scans while the gate is open — the C6 harness's
/// generator, gated. Each iteration creates the scan future fresh (the
/// subscriber-slot lesson from `fw-esp32c6/src/stress.rs`).
#[embassy_executor::task]
async fn scan_task(mut controller: esp_radio::wifi::WifiController<'static>) {
    let config = esp_radio::wifi::scan::ScanConfig::default();
    let mut scans: u32 = 0;
    let mut errors: u32 = 0;
    loop {
        if !SCAN_ACTIVE.load(Ordering::Relaxed) {
            embassy_time::Timer::after_millis(200).await;
            continue;
        }
        match controller.scan_async(&config).await {
            Ok(aps) => {
                scans += 1;
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
