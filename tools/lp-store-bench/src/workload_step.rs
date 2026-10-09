//! A workload step: a list of operations, then `commit`.

use std::sync::Arc;

/// One store operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Put { path: String, bytes: Arc<Vec<u8>> },
    DeletePrefix(String),
}

/// Operations ending in a commit: the unit the oracle calls "old or new".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub ops: Vec<Op>,
}

impl Step {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ops: Vec::new(),
        }
    }

    pub fn put(&mut self, path: impl Into<String>, bytes: Arc<Vec<u8>>) {
        self.ops.push(Op::Put {
            path: path.into(),
            bytes,
        });
    }

    pub fn delete_prefix(&mut self, prefix: impl Into<String>) {
        self.ops.push(Op::DeletePrefix(prefix.into()));
    }

    /// Bytes the step asks the store to hold (the denominator of write
    /// amplification).
    pub fn logical_bytes(&self) -> u64 {
        self.ops
            .iter()
            .map(|op| match op {
                Op::Put { bytes, .. } => bytes.len() as u64,
                Op::DeletePrefix(_) => 0,
            })
            .sum()
    }
}
