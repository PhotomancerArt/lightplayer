#![cfg(feature = "derive")]

use lpc_model::{
    LpValue, SlotAccess, SlotDataAccess, SlotDataMutAccess, SlotMapValueAccess, SlotMutAccess,
    SlotRecordAccess, SlotRecordMutAccess, SlotRecordShape, SlotShape, SlotShapeRegistry,
    StaticSlotAccess, StaticSlotShape, ValueSlot,
};

#[derive(lpc_model::SlotRecord)]
struct DerivedRecord {
    pub enabled: ValueSlot<bool>,
    pub nested: NestedRecord,
}

#[derive(lpc_model::SlotRecord)]
struct NestedRecord {
    pub count: ValueSlot<u32>,
}

#[test]
fn derive_generates_record_shape_access_and_root_registration() {
    let record = DerivedRecord {
        enabled: ValueSlot::new(true),
        nested: NestedRecord {
            count: ValueSlot::new(3),
        },
    };

    assert_eq!(record.shape_id(), DerivedRecord::SHAPE_ID);
    assert_static_slot_access::<DerivedRecord>();
    assert_eq!(
        record.shape_id(),
        <DerivedRecord as StaticSlotShape>::SHAPE_ID
    );
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
    assert!(DerivedRecord::ensure_registered(&mut registry).unwrap());
    assert!(!DerivedRecord::ensure_registered(&mut registry).unwrap());
    DerivedRecord::register_shape(&mut registry).unwrap();
    assert!(registry.get(&DerivedRecord::SHAPE_ID).is_some());
}

#[test]
fn derive_generates_mutable_record_access() {
    let mut record = DerivedRecord {
        enabled: ValueSlot::new(true),
        nested: NestedRecord {
            count: ValueSlot::new(3),
        },
    };

    let Some(SlotDataMutAccess::Value(enabled)) = record.field_mut(0) else {
        panic!("enabled value field");
    };
    enabled
        .set_lp_value(lpc_model::Revision::new(2), LpValue::Bool(false))
        .unwrap();
    assert_eq!(record.enabled.value(), &false);

    let Some(SlotDataMutAccess::Record(nested)) = record.field_mut(1) else {
        panic!("nested record field");
    };
    let Some(SlotDataMutAccess::Value(count)) = nested.field_mut(0) else {
        panic!("nested count field");
    };
    count
        .set_lp_value(lpc_model::Revision::new(3), LpValue::U32(9))
        .unwrap();
    assert_eq!(record.nested.count.value(), &9);

    assert!(matches!(record.data_mut(), SlotDataMutAccess::Record(_)));
}

fn assert_static_slot_access<T: StaticSlotAccess>() {}
