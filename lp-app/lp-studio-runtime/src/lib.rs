//! LightPlayer Studio runtime and effect executors.

pub mod demo_project;
pub mod effect_executor;
pub mod error;
pub mod protocol_event;
pub mod worker_envelope;

#[cfg(feature = "host-process")]
pub mod client_session_runtime;
#[cfg(feature = "host-process")]
pub mod harness;
#[cfg(feature = "host-process")]
pub mod host_process_runtime;
#[cfg(feature = "host-process")]
pub mod project_session_runtime;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_protocol_client;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_worker_runtime;

pub use effect_executor::EffectExecutor;
pub use error::StudioRuntimeError;

#[cfg(feature = "host-process")]
pub use harness::{RuntimeHarness, run_host_process_demo};
#[cfg(feature = "host-process")]
pub use host_process_runtime::HostProcessStudioRuntime;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use browser_worker_runtime::{BrowserWorkerStudioRuntime, run_browser_worker_demo};
