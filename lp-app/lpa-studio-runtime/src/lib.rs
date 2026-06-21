//! LightPlayer Studio runtime and effect executors.

pub mod demo_project;
pub mod effect_executor;
pub mod error;
pub mod harness;
pub mod protocol_event;
pub mod scenario;

#[cfg(feature = "host-process")]
pub mod client_session_runtime;
#[cfg(feature = "host-process")]
pub mod host_process_runtime;
#[cfg(feature = "host-process")]
pub mod project_session_runtime;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_protocol_client;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_worker_runtime;

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_serial_protocol_client;
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_serial_runtime;

pub use effect_executor::EffectExecutor;
pub use error::StudioRuntimeError;
pub use harness::RuntimeHarness;

#[cfg(feature = "host-process")]
pub use harness::run_host_process_demo;
#[cfg(feature = "host-process")]
pub use host_process_runtime::HostProcessStudioRuntime;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use browser_worker_runtime::{BrowserWorkerStudioRuntime, run_browser_worker_demo};

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use browser_serial_runtime::{BrowserSerialStudioRuntime, run_browser_serial_demo};
