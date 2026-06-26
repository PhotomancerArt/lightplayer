//! Renderable slot shape metadata for Studio node surfaces.

use super::ui_slot_value::UiSlotValueKind;

/// One named field inside a renderable record shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotShapeField {
    /// Human-readable field label.
    pub label: String,
    /// Field value shape.
    pub shape: UiSlotShape,
}

impl UiSlotShapeField {
    /// Create a record shape field.
    pub fn new(label: impl Into<String>, shape: UiSlotShape) -> Self {
        Self {
            label: label.into(),
            shape,
        }
    }
}

/// Studio-facing shape vocabulary for slots, products, and produced values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiSlotShape {
    /// No authored value body.
    Empty,
    /// Text or resource-like scalar value.
    Text,
    /// Signed 32-bit integer value.
    Int32,
    /// Unsigned 32-bit integer value.
    UInt32,
    /// 32-bit floating point value.
    Float32,
    /// Boolean value.
    Bool,
    /// Two-component floating point vector.
    Vec2,
    /// Three-component floating point vector.
    Vec3,
    /// Four-component floating point vector.
    Vec4,
    /// Two-component signed integer vector.
    IVec2,
    /// Three-component signed integer vector.
    IVec3,
    /// Four-component signed integer vector.
    IVec4,
    /// Two-component unsigned integer vector.
    UVec2,
    /// Three-component unsigned integer vector.
    UVec3,
    /// Four-component unsigned integer vector.
    UVec4,
    /// Two-component boolean vector.
    BVec2,
    /// Three-component boolean vector.
    BVec3,
    /// Four-component boolean vector.
    BVec4,
    /// 2x2 floating point matrix.
    Mat2x2,
    /// 3x3 floating point matrix.
    Mat3x3,
    /// 4x4 floating point matrix.
    Mat4x4,
    /// Homogeneous or wire-provided array/list payload.
    Array,
    /// Atomic enum payload.
    Enum,
    /// Store-backed resource reference.
    Resource,
    /// Named field collection.
    Record(Vec<UiSlotShapeField>),
    /// File-backed or resource-backed authored content.
    Asset(String),
    /// Produced product family.
    Product(String),
    /// Non-product produced value.
    ProducedValue,
}

impl UiSlotShape {
    /// Build a shape from a slot value kind.
    pub fn from_value_kind(kind: &UiSlotValueKind) -> Self {
        match kind {
            UiSlotValueKind::Unset => Self::Empty,
            UiSlotValueKind::String(_) => Self::Text,
            UiSlotValueKind::I32(_) => Self::Int32,
            UiSlotValueKind::U32(_) => Self::UInt32,
            UiSlotValueKind::F32(_) => Self::Float32,
            UiSlotValueKind::Bool(_) => Self::Bool,
            UiSlotValueKind::Vec2(_) => Self::Vec2,
            UiSlotValueKind::Vec3(_) => Self::Vec3,
            UiSlotValueKind::Vec4(_) => Self::Vec4,
            UiSlotValueKind::IVec2(_) => Self::IVec2,
            UiSlotValueKind::IVec3(_) => Self::IVec3,
            UiSlotValueKind::IVec4(_) => Self::IVec4,
            UiSlotValueKind::UVec2(_) => Self::UVec2,
            UiSlotValueKind::UVec3(_) => Self::UVec3,
            UiSlotValueKind::UVec4(_) => Self::UVec4,
            UiSlotValueKind::BVec2(_) => Self::BVec2,
            UiSlotValueKind::BVec3(_) => Self::BVec3,
            UiSlotValueKind::BVec4(_) => Self::BVec4,
            UiSlotValueKind::Mat2x2(_) => Self::Mat2x2,
            UiSlotValueKind::Mat3x3(_) => Self::Mat3x3,
            UiSlotValueKind::Mat4x4(_) => Self::Mat4x4,
            UiSlotValueKind::Array(_) => Self::Array,
            UiSlotValueKind::Struct { fields, .. } => Self::Record(
                fields
                    .iter()
                    .map(|(label, value)| {
                        UiSlotShapeField::new(label.clone(), Self::from_value_kind(&value.kind))
                    })
                    .collect(),
            ),
            UiSlotValueKind::Enum { .. } => Self::Enum,
            UiSlotValueKind::Resource(_) => Self::Resource,
            UiSlotValueKind::Product(_) => Self::Product("Product".to_string()),
        }
    }

    /// Stable compact label used as a text fallback outside rich Studio UI.
    pub fn summary_label(&self) -> String {
        match self {
            Self::Empty => "Empty".to_string(),
            Self::Text => "Text".to_string(),
            Self::Int32 => "Int32".to_string(),
            Self::UInt32 => "UInt32".to_string(),
            Self::Float32 => "Float32".to_string(),
            Self::Bool => "Bool".to_string(),
            Self::Vec2 => "Vec2".to_string(),
            Self::Vec3 => "Vec3".to_string(),
            Self::Vec4 => "Vec4".to_string(),
            Self::IVec2 => "IVec2".to_string(),
            Self::IVec3 => "IVec3".to_string(),
            Self::IVec4 => "IVec4".to_string(),
            Self::UVec2 => "UVec2".to_string(),
            Self::UVec3 => "UVec3".to_string(),
            Self::UVec4 => "UVec4".to_string(),
            Self::BVec2 => "BVec2".to_string(),
            Self::BVec3 => "BVec3".to_string(),
            Self::BVec4 => "BVec4".to_string(),
            Self::Mat2x2 => "Mat2x2".to_string(),
            Self::Mat3x3 => "Mat3x3".to_string(),
            Self::Mat4x4 => "Mat4x4".to_string(),
            Self::Array => "Array".to_string(),
            Self::Enum => "Enum".to_string(),
            Self::Resource => "Resource".to_string(),
            Self::Record(_) => "Record".to_string(),
            Self::Asset(label) | Self::Product(label) => label.clone(),
            Self::ProducedValue => "Produced value".to_string(),
        }
    }

    /// Stable compact detail used as a text fallback outside rich Studio UI.
    pub fn summary_detail(&self) -> Option<String> {
        match self {
            Self::Record(fields) => {
                let count = fields.len();
                Some(if count == 1 {
                    "1 field".to_string()
                } else {
                    format!("{count} fields")
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{UiSlotShape, UiSlotShapeField, UiSlotValue};

    #[test]
    fn value_kind_shapes_use_friendly_integer_names() {
        assert_eq!(
            UiSlotShape::from_value_kind(&UiSlotValue::i32(-4).kind).summary_label(),
            "Int32"
        );
        assert_eq!(
            UiSlotShape::from_value_kind(&UiSlotValue::u32(4).kind).summary_label(),
            "UInt32"
        );
    }

    #[test]
    fn record_shape_summarizes_field_count() {
        let shape = UiSlotShape::Record(vec![UiSlotShapeField::new("Time", UiSlotShape::Float32)]);

        assert_eq!(shape.summary_label(), "Record");
        assert_eq!(shape.summary_detail().as_deref(), Some("1 field"));
    }
}
