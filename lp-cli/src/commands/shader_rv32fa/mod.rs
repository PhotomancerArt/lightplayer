//! `shader-rv32n` — fastalloc RV32FA debug compiler CLI.

mod args;
mod handler;
pub mod pipeline;

pub use args::Args;
pub use handler::handle_shader_rv32fa;
