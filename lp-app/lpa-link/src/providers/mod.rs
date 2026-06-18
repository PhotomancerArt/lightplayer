//! Link provider implementations.
//!
//! Provider IDs use kebab-case and generally follow
//! `{environment}-{mechanism}-{target?}`:
//!
//! - `host-process`: host process runtime backed by `fw-host`
//! - `browser-worker`: browser worker runtime backed by `fw-browser`
//! - `host-serial-esp32`: ESP32 hardware over host OS serial
//! - `browser-serial-esp32`: ESP32 hardware over browser Web Serial
//! - `host-websocket`: host-side websocket connection to an existing server
//! - `browser-websocket`: browser-side websocket connection to an existing server
//!
//! The target segment is optional when the mechanism already carries the whole
//! contract. Include it when management details are target-specific, such as
//! ESP32 flashing, reset, and filesystem behavior.

#[cfg(feature = "browser-worker")]
pub mod browser_worker;
pub mod fake;
#[cfg(feature = "host-process")]
pub mod host_process;
#[cfg(feature = "host-serial-esp32")]
pub mod host_serial_esp32;
