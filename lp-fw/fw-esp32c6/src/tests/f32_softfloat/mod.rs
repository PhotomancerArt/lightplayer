//! ESP32-C6 f32 soft-float semantics harness.
//!
//! The C6 has no F extension, but it can still execute **IEEE-754 binary32
//! semantics** through soft-float calls. That makes it the only rv32 *hardware*
//! oracle for f32 available until an F-bearing part (ESP32-S31, RV32IMAFC) is on
//! the desk — and the thing it is an oracle for is semantics, not FPU behavior.
//!
//! Two halves, in increasing scope:
//!
//! 1. [`abi_probe`] calls the soft-float ABI symbols directly and compares raw
//!    result words to IEEE reference bit patterns. This measures the **ESP32-C6
//!    mask ROM** (`rvfplib`), which is a different implementation from the
//!    `compiler_builtins` the host emulator links — so their agreement is
//!    something to establish, not assume.
//! 2. [`shader_cases`] compiles a GLSL shader **on the device** in
//!    `FloatMode::F32`, JITs it, and calls it, checking returned bit patterns.
//!    This exercises lowering, relocation against the ROM addresses, and the
//!    f32 argument/return marshalling together.
//!
//! **This is a test configuration, never product firmware.** `float-f32` is off
//! by default on `lpvm-native` and `lps-builtins` precisely so the shipping C6
//! image does not carry an f32 backend it never enters; see
//! `docs/adr/2026-07-28-esp32c6-flash-budget.md` and roadmap decision D2. Report
//! this harness's image size separately from the shipping one.
//!
//! Run with: `just fwtest-f32-softfloat-esp32c6`

use esp_println::println;

use crate::board::esp32c6::init::{init_board, start_runtime};

mod abi_probe;
mod report;
mod shader_cases;

pub async fn run_f32_softfloat_test(_: embassy_executor::Spawner) -> ! {
    let (sw_int, timg0, _rmt, _usb_device, _gpio18, _flash, _gpio4, _gpio20, _wifi, _rwdt) =
        init_board();
    start_runtime(timg0, sw_int);

    // No GPIO is configured anywhere in this harness. On the C6, GPIO12/13 are
    // the USB D-/D+ lines, and driving them costs a physical replug to recover
    // — a compute-only harness has no reason to go near them.
    embassy_time::Timer::after(embassy_time::Duration::from_millis(200)).await;

    println!("[f32-soft] === ESP32-C6 f32 soft-float harness ===");

    let mut abi = report::Report::default();
    abi_probe::run(&mut abi);
    abi.summary("soft-float-abi");

    let mut shader = report::Report::default();
    shader_cases::run(&mut shader);
    shader.summary("f32-shader");

    let failed = abi.failed + shader.failed;
    let passed = abi.passed + shader.passed;
    println!("[f32-soft] TOTAL {passed} passed, {failed} failed");
    if failed == 0 {
        println!("[f32-soft] === DONE: OK ===");
    } else {
        println!("[f32-soft] === DONE: {failed} FAILURES ===");
    }

    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(60)).await;
    }
}
