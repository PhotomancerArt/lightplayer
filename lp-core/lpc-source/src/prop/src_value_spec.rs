//! Author-time **default** description: what to put in a slot before the
//! loader/runtime has real resources.
//!
//! Some [`Kind`][`lpc_model::kind::Kind`]s need more than a plain **runtime value**
//! at author time: **opaque handles** (e.g. [`Kind::Texture`][`lpc_model::kind::Kind::Texture`])
//! are produced from a small **recipe** ([`SrcTextureSpec`]) that the loader
//! materializes into a handle-shaped value (`docs/design/lightplayer/quantity.md`
//! §7). Value-typed Kinds use [`SrcValueSpec::Literal`]. Defaults are serialized
//! as [`SrcValueSpec`], not as an already-resolved GPU handle, so save/reload
//! round-trips author intent (`quantity.md` §7 “Conventions”).
//!
//! # TOML **literal** forms (`default = …` per `quantity.md` §10)
//!
//! The JSON/serde `SrcValueSpecWire` form (`{ kind = "literal", value = … }`) is
//! unchanged. **TOML** load uses a [`Kind`]- and [`Shape`]-aware path so
//! `default` can be written as a plain value:
//!
//! | Kinds / shape | TOML `default` |
//! |----------------|----------------|
//! | `Amplitude`, `Ratio`, `Phase`, `Instant`, `Duration`, `Frequency`, `Angle` | any TOML number → `F32` runtime literal |
//! | `Count`, `Choice` | integer → `I32` runtime literal |
//! | `Bool` | bool |
//! | `Color` | CSS string `"oklch(0.7 0.15 90)"` or table `{ space = "<str>", coords = [f,f,f] }` → struct `Color` ([`ModelType`](lpc_model::ModelType) order: `space`, `coords`) |
//! | `ColorPalette` | authoring table `{ space, count?, entries = [[f,f,f],…] }`; lpfx may materialize this as a height-one texture resource before shader binding |
//! | `Gradient` | authoring table `{ space, method, count?, stops = [{at,c},…] }`; lpfx may materialize this as a height-one texture resource before shader binding |
//! | `Position2d` / `Position3d` | 2- or 3-long array of numbers → `Vec2` / `Vec3` |
//! | `AudioLevel` | table `{ low, mid, high }` |
//! | `Texture` | string `"black"` (v0) → [`SrcValueSpec::Texture`] |
//! | `SrcShape::Array` | TOML array, length must match, elements per element [`SrcSlot`][`crate::prop::src_shape::SrcSlot`]’s shape |
//! | `SrcShape::Struct` | TOML table, one key per struct field, field **declaration** order in [`ModelType`] / slot list |
//!
//! The inverse is `SrcValueSpec::to_toml_for_kind` / `SrcValueSpec::to_toml_for_shape` (private helpers).
//!
//! ## Serde and equality
//!
//! `ModelValue` in `lpc-model` does not derive `Serialize` / `PartialEq` in
//! M2; this module uses [`ModelValue`] for serde and hand-written
//! [`SrcValueSpec`]:[`PartialEq`] (see
//! `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/summary.md` — “SrcValueSpec
//! serde via wire enum” and hand-written `PartialEq` for `SrcValueSpec`).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::prop::src_shape::SrcShape;
use crate::prop::src_texture_spec::SrcTextureSpec;
use crate::prop::src_value_spec_wire::SrcValueSpecWire;
use crate::prop::toml_color::{
    from_toml_struct_kind, wire_color_palette_to_toml, wire_color_to_toml, wire_gradient_to_toml,
};
use crate::prop::toml_parse::{
    model_value_audio_level, toml_f32, toml_i32, vec_n_from_toml, vec2_to_toml_value,
    vec3_to_toml_value, wire_audio_level_to_toml,
};

pub use crate::prop::toml_parse::FromTomlError;

use lpc_model::ModelValue;
use lpc_model::kind::Kind;

