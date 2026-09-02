mod browser_esp32_flash;
mod browser_serial;
mod browser_serial_esp32_options;
mod port_client_io;
mod provider;

pub use browser_esp32_flash::{
    BrowserEsp32EraseResult, BrowserEsp32FilesystemReadResult, BrowserEsp32FirmwareManifest,
    BrowserEsp32FlashProgress, BrowserEsp32FlashResult, BrowserEsp32ProbeResult,
};
pub use browser_serial::{BrowserSerialPortHandle, granted_ports, install_serial_events};
pub use browser_serial_esp32_options::{
    BrowserSerialEsp32Options, DEFAULT_ESPTOOL_MODULE_PATH, DEFAULT_FIRMWARE_BASE_PATH,
};
pub use port_client_io::LensTapLine;
pub use provider::{BrowserSerialEsp32Provider, GrantedSerialEndpoint, descriptor};

#[cfg(test)]
mod tests;
