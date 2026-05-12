use crate::{LpValue, Revision, SlotName, WithRevision, current_revision};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    Unit { revision: Revision },
    Value(WithRevision<LpValue>),
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
    pub fields_revision: Revision,
    pub fields: Vec<SlotData>,
}

impl SlotRecord {
    pub fn new(fields: Vec<SlotData>) -> Self {
        Self::with_revision(current_revision(), fields)
    }

    pub fn with_revision(fields_revision: Revision, fields: Vec<SlotData>) -> Self {
        Self {
            fields_revision,
            fields,
        }
    }
}

/// Owned dynamic stable key/value container.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMapDyn {
    pub keys_revision: Revision,
    #[serde(with = "slot_map_entries")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "Vec<SlotMapEntry>"))]
    pub entries: BTreeMap<SlotMapKey, SlotData>,
}

impl SlotMapDyn {
    pub fn new(entries: BTreeMap<SlotMapKey, SlotData>) -> Self {
        Self::with_revision(current_revision(), entries)
    }

    pub fn with_revision(keys_revision: Revision, entries: BTreeMap<SlotMapKey, SlotData>) -> Self {
        Self {
            keys_revision,
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

/// Serialized key/data entry for a dynamic slot map.
///
/// `SlotMapDyn` stores entries in a `BTreeMap` for lookup, but its wire shape is
/// an array of entries so map keys can be typed values instead of JSON object
/// field names.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMapEntry {
    pub key: SlotMapKey,
    pub data: SlotData,
}

mod slot_map_entries {
    use super::*;

    #[derive(Serialize)]
    struct EntryRef<'a> {
        key: &'a SlotMapKey,
        data: &'a SlotData,
    }

    pub fn serialize<S>(
        entries: &BTreeMap<SlotMapKey, SlotData>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(entries.len()))?;
        for (key, data) in entries {
            seq.serialize_element(&EntryRef { key, data })?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<SlotMapKey, SlotData>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries = Vec::<SlotMapEntry>::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|entry| (entry.key, entry.data))
            .collect())
    }
}

/// Active value of an enum slot.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotEnum {
    pub variant_revision: Revision,
    pub variant: SlotName,
    pub data: Box<SlotData>,
}

impl SlotEnum {
    pub fn new(variant: SlotName, data: SlotData) -> Self {
        Self::with_version(current_revision(), variant, data)
    }

    pub fn with_version(variant_revision: Revision, variant: SlotName, data: SlotData) -> Self {
        Self {
            variant_revision,
            variant,
            data: Box::new(data),
        }
    }
}

/// Owned dynamic optional slot data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotOptionDyn {
    pub presence_revision: Revision,
    pub data: Option<Box<SlotData>>,
}

impl SlotOptionDyn {
    pub fn none() -> Self {
        Self::none_with_version(current_revision())
    }

    pub fn some(data: SlotData) -> Self {
        Self::some_with_version(current_revision(), data)
    }

    pub fn none_with_version(presence_revision: Revision) -> Self {
        Self {
            presence_revision,
            data: None,
        }
    }

    pub fn some_with_version(presence_revision: Revision, data: SlotData) -> Self {
        Self {
            presence_revision,
            data: Some(Box::new(data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Revision;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn slot_data_serializes_versioned_value_leaf() {
        let data = SlotData::Value(WithRevision::new(Revision::new(3), LpValue::Bool(true)));

        let json = serde_json::to_string(&data).unwrap();
        let back: SlotData = serde_json::from_str(&json).unwrap();

        assert_eq!(back, data);
    }

    #[test]
    fn slot_data_serializes_unit_leaf() {
        let data = SlotData::Unit {
            revision: Revision::new(5),
        };

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

    #[test]
    fn slot_map_dyn_serializes_entries_as_key_data_array() {
        let data = SlotData::Map(SlotMapDyn::with_revision(
            Revision::new(7),
            BTreeMap::from([(
                SlotMapKey::String("param.one".to_string()),
                SlotData::Value(WithRevision::new(Revision::new(8), LpValue::U32(42))),
            )]),
        ));

        let json = serde_json::to_string(&data).unwrap();
        let back: SlotData = serde_json::from_str(&json).unwrap();

        assert!(json.contains("\"entries\":["));
        assert!(json.contains("\"param.one\""));
        assert_eq!(back, data);
    }
}
