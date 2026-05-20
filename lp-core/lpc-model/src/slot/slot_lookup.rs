//! Shape-aware lookup through borrowed slot data.

use crate::{
    MapSlotAccess, MapSlotAccessMut, SlotAccess, SlotAccessMut, SlotDataAccess, SlotDataAccessMut,
    SlotMapKey, SlotPath, SlotPathSegment, SlotShapeLookup, SlotShapeView,
};
use alloc::format;
use alloc::string::String;

/// Error returned while resolving a [`SlotPath`] against a slot object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotLookupError {
    pub message: String,
}

impl SlotLookupError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for SlotLookupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for SlotLookupError {}

/// Resolve `path` inside a slot object using registry shape metadata.
pub fn lookup_slot_data<'a>(
    root: &'a dyn SlotAccess,
    registry: &(impl SlotShapeLookup + ?Sized),
    path: &SlotPath,
) -> Result<SlotDataAccess<'a>, SlotLookupError> {
    let shape = registry.get_shape(root.shape_id()).ok_or_else(|| {
        SlotLookupError::new(format!("missing slot path root shape {}", root.shape_id()))
    })?;
    lookup_in_shape(root.data(), shape, registry, path.segments())
}

/// Resolve `path` inside a slot root and return both data and its concrete shape.
pub fn lookup_slot_data_and_shape<'a, 's>(
    root: &'a dyn SlotAccess,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    path: &SlotPath,
) -> Result<(SlotDataAccess<'a>, SlotShapeView<'s>), SlotLookupError> {
    let shape = registry.get_shape(root.shape_id()).ok_or_else(|| {
        SlotLookupError::new(format!("missing slot root shape {}", root.shape_id()))
    })?;
    lookup_in_shape_with_shape(root.data(), shape, registry, path.segments())
}

/// Resolve `path` inside a mutable slot root.
pub fn lookup_slot_data_mut<'a>(
    root: &'a mut dyn SlotAccessMut,
    registry: &(impl SlotShapeLookup + ?Sized),
    path: &SlotPath,
) -> Result<SlotDataAccessMut<'a>, SlotLookupError> {
    let shape = registry.get_shape(root.shape_id()).ok_or_else(|| {
        SlotLookupError::new(format!("missing slot root shape {}", root.shape_id()))
    })?;
    lookup_in_shape_mut(root.data_mut(), shape, registry, path.segments())
}

