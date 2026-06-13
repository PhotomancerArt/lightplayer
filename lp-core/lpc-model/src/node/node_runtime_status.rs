//! Shared runtime node lifecycle and health status.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Node lifecycle and health status reported by the runtime tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum NodeRuntimeStatus {
    /// Created but not yet initialized.
    Created,
    /// Error initializing the node.
    InitError(String),
    /// Running normally.
    Ok,
    /// Running with a warning.
    Warn(String),
    /// Cannot run.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_runtime_status_variants() {
        let status = NodeRuntimeStatus::Created;
        assert_eq!(status, NodeRuntimeStatus::Created);

        let status = NodeRuntimeStatus::InitError("test error".into());
        match status {
            NodeRuntimeStatus::InitError(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected InitError"),
        }
    }
}
