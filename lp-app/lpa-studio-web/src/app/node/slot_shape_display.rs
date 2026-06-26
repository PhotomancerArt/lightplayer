//! Shared presentation for Studio slot shape metadata.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotShape, UiSlotShapeField};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "all shape display modes are exercised by story builds and future callers"
)]
pub(crate) enum SlotShapeDisplayMode {
    Compact,
    CompactFriendly,
    Verbose,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SlotShapeDisplay(shape: UiSlotShape, mode: SlotShapeDisplayMode) -> Element {
    match mode {
        SlotShapeDisplayMode::Compact | SlotShapeDisplayMode::CompactFriendly => rsx! {
            span { class: compact_shape_class(mode),
                code { class: "tw:font-mono tw:font-bold tw:text-heading", "{shape_label(&shape)}" }
                if let Some(detail) = compact_shape_detail(&shape, mode) {
                    span { class: "tw:text-subtle-foreground tw:break-words", "{detail}" }
                }
            }
        },
        SlotShapeDisplayMode::Verbose => {
            let hint = shape_hint(&shape);
            let fields = record_fields(&shape);

            rsx! {
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    span { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5",
                        code { class: "tw:font-mono tw:text-xs tw:font-bold tw:text-heading", "{shape_label(&shape)}" }
                        if let Some(hint) = hint {
                            span { class: "tw:text-xs tw:text-subtle-foreground tw:break-words", "{hint}" }
                        }
                    }
                    if !fields.is_empty() {
                        div { class: "tw:grid tw:min-w-0 tw:gap-0.5 tw:border-l tw:border-border-muted tw:pl-2",
                            for field in fields {
                                ShapeFieldSummary { field: field.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn legacy_shape_from_parts(value: &str, detail: Option<&str>) -> UiSlotShape {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "string" | "text" => UiSlotShape::Text,
        "i32" | "int32" => UiSlotShape::Int32,
        "u32" | "uint32" => UiSlotShape::UInt32,
        "f32" | "float32" => UiSlotShape::Float32,
        "bool" | "boolean" => UiSlotShape::Bool,
        "vec2" => UiSlotShape::Vec2,
        "vec3" => UiSlotShape::Vec3,
        "vec4" => UiSlotShape::Vec4,
        "ivec2" => UiSlotShape::IVec2,
        "ivec3" => UiSlotShape::IVec3,
        "ivec4" => UiSlotShape::IVec4,
        "uvec2" => UiSlotShape::UVec2,
        "uvec3" => UiSlotShape::UVec3,
        "uvec4" => UiSlotShape::UVec4,
        "bvec2" => UiSlotShape::BVec2,
        "bvec3" => UiSlotShape::BVec3,
        "bvec4" => UiSlotShape::BVec4,
        "mat2x2" => UiSlotShape::Mat2x2,
        "mat3x3" => UiSlotShape::Mat3x3,
        "mat4x4" => UiSlotShape::Mat4x4,
        "array" => UiSlotShape::Array,
        "enum" => UiSlotShape::Enum,
        "resource" => UiSlotShape::Resource,
        "record" => UiSlotShape::Record(Vec::new()),
        "empty" => UiSlotShape::Empty,
        "produced value" => UiSlotShape::ProducedValue,
        _ if normalized.ends_with("product") => UiSlotShape::Product(value.to_string()),
        _ if normalized.ends_with("asset") => UiSlotShape::Asset(value.to_string()),
        _ => detail
            .map(|detail| UiSlotShape::Product(format!("{value} {detail}")))
            .unwrap_or_else(|| UiSlotShape::Product(value.to_string())),
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShapeFieldSummary(field: UiSlotShapeField) -> Element {
    let shape = field.shape.clone();
    let detail = shape_inline(&shape, 0);

    rsx! {
        p { class: "tw:m-0 tw:flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5 tw:text-xs tw:leading-snug",
            span { class: "tw:font-bold tw:text-subtle-foreground tw:break-words", "{field.label}:" }
            span { class: "tw:text-muted-foreground tw:break-words", "{detail}" }
        }
    }
}

fn compact_shape_class(mode: SlotShapeDisplayMode) -> &'static str {
    match mode {
        SlotShapeDisplayMode::Compact => {
            "tw:inline-flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5"
        }
        SlotShapeDisplayMode::CompactFriendly | SlotShapeDisplayMode::Verbose => {
            "tw:inline-flex tw:min-w-0 tw:flex-wrap tw:items-baseline tw:gap-x-1.5 tw:gap-y-0.5"
        }
    }
}

fn compact_shape_detail(shape: &UiSlotShape, mode: SlotShapeDisplayMode) -> Option<String> {
    match shape {
        UiSlotShape::Record(fields) if !fields.is_empty() => Some(record_inline(fields, 0)),
        UiSlotShape::Record(fields) => Some(field_count_label(fields.len())),
        _ if mode == SlotShapeDisplayMode::CompactFriendly => shape_hint(shape),
        _ => None,
    }
}

fn shape_label(shape: &UiSlotShape) -> String {
    match shape {
        UiSlotShape::Empty => "Empty".to_string(),
        UiSlotShape::Text => "Text".to_string(),
        UiSlotShape::Int32 => "Int32".to_string(),
        UiSlotShape::UInt32 => "UInt32".to_string(),
        UiSlotShape::Float32 => "Float32".to_string(),
        UiSlotShape::Bool => "Bool".to_string(),
        UiSlotShape::Vec2 => "Vec2".to_string(),
        UiSlotShape::Vec3 => "Vec3".to_string(),
        UiSlotShape::Vec4 => "Vec4".to_string(),
        UiSlotShape::IVec2 => "IVec2".to_string(),
        UiSlotShape::IVec3 => "IVec3".to_string(),
        UiSlotShape::IVec4 => "IVec4".to_string(),
        UiSlotShape::UVec2 => "UVec2".to_string(),
        UiSlotShape::UVec3 => "UVec3".to_string(),
        UiSlotShape::UVec4 => "UVec4".to_string(),
        UiSlotShape::BVec2 => "BVec2".to_string(),
        UiSlotShape::BVec3 => "BVec3".to_string(),
        UiSlotShape::BVec4 => "BVec4".to_string(),
        UiSlotShape::Mat2x2 => "Mat2x2".to_string(),
        UiSlotShape::Mat3x3 => "Mat3x3".to_string(),
        UiSlotShape::Mat4x4 => "Mat4x4".to_string(),
        UiSlotShape::Array => "Array".to_string(),
        UiSlotShape::Enum => "Enum".to_string(),
        UiSlotShape::Resource => "Resource".to_string(),
        UiSlotShape::Record(_) => "Record".to_string(),
        UiSlotShape::Asset(label) | UiSlotShape::Product(label) => label.clone(),
        UiSlotShape::ProducedValue => "Produced value".to_string(),
    }
}

fn shape_hint(shape: &UiSlotShape) -> Option<String> {
    match shape {
        UiSlotShape::Empty => Some("no authored value".to_string()),
        UiSlotShape::Text => Some("text or resource reference".to_string()),
        UiSlotShape::Int32 => Some("signed whole number, -2.1B to 2.1B".to_string()),
        UiSlotShape::UInt32 => Some("whole number, 0 to 4.29B".to_string()),
        UiSlotShape::Float32 => Some("32-bit decimal value".to_string()),
        UiSlotShape::Bool => Some("true or false".to_string()),
        UiSlotShape::Vec2 => Some("two Float32 values".to_string()),
        UiSlotShape::Vec3 => Some("three Float32 values".to_string()),
        UiSlotShape::Vec4 => Some("four Float32 values".to_string()),
        UiSlotShape::IVec2 => Some("two Int32 values".to_string()),
        UiSlotShape::IVec3 => Some("three Int32 values".to_string()),
        UiSlotShape::IVec4 => Some("four Int32 values".to_string()),
        UiSlotShape::UVec2 => Some("two UInt32 values".to_string()),
        UiSlotShape::UVec3 => Some("three UInt32 values".to_string()),
        UiSlotShape::UVec4 => Some("four UInt32 values".to_string()),
        UiSlotShape::BVec2 => Some("two Bool values".to_string()),
        UiSlotShape::BVec3 => Some("three Bool values".to_string()),
        UiSlotShape::BVec4 => Some("four Bool values".to_string()),
        UiSlotShape::Mat2x2 => Some("2 by 2 Float32 matrix".to_string()),
        UiSlotShape::Mat3x3 => Some("3 by 3 Float32 matrix".to_string()),
        UiSlotShape::Mat4x4 => Some("4 by 4 Float32 matrix".to_string()),
        UiSlotShape::Array => Some("array or list payload".to_string()),
        UiSlotShape::Enum => Some("active variant payload".to_string()),
        UiSlotShape::Resource => Some("store-backed resource reference".to_string()),
        UiSlotShape::Record(fields) => Some(field_count_label(fields.len())),
        UiSlotShape::Asset(_) => Some("file-backed authored content".to_string()),
        UiSlotShape::Product(_) => Some("node output product".to_string()),
        UiSlotShape::ProducedValue => Some("runtime output value".to_string()),
    }
}

fn shape_inline(shape: &UiSlotShape, depth: usize) -> String {
    match shape {
        UiSlotShape::Record(fields) if depth < 2 && !fields.is_empty() => {
            format!("Record {}", record_inline(fields, depth + 1))
        }
        _ => shape_label(shape),
    }
}

fn record_inline(fields: &[UiSlotShapeField], depth: usize) -> String {
    if fields.is_empty() {
        return field_count_label(0);
    }

    let entries = fields
        .iter()
        .map(|field| format!("{}: {}", field.label, shape_inline(&field.shape, depth + 1)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{ {entries} }}")
}

fn field_count_label(count: usize) -> String {
    if count == 1 {
        "1 field".to_string()
    } else {
        format!("{count} fields")
    }
}

fn record_fields(shape: &UiSlotShape) -> Vec<UiSlotShapeField> {
    match shape {
        UiSlotShape::Record(fields) => fields.clone(),
        _ => Vec::new(),
    }
}
