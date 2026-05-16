#![cfg(feature = "derive")]

use lpc_model::{
    EnumSlot, LpValue, PositiveF32, PositiveF32Slot, Revision, SlotDataAccess, SlotDataMutAccess,
    SlotEnumAccess, SlotEnumDefaultVariant, SlotEnumShape, SlotRecordMutAccess, SlotShape, Slotted,
    SlottedEnum, ValueSlot,
};

#[derive(Clone, Debug, PartialEq, Slotted)]
enum UnitMode {
    #[default]
    Unset,
    Enabled,
}

#[derive(Clone, Debug, PartialEq, Slotted)]
enum TupleMode {
    #[default]
    Wrapped(NestedPayload),
}

#[derive(Clone, Debug, Default, PartialEq, Slotted)]
struct NestedPayload {
    pub count: ValueSlot<u32>,
}

#[derive(Clone, Debug, PartialEq, Slotted)]
enum RecordMode {
    #[default]
    Unset,
    PathPoints {
        paths: lpc_model::MapSlot<u32, ValueSlot<u32>>,
        sample_diameter: PositiveF32Slot,
    },
}

#[derive(Clone, Debug, PartialEq, Slotted)]
enum RenamedMode {
    #[default]
    #[slot(name = "special")]
    Special,
}

#[test]
fn enum_derive_supports_unit_variants() {
    let mut mode = EnumSlot::new(UnitMode::default());

    assert_eq!(SlotEnumAccess::variant(&mode), "Unset");
    assert!(matches!(mode.data(), SlotDataAccess::Unit(_)));

    SlotEnumDefaultVariant::set_variant_default(&mut mode, Revision::new(9), "Enabled").unwrap();

    assert_eq!(SlotEnumAccess::variant(&mode), "Enabled");
    assert_eq!(mode.variant_revision(), Revision::new(9));
}

#[test]
fn enum_derive_supports_single_tuple_wrappers() {
    let mode = EnumSlot::new(TupleMode::Wrapped(NestedPayload {
        count: ValueSlot::new(7),
    }));

    let SlotShape::Enum { variants, .. } = TupleMode::slot_enum_shape() else {
        panic!("enum shape");
    };
    assert_eq!(variants[0].name.as_str(), "Wrapped");
    assert!(matches!(variants[0].shape, SlotShape::Record { .. }));

    let SlotDataAccess::Record(record) = mode.data() else {
        panic!("tuple wrapper payload should expose wrapped record");
    };
    let Some(SlotDataAccess::Value(count)) = record.field(0) else {
        panic!("count field");
    };
    assert_eq!(count.value(), LpValue::U32(7));
}

#[test]
fn enum_derive_supports_named_variant_records() {
    let mut mode = RecordMode::PathPoints {
        paths: lpc_model::MapSlot::default(),
        sample_diameter: PositiveF32Slot::new(PositiveF32(2.5)),
    };

    assert_eq!(mode.variant(), "PathPoints");
    let SlotDataAccess::Record(record) = mode.data() else {
        panic!("named variant should expose record data");
    };
    assert!(matches!(record.field(0), Some(SlotDataAccess::Map(_))));
    let Some(SlotDataAccess::Value(sample_diameter)) = record.field(1) else {
        panic!("sample diameter");
    };
    assert_eq!(sample_diameter.value(), LpValue::F32(2.5));

    let Some(SlotDataMutAccess::Value(sample_diameter)) = mode.field_mut(1) else {
        panic!("sample diameter mut");
    };
    sample_diameter
        .set_lp_value(Revision::new(11), LpValue::F32(3.0))
        .unwrap();

    let RecordMode::PathPoints {
        sample_diameter, ..
    } = mode
    else {
        panic!("path points");
    };
    assert_eq!(sample_diameter.value().0, 3.0);
}

#[test]
fn enum_derive_can_switch_to_default_named_variant_payloads() {
    let mut mode = EnumSlot::new(RecordMode::Unset);

    SlotEnumDefaultVariant::set_variant_default(&mut mode, Revision::new(4), "PathPoints").unwrap();

    assert_eq!(SlotEnumAccess::variant(&mode), "PathPoints");
    let SlotDataAccess::Record(record) = mode.data() else {
        panic!("record");
    };
    let Some(SlotDataAccess::Map(paths)) = record.field(0) else {
        panic!("paths");
    };
    assert!(paths.keys().is_empty());

    let err = SlotEnumDefaultVariant::set_variant_default(&mut mode, Revision::new(5), "Missing")
        .expect_err("unknown variant");
    assert!(err.to_string().contains("PathPoints"));
}

#[test]
fn enum_derive_supports_variant_name_escape_hatch() {
    let mode = EnumSlot::new(RenamedMode::default());

    assert_eq!(SlotEnumAccess::variant(&mode), "special");
}
