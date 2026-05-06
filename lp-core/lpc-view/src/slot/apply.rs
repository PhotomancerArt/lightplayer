use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use lpc_model::{
    FrameId, ModelStructMember, ModelType, ModelValue, SlotData, SlotMapKey, SlotMapKeyShape,
    SlotPath, SlotShape, SlotShapeId, SlotShapeRegistry,
};
use lpc_wire::{WireSlotChange, WireSlotPatch};

/// Error from applying or preparing slot mirror changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotMirrorError {
    UnknownRoot,
    UnknownPath,
    MissingShape(SlotShapeId),
    NotAValue,
    WrongType,
}

impl core::fmt::Display for SlotMirrorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownRoot => f.write_str("unknown slot root"),
            Self::UnknownPath => f.write_str("unknown slot path"),
            Self::MissingShape(id) => write!(f, "missing slot shape: {id}"),
            Self::NotAValue => f.write_str("slot path does not target a value"),
            Self::WrongType => f.write_str("slot value type does not match shape"),
        }
    }
}

impl core::error::Error for SlotMirrorError {}

pub(super) fn apply_patch(
    roots: &mut BTreeMap<String, SlotData>,
    root_shapes: &BTreeMap<String, SlotShapeId>,
    registry: &SlotShapeRegistry,
    patch: &WireSlotPatch,
) -> Result<(), SlotMirrorError> {
    let shape_id = root_shapes
        .get(&patch.root)
        .ok_or(SlotMirrorError::UnknownRoot)?;
    let data = roots
        .get_mut(&patch.root)
        .ok_or(SlotMirrorError::UnknownRoot)?;
    apply_replace(data, shape_id, &patch.path, &patch.change, registry)
}

pub(super) fn shape_version_for_root(
    root: &str,
    root_shapes: &BTreeMap<String, SlotShapeId>,
    registry: &SlotShapeRegistry,
) -> Result<FrameId, SlotMirrorError> {
    let shape_id = root_shapes.get(root).ok_or(SlotMirrorError::UnknownRoot)?;
    Ok(registry
        .entry(shape_id)
        .ok_or(SlotMirrorError::MissingShape(*shape_id))?
        .changed_frame)
}

pub(super) fn data_version_at(
    root: &SlotData,
    shape_id: &SlotShapeId,
    path: &SlotPath,
    registry: &SlotShapeRegistry,
) -> Result<FrameId, SlotMirrorError> {
    let (_, data) = resolve_path(root, shape_id, path, registry)?;
    match data {
        SlotData::Unit { changed_frame } => Ok(*changed_frame),
        SlotData::Value(value) => Ok(value.changed_frame()),
        SlotData::Record(record) => Ok(record.fields_changed_frame),
        SlotData::Map(map) => Ok(map.keys_changed_frame),
        SlotData::Enum(en) => Ok(en.variant_changed_frame),
        SlotData::Option(option) => Ok(option.presence_changed_frame),
    }
}

pub(super) fn validate_value_at(
    root: &SlotData,
    shape_id: &SlotShapeId,
    path: &SlotPath,
    value: &ModelValue,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotMirrorError> {
    let (shape, data) = resolve_path(root, shape_id, path, registry)?;
    let SlotShape::Value { ty, .. } = shape else {
        return Err(SlotMirrorError::NotAValue);
    };
    let SlotData::Value(_) = data else {
        return Err(SlotMirrorError::NotAValue);
    };
    if model_value_matches_type(value, ty) {
        Ok(())
    } else {
        Err(SlotMirrorError::WrongType)
    }
}

fn apply_replace(
    data: &mut SlotData,
    shape_id: &SlotShapeId,
    path: &SlotPath,
    change: &WireSlotChange,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotMirrorError> {
    let shape = registry
        .get(shape_id)
        .ok_or(SlotMirrorError::MissingShape(*shape_id))?;
    apply_replace_shape(data, shape, path, change, registry)
}

fn apply_replace_shape(
    data: &mut SlotData,
    shape: &SlotShape,
    path: &SlotPath,
    change: &WireSlotChange,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotMirrorError> {
    if let SlotShape::Ref { id } = shape {
        let shape = registry.get(id).ok_or(SlotMirrorError::MissingShape(*id))?;
        return apply_replace_shape(data, shape, path, change, registry);
    }

    if path.is_root() {
        match change {
            WireSlotChange::Replace(replacement) => *data = replacement.clone(),
        }
        return Ok(());
    }

    let (head, tail) = path
        .segments()
        .split_first()
        .ok_or(SlotMirrorError::UnknownPath)?;
    let tail = SlotPath::from_segments(tail.to_vec());
    match (shape, data) {
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *head)
                .ok_or(SlotMirrorError::UnknownPath)?;
            let child = record
                .fields
                .get_mut(index)
                .ok_or(SlotMirrorError::UnknownPath)?;
            apply_replace_shape(child, &field.shape, &tail, change, registry)
        }
        (SlotShape::Map { key, value, .. }, SlotData::Map(map)) => {
            let key = parse_map_key(head.as_str(), *key)?;
            let child = map
                .entries
                .get_mut(&key)
                .ok_or(SlotMirrorError::UnknownPath)?;
            apply_replace_shape(child, value, &tail, change, registry)
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name == *head)
                .ok_or(SlotMirrorError::UnknownPath)?;
            apply_replace_shape(&mut en.data, &variant.shape, &tail, change, registry)
        }
        (SlotShape::Option { some, .. }, SlotData::Option(option)) => {
            if head.as_str() != "some" {
                return Err(SlotMirrorError::UnknownPath);
            }
            let child = option.data.as_mut().ok_or(SlotMirrorError::UnknownPath)?;
            apply_replace_shape(child, some, &tail, change, registry)
        }
        (SlotShape::Unit { .. }, SlotData::Unit { .. }) => Err(SlotMirrorError::UnknownPath),
        (SlotShape::Value { .. }, SlotData::Value(_)) => Err(SlotMirrorError::UnknownPath),
        _ => Err(SlotMirrorError::UnknownPath),
    }
}

