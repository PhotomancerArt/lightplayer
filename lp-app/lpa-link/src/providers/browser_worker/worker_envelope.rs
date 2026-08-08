//! The host half of the browser-worker vocabulary: one type per message the
//! page sends a preview worker, and one per message it sends back.
//!
//! This is an INTERNAL boundary between two halves of the same build (the
//! worker's mirror lives in `lp-fw/fw-browser/src/envelope.rs`), not the
//! device wire — nothing here is versioned or compatibility-shimmed.
//!
//! # Output-frame delivery rides the frame, it is not a second poll
//!
//! A slot draws either the raster (`visual.out`, presented or blitted) or the
//! project's lamps, and both come from the same tick. So the output frame is
//! requested by the `preview_frame` / `present_frame` the host ALREADY
//! schedules at the slot's fps — `output_frame: Some(gate)` — and answered by
//! one extra [`BrowserOutputEnvelope::PreviewOutputFrame`] carrying that
//! frame's `frame_id`. A separate host-driven poll (the shape the device card
//! feed must use, because a device is at the far end of a serial link) would
//! buy nothing here and could only drift off the present cadence.
//!
//! Two consequences worth keeping:
//!
//! - **Samples ride JSON, pixels never do.** A raster is width × height × 4
//!   bytes and gets a transferable `ArrayBuffer`; an output frame is
//!   `lamps × 3 × 2` bytes, which is the size the device already sends
//!   base64 over a serial link. It rides the ordinary envelope path.
//! - **Geometry travels once.** The request carries the same
//!   [`ControlDisplayLayoutRead`] gate the device feed pulls with, so a
//!   steady card asks `Always` once and `IfChanged` thereafter; the layout
//!   crosses only when it actually moved, never per frame.
//!
//! `output_frame: None` means the host is not reading the output side at all
//! — which is every tick of a shader-only slot once the worker's first answer
//! reported `control_first: false` — so those slots pay nothing.

use lpc_wire::{ControlDisplayLayoutRead, OutputFrameEntry};
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
    /// Destroy a runtime previously created with [`Self::CreateRuntime`],
    /// releasing everything it owns (GPU-tier runtimes drop their graphics
    /// backend and any attached card surface).
    ///
    /// The worker answers with [`BrowserOutputEnvelope::RuntimeDestroyed`].
    /// Destroying an unknown id is a no-op ack (release is idempotent);
    /// destroying the boot runtime is refused with
    /// [`BrowserOutputEnvelope::PreviewError`] (`frame_id` 0) — it is the
    /// authoritative sim serving single-runtime consumers.
    DestroyRuntime {
        runtime_id: u32,
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
        /// Also deliver the project's published output frame this tick — see
        /// the type's docs for why the request rides the frame it belongs to.
        #[serde(skip_serializing_if = "Option::is_none")]
        output_frame: Option<ControlDisplayLayoutRead>,
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
        /// Also deliver the project's published output frame this tick — see
        /// the type's docs for why the request rides the frame it belongs to.
        #[serde(skip_serializing_if = "Option::is_none")]
        output_frame: Option<ControlDisplayLayoutRead>,
    },
    Start,
    Stop,
    Drain,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserOutputEnvelope {
    /// Worker lifecycle report. `status` values include `"booting"`,
    /// `"ready"`, `"error"` (one message failed; the worker keeps
    /// serving), and `"fatal"` — the STICKY poisoned-instance report (an
    /// escaped panic=abort trap condemned the wasm instance; the worker
    /// answers every later message with it and must be respawned —
    /// instance-fatal ADR). A fatal `message` carries the primary panic.
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
    /// Response to [`BrowserInputEnvelope::DestroyRuntime`]: the runtime (if
    /// it existed) has been dropped and its worker memory released.
    ///
    /// Acked for unknown ids too — destruction is an idempotent release.
    RuntimeDestroyed { runtime_id: u32 },
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
    /// The published output frame for a `preview_frame` / `present_frame`
    /// that asked for it (`output_frame: Some(…)`), carrying that frame's
    /// `frame_id`.
    ///
    /// It arrives AFTER the visual answer for the same frame, so a slot's
    /// present accounting and backpressure never wait on the lamp half.
    PreviewOutputFrame {
        runtime_id: u32,
        frame_id: u32,
        /// Whether the project's ROOT scope resolves `control.out` — decided
        /// engine-side (where the graph is), never re-derived from the
        /// manifest by the host. `false` is the host's cue to stop asking.
        control_first: bool,
        /// One entry per output node with a published buffer, in tree order;
        /// empty when the project drives no outputs. Same shape the device
        /// card feed consumes, so hosts share one reader.
        outputs: Vec<OutputFrameEntry>,
    },
    /// A `preview_frame` / `present_frame` / `attach_surface` request failed;
    /// carries the caller's `frame_id` (0 for surface attachment).
    PreviewError {
        runtime_id: u32,
        frame_id: u32,
        message: String,
    },
}
