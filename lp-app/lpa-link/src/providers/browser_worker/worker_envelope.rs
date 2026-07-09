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
    /// The worker answers with [`BrowserOutputEnvelope::RuntimeCreated`].
    /// Preview surfaces that host several runtimes per worker use this; the
    /// boot runtime keeps serving single-runtime consumers untouched.
    CreateRuntime {
        label: String,
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
    RuntimeCreated { runtime_id: u32, label: String },
    /// A `preview_frame` request failed; carries the caller's `frame_id`.
    PreviewError {
        runtime_id: u32,
        frame_id: u32,
        message: String,
    },
}
