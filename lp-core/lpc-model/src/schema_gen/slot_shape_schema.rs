//! The shape → JSON Schema compiler.
//!
//! Every mapping decision below is derived from the corresponding reader code
//! in `crate::slot_codec` (cited inline), not from how the types "ought" to
//! serialize. When the reader and an intuitive schema disagree, the reader
//! wins, because the compiled schema's contract is "accepted by `read_json`
//! implies valid against the schema".

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde_json::{Map, Value, json};

use crate::{
    LpType, ModelEnumVariant, ModelStructMember, ProductKind, SlotEnumEncoding, SlotFieldShape,
    SlotMapKeyShape, SlotMeta, SlotName, SlotShape, SlotShapeId, SlotShapeRegistry, SlotValueShape,
    SlotVariantShape,
};

use super::custom_codec_schemas::custom_codec_schema;

/// `$defs` entry name for the recursive `LpType::Any` value schema.
const ANY_DEF_NAME: &str = "LpAnyValue";

/// Bound on `Ref → Ref → …` indirection so a malformed self-referential chain
/// of pure refs degrades to an "unresolved" schema instead of hanging.
const MAX_REF_HOPS: usize = 64;

/// Compile a slot shape (plus the registry backing its `Ref`s) into a JSON
/// Schema document (draft 2020-12) as a `serde_json::Value`.
///
/// Referenced registry shapes become `$defs` entries keyed by the registry's
/// human shape *name* (stable across builds), falling back to the hex id for
/// unnamed shapes. Compilation is infallible by design: a dangling `Ref` makes
/// the reader fail on every instance, so it compiles to a `false`-equivalent
/// schema (`"not": {}`) — an empty accept set keeps the "accepted implies
/// valid" direction vacuously true while staying visible in the output.
pub fn compile_slot_shape_schema(registry: &SlotShapeRegistry, shape: &SlotShape) -> Value {
    let mut compiler = SchemaCompiler::new(registry);
    let root = compiler.compile(shape);
    compiler.finish(root)
}

/// Compile a registered shape by id. Returns `None` when the id is unknown.
pub fn compile_registered_slot_shape_schema(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
) -> Option<Value> {
    registry
        .get(&id)
        .map(|shape| compile_slot_shape_schema(registry, shape))
}

struct SchemaCompiler<'r> {
    registry: &'r SlotShapeRegistry,
    /// Collected `$defs` bodies, keyed by def name. `serde_json::Map` is
    /// BTree-backed here (no `preserve_order` feature), so output key order is
    /// deterministic for free.
    defs: Map<String, Value>,
    /// `Ref` targets that already have a def name assigned. Names are claimed
    /// *before* the target body is compiled so recursive shapes terminate.
    def_names: Vec<(SlotShapeId, String)>,
    /// All claimed def names, for collision-proofing name fallbacks.
    taken_names: BTreeSet<String>,
}

impl<'r> SchemaCompiler<'r> {
    fn new(registry: &'r SlotShapeRegistry) -> Self {
        let mut taken_names = BTreeSet::new();
        // Reserved for the LpType::Any schema; a registry shape that happens to
        // carry the same name gets disambiguated with its id suffix.
        taken_names.insert(String::from(ANY_DEF_NAME));
        Self {
            registry,
            defs: Map::new(),
            def_names: Vec::new(),
            taken_names,
        }
    }

    /// Attach the standard envelope (`$schema`, collected `$defs`) to the
    /// compiled root schema.
    fn finish(self, mut root: Value) -> Value {
        if let Some(obj) = root.as_object_mut() {
            obj.insert(
                String::from("$schema"),
                json!("https://json-schema.org/draft/2020-12/schema"),
            );
            if !self.defs.is_empty() {
                obj.insert(String::from("$defs"), Value::Object(self.defs));
            }
        }
        root
    }

