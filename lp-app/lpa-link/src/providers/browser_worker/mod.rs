mod browser_worker_options;
mod provider;
mod worker_envelope;
#[cfg(target_arch = "wasm32")]
mod worker_handle;

pub use browser_worker_options::BrowserWorkerOptions;
pub use provider::BrowserWorkerProvider;
pub use worker_envelope::{BrowserInputEnvelope, BrowserOutputEnvelope};
#[cfg(target_arch = "wasm32")]
pub use worker_handle::BrowserWorkerHandle;

#[cfg(test)]
mod tests;
