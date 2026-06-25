//! Header metadata for a node pane.

use crate::UiStatus;

/// Identity and runtime summary shown at the top of a node pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNodeHeader {
    /// Display name, usually the node use name.
    pub title: String,
    /// Node kind or definition family.
    pub kind: String,
    /// Stable path shown for orientation and debugging.
    pub path: String,
    /// Optional file or asset source associated with the node.
    pub source: Option<String>,
    /// Compact runtime status for the node.
    pub status: UiStatus,
    /// Optional performance or runtime summary.
    pub summary: Option<String>,
    /// Optional expanded status detail or error text.
    pub detail: Option<String>,
}

impl UiNodeHeader {
    /// Create a header with neutral status.
    pub fn new(title: impl Into<String>, kind: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            kind: kind.into(),
            path: path.into(),
            source: None,
            status: UiStatus::neutral("Idle"),
            summary: None,
            detail: None,
        }
    }

    /// Set the compact status.
    pub fn with_status(mut self, status: UiStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the file or asset source label.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set the runtime summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set the expanded status detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}
