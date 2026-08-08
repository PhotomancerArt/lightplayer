//! Authored binding facts extracted from a node's def slot tree.

use std::collections::BTreeMap;

use lpc_model::{LpValue, SlotPath, SlotPathSegment};

use crate::{
    ProjectNodeAddress, UiBindingEndpoint, UiSlotValue, app::project::format_slot_map_key,
};

/// One authored binding parsed from a node's root `bindings` map.
///
/// Facts are extracted from the def root's `bindings` child and applied to
/// the sibling slots they name — consumed/config slots on the def root and
/// produced slots on the state root. Since M0 (bindings-at-node-roots ADR),
/// binding keys always name slots reachable from a root by record fields.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotBindingFact {
    /// Local slot name the binding is keyed by — a bare top-level field
    /// (`brightness`), or a DOTTED chain of record fields for a leaf
    /// nested in a record (`transport.rate`, the clock's three transport
    /// leaves). See [`binding_fact_slot_key`].
    pub slot: String,
    /// Which side of the binding the endpoint supplies.
    pub kind: SlotBindingFactKind,
}

/// The remote side of an authored binding.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotBindingFactKind {
    /// The slot consumes from an endpoint (`"source": "bus:time"`).
    Source(UiBindingEndpoint),
    /// The slot publishes to an endpoint (`"target": "bus:trigger"`).
    Target(UiBindingEndpoint),
    /// The slot is fed an authored literal (`"value": …`).
    Literal(UiBindingEndpoint),
}

impl SlotBindingFactKind {
    /// The `BindingDef` field this fact kind is authored under.
    fn field_name(&self) -> &'static str {
        match self {
            Self::Source(_) => "source",
            Self::Target(_) => "target",
            Self::Literal(_) => "value",
        }
    }
}

/// Pending binding edits (edit buffer + overlay mirror) that shadow the
/// synced view's authored binding facts, keyed by node and `bindings[…]`
/// path.
///
/// The synced slot tree only reflects a `bindings` edit after the next
/// passive project read, so fact derivation reads *through* these overrides:
/// a just-acked bind/retarget/unbind flips the per-slot presentation
/// (authored vs default, Unbind affordance) immediately instead of lagging
/// the read cadence. Only paths under the def root's `bindings` field are
/// retained.
#[derive(Debug, Default)]
pub(in crate::app::project) struct BindingFactOverrides {
    by_node: BTreeMap<ProjectNodeAddress, BTreeMap<SlotPath, BindingFactEditOp>>,
}

/// One pending edit that can shadow authored binding facts. Structural
/// `EnsurePresent`s carry no endpoint and change no fact, so they are never
/// stored.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::app::project) enum BindingFactEditOp {
    /// A value assignment at the path (a bind/retarget endpoint, usually).
    Assign(LpValue),
    /// A removal at the path (an unbind, an option toggled off, …).
    Remove,
}

impl BindingFactOverrides {
    /// Record `op` at `path` for `node` when the path is a `bindings[…]`
    /// path; later inserts at the same path win (the callers insert the
    /// overlay mirror first, then the edit buffer).
    pub(in crate::app::project) fn insert(
        &mut self,
        node: ProjectNodeAddress,
        path: SlotPath,
        op: BindingFactEditOp,
    ) {
        let under_bindings = matches!(
            path.segments().first(),
            Some(SlotPathSegment::Field(name)) if name.as_str() == "bindings"
        );
        if !under_bindings {
            return;
        }
        self.by_node.entry(node).or_default().insert(path, op);
    }

    /// Shadow `facts` (the synced view's authored facts for `node`) with the
    /// node's pending binding edits, shallow paths first so a removed entry
    /// does not outlive a deeper re-assignment.
    pub(in crate::app::project) fn apply_to(
        &self,
        node: &ProjectNodeAddress,
        facts: &mut Vec<SlotBindingFact>,
    ) {
        let Some(edits) = self.by_node.get(node) else {
            return;
        };
        for (path, op) in edits {
            apply_binding_edit(facts, path, op);
        }
    }
}

/// The fact key a graph binding's slot path names: its leading chain of
/// record FIELD segments, dotted.
///
/// `brightness` → `"brightness"`, `transport.rate` → `"transport.rate"`.
/// A non-field segment ends the chain — `consumed[speed].default` keys as
/// `"consumed"`, which is the top-level row that has always carried that
/// wiring — so widening this to dotted keys changed nothing for map- or
/// option-shaped paths.
///
/// The dotted form is what a `default_bind` declared on a leaf INSIDE a
/// promoted record produces (clock-tape-hero P6). Keying those by their
/// first segment alone collapsed all three transport leaves onto the one
/// `transport` row, and the grouped control could not then tell which
/// dimension was wired.
pub(in crate::app::project) fn binding_fact_slot_key(slot: &SlotPath) -> Option<String> {
    let mut key = String::new();
    for segment in slot.segments() {
        let SlotPathSegment::Field(name) = segment else {
            break;
        };
        if !key.is_empty() {
            key.push('.');
        }
        key.push_str(name.as_str());
    }
    (!key.is_empty()).then_some(key)
}

