//! Typed display data for config slot values.

use crate::UiSlotEditorHint;
use lpc_model::{LpValue, ProductRef, ResourceDomain, ResourceRef};

/// The typed value family that a slot field should render.
#[derive(Clone, Debug, PartialEq)]
pub enum UiSlotValueKind {
    /// An explicitly unset value.
    Unset,
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
    /// Four-component floating point vector.
    Vec4([f32; 4]),
    /// Two-component signed integer vector.
    IVec2([i32; 2]),
    /// Three-component signed integer vector.
    IVec3([i32; 3]),
    /// Four-component signed integer vector.
    IVec4([i32; 4]),
    /// Two-component unsigned integer vector.
    UVec2([u32; 2]),
    /// Three-component unsigned integer vector.
    UVec3([u32; 3]),
    /// Four-component unsigned integer vector.
    UVec4([u32; 4]),
    /// Two-component boolean vector.
    BVec2([bool; 2]),
    /// Three-component boolean vector.
    BVec3([bool; 3]),
    /// Four-component boolean vector.
    BVec4([bool; 4]),
    /// 2x2 floating point matrix.
    Mat2x2([[f32; 2]; 2]),
    /// 3x3 floating point matrix.
    Mat3x3([[f32; 3]; 3]),
    /// 4x4 floating point matrix.
    Mat4x4([[f32; 4]; 4]),
    /// Homogeneous or wire-provided array/list payload.
    Array(Vec<UiSlotValue>),
    /// Structured value payload that is not independently addressable as slots.
    Struct {
        /// Optional type/name metadata.
        name: Option<String>,
        /// Named fields inside the atomic payload.
        fields: Vec<(String, UiSlotValue)>,
    },
    /// Atomic enum value payload.
    Enum {
        /// Active variant index.
        variant: u32,
        /// Optional variant payload.
        payload: Option<Box<UiSlotValue>>,
    },
    /// Store-backed resource reference.
    Resource(ResourceRef),
    /// Lazy graph product reference.
    Product(ProductRef),
}