fn resolve_path<'a>(
    data: &'a SlotData,
    shape_id: &SlotShapeId,
    path: &SlotPath,
    registry: &'a SlotShapeRegistry,
) -> Result<(&'a SlotShape, &'a SlotData), SlotMirrorError> {
    let shape = registry
        .get(shape_id)
        .ok_or(SlotMirrorError::MissingShape(*shape_id))?;
    resolve_path_shape(data, shape, path, registry)
}

fn resolve_path_shape<'a>(
    data: &'a SlotData,
    shape: &'a SlotShape,
    path: &SlotPath,
    registry: &'a SlotShapeRegistry,
) -> Result<(&'a SlotShape, &'a SlotData), SlotMirrorError> {
    if let SlotShape::Ref { id } = shape {
        let shape = registry.get(id).ok_or(SlotMirrorError::MissingShape(*id))?;
        return resolve_path_shape(data, shape, path, registry);
    }

    if path.is_root() {
        return Ok((shape, data));
    }

    let (head, tail) = path
        .segments()
        .split_first()
        .ok_or(SlotMirrorError::UnknownPath)?;
    let tail = SlotPath::from_segments(tail.to_vec());
    match (shape, data) {
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *head)
                .ok_or(SlotMirrorError::UnknownPath)?;
            let child = record
                .fields
                .get(index)
                .ok_or(SlotMirrorError::UnknownPath)?;
            resolve_path_shape(child, &field.shape, &tail, registry)
        }
        (SlotShape::Map { key, value, .. }, SlotData::Map(map)) => {
            let key = parse_map_key(head.as_str(), *key)?;
            let child = map.entries.get(&key).ok_or(SlotMirrorError::UnknownPath)?;
            resolve_path_shape(child, value, &tail, registry)
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name == *head)
                .ok_or(SlotMirrorError::UnknownPath)?;
            resolve_path_shape(&en.data, &variant.shape, &tail, registry)
        }
        (SlotShape::Option { some, .. }, SlotData::Option(option)) => {
            if head.as_str() != "some" {
                return Err(SlotMirrorError::UnknownPath);
            }
            let child = option.data.as_ref().ok_or(SlotMirrorError::UnknownPath)?;
            resolve_path_shape(child, some, &tail, registry)
        }
        (SlotShape::Unit { .. }, SlotData::Unit { .. }) => Err(SlotMirrorError::UnknownPath),
        (SlotShape::Value { .. }, SlotData::Value(_)) => Err(SlotMirrorError::UnknownPath),
        _ => Err(SlotMirrorError::UnknownPath),
    }
}

fn parse_map_key(value: &str, shape: SlotMapKeyShape) -> Result<SlotMapKey, SlotMirrorError> {
    match shape {
        SlotMapKeyShape::String => Ok(SlotMapKey::String(value.to_string())),
        SlotMapKeyShape::I32 => value
            .parse()
            .map(SlotMapKey::I32)
            .map_err(|_| SlotMirrorError::UnknownPath),
        SlotMapKeyShape::U32 => value
            .parse()
            .map(SlotMapKey::U32)
            .map_err(|_| SlotMirrorError::UnknownPath),
    }
}

fn model_value_matches_type(value: &ModelValue, ty: &ModelType) -> bool {
    match (value, ty) {
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
        (ModelValue::Array(values), ModelType::Array(item_ty, len)) => {
            values.len() == *len
                && values
                    .iter()
                    .all(|value| model_value_matches_type(value, item_ty))
        }
        (
            ModelValue::Struct { name, fields },
            ModelType::Struct {
                name: ty_name,
                fields: ty_fields,
            },
        ) => name == ty_name && struct_fields_match(fields, ty_fields),
        _ => false,
    }
}

fn struct_fields_match(fields: &[(String, ModelValue)], ty_fields: &[ModelStructMember]) -> bool {
    fields.len() == ty_fields.len()
        && fields
            .iter()
            .zip(ty_fields.iter())
            .all(|((name, value), ty_field)| {
                name == &ty_field.name && model_value_matches_type(value, &ty_field.ty)
            })
}
