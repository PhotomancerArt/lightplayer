//! Concrete link provider implementations.
//!
//! Each submodule owns one runtime/device integration and its provider-specific
//! resources. Public callers usually enter through `crate::registry` to obtain
//! the enabled provider set; these modules are useful when an application or
//! test wants to construct a specific provider manually.
//!
//! Provider keys use kebab-case and generally follow
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

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_serial_esp32;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_worker;
pub mod fake;
#[cfg(feature = "host-process")]
pub mod host_process;
#[cfg(feature = "host-serial-esp32")]
pub mod host_serial_esp32;

pub use crate::registry::availability::LinkProviderAvailability;
pub use crate::registry::descriptor::LinkProviderDescriptor;
pub use crate::registry::env::LinkEnv;
pub use crate::registry::instance::LinkProviderInstance;
pub use crate::registry::kind::LinkProviderKind;
pub use crate::registry::registry::{LinkProviderRegistry, available_provider_descriptors};
