use alloc::string::String;

use crate::{Slotted, SourcePath, SourcePathSlot, ValueSlot};

/// Authored shader source.
///
/// File-backed sources use `path` and resolve relative to the containing node
/// definition artifact. Inline GLSL uses `glsl`.
#[derive(Debug, Clone, PartialEq, Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
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
    use crate::{EnumSlot, FieldSlotMut, SlotEnumShape, SlotShapeRegistry};

    #[test]
    fn shader_source_parses_path() {
        let source = read_source(
            r#"
path = "./visual.glsl"
"#,
        );

        assert_eq!(source.path_value().unwrap().as_str(), "./visual.glsl");
    }

    #[test]
    fn shader_source_parses_glsl() {
        let source = read_source(
            r#"
glsl = "vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }"
"#,
        );

        assert!(source.glsl_value().unwrap().contains("render"));
    }

    fn read_source(text: &str) -> ShaderSource {
        let registry = SlotShapeRegistry::default();
        let value = toml::from_str::<toml::Value>(text).unwrap();
        let mut reader = crate::slot_codec::SlotReader::new(
            crate::slot_codec::TomlSyntaxSource::new(&value).unwrap(),
            &registry,
        );
        let mut source = EnumSlot::new(ShaderSource::default());
        crate::slot_codec::apply_reader_to_slot(
            source.slot_field_data_mut(),
            &ShaderSource::slot_enum_shape(),
            &registry,
            reader.value(),
        )
        .unwrap();
        source.into_inner()
    }
}
