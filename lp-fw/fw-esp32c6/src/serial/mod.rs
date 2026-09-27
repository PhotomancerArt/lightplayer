#[cfg(feature = "esp32c6")]
pub mod usb_serial;

// esp-emu spike scaffolding: the host link on UART0. See the module and the
// `spike_uart0_link` feature note in Cargo.toml.
#[cfg(all(feature = "esp32c6", feature = "spike_uart0_link"))]
pub mod spike_uart0;

// Harness entry points import the concrete serial type via `crate::serial::…`;
// the app's host link is `usb_link_task` (lp-link). Gated to exactly the
// harnesses that name it — `fw_harness` alone is too broad and reads as an
// unused import in the ones that log through esp_println instead.
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

// The host link on lp-link: the app, and the one harness that speaks the
// wire (`test_json`).
#[cfg(all(feature = "esp32c6", any(not(fw_harness), feature = "test_json"),))]
pub mod usb_link_task;

#[cfg(all(feature = "esp32c6", any(not(fw_harness), feature = "test_json"),))]
pub use usb_link_task::usb_link_task;
