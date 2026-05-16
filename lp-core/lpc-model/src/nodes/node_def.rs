//! Canonical authored node definition enum.
//!
//! This is the closed set of core node definitions understood by the current
//! LightPlayer model. Adding a core node kind should start here, then add the
//! concrete definition type and loader/runtime handling that variant requires.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};

use crate::node::kind::NodeKind;
use crate::nodes::fixture::FixtureDef;
use crate::nodes::output::OutputDef;
use crate::nodes::project::ProjectDef;
use crate::nodes::shader::ShaderDef;
use crate::nodes::texture::TextureDef;
use crate::{
    SlotAccess, SlotDataAccess, SlotMutAccess, SlotShapeId, SlotShapeRegistry, StaticSlotShape,
};

/// Authored body of a node artifact.
///
/// A `NodeDef` is source data: it is what a TOML artifact defines before the
/// engine instantiates a runtime node. Project artifacts are included because
/// a project defines the root project node and its child node invocations.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeDef {
    Project(ProjectDef),
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}

impl NodeDef {
    /// Core node kind for this definition.
    pub fn kind(&self) -> NodeKind {
        match self {
            Self::Project(_) => NodeKind::Project,
            Self::Texture(_) => NodeKind::Texture,
            Self::Shader(_) => NodeKind::Shader,
            Self::Output(_) => NodeKind::Output,
            Self::Fixture(_) => NodeKind::Fixture,
        }
    }