    fn compile(&mut self, shape: &SlotShape) -> Value {
        match shape {
            SlotShape::Ref { id } => self.compile_ref(*id),
            // `apply_reader_to_slot`'s `Unit` arm calls `value.skip_value()`,
            // so a unit slot accepts *any* JSON value and ignores it; the
            // writer emits `{}`. The empty (all-accepting) schema mirrors the
            // reader, not the writer.
            SlotShape::Unit { meta } => with_meta(
                json!({
                    "description":
                        "Unit slot: any value is accepted and ignored on read; written as {}.",
                }),
                meta,
            ),
            SlotShape::Value { shape } => self.compile_value(shape),
            SlotShape::Record { meta, fields } => self.compile_record(meta, fields),
            SlotShape::Map { meta, key, value } => self.compile_map(meta, *key, value),
            SlotShape::Enum {
                meta,
                encoding,
                variants,
            } => {
                let schema = match encoding {
                    SlotEnumEncoding::Tagged { field } => self.compile_tagged_enum(field, variants),
                    SlotEnumEncoding::External => self.compile_external_enum(variants),
                };
                with_meta(schema, meta)
            }
            // `read_option` unconditionally materializes the Some payload and
            // reads the inner shape; there is no `null` branch in the reader
            // (an explicit `null` fails the inner read), and None is encoded
            // as *absence* at the container above. So an option is exactly its
            // inner schema.
            SlotShape::Option { some, .. } => self.compile(some),
            // A custom codec owns its authored syntax entirely; the declared
            // inner `shape` is only the runtime snapshot shape. Prefer the
            // hand-derived side-table schema, fall back to the inner shape for
            // codecs the table does not know (future codecs should add an
            // entry — the conformance tests keep the table honest).
            SlotShape::Custom {
                meta, codec, shape, ..
            } => match custom_codec_schema(*codec) {
                Some(schema) => with_meta(schema, meta),
                None => with_meta(self.compile(shape), meta),
            },
        }
    }

    /// Value leaves: a semantic leaf id (e.g. `ColorOrderValue`) implies
    /// `FromLpValue` validation on the typed read path, expressed by the
    /// custom-codec side table; otherwise the accepted syntax is purely the
    /// structural `LpType` grammar of `read_lp_value`.
    fn compile_value(&mut self, shape: &SlotValueShape) -> Value {
        let schema = match custom_codec_schema(shape.id) {
            Some(schema) => schema,
            None => self.lp_type_schema(&shape.ty),
        };
        with_meta(schema, &shape.meta)
    }

    fn compile_ref(&mut self, id: SlotShapeId) -> Value {
        if let Some((_, name)) = self.def_names.iter().find(|(known, _)| *known == id) {
            return json!({ "$ref": format!("#/$defs/{name}") });
        }
        let registry = self.registry;
        let Some(target) = registry.get(&id) else {
            return unresolved_ref_schema(id);
        };
        let name = self.claim_def_name(id);
        let body = self.compile(target);
        self.defs.insert(name.clone(), body);
        json!({ "$ref": format!("#/$defs/{name}") })
    }

    /// Pick a stable `$defs` key for a registered shape: the registry's human
    /// name when present, else the (stable) hex id. Names are only a debug
    /// convention, so a duplicate falls back to a name+id composite instead of
    /// clobbering an existing def.
    fn claim_def_name(&mut self, id: SlotShapeId) -> String {
        let base = self
            .registry
            .entry(&id)
            .and_then(|entry| entry.name())
            .map(String::from)
            .unwrap_or_else(|| format!("shape_{id}"));
        let name = if self.taken_names.contains(&base) {
            format!("{base}_{id}")
        } else {
            base
        };
        self.taken_names.insert(name.clone());
        self.def_names.push((id, name.clone()));
        name
    }

    /// Records: `read_record_object` rejects unknown property names
    /// (`unknown_field`), hence `additionalProperties: false`. It does NOT
    /// require any field — missing fields keep their factory defaults (see
    /// `dynamic_slot_reader_leaves_missing_fields_at_defaults`) — so there is
    /// deliberately no `required` list, not even for non-`Option` fields.
    fn compile_record(&mut self, meta: &SlotMeta, fields: &[SlotFieldShape]) -> Value {
        let mut properties = Map::new();
        for field in fields {
            properties.insert(
                String::from(field.name.as_str()),
                self.compile(&field.shape),
            );
        }
        with_meta(
            json!({
                "type": "object",
                "properties": properties,
                "additionalProperties": false,
            }),
            meta,
        )
    }

