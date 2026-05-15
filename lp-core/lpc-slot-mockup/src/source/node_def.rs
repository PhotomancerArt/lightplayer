use lpc_model::{__private::Box, SlotAccess, SlotDataAccess, SlotShapeId};

use super::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef};

/// Mock authored node-definition wrapper.
///
/// This mirrors the real domain's closed authored node set and gives the
/// mockup a real place to prove discriminator-based slot codec dispatch.
pub enum NodeDef {
    Project(ProjectDef),
    Output(OutputDef),
    Texture(TextureDef),
    Fixture(FixtureDef),
    Shader(ShaderDef),
}

impl NodeDef {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Project(_) => ProjectDef::KIND,
            Self::Output(_) => OutputDef::KIND,
            Self::Texture(_) => TextureDef::KIND,
            Self::Fixture(_) => FixtureDef::KIND,
            Self::Shader(_) => ShaderDef::KIND,
        }
    }

    pub fn as_project(&self) -> Option<&ProjectDef> {
        match self {
            Self::Project(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_output(&self) -> Option<&OutputDef> {
        match self {
            Self::Output(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_texture(&self) -> Option<&TextureDef> {
        match self {
            Self::Texture(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_fixture(&self) -> Option<&FixtureDef> {
        match self {
            Self::Fixture(def) => Some(def),
            _ => None,
        }
    }

    pub fn as_shader(&self) -> Option<&ShaderDef> {
        match self {
            Self::Shader(def) => Some(def),
            _ => None,
        }
    }
}

impl SlotAccess for NodeDef {
    fn shape_id(&self) -> SlotShapeId {
        match self {
            Self::Project(def) => def.shape_id(),
            Self::Output(def) => def.shape_id(),
            Self::Texture(def) => def.shape_id(),
            Self::Fixture(def) => def.shape_id(),
            Self::Shader(def) => def.shape_id(),
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Project(def) => def.data(),
            Self::Output(def) => def.data(),
            Self::Texture(def) => def.data(),
            Self::Fixture(def) => def.data(),
            Self::Shader(def) => def.data(),
        }
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
        self
    }
}
