//! The editor's active tool.

/// Tools mirror the UX spike: select is home; creation tools return to
/// select after committing (the session enforces that on create/finish).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum MapTool {
    #[default]
    Select,
    Grid,
    Ring,
    /// Path drawing carries its in-progress vertices; Escape backs vertices
    /// out one at a time (never wholesale — parent decision D6).
    Path {
        draft: Vec<[f32; 2]>,
    },
}

impl MapTool {
    /// Start path drawing with an empty draft.
    #[must_use]
    pub const fn path() -> Self {
        Self::Path { draft: Vec::new() }
    }

    #[must_use]
    pub fn is_select(&self) -> bool {
        matches!(self, Self::Select)
    }
}
