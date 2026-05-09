#![cfg(feature = "derive")]

use lpc_model::{
    SlotAccess, SlotDataAccess, SlotMapValueAccess, SlotRecordAccess, SlotRecordShape, SlotShape,
    SlotShapeRegistry, SlotViewRoot, StaticSlotAccess, StaticSlotShape, ValueSlot,
};

#[derive(lpc_model::SlotRecord)]
#[slot(root, view)]
struct DerivedRecord {
    enabled: ValueSlot<bool>,
    nested: NestedRecord,
}

#[derive(lpc_model::SlotRecord)]
struct NestedRecord {
    count: ValueSlot<u32>,
}

struct DerivedRecordSlotView {
    registry_revision: lpc_model::Revision,
    enabled_accessor: lpc_model::SlotAccessor,
    nested_accessor: lpc_model::SlotAccessor,
}

impl DerivedRecordSlotView {
    fn registry_revision(&self) -> lpc_model::Revision {
        self.registry_revision
    }

    fn is_valid_for(&self, registry: &SlotShapeRegistry) -> bool {
        self.registry_revision == registry.revision()
    }

    fn enabled(&self) -> &lpc_model::SlotAccessor {
        &self.enabled_accessor
    }

    fn nested(&self) -> &lpc_model::SlotAccessor {
        &self.nested_accessor
    }
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
fn derive_generates_compiled_view_for_root_records() {
    let mut registry = SlotShapeRegistry::default();
    DerivedRecord::ensure_registered(&mut registry).unwrap();

    let view = DerivedRecord::compile_slot_view(&registry).unwrap();

    assert_eq!(view.registry_revision(), registry.revision());
    assert!(view.is_valid_for(&registry));
    assert_eq!(view.enabled().path().to_string(), "enabled");
    assert_eq!(view.nested().path().to_string(), "nested");
}

fn assert_static_slot_access<T: StaticSlotAccess>() {}
