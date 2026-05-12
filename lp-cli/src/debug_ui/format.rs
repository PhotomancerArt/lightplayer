//! Formatting helpers for the temporary debug UI.

use lpc_model::{LpType, LpValue, ProductRef, ResourceRef};
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
        LpValue::Struct { name, fields } => {
            format!(
                "{} struct[{}]",
                name.as_deref().unwrap_or("anonymous"),
                fields.len()
            )
        }
        LpValue::Resource(value) => format_resource_ref(*value),
        LpValue::Product(value) => format_product_ref(*value),
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
            format!(
                "visual product node={} output={}",
                product.node().0,
                product.output()
            )
        }
        ProductRef::Control(product) => {
            let extent = product.preferred_extent();
            format!(
                "control product node={} output={} extent={}x{}",
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
        "{}  rev {}  {}  {}",
        format_resource_ref(summary.resource_ref),
        summary.revision.0,
        format_resource_kind(summary.kind),
        format_resource_availability(&summary.availability)
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
