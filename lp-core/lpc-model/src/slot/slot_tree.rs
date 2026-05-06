use crate::{ModelStructMember, ModelType, ModelValue, SlotName, SlotPath};
use alloc::string::{String, ToString};
use core::fmt;

use super::{
    SlotData, SlotMapKey, SlotMapKeyShape, SlotRegistry, SlotShape, SlotShapeId, SlotVariantShape,
};

/// Runtime slot data rooted at one registered shape.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotTree {
    pub shape: SlotShapeId,
    pub root: SlotData,
}

impl SlotTree {
    pub fn new(shape: SlotShapeId, root: SlotData) -> Self {
        Self { shape, root }
    }

    /// Get data at `path`, interpreting indexed records through `registry`.
    pub fn get<'a>(&'a self, registry: &'a SlotRegistry, path: &SlotPath) -> Option<&'a SlotData> {
        let shape = registry.get(&self.shape)?;
        get_data(&self.root, shape, registry, path.segments())
    }

    /// Validate this tree against its registered shape.
    pub fn validate(&self, registry: &SlotRegistry) -> Result<(), SlotValidationError> {
        let shape = registry
            .get(&self.shape)
            .ok_or_else(|| SlotValidationError::UnknownShape(self.shape.clone()))?;
        validate_data(&self.root, shape, registry)
    }
}

/// Error returned when slot data does not match its registered shape.
#[derive(Clone, Debug, PartialEq)]
pub enum SlotValidationError {
    UnknownShape(SlotShapeId),
    KindMismatch {
        expected: SlotShapeKind,
        actual: SlotDataKind,
    },
    ModelTypeMismatch {
        expected: ModelType,
        actual: ModelValue,
    },
    RecordFieldCount {
        expected: usize,
        actual: usize,
    },
    MapKeyMismatch {
        expected: SlotMapKeyShape,
        actual: SlotMapKey,
    },
    UnknownEnumVariant(SlotName),
}

impl fmt::Display for SlotValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShape(id) => write!(f, "unknown slot shape id: {id}"),
            Self::KindMismatch { expected, actual } => {
                write!(
                    f,
                    "slot kind mismatch: expected {expected:?}, got {actual:?}"
                )
            }
            Self::ModelTypeMismatch { expected, actual } => {
                write!(
                    f,
                    "model type mismatch: expected {expected:?}, got {actual:?}"
                )
            }
            Self::RecordFieldCount { expected, actual } => {
                write!(
                    f,
                    "record field count mismatch: expected {expected}, got {actual}"
                )
            }
            Self::MapKeyMismatch { expected, actual } => {
                write!(f, "map key mismatch: expected {expected:?}, got {actual:?}")
            }
            Self::UnknownEnumVariant(name) => write!(f, "unknown enum variant: {name}"),
        }
    }
}

impl core::error::Error for SlotValidationError {}

/// Kind of slot shape, used in validation diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotShapeKind {
    Ref,
    Unit,
    Value,
    Record,
    Map,
    Enum,
    Option,
}

/// Kind of slot data, used in validation diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotDataKind {
    Unit,
    Value,
    Record,
    Map,
    Enum,
    Option,
}

fn get_data<'a>(
    data: &'a SlotData,
    shape: &'a SlotShape,
    registry: &'a SlotRegistry,
    segments: &[SlotName],
) -> Option<&'a SlotData> {
    if let SlotShape::Ref { id } = shape {
        return get_data(data, registry.get(id)?, registry, segments);
    }

    let Some((head, tail)) = segments.split_first() else {
        return Some(data);
    };

    match (data, shape) {
        (SlotData::Record(record), SlotShape::Record { fields, .. }) => {
            let index = fields.iter().position(|field| field.name == *head)?;
            let field_data = record.fields.get(index)?;
            let field_shape = &fields.get(index)?.shape;
            get_data(field_data, field_shape, registry, tail)
        }
        (SlotData::Map(map), SlotShape::Map { key, value, .. }) => {
            if *key != SlotMapKeyShape::String {
                return None;
            }
            let data = map
                .entries
                .get(&SlotMapKey::String(head.as_str().to_string()))?;
            get_data(data, value, registry, tail)
        }
        (SlotData::Enum(active), SlotShape::Enum { variants, .. }) => {
            if active.variant != *head {
                return None;
            }
            let variant = find_variant(variants, head)?;
            get_data(&active.data, &variant.shape, registry, tail)
        }
        (SlotData::Option(option), SlotShape::Option { some, .. }) if option.data.is_some() => {
            if head.as_str() != "some" {
                return None;
            }
            get_data(
                option.data.as_deref().expect("checked some"),
                some,
                registry,
                tail,
            )
        }
        _ => None,
    }
}