/// Load-time context for **materializing** author specs: allocating handles,
/// resolving assets, and similar.
///
/// M2 ships a minimal stub; M3+ is expected to wire a real texture allocator
/// and cache (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` — `LoadCtx` stub, `summary.md`).
#[derive(Default)]
pub struct LoadCtx {
    /// Monotonic counter (or future allocator state) for [`SrcTextureSpec`] materialization in tests; not the final handle policy.
    pub next_texture_handle: i32,
}

impl From<&SrcValueSpec> for SrcValueSpecWire {
    fn from(s: &SrcValueSpec) -> Self {
        match s {
            SrcValueSpec::Literal(v) => SrcValueSpecWire::Literal(v.clone()),
            SrcValueSpec::Texture(t) => SrcValueSpecWire::Texture(t.clone()),
        }
    }
}

impl From<SrcValueSpecWire> for SrcValueSpec {
    fn from(w: SrcValueSpecWire) -> Self {
        match w {
            SrcValueSpecWire::Literal(v) => SrcValueSpec::Literal(v),
            SrcValueSpecWire::Texture(t) => SrcValueSpec::Texture(t),
        }
    }
}

/// Either a portable [`ModelValue`] for value-typed kinds, or a handle recipe
/// for opaque kinds (`docs/design/lightplayer/quantity.md` §7).
#[derive(Clone, Debug)]
pub enum SrcValueSpec {
    /// Portable literal or texture recipe; see [`SrcValueSpec::default_model_value`].
    Literal(ModelValue),
    /// [`SrcTextureSpec`] for [`Kind::Texture`] defaults
    /// (M2: v0 has [`SrcTextureSpec::Black`] only, `quantity.md` §7 sketch).
    Texture(SrcTextureSpec),
}

impl serde::Serialize for SrcValueSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SrcValueSpecWire::from(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for SrcValueSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        SrcValueSpecWire::deserialize(deserializer).map(SrcValueSpec::from)
    }
}

impl PartialEq for SrcValueSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(a), Self::Literal(b)) => a.eq(b),
            (Self::Texture(a), Self::Texture(b)) => a == b,
            _ => false,
        }
    }
}

impl SrcValueSpec {
    /// Default **wire** value for this spec: clone for [`SrcValueSpec::Literal`];
    /// for [`SrcValueSpec::Texture`], allocate handle-shaped [`ModelValue`] through `ctx`.
    pub fn default_model_value(&self, ctx: &mut LoadCtx) -> ModelValue {
        match self {
            Self::Literal(v) => v.clone(),
            Self::Texture(spec) => spec.default_model_value(ctx),
        }
    }

    /// On-disk TOML `default` for a **scalar** slot of the given [`Kind`]. See
    /// the module’s “TOML literal forms” table.
    pub(crate) fn from_toml_for_kind(
        value: &toml::Value,
        k: Kind,
    ) -> Result<SrcValueSpec, FromTomlError> {
        if k == Kind::Texture {
            if let toml::Value::String(s) = value {
                if s == "black" {
                    return Ok(SrcValueSpec::Texture(SrcTextureSpec::Black));
                }
            }
            return Err(FromTomlError::msg(
                "texture default must be the string \"black\" in v0",
            ));
        }

        if matches!(k, Kind::Color | Kind::ColorPalette | Kind::Gradient) {
            let v = from_toml_struct_kind(value, k)?;
            return Ok(SrcValueSpec::Literal(v));
        }

        if k == Kind::AudioLevel {
            return Ok(SrcValueSpec::Literal(model_value_audio_level(
                value
                    .as_table()
                    .ok_or_else(|| FromTomlError::msg("audio_level default must be a table"))?,
            )?));
        }

        if k == Kind::Position2d {
            return Ok(SrcValueSpec::Literal(vec_n_from_toml(
                value,
                2,
                "position2d",
            )?));
        }
        if k == Kind::Position3d {
            return Ok(SrcValueSpec::Literal(vec_n_from_toml(
                value,
                3,
                "position3d",
            )?));
        }

        let v =
            match k {
                Kind::Amplitude
                | Kind::Ratio
                | Kind::Phase
                | Kind::Instant
                | Kind::Duration
                | Kind::Frequency
                | Kind::Angle => ModelValue::F32(toml_f32(value)?),
                Kind::Count | Kind::Choice => ModelValue::I32(toml_i32(value)?),
                Kind::Bool => ModelValue::Bool(value.as_bool().ok_or_else(|| {
                    FromTomlError::msg("bool kind expects a TOML boolean `default`")
                })?),
                Kind::Texture
                | Kind::Color
                | Kind::ColorPalette
                | Kind::Gradient
                | Kind::Position2d
                | Kind::Position3d
                | Kind::AudioLevel => {
                    return Err(FromTomlError::msg("internal: kind already handled above"));
                }
            };
        Ok(SrcValueSpec::Literal(v))
    }

