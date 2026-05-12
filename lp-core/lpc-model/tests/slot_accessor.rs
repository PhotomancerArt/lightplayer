#![cfg(feature = "derive")]

use lpc_model::{
    LpValue, Revision, SlotAccessor, SlotAccessorError, SlotDataAccess, SlotPath,
    SlotShapeRegistry, StaticSlotShape, ValueSlot,
};

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct AccessorRoot {
    output: ValueSlot<f32>,
}

#[test]
fn compiled_accessor_reads_record_field_by_index() {
    let root = AccessorRoot {
        output: ValueSlot::new(0.25),
    };
    let mut registry = SlotShapeRegistry::default();
    AccessorRoot::ensure_registered(&mut registry).unwrap();
    let accessor = SlotAccessor::compile_value(
        AccessorRoot::SHAPE_ID,
        SlotPath::parse("output").unwrap(),
        &registry,
    )
    .unwrap();

    let data = accessor.access(&root, &registry).unwrap();

    let SlotDataAccess::Value(value) = data else {
        panic!("value access");
    };
    assert_eq!(value.value(), LpValue::F32(0.25));
}

#[test]
fn missing_field_fails_when_accessor_compiles() {
    let mut registry = SlotShapeRegistry::default();
    AccessorRoot::ensure_registered(&mut registry).unwrap();

    let err = SlotAccessor::compile_value(
        AccessorRoot::SHAPE_ID,
        SlotPath::parse("missing").unwrap(),
        &registry,
    )
    .unwrap_err();

    assert_error_contains(err, "record has no field missing");
}

#[test]
fn registry_revision_mismatch_rejects_stale_accessor() {
    let root = AccessorRoot {
        output: ValueSlot::new(0.25),
    };
    let mut registry = SlotShapeRegistry::default();
    AccessorRoot::ensure_registered(&mut registry).unwrap();
    let accessor = SlotAccessor::compile_value(
        AccessorRoot::SHAPE_ID,
        SlotPath::parse("output").unwrap(),
        &registry,
    )
    .unwrap();

    registry.unregister_root_with_version(Revision::new(100), &AccessorRoot::SHAPE_ID);

    let err = match accessor.access(&root, &registry) {
        Ok(_) => panic!("expected stale accessor error"),
        Err(err) => err,
    };
    assert_error_contains(err, "compiled at registry revision");
}

fn assert_error_contains(err: SlotAccessorError, needle: &str) {
    assert!(
        err.to_string().contains(needle),
        "expected {err} to contain {needle:?}"
    );
}
