//! ESP32-S3 chip facts.
//!
//! Chip-specific values live here rather than in `fw-esp32-common`: the seam
//! rule is that shared firmware code never learns chip facts, it receives them.

// Gated on the consuming feature rather than on `fw_harness`: the JIT corpus
// is the only build that times anything, and a bare `fw_harness` gate would
// warn in every other harness instead of catching real rot in this one.
#[cfg_attr(
    not(feature = "test_xt_jit_corpus"),
    allow(
        dead_code,
        reason = "only the JIT corpus harness reads CPU_HZ; the app path gets \
                  its timing from embassy-time, not the cycle counter"
    )
)]
pub mod constants;
// Reads `CCOUNT` with inline asm, which is unstable on Xtensa and behind
// `asm_experimental_arch`. The app path deliberately does not enable that
// feature, so this module is harness-only.
#[cfg(fw_harness)]
#[cfg_attr(
    not(feature = "test_xt_jit_corpus"),
    allow(
        dead_code,
        reason = "the JIT corpus harness is the only timer consumer; other \
                  harnesses compile it so it cannot rot"
    )
)]
pub mod cycle_counter;
// The app entrypoint's sole source of the peripheral singleton. See the module
// doc for the hazard that makes it the *only* one.
#[cfg(not(fw_harness))]
pub mod init;
// Sole consumer is `serial::io_task`; keep this gate identical to its own.
#[cfg(not(fw_harness))]
pub mod usb_connection;
