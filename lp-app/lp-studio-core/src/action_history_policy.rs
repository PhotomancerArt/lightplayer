use serde::{Deserialize, Serialize};

/// Future undo/history treatment for an action.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ActionHistoryPolicy {
    /// The action is operational or read-only and should never enter undo history.
    Never,
    /// The action is local and temporary, such as selection or panel state.
    Ephemeral,
    /// Future shape for confirmed domain edits that can be inverted.
    UndoableEdit {
        scope: UndoScope,
        label: String,
        merge_key: Option<String>,
    },
    /// Separates undo groups for a scope without being undoable itself.
    Barrier { scope: UndoScope },
}

impl ActionHistoryPolicy {
    pub fn never(&self) -> bool {
        matches!(self, Self::Never)
    }

    pub fn ephemeral(&self) -> bool {
        matches!(self, Self::Ephemeral)
    }
}

/// Scope a future undo entry applies to.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum UndoScope {
    Studio,
    Project {
        project_id: String,
    },
    Artifact {
        project_id: String,
        artifact: String,
    },
}
