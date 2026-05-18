use lpc_model::{
    FromLpValue, LpType, LpValue, SlotShapeId, SlotValue, ToLpValue, ValueEditorHint,
    ValueRootError,
};

#[derive(Clone, Copy, Debug, PartialEq, lpc_model::SlotValue)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
pub struct TestRatio(pub f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, lpc_model::SlotValue)]
#[slot_value(editor = dimensions)]
pub struct TestDim {
    pub width: u32,
    pub height: u32,
}

#[test]
fn slot_value_derive_uses_rust_type_name_as_shape_id() {
    assert_eq!(
        TestRatio::SHAPE_ID,
        SlotShapeId::from_static_name("TestRatio")
    );
    assert_eq!(TestRatio::value_shape().id, TestRatio::SHAPE_ID);
}

#[test]
fn slot_value_derive_supports_tuple_newtypes() {
    let value = TestRatio(0.75);

    assert_eq!(value.to_lp_value(), LpValue::F32(0.75));
    assert_eq!(
        TestRatio::from_lp_value(&LpValue::F32(0.25)).unwrap(),
        TestRatio(0.25)
    );
    assert_eq!(TestRatio::value_shape().ty, LpType::F32);
    assert!(matches!(
        TestRatio::value_shape().editor,
        ValueEditorHint::Slider { .. }
    ));
}

#[test]
fn slot_value_derive_supports_named_structs() {
    let value = TestDim {
        width: 64,
        height: 32,
    };

    assert_eq!(TestDim::from_lp_value(&value.to_lp_value()).unwrap(), value);
    assert!(matches!(TestDim::value_shape().ty, LpType::Struct { .. }));
    assert!(matches!(
        TestDim::value_shape().editor,
        ValueEditorHint::Dimensions
    ));
}

#[test]
fn slot_value_derive_rejects_wrong_lp_value_shape() {
    let error = TestDim::from_lp_value(&LpValue::String("bad".to_string())).unwrap_err();

    assert_eq!(error, ValueRootError::new("expected TestDim struct"));
}
