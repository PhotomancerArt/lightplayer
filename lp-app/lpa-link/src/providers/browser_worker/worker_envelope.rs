use serde::{Deserialize, Serialize};

/// How the browser worker advances the firmware clock.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTickMode {
    /// The worker owns a timer and ticks with real measured deltas.
    ///
    /// This is the mode used by the Studio simulator so previews animate at
    /// roughly real time even when no protocol request is in flight.
    #[default]
    SelfTicking,
    /// Time advances only when the host sends an explicit `tick` envelope.
    ///
    /// Deterministic mode used by tests, stories, and emulator-style harnesses.
    Explicit,
}

/// Shader-execution tier requested for (and recorded on) a runtime.
///
/// Selection is explicit and happens once at runtime creation (fidelity-tiers
/// ADR): a `Gpu` request while the worker has no WebGPU device yields a
/// CPU-tier runtime whose [`BrowserOutputEnvelope::RuntimeCreated`] carries
/// the reason — surfaced, never silent.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeTier {
    /// Q32 on `lpvm-wasm` (authoritative tier; the browser default).
    #[default]
    Cpu,
    /// f32 on WebGPU via `lp-gfx-wgpu` (preview tier).
    Gpu,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserInputEnvelope {
    Boot {
        label: String,
        fw_browser_module_path: String,
        fw_browser_wasm_path: String,
        tick_mode: BrowserTickMode,
    },
    /// Create an additional named runtime in an already-booted worker.
    ///
    /// The worker answers with [`BrowserOutputEnvelope::RuntimeCreated`],
    /// which records the granted tier (and the reason when a `gpu` request
    /// resolved to `cpu`). Preview surfaces that host several runtimes per
    /// worker use this; the boot runtime keeps serving single-runtime
    /// consumers untouched and is always CPU-tier (the authoritative sim).
    CreateRuntime {
        label: String,
        tier: BrowserRuntimeTier,
    },
    ProtocolIn {
        /// Target runtime; `None` addresses the boot runtime.
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime_id: Option<u32>,
        frame: String,
    },
    Tick {
        /// Target runtime; `None` addresses the boot runtime.
        #[serde(skip_serializing_if = "Option::is_none")]
        runtime_id: Option<u32>,
        delta_ms: Option<u32>,
    },
    /// Tick a runtime and render its bus visual product in one worker turn.
    ///
    /// The worker replies with a binary `preview_pixels` message (transferable
    /// `ArrayBuffer`, surfaced as [`super::PreviewPixelFrame`]) on success or
    /// [`BrowserOutputEnvelope::PreviewError`] on failure. Pixels never ride
    /// the JSON envelope path.
    PreviewFrame {
        runtime_id: u32,
        /// Clock advance before rendering; `None` renders without ticking.
        delta_ms: Option<u32>,
        /// Bus channel carrying the visual product (conventionally `visual.out`).
        channel: String,
        width: u32,
        height: u32,
        /// Caller correlation id echoed back on the pixel frame.
        frame_id: u32,
    },
    /// Tick a GPU-tier runtime and present its bus visual product directly
    /// to the card surface attached via `attach_preview_surface` — zero
    /// readback, zero pixel transfer.
    ///
    /// The worker replies with [`BrowserOutputEnvelope::PreviewPresented`]
    /// on success or [`BrowserOutputEnvelope::PreviewError`] on failure.
    /// The render size is the attached surface's size.
    PresentFrame {
        runtime_id: u32,
        /// Clock advance before rendering; `None` renders without ticking.
        delta_ms: Option<u32>,
        /// Bus channel carrying the visual product (conventionally `visual.out`).
        channel: String,
        /// Caller correlation id echoed back on the completion envelope.
        frame_id: u32,
    },
    Start,
    Stop,
    Drain,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserOutputEnvelope {
    Status {
        #[serde(default)]
        runtime_id: Option<u32>,
        status: String,
        message: Option<String>,
    },
    Log {
        runtime_id: u32,
        level: String,
        target: String,
        message: String,
    },
    ProtocolOut {
        /// Producing runtime, so multi-runtime workers can demultiplex
        /// protocol streams.
        runtime_id: u32,
        frame: String,
    },
    /// Response to [`BrowserInputEnvelope::CreateRuntime`].
    ///
    /// `tier` is the tier actually granted; `tier_reason` explains a `gpu`
    /// request that resolved to `cpu` (fidelity-tiers ADR: recorded and
    /// surfaced, never silent).
    RuntimeCreated {
        runtime_id: u32,
        label: String,
        tier: BrowserRuntimeTier,
        #[serde(default)]
        tier_reason: Option<String>,
    },
    /// A transferred card surface was attached to a GPU-tier runtime
    /// (response to the worker `attach_surface` message sent by
    /// `BrowserWorkerHandle::attach_preview_surface`).
    SurfaceAttached { runtime_id: u32 },
    /// A `present_frame` request completed: the frame is on the card surface.
    ///
    /// Mirrors the timing header of the binary `preview_pixels` message —
    /// there are no pixels to transfer on the GPU tier.
    PreviewPresented {
        runtime_id: u32,
        frame_id: u32,
        tick_ms: f64,
        render_ms: f64,
        posted_epoch_ms: f64,
        wasm_memory_bytes: f64,
    },
    /// A `preview_frame` / `present_frame` / `attach_surface` request failed;
    /// carries the caller's `frame_id` (0 for surface attachment).
    PreviewError {
        runtime_id: u32,
        frame_id: u32,
        message: String,
    },
}