fn validate_data(
    data: &SlotData,
    shape: &SlotShape,
    registry: &SlotRegistry,
) -> Result<(), SlotValidationError> {
    if let SlotShape::Ref { id } = shape {
        let shape = registry
            .get(id)
            .ok_or(SlotValidationError::UnknownShape(*id))?;
        return validate_data(data, shape, registry);
    }

    match (data, shape) {
        (SlotData::Unit { .. }, SlotShape::Unit { .. }) => Ok(()),
        (SlotData::Value(value), SlotShape::Value { shape }) => {
            validate_model_value(value.value(), &shape.ty)
        }
        (SlotData::Record(record), SlotShape::Record { fields, .. }) => {
            if record.fields.len() != fields.len() {
                return Err(SlotValidationError::RecordFieldCount {
                    expected: fields.len(),
                    actual: record.fields.len(),
                });
            }
            for (data, field) in record.fields.iter().zip(fields) {
                validate_data(data, &field.shape, registry)?;
            }
            Ok(())
        }
        (SlotData::Map(map), SlotShape::Map { key, value, .. }) => {
            for (map_key, data) in &map.entries {
                validate_map_key(map_key, *key)?;
                validate_data(data, value, registry)?;
            }
            Ok(())
        }
        (SlotData::Enum(active), SlotShape::Enum { variants, .. }) => {
            let variant = find_variant(variants, &active.variant)
                .ok_or_else(|| SlotValidationError::UnknownEnumVariant(active.variant.clone()))?;
            validate_data(&active.data, &variant.shape, registry)
        }
        (SlotData::Option(option), SlotShape::Option { some, .. }) => {
            if let Some(data) = option.data.as_deref() {
                validate_data(data, some, registry)
            } else {
                Ok(())
            }
        }
        _ => Err(SlotValidationError::KindMismatch {
            expected: shape.kind(),
            actual: data.kind(),
        }),
    }
}

fn validate_map_key(
    actual: &SlotMapKey,
    expected: SlotMapKeyShape,
) -> Result<(), SlotValidationError> {
    let matches = matches!(
        (actual, expected),
        (SlotMapKey::String(_), SlotMapKeyShape::String)
            | (SlotMapKey::I32(_), SlotMapKeyShape::I32)
            | (SlotMapKey::U32(_), SlotMapKeyShape::U32)
    );
    if matches {
        Ok(())
    } else {
        Err(SlotValidationError::MapKeyMismatch {
            expected,
            actual: actual.clone(),
        })
    }
}

fn validate_model_value(value: &ModelValue, ty: &ModelType) -> Result<(), SlotValidationError> {
    let matches = match (value, ty) {
        (ModelValue::String(_), ModelType::String)
        | (ModelValue::I32(_), ModelType::I32)
        | (ModelValue::U32(_), ModelType::U32)
        | (ModelValue::F32(_), ModelType::F32)
        | (ModelValue::Bool(_), ModelType::Bool)
        | (ModelValue::Vec2(_), ModelType::Vec2)
        | (ModelValue::Vec3(_), ModelType::Vec3)
        | (ModelValue::Vec4(_), ModelType::Vec4)
        | (ModelValue::IVec2(_), ModelType::IVec2)
        | (ModelValue::IVec3(_), ModelType::IVec3)
        | (ModelValue::IVec4(_), ModelType::IVec4)
        | (ModelValue::UVec2(_), ModelType::UVec2)
        | (ModelValue::UVec3(_), ModelType::UVec3)
        | (ModelValue::UVec4(_), ModelType::UVec4)
        | (ModelValue::BVec2(_), ModelType::BVec2)
        | (ModelValue::BVec3(_), ModelType::BVec3)
        | (ModelValue::BVec4(_), ModelType::BVec4)
        | (ModelValue::Mat2x2(_), ModelType::Mat2x2)
        | (ModelValue::Mat3x3(_), ModelType::Mat3x3)
        | (ModelValue::Mat4x4(_), ModelType::Mat4x4)
        | (ModelValue::Resource(_), ModelType::Resource) => true,
        (ModelValue::Array(values), ModelType::Array(element_ty, len)) => {
            values.len() == *len
                && values
                    .iter()
                    .all(|value| validate_model_value(value, element_ty).is_ok())
        }
        (
            ModelValue::Struct {
                name: value_name,
                fields,
            },
            ModelType::Struct {
                name: type_name,
                fields: type_fields,
            },
        ) => {
            struct_names_match(value_name, type_name)
                && struct_fields_match(fields.as_slice(), type_fields.as_slice())
        }
        _ => false,
    };

    if matches {
        Ok(())
    } else {
        Err(SlotValidationError::ModelTypeMismatch {
            expected: ty.clone(),
            actual: value.clone(),
        })
    }
}

