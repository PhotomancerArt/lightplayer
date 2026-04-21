#[cfg(not(any(
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_gpio",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_msafluid",
    feature = "test_fluid_demo",
)))]
mod provider;
mod rmt;

#[cfg(not(any(
    feature = "test_rmt",
    feature = "test_dither",
    feature = "test_gpio",
    feature = "test_usb",
    feature = "test_json",
    feature = "test_msafluid",
    feature = "test_fluid_demo",
)))]
pub use provider::Esp32OutputProvider;
// Public API - will be used when provider is updated
#[allow(unused_imports, reason = "public API reserved for future use")]
pub use rmt::{LedChannel, LedTransaction};
