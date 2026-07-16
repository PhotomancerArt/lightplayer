//! Side table of hand-derived schemas for the semantic slot codecs in
//! `crate::slots`.
//!
//! Each entry was written by reading the codec's actual read/write impl (and
//! its `FromLpValue` conversion, which the typed read path routes through via
//! `ValueSlot::<T>::set_lp_value`), and each is pinned by an accept/reject
//! conformance test at the bottom of this file. The table is keyed by
//! [`SlotShapeId`]: for [`SlotShape::Custom`](crate::SlotShape::Custom) leaves
//! that is the `codec` id, for semantic
//! [`SlotShape::Value`](crate::SlotShape::Value) leaves it is the
//! `SlotValueShape::id` (`<T as SlotValue>::SHAPE_ID`).
//!
//! Table caveat: `FromLpValue` validation only runs on the *typed* read path
//! (real artifact loads through `ValueSlot<T>` fields). The pure-dynamic
//! registry path stores plain `LpValue`s without conversion, so for the
//! entries that tighten beyond their `LpType` grammar (`ColorOrderValue`,
//! `RelativeNodeRef`) a dynamic read is looser than this schema. Authored
//! artifacts always read typed, so the table describes the authored contract.

use serde_json::{Value, json};

use crate::{
    Affine2d, ArtifactPath, ColorOrderValue, ControlProduct, Dim2u, PositiveF32, ProductKind,
    Ratio, RelativeNodeRef, RenderOrder, ResourceRef, SlotShapeId, SlotValue, VisualProduct, Xy,
};

use super::slot_shape_schema::{
    fixed_array, i32_schema, product_ref_schema, resource_ref_schema, u32_schema,
};

/// Look up the hand-derived schema for a semantic codec id.
///
/// Returns `None` for unknown ids; the compiler then falls back to the
/// declared inner shape. `NodeInvocation` intentionally has no entry — it is a
/// plain externally-tagged `SlotShape::Enum`, handled by the generic enum
/// compiler.
pub fn custom_codec_schema(id: SlotShapeId) -> Option<Value> {
    CUSTOM_CODEC_SCHEMAS
        .iter()
        .find(|(entry_id, _)| *entry_id == id)
        .map(|(_, schema_fn)| schema_fn())
}

/// One entry per semantic codec in `crate::slots`. Ids reference the source
/// `SHAPE_ID` constants so a renamed codec id cannot silently orphan its
/// schema.
pub(crate) const CUSTOM_CODEC_SCHEMAS: &[(SlotShapeId, fn() -> Value)] = &[
    (Xy::SHAPE_ID, xy_schema),
    (Affine2d::SHAPE_ID, affine2d_schema),
    (ColorOrderValue::SHAPE_ID, color_order_schema),
    (Dim2u::SHAPE_ID, dim2u_schema),
    (Ratio::SHAPE_ID, ratio_schema),
    (PositiveF32::SHAPE_ID, positive_f32_schema),
    (
        <alloc::vec::Vec<u32> as SlotValue>::SHAPE_ID,
        u32_list_schema,
    ),
    (RenderOrder::SHAPE_ID, render_order_schema),
    (ResourceRef::SHAPE_ID, resource_ref_slot_schema),
    (RelativeNodeRef::SHAPE_ID, relative_node_ref_schema),
    (ArtifactPath::SHAPE_ID, artifact_path_schema),
    (VisualProduct::SHAPE_ID, visual_product_schema),
    (ControlProduct::SHAPE_ID, control_product_schema),
    (crate::slots::ASSET_SLOT_CODEC_ID, asset_slot_schema),
];

/// `Xy([f32; 2])` reads as `LpType::Vec2`: exactly two numbers
/// (`ValueReader::f32_array::<2>`); `from_lp_value` adds no constraints.
fn xy_schema() -> Value {
    let mut schema = fixed_array(json!({ "type": "number" }), 2);
    describe(&mut schema, "2D XY coordinate as [x, y].");
    schema
}

