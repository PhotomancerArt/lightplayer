//! Structured worker envelope types.
//!
//! The worker envelope is intentionally separate from `lpc_wire`: it carries
//! protocol frames, logs, and lifecycle/status messages over browser
//! `postMessage` without pretending the browser worker is a serial port.

use serde::{Deserialize, Serialize};

use lpc_wire::OutputFrameEntry;

/// Message sent from JavaScript into one browser firmware runtime.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BrowserInputEnvelope {
    /// Queue one complete `lpc_wire::ClientMessage` JSON frame.
    ProtocolIn { frame: String },
    /// Advance the runtime by the given delta.
    ///
    /// The delta is opaque to the runtime: the worker JS decides whether it is a
    /// real measured elapsed time (self-ticking mode) or a fixed deterministic
    /// step (explicit mode). The runtime always advances its clock by exactly the
    /// delta it is handed, so a fixed delta yields deterministic advancement.
    Tick { delta_ms: Option<u32> },
    /// Mark the runtime as running for future autorun support.
    Start,
    /// Mark the runtime as stopped for future autorun support.
    Stop,
    /// Return queued output envelopes without ticking.
    Drain,
}

/// Message emitted by one browser firmware runtime.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum BrowserOutputEnvelope {
    /// Runtime lifecycle or health status.
    Status {
        runtime_id: u32,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Firmware log line surfaced outside the worker.
    Log {
        runtime_id: u32,
        level: String,
        target: String,
        message: String,
    },
    /// One complete `lpc_wire::WireServerMessage` JSON frame.
    ///
    /// `runtime_id` demultiplexes protocol streams when one worker hosts
    /// several runtimes (e.g. the Studio preview lab).
    ProtocolOut { runtime_id: u32, frame: String },
}

/// The `preview_output_frame` answer to a `preview_frame` / `present_frame`
/// that asked for the output side.
///
/// It is composed here rather than in the worker script so the whole shape —
/// discriminator included — stays one Rust type: the script only parses this
/// JSON and posts it. Unlike the runtime outbox envelopes this one is not
/// queued; it belongs to exactly one preview frame and carries that frame's
/// correlation id.
#[derive(Debug, Serialize)]
pub(crate) struct PreviewOutputFrameMessage {
    /// Message discriminator (always `preview_output_frame`), matching the
    /// host's `BrowserOutputEnvelope::PreviewOutputFrame`.
    kind: &'static str,
    runtime_id: u32,
    /// The requesting frame's correlation id, echoed back.
    frame_id: u32,
    /// Whether this project's ROOT scope resolves `control.out` — the
    /// engine-side "leads with lamps" fact, decided where the graph is, never
    /// re-derived from the manifest by the host.
    control_first: bool,
    /// One entry per output node with a published buffer, in tree order.
    /// Empty for a project that drives no outputs (or has not published yet).
    outputs: Vec<OutputFrameEntry>,
}

impl PreviewOutputFrameMessage {
    pub(crate) fn new(
        runtime_id: u32,
        frame_id: u32,
        control_first: bool,
        outputs: Vec<OutputFrameEntry>,
    ) -> Self {
        Self {
            kind: "preview_output_frame",
            runtime_id,
            frame_id,
            control_first,
            outputs,
        }
    }
}