    /// Stable authored `kind` string used in TOML and diagnostics.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Project(_) => ProjectDef::KIND,
            Self::Texture(_) => TextureDef::KIND,
            Self::Shader(_) => ShaderDef::KIND,
            Self::Output(_) => OutputDef::KIND,
            Self::Fixture(_) => FixtureDef::KIND,
        }
    }

    pub fn as_project(&self) -> Option<&ProjectDef> {
        match self {
            Self::Project(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_texture(&self) -> Option<&TextureDef> {
        match self {
            Self::Texture(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_shader(&self) -> Option<&ShaderDef> {
        match self {
            Self::Shader(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_output(&self) -> Option<&OutputDef> {
        match self {
            Self::Output(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_fixture(&self) -> Option<&FixtureDef> {
        match self {
            Self::Fixture(def) => Some(def),
            _ => None,
        }
    }

    /// Parse a TOML node artifact through a caller-provided slot registry.
    pub fn from_toml_str_with_registry(
        registry: &SlotShapeRegistry,
        text: &str,
    ) -> Result<Self, NodeDefParseError> {
        let mut payload = toml::from_str::<toml::Value>(text).map_err(toml_parse_error)?;
        let kind = take_kind(&mut payload)?;

        match kind.as_str() {
            ProjectDef::KIND => read_variant::<ProjectDef>(registry, ProjectDef::SHAPE_ID, payload)
                .map(Self::Project),
            TextureDef::KIND => read_variant::<TextureDef>(registry, TextureDef::SHAPE_ID, payload)
                .map(Self::Texture),
            ShaderDef::KIND => {
                read_variant::<ShaderDef>(registry, ShaderDef::SHAPE_ID, payload).map(Self::Shader)
            }
            OutputDef::KIND => {
                read_variant::<OutputDef>(registry, OutputDef::SHAPE_ID, payload).map(Self::Output)
            }
            FixtureDef::KIND => read_variant::<FixtureDef>(registry, FixtureDef::SHAPE_ID, payload)
                .map(Self::Fixture),
            other => Err(NodeDefParseError::UnknownKind {
                kind: String::from(other),
            }),
        }
    }
}

impl SlotAccess for NodeDef {
    fn shape_id(&self) -> SlotShapeId {
        match self {
            Self::Project(def) => def.shape_id(),
            Self::Texture(def) => def.shape_id(),
            Self::Shader(def) => def.shape_id(),
            Self::Output(def) => def.shape_id(),
            Self::Fixture(def) => def.shape_id(),
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Project(def) => def.data(),
            Self::Texture(def) => def.data(),
            Self::Shader(def) => def.data(),
            Self::Output(def) => def.data(),
            Self::Fixture(def) => def.data(),
        }
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
        self
    }
}

/// Failure parsing an authored node definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeDefParseError {
    UnknownKind { kind: String },
    Toml { error: String },
}

impl core::fmt::Display for NodeDefParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownKind { kind } => write!(f, "unknown node kind `{kind}`"),
            Self::Toml { error } => f.write_str(error),
        }
    }
}

fn take_kind(payload: &mut toml::Value) -> Result<String, NodeDefParseError> {
    let Some(table) = payload.as_table_mut() else {
        return Err(NodeDefParseError::Toml {
            error: String::from("node definition TOML root must be a table"),
        });
    };
    let Some(kind) = table.remove("kind") else {
        return Err(NodeDefParseError::Toml {
            error: String::from("missing required field `kind`"),
        });
    };
    kind.as_str()
        .map(String::from)
        .ok_or_else(|| NodeDefParseError::Toml {
            error: String::from("field `kind` must be a string"),
        })
}

fn read_variant<T>(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    payload: toml::Value,
) -> Result<T, NodeDefParseError>
where
    T: SlotMutAccess + 'static,
{
    let object = registry
        .read_slot_toml(shape_id, &payload)
        .map_err(|error| NodeDefParseError::Toml {
            error: error.to_string(),
        })?;
    object
        .into_any()
        .downcast::<T>()
        .map(|def| *def)
        .map_err(|_| NodeDefParseError::Toml {
            error: format!("slot reader returned unexpected type for shape {shape_id}"),
        })
}

fn toml_parse_error(error: toml::de::Error) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: format!("{error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    use crate::{BindingRef, LpValue, MappingConfig, SlotShapeRegistry, TextureDef};

    #[test]
    fn node_def_delegates_kind_and_slots() {
        let def = NodeDef::Texture(TextureDef::new(64, 48));

        assert_eq!(def.kind(), NodeKind::Texture);
        assert_eq!(def.kind_name(), "texture");
        assert_eq!(def.shape_id(), TextureDef::SHAPE_ID);
    }

    #[test]
    fn node_def_parses_project_and_texture_toml() {
        let registry = registry();
        let project = NodeDef::from_toml_str_with_registry(
            &registry,
            r#"
kind = "project"

[nodes.texture]
artifact = "./texture.toml"
"#,
        )
        .expect("project");
        assert!(matches!(project, NodeDef::Project(_)));

        let texture = NodeDef::from_toml_str_with_registry(
            &registry,
            r#"
kind = "texture"
size = { width = 64, height = 48 }
"#,
        )
        .expect("texture");
        assert!(matches!(texture, NodeDef::Texture(_)));
    }

    #[test]
    fn node_def_parses_shader_output_and_fixture_toml() {
        let registry = registry();

        let shader = NodeDef::from_toml_str_with_registry(
            &registry,
            r#"
kind = "shader"
glsl_path = "shader.glsl"
render_order = 2

[bindings.visual]
target = "bus#visual.out"
"#,
        )
        .expect("shader");
        assert!(matches!(shader, NodeDef::Shader(_)));

        let output = NodeDef::from_toml_str_with_registry(
            &registry,
            r#"
kind = "output"
pin = 18

[options]
brightness = 0.5
"#,
        )
        .expect("output");
        assert!(matches!(output, NodeDef::Output(_)));

        let fixture = NodeDef::from_toml_str_with_registry(
            &registry,
            r#"
kind = "fixture"
render_size = { width = 8, height = 8 }
mapping = { kind = "path_points" }
"#,
        )
        .expect("fixture");
        let NodeDef::Fixture(fixture) = fixture else {
            panic!("expected fixture");
        };
        assert!(matches!(
            fixture.mapping.value(),
            MappingConfig::PathPoints { .. }
        ));
    }

    #[test]
    fn node_def_rejects_missing_invalid_and_unknown_kind() {
        let registry = registry();

        let missing = NodeDef::from_toml_str_with_registry(&registry, "name = \"missing\"")
            .expect_err("missing kind");
        assert!(missing.to_string().contains("kind"));

        let invalid =
            NodeDef::from_toml_str_with_registry(&registry, "kind = 7").expect_err("invalid kind");
        assert!(invalid.to_string().contains("string"));

        let unknown = NodeDef::from_toml_str_with_registry(&registry, "kind = \"bogus\"")
            .expect_err("unknown kind");
        assert_eq!(
            unknown,
            NodeDefParseError::UnknownKind {
                kind: String::from("bogus")
            }
        );
    }

    #[test]
    fn node_def_consumes_kind_before_slotcodec_record_read() {
        let registry = registry();

        let def = NodeDef::from_toml_str_with_registry(
            &registry,
            r#"
kind = "texture"
size = { width = 1, height = 2 }
"#,
        )
        .expect("texture");

        let NodeDef::Texture(def) = def else {
            panic!("expected texture");
        };
        assert_eq!(def.size.value().width, 1);
        assert_eq!(def.size.value().height, 2);
    }

    #[test]
    fn node_def_reads_binding_values_and_refs() {
        let registry = registry();

        let def = NodeDef::from_toml_str_with_registry(
            &registry,
            r##"
kind = "output"
pin = 18

[bindings.main]
value = 0.25
"##,
        )
        .expect("output");
        let NodeDef::Output(def) = def else {
            panic!("expected output");
        };
        let binding = def.bindings.0.entries.get("main").expect("binding");
        assert_eq!(binding.value_literal(), Some(&LpValue::F32(0.25)));

        let def = NodeDef::from_toml_str_with_registry(
            &registry,
            r##"
kind = "output"
pin = 18

[bindings.main]
target = "bus#control.out"
"##,
        )
        .expect("output target");
        let NodeDef::Output(def) = def else {
            panic!("expected output");
        };
        let binding = def.bindings.0.entries.get("main").expect("binding");
        assert!(matches!(binding.target_ref(), Some(BindingRef::Bus(_))));
    }

    fn registry() -> SlotShapeRegistry {
        let mut registry = SlotShapeRegistry::default();
        crate::slot_shapes::register_all_static_slot_shapes(&mut registry).expect("shapes");
        registry
    }
}
