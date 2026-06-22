mod browser_esp32_flash;
mod browser_serial;
mod browser_serial_esp32_options;
mod provider;

pub use browser_esp32_flash::{
    BrowserEsp32EraseResult, BrowserEsp32FirmwareManifest, BrowserEsp32FlashProgress,
    BrowserEsp32FlashResult, BrowserEsp32ProbeResult,
};
pub use browser_serial::BrowserSerialPortHandle;
pub use browser_serial_esp32_options::{
    BrowserSerialEsp32Options, DEFAULT_ESP32C6_FIRMWARE_MANIFEST_PATH, DEFAULT_ESPTOOL_MODULE_PATH,
};
pub use provider::{BrowserSerialEsp32Provider, descriptor};

#[cfg(test)]
mod tests;
