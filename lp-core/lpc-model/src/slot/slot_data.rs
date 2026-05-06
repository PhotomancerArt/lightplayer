use crate::{ModelValue, SlotName, Versioned};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Runtime data for a slot tree.
///
/// `Value` leaves carry their own version. Containers provide structure around
/// those leaves and are interpreted against a registered [`crate::SlotShape`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}

/// Indexed record fields.
///
/// Field names and field order live in the corresponding [`crate::SlotShape`].
/// Keeping record data indexed avoids duplicating names in every live snapshot.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotRecord {
    pub fields: Vec<SlotData>,
}

impl SlotRecord {
    pub fn new(fields: Vec<SlotData>) -> Self {
        Self { fields }
    }
}

/// Stable key/value container for dynamic keyed data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMap {
    pub entries: BTreeMap<SlotMapKey, SlotData>,
}

impl SlotMap {
    pub fn new(entries: BTreeMap<SlotMapKey, SlotData>) -> Self {
        Self { entries }
    }
}

/// Key for a [`SlotMap`].
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
    pub variant: SlotName,
    pub data: Box<SlotData>,
}

impl SlotEnum {
    pub fn new(variant: SlotName, data: SlotData) -> Self {
        Self {
            variant,
            data: Box::new(data),
        }
    }
}

/// Optional slot data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotOption {
    None,
    Some(Box<SlotData>),
}

impl SlotOption {
    pub fn some(data: SlotData) -> Self {
        Self::Some(Box::new(data))
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
