//! Generic mutation through mutable slot access.

use crate::{
    LpType, LpValue, ProductKind, Revision, SlotAccess, SlotDataAccess, SlotDataMutAccess,
    SlotMapKey, SlotMutAccess, SlotMutationError, SlotPath, SlotPathSegment, SlotShape,
    SlotShapeRegistry,
};
use alloc::format;
use alloc::string::{String, ToString};

/// Set an existing value leaf by slot path.
pub fn set_slot_value(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    value: LpValue,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    set_slot_value_in_shape(
        root.data_mut(),
        shape,
        registry,
        path.segments(),
        revision,
        value,
    )
}

/// Switch an enum slot to a default-constructed variant.
pub fn set_slot_variant_default(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    variant: &str,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    set_slot_variant_default_in_shape(
        root.data_mut(),
        shape,
        registry,
        path.segments(),
        revision,
        variant,
    )
}

/// Read the revision for the slot data at a path.
pub fn slot_data_revision(
    root: &dyn SlotAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
) -> Result<Revision, SlotMutationError> {
    registry.get(&root.shape_id()).ok_or_else(|| {
        SlotMutationError::unknown_path(format!("missing slot path root shape {}", root.shape_id()))
    })?;
    let data = crate::lookup_slot_data(root, registry, path)
        .map_err(|err| SlotMutationError::unknown_path(err.to_string()))?;
    Ok(revision_for_data(data))
}

fn set_slot_value_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
    value: LpValue,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape, data) {
            (SlotShape::Value { shape }, SlotDataMutAccess::Value(value_slot)) => {
                if !lp_value_matches_type(&value, &shape.ty) {
                    return Err(SlotMutationError::wrong_type(format!(
                        "expected {:?}, got {:?}",
                        shape.ty, value
                    )));
                }
                value_slot.set_lp_value(revision, value)
            }
            (SlotShape::Value { .. }, _) => Err(SlotMutationError::unsupported_target(
                "slot path resolves to value shape but not value data",
            )),
            _ => Err(SlotMutationError::unsupported_target(
                "set value mutation requires a value leaf",
            )),
        };
    };

    match (shape, data, head) {
        (
            SlotShape::Record { fields, .. },
            SlotDataMutAccess::Record(record),
            SlotPathSegment::Field(name),
        ) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *name)
                .ok_or_else(|| {
                    SlotMutationError::unknown_path(format!("record has no field {name}"))
                })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            set_slot_value_in_shape(field_data, &field.shape, registry, tail, revision, value)
        }
        (
            SlotShape::Map {
                value: value_shape, ..
            },
            SlotDataMutAccess::Map(map),
            SlotPathSegment::Key(key),
        ) => {
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            set_slot_value_in_shape(item_data, value_shape, registry, tail, revision, value)
        }
        (
            SlotShape::Option { some, .. },
            SlotDataMutAccess::Option(option),
            SlotPathSegment::Field(name),
        ) if name.as_str() == "some" => {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            set_slot_value_in_shape(data, some, registry, tail, revision, value)
        }
        (
            SlotShape::Enum { variants, .. },
            SlotDataMutAccess::Enum(en),
            SlotPathSegment::Field(name),
        ) => {
            let active = String::from(en.variant());
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == active)
                .ok_or_else(|| {
                    SlotMutationError::unknown_variant(format!(
                        "enum has no active variant {active:?}"
                    ))
                })?;
            let data = en.data_mut();
            if name.as_str() == active {
                set_slot_value_in_shape(data, &variant.shape, registry, tail, revision, value)
            } else {
                set_slot_value_in_shape(data, &variant.shape, registry, segments, revision, value)
                    .map_err(|_| {
                        SlotMutationError::unknown_path(format!(
                            "unknown path in active enum variant {active:?}: {name}"
                        ))
                    })
            }
        }
        (_, _, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, _, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn set_slot_variant_default_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
    requested_variant: &str,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape, data) {
            (SlotShape::Enum { variants, .. }, SlotDataMutAccess::Enum(en)) => {
                if !variants
                    .iter()
                    .any(|variant| variant.name.as_str() == requested_variant)
                {
                    return Err(SlotMutationError::unknown_variant(format!(
                        "enum has no variant {requested_variant:?}"
                    )));
                }
                en.set_variant_default(revision, requested_variant)
            }
            _ => Err(SlotMutationError::unsupported_target(
                "set variant mutation requires an enum slot",
            )),
        };
    };

    match (shape, data, head) {
        (
            SlotShape::Record { fields, .. },
            SlotDataMutAccess::Record(record),
            SlotPathSegment::Field(name),
        ) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *name)
                .ok_or_else(|| {
                    SlotMutationError::unknown_path(format!("record has no field {name}"))
                })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            set_slot_variant_default_in_shape(
                field_data,
                &field.shape,
                registry,
                tail,
                revision,
                requested_variant,
            )
        }
        (SlotShape::Map { value, .. }, SlotDataMutAccess::Map(map), SlotPathSegment::Key(key)) => {
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            set_slot_variant_default_in_shape(
                item_data,
                value,
                registry,
                tail,
                revision,
                requested_variant,
            )
        }
        (
            SlotShape::Option { some, .. },
            SlotDataMutAccess::Option(option),
            SlotPathSegment::Field(name),
        ) if name.as_str() == "some" => {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            set_slot_variant_default_in_shape(
                data,
                some,
                registry,
                tail,
                revision,
                requested_variant,
            )
        }
        (
            SlotShape::Enum { variants, .. },
            SlotDataMutAccess::Enum(en),
            SlotPathSegment::Field(name),
        ) => {
            let active = String::from(en.variant());
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == active)
                .ok_or_else(|| {
                    SlotMutationError::unknown_variant(format!(
                        "enum has no active variant {active:?}"
                    ))
                })?;
            let data = en.data_mut();
            if name.as_str() == active {
                set_slot_variant_default_in_shape(
                    data,
                    &variant.shape,
                    registry,
                    tail,
                    revision,
                    requested_variant,
                )
            } else {
                set_slot_variant_default_in_shape(
                    data,
                    &variant.shape,
                    registry,
                    segments,
                    revision,
                    requested_variant,
                )
            }
        }
        (_, _, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, _, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn revision_for_data(data: SlotDataAccess<'_>) -> Revision {
    match data {
        SlotDataAccess::Unit(revision) => revision,
        SlotDataAccess::Value(value) => value.changed_at(),
        SlotDataAccess::Record(record) => record.fields_revision(),
        SlotDataAccess::Map(map) => map.keys_revision(),
        SlotDataAccess::Enum(en) => en.variant_revision(),
        SlotDataAccess::Option(option) => option.presence_revision(),
    }
}

fn root_shape<'a>(
    root: &dyn SlotMutAccess,
    registry: &'a SlotShapeRegistry,
) -> Result<&'a SlotShape, SlotMutationError> {
    registry.get(&root.shape_id()).ok_or_else(|| {
        SlotMutationError::unknown_path(format!("missing slot path root shape {}", root.shape_id()))
    })
}

