use alloc::string::String;
use serde::{Deserialize, Serialize};

use crate::{Slotted, SourcePath, SourcePathSlot, ValueSlot};

/// Authored shader source.
///
/// File-backed sources use `path` and resolve relative to the containing node
/// definition artifact. Inline GLSL uses `glsl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ShaderSource {
    #[default]
    Path(SourcePathSlot),
    Glsl(ValueSlot<String>),
}

impl ShaderSource {
    pub fn path(path: impl Into<SourcePath>) -> Self {
        Self::Path(SourcePathSlot::new(path.into()))
    }

    pub fn glsl(source: impl Into<String>) -> Self {
        Self::Glsl(ValueSlot::new(source.into()))
    }

    pub fn path_value(&self) -> Option<&SourcePath> {
        match self {
            Self::Path(path) => Some(path.value()),
            Self::Glsl(_) => None,
        }
    }

    pub fn glsl_value(&self) -> Option<&str> {
        match self {
            Self::Path(_) => None,
            Self::Glsl(source) => Some(source.value().as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_source_parses_path() {
        let source: ShaderSource = toml::from_str(
            r#"
path = "./visual.glsl"
"#,
        )
        .expect("source");

        assert_eq!(source.path_value().unwrap().as_str(), "./visual.glsl");
    }

    #[test]
    fn shader_source_parses_glsl() {
        let source: ShaderSource = toml::from_str(
            r#"
glsl = "vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }"
"#,
        )
        .expect("source");

        assert!(source.glsl_value().unwrap().contains("render"));
    }
}
