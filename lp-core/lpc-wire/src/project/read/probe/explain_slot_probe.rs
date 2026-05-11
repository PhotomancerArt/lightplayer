//! Slot explanation probe.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{LpValue, NodeId, Revision, SlotPath, WithRevision};

/// Request to explain how a node slot resolves.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ExplainSlotProbeRequest {
    pub node: NodeId,
    pub slot: SlotPath,
    #[serde(default)]
    pub include_trace: bool,
}

/// Human-readable slot resolution explanation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotExplanation {
    pub value: Option<WithRevision<LpValue>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<String>,
}

/// Result of an explain-slot probe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExplainSlotProbeResult {
    Explained {
        node: NodeId,
        slot: SlotPath,
        revision: Revision,
        explanation: SlotExplanation,
    },
    Unsupported {
        reason: String,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn explain_slot_probe_round_trips() {
        let result = ExplainSlotProbeResult::Explained {
            node: NodeId::new(5),
            slot: SlotPath::parse("input").unwrap(),
            revision: Revision::new(9),
            explanation: SlotExplanation {
                value: Some(WithRevision::new(Revision::new(9), LpValue::Bool(true))),
                trace: Vec::from([String::from("default def slot")]),
            },
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: ExplainSlotProbeResult = serde_json::from_str(&json).unwrap();

        assert_eq!(back, result);
    }
}
