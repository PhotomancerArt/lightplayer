//! [`Pattern`]: a single-output Visual whose pixels are driven by a shader.
//! See `docs/design/lightplayer/domain.md` and
//! `docs/design/lpfx/overview.md`.

use crate::visual::{params_table::ParamsTable, shader_ref::ShaderRef};
use alloc::string::String;
use lpc_model::artifact::artifact::Artifact;
use lpc_model::prop::shape::Slot;

/// A texture-producing Visual: shader source + parameter surface. No
/// input slot; Patterns generate their pixels from `time`, params, and
/// any bus-routed bindings on those params.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Rainbow"
/// description    = "Rolling rainbow with HSL hue rotation."
///
/// [shader]
/// glsl = """ … """
///
/// [params.speed]
/// kind    = "amplitude"
/// default = 0.25
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Pattern {
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

impl Artifact for Pattern {
    const KIND: &'static str = "pattern";
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
    use crate::visual::shader_ref::{ShaderRefBuiltin, ShaderRefFile, ShaderRefGlsl};

    fn minimal_pattern_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Tiny"

            [shader]
            glsl = "void main() {}"
        "#
    }

    #[test]
    fn minimal_pattern_loads() {
        let p: Pattern = toml::from_str(minimal_pattern_toml()).unwrap();
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.title, "Tiny");
        assert!(matches!(p.shader, ShaderRef::Glsl(ShaderRefGlsl { .. })));
    }

    #[test]
    fn pattern_round_trips_minimal() {
        let p: Pattern = toml::from_str(minimal_pattern_toml()).unwrap();
        let s = toml::to_string(&p).unwrap();
        let back: Pattern = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn pattern_with_file_shader_loads() {
        let toml = r#"
            schema_version = 1
            title = "FBM"
            [shader]
            file = "main.glsl"
        "#;
        let p: Pattern = toml::from_str(toml).unwrap();
        assert!(matches!(
            p.shader,
            ShaderRef::File(ShaderRefFile { ref file }) if file == "main.glsl"
        ));
    }

    #[test]
    fn pattern_with_builtin_shader_loads() {
        let toml = r#"
            schema_version = 1
            title = "Fluid"
            [shader]
            builtin = "fluid"
        "#;
        let p: Pattern = toml::from_str(toml).unwrap();
        assert!(matches!(
            p.shader,
            ShaderRef::Builtin(ShaderRefBuiltin { .. })
        ));
    }

    #[test]
    fn pattern_with_params_loads() {
        let toml = r#"
            schema_version = 1
            title = "Tiny"
            [shader]
            glsl = "void main() {}"
            [params.speed]
            kind    = "amplitude"
            default = 1.0
        "#;
        let p: Pattern = toml::from_str(toml).unwrap();
        assert!(toml::to_string(&p).unwrap().contains("speed"));
    }

    #[test]
    fn pattern_kind_constant_is_pattern() {
        assert_eq!(Pattern::KIND, "pattern");
        assert_eq!(Pattern::CURRENT_VERSION, 1);
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Tiny"
            future_field = "oops"
            [shader]
            glsl = "void main() {}"
        "#;
        let res: Result<Pattern, _> = toml::from_str(toml);
        assert!(res.is_err(), "unknown top-level field must error");
    }
}
