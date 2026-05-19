use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{SlotData, SlotPath, SlotShapeId, SlotShapeRegistrySnapshot};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WireSlotData(
    #[cfg_attr(feature = "schema-gen", schemars(with = "serde_json::Value"))]
    alloc::boxed::Box<RawValue>,
);

impl WireSlotData {
    pub fn get(&self) -> &str {
        self.0.get()
    }
}

impl core::fmt::Debug for WireSlotData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("WireSlotData").field(&self.get()).finish()
    }
}

impl PartialEq for WireSlotData {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

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
    pub data: WireSlotData,
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

pub fn wire_slot_data_from_slot_data(data: &SlotData) -> WireSlotData {
    WireSlotData(serde_json::value::to_raw_value(data).expect("SlotData serializes as JSON"))
}

pub fn wire_slot_data_to_slot_data(data: &WireSlotData) -> Result<SlotData, serde_json::Error> {
    serde_json::from_str(data.get())
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
