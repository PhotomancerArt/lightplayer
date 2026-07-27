//! Project-side lookups the shader agent's run start needs.
//!
//! Small data carriers only — the resolution logic lives on
//! [`ProjectController`](super::project_controller::ProjectController)
//! (`agent_shader_target`, `agent_fixture_defs`, `agent_engine_status`)
//! because it reads private controller state (node tree, def-artifact map).

use lpa_agent::{EngineStatusKind, EngineVerdict};
use lpc_model::Revision;

use crate::{ProjectNodeStatusTone, ProjectNodeStatusView, UiShaderError};

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

/// The engine's latest status for one shader node, as a verdict the agent
/// bridge serves: the status Revision (`change_frame`) plus the parsed
/// classification. Written into the bridge cell on every pull.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentEngineStatus {
    /// The engine frame the status last changed at.
    pub revision: Revision,
    pub verdict: EngineVerdict,
}

/// Map one node status view to the agent-facing verdict. Error tones parse
/// through [`UiShaderError`] (one prefix-strip implementation for the error
/// strip AND the agent); warnings stay `Ok` with their text; neutral states
/// (pending/created) are `Unknown`.
pub(crate) fn engine_verdict(status: &ProjectNodeStatusView) -> EngineVerdict {
    match status.tone {
        ProjectNodeStatusTone::Error => match status.detail.as_deref() {
            Some(detail) => {
                let parsed = UiShaderError::parse(detail);
                EngineVerdict {
                    status: EngineStatusKind::Error,
                    message: Some(parsed.message),
                    line_col: parsed.line_col,
                }
            }
            None => EngineVerdict {
                status: EngineStatusKind::Error,
                message: Some(status.label.clone()),
                line_col: None,
            },
        },
        ProjectNodeStatusTone::Good | ProjectNodeStatusTone::Warning => EngineVerdict {
            status: EngineStatusKind::Ok,
            message: status.detail.clone(),
            line_col: None,
        },
        ProjectNodeStatusTone::Neutral => EngineVerdict {
            status: EngineStatusKind::Unknown,
            message: None,
            line_col: None,
        },
    }
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
