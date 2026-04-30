//! [`Stack`]: composes a base input with a sequence of [`Effect`](crate::visual::Effect)
//! references. See `docs/design/lightplayer/domain.md` and
//! `docs/design/lpfx/overview.md`.

use crate::visual::{params_table::ParamsTable, visual_input::VisualInput};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_source::ArtifactSpec;
use lpc_source::artifact::artifact::Artifact;
use lpc_source::prop::shape::Slot;

/// One Effect in a Stack's chain. Order is the order of declaration.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct EffectRef {
    pub visual: ArtifactSpec,
    #[cfg_attr(
        feature = "schema-gen",
        schemars(schema_with = "crate::visual::params_table::toml_value_btree_map_schema")
    )]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

/// Composes a base input through a sequence of effects.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Psychedelic"
///
/// [input]
/// visual = "../patterns/fbm.pattern.toml"
///
/// [[effects]]
/// visual = "../effects/tint.effect.toml"
///
/// [[effects]]
/// visual = "../effects/kaleidoscope.effect.toml"
///
/// [params.intensity]
/// kind    = "amplitude"
/// default = 1.0
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<VisualInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectRef>,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Stack {
    const KIND: &'static str = "stack";
    const CURRENT_VERSION: u32 = 1;

    fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn walk_slots<F: FnMut(&Slot)>(&self, mut f: F) {
        f(&self.params.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_loads_with_two_effects() {
        let s: Stack = toml::from_str(psychedelic_toml()).unwrap();
        assert_eq!(s.effects.len(), 2);
        assert!(matches!(s.input, Some(VisualInput::Visual(_))));
    }

    #[test]
    fn stack_round_trips() {
        let s: Stack = toml::from_str(psychedelic_toml()).unwrap();
        let out = toml::to_string(&s).unwrap();
        let back: Stack = toml::from_str(&out).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn stack_with_no_input_or_effects_loads() {
        let s: Stack = toml::from_str(
            r#"
            schema_version = 1
            title          = "Empty"
        "#,
        )
        .unwrap();
        assert!(s.input.is_none());
        assert!(s.effects.is_empty());
    }

    #[test]
    fn stack_kind_constant() {
        assert_eq!(Stack::KIND, "stack");
        assert_eq!(Stack::CURRENT_VERSION, 1);
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let res: Result<Stack, _> = toml::from_str(
            r#"
            schema_version = 1
            title          = "X"
            not_a_field    = true
        "#,
        );
        assert!(res.is_err());
    }

    fn psychedelic_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Psychedelic"

            [input]
            visual = "../patterns/fbm.pattern.toml"

            [[effects]]
            visual = "../effects/tint.effect.toml"

            [[effects]]
            visual = "../effects/kaleidoscope.effect.toml"
        "#
    }
}
