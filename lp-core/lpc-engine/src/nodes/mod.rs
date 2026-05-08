pub mod fixture;
pub mod output;
mod placeholder;
pub mod shader;
pub mod texture;

pub use fixture::fixture_node::{FixtureNode, fixture_input_path};
pub use output::output_node::OutputNode;
pub use placeholder::CorePlaceholderNode;
pub use shader::shader_node::{ShaderNode, shader_output_path};
pub use texture::texture_node::TextureNode;
