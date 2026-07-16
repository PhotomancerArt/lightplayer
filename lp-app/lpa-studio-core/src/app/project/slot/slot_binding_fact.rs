//! Authored binding facts extracted from a node's def slot tree.

use crate::UiBindingEndpoint;

/// One authored binding parsed from a node's root `bindings` map.
///
/// Facts are extracted from the def root's `bindings` child and applied to
/// the sibling slots they name — consumed/config slots on the def root and
/// produced slots on the state root. Since M0 (bindings-at-node-roots ADR),
/// binding keys always name root-level slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotBindingFact {
    /// Local slot name the binding is keyed by.
    pub slot: String,
    /// Which side of the binding the endpoint supplies.
    pub kind: SlotBindingFactKind,
}

/// The remote side of an authored binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SlotBindingFactKind {
    /// The slot consumes from an endpoint (`"source": "bus:time"`).
    Source(UiBindingEndpoint),
    /// The slot publishes to an endpoint (`"target": "bus:trigger"`).
    Target(UiBindingEndpoint),
    /// The slot is fed an authored literal (`"value": …`).
    Literal(UiBindingEndpoint),
}
