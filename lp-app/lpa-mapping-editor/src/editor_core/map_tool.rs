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
    /// Polygon drawing: one tool, two population modes (vision D11). The
    /// draft and the Escape grammar are the path tool's; `mode` decides only
    /// which shape the closed outline becomes on finish.
    Polygon {
        draft: Vec<[f32; 2]>,
        mode: PolygonMode,
    },
}

/// How a closed outline is populated with lamps: along its perimeter, or
/// over a lattice filling it.
///
/// This is a property of the OBJECT, not of the gesture — the same outline
/// switches populations after the draw via
/// [`MapEditorSession::set_polygon_population`](crate::editor_core::editor_session::MapEditorSession::set_polygon_population),
/// which is why the tool carries a mode rather than there being two tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PolygonMode {
    /// Lamps distributed around the perimeter ([`lpc_mapping::PolygonShape`]).
    #[default]
    Outline,
    /// Lamps on a lattice filling the outline
    /// ([`lpc_mapping::FilledPolygonShape`]).
    Filled,
}

impl MapTool {
    /// Start path drawing with an empty draft.
    #[must_use]
    pub const fn path() -> Self {
        Self::Path { draft: Vec::new() }
    }

    /// Start polygon drawing in `mode` with an empty draft.
    ///
    /// The tool stores no mode between activations: the caller passes the
    /// mode it wants, so "re-entering the tool keeps the last mode" is the
    /// view's memory to hold, not a second source of truth here.
    #[must_use]
    pub const fn polygon(mode: PolygonMode) -> Self {
        Self::Polygon {
            draft: Vec::new(),
            mode,
        }
    }

    #[must_use]
    pub fn is_select(&self) -> bool {
        matches!(self, Self::Select)
    }
}
