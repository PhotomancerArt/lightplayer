#[cfg(feature = "test_button")]
pub mod button;
#[cfg(not(any(
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_gpio",
    feature = "test_gpio_calibrate",
    feature = "test_button",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_msafluid",
    feature = "test_fluid_demo",
    feature = "test_jit_math_perf",
    feature = "test_espnow",
)))]
pub mod manifest_loader;
