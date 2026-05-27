//! Structured slot mutations within a `.toml` artifact.

use alloc::string::String;

use lpc_model::{LpValue, SlotPath};

use super::PendingSlotTarget;

/// One slot-tree edit within a [`super::ArtifactEdit::Slot`] block.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotEdit {
    /// Select an enum variant at `path`.
    UseEnumVariant { path: SlotPath, variant: String },
    /// Assign a value leaf at `path`.
    AssignValue { path: SlotPath, value: LpValue },
    /// Insert or replace one map entry (`key` is a wire string parsed on apply).
    MapInsert {
        path: SlotPath,
        key: String,
        value: LpValue,
    },
    /// Remove one map entry.
    MapRemove { path: SlotPath, key: String },
    /// Include or omit an option slot (`present = true` inserts the shape default on apply).
    UseOption { path: SlotPath, present: bool },
}

impl SlotEdit {
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::UseEnumVariant { .. } => "use_enum_variant",
            Self::AssignValue { .. } => "assign_value",
            Self::MapInsert { .. } => "map_insert",
            Self::MapRemove { .. } => "map_remove",
            Self::UseOption { .. } => "use_option",
        }
    }

    pub fn path(&self) -> &SlotPath {
        match self {
            Self::UseEnumVariant { path, .. }
            | Self::AssignValue { path, .. }
            | Self::MapInsert { path, .. }
            | Self::MapRemove { path, .. }
            | Self::UseOption { path, .. } => path,
        }
    }

    pub fn pending_target(&self) -> PendingSlotTarget {
        match self {
            Self::MapInsert { path, key, .. } => PendingSlotTarget::MapInsert {
                path: path.clone(),
                key: key.clone(),
            },
            Self::MapRemove { path, key, .. } => PendingSlotTarget::MapRemove {
                path: path.clone(),
                key: key.clone(),
            },
            Self::UseEnumVariant { path, .. }
            | Self::AssignValue { path, .. }
            | Self::UseOption { path, .. } => PendingSlotTarget::Slot(path.clone()),
        }
    }
}
