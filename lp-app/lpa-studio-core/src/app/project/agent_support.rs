//! Project-side lookups the shader agent's run start needs.
//!
//! Small data carriers only — the resolution logic lives on
//! [`ProjectController`](super::project_controller::ProjectController)
//! (`agent_shader_target`, `agent_fixture_defs`) because it reads private
//! controller state (node tree, def-artifact map).

/// The shader node behind one source artifact, resolved for a run start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentShaderTarget {
    /// The node's stable address display (the session-key half).
    pub node_address: String,
    /// Human-readable node label (system-prompt context).
    pub node_label: String,
    /// The declared uniform bindings with their authored defaults.
    pub bindings: Vec<AgentShaderBinding>,
}

/// One declared uniform of the target shader.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentShaderBinding {
    /// Uniform path (e.g. `time`).
    pub name: String,
    /// GLSL type name as the generated uniform header declares it.
    pub ty: String,
    /// Authored default value display, when one exists (values are
    /// bus-driven at runtime).
    pub value: Option<String>,
}
