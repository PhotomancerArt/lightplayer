//! [`Shape`]: the **structural** skeleton of a value (what WGSL/GLSL can
//! represent); [`Slot`]: a [`Shape`] plus **metadata** and wiring hints.
//!
//! Together they are the **composition** layer of the Quantity model
//! (`docs/design/lightplayer/quantity.md` §1, §2, and §6). `Shape` is *only*
//! `Scalar | Array | Struct` (no tuples or sum types) so every slot’s storage
//! projects cleanly to a [`crate::LpsType`] and GPU layouts (`quantity.md` §6,
//! “Why no tuples”).
//!
//! **Defaults (M2, “Q15 Option A”):** [`Shape::Scalar`] carries a **mandatory**
//! [`ValueSpec`][`crate::value_spec::ValueSpec`]. [`Shape::Array`] and
//! [`Shape::Struct`] carry `default: Option<ValueSpec>`; if `None`, the
//! default is **derived** at materialize time from child slots (arrays:
//! N copies; structs: one field per child). If `Some`, that aggregate spec
//! wins. See `quantity.md` §6 “Defaults for compositions” and
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` (Q15). A
//! [`Slot`] has **no** separate top-level `default` field: defaults are entirely
//! expressed through [`Shape`].
//!
//! [`Shape::Struct`]’s `fields` are a **vector** to preserve TOML order, std430
//! layout, and panel field order (`quantity.md` §6).

use crate::binding::Binding;
use crate::constraint::{Constraint, ConstraintChoice, ConstraintFree, ConstraintRange};
use crate::kind::Kind;
use crate::presentation::Presentation;
use crate::types::Name;
use crate::value_spec::{FromTomlError, LoadCtx, ValueSpec};
use crate::{LpsType, LpsValue};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use lps_shared::StructMember;
use serde::Deserialize;
use serde::Serialize;
use serde::de::Error;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;

/// The **recursive** shape of a slot: scalar, fixed-length array, or ordered struct.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum Shape {
    /// One leaf value: [`Kind`], [`Constraint`], and a **required** default
    /// [`ValueSpec`][`crate::value_spec::ValueSpec`] (there is nothing to
    /// derive a default from, `quantity.md` §6).
    Scalar {
        kind: Kind,
        constraint: Constraint,
        default: ValueSpec,
    },
    /// A fixed `length` of `element` slots; optional **aggregate** default
    /// (see module docs). `None` ⇒ N-element array from
    /// [`Slot::default_value`].
    Array {
        element: Box<Slot>,
        length: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<ValueSpec>,
    },
    /// Ordered struct fields. Optional aggregate default: `None` ⇒ struct
    /// map from each field’s default (`quantity.md` §6).
    Struct {
        fields: Vec<(Name, Slot)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<ValueSpec>,
    },
}

/// A **slot declaration**: a [`Shape`] plus optional human and runtime-facing
/// metadata.
///
/// `label` / `description` are for author-facing UI. `bind` and `present`
/// connect to the bus and widget hints; when `present` is `None`, tools use
/// [`Kind::default_presentation`][`crate::kind::Kind::default_presentation`]
/// (`docs/design/lightplayer/quantity.md` §8–9). The slot’s **value default**
/// is fully determined by the nested [`Shape`], not a separate field (Q15, see
/// module `//!` above).
///
/// # TOML and JSON shape
///
/// The on-disk form is a **single table**; see
/// `docs/design/lightplayer/quantity.md` §10.
/// Omitted `shape` defaults to [`Shape::Scalar`]; in TOML output, `shape = "scalar"`
/// is elided (see the [`Slot`] [`Serialize`][`serde::Serialize`] impl).
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// The structural and default-bearing part of the slot.
    pub shape: Shape,
    /// Short user-facing name (optional; falls back to kind defaults in UI, `quantity.md` §6 sketch).
    pub label: Option<String>,
    /// Longer description (optional).
    pub description: Option<String>,
    /// If set, **overrides** [`Kind::default_bind`][`crate::kind::Kind::default_bind`]
    /// for input-side bus wiring (`docs/design/lightplayer/quantity.md` §8).
    pub bind: Option<Binding>,
    /// If set, **overrides** [`Kind::default_presentation`][`crate::kind::Kind::default_presentation`]
    /// for UI (`docs/design/lightplayer/quantity.md` §9).
    pub present: Option<Presentation>,
}

