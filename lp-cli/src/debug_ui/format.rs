//! Formatting helpers for the temporary debug UI.

use lpc_model::{LpType, LpValue, ProductRef, ResourceRef, ValueEditorHint};
use lpc_wire::{
    WireResourceAvailability, WireResourceKindSummary, WireResourceMetadataSummary,
    WireResourceSummary, WireRuntimeBufferKind,
};

pub(crate) fn format_lp_value(value: &LpValue) -> String {
    match value {
        LpValue::String(value) => format!("{value:?}"),
        LpValue::I32(value) => value.to_string(),
        LpValue::U32(value) => value.to_string(),
        LpValue::F32(value) => format!("{value:.3}"),
        LpValue::Bool(value) => value.to_string(),
        LpValue::Vec2(value) => format!("[{:.3}, {:.3}]", value[0], value[1]),
        LpValue::Vec3(value) => format!("[{:.3}, {:.3}, {:.3}]", value[0], value[1], value[2]),
        LpValue::Vec4(value) => format!(
            "[{:.3}, {:.3}, {:.3}, {:.3}]",
            value[0], value[1], value[2], value[3]
        ),
        LpValue::IVec2(value) => format!("{value:?}"),
        LpValue::IVec3(value) => format!("{value:?}"),
        LpValue::IVec4(value) => format!("{value:?}"),
        LpValue::UVec2(value) => format!("{value:?}"),
        LpValue::UVec3(value) => format!("{value:?}"),
        LpValue::UVec4(value) => format!("{value:?}"),
        LpValue::BVec2(value) => format!("{value:?}"),
        LpValue::BVec3(value) => format!("{value:?}"),
        LpValue::BVec4(value) => format!("{value:?}"),
        LpValue::Mat2x2(value) => format!("{value:?}"),
        LpValue::Mat3x3(value) => format!("{value:?}"),
        LpValue::Mat4x4(value) => format!("{value:?}"),
        LpValue::Array(values) => format!("array[{}]", values.len()),
        LpValue::Struct { name, fields } => format_struct_value(name.as_deref(), fields),
        LpValue::Resource(value) => format_resource_ref(*value),
        LpValue::Product(value) => format_product_ref(*value),
    }
}

pub(crate) fn format_value_editor_hint(editor: &ValueEditorHint) -> Option<String> {
    match editor {
        ValueEditorHint::Plain => None,
        ValueEditorHint::NodeRef => Some(String::from("node ref")),
        ValueEditorHint::Path => Some(String::from("path")),
        ValueEditorHint::Number { min, max, step } => {
            let mut parts = Vec::new();
            if let Some(min) = min {
                parts.push(format!("min {}", min.0));
            }
            if let Some(max) = max {
                parts.push(format!("max {}", max.0));
            }
            if let Some(step) = step {
                parts.push(format!("step {}", step.0));
            }
            Some(if parts.is_empty() {
                String::from("number")
            } else {
                format!("number {}", parts.join(", "))
            })
        }
        ValueEditorHint::Slider { min, max, step } => {
            let step = step
                .map(|step| format!(", step {}", step.0))
                .unwrap_or_default();
            Some(format!("slider {}..{}{step}", min.0, max.0))
        }
        ValueEditorHint::Xy => Some(String::from("xy")),
        ValueEditorHint::Dimensions => Some(String::from("dimensions")),
        ValueEditorHint::Affine2d => Some(String::from("affine 2d")),
        ValueEditorHint::Resource => Some(String::from("resource")),
        ValueEditorHint::RuntimeBufferResource => Some(String::from("runtime buffer")),
        ValueEditorHint::VisualProduct => Some(String::from("visual product")),
        ValueEditorHint::ControlProduct => Some(String::from("control product")),
        ValueEditorHint::Dropdown { options } => Some(format!("dropdown[{}]", options.len())),
    }
}

