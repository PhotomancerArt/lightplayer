//! Typed value contracts for [`SlotShape::Value`](crate::SlotShape::Value) leaves.
//!
//! The slot tree owns addressability, versioning, watching, mutation, and sync.
//! A `SlotShape::Value` is the boundary where that tree stops and one complete
//! [`LpValue`] payload begins. [`SlotValue`] is the typed Rust-side contract for
//! values that can live at that boundary, while [`SlotValueShape`] describes the
//! payload type, semantic metadata, and editor hints generic clients need.

use crate::{LpType, LpValue, SlotShapeId};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use super::SlotMeta;

/// Typed Rust value that can occupy a slot value boundary.
///
/// `SlotValue` implementors convert to and from the portable [`LpValue`] wire
/// representation and provide the static shape metadata for that complete
/// payload. Sub-fields inside the payload are value structure, not addressable
/// slots, and they do not get independent versions.
pub trait SlotValue: ToLpValue + FromLpValue {
    const SHAPE_ID: SlotShapeId;

    fn value_shape() -> SlotValueShape;
}

/// Shape of one complete value payload at a slot leaf.
///
/// The `id` is a [`SlotShapeId`] so value shapes participate in the same shape
/// identity space as slot roots. The `ty` validates the portable [`LpValue`]
/// storage form. Metadata and editor hints attach to this semantic value
/// contract, not to arbitrary storage types.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotValueShape {
    pub id: SlotShapeId,
    pub ty: LpType,
    #[serde(default)]
    pub meta: SlotMeta,
    #[serde(default)]
    pub editor: ValueEditorHint,
}

impl SlotValueShape {
    pub fn raw(ty: LpType) -> Self {
        Self {
            id: raw_shape_id(&ty),
            ty,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::default(),
        }
    }
}

/// Editor hint for a slot value leaf.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ValueEditorHint {
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
    RenderProduct,
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
pub trait ToLpValue {
    fn to_lp_value(&self) -> LpValue;
}

/// Conversion from the generic model value into a typed slot leaf value.
pub trait FromLpValue: Sized {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError>;
}
/// Error converting a generic model value into a typed slot leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueRootError {
    pub message: String,
}

impl ValueRootError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ValueRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for ValueRootError {}

impl ToLpValue for LpValue {
    fn to_lp_value(&self) -> LpValue {
        self.clone()
    }
}

impl ToLpValue for String {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.clone())
    }
}

impl ToLpValue for &str {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String((*self).to_string())
    }
}

impl ToLpValue for i32 {
    fn to_lp_value(&self) -> LpValue {
        LpValue::I32(*self)
    }
}

impl ToLpValue for u32 {
    fn to_lp_value(&self) -> LpValue {
        LpValue::U32(*self)
    }
}

impl ToLpValue for f32 {
    fn to_lp_value(&self) -> LpValue {
        LpValue::F32(*self)
    }
}

impl ToLpValue for bool {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Bool(*self)
    }
}

impl ToLpValue for [f32; 2] {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Vec2(*self)
    }
}

impl ToLpValue for [f32; 3] {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Vec3(*self)
    }
}

macro_rules! impl_from_lp_value {
    ($ty:ty, $variant:ident) => {
        impl FromLpValue for $ty {
            fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
                match value {
                    LpValue::$variant(value) => Ok(value),
                    other => Err(ValueRootError::new(alloc::format!(
                        "expected {}, got {other:?}",
                        stringify!($variant)
                    ))),
                }
            }
        }
    };
}

impl_from_lp_value!(String, String);
impl_from_lp_value!(i32, I32);
impl_from_lp_value!(u32, U32);
impl_from_lp_value!(f32, F32);
impl_from_lp_value!(bool, Bool);

impl FromLpValue for [f32; 2] {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Vec2(value) => Ok(value),
            other => Err(ValueRootError::new(alloc::format!(
                "expected Vec2, got {other:?}"
            ))),
        }
    }
}

impl FromLpValue for [f32; 3] {
    fn from_lp_value(value: LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::Vec3(value) => Ok(value),
            other => Err(ValueRootError::new(alloc::format!(
                "expected Vec3, got {other:?}"
            ))),
        }
    }
}

macro_rules! impl_slot_leaf {
    ($ty:ty, $id:literal, $shape:expr) => {
        impl SlotValue for $ty {
            const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name($id);

            fn value_shape() -> SlotValueShape {
                $shape
            }
        }
    };
}

