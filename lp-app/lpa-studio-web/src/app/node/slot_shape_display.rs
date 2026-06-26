//! Shared presentation for Studio slot shape metadata.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotShape, UiSlotShapeField};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