fn struct_names_match(value_name: &Option<String>, type_name: &Option<String>) -> bool {
    type_name.is_none() || value_name == type_name
}

fn struct_fields_match(fields: &[(String, ModelValue)], type_fields: &[ModelStructMember]) -> bool {
    fields.len() == type_fields.len()
        && fields
            .iter()
            .zip(type_fields)
            .all(|((name, value), member)| {
                name == &member.name && validate_model_value(value, &member.ty).is_ok()
            })
}

fn find_variant<'a>(
    variants: &'a [SlotVariantShape],
    name: &SlotName,
) -> Option<&'a SlotVariantShape> {
    variants.iter().find(|variant| variant.name == *name)
}

impl SlotShape {
    fn kind(&self) -> SlotShapeKind {
        match self {
            Self::Ref { .. } => SlotShapeKind::Ref,
            Self::Unit { .. } => SlotShapeKind::Unit,
            Self::Value { .. } => SlotShapeKind::Value,
            Self::Record { .. } => SlotShapeKind::Record,
            Self::Map { .. } => SlotShapeKind::Map,
            Self::Enum { .. } => SlotShapeKind::Enum,
            Self::Option { .. } => SlotShapeKind::Option,
        }
    }
}

impl SlotData {
    fn kind(&self) -> SlotDataKind {
        match self {
            Self::Unit { .. } => SlotDataKind::Unit,
            Self::Value(_) => SlotDataKind::Value,
            Self::Record(_) => SlotDataKind::Record,
            Self::Map(_) => SlotDataKind::Map,
            Self::Enum(_) => SlotDataKind::Enum,
            Self::Option(_) => SlotDataKind::Option,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        FrameId, SlotFieldShape, SlotMapDyn, SlotMeta, SlotOptionDyn, SlotRecord, SlotRegistry,
        SlotShapeId, Versioned,
    };
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use alloc::vec;

    #[test]
    fn validates_and_traverses_indexed_record_data() {
        let mut registry = SlotRegistry::new();
        let shape_id = SlotShapeId::parse("fixture.state").unwrap();
        registry
            .register(
                shape_id.clone(),
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new("size", SlotShape::value(ModelType::Vec2)).unwrap(),
                        SlotFieldShape::new("enabled", SlotShape::value(ModelType::Bool)).unwrap(),
                    ],
                },
            )
            .unwrap();

        let enabled = SlotData::Value(Versioned::new(FrameId::new(9), ModelValue::Bool(true)));
        let tree = SlotTree::new(
            shape_id,
            SlotData::Record(SlotRecord::new(vec![
                SlotData::Value(Versioned::new(
                    FrameId::new(8),
                    ModelValue::Vec2([10.0, 20.0]),
                )),
                enabled.clone(),
            ])),
        );

