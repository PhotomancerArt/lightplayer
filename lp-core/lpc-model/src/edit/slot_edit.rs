//! Structured slot edits within an authored `.toml` artifact.

use crate::{LpValue, SlotPath};

/// One slot-tree edit within an authored artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotEdit {
    /// Default-construct the slot, map entry, option body, or enum variant at `path`.
    EnsurePresent { path: SlotPath },
    /// Assign a value leaf at `path`.
    AssignValue { path: SlotPath, value: LpValue },
    /// Remove optional/map presence at `path`.
    Remove { path: SlotPath },
}

impl SlotEdit {
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::EnsurePresent { .. } => "ensure_present",
            Self::AssignValue { .. } => "assign_value",
            Self::Remove { .. } => "remove",
        }
    }

    pub fn path(&self) -> &SlotPath {
        match self {
            Self::EnsurePresent { path }
            | Self::AssignValue { path, .. }
            | Self::Remove { path } => path,
        }
    }
}
