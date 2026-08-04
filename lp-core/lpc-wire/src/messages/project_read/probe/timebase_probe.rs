//! Timebase probe: the read-only view of one clock's live phasors.
//!
//! `bus:time` carries a [`TimeProduct`] handle, and everything behind that
//! handle — effective seconds, this tick's delta, and every phasor riding on
//! it — lives in the engine's timebase store rather than in any slot. Nothing
//! on the ordinary read surface can see it, which is why the clock's card had
//! no way to answer "what is actually running right now?" (parent D10).
//!
//! This probe is that answer and nothing more: a debug listing, not a control
//! surface. There is no way to create, retune, or delete a phasor through it
//! — phasors materialize on query and despawn on silence, and both of those
//! are the consuming node's business.
//!
//! Rows are inherently transient. A phasor that no consumer asked for in the
//! last couple of seconds is gone from the next result; that is correct, not
//! an error, and clients must render an empty listing as "nothing is running"
//! rather than as a failure.

use alloc::string::String;
use lpc_model::{Revision, TimeProduct};

use super::WireScopeRef;

/// Request the live timebase behind one clock's time product.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct TimebaseProbeRequest {
    /// The time product to inspect — the handle the clock publishes on
    /// `bus:time`. Naming the product (rather than a node id) keeps the
    /// request in the same vocabulary as the render/control product probes.
    pub product: TimeProduct,
}

/// Result of a timebase probe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TimebaseProbeResult {
    /// The clock has published a timebase; `phasors` lists what rides it.
    Timebase {
        product: TimeProduct,
        revision: Revision,
        /// Effective project seconds (rate and scrub applied).
        seconds: f32,
        /// Seconds the timebase advanced on the most recent tick. Negative
        /// while a device scrubs backwards.
        delta_seconds: f32,
        /// Live integrators, in store order. Empty is a normal state: a
        /// project whose shaders declare no phasor has none, and so does one
        /// whose phasors have all gone idle.
        phasors: alloc::vec::Vec<WirePhasorRow>,
    },
    /// The named product resolves to no clock in this runtime (the node is
    /// gone, or has never produced). Structured rather than an error: a card
    /// asking about a node that just left the tree is not a fault.
    Unknown { product: TimeProduct },
}

/// One live phasor integrator.
///
/// Read-only by construction: there is no identity here a client could use to
/// address the integrator, only enough provenance to *name* it in a row.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WirePhasorRow {
    /// Where this integrator's identity came from — which is also who else
    /// is riding it (parent D3).
    pub origin: WirePhasorOrigin,
    /// Current wrapped cycle position, always in `[0,1)`.
    ///
    /// The RAW ramp: `waveform` and `phase_offset` are per-consumer output
    /// shaping applied after the store, so a shared integrator has exactly
    /// one phase and possibly several differently-shaped readings of it.
    /// The listing reports the phase, never a shaped value.
    pub phase: f32,
    /// Completed cycles since the integrator materialized.
    pub cycle: u32,
    /// The period the integrator last advanced at, in seconds. `0.0` means
    /// frozen (or materialized-but-never-advanced).
    pub period_seconds: f32,
}

/// Provenance of a phasor integrator.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WirePhasorOrigin {
    /// Config authored on (or defaulted for) one node's slot: private to
    /// that slot, so nothing else shares its phase.
    Node {
        /// Runtime id of the consuming node.
        node: u32,
        /// The CONSUMED slot the config was resolved for — the uniform's
        /// own path (`phase`), not the config field inside it. That is
        /// what makes the key private: two uniforms on one node are two
        /// integrators.
        slot: String,
    },
    /// Config driven by a bus channel: one integrator for every reader of
    /// that `(scope, channel)`.
    Channel {
        scope: WireScopeRef,
        channel: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::NodeId;

    #[test]
    fn a_timebase_result_round_trips_through_json() {
        let result = TimebaseProbeResult::Timebase {
            product: TimeProduct::new(NodeId::new(2), 0),
            revision: Revision::new(41),
            seconds: 3.5,
            delta_seconds: 0.033,
            phasors: alloc::vec![
                WirePhasorRow {
                    origin: WirePhasorOrigin::Node {
                        node: 8,
                        slot: String::from("phase"),
                    },
                    phase: 0.25,
                    cycle: 3,
                    period_seconds: 4.0,
                },
                WirePhasorRow {
                    origin: WirePhasorOrigin::Channel {
                        scope: WireScopeRef::Module {
                            owner: NodeId::new(1),
                        },
                        channel: String::from("speed"),
                    },
                    phase: 0.5,
                    cycle: 0,
                    period_seconds: 100.0,
                },
            ],
        };

        let json = serde_json::to_string(&result).expect("serialize");
        let parsed: TimebaseProbeResult = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed, result);
    }

    #[test]
    fn an_unknown_clock_round_trips_too() {
        let result = TimebaseProbeResult::Unknown {
            product: TimeProduct::new(NodeId::new(9), 0),
        };

        let json = serde_json::to_string(&result).expect("serialize");

        assert_eq!(
            serde_json::from_str::<TimebaseProbeResult>(&json).expect("deserialize"),
            result
        );
    }
}