impl UiSlotValueKind {
    /// Compact type label for slot metadata.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::Unset => "Unset",
            Self::String(_) => "String",
            Self::I32(_) => "I32",
            Self::U32(_) => "U32",
            Self::F32(_) => "Float32",
            Self::Bool(_) => "Bool",
            Self::Vec2(_) => "Vec2",
            Self::Vec3(_) => "Vec3",
            Self::Vec4(_) => "Vec4",
            Self::IVec2(_) => "IVec2",
            Self::IVec3(_) => "IVec3",
            Self::IVec4(_) => "IVec4",
            Self::UVec2(_) => "UVec2",
            Self::UVec3(_) => "UVec3",
            Self::UVec4(_) => "UVec4",
            Self::BVec2(_) => "BVec2",
            Self::BVec3(_) => "BVec3",
            Self::BVec4(_) => "BVec4",
            Self::Mat2x2(_) => "Mat2x2",
            Self::Mat3x3(_) => "Mat3x3",
            Self::Mat4x4(_) => "Mat4x4",
            Self::Array(_) => "Array",
            Self::Struct { .. } => "Struct",
            Self::Enum { .. } => "Enum",
            Self::Resource(_) => "Resource",
            Self::Product(_) => "Product",
        }
    }

    /// Short type description for slot metadata.
    pub fn type_description(&self) -> &'static str {
        match self {
            Self::Unset => "Unset value.",
            Self::String(_) => "Text or resource-like scalar value.",
            Self::I32(_) => "Signed integer value.",
            Self::U32(_) => "Unsigned integer value.",
            Self::F32(_) => "Floating point value.",
            Self::Bool(_) => "Boolean value.",
            Self::Vec2(_) => "Two-component floating point vector.",
            Self::Vec3(_) => "Three-component floating point vector.",
            Self::Vec4(_) => "Four-component floating point vector.",
            Self::IVec2(_) => "Two-component signed integer vector.",
            Self::IVec3(_) => "Three-component signed integer vector.",
            Self::IVec4(_) => "Four-component signed integer vector.",
            Self::UVec2(_) => "Two-component unsigned integer vector.",
            Self::UVec3(_) => "Three-component unsigned integer vector.",
            Self::UVec4(_) => "Four-component unsigned integer vector.",
            Self::BVec2(_) => "Two-component boolean vector.",
            Self::BVec3(_) => "Three-component boolean vector.",
            Self::BVec4(_) => "Four-component boolean vector.",
            Self::Mat2x2(_) => "2x2 floating point matrix.",
            Self::Mat3x3(_) => "3x3 floating point matrix.",
            Self::Mat4x4(_) => "4x4 floating point matrix.",
            Self::Array(_) => "Array or list value payload.",
            Self::Struct { .. } => "Structured value payload.",
            Self::Enum { .. } => "Enum value payload.",
            Self::Resource(_) => "Store-backed resource reference.",
            Self::Product(_) => "Lazy graph product reference.",
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
    /// Create an unset slot value.
    pub fn unset() -> Self {
        Self::new(UiSlotValueKind::Unset, "unset")
    }

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

    /// Create a four-component vector value.
    pub fn vec4(value: [f32; 4]) -> Self {
        Self::new(UiSlotValueKind::Vec4(value), format_float_array(&value))
    }

    /// Create a two-component signed integer vector value.
    pub fn ivec2(value: [i32; 2]) -> Self {
        Self::new(UiSlotValueKind::IVec2(value), format_array(&value))
    }

    /// Create a three-component signed integer vector value.
    pub fn ivec3(value: [i32; 3]) -> Self {
        Self::new(UiSlotValueKind::IVec3(value), format_array(&value))
    }

    /// Create a four-component signed integer vector value.
    pub fn ivec4(value: [i32; 4]) -> Self {
        Self::new(UiSlotValueKind::IVec4(value), format_array(&value))
    }

    /// Create a two-component unsigned integer vector value.
    pub fn uvec2(value: [u32; 2]) -> Self {
        Self::new(UiSlotValueKind::UVec2(value), format_array(&value))
    }

    /// Create a three-component unsigned integer vector value.
    pub fn uvec3(value: [u32; 3]) -> Self {
        Self::new(UiSlotValueKind::UVec3(value), format_array(&value))
    }

    /// Create a four-component unsigned integer vector value.
    pub fn uvec4(value: [u32; 4]) -> Self {
        Self::new(UiSlotValueKind::UVec4(value), format_array(&value))
    }

    /// Create a two-component boolean vector value.
    pub fn bvec2(value: [bool; 2]) -> Self {
        Self::new(UiSlotValueKind::BVec2(value), format_array(&value))
    }

    /// Create a three-component boolean vector value.
    pub fn bvec3(value: [bool; 3]) -> Self {
        Self::new(UiSlotValueKind::BVec3(value), format_array(&value))
    }

    /// Create a four-component boolean vector value.
    pub fn bvec4(value: [bool; 4]) -> Self {
        Self::new(UiSlotValueKind::BVec4(value), format_array(&value))
    }

    /// Create a 2x2 matrix value.
    pub fn mat2x2(value: [[f32; 2]; 2]) -> Self {
        Self::new(UiSlotValueKind::Mat2x2(value), format_matrix(&value))
    }

    /// Create a 3x3 matrix value.
    pub fn mat3x3(value: [[f32; 3]; 3]) -> Self {
        Self::new(UiSlotValueKind::Mat3x3(value), format_matrix(&value))
    }

    /// Create a 4x4 matrix value.
    pub fn mat4x4(value: [[f32; 4]; 4]) -> Self {
        Self::new(UiSlotValueKind::Mat4x4(value), format_matrix(&value))
    }

    /// Create an array/list value.
    pub fn array(values: Vec<UiSlotValue>) -> Self {
        let display = format!("[{}]", join_displays(values.iter()));
        Self::new(UiSlotValueKind::Array(values), display)
    }

    /// Create a structured atomic value.
    pub fn struct_value(name: Option<String>, fields: Vec<(String, UiSlotValue)>) -> Self {
        let display = format_struct_value(name.as_deref(), &fields);
        Self::new(UiSlotValueKind::Struct { name, fields }, display)
    }

    /// Create an enum value.
    pub fn enum_value(variant: u32, payload: Option<UiSlotValue>) -> Self {
        let display = match payload.as_ref() {
            Some(payload) => format!("variant {variant}({})", payload.display),
            None => format!("variant {variant}"),
        };
        Self::new(
            UiSlotValueKind::Enum {
                variant,
                payload: payload.map(Box::new),
            },
            display,
        )
    }

    /// Create a resource value.
    pub fn resource(value: ResourceRef) -> Self {
        Self::new(UiSlotValueKind::Resource(value), format_resource_ref(value))
    }

    /// Create a product value.
    pub fn product(value: ProductRef) -> Self {
        Self::new(UiSlotValueKind::Product(value), format_product_ref(value))
    }

    /// Create a UI value from a synced LightPlayer value payload.
    pub fn from_lp_value(value: &LpValue) -> Self {
        match value {
            LpValue::Unset => Self::unset(),
            LpValue::String(value) => Self::string(value.clone()),
            LpValue::I32(value) => Self::i32(*value),
            LpValue::U32(value) => Self::u32(*value),
            LpValue::F32(value) => Self::f32(*value),
            LpValue::Bool(value) => Self::bool(*value),
            LpValue::Vec2(value) => Self::vec2(*value),
            LpValue::Vec3(value) => Self::vec3(*value),
            LpValue::Vec4(value) => Self::vec4(*value),
            LpValue::IVec2(value) => Self::ivec2(*value),
            LpValue::IVec3(value) => Self::ivec3(*value),
            LpValue::IVec4(value) => Self::ivec4(*value),
            LpValue::UVec2(value) => Self::uvec2(*value),
            LpValue::UVec3(value) => Self::uvec3(*value),
            LpValue::UVec4(value) => Self::uvec4(*value),
            LpValue::BVec2(value) => Self::bvec2(*value),
            LpValue::BVec3(value) => Self::bvec3(*value),
            LpValue::BVec4(value) => Self::bvec4(*value),
            LpValue::Mat2x2(value) => Self::mat2x2(*value),
            LpValue::Mat3x3(value) => Self::mat3x3(*value),
            LpValue::Mat4x4(value) => Self::mat4x4(*value),
            LpValue::Array(values) => Self::array(values.iter().map(Self::from_lp_value).collect()),
            LpValue::Struct { name, fields } => Self::struct_value(
                name.clone(),
                fields
                    .iter()
                    .map(|(name, value)| (name.clone(), Self::from_lp_value(value)))
                    .collect(),
            ),
            LpValue::Enum { variant, payload } => {
                Self::enum_value(*variant, payload.as_deref().map(Self::from_lp_value))
            }
            LpValue::Resource(value) => Self::resource(*value),
            LpValue::Product(value) => Self::product(*value),
        }
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
    if !value.is_finite() {
        value.to_string()
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.3}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_float_array<const N: usize>(value: &[f32; N]) -> String {
    format_array_with(value, |value| format_float(*value))
}

fn format_array<T: ToString, const N: usize>(value: &[T; N]) -> String {
    format_array_with(value, ToString::to_string)
}

fn format_array_with<T, const N: usize>(value: &[T; N], format: impl Fn(&T) -> String) -> String {
    let values = value.iter().map(format).collect::<Vec<_>>().join(", ");
    format!("({values})")
}

fn format_matrix<const R: usize, const C: usize>(value: &[[f32; C]; R]) -> String {
    let rows = value
        .iter()
        .map(format_float_array)
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rows}]")
}

