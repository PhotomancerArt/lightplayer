use crate::{ModelStructMember, ModelType, ModelValue, RelativeNodeRef, ResourceRef};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use super::SlotMeta;

/// Stable identity for a slot leaf descriptor.
///
/// A leaf id names an atomic value contract: storage shape, semantic meaning,
/// and editor hints travel together. This prevents attaching domain semantics
/// to arbitrary incompatible storage.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotLeafId(u32);

impl SlotLeafId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn from_static_name(input: &str) -> Self {
        Self(fnv1a32(input))
    }

    pub fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SlotLeafId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.0)
    }
}

/// Shape of one atomic value leaf.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotValueShape {
    pub leaf: SlotLeafId,
    pub ty: ModelType,
    #[serde(default)]
    pub meta: SlotMeta,
    #[serde(default)]
    pub editor: SlotEditorHint,
}

impl SlotValueShape {
    pub fn raw(ty: ModelType) -> Self {
        Self {
            leaf: raw_leaf_id(&ty),
            ty,
            meta: SlotMeta::empty(),
            editor: SlotEditorHint::default(),
        }
    }
}

/// Editor hint for a slot value leaf.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotEditorHint {
    #[default]
    Plain,
    NodeRef,
    Path,
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<OrderedF32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<OrderedF32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<OrderedF32>,
    },
    Slider {
        min: OrderedF32,
        max: OrderedF32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<OrderedF32>,
    },
    Xy,
    Dimensions,
    Affine2d,
    Resource,
    RuntimeBufferResource,
    RenderProductResource,
    Dropdown {
        options: Vec<SlotEnumOption>,
    },
}

/// One choice in a dropdown-like atomic leaf.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotEnumOption {
    pub value: String,
    pub label: String,
}

impl SlotEnumOption {
    pub fn new(value: &str, label: &str) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
        }
    }
}

/// A comparable/serializable f32 wrapper for metadata and editor hints.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct OrderedF32(pub f32);

impl PartialEq for OrderedF32 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedF32 {}

/// Conversion from a typed slot leaf value into the generic model value.
pub trait ToModelValue {
    fn to_model_value(&self) -> ModelValue;
}

/// Conversion from the generic model value into a typed slot leaf value.
pub trait FromModelValue: Sized {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError>;
}

/// Atomic typed leaf contract.
pub trait SlotLeaf: ToModelValue + FromModelValue {
    const LEAF_ID: SlotLeafId;

    fn value_shape() -> SlotValueShape;
}

/// Error converting a generic model value into a typed slot leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotLeafError {
    pub message: String,
}

impl SlotLeafError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SlotLeafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for SlotLeafError {}

/// Width/height dimensions in unsigned integer pixels or cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dim2u {
    pub width: u32,
    pub height: u32,
}

/// 2D affine transform with translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2d {
    pub m00: f32,
    pub m01: f32,
    pub m10: f32,
    pub m11: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Affine2d {
    pub fn identity() -> Self {
        Self {
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

/// RGB channel order for fixture/output color packing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorOrderValue {
    Rgb,
    Grb,
    Rbg,
    Gbr,
    Brg,
    Bgr,
}

impl ColorOrderValue {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Grb => "grb",
            Self::Rbg => "rbg",
            Self::Gbr => "gbr",
            Self::Brg => "brg",
            Self::Bgr => "bgr",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rgb" => Some(Self::Rgb),
            "grb" => Some(Self::Grb),
            "rbg" => Some(Self::Rbg),
            "gbr" => Some(Self::Gbr),
            "brg" => Some(Self::Brg),
            "bgr" => Some(Self::Bgr),
            _ => None,
        }
    }
}

pub type RelativeNodeRefSlot = super::SlotValue<RelativeNodeRef>;
pub type ArtifactPathSlot = super::SlotValue<String>;
pub type SourcePathSlot = super::SlotValue<String>;
pub type Dim2uSlot = super::SlotValue<Dim2u>;
pub type RatioSlot = super::SlotValue<f32>;
pub type PositiveF32Slot = super::SlotValue<f32>;
pub type XySlot = super::SlotValue<[f32; 2]>;
pub type Affine2dSlot = super::SlotValue<Affine2d>;
pub type ColorOrderSlot = super::SlotValue<ColorOrderValue>;
pub type RenderOrderSlot = super::SlotValue<i32>;
pub type ResourceRefSlot = super::SlotValue<ResourceRef>;

