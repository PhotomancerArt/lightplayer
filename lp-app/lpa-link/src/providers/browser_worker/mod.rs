mod browser_worker_options;
mod engine_cache;
mod provider;
mod worker_envelope;
mod worker_handle;

/// Pure boot-wait policy (declared outside the wasm gate in
/// `providers/mod.rs` so its tests run natively); re-exported here so
/// wasm consumers read one module path.
pub use crate::providers::browser_worker_boot_wait as boot_wait;
pub use crate::providers::browser_worker_boot_wait::{
    STUDIO_RUNTIME_WORKER_LABEL, WorkerBootPhase, worker_boot_phase,
};
pub use browser_worker_options::BrowserWorkerOptions;
pub use engine_cache::{EngineAssetPhase, engine_asset_phase, warm_engine_cache};
pub use provider::{BrowserWorkerProvider, descriptor};
pub use worker_envelope::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserRuntimeTier, BrowserTickMode,
};
pub use worker_handle::{BrowserWorkerHandle, OutputWait, PosterPixelFrame, PreviewPixelFrame};

#[cfg(test)]
mod tests;
