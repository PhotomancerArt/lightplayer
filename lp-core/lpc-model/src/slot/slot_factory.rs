//! Default object factories for slot shapes.

use crate::{
    ControlProduct, LpType, LpValue, ModelStructMember, ProductKind, ProductRef, ResourceRef,
    SlotAccess, SlotData, SlotMapDyn, SlotMutAccess, SlotOptionDyn, SlotRecord, SlotShape,
    SlotShapeId, SlotShapeRegistry, SlotShapeRegistryError, VisualProduct, WithRevision,
    current_revision,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub type SlotFactoryFn =
    fn(&SlotShapeRegistry, SlotShapeId) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;

#[derive(Clone, Copy)]
pub enum SlotFactory {
    Static(SlotFactoryFn),
    Dynamic,
    Unsupported,
}

impl SlotFactory {
    pub const fn for_default<T>() -> Self
    where
        T: SlotMutAccess + Default + 'static,
    {
        Self::Static(create_default_static::<T>)
    }

    pub const fn dynamic() -> Self {
        Self::Dynamic
    }

    pub const fn unsupported() -> Self {
        Self::Unsupported
    }

    pub fn create_default(
        self,
        registry: &SlotShapeRegistry,
        id: SlotShapeId,
    ) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError> {
        match self {
            Self::Static(create) => create(registry, id),
            Self::Dynamic => create_dynamic_default(registry, id),
            Self::Unsupported => Err(SlotFactoryError::UnsupportedFactory(id)),
        }
    }
}

impl core::fmt::Debug for SlotFactory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Static(_) => f.write_str("SlotFactory::Static(..)"),
            Self::Dynamic => f.write_str("SlotFactory::Dynamic"),
            Self::Unsupported => f.write_str("SlotFactory::Unsupported"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotFactoryError {
    MissingShape(SlotShapeId),
    MissingReferencedShape(SlotShapeId),
    UnsupportedFactory(SlotShapeId),
    EmptyEnum(SlotShapeId),
    InvalidShape { message: String },
}

impl core::fmt::Display for SlotFactoryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingShape(id) => write!(f, "missing slot shape: {id}"),
            Self::MissingReferencedShape(id) => write!(f, "missing referenced slot shape: {id}"),
            Self::UnsupportedFactory(id) => write!(f, "slot shape is not creatable: {id}"),
            Self::EmptyEnum(id) => write!(f, "slot enum shape has no variants: {id}"),
            Self::InvalidShape { message } => f.write_str(message),
        }
    }
}

impl core::error::Error for SlotFactoryError {}

