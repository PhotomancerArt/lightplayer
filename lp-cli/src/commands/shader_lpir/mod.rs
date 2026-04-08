//! Compile a GLSL file to LPIR text (same path as filetests / JIT frontend).

mod args;
mod handler;

pub use args::ShaderLpirArgs;
pub use handler::handle_shader_lpir;
