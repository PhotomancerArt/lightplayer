//! [`ShaderRef`]: how a Visual specifies its shader source. Three
//! mutually exclusive forms — inline GLSL, sibling file (language
//! inferred from extension), or builtin Rust impl by name. See
//! `docs/design/lpfx/overview.md` and the M3 design doc.

use crate::types::Name;
use alloc::string::String;

/// Inline GLSL source (TOML key `glsl`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)] // Mutex flat keys; typos → hard errors per 00-design.md §Constraint.
pub struct ShaderRefGlsl {
    pub glsl: String,
}

/// Sibling shader file path (TOML key `file`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ShaderRefFile {
    pub file: String,
}

/// Built-in shader impl name (TOML key `builtin`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ShaderRefBuiltin {
    pub builtin: Name,
}

/// The shader source backing a visual artifact (pattern, effect, transition, …).
///
/// TOML form (mutex keys under `[shader]`):
///
/// ```toml
/// [shader]
/// glsl = """ … """          # inline source
/// # OR
/// file = "main.glsl"        # sibling path; language by extension
/// # OR
/// builtin = "fluid"         # built-in Rust impl
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ShaderRef {
    Glsl(ShaderRefGlsl),
    File(ShaderRefFile),
    Builtin(ShaderRefBuiltin),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glsl_variant_round_trips() {
        let s = ShaderRef::Glsl(ShaderRefGlsl {
            glsl: "void main() {}".into(),
        });
        let toml = toml::to_string(&s).unwrap();
        let back: ShaderRef = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn file_variant_round_trips() {
        let s = ShaderRef::File(ShaderRefFile {
            file: "main.glsl".into(),
        });
        let toml = toml::to_string(&s).unwrap();
        let back: ShaderRef = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn builtin_variant_round_trips() {
        let s = ShaderRef::Builtin(ShaderRefBuiltin {
            builtin: Name::parse("fluid").unwrap(),
        });
        let toml = toml::to_string(&s).unwrap();
        let back: ShaderRef = toml::from_str(&toml).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn two_keys_present_is_an_error() {
        let toml_str = r#"
            glsl = "void main() {}"
            file = "main.glsl"
        "#;
        let res: Result<ShaderRef, _> = toml::from_str(toml_str);
        assert!(res.is_err(), "two mutex keys must error: got {res:?}");
    }

    #[test]
    fn unknown_key_is_an_error() {
        let toml_str = r#"
            wgsl = "fn main() {}"
        "#;
        let res: Result<ShaderRef, _> = toml::from_str(toml_str);
        assert!(res.is_err(), "unknown key must error: got {res:?}");
    }
}