impl From<SlotFactoryError> for SlotShapeRegistryError {
    fn from(error: SlotFactoryError) -> Self {
        match error {
            SlotFactoryError::MissingShape(id) => Self::MissingReferencedShape(id),
            SlotFactoryError::MissingReferencedShape(id) => Self::MissingReferencedShape(id),
            other => Self::FactoryError(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynamicSlotObject {
    shape_id: SlotShapeId,
    data: SlotData,
}

impl DynamicSlotObject {
    pub fn new(shape_id: SlotShapeId, data: SlotData) -> Self {
        Self { shape_id, data }
    }

    pub fn data_ref(&self) -> &SlotData {
        &self.data
    }

    pub fn data_mut_ref(&mut self) -> &mut SlotData {
        &mut self.data
    }

    pub fn into_data(self) -> SlotData {
        self.data
    }
}

impl SlotAccess for DynamicSlotObject {
    fn shape_id(&self) -> SlotShapeId {
        self.shape_id
    }

    fn data(&self) -> crate::SlotDataAccess<'_> {
        self.data.access()
    }
}

impl SlotMutAccess for DynamicSlotObject {
    fn data_mut(&mut self) -> crate::SlotDataMutAccess<'_> {
        self.data.access_mut()
    }
}

pub fn create_dynamic_slot_data(
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
) -> Result<SlotData, SlotFactoryError> {
    create_dynamic_slot_data_for_root(registry, SlotShapeId::new(0), shape)
}

fn create_default_static<T>(
    _registry: &SlotShapeRegistry,
    _id: SlotShapeId,
) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>
where
    T: SlotMutAccess + Default + 'static,
{
    Ok(Box::new(T::default()))
}

fn create_dynamic_default(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError> {
    let shape = registry
        .get(&id)
        .ok_or(SlotFactoryError::MissingShape(id))?;
    let data = create_dynamic_slot_data_for_root(registry, id, shape)?;
    Ok(Box::new(DynamicSlotObject::new(id, data)))
}

fn create_dynamic_slot_data_for_root(
    registry: &SlotShapeRegistry,
    root_id: SlotShapeId,
    shape: &SlotShape,
) -> Result<SlotData, SlotFactoryError> {
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get(id)
                .ok_or(SlotFactoryError::MissingReferencedShape(*id))?;
            create_dynamic_slot_data_for_root(registry, *id, shape)
        }
        SlotShape::Unit { .. } => Ok(SlotData::Unit {
            revision: current_revision(),
        }),
        SlotShape::Value { shape } => Ok(SlotData::Value(WithRevision::new(
            current_revision(),
            default_lp_value(&shape.ty),
        ))),
        SlotShape::Record { fields, .. } => {
            let mut data = Vec::with_capacity(fields.len());
            for field in fields {
                data.push(create_dynamic_slot_data_for_root(
                    registry,
                    root_id,
                    &field.shape,
                )?);
            }
            Ok(SlotData::Record(SlotRecord::with_revision(
                current_revision(),
                data,
            )))
        }
        SlotShape::Map { .. } => Ok(SlotData::Map(SlotMapDyn::with_revision(
            current_revision(),
            BTreeMap::new(),
        ))),
        SlotShape::Enum { variants, .. } => {
            let variant = variants
                .first()
                .ok_or(SlotFactoryError::EmptyEnum(root_id))?;
            let data = create_dynamic_slot_data_for_root(registry, root_id, &variant.shape)?;
            let name = variant.name.clone();
            Ok(SlotData::Enum(crate::SlotEnum::with_version(
                current_revision(),
                name,
                data,
            )))
        }
        SlotShape::Option { .. } => Ok(SlotData::Option(SlotOptionDyn::none_with_version(
            current_revision(),
        ))),
    }
}

fn default_lp_value(ty: &LpType) -> LpValue {
    match ty {
        LpType::String => LpValue::String(String::new()),
        LpType::I32 => LpValue::I32(0),
        LpType::U32 => LpValue::U32(0),
        LpType::F32 => LpValue::F32(0.0),
        LpType::Bool => LpValue::Bool(false),
        LpType::Vec2 => LpValue::Vec2([0.0; 2]),
        LpType::Vec3 => LpValue::Vec3([0.0; 3]),
        LpType::Vec4 => LpValue::Vec4([0.0; 4]),
        LpType::IVec2 => LpValue::IVec2([0; 2]),
        LpType::IVec3 => LpValue::IVec3([0; 3]),
        LpType::IVec4 => LpValue::IVec4([0; 4]),
        LpType::UVec2 => LpValue::UVec2([0; 2]),
        LpType::UVec3 => LpValue::UVec3([0; 3]),
        LpType::UVec4 => LpValue::UVec4([0; 4]),
        LpType::BVec2 => LpValue::BVec2([false; 2]),
        LpType::BVec3 => LpValue::BVec3([false; 3]),
        LpType::BVec4 => LpValue::BVec4([false; 4]),
        LpType::Mat2x2 => LpValue::Mat2x2([[0.0; 2]; 2]),
        LpType::Mat3x3 => LpValue::Mat3x3([[0.0; 3]; 3]),
        LpType::Mat4x4 => LpValue::Mat4x4([[0.0; 4]; 4]),
        LpType::Array(item, len) => {
            LpValue::Array((0..*len).map(|_| default_lp_value(item)).collect())
        }
        LpType::List(_) => LpValue::Array(Vec::new()),
        LpType::Struct { name, fields } => LpValue::Struct {
            name: name.clone(),
            fields: fields.iter().map(default_struct_field).collect(),
        },
        LpType::Resource => LpValue::Resource(ResourceRef::default()),
        LpType::Product(ProductKind::Visual) => {
            LpValue::Product(ProductRef::visual(VisualProduct::default()))
        }
        LpType::Product(ProductKind::Control) => {
            LpValue::Product(ProductRef::control(ControlProduct::default()))
        }
    }
}

fn default_struct_field(field: &ModelStructMember) -> (String, LpValue) {
    (field.name.clone(), default_lp_value(&field.ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot::shape::{field, record, reference, value, variant};
    use crate::{Revision, SlotDataAccess, SlotMeta, set_current_revision};
    use alloc::vec;

    #[test]
    fn dynamic_slot_object_exposes_data_as_slot_access() {
        let shape_id = SlotShapeId::from_static_name("test.dynamic_object");
        let object = DynamicSlotObject::new(
            shape_id,
            SlotData::Value(WithRevision::new(Revision::new(7), LpValue::Bool(true))),
        );

        assert_eq!(object.shape_id(), shape_id);
        let SlotDataAccess::Value(value) = object.data() else {
            panic!("expected value");
        };
        assert_eq!(value.value(), LpValue::Bool(true));
    }

    #[test]
    fn slot_factory_dynamic_creates_record_data_from_shape() {
        set_current_revision(Revision::new(11));
        let shape_id = SlotShapeId::from_static_name("test.dynamic_record");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                record(vec![
                    field("enabled", value(LpType::Bool)),
                    field("gain", value(LpType::F32)),
                ]),
            )
            .unwrap();

        let object = registry.create_default(shape_id).unwrap();
        assert_eq!(object.shape_id(), shape_id);
        let SlotDataAccess::Record(record) = object.data() else {
            panic!("expected record");
        };
        let SlotDataAccess::Value(enabled) = record.field(0).unwrap() else {
            panic!("expected enabled value");
        };
        let SlotDataAccess::Value(gain) = record.field(1).unwrap() else {
            panic!("expected gain value");
        };
        assert_eq!(enabled.changed_at(), Revision::new(11));
        assert_eq!(gain.changed_at(), Revision::new(11));
    }

    #[test]
    fn slot_factory_dynamic_creates_first_enum_variant() {
        let shape_id = SlotShapeId::from_static_name("test.dynamic_enum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                SlotShape::Enum {
                    meta: SlotMeta::empty(),
                    variants: vec![
                        variant(
                            "disabled",
                            SlotShape::Unit {
                                meta: SlotMeta::empty(),
                            },
                        ),
                        variant("enabled", record(vec![field("gain", value(LpType::F32))])),
                    ],
                },
            )
            .unwrap();

        let object = registry.create_default(shape_id).unwrap();
        let SlotDataAccess::Enum(en) = object.data() else {
            panic!("expected enum");
        };
        assert_eq!(en.variant(), "disabled");
    }

    #[test]
    fn slot_factory_dynamic_resolves_refs() {
        let target_id = SlotShapeId::from_static_name("test.dynamic_ref_target");
        let root_id = SlotShapeId::from_static_name("test.dynamic_ref_root");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(target_id, value(LpType::Bool))
            .unwrap();
        registry
            .register_dynamic_shape(root_id, reference(target_id))
            .unwrap();

        let object = registry.create_default(root_id).unwrap();
        let SlotDataAccess::Value(value) = object.data() else {
            panic!("expected value");
        };
        assert_eq!(value.value(), LpValue::Bool(false));
    }

    #[test]
    fn unsupported_factory_errors_clearly() {
        let shape_id = SlotShapeId::from_static_name("test.unsupported_factory");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_uncreatable_shape(shape_id, value(LpType::Bool))
            .unwrap();

        let Err(error) = registry.create_default(shape_id) else {
            panic!("expected unsupported factory error");
        };
        assert_eq!(error, SlotFactoryError::UnsupportedFactory(shape_id));
    }
}
