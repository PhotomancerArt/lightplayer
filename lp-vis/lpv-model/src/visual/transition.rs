//! [`Transition`]: a 2-arity Visual that crossfades / blends two
//! inputs over `progress` ∈ [0, 1]. Inputs are conventional shader
//! uniforms (`inputA`, `inputB`); not declared in the artifact.
//! See `docs/design/lightplayer/domain.md`.

use crate::visual::{params_table::ParamsTable, shader_ref::ShaderRef};
use alloc::string::String;
use lpc_model::artifact::artifact::Artifact;
use lpc_model::prop::shape::Slot;

/// A 2-input Visual that interpolates between `inputA` and `inputB`
/// based on the `progress` parameter. Used by Live (between
/// candidates) and Playlist (between entries).
///
/// `progress` is conventionally a shader uniform driven by the
/// caller (Live / Playlist runtime); the artifact doesn't declare
/// it as a Slot.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Crossfade"
///
/// [shader]
/// glsl = """ … """
///
/// [params.softness]
/// kind    = "amplitude"
/// default = 1.0
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub shader: ShaderRef,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Transition {
    const KIND: &'static str = "transition";
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
    fn minimal_transition_loads() {
        let toml = r#"
            schema_version = 1
            title = "Crossfade"
            [shader]
            glsl = "void main() {}"
        "#;
        let t: Transition = toml::from_str(toml).unwrap();
        assert_eq!(t.title, "Crossfade");
    }

    #[test]
    fn transition_round_trips() {
        let toml = r#"
            schema_version = 1
            title = "Wipe"
            [shader]
            glsl = "void main() {}"
            [params.angle]
            kind    = "angle"
            default = 0.0
        "#;
        let t: Transition = toml::from_str(toml).unwrap();
        let s = toml::to_string(&t).unwrap();
        let back: Transition = toml::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn input_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Wipe"
            [shader]
            glsl = "void main() {}"
            [input]
            bus = "video/in/0"
        "#;
        let res: Result<Transition, _> = toml::from_str(toml);
        assert!(
            res.is_err(),
            "Transition has no [input] field; deny_unknown_fields must reject it"
        );
    }

    #[test]
    fn transition_kind_constant() {
        assert_eq!(Transition::KIND, "transition");
        assert_eq!(Transition::CURRENT_VERSION, 1);
    }
}
