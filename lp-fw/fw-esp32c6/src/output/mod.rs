mod rmt;
/// The `lp-ws281x`-based replacement for the legacy `rmt` module, not yet
/// constructed — see its module docs and roadmap M5/P1. `pub` because nothing
/// in this crate calls it yet and the next phase's integration is what will.
pub mod rmt_v2;
#[cfg(any(
    not(fw_harness),
    feature = "test_button",
    feature = "test_gpio_calibrate",
    feature = "test_jit_math_perf",
    feature = "test_shader_compile_incremental"
))]
mod rmt_ws281x_driver;

#[cfg(any(
    not(fw_harness),
    feature = "test_button",
    feature = "test_gpio_calibrate",
    feature = "test_jit_math_perf",
    feature = "test_shader_compile_incremental"
))]
pub use fw_esp32_common::output::provider::Esp32OutputProvider;
#[cfg(any(
    not(fw_harness),
    feature = "test_button",
    feature = "test_gpio_calibrate",
    feature = "test_jit_math_perf",
    feature = "test_shader_compile_incremental"
))]
pub use rmt_ws281x_driver::Esp32RmtWs281xDriver;
// Public API - will be used when provider is updated
#[allow(unused_imports, reason = "public API reserved for future use")]
pub use rmt::{LedChannel, LedTransaction};