impl Slot {
    /// Materialize the **default** for this slot: for [`Shape::Scalar`],
    /// [`ValueSpec::materialize`][`crate::value_spec::ValueSpec::materialize`]
    /// on the scalar’s `default`. For array/struct, if `default` is `Some`, use
    /// that; otherwise build `Array` of `length` / `Struct` of field name →
    /// child default, per `docs/design/lightplayer/quantity.md` §6 “Defaults
    /// for compositions”.
    pub fn default_value(&self, ctx: &mut LoadCtx) -> LpsValue {
        match &self.shape {
            Shape::Scalar { default, .. } => default.materialize(ctx),
            Shape::Array {
                element,
                length,
                default,
            } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let mut elems = Vec::with_capacity(*length as usize);
                    for _ in 0..*length {
                        elems.push(element.default_value(ctx));
                    }
                    LpsValue::Array(elems.into_boxed_slice())
                }
            },
            Shape::Struct { fields, default } => match default {
                Some(d) => d.materialize(ctx),
                None => {
                    let entries = fields
                        .iter()
                        .map(|(name, slot)| (name.0.clone(), slot.default_value(ctx)))
                        .collect();
                    LpsValue::Struct {
                        name: None,
                        fields: entries,
                    }
                }
            },
        }
    }

    /// Structural type for GPU and serializers: for scalars,
    /// [`Kind::storage`](crate::kind::Kind::storage) for the leaf kind; for arrays, element type
    /// with length; for structs, ordered members (`quantity.md` §2 table and §6
    /// `storage()` sketch).
    pub fn storage(&self) -> LpsType {
        match &self.shape {
            Shape::Scalar { kind, .. } => kind.storage(),
            Shape::Array {
                element, length, ..
            } => LpsType::Array {
                element: Box::new(element.storage()),
                len: *length,
            },
            Shape::Struct { fields, .. } => LpsType::Struct {
                name: None,
                members: fields
                    .iter()
                    .map(|(name, slot)| StructMember {
                        name: Some(name.0.clone()),
                        ty: slot.storage(),
                    })
                    .collect(),
            },
        }
    }
}

/// Custom [`Deserialize`][`serde::Deserialize`] and [`Serialize`][`serde::Serialize`]
/// for [`Slot`] implement the TOML grammar in `docs/design/lightplayer/quantity.md` §10:
/// metadata keys (`label`, `description`, `bind`, `present`) are peers of the shape
/// table; the wire form is decomposed to [`toml::Value`], then the remainder is
/// deserialized as [`Shape`]. Omitted `shape` defaults to `"scalar"`; misplaced
/// `element` / `props` relative to the active shape are rejected.
impl<'de> Deserialize<'de> for Slot {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value: toml::Value = toml::Value::deserialize(de)?;
        let table = value.as_table_mut().ok_or_else(|| {
            de::Error::custom("Slot must deserialize to a TOML table (or JSON object)")
        })?;

        let label = take_string(table, "label").map_err(D::Error::custom)?;
        let description = take_string(table, "description").map_err(D::Error::custom)?;
        let bind = take_typed(table, "bind").map_err(D::Error::custom)?;
        let present = take_typed(table, "present").map_err(D::Error::custom)?;

        if !table.contains_key("shape") {
            table.insert("shape".into(), toml::Value::String(String::from("scalar")));
        } else if !matches!(table.get("shape"), Some(toml::Value::String(_))) {
            return Err(de::Error::custom("`shape` must be a string if present"));
        }

        let shape_str = table
            .get("shape")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| de::Error::custom("expected `shape` to be a string"))?;

        match shape_str {
            "scalar" => {
                if table.contains_key("element") {
                    return Err(de::Error::custom(
                        "`element` is only valid when `shape = \"array\"`",
                    ));
                }
                if table.contains_key("props") {
                    return Err(de::Error::custom(
                        "`props` is only valid when `shape = \"struct\"`",
                    ));
                }
            }
            "array" => {
                if table.contains_key("props") {
                    return Err(de::Error::custom(
                        "`props` is only valid when `shape = \"struct\"`",
                    ));
                }
            }
            "struct" => {
                if table.contains_key("element") {
                    return Err(de::Error::custom(
                        "`element` is only valid when `shape = \"array\"`",
                    ));
                }
            }
            other => {
                return Err(de::Error::custom(alloc::format!(
                    "unknown `shape` value `{other}` (expected `scalar`, `array`, or `struct`)"
                )));
            }
        }

        let mut table = value.as_table().cloned().ok_or_else(|| {
            de::Error::custom("internal: Slot TOML must be a table after metadata extraction")
        })?;

        let shape = deserialize_shape_from_table(&mut table).map_err(D::Error::custom)?;
        if !table.is_empty() {
            return Err(de::Error::custom(alloc::format!(
                "unknown keys in Slot table: {:?}",
                table.keys().collect::<alloc::vec::Vec<_>>()
            )));
        }

        Ok(Slot {
            shape,
            label,
            description,
            bind,
            present,
        })
    }
}

