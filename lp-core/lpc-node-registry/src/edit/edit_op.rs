//! Atomic edit operations within an artifact block.

use alloc::string::String;

use lpc_model::{LpValue, SlotPath};

/// One edit operation within an [`super::ArtifactEdit`] block.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditOp {
    /// Remove this path on commit.
    Delete,
    /// Whole-file body — assets and optional TOML import escape hatch.
    SetBytes(String),
    /// Set a slot value at `path`.
    ///
    /// Apply interprets the value from slot shape: `String` may switch an enum
    /// variant or node `kind` (at root); other values set scalar leaves. Project
    /// `nodes[*].def` path strings update invocation locators.
    SetSlot { path: SlotPath, value: LpValue },
    /// Insert or replace one map entry (`key` is a wire string parsed on apply).
    MapInsert {
        path: SlotPath,
        key: String,
        value: LpValue,
    },
    /// Remove one map entry.
    MapRemove { path: SlotPath, key: String },
    /// Set option presence (`present = true` inserts the shape default on apply).
    OptionSet { path: SlotPath, present: bool },
}

impl EditOp {
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