/// `Affine2d` reads as `LpType::Mat3x3` (3 rows of 3 numbers); its
/// `from_lp_value` then requires the bottom row to be within 1e-5 of
/// `[0, 0, 1]`. An epsilon comparison is not expressible in JSON Schema, so
/// that constraint stays descriptive: the schema is looser than the codec for
/// perspective matrices.
fn affine2d_schema() -> Value {
    let mut schema = fixed_array(fixed_array(json!({ "type": "number" }), 3), 3);
    describe(
        &mut schema,
        "2D affine transform as a row-major 3x3 matrix; the bottom row must be \
         (approximately) [0, 0, 1] — perspective matrices are rejected on read.",
    );
    schema
}

/// `ColorOrderValue` reads as `LpType::String`, then `from_lp_value` requires
/// one of the six channel-order names (`ColorOrderValue::parse`).
fn color_order_schema() -> Value {
    json!({
        "description": "RGB channel order for fixture/output color packing.",
        "enum": ["rgb", "grb", "rbg", "gbr", "brg", "bgr"],
    })
}

/// `Dim2u` reads as a struct value: `read_lp_struct` requires both fields and
/// rejects unknown ones; each field parses as u32 number text.
fn dim2u_schema() -> Value {
    json!({
        "description": "Width/height in unsigned integer pixels or cells.",
        "type": "object",
        "properties": {
            "width": u32_schema(),
            "height": u32_schema(),
        },
        "required": ["width", "height"],
        "additionalProperties": false,
    })
}

/// `Ratio(f32)` reads as a bare number. The 0..=1 domain is an editor hint
/// (`slider(min = 0.0, max = 1.0)`) only — the derived `from_lp_value` does
/// not enforce it, so neither does the schema.
fn ratio_schema() -> Value {
    json!({
        "description": "Ratio in the intended 0.0..=1.0 domain (not enforced on read).",
        "type": "number",
    })
}

/// `PositiveF32(f32)` reads as a bare number. As with `Ratio`, the
/// non-negative domain is an editor hint (`number(min = 0.0)`); the codec
/// accepts negative values, so the schema must too.
fn positive_f32_schema() -> Value {
    json!({
        "description": "Intended non-negative float (not enforced on read).",
        "type": "number",
    })
}

/// `Vec<u32>` reads as `LpType::List(U32)`: any-length array of u32 number
/// text; `from_lp_value` re-checks that each element is a `U32`.
fn u32_list_schema() -> Value {
    json!({
        "type": "array",
        "items": u32_schema(),
    })
}

/// `RenderOrder(i32)` reads as i32 number text.
fn render_order_schema() -> Value {
    let mut schema = i32_schema();
    describe(&mut schema, "Render ordering value.");
    schema
}

/// `ResourceRef` reads as `LpType::Resource` (`read_resource_ref`);
/// `from_lp_value` only unwraps the already-typed value.
fn resource_ref_slot_schema() -> Value {
    resource_ref_schema()
}

/// `RelativeNodeRef` reads as `LpType::String`, then `from_lp_value` runs
/// `RelativeNodeRef::parse`: `.` alone, an optional leading `./`, then
/// `..` parent hops (only before any name) followed by slash-separated
/// `NodeName` segments (`[A-Za-z_][A-Za-z0-9_]*`). The pattern mirrors the
/// parser exactly except the u8 cap on parent hops.
fn relative_node_ref_schema() -> Value {
    json!({
        "description": "Relative node reference, e.g. \".\", \"..\", \"../texture\", \"child/grandchild\".",
        "type": "string",
        "pattern": "^(\\.|(\\./)?(\\.\\.(/\\.\\.)*(/[A-Za-z_][A-Za-z0-9_]*)*|[A-Za-z_][A-Za-z0-9_]*(/[A-Za-z_][A-Za-z0-9_]*)*))$",
    })
}

