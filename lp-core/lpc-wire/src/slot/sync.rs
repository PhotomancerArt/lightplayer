use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{SlotData, SlotPath, SlotShapeId, SlotShapeRegistrySnapshot};
use serde::{Deserialize, Serialize};

/// Complete slot sync payload for a client-side slot mirror.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireSlotFullSync {
    pub registry: SlotShapeRegistrySnapshot,
    pub roots: Vec<WireSlotRootSnapshot>,
}

/// Slot root snapshots without a registry payload.
///
/// Used when a response already carries shape registry data through another
/// domain, such as `ProjectReadResult::Shapes`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireSlotRootsSnapshot {
    pub roots: Vec<WireSlotRootSnapshot>,
}

/// One root included in a full slot sync.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireSlotRootSnapshot {
    pub name: String,
    pub shape: SlotShapeId,
    pub data: SlotData,
}

/// Incremental slot data patch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WireSlotPatch {
    pub root: String,
    pub path: SlotPath,
    pub change: WireSlotChange,
}

/// Slot data change payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireSlotChange {
    Replace(SlotData),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{LpValue, Revision, SlotShapeRegistry, WithRevision};

    #[test]
    fn slot_patch_round_trips() {
        let patch = WireSlotPatch {
            root: String::from("engine.shader_node"),
            path: SlotPath::parse("params.exposure").unwrap(),
            change: WireSlotChange::Replace(SlotData::Value(WithRevision::new(
                Revision::new(7),
                LpValue::F32(2.0),
            ))),
        };

        let json = serde_json::to_string(&patch).unwrap();
        let back: WireSlotPatch = serde_json::from_str(&json).unwrap();

        assert_eq!(back, patch);
    }

    #[test]
    fn full_sync_round_trips() {
        let sync = WireSlotFullSync {
            registry: SlotShapeRegistry::default().snapshot(),
            roots: vec![],
        };

        let json = serde_json::to_string(&sync).unwrap();
        let back: WireSlotFullSync = serde_json::from_str(&json).unwrap();

        assert_eq!(back, sync);
    }
}
