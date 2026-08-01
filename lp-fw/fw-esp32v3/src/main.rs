//! LightPlayer firmware for the classic ESP32 ("v3", WROOM-32E, Xtensa LX6).
//!
//! P1 of the classic-ESP32 bring-up roadmap
//! (`~/.photomancer/planning/lp2025/2026-07-31-1444-classic-esp32-bringup/`):
//! boot to a serial hello and nothing else. No server, no littlefs, no
//! output driver, no `fw-esp32-common` — those arrive when a later phase
//! needs them, mirroring how fw-esp32s3 grew them incrementally rather than
//! all at once at the app-layer milestone. See `Cargo.toml`'s workspace
//! comment for why this crate stands alone rather than joining the
//! repo-root workspace the way fw-esp32s3 does.
//!
//! ## Panic posture
//!
//! Abort tier (ADR `2026-07-29-per-chip-fw-toolchains`), same tier as
//! fw-esp32s3: `panic=abort` (`.cargo/config.toml`), no `unwinding`, no
//! `.eh_frame`. Unlike fw-esp32s3 this crate does not yet carry the
//! `lp-recovery` RTC crash ledger — that is a real lp2025-internal
//! dependency and P1 was told to add none it doesn't need to boot — so the
//! panic handler below is the same "print, then reset" shape fw-esp32s3's
//! own harness builds use when they, too, have no ledger installed.

#![no_std]
#![no_main]

use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::uart::{Config as UartConfig, Uart};

esp_bootloader_esp_idf::esp_app_desc!();

/// Heap for the allocator.
///
/// Conservative starting point, unlike fw-esp32s3's measured-on-failure
/// 240 KB: nothing in this P1 skeleton allocates at all (no shader JIT, no
/// server, no filesystem), so there is no failure to size against yet. The
/// classic ESP32's `dram_seg` (SRAM2) is only ~192 KB against the S3's
/// ~342 KB (see fw-esp32s3's `HEAP_SIZE` doc comment and the experiment
/// repo's `xt-runner-esp32`, which reserves 96 KB out of the same budget for
/// the same reason), so 100 KB leaves comfortable stack headroom while still
/// being large enough to not immediately need revisiting. P3 of this
/// roadmap is what actually measures free heap radio-off and radio-linked
/// and may revise this number.
#[cfg(not(feature = "radio_ram_probe"))]
const HEAP_SIZE: usize = 100 * 1024;

/// Probe heap: the radio stack's own DRAM statics (~10s of KB of `.bss`)
/// come out of the same 192 KB `dram_seg` the arena does, so the arena must
/// shrink for the image to link at all. 72 KB is the experiment repo's
/// proven radio-coexistent size on this chip (led-lab-esp32 `test_stress`).
#[cfg(feature = "radio_ram_probe")]
const HEAP_SIZE: usize = 72 * 1024;

/// Abort-tier panic handler. Print what panicked, then reset so the next
/// boot starts clean — there is no ledger yet to stage a breadcrumb into
/// (contrast fw-esp32s3's `recovery::panic_path::stage_and_reset`, which
/// this becomes a real consumer of once a later phase adds `lp-recovery`).
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    esp_hal::system::software_reset()
}

#[esp_hal::main]
fn main() -> ! {
    // `esp_hal::init` disables the RTC super watchdog (where present),
    // the RTC watchdog (RWDT), and both TIMG0/TIMG1 watchdogs unconditionally
    // (esp-hal 1.1.1 `lib.rs::init`) — nothing here needs to disable them a
    // second time. `CpuClock::max()` matches fw-esp32s3's `init_board` (240
    // MHz on this chip too), and matters for the same reason the S3's harness
    // comment gives: printed/measured timings assume the fast clock.
    let peripherals =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // ⚠️ Load-bearing, not decorative. `esp-println`'s `uart` feature (see
    // Cargo.toml) writes UART0's TX FIFO directly but never programs the
    // baud divisor — the ROM leaves a divisor for its own (pre-reclock)
    // clock tree, and after `esp_hal::init()` above reclocks to
    // `CpuClock::max()`, that stale divisor makes every `esp_println!` print
    // garbage at any standard host baud rate. Constructing this `Uart` runs
    // esp-hal's own baud-divisor programming for the *current* (post-init)
    // clock, and `esp-println`'s raw FIFO writes then ride out correctly —
    // it does not matter that esp-println never touches this binding
    // directly, only that the divisor got programmed before the first print
    // below. It must stay alive (bound, not dropped) for that programming to
    // hold. Pins and 115200 8N1 match the experiment repo's
    // `xt-runner-esp32` (TX=GPIO1, RX=GPIO3 — the classic devkit's UART0
    // bridge pins), which is where this exact failure mode (their "C1"
    // finding) was diagnosed and fixed on real classic-ESP32 hardware.
    let _uart0 = Uart::new(peripherals.UART0, UartConfig::default())
        .expect("uart0 config")
        .with_tx(peripherals.GPIO1)
        .with_rx(peripherals.GPIO3);

    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    esp_println::println!("[INIT] fw-esp32v3 boot");
    esp_println::println!(
        "[INIT] chip=esp32 arch=xtensa heap_free={}",
        esp_alloc::HEAP.free()
    );

    // M2-P3 RAM probe: bring the radio stack all the way up to an initialised
    // STA controller — the deployment-shaped memory layout — printing the
    // heap ledger at each stage. `[PROBE]` lines are the phase deliverable;
    // the stage traces make a wedged boot attributable (experiment stress.rs
    // precedent).
    #[cfg(feature = "radio_ram_probe")]
    {
        esp_println::println!(
            "[PROBE] stage=pre_rtos heap_size={HEAP_SIZE} heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let sw_int = esp_hal::interrupt::software::SoftwareInterruptControl::new(
            peripherals.SW_INTERRUPT,
        );
        esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
        esp_println::println!(
            "[PROBE] stage=rtos_started heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        let (mut controller, _interfaces) =
            esp_radio::wifi::new(peripherals.WIFI, Default::default())
                .expect("radio probe: wifi init");
        esp_println::println!(
            "[PROBE] stage=wifi_new heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        let station_config = esp_radio::wifi::sta::StationConfig::default();
        controller
            .set_config(&esp_radio::wifi::Config::Station(station_config))
            .expect("radio probe: sta config");
        esp_println::println!(
            "[PROBE] stage=sta_started heap_free={} heap_used={}",
            esp_alloc::HEAP.free(),
            esp_alloc::HEAP.used()
        );
        // Keep the controller alive so the heartbeat below reports the
        // steady-state radio-on ledger, not a post-drop one.
        core::mem::forget(controller);
        esp_println::println!("[PROBE] done — heartbeat shows steady-state radio-on heap");
    }

    esp_println::println!("[INIT] ready");

    let delay = Delay::new();
    let mut uptime_s: u32 = 0;
    loop {
        delay.delay_millis(1000);
        uptime_s += 1;
        esp_println::println!(
            "[HEARTBEAT] uptime_s={uptime_s} heap_free={}",
            esp_alloc::HEAP.free()
        );
    }
}