impl_slot_leaf!(
    String,
    "slot.leaf.raw_string",
    SlotValueShape::raw(LpType::String)
);
impl_slot_leaf!(i32, "slot.leaf.raw_i32", SlotValueShape::raw(LpType::I32));
impl_slot_leaf!(u32, "slot.leaf.raw_u32", SlotValueShape::raw(LpType::U32));
impl_slot_leaf!(f32, "slot.leaf.raw_f32", SlotValueShape::raw(LpType::F32));
impl_slot_leaf!(
    bool,
    "slot.leaf.raw_bool",
    SlotValueShape::raw(LpType::Bool)
);
impl_slot_leaf!(
    [f32; 2],
    "slot.leaf.raw_vec2",
    SlotValueShape::raw(LpType::Vec2)
);
impl_slot_leaf!(
    [f32; 3],
    "slot.leaf.raw_vec3",
    SlotValueShape::raw(LpType::Vec3)
);
fn raw_shape_id(ty: &LpType) -> SlotShapeId {
    SlotShapeId::from_static_name(match ty {
        LpType::String => "slot.leaf.raw_string",
        LpType::I32 => "slot.leaf.raw_i32",
        LpType::U32 => "slot.leaf.raw_u32",
        LpType::F32 => "slot.leaf.raw_f32",
        LpType::Bool => "slot.leaf.raw_bool",
        LpType::Vec2 => "slot.leaf.raw_vec2",
        LpType::Vec3 => "slot.leaf.raw_vec3",
        LpType::Vec4 => "slot.leaf.raw_vec4",
        LpType::IVec2 => "slot.leaf.raw_ivec2",
        LpType::IVec3 => "slot.leaf.raw_ivec3",
        LpType::IVec4 => "slot.leaf.raw_ivec4",
        LpType::UVec2 => "slot.leaf.raw_uvec2",
        LpType::UVec3 => "slot.leaf.raw_uvec3",
        LpType::UVec4 => "slot.leaf.raw_uvec4",
        LpType::BVec2 => "slot.leaf.raw_bvec2",
        LpType::BVec3 => "slot.leaf.raw_bvec3",
        LpType::BVec4 => "slot.leaf.raw_bvec4",
        LpType::Mat2x2 => "slot.leaf.raw_mat2x2",
        LpType::Mat3x3 => "slot.leaf.raw_mat3x3",
        LpType::Mat4x4 => "slot.leaf.raw_mat4x4",
        LpType::Array(_, _) => "slot.leaf.raw_array",
        LpType::List(_) => "slot.leaf.raw_list",
        LpType::Struct { .. } => "slot.leaf.raw_struct",
        LpType::Resource => "slot.leaf.raw_resource",
        LpType::RenderProduct => "slot.leaf.raw_render_product",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Affine2d, ColorOrderValue, Dim2u, FromLpValue, RenderProductId, ResourceRef, ToLpValue,
        affine2d_shape, color_order_shape, dim2u_shape, relative_node_ref_shape,
        render_product_resource_shape, runtime_buffer_resource_shape,
    };

    #[test]
    fn semantic_leaf_shapes_carry_editor_hints() {
        assert!(matches!(
            relative_node_ref_shape().editor,
            ValueEditorHint::NodeRef
        ));
        assert!(matches!(dim2u_shape().editor, ValueEditorHint::Dimensions));
        assert!(matches!(affine2d_shape().editor, ValueEditorHint::Affine2d));
        assert!(matches!(
            runtime_buffer_resource_shape().editor,
            ValueEditorHint::RuntimeBufferResource
        ));
        assert!(matches!(
            render_product_resource_shape().editor,
            ValueEditorHint::RenderProductResource
        ));
        assert!(matches!(
            color_order_shape().editor,
            ValueEditorHint::Dropdown { .. }
        ));
    }

    #[test]
    fn semantic_leaf_values_round_trip_through_lp_value() {
        let dim = Dim2u {
            width: 64,
            height: 32,
        };
        assert_eq!(Dim2u::from_lp_value(dim.to_lp_value()).unwrap(), dim);

        let affine = Affine2d::identity();
        assert_eq!(
            Affine2d::from_lp_value(affine.to_lp_value()).unwrap(),
            affine
        );

        let order = ColorOrderValue::Grb;
        assert_eq!(
            ColorOrderValue::from_lp_value(order.to_lp_value()).unwrap(),
            order
        );

        let resource = ResourceRef::render_product(RenderProductId::new(7));
        assert_eq!(
            ResourceRef::from_lp_value(resource.to_lp_value()).unwrap(),
            resource
        );
    }
}
