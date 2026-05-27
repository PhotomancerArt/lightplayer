#![cfg(feature = "derive")]

use lpc_model::{
    LpValue, SlotAccess, SlotDataAccess, SlotDataMutAccess, SlotMapValueAccess, SlotMutAccess,
    SlotPath, SlotRecordAccess, SlotRecordMutAccess, SlotRecordShape, SlotShape, SlotShapeRegistry,
    StaticSlotAccess, StaticSlotShape, StaticSlotShapeDescriptor, ValueSlot, lookup_slot_data,
};

#[derive(lpc_model::Slotted)]
struct DerivedRecord {
    pub enabled: ValueSlot<bool>,
    pub nested: NestedRecord,
}

#[derive(lpc_model::Slotted)]
struct NestedRecord {
    pub count: ValueSlot<u32>,
}

#[derive(lpc_model::Slotted)]
struct WrappedRecord(NestedRecord);

#[derive(lpc_model::Slotted)]
struct RecordWithWrapper {
    pub wrapped: WrappedRecord,
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
    assert!(matches!(&fields[0].shape, SlotShape::Value { .. }));
    assert_eq!(
        &fields[1].shape,
        &SlotShape::Ref {
            id: NestedRecord::SHAPE_ID
        }
    );
    assert_eq!(
        DerivedRecord::slot_record_shape().referenced_shape_ids(),
        vec![NestedRecord::SHAPE_ID]
    );

    let Some(StaticSlotShapeDescriptor::Record {
        fields: static_fields,
        ..
    }) = DerivedRecord::static_slot_shape_descriptor()
    else {
        panic!("static record shape descriptor");
    };
    assert_eq!(static_fields[1].name, "nested");
    assert_eq!(
        static_fields[1].shape,
        &StaticSlotShapeDescriptor::Ref {
            id: NestedRecord::SHAPE_ID
        }
    );

    let mut registry = SlotShapeRegistry::default();
    assert!(ensure_test_shape::<DerivedRecord>(&mut registry));
    assert!(!ensure_test_shape::<DerivedRecord>(&mut registry));
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

#[test]
fn derive_supports_single_field_tuple_wrappers() {
    let mut wrapper = WrappedRecord(NestedRecord {
        count: ValueSlot::new(3),
    });

    assert_eq!(wrapper.shape_id(), WrappedRecord::SHAPE_ID);
    assert_static_slot_access::<WrappedRecord>();
    assert!(matches!(wrapper.data(), SlotDataAccess::Record(_)));

    assert_eq!(
        WrappedRecord::slot_shape(),
        SlotShape::Ref {
            id: NestedRecord::SHAPE_ID
        }
    );
    assert_eq!(
        WrappedRecord::slot_shape().referenced_shape_ids(),
        vec![NestedRecord::SHAPE_ID]
    );

    let Some(SlotDataMutAccess::Value(count)) = (match wrapper.data_mut() {
        SlotDataMutAccess::Record(record) => record.field_mut(0),
        _ => panic!("wrapper should expose wrapped record data"),
    }) else {
        panic!("wrapped count field");
    };
    count
        .set_lp_value(lpc_model::Revision::new(4), LpValue::U32(12))
        .unwrap();
    assert_eq!(wrapper.0.count.value(), &12);

    let parent = RecordWithWrapper {
        wrapped: WrappedRecord(NestedRecord {
            count: ValueSlot::new(7),
        }),
    };
    let Some(SlotDataAccess::Record(wrapped)) = parent.field(0) else {
        panic!("wrapper field should expose wrapped record directly");
    };
    let Some(SlotDataAccess::Value(count)) = wrapped.field(0) else {
        panic!("wrapped count value");
    };
    assert_eq!(count.value(), LpValue::U32(7));

    let SlotShape::Record { fields, .. } = RecordWithWrapper::slot_record_shape() else {
        panic!("parent record shape");
    };
    assert_eq!(
        &fields[0].shape,
        &SlotShape::Ref {
            id: WrappedRecord::SHAPE_ID
        }
    );
    assert_eq!(
        RecordWithWrapper::slot_record_shape().referenced_shape_ids(),
        vec![WrappedRecord::SHAPE_ID]
    );

    let Some(StaticSlotShapeDescriptor::Record {
        fields: static_fields,
        ..
    }) = RecordWithWrapper::static_slot_shape_descriptor()
    else {
        panic!("static parent record shape descriptor");
    };
    assert_eq!(
        static_fields[0].shape,
        &StaticSlotShapeDescriptor::Ref {
            id: WrappedRecord::SHAPE_ID
        }
    );

    let mut registry = SlotShapeRegistry::default();
    ensure_test_shape::<WrappedRecord>(&mut registry);
    ensure_test_shape::<NestedRecord>(&mut registry);
    let found = lookup_slot_data(&wrapper, &registry, &SlotPath::parse("count").unwrap()).unwrap();
    let SlotDataAccess::Value(count) = found else {
        panic!("count value through wrapper path");
    };
    assert_eq!(count.value(), LpValue::U32(12));
}

fn assert_static_slot_access<T: StaticSlotAccess>() {}

fn ensure_test_shape<T: StaticSlotShape>(registry: &mut SlotShapeRegistry) -> bool {
    match T::shape_name() {
        Some(name) => registry.ensure_shape_named(T::SHAPE_ID, name, T::slot_shape()),
        None => registry.ensure_shape(T::SHAPE_ID, T::slot_shape()),
    }
    .unwrap()
}
