use alloc::collections::BTreeMap;
use alloc::string::String;
use lpc_model::{
    FrameId, LpType, LpValue, ModelStructMember, SlotData, SlotMapKey, SlotMapKeyShape, SlotPath,
    SlotPathSegment, SlotShape, SlotShapeId, SlotShapeRegistry,
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
    value: &LpValue,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotMirrorError> {
    let (shape, data) = resolve_path(root, shape_id, path, registry)?;
    let SlotShape::Value { shape } = shape else {
        return Err(SlotMirrorError::NotAValue);
    };
    let SlotData::Value(_) = data else {
        return Err(SlotMirrorError::NotAValue);
    };
    if lp_value_matches_type(value, &shape.ty) {
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
            let SlotPathSegment::Field(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
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
            let SlotPathSegment::Key(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
            let key = map_key_for_shape(head, *key)?;
            let child = map
                .entries
                .get_mut(&key)
                .ok_or(SlotMirrorError::UnknownPath)?;
            apply_replace_shape(child, value, &tail, change, registry)
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(en)) => {
            let SlotPathSegment::Field(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
            let variant = variants
                .iter()
                .find(|variant| variant.name == *head)
                .ok_or(SlotMirrorError::UnknownPath)?;
            apply_replace_shape(&mut en.data, &variant.shape, &tail, change, registry)
        }
        (SlotShape::Option { some, .. }, SlotData::Option(option)) => {
            let SlotPathSegment::Field(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
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
            let SlotPathSegment::Field(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
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
            let SlotPathSegment::Key(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
            let key = map_key_for_shape(head, *key)?;
            let child = map.entries.get(&key).ok_or(SlotMirrorError::UnknownPath)?;
            resolve_path_shape(child, value, &tail, registry)
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(en)) => {
            let SlotPathSegment::Field(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
            let variant = variants
                .iter()
                .find(|variant| variant.name == *head)
                .ok_or(SlotMirrorError::UnknownPath)?;
            resolve_path_shape(&en.data, &variant.shape, &tail, registry)
        }
        (SlotShape::Option { some, .. }, SlotData::Option(option)) => {
            let SlotPathSegment::Field(head) = head else {
                return Err(SlotMirrorError::UnknownPath);
            };
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

fn map_key_for_shape(
    key: &SlotMapKey,
    shape: SlotMapKeyShape,
) -> Result<SlotMapKey, SlotMirrorError> {
    match (key, shape) {
        (SlotMapKey::String(value), SlotMapKeyShape::String) => {
            Ok(SlotMapKey::String(value.clone()))
        }
        (SlotMapKey::I32(value), SlotMapKeyShape::I32) => Ok(SlotMapKey::I32(*value)),
        (SlotMapKey::U32(value), SlotMapKeyShape::U32) => Ok(SlotMapKey::U32(*value)),
        (SlotMapKey::U32(value), SlotMapKeyShape::I32) => i32::try_from(*value)
            .map(SlotMapKey::I32)
            .map_err(|_| SlotMirrorError::UnknownPath),
        (SlotMapKey::I32(value), SlotMapKeyShape::U32) => u32::try_from(*value)
            .map(SlotMapKey::U32)
            .map_err(|_| SlotMirrorError::UnknownPath),
        _ => Err(SlotMirrorError::UnknownPath),
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
        (LpValue::Array(values), LpType::Array(item_ty, len)) => {
            values.len() == *len
                && values
                    .iter()
                    .all(|value| lp_value_matches_type(value, item_ty))
        }
        (LpValue::Array(values), LpType::List(item_ty)) => values
            .iter()
            .all(|value| lp_value_matches_type(value, item_ty)),
        (
            LpValue::Struct { name, fields },
            LpType::Struct {
                name: ty_name,
                fields: ty_fields,
            },
        ) => name == ty_name && struct_fields_match(fields, ty_fields),
        _ => false,
    }
}

fn struct_fields_match(fields: &[(String, LpValue)], ty_fields: &[ModelStructMember]) -> bool {
    fields.len() == ty_fields.len()
        && fields
            .iter()
            .zip(ty_fields.iter())
            .all(|((name, value), ty_field)| {
                name == &ty_field.name && lp_value_matches_type(value, &ty_field.ty)
            })
}
