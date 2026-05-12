//! Dynamic runtime state root for one compute shader node.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    ComputeShaderDef, LpType, Revision, ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind,
    ShaderValueShapeRef, SlotAccess, SlotData, SlotDataAccess, SlotFieldShape, SlotMapKeyShape,
    SlotName, SlotRecordAccess, SlotShape, SlotShapeId, SlotShapeRegistry, SlotShapeRegistryError,
    WithRevision,
};

/// Runtime-produced slot data for one compute shader node.
///
/// Unlike Rust-authored state structs, compute state is shaped by the shader
/// artifact. The root is still a normal slot root: the registry stores its
/// shape, and `fields` stores values in the same order.
pub struct ComputeShaderState {
    shape_id: SlotShapeId,
    shape_name: String,
    fields: Vec<ComputeShaderStateField>,
}

impl ComputeShaderState {
    pub fn new(node_id: lpc_model::NodeId, def: &ComputeShaderDef, revision: Revision) -> Self {
        let shape_name = format!("runtime.compute.{}.state", node_id.as_u32());
        let shape_id = SlotShapeId::from_static_name(&shape_name);
        let fields = def
            .produced_slots
            .entries
            .iter()
            .map(|(name, slot)| ComputeShaderStateField {
                name: name.clone(),
                slot: slot.clone(),
                data: empty_data_for_slot(slot, revision),
            })
            .collect();
        Self {
            shape_id,
            shape_name,
            fields,
        }
    }

    pub fn shape_id(&self) -> SlotShapeId {
        self.shape_id
    }

    pub fn slot_defs(&self) -> impl Iterator<Item = (&str, &ShaderSlotDef)> {
        self.fields
            .iter()
            .map(|field| (field.name.as_str(), &field.slot))
    }

    pub fn set_slot_data(&mut self, name: &str, data: SlotData) -> Result<(), ComputeStateError> {
        let field = self
            .fields
            .iter_mut()
            .find(|field| field.name == name)
            .ok_or_else(|| ComputeStateError::UnknownProducedSlot(String::from(name)))?;
        field.data = data;
        Ok(())
    }

    pub fn register_shape(
        &self,
        registry: &mut SlotShapeRegistry,
    ) -> Result<(), ComputeStateError> {
        let mut fields = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            fields.push(SlotFieldShape {
                name: SlotName::parse(&field.name)
                    .map_err(|e| ComputeStateError::InvalidSlotName(field.name.clone(), e))?,
                shape: shape_for_shader_slot(&field.slot, registry)?,
            });
        }
        registry.replace_root_named(
            self.shape_id,
            self.shape_name.clone(),
            SlotShape::Record {
                meta: lpc_model::SlotMeta::empty(),
                fields,
            },
        );
        Ok(())
    }
}

impl SlotAccess for ComputeShaderState {
    fn shape_id(&self) -> SlotShapeId {
        self.shape_id
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for ComputeShaderState {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        self.fields.get(index).map(|field| field.data.access())
    }
}

struct ComputeShaderStateField {
    name: String,
    slot: ShaderSlotDef,
    data: SlotData,
}

/// Failure building or updating compute runtime state.
#[derive(Debug)]
pub enum ComputeStateError {
    InvalidSlotName(String, lpc_model::SlotNameError),
    Shape(SlotShapeRegistryError),
    Unsupported(String),
    UnknownNativeShape(String),
    NativeShapeIsNotValue(String),
    UnknownProducedSlot(String),
}

impl core::fmt::Display for ComputeStateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSlotName(name, err) => write!(f, "invalid compute slot {name:?}: {err}"),
            Self::Shape(err) => write!(f, "{err}"),
            Self::Unsupported(message) => f.write_str(message),
            Self::UnknownNativeShape(name) => write!(f, "unknown native shape {name:?}"),
            Self::NativeShapeIsNotValue(name) => {
                write!(f, "native shape {name:?} is not a slot value shape")
            }
            Self::UnknownProducedSlot(name) => write!(f, "unknown produced slot {name:?}"),
        }
    }
}

impl core::error::Error for ComputeStateError {}

impl From<SlotShapeRegistryError> for ComputeStateError {
    fn from(value: SlotShapeRegistryError) -> Self {
        Self::Shape(value)
    }
}

pub fn shape_for_shader_slot(
    slot: &ShaderSlotDef,
    registry: &SlotShapeRegistry,
) -> Result<SlotShape, ComputeStateError> {
    match slot.kind.value() {
        ShaderSlotKind::Value => value_shape_for_ref(slot.value.value(), registry),
        ShaderSlotKind::Map => {
            let key = slot.key.data.as_ref().ok_or_else(|| {
                ComputeStateError::Unsupported(String::from("map slot missing key"))
            })?;
            let key_shape = match key.value() {
                ShaderMapKeyDef::U32 => SlotMapKeyShape::U32,
            };
            Ok(SlotShape::Map {
                meta: lpc_model::SlotMeta::empty(),
                key: key_shape,
                value: alloc::boxed::Box::new(value_shape_for_ref(slot.value.value(), registry)?),
            })
        }
    }
}

fn value_shape_for_ref(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
) -> Result<SlotShape, ComputeStateError> {
    if let Some(ty) = value_ref.as_lp_type() {
        return Ok(SlotShape::value(ty));
    }

    let (id, entry) = registry
        .entry_by_name(value_ref.as_str())
        .ok_or_else(|| ComputeStateError::UnknownNativeShape(value_ref.as_str().to_string()))?;
    match entry.value() {
        SlotShape::Value { .. } => Ok(SlotShape::reference(id)),
        _ => Err(ComputeStateError::NativeShapeIsNotValue(
            value_ref.as_str().to_string(),
        )),
    }
}

fn empty_data_for_slot(slot: &ShaderSlotDef, revision: Revision) -> SlotData {
    match slot.kind.value() {
        ShaderSlotKind::Value => SlotData::Value(WithRevision::new(
            revision,
            default_lp_value_for_ref(slot.value.value()),
        )),
        ShaderSlotKind::Map => SlotData::Map(lpc_model::SlotMapDyn::with_revision(
            revision,
            BTreeMap::new(),
        )),
    }
}

fn default_lp_value_for_ref(value_ref: &ShaderValueShapeRef) -> lpc_model::LpValue {
    match value_ref.as_lp_type().unwrap_or(LpType::F32) {
        LpType::I32 => lpc_model::LpValue::I32(0),
        LpType::U32 => lpc_model::LpValue::U32(0),
        LpType::F32 => lpc_model::LpValue::F32(0.0),
        LpType::Bool => lpc_model::LpValue::Bool(false),
        LpType::Vec2 => lpc_model::LpValue::Vec2([0.0, 0.0]),
        LpType::Vec3 => lpc_model::LpValue::Vec3([0.0, 0.0, 0.0]),
        LpType::Vec4 => lpc_model::LpValue::Vec4([0.0, 0.0, 0.0, 0.0]),
        _ => lpc_model::LpValue::F32(0.0),
    }
}