fn resolve_ref_shape<'a>(
    mut shape: &'a SlotShape,
    registry: &'a SlotShapeRegistry,
) -> Result<&'a SlotShape, SlotMutationError> {
    while let SlotShape::Ref { id } = shape {
        shape = registry.get(id).ok_or_else(|| {
            SlotMutationError::unknown_path(format!("missing referenced slot shape {id}"))
        })?;
    }
    Ok(shape)
}

fn display_key(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => format!("{value:?}"),
        SlotMapKey::I32(value) => format!("{value}"),
        SlotMapKey::U32(value) => format!("{value}"),
    }
}

fn lp_value_matches_type(value: &LpValue, ty: &LpType) -> bool {
    match (value, ty) {
        (LpValue::String(_), LpType::String)
        | (LpValue::I32(_), LpType::I32)
        | (LpValue::U32(_), LpType::U32)
        | (LpValue::F32(_), LpType::F32)
        | (LpValue::Bool(_), LpType::Bool)
        | (LpValue::Vec2(_), LpType::Vec2)
        | (LpValue::Vec3(_), LpType::Vec3)
        | (LpValue::Vec4(_), LpType::Vec4)
        | (LpValue::IVec2(_), LpType::IVec2)
        | (LpValue::IVec3(_), LpType::IVec3)
        | (LpValue::IVec4(_), LpType::IVec4)
        | (LpValue::UVec2(_), LpType::UVec2)
        | (LpValue::UVec3(_), LpType::UVec3)
        | (LpValue::UVec4(_), LpType::UVec4)
        | (LpValue::BVec2(_), LpType::BVec2)
        | (LpValue::BVec3(_), LpType::BVec3)
        | (LpValue::BVec4(_), LpType::BVec4)
        | (LpValue::Mat2x2(_), LpType::Mat2x2)
        | (LpValue::Mat3x3(_), LpType::Mat3x3)
        | (LpValue::Mat4x4(_), LpType::Mat4x4)
        | (LpValue::Resource(_), LpType::Resource) => true,
        (LpValue::Product(product), LpType::Product(kind)) => match (product, kind) {
            (crate::ProductRef::Visual(_), ProductKind::Visual)
            | (crate::ProductRef::Control(_), ProductKind::Control) => true,
            _ => false,
        },
        (LpValue::Array(values), LpType::Array(item_ty, len)) => {
            values.len() == *len
                && values
                    .iter()
                    .all(|value| lp_value_matches_type(value, item_ty))
        }
        (LpValue::Array(values), LpType::List(item_ty)) => values
            .iter()
            .all(|value| lp_value_matches_type(value, item_ty)),
        (LpValue::Struct { fields: values, .. }, LpType::Struct { fields, .. }) => {
            values.len() == fields.len()
                && fields.iter().all(|field| {
                    values
                        .iter()
                        .find(|(name, _)| name == &field.name)
                        .is_some_and(|(_, value)| lp_value_matches_type(value, &field.ty))
                })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FieldSlot, FieldSlotMut, MapSlot, OptionSlot, SlotDataAccess, SlotEnumAccess,
        SlotEnumDefaultVariant, SlotEnumMutAccess, SlotEnumShape, SlotMapValueAccess,
        SlotMapValueMutAccess, SlotMeta, SlotRecordAccess, SlotRecordMutAccess, SlotShapeRegistry,
        StaticSlotShape, ValueSlot,
    };
    use alloc::collections::BTreeMap;
    use alloc::vec;

    #[derive(crate::SlotRecord)]
    struct MutRoot {
        pub gain: ValueSlot<f32>,
        pub params: MapSlot<String, ValueSlot<f32>>,
        pub enabled: OptionSlot<ValueSlot<bool>>,
        #[slot(enum)]
        pub mode: TestEnum,
    }

    enum TestEnum {
        A {
            variant_revision: Revision,
            value: ValueSlot<f32>,
        },
        B {
            variant_revision: Revision,
            other: ValueSlot<f32>,
        },
    }

    impl TestEnum {
        fn a() -> Self {
            Self::A {
                variant_revision: Revision::new(1),
                value: ValueSlot::new(1.0),
            }
        }
    }

    impl SlotEnumShape for TestEnum {
        fn slot_enum_shape() -> SlotShape {
            use crate::slot::shape::{field, record, value, variant};

            SlotShape::Enum {
                meta: SlotMeta::empty(),
                variants: vec![
                    variant("a", record(vec![field("value", value(crate::LpType::F32))])),
                    variant("b", record(vec![field("other", value(crate::LpType::F32))])),
                ],
            }
        }
    }

    impl SlotEnumAccess for TestEnum {
        fn variant_revision(&self) -> Revision {
            match self {
                Self::A {
                    variant_revision, ..
                }
                | Self::B {
                    variant_revision, ..
                } => *variant_revision,
            }
        }

        fn variant(&self) -> &str {
            match self {
                Self::A { .. } => "a",
                Self::B { .. } => "b",
            }
        }

        fn data(&self) -> SlotDataAccess<'_> {
            SlotDataAccess::Record(self)
        }
    }

    impl SlotEnumMutAccess for TestEnum {
        fn variant_revision(&self) -> Revision {
            SlotEnumAccess::variant_revision(self)
        }

        fn variant(&self) -> &str {
            SlotEnumAccess::variant(self)
        }

        fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
            SlotDataMutAccess::Record(self)
        }
    }

    impl SlotEnumDefaultVariant for TestEnum {
        fn set_variant_default(
            &mut self,
            revision: Revision,
            variant: &str,
        ) -> Result<(), SlotMutationError> {
            match variant {
                "a" => {
                    *self = Self::A {
                        variant_revision: revision,
                        value: ValueSlot::default(),
                    };
                    Ok(())
                }
                "b" => {
                    *self = Self::B {
                        variant_revision: revision,
                        other: ValueSlot::default(),
                    };
                    Ok(())
                }
                other => Err(SlotMutationError::unknown_variant(format!(
                    "unknown TestEnum variant {other:?}"
                ))),
            }
        }
    }

    impl SlotRecordAccess for TestEnum {
        fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
            match self {
                Self::A { value, .. } => match index {
                    0 => Some(SlotDataAccess::Value(value)),
                    _ => None,
                },
                Self::B { other, .. } => match index {
                    0 => Some(SlotDataAccess::Value(other)),
                    _ => None,
                },
            }
        }
    }

    impl SlotRecordMutAccess for TestEnum {
        fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
            match self {
                Self::A { value, .. } => match index {
                    0 => Some(SlotDataMutAccess::Value(value)),
                    _ => None,
                },
                Self::B { other, .. } => match index {
                    0 => Some(SlotDataMutAccess::Value(other)),
                    _ => None,
                },
            }
        }
    }

    impl SlotMapValueAccess for TestEnum {
        fn slot_data(&self) -> SlotDataAccess<'_> {
            SlotDataAccess::Enum(self)
        }
    }

    impl SlotMapValueMutAccess for TestEnum {
        fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
            SlotDataMutAccess::Enum(self)
        }
    }

    impl FieldSlot for TestEnum {
        fn slot_field_shape() -> SlotShape {
            Self::slot_enum_shape()
        }

        fn slot_field_data(&self) -> SlotDataAccess<'_> {
            SlotDataAccess::Enum(self)
        }
    }

    impl FieldSlotMut for TestEnum {
        fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
            SlotDataMutAccess::Enum(self)
        }
    }

    #[test]
    fn slot_mutation_sets_record_value_leaf() {
        let mut root = test_root();
        let registry = registry();

        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("gain").unwrap(),
            Revision::new(5),
            LpValue::F32(2.0),
        )
        .unwrap();

        assert_eq!(root.gain.value(), &2.0);
        assert_eq!(root.gain.revision(), Revision::new(5));
    }

    #[test]
    fn slot_mutation_sets_existing_map_value_leaf() {
        let mut root = test_root();
        let registry = registry();

        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("params[exposure]").unwrap(),
            Revision::new(6),
            LpValue::F32(3.0),
        )
        .unwrap();

        assert_eq!(root.params.entries["exposure"].value(), &3.0);
    }

    #[test]
    fn slot_mutation_sets_option_some_value_leaf() {
        let mut root = test_root();
        let registry = registry();

        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("enabled.some").unwrap(),
            Revision::new(7),
            LpValue::Bool(false),
        )
        .unwrap();

        assert_eq!(root.enabled.data.as_ref().unwrap().value(), &false);
    }

    #[test]
    fn slot_mutation_sets_active_enum_payload_leaf() {
        let mut root = test_root();
        let registry = registry();

        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("mode.value").unwrap(),
            Revision::new(8),
            LpValue::F32(4.0),
        )
        .unwrap();

        let TestEnum::A { value, .. } = &root.mode else {
            panic!("expected a");
        };
        assert_eq!(value.value(), &4.0);
    }

    #[test]
    fn slot_mutation_switches_enum_to_default_variant_then_sets_payload() {
        let mut root = test_root();
        let registry = registry();

        set_slot_variant_default(
            &mut root,
            &registry,
            &SlotPath::parse("mode").unwrap(),
            Revision::new(9),
            "b",
        )
        .unwrap();
        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("mode.other").unwrap(),
            Revision::new(10),
            LpValue::F32(5.0),
        )
        .unwrap();

        let TestEnum::B {
            variant_revision,
            other,
        } = &root.mode
        else {
            panic!("expected b");
        };
        assert_eq!(*variant_revision, Revision::new(9));
        assert_eq!(other.value(), &5.0);
    }

    fn test_root() -> MutRoot {
        MutRoot {
            gain: ValueSlot::new(1.0),
            params: MapSlot::new(BTreeMap::from([(
                String::from("exposure"),
                ValueSlot::new(1.0),
            )])),
            enabled: OptionSlot::some(ValueSlot::new(true)),
            mode: TestEnum::a(),
        }
    }

    fn registry() -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        MutRoot::ensure_registered(&mut registry).unwrap();
        registry
    }
}