/// `ArtifactPath(String)` reads as any string; no further validation.
fn artifact_path_schema() -> Value {
    json!({
        "description": "Path to an authored artifact file.",
        "type": "string",
    })
}

/// `VisualProduct` reads as `LpType::Product(Visual)` (`read_product_ref`).
fn visual_product_schema() -> Value {
    product_ref_schema(ProductKind::Visual)
}

/// `ControlProduct` reads as `LpType::Product(Control)` (`read_product_ref`).
fn control_product_schema() -> Value {
    product_ref_schema(ProductKind::Control)
}

/// `AssetSlot` is the one true `SlotShape::Custom` codec
/// (`lp::slots::AssetSlotCodec`). Its reader (`asset_slot::read_value`)
/// accepts:
///
/// - a bare artifact spec string (`"shader.glsl"`, `"lib:core/x"`), or
/// - an object whose single property is `path` (or legacy `$path`) with a
///   string value; any other first property is rejected as an unsupported
///   inline body, and trailing properties fail `object.finish()`.
///
/// `ArtifactSpec::parse` additionally validates `lib:` suffixes, which a
/// schema cannot express; malformed `lib:` strings pass the schema but fail
/// the read.
fn asset_slot_schema() -> Value {
    json!({
        "description": "Asset reference: an artifact spec string, or { \"path\": <spec> }.",
        "anyOf": [
            { "type": "string" },
            {
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false,
            },
            {
                "type": "object",
                "properties": { "$path": { "type": "string" } },
                "required": ["$path"],
                "additionalProperties": false,
            },
        ],
    })
}

