//! Classic ESP32 (LX6) chip facts.
//!
//! Chip-specific values live here rather than in `fw-esp32-common`: the seam
//! rule is that shared firmware code never learns chip facts, it receives
//! them (ADR `2026-07-29-per-chip-fw-toolchains`).
//!
//! There is no `usb_connection` sibling to fw-esp32s3's: this chip has no
//! USB-Serial-JTAG peripheral, so there is no SOF signal to poll and no
//! enumeration state to track. See `crate::serial` for what replaces it.

// The app entrypoint's sole source of the peripheral singleton. See the module
// doc for the hazard that makes it the *only* one.
/// Arming the FPU for the context that runs compiled float code. Unconditional,
/// not gated on `float-f32`: the two instructions cost nothing, and a board that
/// only arms when a feature is on is a board whose failure mode depends on how
/// it was built.
///
/// Reachable from the `shader-compile-stress` harness as well as from the app
/// (M5 P2): that harness runs the real compiler, the compiler does f32
/// arithmetic, and an unarmed `CPENABLE` turns the first of those
/// instructions into `EXCCAUSE=32` — which on a harness image with no
/// recovery ledger is an exception loop, not a message.
pub mod fpu;
/// The app's board bring-up. App-only with **one** exception: it hands out the
/// peripheral singleton and starts the runtime, neither of which a harness
/// that replaces the application has any use for — except the APP-core ROM
/// canary (`test_appcore_rom_path`), which calls the product's own
/// `init_board` on purpose, because a discriminator that reimplemented board
/// bring-up would be measuring itself rather than the product.
///
/// So the gate is a union, not app-only, and the M5 P2 payload harnesses are
/// deliberately NOT in it: `shader-compile-stress` reaches `board` for
/// `fpu::arm()` alone and must not drag the runtime in behind it.
///
/// The `rmt-chase` payload (`test_rmt`, M4 P5) is the second exception and
/// earns it the same way: it drives the product's RMT backend, and the clock,
/// the RMT peripheral and UART0's baud divisor all come out of this one call.
/// It takes `init_board` and **not** `start_runtime` — the harness starts no
/// executor.
#[cfg(any(
    all(feature = "server", not(feature = "radio_ram_probe"), not(fw_harness)),
    feature = "test_appcore_rom_path",
    feature = "test_rmt"
))]
pub mod init;
