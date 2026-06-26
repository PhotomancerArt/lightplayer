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
            UiSlotValueKind::String(_) => Self::Text,
            UiSlotValueKind::I32(_) => Self::Int32,
            UiSlotValueKind::U32(_) => Self::UInt32,
            UiSlotValueKind::F32(_) => Self::Float32,
            UiSlotValueKind::Bool(_) => Self::Bool,
            UiSlotValueKind::Vec2(_) => Self::Vec2,
            UiSlotValueKind::Vec3(_) => Self::Vec3,
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