    /// On-disk TOML `default` for a value matching `shape` (compositions recurse).
    pub(crate) fn from_toml_for_shape(
        value: &toml::Value,
        shape: &SrcShape,
    ) -> Result<SrcValueSpec, FromTomlError> {
        match shape {
            SrcShape::Scalar { kind, .. } => Self::from_toml_for_kind(value, *kind),
            SrcShape::Array {
                element,
                length,
                default: _,
            } => {
                let arr = value
                    .as_array()
                    .ok_or_else(|| FromTomlError::msg("array default must be a TOML array"))?;
                if arr.len() as u32 != *length {
                    return Err(FromTomlError::msg(
                        "array default length does not match `length`",
                    ));
                }
                let mut out = Vec::with_capacity(arr.len());
                for elt in arr {
                    match Self::from_toml_for_shape(elt, &element.shape)? {
                        SrcValueSpec::Literal(lv) => out.push(lv),
                        SrcValueSpec::Texture(_) => {
                            return Err(FromTomlError::msg(
                                "array default elements must be literal",
                            ));
                        }
                    }
                }
                Ok(SrcValueSpec::Literal(ModelValue::Array(out)))
            }
            SrcShape::Struct { fields, default: _ } => {
                let t = value
                    .as_table()
                    .ok_or_else(|| FromTomlError::msg("struct default must be a TOML table"))?;
                let mut out_fields: Vec<(String, ModelValue)> = Vec::with_capacity(fields.len());
                for (name, slot) in fields {
                    let v = t.get(name.0.as_str()).ok_or_else(|| {
                        FromTomlError(format!("struct default table missing field `{}`", name.0))
                    })?;
                    match Self::from_toml_for_shape(v, &slot.shape)? {
                        SrcValueSpec::Literal(lv) => out_fields.push((name.0.clone(), lv)),
                        SrcValueSpec::Texture(_) => {
                            return Err(FromTomlError::msg(
                                "struct default field values must be literal in v0",
                            ));
                        }
                    }
                }
                Ok(SrcValueSpec::Literal(ModelValue::Struct {
                    name: None,
                    fields: out_fields,
                }))
            }
        }
    }

