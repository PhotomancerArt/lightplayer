//! Canonical wire/sync codec for dynamic slot snapshots.
//!
//! This codec is intentionally separate from authored slot config JSON. Authored
//! JSON can omit defaults and is meant to construct typed slot objects. Sync JSON
//! is a lossless snapshot of [`crate::SlotData`] semantics, including container
//! and leaf revisions.

mod snapshot_reader;
mod snapshot_writer;

pub use snapshot_reader::{read_slot_snapshot_json, read_slot_snapshot_shape_json};
pub use snapshot_writer::{
    write_slot_snapshot_json, write_slot_snapshot_shape_value, write_slot_snapshot_value,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LpType, LpValue, Revision, SlotData, SlotFieldShape, SlotMapDyn, SlotMapKey,
        SlotMapKeyShape, SlotOptionDyn, SlotRecord, SlotShape, SlotShapeId, SlotShapeRegistry,
        WithRevision,
    };
    use alloc::collections::BTreeMap;
    use alloc::string::String;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn sync_snapshot_preserves_revisions_and_map_keys() {
        let shape_id = SlotShapeId::from_static_name("test.sync.Root");
        let registry = registry(shape_id, root_shape());
        let data = root_data();

        let json = write_slot_snapshot_json(&registry, shape_id, data.access(), Vec::new())
            .expect("write slot sync snapshot");
        let json = String::from_utf8(json).expect("utf8 snapshot");
        let back = read_slot_snapshot_json(&registry, shape_id, &json).expect("read snapshot");

        assert_eq!(back, data);
        assert!(json.contains("\"kind\":\"record\""));
        assert!(json.contains("\"keys_revision\":5"));
    }

    #[test]
    fn sync_snapshot_rejects_wrong_shape_kind() {
        let shape_id = SlotShapeId::from_static_name("test.sync.Value");
        let registry = registry(shape_id, SlotShape::value(LpType::F32));
        let json = r#"{"kind":"map","keys_revision":1,"entries":[]}"#;

        let error = read_slot_snapshot_json(&registry, shape_id, json).unwrap_err();

        assert!(error.to_string().contains("invalid discriminator"));
    }

    fn registry(id: SlotShapeId, shape: SlotShape) -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_shape(id, shape)
            .expect("register test shape");
        registry
    }

    fn root_shape() -> SlotShape {
        SlotShape::Record {
            meta: Default::default(),
            fields: vec![
                SlotFieldShape::new("enabled", SlotShape::value(LpType::Bool)).unwrap(),
                SlotFieldShape::new(
                    "params",
                    SlotShape::Map {
                        meta: Default::default(),
                        key: SlotMapKeyShape::String,
                        value: alloc::boxed::Box::new(SlotShape::value(LpType::U32)),
                    },
                )
                .unwrap(),
                SlotFieldShape::new(
                    "maybe",
                    SlotShape::Option {
                        meta: Default::default(),
                        some: alloc::boxed::Box::new(SlotShape::value(LpType::String)),
                    },
                )
                .unwrap(),
            ],
        }
    }

    fn root_data() -> SlotData {
        SlotData::Record(SlotRecord::with_revision(
            Revision::new(2),
            vec![
                SlotData::Value(WithRevision::new(Revision::new(3), LpValue::Bool(true))),
                SlotData::Map(SlotMapDyn::with_revision(
                    Revision::new(5),
                    BTreeMap::from([(
                        SlotMapKey::String(String::from("gain")),
                        SlotData::Value(WithRevision::new(Revision::new(8), LpValue::U32(42))),
                    )]),
                )),
                SlotData::Option(SlotOptionDyn::some_with_version(
                    Revision::new(13),
                    SlotData::Value(WithRevision::new(
                        Revision::new(21),
                        LpValue::String(String::from("ready")),
                    )),
                )),
            ],
        ))
    }
}
