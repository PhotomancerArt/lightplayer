use crate::{FrameId, ModelValue, SlotName, Versioned, current_state_version};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Owned dynamic data for a slot-accessible value tree.
///
/// `Value` leaves carry their own version. Containers provide structure around
/// those leaves and are interpreted against registered slot shapes. This type is
/// the generic snapshot/wire mirror; Rust-authored source and runtime structs
/// can expose the same model through access traits without first converting
/// themselves into `SlotData`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMapDyn),
    Enum(SlotEnum),
    Option(SlotOptionDyn),
}

/// Indexed record fields.
///
/// Field names and field order live in the corresponding [`crate::SlotShape`].
/// Keeping record data indexed avoids duplicating names in every live snapshot.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotRecord {
    pub fields_changed_frame: FrameId,
    pub fields: Vec<SlotData>,
}

impl SlotRecord {
    pub fn new(fields: Vec<SlotData>) -> Self {
        Self::with_version(current_state_version(), fields)
    }

    pub fn with_version(fields_changed_frame: FrameId, fields: Vec<SlotData>) -> Self {
        Self {
            fields_changed_frame,
            fields,
        }
    }
}

/// Owned dynamic stable key/value container.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMapDyn {
    pub keys_changed_frame: FrameId,
    pub entries: BTreeMap<SlotMapKey, SlotData>,
}

impl SlotMapDyn {
    pub fn new(entries: BTreeMap<SlotMapKey, SlotData>) -> Self {
        Self::with_version(current_state_version(), entries)
    }

    pub fn with_version(
        keys_changed_frame: FrameId,
        entries: BTreeMap<SlotMapKey, SlotData>,
    ) -> Self {
        Self {
            keys_changed_frame,
            entries,
        }
    }
}

/// Key for a dynamic or typed slot map.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotMapKey {
    String(String),
    I32(i32),
    U32(u32),
}

/// Active value of an enum slot.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotEnum {
    pub variant_changed_frame: FrameId,
    pub variant: SlotName,
    pub data: Box<SlotData>,
}

impl SlotEnum {
    pub fn new(variant: SlotName, data: SlotData) -> Self {
        Self::with_version(current_state_version(), variant, data)
    }

    pub fn with_version(variant_changed_frame: FrameId, variant: SlotName, data: SlotData) -> Self {
        Self {
            variant_changed_frame,
            variant,
            data: Box::new(data),
        }
    }
}

/// Owned dynamic optional slot data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotOptionDyn {
    pub presence_changed_frame: FrameId,
    pub data: Option<Box<SlotData>>,
}

impl SlotOptionDyn {
    pub fn none() -> Self {
        Self::none_with_version(current_state_version())
    }

    pub fn some(data: SlotData) -> Self {
        Self::some_with_version(current_state_version(), data)
    }

    pub fn none_with_version(presence_changed_frame: FrameId) -> Self {
        Self {
            presence_changed_frame,
            data: None,
        }
    }

    pub fn some_with_version(presence_changed_frame: FrameId, data: SlotData) -> Self {
        Self {
            presence_changed_frame,
            data: Some(Box::new(data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameId;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn slot_data_serializes_versioned_value_leaf() {
        let data = SlotData::Value(Versioned::new(FrameId::new(3), ModelValue::Bool(true)));

        let json = serde_json::to_string(&data).unwrap();
        let back: SlotData = serde_json::from_str(&json).unwrap();

        assert_eq!(back, data);
    }

    #[test]
    fn slot_map_key_orders_stable_key_domains() {
        let mut entries = BTreeMap::new();
        entries.insert(
            SlotMapKey::U32(2),
            SlotData::Record(SlotRecord::new(vec![])),
        );
        entries.insert(
            SlotMapKey::String("a".to_string()),
            SlotData::Record(SlotRecord::new(vec![])),
        );
        entries.insert(
            SlotMapKey::I32(-1),
            SlotData::Record(SlotRecord::new(vec![])),
        );

        assert_eq!(entries.len(), 3);
    }
}
