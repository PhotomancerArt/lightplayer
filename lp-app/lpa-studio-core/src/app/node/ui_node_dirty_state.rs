//! Edited-state affordances for node anatomy data.

/// Whether a UI datum matches the persisted project state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiNodeDirtyState {
    /// The value is in sync with the loaded project.
    Clean,
    /// The value has local edits that are not yet committed.
    Dirty,
    /// The value is being written or refreshed.
    Saving,
    /// The last write failed and the UI should preserve the edited value.
    Error,
}

impl UiNodeDirtyState {
    /// Returns true when the value needs a visible edited-state affordance.
    pub fn needs_attention(self) -> bool {
        matches!(self, Self::Dirty | Self::Saving | Self::Error)
    }
}
