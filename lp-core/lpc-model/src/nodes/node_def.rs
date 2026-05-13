//! Canonical authored node definition enum.
//!
//! This is the closed set of core node definitions understood by the current
//! LightPlayer model. Adding a core node kind should start here, then add the
//! concrete definition type and loader/runtime handling that variant requires.

use alloc::format;
use alloc::string::String;

use crate::node::kind::NodeKind;
use crate::nodes::fixture::FixtureDef;
use crate::nodes::fluid::FluidDef;
use crate::nodes::output::OutputDef;
use crate::nodes::project::ProjectDef;
use crate::nodes::shader::{ComputeShaderDef, ShaderDef};
use crate::nodes::texture::TextureDef;
use crate::{SlotAccess, SlotDataAccess, SlotShapeId};

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
    ComputeShader(ComputeShaderDef),
    Fluid(FluidDef),
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
            Self::ComputeShader(_) => NodeKind::ComputeShader,
            Self::Fluid(_) => NodeKind::Fluid,
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
            Self::ComputeShader(_) => ComputeShaderDef::KIND,
            Self::Fluid(_) => FluidDef::KIND,
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

    pub fn as_compute_shader(&self) -> Option<&ComputeShaderDef> {
        match self {
            Self::ComputeShader(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_fluid(&self) -> Option<&FluidDef> {
        match self {
            Self::Fluid(def) => Some(def),
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

    /// Parse a TOML node artifact into the canonical node-definition enum.
    pub fn from_toml_str(text: &str) -> Result<Self, NodeDefParseError> {
        let probe: NodeDefKindProbe =
            toml::from_str(text).map_err(|error| NodeDefParseError::Toml {
                error: format!("{error}"),
            })?;
        match probe.kind.as_str() {
            ProjectDef::KIND => parse_variant(text).map(Self::Project),
            TextureDef::KIND => parse_variant(text).map(Self::Texture),
            ShaderDef::KIND => parse_variant(text).map(Self::Shader),
            ComputeShaderDef::KIND => parse_variant(text).map(Self::ComputeShader),
            FluidDef::KIND => parse_variant(text).map(Self::Fluid),
            OutputDef::KIND => parse_variant(text).map(Self::Output),
            FixtureDef::KIND => parse_variant(text).map(Self::Fixture),
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
            Self::ComputeShader(def) => def.shape_id(),
            Self::Fluid(def) => def.shape_id(),
            Self::Output(def) => def.shape_id(),
            Self::Fixture(def) => def.shape_id(),
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Project(def) => def.data(),
            Self::Texture(def) => def.data(),
            Self::Shader(def) => def.data(),
            Self::ComputeShader(def) => def.data(),
            Self::Fluid(def) => def.data(),
            Self::Output(def) => def.data(),
            Self::Fixture(def) => def.data(),
        }
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

#[derive(serde::Deserialize)]
struct NodeDefKindProbe {
    kind: String,
}

fn parse_variant<T>(text: &str) -> Result<T, NodeDefParseError>
where
    T: serde::de::DeserializeOwned,
{
    toml::from_str(text).map_err(|error| NodeDefParseError::Toml {
        error: format!("{error}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StaticSlotShape, TextureDef};

    #[test]
    fn node_def_delegates_kind_and_slots() {
        let def = NodeDef::Texture(TextureDef::new(64, 48));

        assert_eq!(def.kind(), NodeKind::Texture);
        assert_eq!(def.kind_name(), "texture");
        assert_eq!(def.shape_id(), TextureDef::SHAPE_ID);
    }

    #[test]
    fn node_def_parses_project_and_texture_toml() {
        let project = NodeDef::from_toml_str(
            r#"
kind = "project"

[nodes.texture]
artifact = "./texture.toml"
"#,
        )
        .expect("project");
        assert!(matches!(project, NodeDef::Project(_)));

        let texture = NodeDef::from_toml_str(
            r#"
kind = "texture"
size = { width = 64, height = 48 }
"#,
        )
        .expect("texture");
        assert!(matches!(texture, NodeDef::Texture(_)));
    }
}
