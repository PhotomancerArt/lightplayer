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
//!
//! # Structure on change, values every read
//!
//! The answer has two halves that move at very different rates:
//!
//! - the **structure** ([`WireBindingGraph`]): the bindings, and each
//!   channel's identity and static fields (scope, name, kind, providers,
//!   consumers, the primary-visual role). It moves when the wiring does — a
//!   binding registered or removed, a priority or kind changed, a channel
//!   appearing or disappearing, a `panel = "show"` hint changing, a panel
//!   writer engaging or letting go;
//! - the **values** ([`WireBusChannelValues`]): each channel's resolved value,
//!   which moves every tick.
//!
//! The structure is revision-gated (`revision_gate`): the request says
//! [`RevisionGateRead`], and the answer's structure is a
//! [`RevisionGateResult`] — `Unchanged { revision }` on a steady read. The
//! values ride every read that asks for them, as a positional list in the
//! structure's channel order, stamped with the structure revision they were
//! resolved against. A client applies a value list ONLY to a cached
//! structure at that revision; on a mismatch it drops the list and asks the
//! structure `Always` next read. Values are never applied to the wrong
//! channels.
//!
//! The structure revision moves only when the structure does: the engine
//! compares each read's structure by content and stamps the revision at which
//! it last changed. A value can therefore never sit inside the structure —
//! which is why an engaged panel writer is the value-free
//! [`WireBindingEndpoint::PanelWriter`], not a literal carrying the knob's
//! position. See `docs/adr/2026-07-06-binding-graph-probe.md` (amended
//! 2026-09-23).

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{Kind, LpValue, NodeId, Revision, SlotPath};

use super::{RevisionGateRead, RevisionGateResult};

/// Request the project's effective binding graph and bus channel values.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BindingGraphProbeRequest {
    /// Whether and how to ship the graph's structure (revision-gated).
    pub structure: RevisionGateRead,
    /// Resolve and include each channel's current value.
    pub include_values: bool,
}

/// Result for one [`BindingGraphProbeRequest`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BindingGraphProbeResult {
    Graph(WireBindingGraphRead),
    Error { message: String },
}

/// One binding-graph answer: the gated structure and, when asked, the values.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBindingGraphRead {
    /// The graph's structure, revision-gated.
    pub structure: RevisionGateResult<WireBindingGraph>,
    /// Every channel's value, present when the request asked for values.
    pub values: Option<WireBusChannelValues>,
}

/// The project's effective binding graph: its structure at one structure
/// revision. Carries no channel values (see [`WireBusChannelValues`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBindingGraph {
    /// The structure revision: the engine revision at which this structure
    /// was first answered. Moves only when the structure does — never on a
    /// value change, never merely because the engine ticked.
    pub revision: Revision,
    /// Every registered binding, authored and default, plus one row per
    /// engaged panel writer.
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
    /// The consumed slot's declared `panel = "show"` hint: a Default-origin
    /// binding so marked still presents a panel control (the additive
    /// override on ADR 2026-08-03-panel-visibility-is-derived). Additive
    /// field — a server that predates it simply never promotes.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub panel_show: bool,
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

/// Structured identity of one bus scope on the wire (modules.md R1/R2).
///
/// Owners are runtime node ids from the same tree the probe ships, so
/// clients key scoped channels structurally and derive any display string
/// themselves (the engine never flattens scope into a label).
#[derive(
    Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WireScopeRef {
    /// The scope a module node introduces around its project children.
    Module { owner: NodeId },
    /// The anonymous sink scope one playlist entry wraps its child in.
    /// Sink channels never surface in probe listings; the variant exists
    /// so endpoints can still name one structurally.
    Sink { owner: NodeId, entry: u32 },
}

impl WireScopeRef {
    pub fn owner(&self) -> NodeId {
        match self {
            Self::Module { owner } | Self::Sink { owner, .. } => *owner,
        }
    }

    pub fn is_sink(&self) -> bool {
        matches!(self, Self::Sink { .. })
    }
}

/// The remote side of an effective binding.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireBindingEndpoint {
    /// A bus channel, scoped: for a publishing endpoint the scope the
    /// write lands in; for a consuming endpoint the scope its resolution
    /// starts from (writer-shadowing walks outward from there, R5).
    Bus {
        scope: Option<WireScopeRef>,
        channel: String,
    },
    /// Another node's slot.
    NodeSlot { node: NodeId, slot: SlotPath },
    /// An authored literal value.
    Literal { value: LpValue },
    /// An engaged panel writer (a [`WireBindingOrigin::Panel`] row). The
    /// value it holds is deliberately NOT here: it is the channel's value
    /// (the writer outranks every provider), which rides the values list. A
    /// knob turn must not move the structure.
    PanelWriter,
}

/// Where an effective binding came from.
#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireBindingOrigin {
    /// Authored in project data.
    Authored,
    /// An engaged panel writer (lazy runtime state, never authored).
    Panel,
    /// Materialized from default binding policy (fallback priority).
    Default,
}

/// One bus channel summary.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBusChannel {
    /// The scope this channel entry lists in (R3: a channel exists in the
    /// scope of the slots bound to it). Same-named channels in different
    /// scopes are distinct entries. `None` only from scope-less engines
    /// (test fakes).
    pub scope: Option<WireScopeRef>,
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
    /// Engine-reported root-scope role: true for THE channel the root
    /// module's output interface mirrors (the project's primary visual).
    /// Consumers must key off this flag, never off the channel name.
    pub primary_visual: bool,
}