/// Fold one pending `bindings[…]` edit into the authored fact list. Only the
/// shapes the binding editors produce are meaningful; anything deeper or
/// oddly-shaped changes nothing (the next project read reconciles it).
fn apply_binding_edit(facts: &mut Vec<SlotBindingFact>, path: &SlotPath, op: &BindingFactEditOp) {
    // segments[0] is the `bindings` field itself (checked at insert).
    let rest = &path.segments()[1..];
    match (rest, op) {
        // The whole map removed: no authored facts survive.
        ([], BindingFactEditOp::Remove) => facts.clear(),
        // `bindings[key]` removed: an unbind drops every fact for the slot.
        ([SlotPathSegment::Key(key)], BindingFactEditOp::Remove) => {
            let slot = format_slot_map_key(key);
            facts.retain(|fact| fact.slot != slot);
        }
        // `bindings[key].source` / `.source.some` removed: that side only.
        (
            [
                SlotPathSegment::Key(key),
                SlotPathSegment::Field(field),
                tail @ ..,
            ],
            BindingFactEditOp::Remove,
        ) if binding_field(field.as_str()) && endpoint_tail(tail) => {
            let slot = format_slot_map_key(key);
            facts.retain(|fact| fact.slot != slot || fact.kind.field_name() != field.as_str());
        }
        // `bindings[key].source.some` assigned: bind/retarget that side.
        (
            [
                SlotPathSegment::Key(key),
                SlotPathSegment::Field(field),
                SlotPathSegment::Field(some),
            ],
            BindingFactEditOp::Assign(value),
        ) if binding_field(field.as_str()) && some.as_str() == "some" => {
            let slot = format_slot_map_key(key);
            facts.retain(|fact| fact.slot != slot || fact.kind.field_name() != field.as_str());
            let endpoint = binding_edit_endpoint(value);
            let kind = match field.as_str() {
                "source" => SlotBindingFactKind::Source(endpoint),
                "target" => SlotBindingFactKind::Target(endpoint),
                _ => SlotBindingFactKind::Literal(endpoint),
            };
            facts.push(SlotBindingFact { slot, kind });
        }
        _ => {}
    }
}

/// True for the `BindingDef` fields that carry one side of a binding.
fn binding_field(name: &str) -> bool {
    matches!(name, "source" | "target" | "value")
}

/// True when `tail` addresses the binding field itself or its option payload.
fn endpoint_tail(tail: &[SlotPathSegment]) -> bool {
    match tail {
        [] => true,
        [SlotPathSegment::Field(some)] => some.as_str() == "some",
        _ => false,
    }
}

/// Endpoint carried by a pending binding-side assignment, mirroring
/// `SlotController::binding_endpoint`: endpoint strings bind as-is, anything
/// else displays as an authored literal.
fn binding_edit_endpoint(value: &LpValue) -> UiBindingEndpoint {
    match value {
        LpValue::String(endpoint) => UiBindingEndpoint::new(endpoint.clone()),
        other => UiBindingEndpoint::new(UiSlotValue::from_lp_value(other).display)
            .with_detail("literal value"),
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::SlotPath;

    use super::binding_fact_slot_key;

    /// A leaf `default_bind` inside a promoted record keys by its DOTTED
    /// field chain, so all three of the clock's transport leaves get their
    /// own row instead of collapsing onto `transport` (P8).
    #[test]
    fn a_record_leaf_keys_by_its_dotted_field_chain() {
        let key = |path: &str| binding_fact_slot_key(&SlotPath::parse(path).expect("valid path"));

        assert_eq!(key("transport.rate").as_deref(), Some("transport.rate"));
        assert_eq!(
            key("transport.scrub_offset_seconds").as_deref(),
            Some("transport.scrub_offset_seconds")
        );
    }

    /// Everything that was keyed by its first segment before still is: a
    /// bare top-level field, and any path whose next step leaves the record
    /// world (a map key). An option's `some` reads as a field here and is
    /// resolved back to its row by the slot-side descent, which only walks
    /// records.
    #[test]
    fn ordinary_wiring_keys_exactly_as_it_always_did() {
        let key = |path: &str| binding_fact_slot_key(&SlotPath::parse(path).expect("valid path"));

        assert_eq!(key("brightness").as_deref(), Some("brightness"));
        assert_eq!(key("consumed[speed]").as_deref(), Some("consumed"));
        assert_eq!(key("consumed[speed].default").as_deref(), Some("consumed"));
    }
}
