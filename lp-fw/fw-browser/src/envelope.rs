//! Structured worker envelope types.
//!
//! The worker envelope is intentionally separate from `lpc_wire`: it carries
//! protocol frames, logs, and lifecycle/status messages over browser
//! `postMessage` without pretending the browser worker is a serial port.

use serde::{Deserialize, Serialize};

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
    ProtocolOut { frame: String },
}
