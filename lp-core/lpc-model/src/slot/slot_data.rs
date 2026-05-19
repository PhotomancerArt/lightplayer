use crate::{LpValue, Revision, SlotName, WithRevision, current_revision};
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
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMapDyn {
    pub keys_revision: Revision,
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum SlotMapKey {
    String(String),
    I32(i32),
    U32(u32),
}

/// Active value of an enum slot.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

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