        tree.validate(&registry).unwrap();
        assert_eq!(
            tree.get(&registry, &SlotPath::parse("enabled").unwrap()),
            Some(&enabled)
        );
    }

    #[test]
    fn validates_string_keyed_maps() {
        let mut registry = SlotRegistry::new();
        let shape_id = SlotShapeId::parse("fixture.shapes").unwrap();
        registry
            .register(
                shape_id.clone(),
                SlotShape::Map {
                    meta: SlotMeta::empty(),
                    key: SlotMapKeyShape::String,
                    value: Box::new(SlotShape::value(ModelType::Vec4)),
                },
            )
            .unwrap();

        let value = SlotData::Value(Versioned::new(
            FrameId::new(1),
            ModelValue::Vec4([0.0, 1.0, 2.0, 3.0]),
        ));
        let mut entries = BTreeMap::new();
        entries.insert(SlotMapKey::String("dome".to_string()), value.clone());
        let tree = SlotTree::new(shape_id, SlotData::Map(SlotMapDyn::new(entries)));

        tree.validate(&registry).unwrap();
        assert_eq!(
            tree.get(&registry, &SlotPath::parse("dome").unwrap()),
            Some(&value)
        );
    }

    #[test]
    fn validation_rejects_value_type_mismatch() {
        let mut registry = SlotRegistry::new();
        let shape_id = SlotShapeId::parse("texture.config").unwrap();
        registry
            .register(shape_id.clone(), SlotShape::value(ModelType::Vec2))
            .unwrap();

        let tree = SlotTree::new(
            shape_id,
            SlotData::Value(Versioned::new(FrameId::new(1), ModelValue::Bool(true))),
        );

        assert!(matches!(
            tree.validate(&registry),
            Err(SlotValidationError::ModelTypeMismatch { .. })
        ));
    }

    #[test]
    fn validates_unit_shape_and_data() {
        let mut registry = SlotRegistry::new();
        let shape_id = SlotShapeId::parse("mapping.disabled").unwrap();
        registry
            .register(shape_id.clone(), SlotShape::unit())
            .unwrap();

        let tree = SlotTree::new(
            shape_id,
            SlotData::Unit {
                changed_frame: FrameId::new(2),
            },
        );

        tree.validate(&registry).unwrap();
        assert_eq!(
            tree.get(&registry, &SlotPath::root()),
            Some(&SlotData::Unit {
                changed_frame: FrameId::new(2),
            })
        );
    }

    #[test]
    fn validation_rejects_map_key_shape_mismatch() {
        let mut registry = SlotRegistry::new();
        let shape_id = SlotShapeId::parse("map").unwrap();
        registry
            .register(
                shape_id.clone(),
                SlotShape::Map {
                    meta: SlotMeta::empty(),
                    key: SlotMapKeyShape::U32,
                    value: Box::new(SlotShape::value(ModelType::Bool)),
                },
            )
            .unwrap();

        let mut entries = BTreeMap::new();
        entries.insert(
            SlotMapKey::String("bad".to_string()),
            SlotData::Value(Versioned::new(FrameId::new(1), ModelValue::Bool(true))),
        );

        let tree = SlotTree::new(shape_id, SlotData::Map(SlotMapDyn::new(entries)));
        assert!(matches!(
            tree.validate(&registry),
            Err(SlotValidationError::MapKeyMismatch { .. })
        ));
    }

    #[test]
    fn validates_enum_and_option_containers() {
        let mut registry = SlotRegistry::new();
        let shape_id = SlotShapeId::parse("fixture.mapping").unwrap();
        registry
            .register(
                shape_id.clone(),
                SlotShape::Enum {
                    meta: SlotMeta::empty(),
                    variants: vec![
                        SlotVariantShape {
                            name: SlotName::parse("shape").unwrap(),
                            shape: SlotShape::Option {
                                meta: SlotMeta::empty(),
                                some: Box::new(SlotShape::value(ModelType::Resource)),
                            },
                        },
                        SlotVariantShape {
                            name: SlotName::parse("disabled").unwrap(),
                            shape: SlotShape::unit(),
                        },
                    ],
                },
            )
            .unwrap();

        let tree = SlotTree::new(
            shape_id,
            SlotData::Enum(super::super::SlotEnum::new(
                SlotName::parse("shape").unwrap(),
                SlotData::Option(SlotOptionDyn::none()),
            )),
        );

        tree.validate(&registry).unwrap();

        let unit_tree = SlotTree::new(
            SlotShapeId::parse("fixture.mapping").unwrap(),
            SlotData::Enum(super::super::SlotEnum::new(
                SlotName::parse("disabled").unwrap(),
                SlotData::Unit {
                    changed_frame: FrameId::new(4),
                },
            )),
        );

        unit_tree.validate(&registry).unwrap();
    }
}