/// Every channel's value at one read, keyed to the structure by position.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireBusChannelValues {
    /// The structure revision these values were resolved against. A client
    /// whose cached structure is at any other revision must drop the list.
    pub structure_revision: Revision,
    /// One entry per [`WireBindingGraph::channels`] row, in that order.
    pub values: Vec<WireBusChannelValue>,
}

/// One channel's value at the read.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireBusChannelValue {
    /// Not resolved: resolving it would put demand on an inactive sink scope
    /// (the R2 no-demand property — a probe never renders an idle playlist
    /// entry).
    Unresolved,
    /// Resolved, and the channel carries no value.
    Empty,
    /// The resolved value.
    Value(LpValue),
    /// Nothing writes the channel in any scope its consumers can see: the
    /// resolver's no-provider case, which consumers treat as a legitimate
    /// empty channel (authored defaults apply). Its own variant rather than
    /// an [`Self::Error`] string because it is the common case on every
    /// project with unwritten inputs, and the channel it names is already the
    /// structure row at the same position — as a string it cost ~60 B per
    /// such channel on every read (lean-wire P6).
    NoProvider,
    /// Resolution failed for any other reason.
    Error(String),
}

impl WireBusChannelValue {
    /// The resolved value, when there is one.
    #[must_use]
    pub fn value(&self) -> Option<&LpValue> {
        match self {
            Self::Value(value) => Some(value),
            Self::Unresolved | Self::Empty | Self::NoProvider | Self::Error(_) => None,
        }
    }

    /// The resolution failure, when resolution failed for a reason other
    /// than [`Self::NoProvider`].
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Error(error) => Some(error),
            Self::Unresolved | Self::Empty | Self::NoProvider | Self::Value(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;

    #[test]
    fn binding_graph_read_round_trips_through_json() {
        let graph = WireBindingGraph {
            revision: Revision::new(7),
            bindings: vec![WireEffectiveBinding {
                owner: NodeId::new(3),
                node: NodeId::new(3),
                slot: Some(SlotPath::parse("trigger").unwrap()),
                direction: WireBindingDirection::Consumes,
                endpoint: WireBindingEndpoint::Bus {
                    scope: Some(WireScopeRef::Module {
                        owner: NodeId::new(0),
                    }),
                    channel: "trigger".to_string(),
                },
                origin: WireBindingOrigin::Authored,
                priority: 0,
                kind: Kind::Instant,
                panel_show: false,
            }],
            channels: vec![WireBusChannel {
                scope: Some(WireScopeRef::Module {
                    owner: NodeId::new(0),
                }),
                name: "trigger".to_string(),
                kind: Some(Kind::Instant),
                providers: vec![],
                consumers: vec![0],
                primary_visual: false,
            }],
        };
        let result = BindingGraphProbeResult::Graph(WireBindingGraphRead {
            structure: RevisionGateResult::Changed(graph),
            values: Some(WireBusChannelValues {
                structure_revision: Revision::new(7),
                values: vec![WireBusChannelValue::Error("no writer".to_string())],
            }),
        });

        let json = serde_json::to_string(&result).unwrap();
        let decoded: BindingGraphProbeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, result);
    }

    /// A steady read is the structure's `unchanged` plus the values, and each
    /// value is a few bytes: no per-channel revision, no null fields.
    #[test]
    fn a_steady_read_is_values_only() {
        let read = WireBindingGraphRead {
            structure: RevisionGateResult::Unchanged {
                revision: Revision::new(40),
            },
            values: Some(WireBusChannelValues {
                structure_revision: Revision::new(40),
                values: vec![
                    WireBusChannelValue::Value(LpValue::F32(0.5)),
                    WireBusChannelValue::Empty,
                    WireBusChannelValue::Unresolved,
                ],
            }),
        };
        let json = crate::json::to_string(&read).unwrap();
        assert_eq!(
            json,
            r#"{"structure":{"unchanged":{"revision":40}},"values":{"structure_revision":40,"values":[{"value":{"f32":0.5}},"empty","unresolved"]}}"#
        );
    }

    #[test]
    fn endpoint_variants_round_trip() {
        for endpoint in [
            WireBindingEndpoint::Bus {
                scope: Some(WireScopeRef::Sink {
                    owner: NodeId::new(3),
                    entry: 7,
                }),
                channel: "visual.out".to_string(),
            },
            WireBindingEndpoint::NodeSlot {
                node: NodeId::new(9),
                slot: SlotPath::parse("entry_time").unwrap(),
            },
            WireBindingEndpoint::Literal {
                value: LpValue::F32(0.5),
            },
            WireBindingEndpoint::PanelWriter,
        ] {
            let json = serde_json::to_string(&endpoint).unwrap();
            let decoded: WireBindingEndpoint = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, endpoint);
        }
    }

    #[test]
    fn channel_value_accessors_split_value_from_error() {
        let value = WireBusChannelValue::Value(LpValue::F32(1.0));
        assert_eq!(value.value(), Some(&LpValue::F32(1.0)));
        assert_eq!(value.error(), None);
        let error = WireBusChannelValue::Error("boom".to_string());
        assert_eq!(error.value(), None);
        assert_eq!(error.error(), Some("boom"));
        assert_eq!(WireBusChannelValue::Unresolved.value(), None);
        assert_eq!(WireBusChannelValue::NoProvider.value(), None);
        assert_eq!(WireBusChannelValue::NoProvider.error(), None);
    }

    #[test]
    fn no_provider_is_a_bare_tag_on_the_wire() {
        let json = serde_json::to_string(&WireBusChannelValue::NoProvider).unwrap();
        assert_eq!(json, r#""no_provider""#);
        let decoded: WireBusChannelValue = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, WireBusChannelValue::NoProvider);
    }
}