fn describe(schema: &mut Value, text: &str) {
    if let Some(obj) = schema.as_object_mut() {
        obj.insert(
            alloc::string::String::from("description"),
            Value::String(alloc::string::String::from(text)),
        );
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::schema_gen::test_support::{check_conformance, typed_read};
    use crate::slot::shape;
    use crate::slot_codec::SyntaxError;
    use crate::slots::node_invocation_slot::NodeInvocation;
    use crate::{
        Affine2d, ArtifactPath, AssetSlot, ColorOrderValue, ControlProduct, Dim2u, EnumSlot,
        FieldSlot, FieldSlotMut, NodeId, PositiveF32, Ratio, RelativeNodeRef, RenderOrder,
        ResourceRef, SlotEnumShape, SlotShape, SlotShapeRegistry, SlotValue, ValueSlot,
        VisualProduct, Xy,
    };

    use super::{CUSTOM_CODEC_SCHEMAS, custom_codec_schema};

    #[test]
    fn table_has_one_entry_per_semantic_codec() {
        // 13 semantic value leaves + the asset custom codec. NodeInvocation is
        // deliberately absent (plain external enum shape).
        assert_eq!(CUSTOM_CODEC_SCHEMAS.len(), 14);
        for (index, (id, _)) in CUSTOM_CODEC_SCHEMAS.iter().enumerate() {
            assert!(
                custom_codec_schema(*id).is_some(),
                "entry {index} not resolvable by id"
            );
            assert!(
                !CUSTOM_CODEC_SCHEMAS[index + 1..]
                    .iter()
                    .any(|(other, _)| other == id),
                "duplicate table id at entry {index}"
            );
        }
    }

    #[test]
    fn xy_accepts_two_number_arrays() {
        check_value_slot::<Xy>(
            &[r#"[1.0,2.0]"#, r#"[1,2]"#],
            &[r#"[1.0]"#, r#"[1,2,3]"#, r#"["1",2]"#, r#"{"x":1,"y":2}"#],
        );
    }

    #[test]
    fn affine2d_accepts_three_by_three_matrices() {
        check_value_slot::<Affine2d>(
            &[
                r#"[[1,0,0],[0,1,0],[0,0,1]]"#,
                r#"[[1,0.25,12],[-0.5,2,-8],[0,0,1]]"#,
            ],
            &[
                r#"[[1,0,0],[0,1,0]]"#,
                r#"[[1,0],[0,1],[0,0]]"#,
                r#"[1,0,0,0,1,0,0,0,1]"#,
                r#""identity""#,
            ],
        );
    }

    #[test]
    fn color_order_accepts_the_six_channel_orders() {
        check_value_slot::<ColorOrderValue>(
            &[r#""rgb""#, r#""grb""#, r#""bgr""#],
            &[r#""xyz""#, r#""GRB""#, r#"5"#, r#"["g","r","b"]"#],
        );
    }

    #[test]
    fn dim2u_requires_width_and_height() {
        check_value_slot::<Dim2u>(
            &[r#"{"width":64,"height":32}"#, r#"{"width":0,"height":0}"#],
            &[
                r#"{"width":64}"#,
                r#"{"width":64,"height":32,"depth":8}"#,
                r#"{"width":-1,"height":2}"#,
                r#"[64,32]"#,
            ],
        );
    }

    #[test]
    fn ratio_accepts_any_number() {
        // The 0..=1 domain is an editor hint only; the codec accepts any
        // number, so the schema must as well.
        check_value_slot::<Ratio>(&[r#"0.75"#, r#"0"#, r#"2.5"#, r#"-1.0"#], &[r#""0.75""#]);
    }

    #[test]
    fn positive_f32_accepts_any_number() {
        // Same story as Ratio: min = 0.0 is an editor hint, not read validation.
        check_value_slot::<PositiveF32>(&[r#"2.0"#, r#"-1.5"#], &[r#""2.0""#, r#"true"#]);
    }

    #[test]
    fn u32_list_accepts_u32_arrays() {
        check_value_slot::<Vec<u32>>(
            &[r#"[]"#, r#"[1,8,12]"#],
            &[r#"[1,-2]"#, r#"[1,"8"]"#, r#"[1.5]"#, r#"5"#],
        );
    }

    #[test]
    fn render_order_accepts_i32() {
        check_value_slot::<RenderOrder>(&[r#"10"#, r#"-10"#], &[r#"1.5"#, r#""10""#]);
    }

    #[test]
    fn resource_ref_accepts_domain_and_id() {
        check_value_slot::<ResourceRef>(
            &[
                r#"{"domain":"runtime_buffer","id":7}"#,
                r#"{"domain":"unset","id":0}"#,
                r#"{}"#,
            ],
            &[r#"{"domain":"bogus","id":7}"#, r#"{"buffer":7}"#, r#"7"#],
        );
    }

    #[test]
    fn relative_node_ref_accepts_dot_relative_paths() {
        check_typed(
            shape::leaf(RelativeNodeRef::value_shape()),
            |shape, text| {
                let mut slot = ValueSlot::new(RelativeNodeRef::current());
                typed_read(slot.slot_field_data_mut(), shape, text)
            },
            &[
                r#"".""#,
                r#""..""#,
                r#""../texture""#,
                r#""../../aunt/child""#,
                r#""texture""#,
                r#""./texture""#,
                r#""child/grandchild""#,
            ],
            &[
                // Absolute paths must be spelled relative.
                r#""/texture""#,
                r#""""#,
                r#""a//b""#,
                r#""child/""#,
                // Re-ascending after a name segment is malformed.
                r#""child/..""#,
                r#""..a..b""#,
                r#"".9lives""#,
                r#"42"#,
            ],
        );
    }

    #[test]
    fn artifact_path_accepts_any_string() {
        check_value_slot::<ArtifactPath>(&[r#""./shader.json""#, r#""""#], &[r#"42"#, r#"["a"]"#]);
    }

    #[test]
    fn visual_product_pins_kind_visual() {
        check_typed(
            shape::leaf(VisualProduct::value_shape()),
            |shape, text| {
                let mut slot = ValueSlot::new(VisualProduct::new(NodeId::new(0), 0));
                typed_read(slot.slot_field_data_mut(), shape, text)
            },
            &[
                r#"{"kind":"visual","node":2,"output":1}"#,
                r#"{"node":2}"#,
                r#"{}"#,
            ],
            &[r#"{"kind":"control","node":2,"output":1}"#, r#"{"port":1}"#],
        );
    }

    #[test]
    fn control_product_pins_kind_control() {
        check_typed(
            shape::leaf(ControlProduct::value_shape()),
            |shape, text| {
                let mut slot = ValueSlot::new(ControlProduct::new(
                    NodeId::new(0),
                    0,
                    crate::ControlExtent::default(),
                ));
                typed_read(slot.slot_field_data_mut(), shape, text)
            },
            &[
                r#"{"kind":"control","node":3,"output":2,"preferred_extent":{"rows":4,"samples_per_row":12}}"#,
                r#"{}"#,
            ],
            &[
                r#"{"kind":"visual","node":3}"#,
                r#"{"preferred_extent":{"rows":4,"samples":12}}"#,
            ],
        );
    }

    #[test]
    fn asset_slot_accepts_spec_string_and_path_objects() {
        check_typed(
            AssetSlot::slot_field_shape(),
            |shape, text| {
                let mut slot = AssetSlot::default();
                typed_read(slot.slot_field_data_mut(), shape, text)
            },
            &[
                r#""shader.glsl""#,
                r#""lib:core/visual/checkerboard""#,
                r#"{"path":"./shader.glsl"}"#,
                r#"{"$path":"./shader.glsl"}"#,
            ],
            &[
                // Inline bodies are unsupported.
                r#"{"glsl":"void main() {}"}"#,
                r#"{"extension":"png","bytes":[137,80,78,71]}"#,
                r#"{"path":"a.glsl","extra":1}"#,
                r#"{"path":42}"#,
                r#"42"#,
            ],
        );
    }

    /// Not a table entry: `NodeInvocation` is a plain externally-tagged enum
    /// shape and must round through the generic enum compiler.
    #[test]
    fn node_invocation_compiles_through_generic_enum_path() {
        check_typed(
            NodeInvocation::slot_enum_shape(),
            |shape, text| {
                let mut slot = EnumSlot::new(NodeInvocation::default());
                typed_read(slot.slot_field_data_mut(), shape, text)
            },
            &[
                r#"{"unset":{}}"#,
                r#"{"ref":"./texture.json"}"#,
                // Unit variant payloads are skipped by the reader, so any
                // value after "unset" is accepted.
                r#"{"unset":42}"#,
            ],
            &[
                r#"{}"#,
                r#"{"def":{"path":"./texture.json"}}"#,
                r#"{"artifact":"./texture.json"}"#,
                r#"{"unset":{},"ref":"./a.json"}"#,
                r#"{"ref":7}"#,
            ],
        );
    }

    // --- helpers -------------------------------------------------------------------------------

    /// Conformance check for a `ValueSlot<T>` semantic leaf through the typed
    /// read path (which routes through `T::from_lp_value`).
    fn check_value_slot<T>(accepted: &[&str], rejected: &[&str])
    where
        T: SlotValue + Default,
    {
        check_typed(
            shape::leaf(T::value_shape()),
            |shape, text| {
                let mut slot = ValueSlot::new(T::default());
                typed_read(slot.slot_field_data_mut(), shape, text)
            },
            accepted,
            rejected,
        );
    }

    fn check_typed(
        shape: SlotShape,
        mut read: impl FnMut(&SlotShape, &str) -> Result<(), SyntaxError>,
        accepted: &[&str],
        rejected: &[&str],
    ) {
        let registry = SlotShapeRegistry::default();
        check_conformance(
            &registry,
            &shape,
            |text| read(&shape, text),
            accepted,
            rejected,
        );
    }
}
