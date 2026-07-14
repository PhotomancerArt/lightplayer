//! wasm-bindgen exports used by `fw-browser-worker.js`.

use lpvm_wasm::rt_browser::init_host_exports;
use wasm_bindgen::prelude::*;

use crate::envelope::BrowserInputEnvelope;
use crate::tier::RuntimeTier;
use crate::{logger, runtime_registry};

/// Initialize LPVM browser host exports.
///
/// Call this once after wasm-bindgen initialization, passing the embedding
/// module's `wasm_bindgen::exports()`.
#[wasm_bindgen]
pub fn fw_browser_init_exports(exports: JsValue) {
    logger::install();
    init_host_exports(exports);
}

/// Request the worker's WebGPU adapter/device (once per worker, at boot).
///
/// Returns a JSON object string: `{"available":true}` when the device is
/// ready, `{"available":false,"reason":"…"}` when WebGPU is unavailable in
/// this worker. The outcome is recorded and applied to every later `gpu`
/// tier request — boot never fails over it (the CPU tier is always
/// functional, and the recorded reason is surfaced per the fidelity-tiers
/// ADR).
#[wasm_bindgen]
pub async fn init_gpu_device() -> String {
    match crate::gpu::init_device().await {
        Ok(()) => String::from(r#"{"available":true}"#),
        Err(reason) => serde_json::to_string(&serde_json::json!({
            "available": false,
            "reason": reason,
        }))
        .unwrap_or_else(|_| String::from(r#"{"available":false}"#)),
    }
}

/// Create a browser-local firmware runtime on the requested tier
/// (`"cpu"` or `"gpu"`).
///
/// Returns a JSON object string carrying the recorded tier selection:
/// `{"runtime_id":N,"tier":"gpu","tier_reason":null}`. A `"gpu"` request
/// while the worker device is unavailable yields a CPU-tier runtime with
/// `tier_reason` set — the worker script forwards all three fields on the
/// `runtime_created` message so hosts can show the tier badge.
#[wasm_bindgen]
pub fn create_runtime(label: &str, tier: &str) -> Result<String, String> {
    logger::install();
    let requested = match tier {
        "gpu" => RuntimeTier::Gpu,
        "cpu" => RuntimeTier::Cpu,
        other => return Err(format!("unknown runtime tier request: {other:?}")),
    };
    let (runtime_id, selection) = runtime_registry::create_runtime(label, requested)?;
    serde_json::to_string(&serde_json::json!({
        "runtime_id": runtime_id,
        "tier": selection.tier.as_str(),
        "tier_reason": selection.reason,
    }))
    .map_err(|error| format!("serialize runtime creation result: {error}"))
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
/// and without routing pixels through the JSON envelope path. CPU tier only:
/// GPU-tier runtimes keep render products GPU-resident and present via
/// [`present_bus_texture`].
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

/// Attach a transferred `OffscreenCanvas` as a GPU-tier runtime's card
/// surface (the canvas arrives in the worker's `attach_surface` message
/// transfer list).
#[wasm_bindgen]
pub fn attach_preview_surface(
    runtime_id: u32,
    canvas: web_sys::OffscreenCanvas,
) -> Result<(), String> {
    runtime_registry::with_runtime_mut(runtime_id, |runtime| runtime.attach_preview_surface(canvas))
}

/// Render the visual product on a bus channel directly to the runtime's
/// attached card surface — the GPU-tier presentation path (zero readback,
/// no pixel transfer).
#[wasm_bindgen]
pub fn present_bus_texture(runtime_id: u32, channel: &str) -> Result<(), String> {
    runtime_registry::with_runtime_mut(runtime_id, |runtime| runtime.present_bus_texture(channel))
}