    /// Maps: keys arrive as JSON object property names and integer keys are
    /// parsed with Rust `str::parse` (`parse_map_key`), which accepts optional
    /// sign plus ASCII digits (and leading zeros). Range overflow fails
    /// `parse` but is not expressible in a `propertyNames` pattern, so the
    /// schema is slightly looser than the reader for out-of-range keys.
    fn compile_map(&mut self, meta: &SlotMeta, key: SlotMapKeyShape, value: &SlotShape) -> Value {
        let value_schema = self.compile(value);
        let mut schema = json!({
            "type": "object",
            "additionalProperties": value_schema,
        });
        let pattern = match key {
            SlotMapKeyShape::String => None,
            SlotMapKeyShape::I32 => Some("^[+-]?[0-9]+$"),
            // `u32::from_str` accepts a leading `+` but not `-`.
            SlotMapKeyShape::U32 => Some("^\\+?[0-9]+$"),
        };
        if let (Some(pattern), Some(obj)) = (pattern, schema.as_object_mut()) {
            obj.insert(String::from("propertyNames"), json!({ "pattern": pattern }));
        }
        with_meta(schema, meta)
    }

    /// Tagged enums: one `oneOf` branch per variant, pinning the discriminator
    /// property to the variant name with the payload's record fields flattened
    /// beside it (`read_tagged_enum_object` reads the remaining properties
    /// straight into the variant's record).
    ///
    /// Reader strictnesses mirrored here: the variant name comparison is
    /// case-sensitive exact match against the registered spelling
    /// (`expected.contains(&actual)`), and unknown/extra properties are
    /// rejected. Not expressible: `expect_discriminator` additionally requires
    /// the tag to be the *first* property of the object.
    fn compile_tagged_enum(&mut self, field: &SlotName, variants: &[SlotVariantShape]) -> Value {
        let branches: Vec<Value> = variants
            .iter()
            .map(|variant| self.compile_tagged_variant(field, variant))
            .collect();
        json!({ "oneOf": branches })
    }

