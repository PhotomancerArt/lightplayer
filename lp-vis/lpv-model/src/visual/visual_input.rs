//! [`VisualInput`]: the polymorphic input slot of a stack or effect: either
//! composes a child Visual into the node tree or routes from a bus channel.
//!
//! `[input]` is **structural composition**, not a binding. A
//! [`crate::binding::Binding`] is pure routing: it points to existing
//! values and never instantiates nodes.
//! [`VisualInput::Visual`] *does* instantiate a child node, which is
//! why it lives here and not as a `Binding` variant. See `00-notes.md`
//! Q-D3 for the full discussion.

use alloc::collections::BTreeMap;
use alloc::string::String;
use lpc_model::ArtifactSpec;
use lpc_model::ChannelName;

/// Child visual reference plus optional param overrides (TOML keys `visual`, `params`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)] // Mutex flat keys; typos → hard errors per 00-design.md §Constraint.
pub struct VisualInputVisual {
    pub visual: ArtifactSpec,
    /// `toml::Value` per key; schemars uses an open object so serde JSON and
    /// schema both allow `params` when present.
    #[cfg_attr(
        feature = "schema-gen",
        schemars(schema_with = "super::params_table::toml_value_btree_map_schema")
    )]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

/// Bus channel input (TOML key `bus`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct VisualInputBus {
    pub bus: ChannelName,
}

/// One input slot of a Stack or Effect.
///
/// TOML form (mutex keys under `[input]`):
///
/// ```toml
/// [input]
/// visual = "../patterns/fbm.pattern.toml"
///
/// [input.params]
/// scale = 6.0
/// ```
///
/// or
///
/// ```toml
/// [input]
/// bus = "video/in/0"
/// ```
///
/// `params` value-overrides on the visual form
/// are kept as raw `toml::Value` in v0; type-checking them needs the
/// referenced Visual's param schema, which is a cross-artifact
/// concern explicitly out of scope for M3.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum VisualInput {
    Visual(VisualInputVisual),
    Bus(VisualInputBus),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_variant_round_trips() {
        let v = VisualInput::Visual(VisualInputVisual {
            visual: ArtifactSpec("../patterns/fbm.pattern.toml".into()),
            params: BTreeMap::new(),
        });
        let toml_str = toml::to_string(&v).unwrap();
        let back: VisualInput = toml::from_str(&toml_str).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn visual_with_params_round_trips() {
        let mut params = BTreeMap::new();
        params.insert("scale".into(), toml::Value::Float(6.0));
        let v = VisualInput::Visual(VisualInputVisual {
            visual: ArtifactSpec("../patterns/fbm.pattern.toml".into()),
            params,
        });
        let toml_str = toml::to_string(&v).unwrap();
        let back: VisualInput = toml::from_str(&toml_str).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn bus_variant_round_trips() {
        let v = VisualInput::Bus(VisualInputBus {
            bus: ChannelName("video/in/0".into()),
        });
        let toml_str = toml::to_string(&v).unwrap();
        let back: VisualInput = toml::from_str(&toml_str).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn both_keys_present_is_an_error() {
        let toml_str = r#"
            visual = "../patterns/fbm.pattern.toml"
            bus    = "video/in/0"
        "#;
        let res: Result<VisualInput, _> = toml::from_str(toml_str);
        assert!(res.is_err());
    }

    #[test]
    fn neither_key_present_is_an_error() {
        let toml_str = r#""#;
        let res: Result<VisualInput, _> = toml::from_str(toml_str);
        assert!(res.is_err());
    }
}
