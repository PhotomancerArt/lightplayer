#[cfg(target_arch = "wasm32")]
mod browser_esp32_flash;
#[cfg(target_arch = "wasm32")]
mod browser_serial;
mod browser_serial_esp32_options;
mod provider;

#[cfg(target_arch = "wasm32")]
pub use browser_esp32_flash::{
    BrowserEsp32FirmwareManifest, BrowserEsp32FlashProgress, BrowserEsp32FlashResult,
    BrowserEsp32ProbeResult,
};
#[cfg(target_arch = "wasm32")]
pub use browser_serial::BrowserSerialPortHandle;
pub use browser_serial_esp32_options::{
    BrowserSerialEsp32Options, DEFAULT_ESP32C6_FIRMWARE_MANIFEST_PATH,
};
pub use provider::BrowserSerialEsp32Provider;

#[cfg(test)]
mod tests;
