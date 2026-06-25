//! Child nodes extracted from config slots.

use crate::{UiNodeDirtyState, UiNodeSection, UiStatus};

/// A child node rendered outside its parent node pane.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNodeChild {
    /// Child use label.
    pub label: String,
    /// Child node kind.
    pub kind: String,
    /// Slot, source, or invocation detail.
    pub detail: String,
    /// Runtime status for the child.
    pub status: UiStatus,
    /// Optional active-state or timing summary.
    pub summary: Option<String>,
    /// Whether this child is the active branch for its parent.
    pub active: bool,
    /// Compact body sections for expanded child display.
    pub sections: Vec<UiNodeSection>,
    /// Edited-state affordance for child invocation metadata.
    pub dirty: UiNodeDirtyState,
}

impl UiNodeChild {
    /// Create a child node summary.
    pub fn new(
        label: impl Into<String>,
        kind: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            kind: kind.into(),
            detail: detail.into(),
            status: UiStatus::neutral("Idle"),
            summary: None,
            active: false,
            sections: Vec::new(),
            dirty: UiNodeDirtyState::Clean,
        }
    }

    /// Mark the child as active.
    pub fn active(mut self, summary: impl Into<String>) -> Self {
        self.active = true;
        self.status = UiStatus::good("Active");
        self.summary = Some(summary.into());
        self
    }

    /// Add compact body sections.
    pub fn with_sections(mut self, sections: Vec<UiNodeSection>) -> Self {
        self.sections = sections;
        self
    }
}
