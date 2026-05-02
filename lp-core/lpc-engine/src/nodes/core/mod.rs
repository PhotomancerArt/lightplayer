//! First-class MVP core nodes (M4). Phase 3 installs placeholders only.

mod fixture_node;
mod output_node;
mod placeholder;
mod shader_node;
mod texture_node;

pub use fixture_node::FixtureNode;
pub use output_node::OutputNode;
pub use placeholder::CorePlaceholderNode;
pub use shader_node::{ShaderNode, shader_texture_output_path};
pub use texture_node::TextureNode;
