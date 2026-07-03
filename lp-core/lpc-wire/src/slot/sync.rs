use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::{SlotPath, SlotShapeId, SlotShapeRegistrySnapshot};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WireSlotData(
    #[cfg_attr(feature = "schema-gen", schemars(with = "serde_json::Value"))]
    alloc::boxed::Box<RawValue>,
);

impl WireSlotData {
    pub fn from_json_string(json: String) -> Result<Self, serde_json::Error> {
        serde_json::value::RawValue::from_string(json).map(Self)
    }

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
    Replace(WireSlotData),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::SlotShapeRegistry;

    #[test]
    fn slot_patch_round_trips() {
        let patch = WireSlotPatch {
            root: String::from("engine.shader_node"),
            path: SlotPath::parse("params.exposure").unwrap(),
            change: WireSlotChange::Replace(
                WireSlotData::from_json_string(String::from(
                    r#"{"kind":"value","changed_at":7,"value":2.0}"#,
                ))
                .unwrap(),
            ),
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

    #[cfg(feature = "ser-write-json")]
    #[test]
    fn slot_data_serializes_as_json_value_with_ser_write_json() {
        use core::convert::Infallible;
        use ser_write_json::SerWrite;

        struct VecWriter(Vec<u8>);

        impl SerWrite for VecWriter {
            type Error = Infallible;

            fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
                self.0.extend_from_slice(buf);
                Ok(())
            }
        }

        let root = WireSlotRootSnapshot {
            name: String::from("node.0.def"),
            shape: SlotShapeId::new(7),
            data: WireSlotData::from_json_string(String::from(
                r#"{"kind":"value","changed_at":7,"value":2.0}"#,
            ))
            .unwrap(),
        };

        let mut writer = VecWriter(Vec::new());
        ser_write_json::ser::to_writer(&mut writer, &root).unwrap();
        let json = core::str::from_utf8(&writer.0).unwrap();

        assert!(json.contains(r#""data":{"kind":"value""#), "{json}");
        assert!(!json.contains("$serde_json::private::RawValue"), "{json}");
        let back: WireSlotRootSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(back, root);
    }
}