fn join_displays<'a>(values: impl IntoIterator<Item = &'a UiSlotValue>) -> String {
    values
        .into_iter()
        .map(|value| value.display.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_struct_value(name: Option<&str>, fields: &[(String, UiSlotValue)]) -> String {
    let fields = fields
        .iter()
        .map(|(name, value)| format!("{name}: {}", value.display))
        .collect::<Vec<_>>()
        .join(", ");
    match name {
        Some(name) => format!("{name} {{ {fields} }}"),
        None => format!("{{ {fields} }}"),
    }
}

fn format_resource_ref(value: ResourceRef) -> String {
    format!(
        "resource {}:{}",
        resource_domain_label(value.domain),
        value.id
    )
}

fn resource_domain_label(domain: ResourceDomain) -> &'static str {
    match domain {
        ResourceDomain::Unset => "unset",
        ResourceDomain::RuntimeBuffer => "runtime_buffer",
    }
}

fn format_product_ref(value: ProductRef) -> String {
    match value {
        ProductRef::Visual(product) => {
            format!(
                "visual product node {} output {}",
                product.node(),
                product.output()
            )
        }
        ProductRef::Control(product) => {
            let extent = product.preferred_extent();
            format!(
                "control product node {} output {} ({}x{})",
                product.node(),
                product.output(),
                extent.rows,
                extent.samples_per_row
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::UiSlotValue;
    use lpc_model::{
        ControlExtent, ControlProduct, LpValue, NodeId, ProductRef, ResourceRef, RuntimeBufferId,
        VisualProduct,
    };

    #[test]
    fn trims_float_display() {
        assert_eq!(UiSlotValue::f32(0.350).display, "0.35");
        assert_eq!(UiSlotValue::f32(2.0).display, "2");
    }

    #[test]
    fn formats_vector_display() {
        assert_eq!(UiSlotValue::vec2([0.5, 1.0]).display, "(0.5, 1)");
    }

    #[test]
    fn covers_all_scalar_value_families() {
        let cases = [
            (LpValue::Unset, "Unset", "unset"),
            (LpValue::String("idle".to_string()), "String", "idle"),
            (LpValue::I32(-4), "I32", "-4"),
            (LpValue::U32(4), "U32", "4"),
            (LpValue::F32(0.35), "Float32", "0.35"),
            (LpValue::Bool(true), "Bool", "true"),
        ];

        for (value, label, display) in cases {
            let ui = UiSlotValue::from_lp_value(&value);

            assert_eq!(ui.kind.type_label(), label);
            assert_eq!(ui.display, display);
        }
    }

    #[test]
    fn covers_vector_and_matrix_value_families() {
        let cases = [
            (
                UiSlotValue::from_lp_value(&LpValue::Vec4([1.0, 2.0, 3.0, 4.0])),
                "Vec4",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::IVec2([-1, 2])),
                "IVec2",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::IVec3([-1, 2, 3])),
                "IVec3",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::IVec4([-1, 2, 3, 4])),
                "IVec4",
            ),
            (UiSlotValue::from_lp_value(&LpValue::UVec2([1, 2])), "UVec2"),
            (
                UiSlotValue::from_lp_value(&LpValue::UVec3([1, 2, 3])),
                "UVec3",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::UVec4([1, 2, 3, 4])),
                "UVec4",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::BVec2([true, false])),
                "BVec2",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::BVec3([true, false, true])),
                "BVec3",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::BVec4([true, false, true, false])),
                "BVec4",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::Mat2x2([[1.0, 0.0], [0.0, 1.0]])),
                "Mat2x2",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::Mat3x3([
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                ])),
                "Mat3x3",
            ),
            (
                UiSlotValue::from_lp_value(&LpValue::Mat4x4([
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ])),
                "Mat4x4",
            ),
        ];

        for (value, label) in cases {
            assert_eq!(value.kind.type_label(), label);
            assert!(!value.display.is_empty());
        }
    }

    #[test]
    fn covers_structural_resource_and_product_value_families() {
        let values = [
            UiSlotValue::from_lp_value(&LpValue::Array(vec![LpValue::I32(1), LpValue::I32(2)])),
            UiSlotValue::from_lp_value(&LpValue::Struct {
                name: Some("Pair".to_string()),
                fields: vec![("x".to_string(), LpValue::F32(1.0))],
            }),
            UiSlotValue::from_lp_value(&LpValue::Enum {
                variant: 7,
                payload: Some(Box::new(LpValue::String("ready".to_string()))),
            }),
            UiSlotValue::from_lp_value(&LpValue::Resource(ResourceRef::runtime_buffer(
                RuntimeBufferId::new(4),
            ))),
            UiSlotValue::from_lp_value(&LpValue::Product(ProductRef::visual(VisualProduct::new(
                NodeId::new(3),
                0,
            )))),
            UiSlotValue::from_lp_value(&LpValue::Product(ProductRef::control(
                ControlProduct::new(NodeId::new(4), 1, ControlExtent::new(2, 24)),
            ))),
        ];
        let labels = ["Array", "Struct", "Enum", "Resource", "Product", "Product"];

        for (value, label) in values.into_iter().zip(labels) {
            assert_eq!(value.kind.type_label(), label);
            assert!(!value.display.is_empty());
        }
    }
}
