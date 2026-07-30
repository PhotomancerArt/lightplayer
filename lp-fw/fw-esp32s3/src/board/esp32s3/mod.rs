//! ESP32-S3 chip facts.
//!
//! Chip-specific values live here rather than in `fw-esp32-common`: the seam
//! rule is that shared firmware code never learns chip facts, it receives them.

#[cfg_attr(
    not(fw_harness),
    allow(
        dead_code,
        reason = "only the harness reads CPU_HZ today; the app path picks it \
                  up in M3 P5"
    )
)]
pub mod constants;
// Reads `CCOUNT` with inline asm, which is unstable on Xtensa and behind
// `asm_experimental_arch`. The app path deliberately does not enable that
// feature, so this module is harness-only.
#[cfg(fw_harness)]
pub mod cycle_counter;
// Compiled but not yet called: `main.rs` still owns the boot skeleton and P5
// hands it over to `init_board`. See the module doc for the singleton hazard.
#[cfg(not(fw_harness))]
#[allow(
    dead_code,
    reason = "app entrypoint lands in M3 P5; the port is compiled here so it \
              cannot rot between phases"
)]
pub mod init;
// Sole consumer is `serial::io_task`; keep this gate identical to its own.
#[cfg(not(fw_harness))]
pub mod usb_connection;
