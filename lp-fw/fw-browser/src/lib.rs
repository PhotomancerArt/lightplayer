//! Browser/Web Worker LightPlayer firmware runtime.
//!
//! JavaScript owns worker creation and `postMessage`; this crate owns the
//! firmware-shaped runtime behind that boundary: `LpServer`, filesystem,
//! virtual hardware/output, tick state, logs, and protocol message routing.

#![cfg(target_arch = "wasm32")]

mod envelope;
mod executor;
mod gpu;
mod logger;
mod manual_time_provider;
mod preview_surface;
mod runtime;
mod runtime_registry;
mod server_transport;
mod texture_convert;
mod tier;
mod wasm_exports;

pub use wasm_exports::{
    attach_preview_surface, create_runtime, drain_output_json, fw_browser_init_exports,
    handle_envelope_json, init_gpu_device, present_bus_texture, render_bus_texture_rgba8,
    runtime_count, tick_runtime,
};

#[cfg(test)]
mod tests;
