//! Binding authoring model for slot rows (roadmap M4, bus-only authoring D1).
//!
//! A bindable row carries a [`UiBindingAuthoring`]: everything the popover
//! picker needs to bind, retarget, or unbind the slot as ordinary slot edits
//! on the node's `bindings` map — no dedicated wire surface. Direction is
//! derived, never asked (a consumed/config slot authors a `source`; a
//! produced slot authors a `target`), and the editor only creates `bus:…`
//! endpoints (node-to-node refs stay valid on disk and render read-only).

use lpc_model::SlotMapKey;

use crate::{ProjectSlotAddress, UiBindingEndpoint};

/// Which side of a binding this slot authors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiBindingAuthoringDirection {
    /// The slot consumes: binding entries carry `source`.
    Source,
    /// The slot publishes: binding entries carry `target`.
    Target,
}

impl UiBindingAuthoringDirection {
    /// The `BindingDef` field this direction authors.
    pub fn field(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
        }
    }
}

/// Authoring surface for one bindable slot row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiBindingAuthoring {
    /// Local slot name keying the node's `bindings` map (bindings live at
    /// node-def roots since M0).
    pub key: String,
    /// Derived authoring direction.
    pub direction: UiBindingAuthoringDirection,
    /// Address of the owning node's def-root `bindings` map.
    pub bindings_map: ProjectSlotAddress,
    /// The currently *authored* endpoint, when one exists (enables
    /// Retarget/Unbind; default-origin wiring does not count — binding over
    /// a default is a plain Bind, and removing the authored entry re-enables
    /// the default).
    pub authored: Option<UiBindingEndpoint>,
}

impl UiBindingAuthoring {
    /// Address of this slot's entry in the `bindings` map.
    pub fn entry_address(&self) -> ProjectSlotAddress {
        self.bindings_map
            .child_map_entry(SlotMapKey::String(self.key.clone()))
    }

    /// Address of the endpoint value inside the entry
    /// (`bindings[key].source.some` / `bindings[key].target.some`).
    pub fn endpoint_value_address(&self) -> Option<ProjectSlotAddress> {
        self.entry_address()
            .child_field(self.direction.field())?
            .child_field("some")
    }
}

/// One channel the binding picker offers: the union of channels observed in
/// the project (M2 binding graph) and the well-known registry. Arbitrary
/// names stay legal — the picker teaches the norm, it does not gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiChannelChoice {
    /// Channel name (without the `bus:` scheme).
    pub name: String,
    /// Semantic kind label, when known (registry or established by wiring).
    pub kind: Option<String>,
    /// Registry description, for well-known channels.
    pub doc: Option<&'static str>,
    /// True when the channel is in the well-known registry.
    pub well_known: bool,
    /// True when the channel is currently observed in the project.
    pub observed: bool,
}
