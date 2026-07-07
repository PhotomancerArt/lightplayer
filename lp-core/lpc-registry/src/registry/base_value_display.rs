//! Base (saved) value lookups and display strings against a parsed base def.
//!
//! The overlay annotation surfaces (mutation-ack effects and the overlay-read
//! response) both describe "the old value" of an edited slot as a plain
//! display string derived from the **base** (unoverlaid) definition. These
//! helpers take an already-parsed base [`NodeDef`] so a caller pays one base
//! parse per artifact regardless of how many paths it annotates (the
//! normalization path already holds the parse; the overlay read parses once
//! per overlaid artifact).

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::slot_codec::write_slot_subtree_json;
use lpc_model::{
    LpValue, NodeDef, ProductRef, ResourceRef, SlotDataAccess, SlotPath, SlotPathSegment,
    lookup_slot_data, lookup_slot_data_and_shape,
};

use crate::ParseCtx;

/// Character cap for composite (record/map/enum) base-value display strings:
/// longer compact-JSON renderings are cut to `COMPOSITE_DISPLAY_CAP - 1`
/// characters plus an ellipsis. Display-only best effort — never parsed back.
const COMPOSITE_DISPLAY_CAP: usize = 120;

/// Base (unoverlaid) value at `path` in `def`, or `None` when the path does
/// not resolve to a value leaf in the base definition.
///
/// Variant-prefixed paths (a leading segment naming the artifact root
/// variant) resolve only when the base definition is already that variant; a
/// variant-switching edit never equals base. Comparison at the caller is
/// exact [`LpValue`] equality — a near-miss float (`1.000_000_1` vs `1.0`)
/// stays an edit.
pub(crate) fn base_value_in_def(
    def: &NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
) -> Option<LpValue> {
    let path = strip_root_variant_prefix(def, path)?;
    match lookup_slot_data(def, ctx.shapes, &path).ok()? {
        SlotDataAccess::Value(value) => Some(value.value()),
        _ => None,
    }
}

/// Presence of the structural target at `path` in the base definition `def`.
///
/// - `true`: the base already satisfies the path — every segment resolves
///   (map keys present, options `Some`, enum variant segments matching the
///   active variant) and a path terminating at an option finds it `Some`.
///   An `EnsurePresent` here is a no-op vs base.
/// - `false`: the base does not contain the target — a segment fails to
///   resolve (map key absent, option `None`, inactive enum variant, a prefix
///   naming a variant other than the base's) or a terminal option is `None`.
///   A `Remove` here is a no-op vs base.
pub(crate) fn base_presence_in_def(def: &NodeDef, path: &SlotPath, ctx: &ParseCtx<'_>) -> bool {
    let Some(path) = strip_root_variant_prefix(def, path) else {
        return false;
    };
    let Ok(data) = lookup_slot_data(def, ctx.shapes, &path) else {
        return false;
    };
    match data {
        SlotDataAccess::Option(option) => option.data().is_some(),
        _ => true,
    }
}

/// Display string of the base definition's state at `path`, or `None` when
/// the target is absent in the base (client renders "—"/"not set").
///
/// Derivation rule (plan Q2): a value leaf formats with the same plain
/// [`LpValue`] display conventions the client DTOs use
/// ([`format_base_lp_value`]); a structural/composite target (record, map,
/// enum, present option body) renders as compact JSON via the dynamic slot
/// writer, capped at [`COMPOSITE_DISPLAY_CAP`] characters with an ellipsis;
/// an unresolvable path, a `None` option, a variant-prefix mismatch, or a
/// writer failure all degrade to `None` (best effort, never an error).
pub(crate) fn base_display_in_def(
    def: &NodeDef,
    path: &SlotPath,
    ctx: &ParseCtx<'_>,
) -> Option<String> {
    let path = strip_root_variant_prefix(def, path)?;
    let (data, shape) = lookup_slot_data_and_shape(def, ctx.shapes, &path).ok()?;
    match data {
        SlotDataAccess::Value(value) => Some(format_base_lp_value(&value.value())),
        SlotDataAccess::Option(option) if option.data().is_none() => None,
        data => {
            let json =
                write_slot_subtree_json(ctx.shapes, &shape.to_owned_shape(), data, Vec::new())
                    .ok()?;
            let json = String::from_utf8(json).ok()?;
            Some(cap_composite_display(json))
        }
    }
}

/// Resolve the leading artifact-root-variant segment against `def`'s active
/// variant: strip it when it matches, `None` when it names another variant
/// (the base does not contain the target), pass bare paths through.
fn strip_root_variant_prefix(def: &NodeDef, path: &SlotPath) -> Option<SlotPath> {
    match path.segments().split_first() {
        Some((SlotPathSegment::Field(name), tail)) if NodeDef::is_variant_name(name.as_str()) => {
            if def.variant_name() != name.as_str() {
                return None;
            }
            Some(SlotPath::from_segments(tail.to_vec()))
        }
        _ => Some(path.clone()),
    }
}

