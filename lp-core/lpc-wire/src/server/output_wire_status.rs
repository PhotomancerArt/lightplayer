//! Wire representation of per-output-wire transmission status.
//!
//! Carried in the periodic `Heartbeat` so clients can show each physical
//! wire's health without an extra request. The counters mirror
//! `lp-ws281x`'s per-wire attribution (`WireStats`); assembly lives with
//! the firmware embedder — this crate stays a plain data schema. Absent
//! entirely on targets whose output drivers keep no per-wire attribution
//! (the host server, single-core fallback boots).

use serde::{Deserialize, Serialize};

/// One physical output wire's cumulative transmission counters.
///
/// All counters are since boot, monotonic; a client derives rates or
/// deltas itself. `waved` vs `mux`: a *waved* frame had to wait for a
/// pooled transmitter slot (the second-wave signature at more wires than
/// slots); a *mux* is a slot rebinding through the pad matrix, which the
/// steady state performs routinely at five wires over four slots — churn,
/// not distress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputWireStatus {
    /// Manifest wire index (`/rmt/ws281xK`).
    pub wire: u8,
    /// The GPIO pad this wire drives — the join key against a project's
    /// authored channel rows (endpoint → pin → gpio).
    pub gpio: u8,
    /// Frames the render loop posted for this wire.
    pub posted: u32,
    /// Frames that reached the end of transmission (truncated or not).
    pub sent: u32,
    /// Sent frames that ended on the guard word — torn on the strand.
    pub torn: u32,
    /// Frames that waited for a slot before starting.
    pub waved: u32,
    /// Slot rebinds through the pad matrix.
    pub mux: u32,
    /// Worst post→start latency observed, µs.
    pub queue_wait_max_us: u32,
}
