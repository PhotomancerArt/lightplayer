//! [`AgentHost`]: the seam between the agent and the surrounding app.
//!
//! Studio implements this (P5); tests and evals use stubs. The host owns the
//! shader's source of truth (overlay staging) and the fixture knowledge; the
//! agent core never touches project state directly.

use lps_probe::LedPoint;

/// Injected by the embedding app. The write surface is deliberately tiny:
/// the agent can only stage this one shader's source.
pub trait AgentHost {
    /// The shader source as the user currently sees it (including any
    /// staged-but-unsaved edits).
    fn current_source(&self) -> Result<String, HostError>;

    /// Stage new source as an unsaved overlay edit (the user can Save or
    /// revert it).
    fn stage_source(&mut self, source: &str) -> Result<(), HostError>;

    /// The target fixture's LED sample points (normalized coordinates).
    fn led_points(&self) -> Vec<LedPoint>;

    /// Context for the system prompt: bindings table, fixture summary,
    /// node/project names.
    fn shader_context(&self) -> ShaderContext;
}

/// A host-side failure (project unavailable, write refused, ...). These are
/// the only failures reported as `is_error` tool results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub message: String,
}

impl HostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// What the prompt builder knows about the shader being edited.
#[derive(Clone, Debug, Default)]
pub struct ShaderContext {
    pub project_name: String,
    pub node_name: String,
    /// The fixture this shader drives, when one is wired.
    pub fixture: Option<FixtureSummary>,
    /// Declared uniform bindings with their current values.
    pub bindings: Vec<BindingInfo>,
}

/// Fixture summary for the prompt.
#[derive(Clone, Debug)]
pub struct FixtureSummary {
    pub name: String,
    pub led_count: u32,
    /// Human-readable mapping kind (e.g. "2D grid", "strip").
    pub mapping_kind: String,
}

/// One declared uniform binding.
#[derive(Clone, Debug)]
pub struct BindingInfo {
    /// Uniform path (e.g. `time`, `cfg.speed`).
    pub name: String,
    /// GLSL type name (e.g. `float`, `vec3`).
    pub ty: String,
    /// Current value, human-formatted.
    pub value: String,
}