    fn compile_tagged_variant(&mut self, field: &SlotName, variant: &SlotVariantShape) -> Value {
        let payload = match resolve_refs(self.registry, &variant.shape) {
            Resolved::Shape(shape) => shape,
            Resolved::Dangling(id) => return unresolved_ref_schema(id),
        };
        let mut properties = Map::new();
        properties.insert(
            String::from(field.as_str()),
            json!({ "const": variant.name.as_str() }),
        );
        match payload {
            SlotShape::Record { fields, .. } => {
                for record_field in fields {
                    properties.insert(
                        String::from(record_field.name.as_str()),
                        self.compile(&record_field.shape),
                    );
                }
            }
            // Unit payload: `read_enum_payload_object` calls `object.finish()`,
            // which rejects any property after the discriminator.
            SlotShape::Unit { .. } => {}
            // `read_enum_payload_object` errors for every other payload kind
            // ("dynamic enum reader only supports record and unit variant
            // payloads"), so this branch accepts nothing.
            _ => {
                return json!({
                    "not": {},
                    "description": format!(
                        "variant {:?} has a payload the dynamic enum reader does not support \
                         (only record and unit payloads are readable)",
                        variant.name.as_str()
                    ),
                });
            }
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": [field.as_str()],
            "additionalProperties": false,
        })
    }

    /// External enums: `read_external_enum_object` requires exactly one
    /// property whose name is the (case-sensitive) variant name and whose
    /// value is the variant payload; a second property is rejected. `required`
    /// plus `additionalProperties: false` expresses "exactly this one
    /// property".
    fn compile_external_enum(&mut self, variants: &[SlotVariantShape]) -> Value {
        let branches: Vec<Value> = variants
            .iter()
            .map(|variant| {
                let payload = self.compile(&variant.shape);
                let mut properties = Map::new();
                properties.insert(String::from(variant.name.as_str()), payload);
                json!({
                    "type": "object",
                    "properties": properties,
                    "required": [variant.name.as_str()],
                    "additionalProperties": false,
                })
            })
            .collect();
        json!({ "oneOf": branches })
    }

    /// The one `LpType` → schema mapping, derived from `read_lp_value` in
    /// `slot_codec/slot_value_codec.rs` (see per-arm citations).
    fn lp_type_schema(&mut self, ty: &LpType) -> Value {
        match ty {
            LpType::Any => self.any_value_schema(),
            LpType::String => json!({ "type": "string" }),
            // Numbers are read as raw JSON number *text* and `str::parse`d,
            // so `1.5` fails i32/u32 while any parseable number passes f32
            // (`ValueReader::{i32,u32,f32}`). JSON Schema `integer` is close
            // but slightly looser: it also admits `1.0`/`1e2`, whose text
            // fails Rust integer parse.
            LpType::I32 => i32_schema(),
            LpType::U32 => u32_schema(),
            LpType::F32 => json!({ "type": "number" }),
            LpType::Bool => json!({ "type": "boolean" }),
            // Vectors are fixed-length flat arrays (`read_f32_array` /
            // `read_copy_array`): element count must match exactly.
            LpType::Vec2 => fixed_array(json!({ "type": "number" }), 2),
            LpType::Vec3 => fixed_array(json!({ "type": "number" }), 3),
            LpType::Vec4 => fixed_array(json!({ "type": "number" }), 4),
            LpType::IVec2 => fixed_array(i32_schema(), 2),
            LpType::IVec3 => fixed_array(i32_schema(), 3),
            LpType::IVec4 => fixed_array(i32_schema(), 4),
            LpType::UVec2 => fixed_array(u32_schema(), 2),
            LpType::UVec3 => fixed_array(u32_schema(), 3),
            LpType::UVec4 => fixed_array(u32_schema(), 4),
            LpType::BVec2 => fixed_array(json!({ "type": "boolean" }), 2),
            LpType::BVec3 => fixed_array(json!({ "type": "boolean" }), 3),
            LpType::BVec4 => fixed_array(json!({ "type": "boolean" }), 4),
            // Matrices are row-major nested arrays (`read_matrix`): N rows of
            // N numbers.
            LpType::Mat2x2 => matrix_schema(2),
            LpType::Mat3x3 => matrix_schema(3),
            LpType::Mat4x4 => matrix_schema(4),
            LpType::Array(item, len) => fixed_array(self.lp_type_schema(item), *len),
            LpType::List(item) => json!({
                "type": "array",
                "items": self.lp_type_schema(item),
            }),
            LpType::Struct { fields, .. } => self.lp_struct_schema(fields),
            LpType::Enum { variants, .. } => self.lp_enum_schema(variants),
            LpType::Resource => resource_ref_schema(),
            LpType::Product(kind) => product_ref_schema(*kind),
        }
    }

    /// Value structs: unlike record *slots* (whose missing fields keep
    /// defaults), `read_lp_struct` demands every declared field
    /// (`missing_required_field`) and rejects unknown ones, so all fields are
    /// `required`. The struct *name* never appears in the JSON.
    fn lp_struct_schema(&mut self, fields: &[ModelStructMember]) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for field in fields {
            properties.insert(field.name.clone(), self.lp_type_schema(&field.ty));
            required.push(Value::String(field.name.clone()));
        }
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
    }

    /// Value enums: `read_lp_enum` hardcodes a `kind` discriminator (this is
    /// independent of `SlotEnumEncoding`) followed by a `payload` property
    /// that is *required* for payload variants and *forbidden* for unit
    /// variants. As with tagged slot enums, "kind must come first" is not
    /// expressible.
    fn lp_enum_schema(&mut self, variants: &[ModelEnumVariant]) -> Value {
        let branches: Vec<Value> = variants
            .iter()
            .map(|variant| match &variant.payload {
                Some(payload_ty) => {
                    let payload = self.lp_type_schema(payload_ty);
                    json!({
                        "type": "object",
                        "properties": {
                            "kind": { "const": variant.name },
                            "payload": payload,
                        },
                        "required": ["kind", "payload"],
                        "additionalProperties": false,
                    })
                }
                None => json!({
                    "type": "object",
                    "properties": { "kind": { "const": variant.name } },
                    "required": ["kind"],
                    "additionalProperties": false,
                }),
            })
            .collect();
        json!({ "oneOf": branches })
    }

    /// `LpType::Any` values are read by `ValueReader::lp_value`, which accepts
    /// scalar strings, numbers, and bools plus arbitrarily nested arrays of
    /// those — and rejects objects and `null` ("expected lp value"). The
    /// recursion needs a named def.
    fn any_value_schema(&mut self) -> Value {
        if !self.defs.contains_key(ANY_DEF_NAME) {
            self.defs.insert(
                String::from(ANY_DEF_NAME),
                json!({
                    "description":
                        "Dynamically typed value: string, number, bool, or an array of \
                         these (objects and null are rejected by the reader).",
                    "anyOf": [
                        { "type": "string" },
                        { "type": "number" },
                        { "type": "boolean" },
                        { "type": "array", "items": { "$ref": "#/$defs/LpAnyValue" } },
                    ],
                }),
            );
        }
        json!({ "$ref": "#/$defs/LpAnyValue" })
    }
}

