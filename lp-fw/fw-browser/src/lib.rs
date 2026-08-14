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
mod panic_hook;
mod preview_surface;
mod runtime;
mod runtime_registry;
mod server_transport;
mod texture_convert;
mod tier;
mod wasm_exports;

pub use wasm_exports::{
    attach_preview_surface, capture_poster_rgba8, create_runtime, debug_force_panic,
    drain_output_json, fw_browser_init_exports, handle_envelope_json, init_gpu_device,
    present_bus_texture, render_bus_texture_rgba8, runtime_count, tick_runtime,
};

#[cfg(test)]
mod tests;

// The build's self-description, embedded as a scannable blob in the shipped
// .wasm (extracted by `lp-cli firmware show` and reported on ServerHello in
// M4). Provenance mirrors runtime.rs's set_hello: no VCS facts in this build.
lpc_model::lp_embed_manifest_core! {
    package: env!("CARGO_PKG_NAME"),
    chip_family: "browser",
    chip: "wasm32",
    cargo_target: "wasm32-unknown-unknown",
    profile: if cfg!(debug_assertions) { "debug" } else { "release" },
    commit: "unknown",
    dirty: false,
    wire_proto: lpc_wire::WIRE_PROTO_VERSION,
    features: [
        lpa_server::ENGINE_FEATURE_FRAGMENT,
        lpc_model::manifest::feature_fragment(true, lpc_model::LpFeature::GfxLpvm),
    ],
    limits_json: "{}",
}