pub fn relative_node_ref_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.relative_node_ref"),
        ty: ModelType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::NodeRef,
    }
}

pub fn artifact_path_shape() -> SlotValueShape {
    path_shape("slot.leaf.artifact_path")
}

pub fn source_path_shape() -> SlotValueShape {
    path_shape("slot.leaf.source_path")
}

pub fn dim2u_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.dim2u"),
        ty: dim2u_model_type(),
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Dimensions,
    }
}

pub fn ratio_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.ratio"),
        ty: ModelType::F32,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Slider {
            min: OrderedF32(0.0),
            max: OrderedF32(1.0),
            step: Some(OrderedF32(0.01)),
        },
    }
}

pub fn positive_f32_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.positive_f32"),
        ty: ModelType::F32,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Number {
            min: Some(OrderedF32(0.0)),
            max: None,
            step: None,
        },
    }
}

pub fn xy_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.xy"),
        ty: ModelType::Vec2,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Xy,
    }
}

pub fn affine2d_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.affine2d"),
        ty: affine2d_model_type(),
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Affine2d,
    }
}

pub fn color_order_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.color_order"),
        ty: ModelType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Dropdown {
            options: vec![
                SlotEnumOption::new("rgb", "RGB"),
                SlotEnumOption::new("grb", "GRB"),
                SlotEnumOption::new("rbg", "RBG"),
                SlotEnumOption::new("gbr", "GBR"),
                SlotEnumOption::new("brg", "BRG"),
                SlotEnumOption::new("bgr", "BGR"),
            ],
        },
    }
}

pub fn render_order_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.render_order"),
        ty: ModelType::I32,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Number {
            min: None,
            max: None,
            step: Some(OrderedF32(1.0)),
        },
    }
}

pub fn resource_ref_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.resource_ref"),
        ty: ModelType::Resource,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Resource,
    }
}

pub fn runtime_buffer_resource_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.runtime_buffer_resource"),
        ty: ModelType::Resource,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::RuntimeBufferResource,
    }
}

pub fn render_product_resource_shape() -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name("slot.leaf.render_product_resource"),
        ty: ModelType::Resource,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::RenderProductResource,
    }
}

fn path_shape(name: &str) -> SlotValueShape {
    SlotValueShape {
        leaf: SlotLeafId::from_static_name(name),
        ty: ModelType::String,
        meta: SlotMeta::empty(),
        editor: SlotEditorHint::Path,
    }
}

fn dim2u_model_type() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("Dim2u")),
        fields: vec![
            ModelStructMember {
                name: String::from("width"),
                ty: ModelType::U32,
            },
            ModelStructMember {
                name: String::from("height"),
                ty: ModelType::U32,
            },
        ],
    }
}

fn affine2d_model_type() -> ModelType {
    ModelType::Struct {
        name: Some(String::from("Affine2d")),
        fields: vec![
            ModelStructMember {
                name: String::from("m00"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("m01"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("m10"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("m11"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("tx"),
                ty: ModelType::F32,
            },
            ModelStructMember {
                name: String::from("ty"),
                ty: ModelType::F32,
            },
        ],
    }
}

impl ToModelValue for ModelValue {
    fn to_model_value(&self) -> ModelValue {
        self.clone()
    }
}

impl ToModelValue for String {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::String(self.clone())
    }
}

impl ToModelValue for &str {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::String((*self).to_string())
    }
}

impl ToModelValue for RelativeNodeRef {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::String(self.to_string())
    }
}

impl ToModelValue for i32 {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::I32(*self)
    }
}

impl ToModelValue for u32 {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::U32(*self)
    }
}

impl ToModelValue for f32 {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::F32(*self)
    }
}

impl ToModelValue for bool {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Bool(*self)
    }
}

impl ToModelValue for [f32; 2] {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Vec2(*self)
    }
}

impl ToModelValue for [f32; 3] {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Vec3(*self)
    }
}

impl ToModelValue for Dim2u {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Struct {
            name: Some(String::from("Dim2u")),
            fields: vec![
                (String::from("width"), ModelValue::U32(self.width)),
                (String::from("height"), ModelValue::U32(self.height)),
            ],
        }
    }
}

impl ToModelValue for Affine2d {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Struct {
            name: Some(String::from("Affine2d")),
            fields: vec![
                (String::from("m00"), ModelValue::F32(self.m00)),
                (String::from("m01"), ModelValue::F32(self.m01)),
                (String::from("m10"), ModelValue::F32(self.m10)),
                (String::from("m11"), ModelValue::F32(self.m11)),
                (String::from("tx"), ModelValue::F32(self.tx)),
                (String::from("ty"), ModelValue::F32(self.ty)),
            ],
        }
    }
}