/// Cut a composite JSON rendering to the display cap (character-based so the
/// cut never lands inside a multi-byte character).
fn cap_composite_display(json: String) -> String {
    if json.chars().count() <= COMPOSITE_DISPLAY_CAP {
        return json;
    }
    let mut capped: String = json.chars().take(COMPOSITE_DISPLAY_CAP - 1).collect();
    capped.push('…');
    capped
}

/// Plain [`LpValue`] display formatting for base-value annotations.
///
/// Mirrors the client's DTO conventions
/// (`lpa-studio-core/src/app/project/project_value_format.rs`,
/// `format_lp_value`) so server-derived "old value" strings read exactly like
/// the client-formatted "new value" strings beside them. Keep the two in
/// sync until the convention gets a shared home.
fn format_base_lp_value(value: &LpValue) -> String {
    match value {
        LpValue::Unset => "unset".to_string(),
        LpValue::String(value) => value.clone(),
        LpValue::I32(value) => value.to_string(),
        LpValue::U32(value) => value.to_string(),
        LpValue::F32(value) => format_float(*value),
        LpValue::Bool(value) => value.to_string(),
        LpValue::Vec2(value) => format_float_array(value),
        LpValue::Vec3(value) => format_float_array(value),
        LpValue::Vec4(value) => format_float_array(value),
        LpValue::IVec2(value) => format_int_array(value),
        LpValue::IVec3(value) => format_int_array(value),
        LpValue::IVec4(value) => format_int_array(value),
        LpValue::UVec2(value) => format_int_array(value),
        LpValue::UVec3(value) => format_int_array(value),
        LpValue::UVec4(value) => format_int_array(value),
        LpValue::BVec2(value) => format_int_array(value),
        LpValue::BVec3(value) => format_int_array(value),
        LpValue::BVec4(value) => format_int_array(value),
        LpValue::Mat2x2(value) => format_matrix(value),
        LpValue::Mat3x3(value) => format_matrix(value),
        LpValue::Mat4x4(value) => format_matrix(value),
        LpValue::Array(values) => {
            let values = values
                .iter()
                .map(format_base_lp_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{values}]")
        }
        LpValue::Struct { name, fields } => {
            let fields = fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", format_base_lp_value(value)))
                .collect::<Vec<_>>()
                .join(", ");
            match name {
                Some(name) => format!("{name} {{ {fields} }}"),
                None => format!("{{ {fields} }}"),
            }
        }
        LpValue::Enum { variant, payload } => match payload {
            Some(payload) => format!("variant {variant}({})", format_base_lp_value(payload)),
            None => format!("variant {variant}"),
        },
        LpValue::Resource(resource) => format_resource_ref(*resource),
        LpValue::Product(product) => format_product_ref(*product),
    }
}

fn format_resource_ref(resource: ResourceRef) -> String {
    format!("resource {:?}:{}", resource.domain, resource.id)
}

