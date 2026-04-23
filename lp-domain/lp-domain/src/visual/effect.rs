//! [`Effect`]: a single-input Visual that transforms its input texture
//! through a shader. See `docs/design/lightplayer/domain.md` and
//! `docs/design/lpfx/overview.md`.

use crate::schema::Artifact;
use crate::shape::Slot;
use crate::visual::{params_table::ParamsTable, shader_ref::ShaderRef, visual_input::VisualInput};
use alloc::string::String;

/// An input-transforming Visual: input slot + shader + parameter
/// surface. The shader reads the input via the `inputColor` uniform
/// (convention; not enforced by this layer).
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Tint"
///
/// [shader]
/// glsl = """ … """
///
/// [input]
/// bus = "video/in/0"
///
/// [params.color]
/// kind    = "color"
/// default = { space = "oklch", coords = [0.7, 0.15, 90] }
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub shader: ShaderRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<VisualInput>,
    #[serde(default)]
    pub params: ParamsTable,
}

impl Artifact for Effect {
    const KIND: &'static str = "effect";
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
    fn minimal_effect_loads_without_input() {
        let toml = r#"
            schema_version = 1
            title = "Identity"
            [shader]
            glsl = "void main() {}"
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        assert!(e.input.is_none());
    }

    #[test]
    fn effect_with_bus_input_loads() {
        let toml = r#"
            schema_version = 1
            title = "Tint"
            [shader]
            glsl = "void main() {}"
            [input]
            bus = "video/in/0"
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        assert!(matches!(e.input, Some(VisualInput::Bus(_))));
    }

    #[test]
    fn effect_with_visual_input_loads() {
        let toml = r#"
            schema_version = 1
            title = "Stacked tint"
            [shader]
            glsl = "void main() {}"
            [input]
            visual = "../patterns/fbm.pattern.toml"
            [input.params]
            scale = 6.0
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        assert!(matches!(e.input, Some(VisualInput::Visual(_))));
    }

    #[test]
    fn effect_round_trips() {
        let toml = r#"
            schema_version = 1
            title = "Tint"
            [shader]
            glsl = "void main() {}"
            [input]
            bus = "video/in/0"
        "#;
        let e: Effect = toml::from_str(toml).unwrap();
        let s = toml::to_string(&e).unwrap();
        let back: Effect = toml::from_str(&s).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn effect_kind_constant() {
        assert_eq!(Effect::KIND, "effect");
        assert_eq!(Effect::CURRENT_VERSION, 1);
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "X"
            not_a_field = true
            [shader]
            glsl = "void main() {}"
        "#;
        let res: Result<Effect, _> = toml::from_str(toml);
        assert!(res.is_err());
    }
}