impl ToModelValue for ColorOrderValue {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::String(self.as_str().to_string())
    }
}

impl ToModelValue for ResourceRef {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::Resource(*self)
    }
}

macro_rules! impl_from_model_value {
    ($ty:ty, $variant:ident) => {
        impl FromModelValue for $ty {
            fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
                match value {
                    ModelValue::$variant(value) => Ok(value),
                    other => Err(SlotLeafError::new(alloc::format!(
                        "expected {}, got {other:?}",
                        stringify!($variant)
                    ))),
                }
            }
        }
    };
}

impl_from_model_value!(String, String);
impl_from_model_value!(i32, I32);
impl_from_model_value!(u32, U32);
impl_from_model_value!(f32, F32);
impl_from_model_value!(bool, Bool);

impl FromModelValue for [f32; 2] {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::Vec2(value) => Ok(value),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected Vec2, got {other:?}"
            ))),
        }
    }
}

impl FromModelValue for [f32; 3] {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::Vec3(value) => Ok(value),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected Vec3, got {other:?}"
            ))),
        }
    }
}

impl FromModelValue for RelativeNodeRef {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::String(value) => RelativeNodeRef::parse(&value)
                .map_err(|err| SlotLeafError::new(alloc::format!("{err}"))),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl FromModelValue for Dim2u {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        let ModelValue::Struct { name, fields } = value else {
            return Err(SlotLeafError::new("expected Dim2u struct"));
        };
        if name.as_deref() != Some("Dim2u") || fields.len() != 2 {
            return Err(SlotLeafError::new("expected Dim2u struct"));
        }
        let width = match &fields[0] {
            (name, ModelValue::U32(value)) if name == "width" => *value,
            _ => return Err(SlotLeafError::new("expected Dim2u.width")),
        };
        let height = match &fields[1] {
            (name, ModelValue::U32(value)) if name == "height" => *value,
            _ => return Err(SlotLeafError::new("expected Dim2u.height")),
        };
        Ok(Self { width, height })
    }
}

impl FromModelValue for Affine2d {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        let ModelValue::Struct { name, fields } = value else {
            return Err(SlotLeafError::new("expected Affine2d struct"));
        };
        if name.as_deref() != Some("Affine2d") || fields.len() != 6 {
            return Err(SlotLeafError::new("expected Affine2d struct"));
        }
        Ok(Self {
            m00: struct_f32(&fields, 0, "m00")?,
            m01: struct_f32(&fields, 1, "m01")?,
            m10: struct_f32(&fields, 2, "m10")?,
            m11: struct_f32(&fields, 3, "m11")?,
            tx: struct_f32(&fields, 4, "tx")?,
            ty: struct_f32(&fields, 5, "ty")?,
        })
    }
}

impl FromModelValue for ColorOrderValue {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::String(value) => Self::parse(&value)
                .ok_or_else(|| SlotLeafError::new("expected RGB color order value")),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl FromModelValue for ResourceRef {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::Resource(value) => Ok(value),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected Resource, got {other:?}"
            ))),
        }
    }
}

macro_rules! impl_slot_leaf {
    ($ty:ty, $id:literal, $shape:expr) => {
        impl SlotLeaf for $ty {
            const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name($id);

            fn value_shape() -> SlotValueShape {
                $shape
            }
        }
    };
}

impl_slot_leaf!(
    RelativeNodeRef,
    "slot.leaf.relative_node_ref",
    relative_node_ref_shape()
);
impl_slot_leaf!(
    String,
    "slot.leaf.raw_string",
    SlotValueShape::raw(ModelType::String)
);
impl_slot_leaf!(
    i32,
    "slot.leaf.raw_i32",
    SlotValueShape::raw(ModelType::I32)
);
impl_slot_leaf!(
    u32,
    "slot.leaf.raw_u32",
    SlotValueShape::raw(ModelType::U32)
);
impl_slot_leaf!(
    f32,
    "slot.leaf.raw_f32",
    SlotValueShape::raw(ModelType::F32)
);
impl_slot_leaf!(
    bool,
    "slot.leaf.raw_bool",
    SlotValueShape::raw(ModelType::Bool)
);
impl_slot_leaf!(
    [f32; 2],
    "slot.leaf.raw_vec2",
    SlotValueShape::raw(ModelType::Vec2)
);
impl_slot_leaf!(
    [f32; 3],
    "slot.leaf.raw_vec3",
    SlotValueShape::raw(ModelType::Vec3)
);
impl_slot_leaf!(Dim2u, "slot.leaf.dim2u", dim2u_shape());
impl_slot_leaf!(Affine2d, "slot.leaf.affine2d", affine2d_shape());
impl_slot_leaf!(
    ColorOrderValue,
    "slot.leaf.color_order",
    color_order_shape()
);
impl_slot_leaf!(ResourceRef, "slot.leaf.resource_ref", resource_ref_shape());

