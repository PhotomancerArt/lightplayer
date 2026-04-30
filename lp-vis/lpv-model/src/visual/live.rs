//! [`Live`]: a placeholder Visual for live-mode authoring. M3 ships
//! a barebones form: a list of candidates, a default transition,
//! and the `[bindings]` cascade. The `[selection]` block (min_hold,
//! debounce, etc.) is **deferred** — see `00-notes.md` Q-D4d.
//!
//! Live mode requires runtime input wiring that is out of scope
//! for the M3 milestone; this struct exists primarily to reserve
//! the on-disk shape and the artifact KIND.

use crate::visual::transition_ref::TransitionRef;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_source::SrcArtifactSpec;
use lpc_source::artifact::src_artifact::SrcArtifact;
use lpc_source::prop::binding::SrcBinding;

/// One candidate Visual in a Live show.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LiveCandidate {
    pub visual: SrcArtifactSpec,
    #[serde(default = "default_priority")]
    pub priority: f32,
    #[cfg_attr(
        feature = "schema-gen",
        schemars(schema_with = "super::params_table::toml_value_btree_map_schema")
    )]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

fn default_priority() -> f32 {
    1.0
}

/// Live show: candidates + default transition + bindings cascade.
/// **No selection block in M3** (Q-D4d).
///
/// `bindings` keys are raw relative-NodePropSpec strings in M3; cross-
/// artifact validation lands with the binding resolution roadmap.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Main Live"
///
/// [[candidates]]
/// visual   = "../patterns/rainbow.pattern.toml"
/// priority = 1.0
///
/// [[candidates]]
/// visual   = "../stacks/psychedelic.stack.toml"
/// priority = 0.5
///
/// [transition]
/// visual   = "../transitions/crossfade.transition.toml"
/// duration = 2.0
///
/// [bindings]
/// "rainbow.pattern#params.speed" = { bus = "audio/in/0/level" }
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Live {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub candidates: Vec<LiveCandidate>,
    pub transition: TransitionRef,
    // TODO(binding-resolution): keys are raw relative-NodePropSpec
    // strings in M3; parse + validate against the candidates' param
    // schemas when binding resolution lands.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, SrcBinding>,
}

impl SrcArtifact for Live {
    const KIND: &'static str = "live";
    const CURRENT_VERSION: u32 = 1;

    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::ChannelName;

    #[test]
    fn live_loads_with_two_candidates() {
        let l: Live = toml::from_str(main_live_toml()).unwrap();
        assert_eq!(l.candidates.len(), 2);
        assert_eq!(l.transition.duration, 2.0);
    }

    #[test]
    fn live_round_trips() {
        let l: Live = toml::from_str(main_live_toml()).unwrap();
        let s = toml::to_string(&l).unwrap();
        let back: Live = toml::from_str(&s).unwrap();
        assert_eq!(l, back);
    }

    #[test]
    fn live_with_bindings_loads() {
        let toml = r#"
            schema_version = 1
            title = "Main"

            [[candidates]]
            visual = "../patterns/rainbow.pattern.toml"

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0

            [bindings]
            "rainbow.pattern#params.speed" = { bus = "audio/in/0/level" }
        "#;
        let l: Live = toml::from_str(toml).unwrap();
        assert_eq!(l.bindings.len(), 1);
        let b = l
            .bindings
            .get("rainbow.pattern#params.speed")
            .expect("binding key");
        assert_eq!(
            b,
            &SrcBinding::Bus(ChannelName(String::from("audio/in/0/level")))
        );
    }

    #[test]
    fn selection_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Main"
            [[candidates]]
            visual = "../patterns/rainbow.pattern.toml"
            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0
            [selection]
            min_hold = 5.0
        "#;
        let res: Result<Live, _> = toml::from_str(toml);
        assert!(res.is_err(), "Live has no [selection] field in M3");
    }

    #[test]
    fn live_kind_constant() {
        assert_eq!(Live::KIND, "live");
        assert_eq!(Live::CURRENT_VERSION, 1);
    }

    fn main_live_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Main"

            [[candidates]]
            visual   = "../patterns/rainbow.pattern.toml"
            priority = 1.0

            [[candidates]]
            visual   = "../patterns/fluid.pattern.toml"
            priority = 0.5

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 2.0
        "#
    }
}
