mod fixture_def;
mod mapping;
mod output_def;
mod project_def;
mod shader_def;
mod texture_def;

use lpc_model::StaticSlotAccess;

pub use fixture_def::FixtureDef;
pub use mapping::{FixtureMapping, MappingPoint, PathSpec};
pub use output_def::OutputDef;
pub use project_def::{NodeInvocationDef, ProjectDef};
pub use shader_def::{CompilerOptions, ScalarHint, ShaderDef, ShaderParamDef};
pub use texture_def::TextureDef;

pub(crate) fn register_shapes(registry: &mut lpc_model::SlotShapeRegistry) {
    ProjectDef::register_shape(registry).unwrap();
    ShaderDef::register_shape(registry).unwrap();
    FixtureDef::register_shape(registry).unwrap();
    OutputDef::register_shape(registry).unwrap();
    TextureDef::register_shape(registry).unwrap();
}
