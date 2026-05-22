//! Per-artifact edit operations.

use alloc::string::String;

use lpc_model::{LpValue, SlotPath};

/// One edit operation within an artifact block.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactOp {
    /// Remove this path on commit.
    Delete,
    /// Whole-file body — assets and optional TOML import escape hatch.
    SetBytes(String),
    /// Set a slot leaf value.
    SetSlot { path: SlotPath, value: LpValue },
    /// Insert or replace one map entry (key is wire string; parsed on apply in M4).
    MapInsert {
        path: SlotPath,
        key: String,
        value: LpValue,
    },
    /// Remove one map entry.
    MapRemove { path: SlotPath, key: String },
    /// Set option presence (`present = true` uses shape default on apply in M4).
    OptionSet { path: SlotPath, present: bool },
}

impl ArtifactOp {
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::SetBytes(_) => "set_bytes",
            Self::SetSlot { .. } => "set_slot",
            Self::MapInsert { .. } => "map_insert",
            Self::MapRemove { .. } => "map_remove",
            Self::OptionSet { .. } => "option_set",
        }
    }
}
