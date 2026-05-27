//! Generic mutation through mutable slot access.

use crate::{
    LpType, LpValue, ProductKind, Revision, SlotAccess, SlotDataAccess, SlotDataMutAccess,
    SlotMapKey, SlotMutAccess, SlotMutationError, SlotName, SlotPath, SlotPathSegment, SlotShape,
    SlotShapeLookup, SlotShapeRegistry, SlotShapeView,
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

/// Ensure every structural segment in `path` exists.
///
/// Missing map entries are default-constructed, `some` option bodies are
/// default-constructed, and enum variant segments select the requested variant.
pub fn ensure_slot_present(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    ensure_slot_present_in_shape(root.data_mut(), shape, registry, path.segments(), revision)
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

/// Insert a default value into an existing map slot by path.
pub fn insert_slot_map_entry_default(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    key: &SlotMapKey,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    insert_slot_map_entry_default_in_shape(
        root.data_mut(),
        shape,
        registry,
        path.segments(),
        revision,
        key,
    )
}

/// Set an option slot to `none`.
pub fn set_slot_option_none(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    set_slot_option_none_in_shape(root.data_mut(), shape, registry, path.segments(), revision)
}

/// Remove one map entry by path.
pub fn remove_slot_map_entry(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    key: &SlotMapKey,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    remove_slot_map_entry_in_shape(
        root.data_mut(),
        shape,
        registry,
        path.segments(),
        revision,
        key,
    )
}

/// Set an option slot to `some(default)`.
pub fn set_slot_option_some_default(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
) -> Result<(), SlotMutationError> {
    let shape = root_shape(root, registry)?;
    set_slot_option_some_default_in_shape(
        root.data_mut(),
        shape,
        registry,
        path.segments(),
        revision,
    )
}

/// Read the revision for the slot data at a path.
pub fn slot_data_revision(
    root: &dyn SlotAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
) -> Result<Revision, SlotMutationError> {
    root_shape_ref(root, registry)?;
    let data = crate::lookup_slot_data(root, registry, path)
        .map_err(|err| SlotMutationError::unknown_path(err.to_string()))?;
    Ok(revision_for_data(data))
}

fn ensure_slot_present_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.option_some(), data) {
            (Some(some), SlotDataMutAccess::Option(option)) => {
                if option.data_mut().is_none() {
                    let owned_some = some.to_owned_shape();
                    option.set_some_default(revision, registry, &owned_some)?;
                }
                Ok(())
            }
            _ => Ok(()),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            ensure_slot_present_in_shape(field_data, field.shape(), registry, tail, revision)
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            let value_shape = shape.map_value().expect("map value shape");
            if map.get_mut(key).is_none() {
                let owned_value = value_shape.to_owned_shape();
                map.insert_default(revision, key, registry, &owned_value)?;
            }
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            ensure_slot_present_in_shape(item_data, value_shape, registry, tail, revision)
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let some_shape = shape.option_some().expect("option some shape");
            if option.data_mut().is_none() {
                let owned_some = some_shape.to_owned_shape();
                option.set_some_default(revision, registry, &owned_some)?;
            }
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            ensure_slot_present_in_shape(data, some_shape, registry, tail, revision)
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            if let Some(variant) = shape.enum_variant_by_name(name) {
                if en.variant() != name.as_str() {
                    let owned_shape = shape.to_owned_shape();
                    let SlotShape::Enum { variants, .. } = owned_shape else {
                        unreachable!("enum shape checked above");
                    };
                    en.set_variant_default_with_shape(
                        revision,
                        name.as_str(),
                        registry,
                        &variants,
                    )?;
                }
                let data = en.data_mut();
                ensure_slot_present_in_shape(data, variant.shape(), registry, tail, revision)
            } else {
                let active = String::from(en.variant());
                let variant = enum_variant_by_str(shape, &active)?;
                let data = en.data_mut();
                ensure_slot_present_in_shape(data, variant, registry, segments, revision)
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn set_slot_value_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
    value: LpValue,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.value_shape(), data) {
            (Some(shape), SlotDataMutAccess::Value(value_slot)) => {
                let ty = shape.ty_owned();
                if !lp_value_matches_type(&value, &ty) {
                    return Err(SlotMutationError::wrong_type(format!(
                        "expected {ty:?}, got {value:?}"
                    )));
                }
                value_slot.set_lp_value(revision, value)
            }
            (Some(_), _) => Err(SlotMutationError::unsupported_target(
                "slot path resolves to value shape but not value data",
            )),
            _ => Err(SlotMutationError::unsupported_target(
                "set value mutation requires a value leaf",
            )),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            set_slot_value_in_shape(field_data, field.shape(), registry, tail, revision, value)
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            set_slot_value_in_shape(
                item_data,
                shape.map_value().expect("map value shape"),
                registry,
                tail,
                revision,
                value,
            )
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            set_slot_value_in_shape(
                data,
                shape.option_some().expect("option some shape"),
                registry,
                tail,
                revision,
                value,
            )
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            let active = String::from(en.variant());
            let variant = enum_variant_by_str(shape, &active)?;
            let data = en.data_mut();
            if name.as_str() == active {
                set_slot_value_in_shape(data, variant, registry, tail, revision, value)
            } else {
                set_slot_value_in_shape(data, variant, registry, segments, revision, value).map_err(
                    |_| {
                        SlotMutationError::unknown_path(format!(
                            "unknown path in active enum variant {active:?}: {name}"
                        ))
                    },
                )
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn set_slot_variant_default_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
    requested_variant: &str,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.is_enum(), data) {
            (true, SlotDataMutAccess::Enum(en)) => {
                enum_variant_by_str(shape, requested_variant)?;
                let owned_shape = shape.to_owned_shape();
                let SlotShape::Enum { variants, .. } = owned_shape else {
                    unreachable!("enum shape checked above");
                };
                en.set_variant_default_with_shape(revision, requested_variant, registry, &variants)
            }
            _ => Err(SlotMutationError::unsupported_target(
                "set variant mutation requires an enum slot",
            )),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            set_slot_variant_default_in_shape(
                field_data,
                field.shape(),
                registry,
                tail,
                revision,
                requested_variant,
            )
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            set_slot_variant_default_in_shape(
                item_data,
                shape.map_value().expect("map value shape"),
                registry,
                tail,
                revision,
                requested_variant,
            )
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            set_slot_variant_default_in_shape(
                data,
                shape.option_some().expect("option some shape"),
                registry,
                tail,
                revision,
                requested_variant,
            )
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            let active = String::from(en.variant());
            let variant = enum_variant_by_str(shape, &active)?;
            let data = en.data_mut();
            if name.as_str() == active {
                set_slot_variant_default_in_shape(
                    data,
                    variant,
                    registry,
                    tail,
                    revision,
                    requested_variant,
                )
            } else {
                set_slot_variant_default_in_shape(
                    data,
                    variant,
                    registry,
                    segments,
                    revision,
                    requested_variant,
                )
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn insert_slot_map_entry_default_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
    key: &SlotMapKey,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.map_value(), data) {
            (Some(value), SlotDataMutAccess::Map(map)) => {
                let owned_value = value.to_owned_shape();
                map.insert_default(revision, key, registry, &owned_value)
            }
            _ => Err(SlotMutationError::unsupported_target(
                "insert default map entry requires a map slot",
            )),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            insert_slot_map_entry_default_in_shape(
                field_data,
                field.shape(),
                registry,
                tail,
                revision,
                key,
            )
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(map_key))
            if shape.map_value().is_some() =>
        {
            let item_data = map.get_mut(map_key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(map_key)))
            })?;
            insert_slot_map_entry_default_in_shape(
                item_data,
                shape.map_value().expect("map value shape"),
                registry,
                tail,
                revision,
                key,
            )
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            insert_slot_map_entry_default_in_shape(
                data,
                shape.option_some().expect("option some shape"),
                registry,
                tail,
                revision,
                key,
            )
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            let active = String::from(en.variant());
            let variant = enum_variant_by_str(shape, &active)?;
            let data = en.data_mut();
            if name.as_str() == active {
                insert_slot_map_entry_default_in_shape(data, variant, registry, tail, revision, key)
            } else {
                insert_slot_map_entry_default_in_shape(
                    data, variant, registry, segments, revision, key,
                )
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn set_slot_option_none_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.option_some(), data) {
            (_, SlotDataMutAccess::Option(option)) => option.clear_presence(revision),
            _ => Err(SlotMutationError::unsupported_target(
                "set option none requires an option slot",
            )),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            set_slot_option_none_in_shape(field_data, field.shape(), registry, tail, revision)
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            set_slot_option_none_in_shape(
                item_data,
                shape.map_value().expect("map value shape"),
                registry,
                tail,
                revision,
            )
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            set_slot_option_none_in_shape(
                data,
                shape.option_some().expect("option some shape"),
                registry,
                tail,
                revision,
            )
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            let active = String::from(en.variant());
            let variant = enum_variant_by_str(shape, &active)?;
            let data = en.data_mut();
            if name.as_str() == active {
                set_slot_option_none_in_shape(data, variant, registry, tail, revision)
            } else {
                set_slot_option_none_in_shape(data, variant, registry, segments, revision)
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn remove_slot_map_entry_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
    key: &SlotMapKey,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.map_value(), data) {
            (_, SlotDataMutAccess::Map(map)) => map.remove_entry(revision, key),
            _ => Err(SlotMutationError::unsupported_target(
                "remove map entry requires a map slot",
            )),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            remove_slot_map_entry_in_shape(field_data, field.shape(), registry, tail, revision, key)
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(map_key))
            if shape.map_value().is_some() =>
        {
            let item_data = map.get_mut(map_key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(map_key)))
            })?;
            remove_slot_map_entry_in_shape(
                item_data,
                shape.map_value().expect("map value shape"),
                registry,
                tail,
                revision,
                key,
            )
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            remove_slot_map_entry_in_shape(
                data,
                shape.option_some().expect("option some shape"),
                registry,
                tail,
                revision,
                key,
            )
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            let active = String::from(en.variant());
            let variant = enum_variant_by_str(shape, &active)?;
            let data = en.data_mut();
            if name.as_str() == active {
                remove_slot_map_entry_in_shape(data, variant, registry, tail, revision, key)
            } else {
                remove_slot_map_entry_in_shape(data, variant, registry, segments, revision, key)
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn set_slot_option_some_default_in_shape(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    revision: Revision,
) -> Result<(), SlotMutationError> {
    let shape = resolve_ref_shape(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return match (shape.option_some(), data) {
            (Some(some), SlotDataMutAccess::Option(option)) => {
                let owned_some = some.to_owned_shape();
                option.set_some_default(revision, registry, &owned_some)
            }
            _ => Err(SlotMutationError::unsupported_target(
                "set option some default requires an option slot",
            )),
        };
    };

    match (data, head) {
        (SlotDataMutAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_fields_len().is_some() =>
        {
            let (index, field) = shape.record_field_by_name(name).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record has no field {name}"))
            })?;
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("record field {name} has no data"))
            })?;
            set_slot_option_some_default_in_shape(
                field_data,
                field.shape(),
                registry,
                tail,
                revision,
            )
        }
        (SlotDataMutAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            let item_data = map.get_mut(key).ok_or_else(|| {
                SlotMutationError::unknown_path(format!("map has no key {}", display_key(key)))
            })?;
            set_slot_option_some_default_in_shape(
                item_data,
                shape.map_value().expect("map value shape"),
                registry,
                tail,
                revision,
            )
        }
        (SlotDataMutAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotMutationError::unknown_path("option slot is none"))?;
            set_slot_option_some_default_in_shape(
                data,
                shape.option_some().expect("option some shape"),
                registry,
                tail,
                revision,
            )
        }
        (SlotDataMutAccess::Enum(en), SlotPathSegment::Field(name)) if shape.is_enum() => {
            let active = String::from(en.variant());
            let variant = enum_variant_by_str(shape, &active)?;
            let data = en.data_mut();
            if name.as_str() == active {
                set_slot_option_some_default_in_shape(data, variant, registry, tail, revision)
            } else {
                set_slot_option_some_default_in_shape(data, variant, registry, segments, revision)
            }
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotMutationError::unknown_path(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotMutationError::unknown_path(format!(
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
        SlotDataAccess::Custom(custom) => custom.custom_revision(),
    }
}

fn root_shape<'a>(
    root: &dyn SlotMutAccess,
    registry: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, SlotMutationError> {
    registry.get_shape(root.shape_id()).ok_or_else(|| {
        SlotMutationError::unknown_path(format!("missing slot path root shape {}", root.shape_id()))
    })
}

fn root_shape_ref<'a>(
    root: &dyn SlotAccess,
    registry: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, SlotMutationError> {
    registry.get_shape(root.shape_id()).ok_or_else(|| {
        SlotMutationError::unknown_path(format!("missing slot path root shape {}", root.shape_id()))
    })
}

fn resolve_ref_shape<'a>(
    mut shape: SlotShapeView<'a>,
    registry: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, SlotMutationError> {
    while let Some(id) = shape.ref_id() {
        shape = registry.get_shape(id).ok_or_else(|| {
            SlotMutationError::unknown_path(format!("missing referenced slot shape {id}"))
        })?;
    }
    Ok(shape)
}

fn enum_variant_by_str<'a>(
    shape: SlotShapeView<'a>,
    variant: &str,
) -> Result<SlotShapeView<'a>, SlotMutationError> {
    let name = SlotName::parse(variant).map_err(|_| {
        SlotMutationError::unknown_variant(format!("invalid enum variant {variant:?}"))
    })?;
    shape
        .enum_variant_by_name(&name)
        .map(|variant| variant.shape())
        .ok_or_else(|| {
            SlotMutationError::unknown_variant(format!("enum has no variant {variant:?}"))
        })
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
        (_, LpType::Any) => true,
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
        (LpValue::Enum { variant, payload }, LpType::Enum { variants, .. }) => variants
            .get(*variant as usize)
            .is_some_and(|variant| match (&variant.payload, payload.as_deref()) {
                (Some(payload_ty), Some(payload)) => lp_value_matches_type(payload, payload_ty),
                (None, None) => true,
                _ => false,
            }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EnumSlot, MapSlot, OptionSlot, SlotDataAccess, SlotEnumShape, SlotMapValueAccess,
        SlotMapValueMutAccess, SlotMeta, SlotRecordAccess, SlotRecordMutAccess, SlotShapeId,
        SlotShapeRegistry, SlottedEnum, SlottedEnumMut, StaticSlotShape, ValueSlot,
    };
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use alloc::vec;

    #[derive(crate::Slotted)]
    struct MutRoot {
        pub gain: ValueSlot<f32>,
        pub params: MapSlot<String, ValueSlot<f32>>,
        pub enabled: OptionSlot<ValueSlot<bool>>,
        pub payload: ValueSlot<LpValue>,
        pub mode: EnumSlot<TestEnum>,
    }

    enum TestEnum {
        A { value: ValueSlot<f32> },
        B { other: ValueSlot<f32> },
    }

    impl TestEnum {
        fn a() -> Self {
            Self::A {
                value: ValueSlot::new(1.0),
            }
        }
    }

    impl SlotEnumShape for TestEnum {
        fn slot_enum_shape() -> SlotShape {
            use crate::slot::shape::{field, record, value, variant};

            SlotShape::Enum {
                meta: SlotMeta::empty(),
                encoding: crate::SlotEnumEncoding::default(),
                variants: vec![
                    variant("a", record(vec![field("value", value(crate::LpType::F32))])),
                    variant("b", record(vec![field("other", value(crate::LpType::F32))])),
                ],
            }
        }
    }

    impl SlottedEnum for TestEnum {
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

    impl SlottedEnumMut for TestEnum {
        fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
            SlotDataMutAccess::Record(self)
        }

        fn set_variant_default(&mut self, variant: &str) -> Result<(), SlotMutationError> {
            match variant {
                "a" => {
                    *self = Self::A {
                        value: ValueSlot::default(),
                    };
                    Ok(())
                }
                "b" => {
                    *self = Self::B {
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
            SlotDataAccess::Record(self)
        }
    }

    impl SlotMapValueMutAccess for TestEnum {
        fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
            SlotDataMutAccess::Record(self)
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
    fn slot_mutation_accepts_raw_enum_value_leaf() {
        let mut root = test_root();
        let registry = registry();
        let value = LpValue::Enum {
            variant: 3,
            payload: Some(Box::new(LpValue::F32(0.5))),
        };

        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("payload").unwrap(),
            Revision::new(9),
            value.clone(),
        )
        .unwrap();

        assert_eq!(root.payload.value(), &value);
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
    fn slot_mutation_inserts_typed_map_default_then_sets_leaf() {
        let mut root = MutRoot {
            params: MapSlot::default(),
            ..test_root()
        };
        let registry = registry();

        insert_slot_map_entry_default(
            &mut root,
            &registry,
            &SlotPath::parse("params").unwrap(),
            Revision::new(6),
            &SlotMapKey::String(String::from("gain")),
        )
        .unwrap();
        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("params[gain]").unwrap(),
            Revision::new(7),
            LpValue::F32(9.0),
        )
        .unwrap();

        assert_eq!(root.params.keys_revision, Revision::new(6));
        assert_eq!(root.params.entries["gain"].value(), &9.0);
    }

    #[test]
    fn slot_mutation_set_value_still_rejects_missing_map_key() {
        let mut root = MutRoot {
            params: MapSlot::default(),
            ..test_root()
        };
        let registry = registry();

        let error = set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("params[gain]").unwrap(),
            Revision::new(7),
            LpValue::F32(9.0),
        )
        .unwrap_err();

        assert!(matches!(error, SlotMutationError::UnknownPath { .. }));
        assert!(!root.params.entries.contains_key("gain"));
    }

    #[test]
    fn ensure_slot_present_inserts_missing_map_key_without_replacing_existing() {
        let mut root = test_root();
        let registry = registry();

        ensure_slot_present(
            &mut root,
            &registry,
            &SlotPath::parse("params[gain]").unwrap(),
            Revision::new(7),
        )
        .unwrap();
        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("params[gain]").unwrap(),
            Revision::new(8),
            LpValue::F32(9.0),
        )
        .unwrap();
        ensure_slot_present(
            &mut root,
            &registry,
            &SlotPath::parse("params[gain]").unwrap(),
            Revision::new(9),
        )
        .unwrap();

        assert_eq!(root.params.keys_revision, Revision::new(7));
        assert_eq!(root.params.entries["gain"].value(), &9.0);
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
    fn slot_mutation_sets_option_some_default_then_sets_leaf() {
        let mut root = MutRoot {
            enabled: OptionSlot::none(),
            ..test_root()
        };
        let registry = registry();

        set_slot_option_some_default(
            &mut root,
            &registry,
            &SlotPath::parse("enabled").unwrap(),
            Revision::new(7),
        )
        .unwrap();
        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("enabled.some").unwrap(),
            Revision::new(8),
            LpValue::Bool(false),
        )
        .unwrap();

        assert_eq!(root.enabled.presence_revision, Revision::new(7));
        assert_eq!(root.enabled.data.as_ref().unwrap().value(), &false);
    }

    #[test]
    fn ensure_slot_present_sets_option_some_default() {
        let mut root = MutRoot {
            enabled: OptionSlot::none(),
            ..test_root()
        };
        let registry = registry();

        ensure_slot_present(
            &mut root,
            &registry,
            &SlotPath::parse("enabled.some").unwrap(),
            Revision::new(7),
        )
        .unwrap();

        assert_eq!(root.enabled.presence_revision, Revision::new(7));
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

        let TestEnum::A { value } = root.mode.value() else {
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

        let TestEnum::B { other } = root.mode.value() else {
            panic!("expected b");
        };
        assert_eq!(root.mode.variant_revision(), Revision::new(9));
        assert_eq!(other.value(), &5.0);
    }

    #[test]
    fn ensure_slot_present_selects_enum_variant() {
        let mut root = test_root();
        let registry = registry();

        ensure_slot_present(
            &mut root,
            &registry,
            &SlotPath::parse("mode.b").unwrap(),
            Revision::new(9),
        )
        .unwrap();

        let TestEnum::B { other } = root.mode.value() else {
            panic!("expected b");
        };
        assert_eq!(root.mode.variant_revision(), Revision::new(9));
        assert_eq!(other.value(), &0.0);
    }

    #[test]
    fn slot_mutation_switches_dynamic_enum_to_default_variant() {
        use crate::{DynamicSlotObject, SlotData, SlotEnum, SlotMeta};

        let shape_id = SlotShapeId::from_static_name("test.dynamic_enum_mutation");
        let shape = SlotShape::Enum {
            meta: SlotMeta::empty(),
            encoding: crate::SlotEnumEncoding::default(),
            variants: vec![
                crate::slot::shape::variant("a", crate::slot::shape::record(vec![])),
                crate::slot::shape::variant(
                    "b",
                    crate::slot::shape::record(vec![crate::slot::shape::field(
                        "other",
                        crate::slot::shape::value(crate::LpType::F32),
                    )]),
                ),
            ],
        };
        let mut registry = SlotShapeRegistry::default();
        registry.register_dynamic_shape(shape_id, shape).unwrap();
        let mut root = DynamicSlotObject::new(
            shape_id,
            SlotData::Enum(SlotEnum::with_version(
                Revision::new(1),
                crate::SlotName::parse("a").unwrap(),
                SlotData::Record(crate::SlotRecord::new(vec![])),
            )),
        );

        set_slot_variant_default(
            &mut root,
            &registry,
            &SlotPath::root(),
            Revision::new(9),
            "b",
        )
        .unwrap();
        set_slot_value(
            &mut root,
            &registry,
            &SlotPath::parse("other").unwrap(),
            Revision::new(10),
            LpValue::F32(12.0),
        )
        .unwrap();

        let SlotData::Enum(en) = root.data_ref() else {
            panic!("expected enum");
        };
        assert_eq!(en.variant.as_str(), "b");
    }

    fn test_root() -> MutRoot {
        MutRoot {
            gain: ValueSlot::new(1.0),
            params: MapSlot::new(BTreeMap::from([(
                String::from("exposure"),
                ValueSlot::new(1.0),
            )])),
            enabled: OptionSlot::some(ValueSlot::new(true)),
            payload: ValueSlot::new(LpValue::Bool(false)),
            mode: EnumSlot::new(TestEnum::a()),
        }
    }

    fn registry() -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        registry
            .ensure_shape_named(
                MutRoot::SHAPE_ID,
                MutRoot::shape_name().expect("shape name"),
                MutRoot::slot_shape(),
            )
            .unwrap();
        registry
    }
}
