//! GLSL → annotated RV32 assembly (`lpvm-native`; linear path) or fastalloc dump (`--pipeline fast`).

mod args;
mod handler;

pub use args::ShaderRv32Args;
pub use handler::handle_shader_rv32;
