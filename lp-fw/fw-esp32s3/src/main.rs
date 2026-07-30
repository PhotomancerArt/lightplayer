//! LightPlayer firmware for ESP32-S3 (Xtensa LX7).
//!
//! Boot skeleton: brings up the clocks, heap, and serial logging far enough to
//! print the `[INIT]` marker family the hardware gate looks for. The server /
//! radio / output stacks arrive in later phases of M5.

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

// Harness-only: the runner allocates (JIT buffers, module tables). The app
// path has no allocating code yet, and an unconditional `extern crate alloc`
// trips `-W unused-extern-crates` there.
#[cfg(fw_harness)]
extern crate alloc;

use esp_hal::main;

// Also harness-only for now. `board::esp32s3::cycle_counter` reads `CCOUNT`
// with inline asm, which is unstable on Xtensa and gated behind
// `asm_experimental_arch` — a feature the app path deliberately does not
// enable. The app layer will take this module when it needs chip constants.
#[cfg(fw_harness)]
mod board;
#[cfg(fw_harness)]
mod tests;

esp_bootloader_esp_idf::esp_app_desc!();

/// Heap for the allocator. Sized conservatively for the boot skeleton; the S3
/// has far more SRAM than the C6, and the real budget lands with the server
/// stack.
const HEAP_SIZE: usize = 64 * 1024;

/// Abort-tier panic handler (ADR 2026-07-29-per-chip-fw-toolchains): print,
/// then reset. Deliberately NOT the C6's shape — that one calls
/// `unwinding::begin_panic` so `catch_unwind` can recover a failing node
/// render, and it needs `panic=unwind` plus retained `.eh_frame`. This chip
/// takes the abort tier instead, so a panic is terminal for the boot.
///
/// Still missing the breadcrumb: the real handler stages a crash record into
/// the `lp-recovery` RTC region before resetting, so the next boot can report
/// what died. That arrives with the recovery glue; until then a panic is
/// silent across the reset except for this line.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("[PANIC] {info}");
    esp_hal::system::software_reset()
}

#[main]
fn boot() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    esp_println::println!("[INIT] fw-esp32s3 boot");
    esp_println::println!("[INIT] chip=esp32s3 arch=xtensa heap={HEAP_SIZE}");
    esp_println::println!("[INIT] ready");

    // Harness builds hand off here instead of parking. The app path (server,
    // storage, output) is a later milestone; today it parks.
    #[cfg(feature = "test_xt_jit_corpus")]
    tests::xt_jit_corpus::run_all();

    #[cfg(not(fw_harness))]
    loop {
        // Nothing to schedule yet; the executor and tasks land with the
        // server stack. Xtensa's wait-for-interrupt (`waiti`) needs
        // `asm_experimental_arch`, which the app path does not enable — the
        // executor replaces this loop anyway.
        core::hint::spin_loop();
    }
}
