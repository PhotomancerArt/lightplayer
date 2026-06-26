//! Asset editor data embedded in config slot rows.

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
}

impl UiSlotAsset {
    /// Create asset slot data.
    pub fn new(source: impl Into<String>, editor: UiAssetEditorKind) -> Self {
        Self {
            source: source.into(),
            editor,
            detail: None,
            content: None,
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

    /// Compact editor label for slot detail popups.
    pub fn editor_label(&self) -> &'static str {
        match self.editor {
            UiAssetEditorKind::Text => "Text asset",
            UiAssetEditorKind::Glsl => "GLSL asset",
            UiAssetEditorKind::Svg => "SVG asset",
            UiAssetEditorKind::Binary => "Binary asset",
        }
    }
}
