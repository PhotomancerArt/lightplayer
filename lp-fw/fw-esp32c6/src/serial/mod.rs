#[cfg(feature = "esp32c6")]
pub mod usb_serial;

// Harness entry points import the concrete serial type via `crate::serial::…`;
// app code constructs it through io_task. Gated to exactly the harnesses that
// name it — `fw_harness` alone is too broad and reads as an unused import in
// the ones that log through esp_println or io_task instead.
#[cfg(all(
    feature = "esp32c6",
    any(
        feature = "test_rmt",
        feature = "test_dither",
        feature = "test_gpio",
        feature = "test_gpio_calibrate",
        feature = "test_msafluid",
        feature = "test_fluid_demo",
        feature = "test_jit_math_perf",
        feature = "test_shader_compile_incremental",
    )
))]
pub use usb_serial::Esp32UsbSerialIo;

#[cfg(all(feature = "esp32c6", any(not(fw_harness), feature = "test_json"),))]
pub mod io_task;

#[cfg(all(feature = "esp32c6", any(not(fw_harness), feature = "test_json"),))]
pub use io_task::io_task;
