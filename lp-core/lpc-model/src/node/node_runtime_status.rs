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
    /// The node's kind has no runtime compiled into this build; a
    /// placeholder is attached in its place. The message names the kind.
    ///
    /// Distinct from [`Self::Error`] on purpose: nothing is broken and
    /// nothing the author did is wrong — this build simply does not carry
    /// that node kind (see `LpFeature::for_node_kind`).
    Unsupported(String),
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

        let status = NodeRuntimeStatus::Unsupported("node kind Button is not included".into());
        assert_ne!(
            status,
            NodeRuntimeStatus::Error("node kind Button is not included".into()),
            "unsupported is not an error: same message, different status"
        );
    }

    /// The variant survives the wire as its own tag — a client must be able
    /// to tell "not in this build" from "broken" without string sniffing.
    #[test]
    fn unsupported_round_trips_as_its_own_variant() {
        let status = NodeRuntimeStatus::Unsupported("node kind Fluid".into());
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Unsupported"), "{json}");
        let back: NodeRuntimeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }
}