fn format_product_ref(product: ProductRef) -> String {
    match product {
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

fn format_float(value: f32) -> String {
    if value.is_finite() {
        // `libm` stands in for the std float methods (`round`, `fract`):
        // lpc-registry is `no_std` and this runs on the firmware server.
        let rounded = libm::roundf(value * 1000.0) / 1000.0;
        if rounded == libm::truncf(rounded) {
            format!("{rounded:.1}")
        } else {
            rounded.to_string()
        }
    } else {
        value.to_string()
    }
}

fn format_float_array<const N: usize>(value: &[f32; N]) -> String {
    let values = value
        .iter()
        .map(|value| format_float(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({values})")
}

fn format_int_array<T: ToString, const N: usize>(value: &[T; N]) -> String {
    let values = value
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
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

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::SlotShapeRegistry;

    fn ctx_shapes() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn clock_def(json: &str) -> NodeDef {
        let shapes = ctx_shapes();
        NodeDef::read_json(&shapes, json).expect("parse clock def")
    }

    fn path(text: &str) -> SlotPath {
        SlotPath::parse(text).expect("slot path")
    }

    #[test]
    fn leaf_base_display_uses_plain_value_formatting() {
        let shapes = ctx_shapes();
        let ctx = ParseCtx { shapes: &shapes };
        let def = clock_def(r#"{ "kind": "Clock", "controls": { "rate": 2.5 } }"#);

        assert_eq!(
            base_display_in_def(&def, &path("controls.rate"), &ctx),
            Some("2.5".to_string())
        );
        assert_eq!(
            base_display_in_def(&def, &path("controls.running"), &ctx),
            Some("true".to_string()),
            "unauthored leaves display their shape default"
        );
    }

    #[test]
    fn composite_base_display_is_compact_json() {
        let shapes = ctx_shapes();
        let ctx = ParseCtx { shapes: &shapes };
        let json = r#"{
            "kind": "Fixture",
            "mapping": {
                "kind": "PathPoints",
                "paths": { "0": { "kind": "PointList", "first_channel": 5 } }
            }
        }"#;
        let def = NodeDef::read_json(&shapes, json).expect("parse fixture def");

        let display = base_display_in_def(&def, &path("mapping.PathPoints.paths[0]"), &ctx)
            .expect("composite display");

        assert!(display.starts_with('{'), "{display}");
        assert!(display.contains("PointList"), "{display}");
        assert!(display.contains("\"first_channel\":5"), "{display}");
    }

    #[test]
    fn absent_base_target_displays_none() {
        let shapes = ctx_shapes();
        let ctx = ParseCtx { shapes: &shapes };
        let def = clock_def(r#"{ "kind": "Clock" }"#);

        assert_eq!(
            base_display_in_def(&def, &path("bindings[speed]"), &ctx),
            None,
            "base-absent map entry has no old value"
        );
        assert_eq!(
            base_display_in_def(&def, &path("controls.bogus"), &ctx),
            None,
            "unresolvable path degrades to None"
        );
    }

    #[test]
    fn variant_prefix_resolves_only_against_the_base_variant() {
        let shapes = ctx_shapes();
        let ctx = ParseCtx { shapes: &shapes };
        let def = clock_def(r#"{ "kind": "Clock", "controls": { "rate": 4.0 } }"#);

        assert_eq!(
            base_display_in_def(&def, &path("Clock.controls.rate"), &ctx),
            Some("4.0".to_string())
        );
        assert_eq!(
            base_display_in_def(&def, &path("Fixture.color_order"), &ctx),
            None,
            "a prefix naming another variant is absent in base"
        );
        // Presence mirrors the same rule (a variant switch is a real diff).
        assert!(!base_presence_in_def(
            &def,
            &path("Fixture.color_order"),
            &ctx
        ));
        assert!(base_presence_in_def(
            &def,
            &path("Clock.controls.rate"),
            &ctx
        ));
    }

    #[test]
    fn long_composite_display_is_capped_with_ellipsis() {
        let shapes = ctx_shapes();
        let ctx = ParseCtx { shapes: &shapes };
        // A fixture mapping with enough map entries to overflow the cap.
        let entries = (0..40)
            .map(|index| {
                format!("\"{index}\": {{ \"kind\": \"PointList\", \"first_channel\": {index} }}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let json = format!(
            r#"{{ "kind": "Fixture", "mapping": {{ "kind": "PathPoints", "paths": {{ {entries} }} }} }}"#
        );
        let def = NodeDef::read_json(&shapes, &json).expect("parse fixture def");

        let display =
            base_display_in_def(&def, &path("mapping.PathPoints.paths"), &ctx).expect("display");

        assert_eq!(display.chars().count(), COMPOSITE_DISPLAY_CAP);
        assert!(display.ends_with('…'), "{display}");
    }

    #[test]
    fn base_value_resolves_leaves_only() {
        let shapes = ctx_shapes();
        let ctx = ParseCtx { shapes: &shapes };
        let def = clock_def(r#"{ "kind": "Clock", "controls": { "rate": 2.0 } }"#);

        assert_eq!(
            base_value_in_def(&def, &path("controls.rate"), &ctx),
            Some(LpValue::F32(2.0))
        );
        assert_eq!(base_value_in_def(&def, &path("controls"), &ctx), None);
    }

    #[test]
    fn format_matches_client_display_conventions() {
        assert_eq!(format_base_lp_value(&LpValue::Bool(true)), "true");
        assert_eq!(format_base_lp_value(&LpValue::F32(0.33333334)), "0.333");
        assert_eq!(format_base_lp_value(&LpValue::F32(2.0)), "2.0");
        assert_eq!(
            format_base_lp_value(&LpValue::Vec3([1.0, 2.5, 3.0])),
            "(1.0, 2.5, 3.0)"
        );
        assert_eq!(
            format_base_lp_value(&LpValue::String("rgb".to_string())),
            "rgb"
        );
    }

    #[test]
    fn cap_is_character_safe() {
        let long: String = "é".repeat(COMPOSITE_DISPLAY_CAP + 10);
        let capped = cap_composite_display(long);
        assert_eq!(capped.chars().count(), COMPOSITE_DISPLAY_CAP);
        assert!(capped.ends_with('…'));
    }
}
