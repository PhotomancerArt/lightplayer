mod fixture_node;
mod output_node;
mod runtime;
mod shader_node;

use lpc_model::StaticSlotAccess;

pub use fixture_node::{FixtureNode, TouchState};
pub use output_node::OutputNode;
pub use runtime::MockRuntime;
pub use shader_node::ShaderNode;

pub(crate) fn register_shapes(registry: &mut lpc_model::SlotShapeRegistry) {
    ShaderNode::register_shape(registry).unwrap();
    FixtureNode::register_shape(registry).unwrap();
    OutputNode::register_shape(registry).unwrap();
}
