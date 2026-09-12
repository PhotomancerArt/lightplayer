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
//! `tests/browser_serial_conformance.rs` drives it over the polyfill. The
//! CI half (`just lpa-link-browser-test`) runs in the runner's browser —
//! Firefox on CI, where the polyfill is the whole `navigator.serial` — and the
//! live half against a real `emu serve` (`just lpa-link-browser-test-live`)
//! runs in Chrome; the suite is documented as runnable locally in Chrome and
//! Brave too (`AGENTS.md`). The frozen-file discipline that once backed the
//! "runs unchanged" claim mechanically (a content-hash lint) was retired when
//! emulator plan two closed — the rule now lives permanently in
//! `docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md`,
//! rule 1.

mod browser_esp32_flash;
mod browser_serial;
mod port_client_io;
mod provider;

/// The packaged-firmware URL policy (declared outside the wasm gate in
/// `providers/mod.rs` so the host-tested emu transport shares it);
/// re-exported here so wasm consumers read one module path.
pub use crate::providers::browser_serial_esp32_options::{
    BrowserSerialEsp32Options, DEFAULT_ESPTOOL_MODULE_PATH, DEFAULT_FIRMWARE_BASE_PATH,
};
pub use browser_esp32_flash::{
    BrowserEsp32EraseResult, BrowserEsp32FilesystemReadResult, BrowserEsp32FirmwareManifest,
    BrowserEsp32FlashProgress, BrowserEsp32FlashResult, BrowserEsp32ProbeResult,
};
pub use browser_serial::{BrowserSerialPortHandle, granted_ports, install_serial_events};
pub use port_client_io::LensTapLine;
pub use provider::{BrowserSerialEsp32Provider, GrantedSerialEndpoint, descriptor};

#[cfg(test)]
mod tests;