/// Follow `Ref` indirection to a concrete shape (bounded against cycles).
enum Resolved<'s> {
    Shape(&'s SlotShape),
    Dangling(SlotShapeId),
}

fn resolve_refs<'s>(registry: &'s SlotShapeRegistry, shape: &'s SlotShape) -> Resolved<'s> {
    let mut current = shape;
    for _ in 0..MAX_REF_HOPS {
        let SlotShape::Ref { id } = current else {
            return Resolved::Shape(current);
        };
        match registry.get(id) {
            Some(next) => current = next,
            None => return Resolved::Dangling(*id),
        }
    }
    match current {
        SlotShape::Ref { id } => Resolved::Dangling(*id),
        other => Resolved::Shape(other),
    }
}

fn unresolved_ref_schema(id: SlotShapeId) -> Value {
    // The reader errors on every instance for a dangling ref, so the honest
    // schema admits nothing (`"not": {}` is the annotatable spelling of
    // `false`).
    json!({
        "not": {},
        "description": format!("unresolved slot shape reference {id}"),
    })
}

/// Copy `SlotMeta` presentation text onto a schema object without overriding
/// text a more specific mapping already provided.
fn with_meta(mut schema: Value, meta: &SlotMeta) -> Value {
    let Some(obj) = schema.as_object_mut() else {
        return schema;
    };
    if let Some(description) = &meta.description
        && !obj.contains_key("description")
    {
        obj.insert(String::from("description"), json!(description));
    }
    if let Some(label) = &meta.label
        && !obj.contains_key("title")
    {
        obj.insert(String::from("title"), json!(label));
    }
    schema
}

pub(crate) fn fixed_array(item: Value, len: usize) -> Value {
    json!({
        "type": "array",
        "items": item,
        "minItems": len,
        "maxItems": len,
    })
}

fn matrix_schema(n: usize) -> Value {
    fixed_array(fixed_array(json!({ "type": "number" }), n), n)
}

pub(crate) fn i32_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": i64::from(i32::MIN),
        "maximum": i64::from(i32::MAX),
    })
}

pub(crate) fn u32_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": 0,
        "maximum": i64::from(u32::MAX),
    })
}

/// Mirrors `read_resource_ref`: both fields are optional (the reader starts
/// from `ResourceRef::default()` and only overwrites present properties),
/// unknown fields are rejected, and `domain` is one of the two authored names.
pub(crate) fn resource_ref_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "domain": { "enum": ["unset", "runtime_buffer"] },
            "id": u32_schema(),
        },
        "additionalProperties": false,
    })
}

