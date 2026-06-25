//! Typed display data for config slot values.

use crate::UiSlotEditorHint;

/// The typed value family that a slot field should render.
#[derive(Clone, Debug, PartialEq)]
pub enum UiSlotValueKind {
    /// Text or resource-like scalar value.
    String(String),
    /// Signed integer value.
    I32(i32),
    /// Unsigned integer value.
    U32(u32),
    /// Floating point value.
    F32(f32),
    /// Boolean value.
    Bool(bool),
    /// Two-component floating point vector.
    Vec2([f32; 2]),
    /// Three-component floating point vector.
    Vec3([f32; 3]),
}

impl UiSlotValueKind {
    /// Compact type label for slot metadata.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::String(_) => "String",
            Self::I32(_) => "I32",
            Self::U32(_) => "U32",
            Self::F32(_) => "Float32",
            Self::Bool(_) => "Bool",
            Self::Vec2(_) => "Vec2",
            Self::Vec3(_) => "Vec3",
        }
    }

    /// Short type description for slot metadata.
    pub fn type_description(&self) -> &'static str {
        match self {
            Self::String(_) => "Text or resource-like scalar value.",
            Self::I32(_) => "Signed integer value.",
            Self::U32(_) => "Unsigned integer value.",
            Self::F32(_) => "Floating point value.",
            Self::Bool(_) => "Boolean value.",
            Self::Vec2(_) => "Two-component floating point vector.",
            Self::Vec3(_) => "Three-component floating point vector.",
        }
    }
}

/// A typed slot value plus display and editor metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSlotValue {
    /// Value family consumed by field components.
    pub kind: UiSlotValueKind,
    /// Compact formatted value for read-only and summary display.
    pub display: String,
    /// Optional unit, shape, or secondary detail.
    pub detail: Option<String>,
    /// Preferred editor treatment for this value.
    pub editor: UiSlotEditorHint,
}

impl UiSlotValue {
    /// Create a text slot value.
    pub fn string(value: impl Into<String>) -> Self {
        let value = value.into();
        Self::new(UiSlotValueKind::String(value.clone()), value)
    }

    /// Create a signed integer slot value.
    pub fn i32(value: i32) -> Self {
        Self::new(UiSlotValueKind::I32(value), value.to_string())
    }

    /// Create an unsigned integer slot value.
    pub fn u32(value: u32) -> Self {
        Self::new(UiSlotValueKind::U32(value), value.to_string())
    }

    /// Create a floating point slot value.
    pub fn f32(value: f32) -> Self {
        Self::new(UiSlotValueKind::F32(value), format_float(value))
    }

    /// Create a boolean slot value.
    pub fn bool(value: bool) -> Self {
        Self::new(UiSlotValueKind::Bool(value), value.to_string())
    }

    /// Create a two-component vector value.
    pub fn vec2(value: [f32; 2]) -> Self {
        Self::new(
            UiSlotValueKind::Vec2(value),
            format!("({}, {})", format_float(value[0]), format_float(value[1])),
        )
    }

    /// Create a three-component vector value.
    pub fn vec3(value: [f32; 3]) -> Self {
        Self::new(
            UiSlotValueKind::Vec3(value),
            format!(
                "({}, {}, {})",
                format_float(value[0]),
                format_float(value[1]),
                format_float(value[2])
            ),
        )
    }

    /// Create a typed slot value with an explicit display string.
    pub fn new(kind: UiSlotValueKind, display: impl Into<String>) -> Self {
        Self {
            kind,
            display: display.into(),
            detail: None,
            editor: UiSlotEditorHint::Auto,
        }
    }

    /// Add secondary detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add an editor hint.
    pub fn with_editor(mut self, editor: UiSlotEditorHint) -> Self {
        self.editor = editor;
        self
    }
}

fn format_float(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.3}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::UiSlotValue;

    #[test]
    fn trims_float_display() {
        assert_eq!(UiSlotValue::f32(0.350).display, "0.35");
        assert_eq!(UiSlotValue::f32(2.0).display, "2");
    }

    #[test]
    fn formats_vector_display() {
        assert_eq!(UiSlotValue::vec2([0.5, 1.0]).display, "(0.5, 1)");
    }
}
