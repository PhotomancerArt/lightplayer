//! [`ParamsTable`]: the implicit [`Shape::Struct`] form of a Visual’s
//! top-level `[params]` block. See `docs/design/lightplayer/quantity.md` §10
//! (“Top-level `[params]` is implicit `Shape::Struct`.”).
//!
//! The workspace enables `toml`’s **`preserve_order`** feature so
//! `toml::Table` keeps authoring order on round-trip (unlike a plain
//! `BTreeMap` ordering).

use crate::NodeName;
use alloc::vec::Vec;
use lpc_model::prop::shape::{Shape, Slot};

/// A Visual’s `[params]` block: a [`Slot`] whose [`Shape`] is always
/// [`Shape::Struct`], synthesized from the TOML table keys.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamsTable(pub Slot);

impl Default for ParamsTable {
    fn default() -> Self {
        ParamsTable(Slot {
            shape: Shape::Struct {
                fields: Vec::new(),
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        })
    }
}

impl<'de> serde::Deserialize<'de> for ParamsTable {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let table: toml::Table = toml::Table::deserialize(de)?;
        let mut fields: Vec<(NodeName, Slot)> = Vec::with_capacity(table.len());
        for (k, v) in table {
            let name = NodeName::parse(&k).map_err(|e| {
                serde::de::Error::custom(alloc::format!("invalid param name `{k}`: {e}"))
            })?;
            let s = toml::ser::to_string(&v).map_err(serde::de::Error::custom)?;
            let slot: Slot = toml::from_str(&s).map_err(serde::de::Error::custom)?;
            fields.push((name, slot));
        }
        Ok(ParamsTable(Slot {
            shape: Shape::Struct {
                fields,
                default: None,
            },
            label: None,
            description: None,
            bind: None,
            present: None,
        }))
    }
}

impl serde::Serialize for ParamsTable {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let fields = match &self.0.shape {
            Shape::Struct { fields, .. } => fields,
            _ => {
                return Err(serde::ser::Error::custom(
                    "ParamsTable inner shape must be Struct",
                ));
            }
        };
        let mut map = ser.serialize_map(Some(fields.len()))?;
        for (name, slot) in fields {
            map.serialize_entry(&name.0, slot)?;
        }
        map.end()
    }
}

#[cfg(feature = "schema-gen")]
impl schemars::JsonSchema for ParamsTable {
    fn schema_name() -> alloc::borrow::Cow<'static, str> {
        "ParamsTable".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let slot_schema = <Slot as schemars::JsonSchema>::json_schema(generator);
        schemars::json_schema!({
            "description": "Implicit Shape::Struct for [params]. Keys are param names; values are Slot tables.",
            "type": "object",
            "additionalProperties": slot_schema,
        })
    }
}

/// `BTreeMap<String, toml::Value>` serialized in JSON must be allowed in schema
/// when `schemars(skip)` would hide `params` but serde still emits it.
#[cfg(feature = "schema-gen")]
pub(crate) fn toml_value_btree_map_schema(
    _gen: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::kind::Kind;
    use lpc_model::prop::shape::Shape;

    #[test]
    fn empty_params_round_trips() {
        let p = ParamsTable::default();
        let s = toml::to_string(&p).unwrap();
        assert_eq!(s.trim(), "");
        let back: ParamsTable = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn single_scalar_param_loads() {
        let toml_str = r#"
            [speed]
            kind    = "amplitude"
            default = 1.0
        "#;
        let p: ParamsTable = toml::from_str(toml_str).unwrap();
        match &p.0.shape {
            Shape::Struct { fields, .. } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0.0, "speed");
                assert!(matches!(
                    fields[0].1.shape,
                    Shape::Scalar {
                        kind: Kind::Amplitude,
                        ..
                    }
                ));
            }
            _ => panic!("expected Struct"),
        }
    }

    #[test]
    fn multi_scalar_params_preserve_order() {
        let toml_str = r#"
            [time]
            kind    = "instant"
            default = 0.0

            [speed]
            kind    = "amplitude"
            default = 1.0

            [saturation]
            kind    = "amplitude"
            default = 0.8
        "#;
        let p: ParamsTable = toml::from_str(toml_str).unwrap();
        match &p.0.shape {
            Shape::Struct { fields, .. } => {
                assert_eq!(fields.len(), 3);
                let order: alloc::vec::Vec<_> = fields.iter().map(|(n, _)| n.0.as_str()).collect();
                assert_eq!(order, alloc::vec!["time", "speed", "saturation"]);
            }
            _ => panic!("expected Struct"),
        }
    }
}
