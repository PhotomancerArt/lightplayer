//! LightPlayer firmware for ESP32-S3 (Xtensa LX7).
//!
//! Boot skeleton: brings up the clocks, heap, crash recovery, and serial
//! logging far enough to print the `[INIT]` marker family the hardware gate
//! looks for.
//!
//! The chip-side pieces the app layer needs — `board::esp32s3::init`,
//! `serial`, `flash_storage` — are ported and compiled (M3 P2) but nothing
//! here drives them yet: M3 P5 replaces `boot` with the app entrypoint that
//! does. Until then the linker strips them, so the image size below is not a
//! measure of what they cost. See `board::esp32s3::init` for the
//! peripheral-singleton hazard that hand-off has to resolve.
//!
//! `recovery` is the exception: it is live from this boot onward, because a
//! crash path that only starts working once the app exists is a crash path that
//! cannot report the app failing to come up.

#![no_std]
#![no_main]
// `rsr.ccount` in board::esp32s3::cycle_counter — Xtensa inline asm is still
// behind this gate upstream. Scoped to harness builds so the app path keeps a
// clean feature set.
#![cfg_attr(fw_harness, feature(asm_experimental_arch))]
#![cfg_attr(
    fw_harness,
    allow(
        unstable_features,
        reason = "asm_experimental_arch is required to read Xtensa's CCOUNT \
                  cycle counter; harness builds only"
    )
)]

// The JIT harness allocates (JIT buffers, module tables); the app path leaks
// the recovery instance into a `&'static mut` at boot. `test_backtrace_oracle`
// is the exception — it is deliberately allocation-free, because it exercises
// a walk the panic path takes, and the panic path must not allocate.
#[cfg(any(not(fw_harness), feature = "test_xt_jit_corpus"))]
extern crate alloc;

use esp_hal::main;

mod board;
#[cfg(not(fw_harness))]
#[allow(
    dead_code,
    reason = "the app entrypoint that mounts the filesystem lands in M3 P5"
)]
mod flash_storage;
#[cfg(not(fw_harness))]
mod recovery;
#[cfg(not(fw_harness))]
#[allow(
    dead_code,
    reason = "nothing spawns io_task until M3 P5 wires the app entrypoint; \
              the transport is ported here so the two phases cannot drift"
)]
mod serial;
#[cfg(fw_harness)]
mod tests;

esp_bootloader_esp_idf::esp_app_desc!();

/// Heap for the allocator. Sized conservatively for the boot skeleton; the S3
/// has far more SRAM than the C6, and the real budget lands with the server
/// stack.
const HEAP_SIZE: usize = 64 * 1024;

/// Abort-tier panic handler (ADR 2026-07-29-per-chip-fw-toolchains): stage a
/// breadcrumb into the `lp-recovery` RTC ledger, then reset, so the next boot
/// can report what died.
///
/// Deliberately NOT the C6's shape — that one calls `unwinding::begin_panic` so
/// `catch_unwind` can recover a failing node render, and it needs
/// `panic = "unwind"` plus retained `.eh_frame`. This chip takes the abort tier
/// instead, so a panic is terminal for the boot. See `recovery::panic_path` for
/// the rest of the reasoning, including why the C6's esp-sync reentrant-lock
/// guard has no counterpart here.
///
/// Harness builds never boot recovery — no RTC ledger is installed and there is
/// no next boot that would read one — so they take the bare print-and-reset
/// path rather than linking the whole subsystem for a no-op.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    #[cfg(fw_harness)]
    {
        esp_println::println!("\n\n====================== PANIC ======================");
        esp_println::println!("{info}");
        esp_hal::system::software_reset()
    }
    #[cfg(not(fw_harness))]
    recovery::panic_path::stage_and_reset(info)
}

#[main]
fn boot() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    esp_println::println!("[INIT] fw-esp32s3 boot");
    esp_println::println!("[INIT] chip=esp32s3 arch=xtensa heap={HEAP_SIZE}");

    // Crash recovery first, before anything crash-prone runs: this both reports
    // the previous run and gives everything after it somewhere to leave a
    // breadcrumb. It needs the heap (the instance is leaked into a
    // `&'static mut`), so it cannot move above the allocator.
    //
    // The RWDT is NOT armed here on purpose — `recovery::watchdog` explains
    // why arming without the io_task that feeds it would just boot-loop the
    // board. M3 P5 arms it next to the spawn.
    #[cfg(not(fw_harness))]
    let _boot_assessment = recovery::boot_report::init_and_report();

    esp_println::println!("[INIT] ready");

    // Harness builds hand off here instead of parking. The app path (server,
    // storage, output) is a later milestone; today it parks.
    #[cfg(feature = "test_xt_jit_corpus")]
    tests::xt_jit_corpus::run_all();

    #[cfg(feature = "test_backtrace_oracle")]
    tests::backtrace_oracle::run_all();

    #[cfg(not(fw_harness))]
    {
        // Reaching the park loop is the whole of this build's boot, so this is
        // where the boot-complete milestone belongs today. Without it every
        // boot counts as incomplete and `lp-recovery` reports `safe_mode=true`
        // from the third boot on — a false alarm on a healthy board. M3 P5
        // hands this off to `fw_esp32_common::server_loop`, which marks it on
        // the first served frame; drop this call then.
        lp_recovery::mark_boot_complete();

        loop {
            // Nothing to schedule yet; the executor and tasks land with the
            // server stack. Xtensa's wait-for-interrupt (`waiti`) needs
            // `asm_experimental_arch`, which the app path does not enable —
            // the executor replaces this loop anyway.
            core::hint::spin_loop();
        }
    }
}
