//! Binding-graph probe: the project's effective bindings and bus channels.
//!
//! One probe returns the whole runtime binding graph — every registered
//! binding (authored and default, including bindings on implicit runtime
//! consumed slots that have no def field) plus a summary of every bus
//! channel those bindings reference. Channel provider/consumer lists index
//! into the binding list so sites are never duplicated.
//!
//! The graph is a snapshot derived from the runtime binding index; the bus
//! itself stays virtual (demand-resolved). Channel values are resolved on
//! demand when `include_values` is set, so a topology-only read costs no
//! resolution work. A future materialized bus can serve the same contract.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{Kind, LpValue, NodeId, Revision, SlotPath};

/// Request the project's effective binding graph and bus channel summary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BindingGraphProbeRequest {
    /// Resolve and include each channel's current value.
    pub include_values: bool,
}

/// Result for one [`BindingGraphProbeRequest`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BindingGraphProbeResult {
    Graph(WireBindingGraph),
    Error { message: String },
}

/// The project's effective binding graph at one revision.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBindingGraph {
    /// Engine revision the snapshot was taken at.
    pub revision: Revision,
    /// Every registered binding, authored and default.
    pub bindings: Vec<WireEffectiveBinding>,
    /// Every bus channel referenced by at least one binding.
    pub channels: Vec<WireBusChannel>,
}

/// One effective binding, anchored to the local slot it feeds or publishes.
///
/// Node identity travels as [`NodeId`]; clients resolve display labels from
/// their node-tree mirror and use the id for navigation (focus/reveal).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireEffectiveBinding {
    /// Node that owns the binding registration.
    pub owner: NodeId,
    /// Node whose slot the binding anchors to (the owner today; explicit so
    /// cross-node ownership never needs a wire change).
    pub node: NodeId,
    /// Anchor slot path. `None` when the binding has no local slot (for
    /// example a literal published straight onto a bus channel).
    pub slot: Option<SlotPath>,
    /// Whether the anchor slot consumes from or publishes to the endpoint.
    pub direction: WireBindingDirection,
    /// The remote side of the binding.
    pub endpoint: WireBindingEndpoint,
    /// Whether the binding was authored or materialized by default policy.
    pub origin: WireBindingOrigin,
    /// Writer priority (higher wins at bus resolution).
    pub priority: i32,
    /// Semantic value kind carried by the binding.
    pub kind: Kind,
}

/// Which way the anchor slot participates in the binding.
#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireBindingDirection {
    /// The anchor slot's value comes from the endpoint.
    Consumes,
    /// The anchor slot's value is published to the endpoint.
    Publishes,
}

/// The remote side of an effective binding.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireBindingEndpoint {
    /// A bus channel by name.
    Bus { channel: String },
    /// Another node's slot.
    NodeSlot { node: NodeId, slot: SlotPath },
    /// An authored literal value.
    Literal { value: LpValue },
}

/// Where an effective binding came from.
#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireBindingOrigin {
    /// Authored in project data.
    Authored,
    /// Materialized from default binding policy (fallback priority).
    Default,
}

/// One bus channel summary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBusChannel {
    /// Channel name (`time`, `trigger`, `visual.out`, …).
    pub name: String,
    /// Established channel kind, when any binding declared one.
    pub kind: Option<Kind>,
    /// Indices into [`WireBindingGraph::bindings`] whose endpoint publishes
    /// to this channel, highest priority first.
    pub providers: Vec<u32>,
    /// Indices into [`WireBindingGraph::bindings`] whose endpoint consumes
    /// from this channel.
    pub consumers: Vec<u32>,
    /// Resolved current value, present when the request asked for values.
    pub value: Option<WireBusChannelValue>,
}

/// A channel's resolved value (or the resolution failure) at the snapshot
/// revision.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBusChannelValue {
    /// Engine revision the value was resolved at.
    pub revision: Revision,
    /// Resolved value; `None` when resolution failed.
    pub value: Option<LpValue>,
    /// Resolution failure detail when `value` is `None`.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;

    #[test]
    fn binding_graph_round_trips_through_json() {
        let graph = WireBindingGraph {
            revision: Revision::new(7),
            bindings: vec![WireEffectiveBinding {
                owner: NodeId::new(3),
                node: NodeId::new(3),
                slot: Some(SlotPath::parse("trigger").unwrap()),
                direction: WireBindingDirection::Consumes,
                endpoint: WireBindingEndpoint::Bus {
                    channel: "trigger".to_string(),
                },
                origin: WireBindingOrigin::Authored,
                priority: 0,
                kind: Kind::Instant,
            }],
            channels: vec![WireBusChannel {
                name: "trigger".to_string(),
                kind: Some(Kind::Instant),
                providers: vec![],
                consumers: vec![0],
                value: Some(WireBusChannelValue {
                    revision: Revision::new(7),
                    value: None,
                    error: None,
                }),
            }],
        };
        let result = BindingGraphProbeResult::Graph(graph.clone());

        let json = serde_json::to_string(&result).unwrap();
        let decoded: BindingGraphProbeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, BindingGraphProbeResult::Graph(graph));
    }

    #[test]
    fn endpoint_variants_round_trip() {
        for endpoint in [
            WireBindingEndpoint::Bus {
                channel: "visual.out".to_string(),
            },
            WireBindingEndpoint::NodeSlot {
                node: NodeId::new(9),
                slot: SlotPath::parse("entry_time").unwrap(),
            },
            WireBindingEndpoint::Literal {
                value: LpValue::F32(0.5),
            },
        ] {
            let json = serde_json::to_string(&endpoint).unwrap();
            let decoded: WireBindingEndpoint = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, endpoint);
        }
    }
}