impl Serialize for Slot {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let table = self.to_toml_table().map_err(serde::ser::Error::custom)?;
        toml::Value::Table(table).serialize(ser)
    }
}

impl Slot {
    fn to_toml_table(&self) -> Result<toml::map::Map<String, toml::Value>, FromTomlError> {
        let mut table = shape_to_toml_map(&self.shape)?;
        if matches!(self.shape, Shape::Scalar { .. }) {
            table.remove("shape");
        }
        if let Some(l) = &self.label {
            table.insert("label".into(), toml::Value::String(l.clone()));
        }
        if let Some(d) = &self.description {
            table.insert("description".into(), toml::Value::String(d.clone()));
        }
        if let Some(b) = &self.bind {
            let mut t = toml::map::Map::new();
            match b {
                Binding::Bus(ch) => {
                    t.insert("bus".into(), toml::Value::String(ch.0.clone()));
                }
            }
            table.insert("bind".into(), toml::Value::Table(t));
        }
        if let Some(p) = &self.present {
            table.insert("present".into(), presentation_toml_value(p));
        }
        Ok(table)
    }
}

fn shape_to_toml_map(shape: &Shape) -> Result<toml::map::Map<String, toml::Value>, FromTomlError> {
    let mut t = toml::map::Map::new();
    match shape {
        Shape::Scalar {
            kind,
            constraint,
            default,
        } => {
            t.insert("shape".into(), toml::Value::String(String::from("scalar")));
            t.insert("kind".into(), kind_to_toml_value(kind));
            for (k, v) in constraint_to_map(constraint) {
                t.insert(k, v);
            }
            t.insert(
                "default".into(),
                ValueSpec::to_toml_for_kind(default, *kind)?,
            );
        }
        Shape::Array {
            element,
            length,
            default,
        } => {
            t.insert("shape".into(), toml::Value::String(String::from("array")));
            t.insert("length".into(), toml::Value::Integer(i64::from(*length)));
            t.insert(
                "element".into(),
                toml::Value::Table(slot_to_toml_map(element)?),
            );
            if let Some(d) = default {
                let s = Shape::Array {
                    element: element.clone(),
                    length: *length,
                    default: None,
                };
                t.insert("default".into(), ValueSpec::to_toml_for_shape(d, &s)?);
            }
        }
        Shape::Struct { fields, default } => {
            t.insert("shape".into(), toml::Value::String(String::from("struct")));
            let mut arr = alloc::vec::Vec::new();
            for (n, s) in fields {
                let inner = toml::Value::Table(slot_to_toml_map(s)?);
                arr.push(toml::Value::Array(alloc::vec![
                    toml::Value::String(n.0.clone()),
                    inner,
                ]));
            }
            t.insert("fields".into(), toml::Value::Array(arr));
            if let Some(d) = default {
                let s = Shape::Struct {
                    fields: fields.clone(),
                    default: None,
                };
                t.insert("default".into(), ValueSpec::to_toml_for_shape(d, &s)?);
            }
        }
    }
    Ok(t)
}

fn slot_to_toml_map(s: &Slot) -> Result<toml::map::Map<String, toml::Value>, FromTomlError> {
    s.to_toml_table()
}

fn slot_from_toml_value(v: toml::Value) -> Result<Slot, String> {
    let s = toml::ser::to_string(&v).map_err(|e| e.to_string())?;
    toml::from_str(&s).map_err(|e: toml::de::Error| e.to_string())
}

