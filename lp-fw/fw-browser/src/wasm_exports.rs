//! wasm-bindgen exports used by `fw-browser-worker.js`.

use lpvm_wasm::rt_browser::init_host_exports;
use wasm_bindgen::prelude::*;

use crate::envelope::BrowserInputEnvelope;
use crate::runtime_registry;

/// Initialize LPVM browser host exports.
///
/// Call this once after wasm-bindgen initialization, passing the embedding
/// module's `wasm_bindgen::exports()`.
#[wasm_bindgen]
pub fn fw_browser_init_exports(exports: JsValue) {
    init_host_exports(exports);
}

/// Create a browser-local firmware runtime and return its runtime id.
#[wasm_bindgen]
pub fn create_runtime(label: &str) -> Result<u32, String> {
    runtime_registry::create_runtime(label)
}

/// Number of live browser firmware runtimes.
#[wasm_bindgen]
pub fn runtime_count() -> u32 {
    runtime_registry::runtime_count()
}

/// Handle one input envelope encoded as JSON and return output envelopes JSON.
#[wasm_bindgen]
pub fn handle_envelope_json(runtime_id: u32, envelope_json: &str) -> Result<String, String> {
    let envelope: BrowserInputEnvelope =
        serde_json::from_str(envelope_json).map_err(|error| format!("parse envelope: {error}"))?;
    runtime_registry::with_runtime_mut(runtime_id, |runtime| {
        runtime.handle_envelope(envelope)?;
        runtime.drain_output_json()
    })
}

/// Tick a runtime by `delta_ms` and return output envelopes JSON.
#[wasm_bindgen]
pub fn tick_runtime(runtime_id: u32, delta_ms: u32) -> Result<String, String> {
    runtime_registry::with_runtime_mut(runtime_id, |runtime| {
        runtime.tick(delta_ms.max(1))?;
        runtime.drain_output_json()
    })
}

/// Drain pending output envelopes without ticking.
#[wasm_bindgen]
pub fn drain_output_json(runtime_id: u32) -> Result<String, String> {
    runtime_registry::with_runtime_mut(runtime_id, |runtime| runtime.drain_output_json())
}

/// Materialize the visual product on a bus channel as sRGB RGBA8 pixels.
///
/// The returned `Uint8Array` (width × height × 4 bytes, row-major) is a fresh
/// JS-owned buffer, so the worker can transfer it to the page without copying
/// and without routing pixels through the JSON envelope path.
#[wasm_bindgen]
pub fn render_bus_texture_rgba8(
    runtime_id: u32,
    channel: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    runtime_registry::with_runtime_mut(runtime_id, |runtime| {
        runtime.render_bus_texture_rgba8(channel, width, height)
    })
}