pub(crate) fn format_lp_type(ty: &LpType) -> String {
    match ty {
        LpType::String => String::from("string"),
        LpType::I32 => String::from("i32"),
        LpType::U32 => String::from("u32"),
        LpType::F32 => String::from("f32"),
        LpType::Bool => String::from("bool"),
        LpType::Vec2 => String::from("vec2"),
        LpType::Vec3 => String::from("vec3"),
        LpType::Vec4 => String::from("vec4"),
        LpType::IVec2 => String::from("ivec2"),
        LpType::IVec3 => String::from("ivec3"),
        LpType::IVec4 => String::from("ivec4"),
        LpType::UVec2 => String::from("uvec2"),
        LpType::UVec3 => String::from("uvec3"),
        LpType::UVec4 => String::from("uvec4"),
        LpType::BVec2 => String::from("bvec2"),
        LpType::BVec3 => String::from("bvec3"),
        LpType::BVec4 => String::from("bvec4"),
        LpType::Mat2x2 => String::from("mat2x2"),
        LpType::Mat3x3 => String::from("mat3x3"),
        LpType::Mat4x4 => String::from("mat4x4"),
        LpType::Array(item, len) => format!("[{}; {len}]", format_lp_type(item)),
        LpType::List(item) => format!("list<{}>", format_lp_type(item)),
        LpType::Struct { name, fields } => format!(
            "{} struct[{}]",
            name.as_deref().unwrap_or("anonymous"),
            fields.len()
        ),
        LpType::Resource => String::from("resource"),
        LpType::Product(kind) => format!("product::{kind:?}"),
    }
}

pub(crate) fn format_resource_ref(resource_ref: ResourceRef) -> String {
    format!("resource {:?}/{}", resource_ref.domain, resource_ref.id)
}

pub(crate) fn format_product_ref(product: ProductRef) -> String {
    match product {
        ProductRef::Visual(product) => {
            format!("visual product #{}:{}", product.node().0, product.output())
        }
        ProductRef::Control(product) => {
            let extent = product.preferred_extent();
            format!(
                "control product #{}:{}  {}x{}",
                product.node().0,
                product.output(),
                extent.rows,
                extent.samples_per_row
            )
        }
    }
}

pub(crate) fn format_resource_summary(summary: &WireResourceSummary) -> String {
    format!(
        "{}  rev {}  {}  {}{}",
        format_resource_ref(summary.resource_ref),
        summary.revision.0,
        format_resource_kind(summary.kind),
        format_resource_availability(&summary.availability),
        summary
            .owner
            .map(|owner| format!("  owner #{}", owner.0))
            .unwrap_or_default()
    )
}

pub(crate) fn format_resource_metadata(metadata: &WireResourceMetadataSummary) -> String {
    match metadata {
        WireResourceMetadataSummary::Texture {
            width,
            height,
            format,
        } => format!("texture {width}x{height} {format:?}"),
        WireResourceMetadataSummary::FixtureColors { channels, layout } => {
            format!("fixture colors channels={channels} {layout:?}")
        }
        WireResourceMetadataSummary::OutputChannels {
            channels,
            sample_format,
        } => format!("output channels={channels} {sample_format:?}"),
        WireResourceMetadataSummary::Raw => String::from("raw"),
    }
}

fn format_resource_kind(kind: WireResourceKindSummary) -> &'static str {
    match kind {
        WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::Texture) => "texture buffer",
        WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::FixtureColors) => {
            "fixture color buffer"
        }
        WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::OutputChannels) => {
            "output channel buffer"
        }
        WireResourceKindSummary::RuntimeBuffer(WireRuntimeBufferKind::Raw) => "raw buffer",
    }
}

fn format_resource_availability(availability: &WireResourceAvailability) -> String {
    match availability {
        WireResourceAvailability::Available => String::from("available"),
        WireResourceAvailability::Pending => String::from("pending"),
        WireResourceAvailability::NotFound => String::from("not found"),
        WireResourceAvailability::Error(message) => format!("error: {message}"),
    }
}

fn format_struct_value(name: Option<&str>, fields: &[(String, LpValue)]) -> String {
    match name {
        Some("BindingDef") => format_binding_def(fields),
        Some(name) => format!("{name} {{ {} }}", format_struct_fields(fields, 3)),
        None => format!("{{ {} }}", format_struct_fields(fields, 3)),
    }
}

fn format_binding_def(fields: &[(String, LpValue)]) -> String {
    let direction = struct_string_field(fields, "direction").unwrap_or("binding");
    let endpoint = struct_string_field(fields, "endpoint").unwrap_or("<invalid>");
    format!("{direction} {endpoint}")
}

fn struct_string_field<'a>(fields: &'a [(String, LpValue)], name: &str) -> Option<&'a str> {
    fields.iter().find_map(|(field, value)| {
        (field == name)
            .then_some(value)
            .and_then(|value| match value {
                LpValue::String(value) => Some(value.as_str()),
                _ => None,
            })
    })
}

fn format_struct_fields(fields: &[(String, LpValue)], limit: usize) -> String {
    let mut parts = fields
        .iter()
        .take(limit)
        .map(|(name, value)| format!("{name}: {}", format_lp_value(value)))
        .collect::<Vec<_>>();
    if fields.len() > limit {
        parts.push(format!("+{} more", fields.len() - limit));
    }
    parts.join(", ")
}
