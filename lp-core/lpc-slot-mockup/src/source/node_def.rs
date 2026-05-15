use lpc_model::{
    __private::Box,
    SlotAccess, SlotDataAccess, SlotShapeId,
    slot_codec::{
        SlotCodec, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError, SyntaxEventSource,
        ValueReader,
    },
};

use super::{FixtureDef, OutputDef, ProjectDef, ShaderDef, TextureDef};
use crate::generated_slot_codec::{
    read_fixture_def_slot_body, read_output_def_slot_body, read_project_def_slot_body,
    read_shader_def_slot_body, read_texture_def_slot_body, write_fixture_def_slot_body,
    write_output_def_slot_body, write_project_def_slot_body, write_shader_def_slot_body,
    write_texture_def_slot_body,
};

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

impl SlotCodec for NodeDef {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let mut object = value.object()?;
        let kind = object.expect_discriminator(
            "kind",
            &[
                ProjectDef::KIND,
                OutputDef::KIND,
                TextureDef::KIND,
                FixtureDef::KIND,
                ShaderDef::KIND,
            ],
        )?;
        match kind.as_str() {
            ProjectDef::KIND => read_project_def_slot_body(object).map(Self::Project),
            OutputDef::KIND => read_output_def_slot_body(object).map(Self::Output),
            TextureDef::KIND => read_texture_def_slot_body(object).map(Self::Texture),
            FixtureDef::KIND => read_fixture_def_slot_body(object).map(Self::Fixture),
            ShaderDef::KIND => read_shader_def_slot_body(object).map(Self::Shader),
            _ => unreachable!("expect_discriminator validated variants"),
        }
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let mut object = value.object()?;
        match self {
            Self::Project(project) => {
                object.prop("kind")?.string(ProjectDef::KIND)?;
                write_project_def_slot_body(&mut object, project)?;
            }
            Self::Output(output) => {
                object.prop("kind")?.string(OutputDef::KIND)?;
                write_output_def_slot_body(&mut object, output)?;
            }
            Self::Texture(texture) => {
                object.prop("kind")?.string(TextureDef::KIND)?;
                write_texture_def_slot_body(&mut object, texture)?;
            }
            Self::Fixture(fixture) => {
                object.prop("kind")?.string(FixtureDef::KIND)?;
                write_fixture_def_slot_body(&mut object, fixture)?;
            }
            Self::Shader(shader) => {
                object.prop("kind")?.string(ShaderDef::KIND)?;
                write_shader_def_slot_body(&mut object, shader)?;
            }
        }
        object.finish()
    }
}
