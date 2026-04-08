//! GLSL → annotated RV32 assembly (`lpvm-native`).

mod args;
mod handler;

pub use args::ShaderRv32Args;
pub use handler::handle_shader_rv32;
