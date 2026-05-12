//! Shape-aware lookup through borrowed slot data.

use crate::{
    MapSlotAccess, SlotAccess, SlotDataAccess, SlotMapKey, SlotPath, SlotPathSegment, SlotShape,
    SlotShapeRegistry,
};
use alloc::format;
use alloc::string::String;

/// Error returned while resolving a [`SlotPath`] against a slot root.
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

/// Resolve `path` inside a slot root using registry shape metadata.
pub fn lookup_slot_data<'a>(
    root: &'a dyn SlotAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
) -> Result<SlotDataAccess<'a>, SlotLookupError> {
    let shape = registry.get(&root.shape_id()).ok_or_else(|| {
        SlotLookupError::new(format!("missing slot root shape {}", root.shape_id()))
    })?;
    lookup_in_shape(root.data(), shape, registry, path.segments())
}

/// Resolve `path` inside a slot root and return both data and its concrete shape.
pub fn lookup_slot_data_and_shape<'a, 's>(
    root: &'a dyn SlotAccess,
    registry: &'s SlotShapeRegistry,
    path: &SlotPath,
) -> Result<(SlotDataAccess<'a>, &'s SlotShape), SlotLookupError> {
    let shape = registry.get(&root.shape_id()).ok_or_else(|| {
        SlotLookupError::new(format!("missing slot root shape {}", root.shape_id()))
    })?;
    lookup_in_shape_with_shape(root.data(), shape, registry, path.segments())
}

fn lookup_in_shape<'a>(
    data: SlotDataAccess<'a>,
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
) -> Result<SlotDataAccess<'a>, SlotLookupError> {
    let mut shape = shape;
    while let SlotShape::Ref { id } = shape {
        shape = registry
            .get(id)
            .ok_or_else(|| SlotLookupError::new(format!("missing referenced slot shape {id}")))?;
    }

    let Some((head, tail)) = segments.split_first() else {
        return Ok(data);
    };

    match (shape, data, head) {
        (
            SlotShape::Record { fields, .. },
            SlotDataAccess::Record(record),
            SlotPathSegment::Field(name),
        ) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *name)
                .ok_or_else(|| SlotLookupError::new(format!("record has no field {name}")))?;
            let field_data = record
                .field(index)
                .ok_or_else(|| SlotLookupError::new(format!("record field {name} has no data")))?;
            lookup_in_shape(field_data, &field.shape, registry, tail)
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map), SlotPathSegment::Key(key)) => {
            lookup_map_key(map, key, value, registry, tail)
        }
        (
            SlotShape::Option { some, .. },
            SlotDataAccess::Option(option),
            SlotPathSegment::Field(name),
        ) if name.as_str() == "some" => {
            let data = option
                .data()
                .ok_or_else(|| SlotLookupError::new("option slot is none"))?;
            lookup_in_shape(data, some, registry, tail)
        }
        (
            SlotShape::Enum { variants, .. },
            SlotDataAccess::Enum(en),
            SlotPathSegment::Field(name),
        ) => {
            if en.variant() != name.as_str() {
                return Err(SlotLookupError::new(format!(
                    "enum active variant is {}, not {name}",
                    en.variant()
                )));
            }
            let variant = variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or_else(|| SlotLookupError::new(format!("enum has no variant {name}")))?;
            lookup_in_shape(en.data(), &variant.shape, registry, tail)
        }
        (_, _, SlotPathSegment::Field(name)) => Err(SlotLookupError::new(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, _, SlotPathSegment::Key(key)) => Err(SlotLookupError::new(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn lookup_in_shape_with_shape<'a, 's>(
    data: SlotDataAccess<'a>,
    shape: &'s SlotShape,
    registry: &'s SlotShapeRegistry,
    segments: &[SlotPathSegment],
) -> Result<(SlotDataAccess<'a>, &'s SlotShape), SlotLookupError> {
    let mut shape = shape;
    while let SlotShape::Ref { id } = shape {
        shape = registry
            .get(id)
            .ok_or_else(|| SlotLookupError::new(format!("missing referenced slot shape {id}")))?;
    }

    let Some((head, tail)) = segments.split_first() else {
        return Ok((data, shape));
    };

    match (shape, data, head) {
        (
            SlotShape::Record { fields, .. },
            SlotDataAccess::Record(record),
            SlotPathSegment::Field(name),
        ) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *name)
                .ok_or_else(|| SlotLookupError::new(format!("record has no field {name}")))?;
            let field_data = record
                .field(index)
                .ok_or_else(|| SlotLookupError::new(format!("record field {name} has no data")))?;
            lookup_in_shape_with_shape(field_data, &field.shape, registry, tail)
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map), SlotPathSegment::Key(key)) => {
            let data = map.get(key).ok_or_else(|| {
                SlotLookupError::new(format!("map has no key {}", display_key(key)))
            })?;
            lookup_in_shape_with_shape(data, value, registry, tail)
        }
        (
            SlotShape::Option { some, .. },
            SlotDataAccess::Option(option),
            SlotPathSegment::Field(name),
        ) if name.as_str() == "some" => {
            let data = option
                .data()
                .ok_or_else(|| SlotLookupError::new("option slot is none"))?;
            lookup_in_shape_with_shape(data, some, registry, tail)
        }
        (
            SlotShape::Enum { variants, .. },
            SlotDataAccess::Enum(en),
            SlotPathSegment::Field(name),
        ) => {
            if en.variant() != name.as_str() {
                return Err(SlotLookupError::new(format!(
                    "enum active variant is {}, not {name}",
                    en.variant()
                )));
            }
            let variant = variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or_else(|| SlotLookupError::new(format!("enum has no variant {name}")))?;
            lookup_in_shape_with_shape(en.data(), &variant.shape, registry, tail)
        }
        (_, _, SlotPathSegment::Field(name)) => Err(SlotLookupError::new(format!(
            "slot path field {name} cannot descend into current slot shape"
        ))),
        (_, _, SlotPathSegment::Key(key)) => Err(SlotLookupError::new(format!(
            "slot path key {} cannot descend into current slot shape",
            display_key(key)
        ))),
    }
}

fn lookup_map_key<'a>(
    map: &'a dyn MapSlotAccess,
    key: &SlotMapKey,
    value_shape: &SlotShape,
    registry: &SlotShapeRegistry,
    tail: &[SlotPathSegment],
) -> Result<SlotDataAccess<'a>, SlotLookupError> {
    let data = map
        .get(key)
        .ok_or_else(|| SlotLookupError::new(format!("map has no key {}", display_key(key))))?;
    lookup_in_shape(data, value_shape, registry, tail)
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
    use crate::{SlotShapeRegistry, StaticSlotShape, ValueSlot};

    #[derive(lpc_slot_macros::SlotRecord)]
    #[slot(root)]
    struct TestRoot {
        output: ValueSlot<f32>,
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
