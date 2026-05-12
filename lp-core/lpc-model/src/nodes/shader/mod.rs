pub mod compute_shader_def;
pub mod glsl_opts;
pub mod shader_def;
pub mod shader_header_gen;
pub mod shader_slot_def;
pub mod shader_slot_mapping;
pub mod shader_state;

pub use crate::slot_views::ShaderDefView;
pub use compute_shader_def::ComputeShaderDef;
pub use glsl_opts::{AddSubMode, DivMode, GlslOpts, MulMode};
pub use shader_def::ShaderDef;
pub use shader_header_gen::{ShaderHeaderGenError, generate_compute_shader_header};
pub use shader_slot_def::{ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind, ShaderValueShapeRef};
pub use shader_slot_mapping::{ShaderSlotMappingDef, ShaderSlotMappingKind};
pub use shader_state::ShaderState;
