//! Asset editor data embedded in config slot rows.

use crate::UiAssetEditor;

/// Preferred editor treatment for an asset slot.
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

/// Editor-ready asset content carried by a config slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotAsset {
    /// Asset path, inline label, or source reference.
    pub source: String,
    /// Content/editor family.
    pub editor: UiAssetEditorKind,
    /// Optional language, size, revision, or load-state detail.
    pub detail: Option<String>,
    /// Optional preview or full inline content.
    pub content: Option<String>,
    /// Inline editor data when this asset resolves to an editable artifact
    /// (`None` for inline/read-only assets or kinds that do not support
    /// editing). Populated controller-side during the node walk.
    pub inline_editor: Option<UiAssetEditor>,
}

impl UiSlotAsset {
    /// Create asset slot data.
    pub fn new(source: impl Into<String>, editor: UiAssetEditorKind) -> Self {
        Self {
            source: source.into(),
            editor,
            detail: None,
            content: None,
            inline_editor: None,
        }
    }

    /// Add secondary detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add preview or inline editor content.
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Attach resolved inline editor data.
    pub fn with_inline_editor(mut self, editor: UiAssetEditor) -> Self {
        self.inline_editor = Some(editor);
        self
    }

    /// Compact editor label for slot detail popups.
    pub fn editor_label(&self) -> &'static str {
        self.editor.editor_label()
    }
}

impl UiAssetEditorKind {
    /// Whether Studio offers an inline editor for assets of this kind. This
    /// is the data-driven gate: GLSL only today; `Svg` joins in M3 by
    /// extending this match (nothing else keys on the kind).
    pub fn supports_editor(self) -> bool {
        matches!(self, Self::Glsl)
    }

    /// Compact editor label shared by slot detail popups and the editor.
    pub fn editor_label(self) -> &'static str {
        match self {
            Self::Text => "Text asset",
            Self::Glsl => "GLSL asset",
            Self::Svg => "SVG asset",
            Self::Binary => "Binary asset",
        }
    }
}
