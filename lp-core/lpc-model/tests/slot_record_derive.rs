#![cfg(feature = "derive")]

use lpc_model::{
    SlotAccess, SlotDataAccess, SlotDirection, SlotMapValueAccess, SlotMerge, SlotRecordAccess,
    SlotRecordShape, SlotShape, SlotShapeRegistry, StaticSlotAccess, StaticSlotShape, ValueSlot,
};

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct DerivedRecord {
    enabled: ValueSlot<bool>,
    nested: NestedRecord,
}

#[derive(lpc_model::SlotRecord)]
struct NestedRecord {
    count: ValueSlot<u32>,
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

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct SemanticRecord {
    #[slot(consumed, merge = "by_key")]
    emitters: ValueSlot<u32>,
    #[slot(produced)]
    output: ValueSlot<u32>,
}

#[test]
fn derive_preserves_field_semantics() {
    let SlotShape::Record { fields, .. } = SemanticRecord::slot_record_shape() else {
        panic!("record shape");
    };

    assert_eq!(fields[0].name.as_str(), "emitters");
    assert_eq!(fields[0].semantics.direction, SlotDirection::Consumed);
    assert_eq!(fields[0].semantics.merge, SlotMerge::ByKey);

    assert_eq!(fields[1].name.as_str(), "output");
    assert_eq!(fields[1].semantics.direction, SlotDirection::Produced);
    assert_eq!(fields[1].semantics.merge, SlotMerge::Latest);
}

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct WritableDefaultRecord {
    enabled: ValueSlot<bool>,
    #[slot(policy = "read_only_persisted")]
    locked: ValueSlot<bool>,
}

#[test]
fn derive_defaults_fields_to_writable_with_field_override() {
    let SlotShape::Record { fields, .. } = WritableDefaultRecord::slot_record_shape() else {
        panic!("record shape");
    };

    assert!(fields[0].policy.writable);
    assert_eq!(
        fields[0].policy,
        lpc_model::SlotPolicy::writable_persisted()
    );
    assert!(!fields[1].policy.writable);
    assert_eq!(
        fields[1].policy,
        lpc_model::SlotPolicy::read_only_persisted()
    );
}

#[derive(lpc_model::SlotRecord)]
#[slot(root, default_policy = "read_only_transient")]
struct RuntimeStateRecord {
    frame: ValueSlot<u32>,
}

#[test]
fn derive_supports_read_only_state_policy_override() {
    let SlotShape::Record { fields, .. } = RuntimeStateRecord::slot_record_shape() else {
        panic!("record shape");
    };

    assert_eq!(
        fields[0].policy,
        lpc_model::SlotPolicy::read_only_transient()
    );
}

fn assert_static_slot_access<T: StaticSlotAccess>() {}
