//! Structured slot edits within an authored `.toml` artifact.

use crate::{LpValue, SlotPath};

/// One slot-tree edit within an authored artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SlotEdit {
    pub path: SlotPath,
    pub op: SlotEditOp,
}

/// Path-free slot operation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotEditOp {
    /// Default-construct the slot, map entry, option body, or enum variant at `path`.
    EnsurePresent,
    /// Assign a value leaf at `path`.
    AssignValue(LpValue),
    /// Remove optional/map presence at `path`.
    Remove,
}

impl SlotEdit {
    pub fn ensure_present(path: SlotPath) -> Self {
        Self {
            path,
            op: SlotEditOp::EnsurePresent,
        }
    }

    pub fn assign_value(path: SlotPath, value: LpValue) -> Self {
        Self {
            path,
            op: SlotEditOp::AssignValue(value),
        }
    }

    pub fn remove(path: SlotPath) -> Self {
        Self {
            path,
            op: SlotEditOp::Remove,
        }
    }

    pub fn op_name(&self) -> &'static str {
        self.op.op_name()
    }

    pub fn path(&self) -> &SlotPath {
        &self.path
    }
}

impl SlotEditOp {
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::EnsurePresent => "ensure_present",
            Self::AssignValue(_) => "assign_value",
            Self::Remove => "remove",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_split_path_from_op() {
        let path = SlotPath::parse("controls.rate").unwrap();
        let edit = SlotEdit::assign_value(path.clone(), LpValue::F32(2.0));

        assert_eq!(edit.path(), &path);
        assert_eq!(edit.op_name(), "assign_value");
        assert_eq!(edit.op, SlotEditOp::AssignValue(LpValue::F32(2.0)));
    }
}
