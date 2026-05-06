use alloc::string::String;
use lpc_model::{FrameId, ModelValue, SlotPath};
use serde::{Deserialize, Serialize};

/// Client-visible id for one requested slot mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct WireSlotMutationId(pub u64);

impl WireSlotMutationId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn id(self) -> u64 {
        self.0
    }
}

/// Client request to mutate one server-owned slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireSlotMutationRequest {
    pub id: WireSlotMutationId,
    pub root: String,
    pub path: SlotPath,
    pub expected_shape_version: FrameId,
    pub expected_data_version: FrameId,
    pub op: WireSlotMutationOp,
}

/// Mutation operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireSlotMutationOp {
    SetValue(ModelValue),
}

/// Server response for one slot mutation request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireSlotMutationResponse {
    pub id: WireSlotMutationId,
    pub result: WireSlotMutationResult,
}

/// Accepted or rejected mutation result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireSlotMutationResult {
    Accepted,
    Rejected(WireSlotMutationRejection),
}

/// Why a slot mutation was rejected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum WireSlotMutationRejection {
    ShapeConflict { current_version: FrameId },
    DataConflict { current_version: FrameId },
    WrongType,
    UnknownRoot,
    UnknownPath,
    UnsupportedTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_request_round_trips() {
        let request = WireSlotMutationRequest {
            id: WireSlotMutationId::new(42),
            root: String::from("engine.shader_node"),
            path: SlotPath::parse("params.exposure").unwrap(),
            expected_shape_version: FrameId::new(1),
            expected_data_version: FrameId::new(3),
            op: WireSlotMutationOp::SetValue(ModelValue::F32(2.0)),
        };

        let json = serde_json::to_string(&request).unwrap();
        let back: WireSlotMutationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(back, request);
    }

    #[test]
    fn mutation_response_round_trips() {
        let response = WireSlotMutationResponse {
            id: WireSlotMutationId::new(7),
            result: WireSlotMutationResult::Rejected(WireSlotMutationRejection::DataConflict {
                current_version: FrameId::new(5),
            }),
        };

        let json = serde_json::to_string(&response).unwrap();
        let back: WireSlotMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(back, response);
    }
}
