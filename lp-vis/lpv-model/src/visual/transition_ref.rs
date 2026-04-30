//! [`TransitionRef`]: a reference to a [`crate::visual::Transition`]
//! artifact with a duration and optional inline param overrides. Used
//! by [`crate::visual::Live`] and [`crate::visual::Playlist`] as the
//! default transition between candidates / entries.

use alloc::collections::BTreeMap;
use alloc::string::String;
use lpc_source::SrcArtifactSpec;

/// Reference to a Transition with playback parameters.
///
/// `duration` is in seconds. `params` overrides are stored as raw
/// `toml::Value` in v0; type-checking against the referenced
/// Transition's param schema lands with cross-artifact resolution.
///
/// # Example
///
/// ```text
/// [transition]
/// visual   = "../transitions/crossfade.transition.toml"
/// duration = 1.5
///
/// [transition.params]
/// softness = 0.7
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TransitionRef {
    pub visual: SrcArtifactSpec,
    pub duration: f32,
    #[cfg_attr(
        feature = "schema-gen",
        schemars(schema_with = "crate::visual::params_table::toml_value_btree_map_schema")
    )]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_ref_round_trips() {
        let t: TransitionRef = toml::from_str(
            r#"
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.5
        "#,
        )
        .unwrap();
        let s = toml::to_string(&t).unwrap();
        let back: TransitionRef = toml::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn transition_ref_with_params_round_trips() {
        let t: TransitionRef = toml::from_str(
            r#"
            visual   = "../transitions/wipe.transition.toml"
            duration = 2.0
            [params]
            angle = 0.785
        "#,
        )
        .unwrap();
        assert!(t.params.contains_key("angle"));
        let s = toml::to_string(&t).unwrap();
        let back: TransitionRef = toml::from_str(&s).unwrap();
        assert_eq!(t, back);
    }
}
