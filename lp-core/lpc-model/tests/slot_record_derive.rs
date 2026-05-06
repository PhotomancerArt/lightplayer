#![cfg(feature = "derive")]

use lpc_model::{
    ModelType, SlotAccess, SlotDataAccess, SlotMapValueAccess, SlotRecordAccess, SlotRecordShape,
    SlotShape, SlotShapeRegistry, SlotValue, StaticSlotAccess,
};

#[derive(lpc_model::SlotRecord)]
#[slot(shape_id = "test.derived_record")]
struct DerivedRecord {
    #[slot(value = ModelType::Bool)]
    enabled: SlotValue<bool>,
    #[slot(record)]
    nested: NestedRecord,
}

#[derive(lpc_model::SlotRecord)]
struct NestedRecord {
    #[slot(value = ModelType::U32)]
    count: SlotValue<u32>,
}

#[test]
fn derive_generates_record_shape_access_and_root_registration() {
    let record = DerivedRecord {
        enabled: SlotValue::new(true),
        nested: NestedRecord {
            count: SlotValue::new(3),
        },
    };

    assert_eq!(record.shape_id(), DerivedRecord::SHAPE_ID);
    assert!(matches!(record.field(0), Some(SlotDataAccess::Value(_))));
    assert!(matches!(record.field(1), Some(SlotDataAccess::Record(_))));
    assert!(record.field(2).is_none());
    assert!(matches!(record.slot_data(), SlotDataAccess::Record(_)));

    let SlotShape::Record { fields, .. } = DerivedRecord::slot_record_shape() else {
        panic!("record shape");
    };
    assert_eq!(fields[0].name.as_str(), "enabled");
    assert_eq!(fields[1].name.as_str(), "nested");

    let mut registry = SlotShapeRegistry::default();
    DerivedRecord::register_shape(&mut registry).unwrap();
    assert!(registry.get(&DerivedRecord::SHAPE_ID).is_some());
}
