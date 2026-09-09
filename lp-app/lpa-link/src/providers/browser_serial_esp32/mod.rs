//! ESP32 hardware over the browser's Web Serial API.
//!
//! # The port under this may not be a board
//!
//! Nothing in this module knows it, and that is the point: `navigator.serial`
//! in the page may be a **polyfill** rather than Chromium's own — a virtual
//! USB bus whose ports are emulated C6 boards running the shipped firmware
//! inside `lp-cli emu serve` (emulator plan two). It is installed from
//! `index.html` under a dev-only `?emu=<url>` flag and lives in two files
//! beside the device controller this module loads:
//!
//! - `lp-app/lpa-studio-web/public/lpa-link/virtual_serial.js` — `install()` /
//!   `uninstall()`, and the `SerialPort` double. `getInfo()` reports
//!   `303a:1001` (Espressif native USB, honestly indistinguishable), a reset
//!   mints a NEW port object the way a replug does, a closed port keeps its
//!   grant, and `readable`/`writable` are null while closed.
//! - `lp-app/lpa-studio-web/public/lpa-link/emulator_port.js` — `EmulatorPort`,
//!   one emulated board as bytes plus a control channel, over the door's two
//!   WebSockets.
//!
//! The shim **translates nothing**: DTR/RTS reach the emulator as the lines
//! they were written with, and the emulator decodes the reset dances itself
//! from the RTS falling edge, exactly as silicon does. So there is no phantom
//! port to hunt and no branch here to find — if this module behaves
//! differently under the shim, the shim is wrong.
//!
//! The claim that this layer runs unchanged is a test, not a hope:
//! `tests/browser_serial_conformance.rs` drives it in a real Chrome
//! (`just lpa-link-browser-test`), and
//! `scripts/check-browser-serial-js-frozen.sh` fails if `browser_serial.js`,
//! `browser_esp32_flash.js` or `browser_esp32_device_controller.js` changes.

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
