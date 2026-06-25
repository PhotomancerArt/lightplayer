//! Recursive consumed-slot data.

use crate::{UiBindingEndpoint, UiNodeDirtyState};

/// Where a consumed slot gets its value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiSlotSource {
    /// The value is authored directly on the slot.
    Direct,
    /// The value comes from a binding endpoint.
    Bound(UiBindingEndpoint),
    /// The slot owns or references an extracted child node.
    Child(String),
    /// The slot has no value or binding yet.
    Unset,
}

/// A recursive row in the node configuration tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiConsumedSlot {
    /// Slot label shown in the tree.
    pub label: String,
    /// Optional formatted value for leaves and summaries.
    pub value: Option<String>,
    /// Optional type, shape, revision, or unit detail.
    pub detail: Option<String>,
    /// Source of the consumed value.
    pub source: UiSlotSource,
    /// Edited-state affordance for authored slot values and bindings.
    pub dirty: UiNodeDirtyState,
    /// Nested fields, map entries, enum payloads, or option payloads.
    pub children: Vec<UiConsumedSlot>,
    /// Issues discovered while projecting this slot.
    pub issues: Vec<String>,
}

impl UiConsumedSlot {
    /// Create a direct-value consumed slot.
    pub fn direct(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: Some(value.into()),
            detail: None,
            source: UiSlotSource::Direct,
            dirty: UiNodeDirtyState::Clean,
            children: Vec::new(),
            issues: Vec::new(),
        }
    }

    /// Create a bound consumed slot.
    pub fn bound(label: impl Into<String>, endpoint: UiBindingEndpoint) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            source: UiSlotSource::Bound(endpoint),
            dirty: UiNodeDirtyState::Clean,
            children: Vec::new(),
            issues: Vec::new(),
        }
    }

    /// Create a grouping slot with nested rows.
    pub fn group(label: impl Into<String>, children: Vec<UiConsumedSlot>) -> Self {
        Self {
            label: label.into(),
            value: None,
            detail: None,
            source: UiSlotSource::Direct,
            dirty: UiNodeDirtyState::Clean,
            children,
            issues: Vec::new(),
        }
    }

    /// Add secondary detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Mark the row dirty, saving, or failed.
    pub fn with_dirty(mut self, dirty: UiNodeDirtyState) -> Self {
        self.dirty = dirty;
        self
    }
}
