//! Config assets extracted from the slot tree.

use crate::UiNodeDirtyState;

/// Preferred editor treatment for an extracted asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAssetEditorKind {
    /// Plain text or unknown source.
    Text,
    /// GLSL shader source.
    Glsl,
    /// SVG document or fixture map.
    Svg,
    /// Binary or opaque asset.
    Binary,
}

/// A config asset promoted to top-level treatment in the node pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiConfigAsset {
    /// Slot or asset label.
    pub label: String,
    /// Asset path, inline label, or source reference.
    pub source: String,
    /// Content/editor family.
    pub editor: UiAssetEditorKind,
    /// Optional language, size, revision, or load-state detail.
    pub detail: Option<String>,
    /// Optional preview or snippet.
    pub summary: Option<String>,
    /// Edited-state affordance for asset edits.
    pub dirty: UiNodeDirtyState,
}

impl UiConfigAsset {
    /// Create an extracted asset.
    pub fn new(
        label: impl Into<String>,
        source: impl Into<String>,
        editor: UiAssetEditorKind,
    ) -> Self {
        Self {
            label: label.into(),
            source: source.into(),
            editor,
            detail: None,
            summary: None,
            dirty: UiNodeDirtyState::Clean,
        }
    }

    /// Add secondary detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add a short content summary or preview snippet.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }
}
