//! LightPlayer firmware for ESP32-S3 (Xtensa LX7).
//!
//! Boot skeleton: brings up the clocks, heap, and serial logging far enough to
//! print the `[INIT]` marker family the hardware gate looks for. The server /
//! radio / output stacks arrive in later phases of M5.

#![no_std]
#![no_main]

use esp_hal::main;

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

    loop {
        // Nothing to schedule yet; the executor and tasks land with the
        // server stack. Xtensa's wait-for-interrupt (`waiti`) needs
        // `asm_experimental_arch`, which is not worth a crate-wide feature
        // gate for a boot skeleton — the executor replaces this loop anyway.
        core::hint::spin_loop();
    }
}