    /// Serialize a [`SrcValueSpec`]'s TOML literal (inverse of [`Self::from_toml_for_kind`]).
    pub(crate) fn to_toml_for_kind(
        spec: &SrcValueSpec,
        k: Kind,
    ) -> Result<toml::Value, FromTomlError> {
        match (spec, k) {
            (SrcValueSpec::Texture(SrcTextureSpec::Black), Kind::Texture) => {
                Ok(toml::Value::String("black".into()))
            }
            (SrcValueSpec::Texture(_), _) => {
                Err(FromTomlError::msg("texture only for Kind::Texture"))
            }
            (SrcValueSpec::Literal(_), Kind::Texture) => Err(FromTomlError::msg(
                "Kind::Texture default must be SrcValueSpec::Texture in v0",
            )),
            (SrcValueSpec::Literal(v), Kind::Color) => Ok(wire_color_to_toml(v)?),
            (SrcValueSpec::Literal(v), Kind::ColorPalette) => Ok(wire_color_palette_to_toml(v)?),
            (SrcValueSpec::Literal(v), Kind::Gradient) => Ok(wire_gradient_to_toml(v)?),
            (SrcValueSpec::Literal(v), Kind::AudioLevel) => Ok(wire_audio_level_to_toml(v)?),
            (SrcValueSpec::Literal(v), Kind::Position2d) => vec2_to_toml_value(v),
            (SrcValueSpec::Literal(v), Kind::Position3d) => vec3_to_toml_value(v),
            (SrcValueSpec::Literal(v), _) if k == Kind::Bool => match v {
                ModelValue::Bool(b) => Ok(toml::Value::Boolean(*b)),
                _ => Err(FromTomlError::msg(
                    "bool literal expected in SrcValueSpec::Literal",
                )),
            },
            (SrcValueSpec::Literal(v), _) if k == Kind::Count || k == Kind::Choice => match v {
                ModelValue::I32(i) => Ok(toml::Value::Integer(i64::from(*i))),
                _ => Err(FromTomlError::msg(
                    "i32 literal expected in SrcValueSpec::Literal",
                )),
            },
            (SrcValueSpec::Literal(v), _) => match v {
                ModelValue::F32(f) => Ok(toml::Value::Float(f64::from(*f))),
                _ => Err(FromTomlError::msg("f32 scalar literal expected")),
            },
        }
    }