/// Mirrors `read_product_ref`: every field is optional (`kind` defaults to the
/// shape's expected kind and everything else to zero), unknown fields are
/// rejected, and a present-but-mismatched `kind` is rejected — hence the
/// `const`. `preferred_extent` is parsed for both kinds even though the visual
/// constructor ignores it.
pub(crate) fn product_ref_schema(kind: ProductKind) -> Value {
    let kind_name = match kind {
        ProductKind::Visual => "visual",
        ProductKind::Control => "control",
    };
    json!({
        "type": "object",
        "properties": {
            "kind": { "const": kind_name },
            "node": u32_schema(),
            "output": u32_schema(),
            "preferred_extent": {
                "type": "object",
                "properties": {
                    "rows": u32_schema(),
                    "samples_per_row": u32_schema(),
                },
                "additionalProperties": false,
            },
        },
        "additionalProperties": false,
    })
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec;

    use crate::schema_gen::test_support::check_conformance;
    use crate::slot::shape;
    use crate::slot_codec::{JsonSyntaxSource, SlotReader, SyntaxError, read_dynamic_slot};
    use crate::{
        LpType, ModelEnumVariant, ModelStructMember, ProductKind, SlotShape, SlotShapeId,
        SlotShapeRegistry,
    };

    use super::compile_slot_shape_schema;

    #[test]
    fn record_accepts_partial_objects_and_rejects_unknown_fields() {
        let (registry, id) = registered(
            "test.SchemaRecord",
            shape::record(vec![
                shape::field("pin", shape::value(LpType::U32)),
                shape::field("name", shape::value(LpType::String)),
            ]),
        );

        check_dynamic(
            &registry,
            id,
            &[
                r#"{"pin":18,"name":"main"}"#,
                // Missing fields keep their defaults: nothing is required.
                r#"{"name":"main"}"#,
                r#"{}"#,
            ],
            &[
                r#"{"surprise":18}"#,
                r#"{"pin":"eighteen"}"#,
                r#"{"pin":1.5}"#,
                r#"[1,2]"#,
            ],
        );
    }

    #[test]
    fn string_map_checks_value_schema() {
        let (registry, id) = registered(
            "test.SchemaStringMap",
            shape::map(crate::SlotMapKeyShape::String, shape::value(LpType::U32)),
        );

        check_dynamic(
            &registry,
            id,
            &[r#"{"a":1,"b":2}"#, r#"{}"#],
            &[r#"{"a":"one"}"#, r#"{"a":-1}"#, r#"7"#],
        );
    }

    #[test]
    fn integer_key_maps_constrain_property_names() {
        let (registry, u32_id) = registered(
            "test.SchemaU32Map",
            shape::map(crate::SlotMapKeyShape::U32, shape::value(LpType::U32)),
        );
        check_dynamic(
            &registry,
            u32_id,
            &[r#"{"7":1,"0":2}"#],
            &[r#"{"seven":1}"#, r#"{"-7":1}"#, r#"{"":1}"#],
        );

        let (registry, i32_id) = registered(
            "test.SchemaI32Map",
            shape::map(crate::SlotMapKeyShape::I32, shape::value(LpType::U32)),
        );
        check_dynamic(
            &registry,
            i32_id,
            &[r#"{"-3":1,"12":2}"#],
            &[r#"{"3.5":1}"#, r#"{"x":1}"#],
        );
    }

    #[test]
    fn tagged_enum_pins_discriminator_and_flattens_record_payload() {
        let (registry, id) = registered(
            "test.SchemaTaggedEnum",
            shape::enum_tagged(vec![
                shape::variant(
                    "square",
                    shape::record(vec![shape::field("size", shape::value(LpType::F32))]),
                ),
                shape::variant("empty", shape::unit()),
            ]),
        );

        check_dynamic(
            &registry,
            id,
            &[
                r#"{"kind":"square","size":0.5}"#,
                // Payload fields stay record-optional.
                r#"{"kind":"square"}"#,
                r#"{"kind":"empty"}"#,
            ],
            &[
                r#"{"kind":"circle"}"#,
                // Variant-name matching is case-sensitive exact spelling.
                r#"{"kind":"Square","size":0.5}"#,
                r#"{"kind":"square","size":0.5,"extra":1}"#,
                // Unit payloads reject trailing properties.
                r#"{"kind":"empty","extra":1}"#,
                r#"{}"#,
                r#"{"size":0.5}"#,
            ],
        );
    }

    #[test]
    fn external_enum_requires_exactly_one_variant_property() {
        let (registry, id) = registered(
            "test.SchemaExternalEnum",
            shape::enum_external(vec![
                shape::variant("file", shape::value(LpType::String)),
                shape::variant("none", shape::unit()),
            ]),
        );

        check_dynamic(
            &registry,
            id,
            &[
                r#"{"file":"compute.glsl"}"#,
                r#"{"none":{}}"#,
                // Unit variant payloads are skipped, so any value is accepted.
                r#"{"none":42}"#,
            ],
            &[
                r#"{}"#,
                r#"{"file":"a.glsl","none":{}}"#,
                r#"{"missing":{}}"#,
                r#"{"file":7}"#,
            ],
        );
    }

    #[test]
    fn option_compiles_to_inner_schema_and_rejects_null() {
        let (registry, id) = registered(
            "test.SchemaOption",
            shape::record(vec![shape::field(
                "format",
                shape::option(shape::value(LpType::U32)),
            )]),
        );

        check_dynamic(
            &registry,
            id,
            &[r#"{"format":1}"#, r#"{}"#],
            // The reader has no null branch for options: absence is the only
            // None spelling.
            &[r#"{"format":null}"#, r#"{"format":"one"}"#],
        );
    }

    #[test]
    fn unit_accepts_anything() {
        let (registry, id) = registered("test.SchemaUnit", shape::unit());
        check_dynamic(
            &registry,
            id,
            &[r#"{}"#, r#"[1,2]"#, r#""text""#, r#"null"#, r#"7"#],
            &[],
        );
    }

    #[test]
    fn scalar_value_leaves_match_number_text_parsing() {
        check_value(LpType::String, &[r#""hi""#, r#""""#], &[r#"7"#, r#"null"#]);
        check_value(LpType::I32, &[r#"-5"#, r#"12"#], &[r#"1.5"#, r#""1""#]);
        check_value(
            LpType::U32,
            &[r#"0"#, r#"4294967295"#],
            &[r#"-1"#, r#"1.5"#],
        );
        check_value(
            LpType::F32,
            &[r#"1"#, r#"-2.5"#, r#"1e3"#],
            &[r#""1.0""#, r#"true"#],
        );
        check_value(
            LpType::Bool,
            &[r#"true"#, r#"false"#],
            &[r#"1"#, r#""true""#],
        );
    }

    #[test]
    fn vector_value_leaves_are_fixed_length_arrays() {
        check_value(
            LpType::Vec2,
            &[r#"[1.0,2.0]"#, r#"[1,2]"#],
            &[r#"[1.0]"#, r#"[1,2,3]"#, r#"[1,"2"]"#, r#"7"#],
        );
        check_value(
            LpType::IVec3,
            &[r#"[1,-2,3]"#],
            &[r#"[1,2]"#, r#"[1,2,3.5]"#],
        );
        check_value(LpType::UVec2, &[r#"[0,7]"#], &[r#"[-1,7]"#]);
        check_value(LpType::BVec2, &[r#"[true,false]"#], &[r#"[true,1]"#]);
    }

    #[test]
    fn matrix_value_leaves_are_nested_row_arrays() {
        check_value(
            LpType::Mat2x2,
            &[r#"[[1,0],[0,1]]"#],
            &[
                r#"[[1,0]]"#,
                r#"[[1,0],[0,1],[0,0]]"#,
                r#"[[1,0],[0]]"#,
                r#"[1,0,0,1]"#,
            ],
        );
    }

    #[test]
    fn array_and_list_value_leaves() {
        check_value(
            LpType::Array(Box::new(LpType::F32), 3),
            &[r#"[1,2,3]"#],
            &[r#"[1,2]"#, r#"[1,2,3,4]"#],
        );
        check_value(
            LpType::List(Box::new(LpType::U32)),
            &[r#"[]"#, r#"[1,2,3]"#],
            &[r#"[1,-2]"#, r#"[1,"2"]"#, r#"1"#],
        );
    }

    #[test]
    fn struct_value_leaves_require_every_field() {
        let ty = LpType::Struct {
            name: Some(alloc::string::String::from("Extent")),
            fields: vec![
                ModelStructMember {
                    name: alloc::string::String::from("rows"),
                    ty: LpType::U32,
                },
                ModelStructMember {
                    name: alloc::string::String::from("cols"),
                    ty: LpType::U32,
                },
            ],
        };
        check_value(
            ty,
            &[r#"{"rows":1,"cols":2}"#],
            // Unlike record slots, struct values require all fields.
            &[r#"{"rows":1}"#, r#"{"rows":1,"cols":2,"depth":3}"#, r#"{}"#],
        );
    }

    #[test]
    fn enum_value_leaves_require_payload_agreement() {
        let ty = LpType::Enum {
            name: Some(alloc::string::String::from("Endpoint")),
            variants: vec![
                ModelEnumVariant {
                    name: alloc::string::String::from("Unset"),
                    payload: None,
                },
                ModelEnumVariant {
                    name: alloc::string::String::from("Value"),
                    payload: Some(LpType::F32),
                },
            ],
        };
        check_value(
            ty,
            &[r#"{"kind":"Unset"}"#, r#"{"kind":"Value","payload":0.75}"#],
            &[
                r#"{"kind":"Blark12"}"#,
                // Payload variants require the payload...
                r#"{"kind":"Value"}"#,
                // ...and unit variants forbid it.
                r#"{"kind":"Unset","payload":1}"#,
                r#"{}"#,
            ],
        );
    }

    #[test]
    fn any_value_leaves_reject_objects_and_null() {
        check_value(
            LpType::Any,
            &[
                r#""text""#,
                r#"1"#,
                r#"1.5"#,
                r#"true"#,
                r#"[1,"a",[true]]"#,
            ],
            &[r#"{"a":1}"#, r#"null"#, r#"[{"a":1}]"#],
        );
    }

    #[test]
    fn resource_and_product_value_leaves() {
        check_value(
            LpType::Resource,
            &[
                r#"{"domain":"runtime_buffer","id":7}"#,
                r#"{}"#,
                r#"{"id":3}"#,
            ],
            &[
                r#"{"domain":"bogus","id":7}"#,
                r#"{"extra":1}"#,
                r#""buffer""#,
            ],
        );
        check_value(
            LpType::Product(ProductKind::Visual),
            &[
                r#"{"kind":"visual","node":2,"output":1}"#,
                r#"{"node":2}"#,
                // preferred_extent parses (and is ignored) for visual products.
                r#"{"kind":"visual","preferred_extent":{"rows":1,"samples_per_row":2}}"#,
            ],
            &[r#"{"kind":"control","node":2}"#, r#"{"bogus":1}"#],
        );
        check_value(
            LpType::Product(ProductKind::Control),
            &[
                r#"{"kind":"control","node":3,"output":2,"preferred_extent":{"rows":4,"samples_per_row":12}}"#,
            ],
            &[
                r#"{"kind":"visual"}"#,
                r#"{"preferred_extent":{"rows":-1,"samples_per_row":2}}"#,
            ],
        );
    }

    #[test]
    fn refs_become_defs_keyed_by_registry_name() {
        let point_id = SlotShapeId::from_static_name("test.SchemaPoint");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape_named(
                point_id,
                "test.SchemaPoint",
                shape::record(vec![
                    shape::field("x", shape::value(LpType::I32)),
                    shape::field("y", shape::value(LpType::I32)),
                ]),
            )
            .unwrap();
        let outer_id = SlotShapeId::from_static_name("test.SchemaRefOuter");
        registry
            .register_dynamic_shape(
                outer_id,
                shape::record(vec![shape::field("origin", shape::reference(point_id))]),
            )
            .unwrap();

        let schema = compile_slot_shape_schema(&registry, registry.get(&outer_id).unwrap());
        let text = serde_json::to_string(&schema).unwrap();
        assert!(
            text.contains(r##""$ref":"#/$defs/test.SchemaPoint""##),
            "expected named $defs ref: {text}"
        );
        assert!(
            schema["$defs"]["test.SchemaPoint"].is_object(),
            "expected $defs entry keyed by registry name: {text}"
        );
        // The hex id must not be the def key.
        assert!(
            !text.contains(&alloc::format!("{point_id}")),
            "def should be keyed by name, not numeric id: {text}"
        );

        check_dynamic(
            &registry,
            outer_id,
            &[r#"{"origin":{"x":1,"y":-2}}"#, r#"{}"#],
            &[r#"{"origin":{"x":1,"z":2}}"#, r#"{"origin":7}"#],
        );
    }

    #[test]
    fn unresolved_refs_compile_to_reject_everything() {
        let registry = SlotShapeRegistry::default();
        let missing = SlotShape::reference(SlotShapeId::from_static_name("test.SchemaMissing"));
        let schema = compile_slot_shape_schema(&registry, &missing);
        let validator = jsonschema::draft202012::new(&schema).unwrap();
        assert!(!validator.is_valid(&serde_json::json!({})));
        assert!(!validator.is_valid(&serde_json::json!("anything")));
    }

    // --- helpers -------------------------------------------------------------------------------

    fn registered(name: &str, shape: SlotShape) -> (SlotShapeRegistry, SlotShapeId) {
        let id = SlotShapeId::from_static_name(name);
        let mut registry = SlotShapeRegistry::default();
        registry.register_dynamic_shape(id, shape).unwrap();
        (registry, id)
    }

    /// Conformance check through the dynamic reader (`read_dynamic_slot`),
    /// which is the pure shape-driven read path.
    fn check_dynamic(
        registry: &SlotShapeRegistry,
        id: SlotShapeId,
        accepted: &[&str],
        rejected: &[&str],
    ) {
        let shape = registry.get(&id).unwrap().clone();
        let read = |text: &str| -> Result<(), SyntaxError> {
            let mut reader = SlotReader::new(JsonSyntaxSource::new(text)?, registry);
            read_dynamic_slot(registry, id, reader.value()).map(|_| ())
        };
        check_conformance(registry, &shape, read, accepted, rejected);
    }

    /// Conformance check for a single raw `LpType` value leaf.
    fn check_value(ty: LpType, accepted: &[&str], rejected: &[&str]) {
        let name = alloc::format!("test.SchemaValue.{ty:?}");
        let (registry, id) = registered(&name, shape::value(ty));
        check_dynamic(&registry, id, accepted, rejected);
    }
}