fn constraint_to_map(c: &Constraint) -> toml::map::Map<String, toml::Value> {
    let mut t = toml::map::Map::new();
    match c {
        Constraint::Range(ConstraintRange { range, step }) => {
            t.insert(
                "range".into(),
                toml::Value::Array(alloc::vec![
                    toml::Value::Float(f64::from(range[0])),
                    toml::Value::Float(f64::from(range[1])),
                ]),
            );
            if let Some(s) = step {
                t.insert("step".into(), toml::Value::Float(f64::from(*s)));
            }
        }
        Constraint::Choice(ConstraintChoice { choices, labels }) => {
            t.insert(
                "choices".into(),
                toml::Value::Array(
                    choices
                        .iter()
                        .map(|&x| toml::Value::Float(f64::from(x)))
                        .collect(),
                ),
            );
            t.insert(
                "labels".into(),
                toml::Value::Array(
                    labels
                        .iter()
                        .map(|s| toml::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        Constraint::Free(ConstraintFree {}) => {}
    }
    t
}

fn kind_to_toml_value(kind: &Kind) -> toml::Value {
    toml::Value::String(String::from(match kind {
        Kind::Amplitude => "amplitude",
        Kind::Ratio => "ratio",
        Kind::Phase => "phase",
        Kind::Count => "count",
        Kind::Bool => "bool",
        Kind::Choice => "choice",
        Kind::Instant => "instant",
        Kind::Duration => "duration",
        Kind::Frequency => "frequency",
        Kind::Angle => "angle",
        Kind::Color => "color",
        Kind::ColorPalette => "color_palette",
        Kind::Gradient => "gradient",
        Kind::Position2d => "position2d",
        Kind::Position3d => "position3d",
        Kind::Texture => "texture",
        Kind::AudioLevel => "audio_level",
    }))
}

fn take_constraint(
    t: &mut toml::map::Map<String, toml::Value>,
    kind: Kind,
) -> Result<Constraint, String> {
    let mut c = toml::map::Map::new();
    for k in ["range", "step", "choices", "labels"] {
        if let Some(v) = t.remove(k) {
            c.insert(String::from(k), v);
        }
    }
    if c.is_empty() {
        return Ok(kind.default_constraint());
    }
    Constraint::deserialize(toml::Value::Table(c))
        .map_err(|e: toml::de::Error| e.message().to_string())
}

fn deserialize_shape_from_table(
    t: &mut toml::map::Map<String, toml::Value>,
) -> Result<Shape, String> {
    let shape_s = t
        .get("shape")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| String::from("missing or invalid `shape`"))?
        .to_string();

    match shape_s.as_str() {
        "scalar" => {
            let kval = t
                .remove("kind")
                .ok_or_else(|| String::from("missing `kind` for scalar shape"))?;
            let kind: Kind =
                Kind::deserialize(kval).map_err(|e: toml::de::Error| e.message().to_string())?;
            let def = t
                .remove("default")
                .ok_or_else(|| String::from("missing `default` for scalar"))?;
            let constraint = take_constraint(t, kind)?;
            let default = ValueSpec::from_toml_for_kind(&def, kind)
                .map_err(|e: FromTomlError| e.0.clone())?;
            t.remove("shape");
            Ok(Shape::Scalar {
                kind,
                constraint,
                default,
            })
        }
        "array" => {
            let length: u32 = t
                .remove("length")
                .and_then(|v| v.as_integer().and_then(|i| u32::try_from(i).ok()))
                .ok_or_else(|| String::from("missing or bad `length` for array"))?;
            let el = t
                .remove("element")
                .ok_or_else(|| String::from("missing `element` for array"))?;
            let element = Box::new(
                slot_from_toml_value(el)
                    .map_err(|e| alloc::format!("invalid nested `element` slot: {e}"))?,
            );
            let default = match t.remove("default") {
                None => None,
                Some(v) => {
                    let s = Shape::Array {
                        element: element.clone(),
                        length,
                        default: None,
                    };
                    Some(
                        ValueSpec::from_toml_for_shape(&v, &s)
                            .map_err(|e: FromTomlError| e.0.clone())?,
                    )
                }
            };
            t.remove("shape");
            Ok(Shape::Array {
                element,
                length,
                default,
            })
        }
        "struct" => {
            let fields_v = t
                .remove("fields")
                .ok_or_else(|| String::from("missing `fields` for struct"))?;
            let a = fields_v
                .as_array()
                .ok_or_else(|| String::from("`fields` must be a TOML array"))?;
            let mut out: Vec<(Name, Slot)> = Vec::with_capacity(a.len());
            for (i, item) in a.iter().enumerate() {
                let arr = item
                    .as_array()
                    .ok_or_else(|| alloc::format!("`fields[{i}]` must be a 2-long array"))?;
                if arr.len() != 2 {
                    return Err(alloc::format!("`fields[{i}]` must be [name, slot]"));
                }
                let n_str = arr[0]
                    .as_str()
                    .ok_or_else(|| alloc::format!("`fields[{i}][0]` must be a string"))?;
                let name = Name::parse(n_str).map_err(|e| alloc::format!("`fields[{i}]`: {e}"))?;
                let slot: Slot = slot_from_toml_value(arr[1].clone())
                    .map_err(|_| alloc::format!("`fields[{i}]`: bad nested slot"))?;
                out.push((name, slot));
            }
            let default = match t.remove("default") {
                None => None,
                Some(v) => {
                    let s = Shape::Struct {
                        fields: out.clone(),
                        default: None,
                    };
                    Some(
                        ValueSpec::from_toml_for_shape(&v, &s)
                            .map_err(|e: FromTomlError| e.0.clone())?,
                    )
                }
            };
            t.remove("shape");
            Ok(Shape::Struct {
                fields: out,
                default,
            })
        }
        _ => Err(alloc::format!("unknown `shape` value `{shape_s}`")),
    }
}

fn take_string(
    t: &mut toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match t.remove(key) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(alloc::format!("`{key}` must be a string")),
    }
}

fn take_typed<T>(
    t: &mut toml::map::Map<String, toml::Value>,
    key: &str,
) -> Result<Option<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    match t.remove(key) {
        None => Ok(None),
        Some(v) => T::deserialize(v)
            .map(Some)
            .map_err(|e: toml::de::Error| String::from(e.message())),
    }
}

fn presentation_toml_value(p: &Presentation) -> toml::Value {
    toml::Value::String(String::from(match p {
        Presentation::Knob => "knob",
        Presentation::Fader => "fader",
        Presentation::Toggle => "toggle",
        Presentation::NumberInput => "number_input",
        Presentation::Dropdown => "dropdown",
        Presentation::XyPad => "xy_pad",
        Presentation::ColorPicker => "color_picker",
        Presentation::PaletteEditor => "palette_editor",
        Presentation::GradientEditor => "gradient_editor",
        Presentation::TexturePreview => "texture_preview",
    }))
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for Slot {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "Slot".into()
    }

    /// Matched to [`impl Serialize for Slot`]: a TOML `Table` (→ JSON object) with
    /// `shape` omitted for scalars, [`Constraint`] **flattened** (no nested
    /// `constraint` key), and struct `fields` as a JSON array of `[name, table]`
    /// pairs (see `shape_to_toml_map` / `to_toml_table` above). Scalar `default`
    /// is [`ValueSpec`][`crate::value_spec::ValueSpec`]’s TOML wire form, not
    /// the adjacently-tagged `ValueSpec` JSON shape, so the schema allows any
    /// value at `default` in M3.
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // `subschema_for::<Slot>` first so the generator can emit a stable `$ref` for
        // recursive `array.element` and struct field slots.
        let slot = generator.subschema_for::<Slot>();

        let any_default = schemars::Schema::from(true);
        let label = generator.subschema_for::<Option<String>>();
        let description = generator.subschema_for::<Option<String>>();
        let bind = generator.subschema_for::<Option<Binding>>();
        let present = generator.subschema_for::<Option<Presentation>>();

        // `Schema` is not `Copy`; each `json_schema!` takes ownership, so we clone
        // shared subschemas in each arm.
        let mut kind_r = || generator.subschema_for::<Kind>();

        let wire_scalar_range = {
            let kind = kind_r();
            let (label, description, bind, present) = (
                label.clone(),
                description.clone(),
                bind.clone(),
                present.clone(),
            );
            let any_default = any_default.clone();
            schemars::json_schema!({
                "type": "object",
                "required": ["kind", "default", "range"],
                "additionalProperties": false,
                "properties": {
                    "kind": kind,
                    "default": any_default,
                    "label": label,
                    "description": description,
                    "bind": bind,
                    "present": present,
                    "range": { "type": "array", "minItems": 2, "maxItems": 2, "items": { "type": "number" } },
                    "step": { "type": "number" }
                }
            })
        };

        let wire_scalar_choice = {
            let kind = kind_r();
            let (label, description, bind, present) = (
                label.clone(),
                description.clone(),
                bind.clone(),
                present.clone(),
            );
            let any_default = any_default.clone();
            schemars::json_schema!({
                "type": "object",
                "required": ["kind", "default", "choices", "labels"],
                "additionalProperties": false,
                "properties": {
                    "kind": kind,
                    "default": any_default,
                    "label": label,
                    "description": description,
                    "bind": bind,
                    "present": present,
                    "choices": { "type": "array", "items": { "type": "number" } },
                    "labels": { "type": "array", "items": { "type": "string" } }
                }
            })
        };

        // Free: only `kind`+`default`+optional slot metadata; no `range` / `step` / `choices` / `labels`.
        let wire_scalar_free = {
            let kind = kind_r();
            let any_default = any_default.clone();
            schemars::json_schema!({
                "type": "object",
                "required": ["kind", "default"],
                "additionalProperties": false,
                "properties": {
                    "kind": kind,
                    "default": any_default,
                    "label": label,
                    "description": description,
                    "bind": bind,
                    "present": present
                }
            })
        };

        let slot_for_array = slot.clone();
        // `shape` = "array" (emitted). `default` = aggregate `ValueSpec` (permissive).
        let wire_array = {
            let (label, description, bind, present) = (
                label.clone(),
                description.clone(),
                bind.clone(),
                present.clone(),
            );
            let any_default = any_default.clone();
            schemars::json_schema!({
                "type": "object",
                "required": ["shape", "length", "element"],
                "additionalProperties": false,
                "properties": {
                    "shape": { "const": "array" },
                    "length": { "type": "integer", "minimum": 0 },
                    "element": slot_for_array,
                    "default": any_default,
                    "label": label,
                    "description": description,
                    "bind": bind,
                    "present": present
                }
            })
        };

        // `fields`: TOML `[[name, table], …]` → JSON `[[ "name", { … } ], …]`.
        let struct_field_item = schemars::json_schema!({
            "type": "array",
            "minItems": 2,
            "maxItems": 2,
            "prefixItems": [ { "type": "string" }, slot ],
            "items": false
        });
        let wire_struct = {
            let any_default = any_default.clone();
            schemars::json_schema!({
                "type": "object",
                "required": ["shape", "fields"],
                "additionalProperties": false,
                "properties": {
                    "shape": { "const": "struct" },
                    "fields": { "type": "array", "items": struct_field_item },
                    "default": any_default,
                    "label": label,
                    "description": description,
                    "bind": bind,
                    "present": present
                }
            })
        };

        schemars::json_schema!({
            "oneOf": [
                wire_scalar_range,
                wire_scalar_choice,
                wire_scalar_free,
                wire_array,
                wire_struct
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::ConstraintRange;

    #[test]
    fn scalar_default_value_is_literal() {
        let mut ctx = LoadCtx::default();
        match scalar_amplitude_slot().default_value(&mut ctx) {
            LpsValue::F32(v) => assert_eq!(v, 1.0),
            other => panic!("expected F32(1.0), got {other:?}"),
        }
    }

    #[test]
    fn array_with_no_default_derives_from_element() {
        let elem = scalar_amplitude_slot();
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(elem),
                length: 3,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let mut ctx = LoadCtx::default();
        match array_slot.default_value(&mut ctx) {
            LpsValue::Array(items) => {
                assert_eq!(items.len(), 3);
                for item in items.iter() {
                    match item {
                        LpsValue::F32(v) => assert_eq!(*v, 1.0),
                        other => panic!("expected F32, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn array_with_explicit_default_uses_override() {
        let elem = scalar_amplitude_slot();
        let preset: Vec<LpsValue> = alloc::vec![LpsValue::F32(0.2), LpsValue::F32(0.7)];
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(elem),
                length: 2,
                default: Some(ValueSpec::Literal(LpsValue::Array(
                    preset.into_boxed_slice(),
                ))),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let mut ctx = LoadCtx::default();
        match array_slot.default_value(&mut ctx) {
            LpsValue::Array(items) => {
                assert_eq!(items.len(), 2);
                match (&items[0], &items[1]) {
                    (LpsValue::F32(a), LpsValue::F32(b)) => {
                        assert_eq!(*a, 0.2);
                        assert_eq!(*b, 0.7);
                    }
                    other => panic!("expected two F32s, got {other:?}"),
                }
            }
            other => panic!("expected Array, got {other:?}"),
        }
    }

    #[test]
    fn struct_with_no_default_derives_from_fields() {
        let speed = (Name::parse("speed").unwrap(), scalar_amplitude_slot());
        let struct_slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let mut ctx = LoadCtx::default();
        match struct_slot.default_value(&mut ctx) {
            LpsValue::Struct { fields, .. } => {
                assert_eq!(fields.len(), 1);
                let (name, val) = &fields[0];
                assert_eq!(name, "speed");
                match val {
                    LpsValue::F32(v) => assert_eq!(*v, 1.0),
                    other => panic!("expected F32, got {other:?}"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn slot_storage_projection_scalar() {
        assert_eq!(scalar_amplitude_slot().storage(), LpsType::Float);
    }

    #[test]
    fn slot_storage_projection_array() {
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 4,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        match array_slot.storage() {
            LpsType::Array { element, len } => {
                assert_eq!(*element, LpsType::Float);
                assert_eq!(len, 4);
            }
            _ => panic!("expected Array storage"),
        }
    }

    #[test]
    fn slot_serde_round_trip_scalar() {
        let s = scalar_amplitude_slot();
        let json = serde_json::to_string(&s).unwrap();
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
        let toml = toml::to_string(&s).unwrap();
        let back2: Slot = toml::from_str(&toml).unwrap();
        assert_eq!(s, back2);
    }

    #[test]
    fn slot_serde_omits_none_overrides_on_composed() {
        let array_slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 2,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let json = serde_json::to_string(&array_slot).unwrap();
        assert!(!json.contains("\"default\":null"));
        let toml = toml::to_string(&array_slot).unwrap();
        let back: Slot = toml::from_str(&toml).unwrap();
        assert_eq!(array_slot, back);
    }

    #[test]
    fn slot_serde_round_trips_recursive() {
        let speed = (Name::parse("speed").unwrap(), scalar_amplitude_slot());
        let struct_slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let json = serde_json::to_string(&struct_slot).unwrap();
        let back: Slot = serde_json::from_str(&json).unwrap();
        assert_eq!(struct_slot, back);
        let toml = toml::to_string(&struct_slot).unwrap();
        let back2: Slot = toml::from_str(&toml).unwrap();
        assert_eq!(struct_slot, back2);
    }

    #[test]
    fn scalar_slot_roundtrips_with_implicit_shape_in_toml() {
        let slot = scalar_amplitude_slot();
        let s = toml::to_string(&slot).unwrap();
        assert!(
            !s.contains("shape"),
            "implicit shape must be elided: got {s}"
        );
        let back: Slot = toml::from_str(&s).unwrap();
        assert_eq!(slot, back);
    }

    #[test]
    fn scalar_slot_loads_when_shape_is_omitted() {
        let expected = scalar_amplitude_slot();
        let written = toml::to_string(&expected).unwrap();
        assert!(!written.contains("shape"), "elided: {written}");
        let s: Slot = toml::from_str(&written).unwrap();
        assert_eq!(s, expected);
    }

    #[test]
    fn scalar_slot_loads_when_shape_is_explicit() {
        let expected = scalar_amplitude_slot();
        let mut no_shape = toml::to_string(&expected).unwrap();
        assert!(!no_shape.contains("shape"));
        no_shape.insert_str(0, "shape = \"scalar\"\n");
        let s: Slot = toml::from_str(&no_shape).unwrap();
        assert_eq!(s, expected);
    }

    #[test]
    fn slot_toml_emits_literal_default_not_value_spec_wire() {
        let t = toml::to_string(&scalar_amplitude_slot()).unwrap();
        assert!(!t.contains("kind = \"literal\""), "wire ValueSpec: {t}");
    }

    #[test]
    fn count_slot_roundtrips_toml_with_integer_literal() {
        let s = count_slot(4);
        let toml = toml::to_string(&s).unwrap();
        let back: Slot = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
        assert!(!toml.contains("literal"));
    }

    #[test]
    fn color_slot_roundtrips_toml_inline_table() {
        let c = LpsValue::Struct {
            name: Some(String::from("Color")),
            fields: alloc::vec![
                (String::from("space"), LpsValue::I32(0)),
                (String::from("coords"), LpsValue::Vec3([0.7, 0.15, 90.0])),
            ],
        };
        let slot = Slot {
            shape: Shape::Scalar {
                kind: Kind::Color,
                constraint: Kind::Color.default_constraint(),
                default: ValueSpec::Literal(c),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let t = toml::to_string(&slot).unwrap();
        assert!(!t.contains("kind = \"literal\""), "{t}");
        let back: Slot = toml::from_str(&t).unwrap();
        assert_eq!(slot, back);
    }

    #[test]
    fn audio_level_slot_roundtrips_toml() {
        let a = LpsValue::Struct {
            name: Some(String::from("AudioLevel")),
            fields: alloc::vec![
                (String::from("low"), LpsValue::F32(0.0)),
                (String::from("mid"), LpsValue::F32(0.0)),
                (String::from("high"), LpsValue::F32(0.0)),
            ],
        };
        let slot = Slot {
            shape: Shape::Scalar {
                kind: Kind::AudioLevel,
                constraint: Kind::AudioLevel.default_constraint(),
                default: ValueSpec::Literal(a),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let t = toml::to_string(&slot).unwrap();
        let back: Slot = toml::from_str(&t).unwrap();
        assert_eq!(slot, back);
    }

    #[test]
    fn bool_slot_roundtrips() {
        let slot = Slot {
            shape: Shape::Scalar {
                kind: Kind::Bool,
                constraint: Kind::Bool.default_constraint(),
                default: ValueSpec::Literal(LpsValue::Bool(true)),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let t = toml::to_string(&slot).unwrap();
        let back: Slot = toml::from_str(&t).unwrap();
        assert_eq!(slot, back);
    }

    #[test]
    fn position2d_slot_roundtrips() {
        let slot = Slot {
            shape: Shape::Scalar {
                kind: Kind::Position2d,
                constraint: Kind::Position2d.default_constraint(),
                default: ValueSpec::Literal(LpsValue::Vec2([0.5, 0.5])),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let t = toml::to_string(&slot).unwrap();
        let back: Slot = toml::from_str(&t).unwrap();
        assert_eq!(slot, back);
    }

    #[test]
    fn slot_metadata_fields_coexist_with_shape() {
        use crate::binding::Binding;
        use crate::types::ChannelName;

        let expected = Slot {
            label: Some(String::from("Speed")),
            description: Some(String::from("How fast")),
            bind: Some(Binding::Bus(ChannelName(String::from("audio/in/0/level")))),
            present: Some(Presentation::Fader),
            ..scalar_amplitude_slot()
        };
        let t = toml::to_string(&expected).unwrap();
        let s: Slot = toml::from_str(&t).unwrap();
        assert_eq!(s.label.as_deref(), Some("Speed"));
        assert_eq!(s.description.as_deref(), Some("How fast"));
        assert_eq!(
            s.bind,
            Some(Binding::Bus(ChannelName(String::from("audio/in/0/level"))))
        );
        assert_eq!(s.present, Some(Presentation::Fader));
        assert_eq!(s, expected);
    }

    #[test]
    fn array_with_props_at_root_is_rejected() {
        let toml = r#"
            shape  = "array"
            length = 2
            [props.x]
            kind = "amplitude"
            default = 0.0
        "#;
        let res: Result<Slot, _> = toml::from_str(toml);
        assert!(res.is_err());
    }

    #[test]
    fn struct_with_element_at_root_is_rejected() {
        let toml = r#"
            shape = "struct"
            [element]
            kind = "amplitude"
            default = 0.0
        "#;
        let res: Result<Slot, _> = toml::from_str(toml);
        assert!(res.is_err());
    }

    #[test]
    fn unknown_shape_is_rejected() {
        let toml = r#"
            shape = "tensor"
            kind  = "amplitude"
        "#;
        let res: Result<Slot, _> = toml::from_str(toml);
        assert!(res.is_err());
    }

    #[test]
    fn array_slot_roundtrips_in_toml() {
        let slot = Slot {
            shape: Shape::Array {
                element: Box::new(scalar_amplitude_slot()),
                length: 3,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let s = toml::to_string(&slot).unwrap();
        let back: Slot = toml::from_str(&s).unwrap();
        assert_eq!(slot, back);
    }

    #[test]
    fn struct_slot_roundtrips_in_toml() {
        let speed = (Name::parse("speed").unwrap(), scalar_amplitude_slot());
        let slot = Slot {
            shape: Shape::Struct {
                fields: alloc::vec![speed],
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        };
        let s = toml::to_string(&slot).unwrap();
        let back: Slot = toml::from_str(&s).unwrap();
        assert_eq!(slot, back);
    }

    #[cfg(feature = "schema-gen")]
    #[test]
    fn slot_schema_is_non_degenerate() {
        let s = schemars::schema_for!(Slot);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("oneOf") && json.contains("additionalProperties"));
        assert!(json.contains("range") && json.contains("choices") && json.contains("struct"));
    }

    fn scalar_amplitude_slot() -> Slot {
        Slot {
            shape: Shape::Scalar {
                kind: Kind::Amplitude,
                constraint: Constraint::Range(ConstraintRange {
                    range: [0.0, 1.0],
                    step: None,
                }),
                default: ValueSpec::Literal(LpsValue::F32(1.0)),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        }
    }

    fn count_slot(n: i32) -> Slot {
        Slot {
            shape: Shape::Scalar {
                kind: Kind::Count,
                constraint: Kind::Count.default_constraint(),
                default: ValueSpec::Literal(LpsValue::I32(n)),
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        }
    }
}