fn lookup_in_shape<'a>(
    data: SlotDataAccess<'a>,
    shape: SlotShapeView<'_>,
    registry: &(impl SlotShapeLookup + ?Sized),
    segments: &[SlotPathSegment],
) -> Result<SlotDataAccess<'a>, SlotLookupError> {
    let shape = resolve_shape_projection(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Ok(data);
    };

    match (data, head) {
        (SlotDataAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_field_by_name(name).is_some() =>
        {
            let (index, field) = shape
                .record_field_by_name(name)
                .expect("field checked above");
            let field_data = record
                .field(index)
                .ok_or_else(|| SlotLookupError::new(format!("record field {name} has no data")))?;
            lookup_in_shape(field_data, field.shape(), registry, tail)
        }
        (SlotDataAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            lookup_map_key(
                map,
                key,
                shape.map_value().expect("map value checked above"),
                registry,
                tail,
            )
        }
        (SlotDataAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data()
                .ok_or_else(|| SlotLookupError::new("option slot is none"))?;
            lookup_in_shape(
                data,
                shape.option_some().expect("option some checked above"),
                registry,
                tail,
            )
        }
        (SlotDataAccess::Enum(en), SlotPathSegment::Field(name))
            if shape.enum_variant_by_name(name).is_some() =>
        {
            if en.variant() != name.as_str() {
                return Err(SlotLookupError::new(format!(
                    "enum active variant is {}, not {name}",
                    en.variant()
                )));
            }
            let variant = shape
                .enum_variant_by_name(name)
                .expect("variant checked above");
            lookup_in_shape(en.data(), variant.shape(), registry, tail)
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotLookupError::new(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotLookupError::new(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn lookup_in_shape_with_shape<'a, 's>(
    data: SlotDataAccess<'a>,
    shape: SlotShapeView<'s>,
    registry: &'s (impl SlotShapeLookup + ?Sized),
    segments: &[SlotPathSegment],
) -> Result<(SlotDataAccess<'a>, SlotShapeView<'s>), SlotLookupError> {
    let shape = resolve_shape_projection(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Ok((data, shape));
    };

    match (data, head) {
        (SlotDataAccess::Record(record), SlotPathSegment::Field(name))
            if shape.record_field_by_name(name).is_some() =>
        {
            let (index, field) = shape
                .record_field_by_name(name)
                .expect("field checked above");
            let field_data = record
                .field(index)
                .ok_or_else(|| SlotLookupError::new(format!("record field {name} has no data")))?;
            lookup_in_shape_with_shape(field_data, field.shape(), registry, tail)
        }
        (SlotDataAccess::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            let data = map.get(key).ok_or_else(|| {
                SlotLookupError::new(format!("map has no key {}", display_key(key)))
            })?;
            lookup_in_shape_with_shape(
                data,
                shape.map_value().expect("map value checked above"),
                registry,
                tail,
            )
        }
        (SlotDataAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data()
                .ok_or_else(|| SlotLookupError::new("option slot is none"))?;
            lookup_in_shape_with_shape(
                data,
                shape.option_some().expect("option some checked above"),
                registry,
                tail,
            )
        }
        (SlotDataAccess::Enum(en), SlotPathSegment::Field(name))
            if shape.enum_variant_by_name(name).is_some() =>
        {
            if en.variant() != name.as_str() {
                return Err(SlotLookupError::new(format!(
                    "enum active variant is {}, not {name}",
                    en.variant()
                )));
            }
            let variant = shape
                .enum_variant_by_name(name)
                .expect("variant checked above");
            lookup_in_shape_with_shape(en.data(), variant.shape(), registry, tail)
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotLookupError::new(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotLookupError::new(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn lookup_map_key<'a>(
    map: &'a dyn MapSlotAccess,
    key: &SlotMapKey,
    value_shape: SlotShapeView<'_>,
    registry: &(impl SlotShapeLookup + ?Sized),
    tail: &[SlotPathSegment],
) -> Result<SlotDataAccess<'a>, SlotLookupError> {
    let data = map
        .get(key)
        .ok_or_else(|| SlotLookupError::new(format!("map has no key {}", display_key(key))))?;
    lookup_in_shape(data, value_shape, registry, tail)
}

fn lookup_in_shape_mut<'a>(
    data: SlotDataAccessMut<'a>,
    shape: SlotShapeView<'_>,
    registry: &(impl SlotShapeLookup + ?Sized),
    segments: &[SlotPathSegment],
) -> Result<SlotDataAccessMut<'a>, SlotLookupError> {
    let shape = resolve_shape_projection(shape, registry)?;
    let Some((head, tail)) = segments.split_first() else {
        return Ok(data);
    };

    match (data, head) {
        (SlotDataAccessMut::Record(record), SlotPathSegment::Field(name))
            if shape.record_field_by_name(name).is_some() =>
        {
            let (index, field) = shape
                .record_field_by_name(name)
                .expect("field checked above");
            let field_data = record.field_mut(index).ok_or_else(|| {
                SlotLookupError::new(format!("record field {name} has no mutable data"))
            })?;
            lookup_in_shape_mut(field_data, field.shape(), registry, tail)
        }
        (SlotDataAccessMut::Map(map), SlotPathSegment::Key(key)) if shape.map_value().is_some() => {
            lookup_map_key_mut(
                map,
                key,
                shape.map_value().expect("map value checked above"),
                registry,
                tail,
            )
        }
        (SlotDataAccessMut::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" && shape.option_some().is_some() =>
        {
            let data = option
                .data_mut()
                .ok_or_else(|| SlotLookupError::new("option slot is none"))?;
            lookup_in_shape_mut(
                data,
                shape.option_some().expect("option some checked above"),
                registry,
                tail,
            )
        }
        (SlotDataAccessMut::Enum(en), SlotPathSegment::Field(name))
            if shape.enum_variant_by_name(name).is_some() =>
        {
            if en.variant() != name.as_str() {
                return Err(SlotLookupError::new(format!(
                    "enum active variant is {}, not {name}",
                    en.variant()
                )));
            }
            let variant = shape
                .enum_variant_by_name(name)
                .expect("variant checked above");
            lookup_in_shape_mut(en.data_mut(), variant.shape(), registry, tail)
        }
        (_, SlotPathSegment::Field(name)) => Err(SlotLookupError::new(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, SlotPathSegment::Key(key)) => Err(SlotLookupError::new(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn lookup_map_key_mut<'a>(
    map: &'a mut dyn MapSlotAccessMut,
    key: &SlotMapKey,
    value_shape: SlotShapeView<'_>,
    registry: &(impl SlotShapeLookup + ?Sized),
    tail: &[SlotPathSegment],
) -> Result<SlotDataAccessMut<'a>, SlotLookupError> {
    let data = map
        .get_mut(key)
        .ok_or_else(|| SlotLookupError::new(format!("map has no key {}", display_key(key))))?;
    lookup_in_shape_mut(data, value_shape, registry, tail)
}

fn resolve_shape_projection<'a>(
    shape: SlotShapeView<'a>,
    registry: &'a (impl SlotShapeLookup + ?Sized),
) -> Result<SlotShapeView<'a>, SlotLookupError> {
    let mut shape = shape;
    loop {
        if let Some(id) = shape.ref_id() {
            shape = registry.get_shape(id).ok_or_else(|| {
                SlotLookupError::new(format!("missing referenced slot shape {id}"))
            })?;
        } else if let Some(projected) = shape.custom_shape() {
            shape = projected;
        } else {
            return Ok(shape);
        }
    }
}

fn display_key(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => format!("{value:?}"),
        SlotMapKey::I32(value) => format!("{value}"),
        SlotMapKey::U32(value) => format!("{value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SlotShapeRegistry, Slotted, StaticSlotShape, ValueSlot};

    #[derive(Slotted)]
    struct TestRoot {
        pub output: ValueSlot<f32>,
    }

    #[test]
    fn lookup_value_field_from_record_root() {
        let root = TestRoot {
            output: ValueSlot::new(0.5),
        };
        let mut registry = SlotShapeRegistry::default();
        TestRoot::ensure_registered(&mut registry).unwrap();

        let found =
            lookup_slot_data(&root, &registry, &SlotPath::parse("output").unwrap()).unwrap();

        let SlotDataAccess::Value(value) = found else {
            panic!("value");
        };
        assert_eq!(value.value(), crate::LpValue::F32(0.5));
    }
}