fn struct_f32(
    fields: &[(String, ModelValue)],
    index: usize,
    expected_name: &str,
) -> Result<f32, SlotLeafError> {
    match fields.get(index) {
        Some((name, ModelValue::F32(value))) if name == expected_name => Ok(*value),
        _ => Err(SlotLeafError::new(alloc::format!(
            "expected Affine2d.{expected_name}"
        ))),
    }
}

fn raw_leaf_id(ty: &ModelType) -> SlotLeafId {
    SlotLeafId::from_static_name(match ty {
        ModelType::String => "slot.leaf.raw_string",
        ModelType::I32 => "slot.leaf.raw_i32",
        ModelType::U32 => "slot.leaf.raw_u32",
        ModelType::F32 => "slot.leaf.raw_f32",
        ModelType::Bool => "slot.leaf.raw_bool",
        ModelType::Vec2 => "slot.leaf.raw_vec2",
        ModelType::Vec3 => "slot.leaf.raw_vec3",
        ModelType::Vec4 => "slot.leaf.raw_vec4",
        ModelType::IVec2 => "slot.leaf.raw_ivec2",
        ModelType::IVec3 => "slot.leaf.raw_ivec3",
        ModelType::IVec4 => "slot.leaf.raw_ivec4",
        ModelType::UVec2 => "slot.leaf.raw_uvec2",
        ModelType::UVec3 => "slot.leaf.raw_uvec3",
        ModelType::UVec4 => "slot.leaf.raw_uvec4",
        ModelType::BVec2 => "slot.leaf.raw_bvec2",
        ModelType::BVec3 => "slot.leaf.raw_bvec3",
        ModelType::BVec4 => "slot.leaf.raw_bvec4",
        ModelType::Mat2x2 => "slot.leaf.raw_mat2x2",
        ModelType::Mat3x3 => "slot.leaf.raw_mat3x3",
        ModelType::Mat4x4 => "slot.leaf.raw_mat4x4",
        ModelType::Array(_, _) => "slot.leaf.raw_array",
        ModelType::Struct { .. } => "slot.leaf.raw_struct",
        ModelType::Resource => "slot.leaf.raw_resource",
    })
}

const fn fnv1a32(input: &str) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;

    let bytes = input.as_bytes();
    let mut hash = OFFSET;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u32;
        hash = hash.wrapping_mul(PRIME);
        index += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_leaf_shapes_carry_editor_hints() {
        assert!(matches!(
            relative_node_ref_shape().editor,
            SlotEditorHint::NodeRef
        ));
        assert!(matches!(dim2u_shape().editor, SlotEditorHint::Dimensions));
        assert!(matches!(affine2d_shape().editor, SlotEditorHint::Affine2d));
        assert!(matches!(
            runtime_buffer_resource_shape().editor,
            SlotEditorHint::RuntimeBufferResource
        ));
        assert!(matches!(
            render_product_resource_shape().editor,
            SlotEditorHint::RenderProductResource
        ));
        assert!(matches!(
            color_order_shape().editor,
            SlotEditorHint::Dropdown { .. }
        ));
    }

    #[test]
    fn semantic_leaf_values_round_trip_through_model_value() {
        let dim = Dim2u {
            width: 64,
            height: 32,
        };
        assert_eq!(Dim2u::from_model_value(dim.to_model_value()).unwrap(), dim);

        let affine = Affine2d::identity();
        assert_eq!(
            Affine2d::from_model_value(affine.to_model_value()).unwrap(),
            affine
        );

        let order = ColorOrderValue::Grb;
        assert_eq!(
            ColorOrderValue::from_model_value(order.to_model_value()).unwrap(),
            order
        );

        let resource = ResourceRef::render_product(crate::RenderProductId::new(7));
        assert_eq!(
            ResourceRef::from_model_value(resource.to_model_value()).unwrap(),
            resource
        );
    }
}