    /// Serialize a [`SrcValueSpec`]'s TOML literal (inverse of [`Self::from_toml_for_shape`]).
    pub(crate) fn to_toml_for_shape(
        spec: &SrcValueSpec,
        shape: &SrcShape,
    ) -> Result<toml::Value, FromTomlError> {
        match (spec, shape) {
            (
                SrcValueSpec::Texture(t),
                SrcShape::Scalar {
                    kind: Kind::Texture,
                    ..
                },
            ) => Self::to_toml_for_kind(&SrcValueSpec::Texture(t.clone()), Kind::Texture),
            (SrcValueSpec::Literal(_), SrcShape::Scalar { kind, .. }) => {
                Self::to_toml_for_kind(spec, *kind)
            }
            (SrcValueSpec::Texture(_), _) => Err(FromTomlError::msg(
                "aggregate default must be literal in v0",
            )),
            (
                SrcValueSpec::Literal(v),
                SrcShape::Array {
                    element, length, ..
                },
            ) => {
                let a = match v {
                    ModelValue::Array(x) => x,
                    _ => {
                        return Err(FromTomlError::msg("array spec must be ModelValue::Array"));
                    }
                };
                if a.len() as u32 != *length {
                    return Err(FromTomlError::msg("array literal length mismatch"));
                }
                let mut arr = Vec::with_capacity(a.len());
                for (i, elt) in a.iter().enumerate() {
                    let s = match Self::to_toml_for_shape(
                        &SrcValueSpec::Literal(elt.clone()),
                        &element.shape,
                    ) {
                        Ok(t) => t,
                        Err(e) => {
                            return Err(FromTomlError(format!("array element {i}: {e}")));
                        }
                    };
                    arr.push(s);
                }
                Ok(toml::Value::Array(arr))
            }
            (SrcValueSpec::Literal(v), SrcShape::Struct { fields, .. }) => {
                let tval = match v {
                    ModelValue::Struct { fields, .. } => fields,
                    _ => {
                        return Err(FromTomlError::msg("struct spec must be ModelValue::Struct"));
                    }
                };
                let mut map: toml::map::Map<String, toml::Value> = toml::map::Map::new();
                for (n, s) in fields {
                    let lv = tval
                        .iter()
                        .find(|(k, _)| k == n.0.as_str())
                        .ok_or_else(|| {
                            FromTomlError::msg("struct literal missing a field (serialization)")
                        })?
                        .1
                        .clone();
                    let tv = SrcValueSpec::to_toml_for_shape(&SrcValueSpec::Literal(lv), &s.shape)?;
                    map.insert(n.0.clone(), tv);
                }
                Ok(toml::Value::Table(map))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop::src_shape::{SrcShape, SrcSlot};
    use alloc::boxed::Box;
    use lpc_model::NodeName;
    use lpc_model::prop::kind::Kind;

    #[test]
    fn literal_materializes_to_itself() {
        let mut ctx = LoadCtx::default();
        let spec = SrcValueSpec::Literal(ModelValue::F32(0.5));
        match spec.default_model_value(&mut ctx) {
            ModelValue::F32(v) => assert_eq!(v, 0.5),
            other => panic!("expected F32(0.5), got {other:?}"),
        }
    }

    #[test]
    fn texture_black_materializes_to_handle_zero() {
        let mut ctx = LoadCtx::default();
        let spec = SrcValueSpec::Texture(SrcTextureSpec::Black);
        let v = spec.default_model_value(&mut ctx);
        match v {
            ModelValue::Struct { fields, .. } => {
                let handle = fields
                    .iter()
                    .find(|(n, _)| n == "handle")
                    .expect("handle field");
                match &handle.1 {
                    ModelValue::I32(h) => assert_eq!(*h, 0),
                    _ => panic!("handle must be I32"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn f32_literal_round_trips_in_toml_for_amplitude() {
        let v = toml::Value::Float(1.0);
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::Amplitude).unwrap();
        let t = SrcValueSpec::to_toml_for_kind(&s, Kind::Amplitude).unwrap();
        assert!(matches!(&t, toml::Value::Float(f) if (*f as f32 - 1.0).abs() < 1e-6));
    }

    #[test]
    fn i32_literal_round_trips_in_toml_for_count() {
        let v = toml::Value::Integer(4);
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::Count).unwrap();
        let t = SrcValueSpec::to_toml_for_kind(&s, Kind::Count).unwrap();
        assert_eq!(t.as_integer(), Some(4));
    }

    #[test]
    fn bool_literal_round_trips_in_toml_for_bool() {
        let v = toml::Value::Boolean(true);
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::Bool).unwrap();
        let t = SrcValueSpec::to_toml_for_kind(&s, Kind::Bool).unwrap();
        assert_eq!(t.as_bool(), Some(true));
    }

    #[test]
    fn color_literal_round_trips_in_toml() {
        let css = toml::Value::String("oklch(0.7 0.15 90)".into());
        let s = SrcValueSpec::from_toml_for_kind(&css, Kind::Color).unwrap();
        let out = SrcValueSpec::to_toml_for_kind(&s, Kind::Color).unwrap();
        assert_eq!(
            out.as_str(),
            Some("oklch(0.7 0.15 90)"),
            "serialized Color must be a CSS function string: {out:?}"
        );
    }

    #[test]
    fn color_literal_accepts_table_backward_compat() {
        let toml = r#"
        space = "oklch"
        coords = [0.72, 0.14, 285]
        "#;
        let table: toml::Table = toml::from_str(toml).unwrap();
        let v = toml::Value::Table(table);
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::Color).unwrap();
        let out = SrcValueSpec::to_toml_for_kind(&s, Kind::Color).unwrap();
        assert_eq!(out.as_str(), Some("oklch(0.72 0.14 285)"));
    }

    #[test]
    fn color_literal_parses_oklch_hex_and_rgb_strings() {
        let g = 87.0f32 / 255.0;
        let b = 51.0f32 / 255.0;
        let cases: [(&str, toml::Value, i32, f32, f32, f32); 3] = [
            (
                "oklch",
                toml::Value::String("oklch(0.72 0.14 285)".into()),
                0,
                0.72,
                0.14,
                285.0,
            ),
            ("hex", toml::Value::String("#ff5733".into()), 3, 1.0, g, b),
            (
                "rgb",
                toml::Value::String("rgb(255 87 51)".into()),
                3,
                1.0,
                g,
                b,
            ),
        ];
        for (_label, tval, sp, a, b0, c0) in cases {
            let s = SrcValueSpec::from_toml_for_kind(&tval, Kind::Color).unwrap();
            let got = s.default_model_value(&mut LoadCtx::default());
            let ModelValue::Struct { fields, .. } = got else {
                panic!("struct color");
            };
            let space = fields
                .iter()
                .find_map(|(n, v)| (n == "space").then(|| v))
                .expect("space");
            let coords = fields
                .iter()
                .find_map(|(n, v)| (n == "coords").then(|| v))
                .expect("coords");
            let ModelValue::I32(sid) = space else {
                panic!("space");
            };
            let ModelValue::Vec3([x, y, z]) = coords else {
                panic!("coords");
            };
            assert_eq!(*sid, sp);
            assert!((x - a).abs() < 0.001, "x {x} vs {a}");
            assert!((y - b0).abs() < 0.001, "y {y} vs {b0}");
            assert!((z - c0).abs() < 0.001, "z {z} vs {c0}");
        }
    }

    #[test]
    fn audio_level_literal_round_trips_in_toml() {
        let toml = r#"
        low  = 0.1
        mid  = 0.2
        high = 0.3
        "#;
        let table: toml::Table = toml::from_str(toml).unwrap();
        let v = toml::Value::Table(table);
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::AudioLevel).unwrap();
        let out = SrcValueSpec::to_toml_for_kind(&s, Kind::AudioLevel).unwrap();
        let t = out.as_table().expect("table");
        assert!(t.get("low").is_some());
    }

    #[test]
    fn position2d_array_literal_round_trips() {
        let v = toml::Value::Array(alloc::vec![
            toml::Value::Float(0.0),
            toml::Value::Float(1.0)
        ]);
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::Position2d).unwrap();
        let t = SrcValueSpec::to_toml_for_kind(&s, Kind::Position2d).unwrap();
        assert_eq!(t.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn texture_black_string_round_trips() {
        let v = toml::Value::String("black".into());
        let s = SrcValueSpec::from_toml_for_kind(&v, Kind::Texture).unwrap();
        let t = SrcValueSpec::to_toml_for_kind(&s, Kind::Texture).unwrap();
        assert_eq!(t.as_str(), Some("black"));
    }

    fn amp_slot() -> SrcShape {
        SrcShape::Scalar {
            kind: Kind::Amplitude,
            constraint: lpc_model::prop::kind::Kind::Amplitude.default_constraint(),
            default: SrcValueSpec::Literal(ModelValue::F32(0.0)),
        }
    }

    #[test]
    fn array_of_amplitude_literals_round_trips() {
        let v = toml::Value::Array(alloc::vec![
            toml::Value::Float(0.1),
            toml::Value::Float(0.2),
        ]);
        let shape = SrcShape::Array {
            element: Box::new(SrcSlot {
                shape: amp_slot(),
                label: None,
                description: None,
                bind: None,
                present: None,
            }),
            length: 2,
            default: None,
        };
        let s = SrcValueSpec::from_toml_for_shape(&v, &shape).unwrap();
        let t = SrcValueSpec::to_toml_for_shape(&s, &shape).unwrap();
        assert_eq!(t.as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn struct_of_two_amplitudes_round_trips() {
        let toml = r#"
        a = 0.1
        b = 0.2
        "#;
        let table: toml::Table = toml::from_str(toml).unwrap();
        let v = toml::Value::Table(table);
        let fields = alloc::vec![
            (
                NodeName::parse("a").unwrap(),
                SrcSlot {
                    shape: amp_slot(),
                    label: None,
                    description: None,
                    bind: None,
                    present: None,
                },
            ),
            (
                NodeName::parse("b").unwrap(),
                SrcSlot {
                    shape: amp_slot(),
                    label: None,
                    description: None,
                    bind: None,
                    present: None,
                },
            ),
        ];
        let shape = SrcShape::Struct {
            fields: fields.clone(),
            default: None,
        };
        let s = SrcValueSpec::from_toml_for_shape(&v, &shape).unwrap();
        let t = SrcValueSpec::to_toml_for_shape(&s, &shape).unwrap();
        assert_eq!(t.as_table().map(|m| m.len()), Some(2));
    }

    #[test]
    fn literal_f32_serde_tag_matches_internal_wire_form() {
        let spec = SrcValueSpec::Literal(ModelValue::F32(0.25));
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(json, r#"{"kind":"literal","value":{"f32":0.25}}"#);
    }
}
