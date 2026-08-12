//! [`AgentOp`]: chat-surface gestures riding the normal action queue.
//!
//! The web layer never constructs these directly — [`crate::UiAgentView`]
//! prebuilds the actions ([`crate::UiAgentView::send_action`] /
//! [`crate::UiAgentView::stop_action`]) so no domain types leak into the
//! view, mirroring [`crate::UiAssetEditor::apply_action`].

use core::any::Any;

use lpc_model::ArtifactLocation;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// A gesture on one shader's agent chat, targeting the shader through its
/// source artifact (the same identity the inline editor edits by).
#[derive(Clone, Debug, PartialEq)]
pub enum AgentOp {
    /// Send one user message: resolve the shader's context, then spawn the
    /// agent run (the dispatch itself returns immediately; progress arrives
    /// as [`crate::AgentFeedback`] commands).
    Send {
        artifact: ArtifactLocation,
        text: String,
    },
    /// Flip the running session's abort flag (the Stop button).
    Stop { artifact: ArtifactLocation },
    /// Restage the source of one session edit record (the history strip's
    /// revert): pull the recorded source, dispatch it through the SAME
    /// `AssetEditOp::ApplyBody` overlay path a staged agent edit rides, and
    /// mirror it into the session's bridge state so the next run's
    /// `current_source` agrees. Refused while a run is in flight.
    RevertToTurn {
        artifact: ArtifactLocation,
        /// The edit record's session-scoped ordinal.
        turn: u32,
    },
    /// Build the debug export for one session: the raw model-facing
    /// transcript dump lands on the session's DTO
    /// ([`crate::UiAgentView::debug`]) with a fresh `seq`, and the web
    /// shell downloads it. Refused while a run is in flight — the raw
    /// transcript is parked in the controller only between runs.
    ExportDebug { artifact: ArtifactLocation },
    /// The agent's `upsert_param` write (dispatched by the host bridge, not
    /// the web layer): send ONE `PutSlotEdit` batch on the target node's
    /// def artifact and record the outcome into the session's bridge cell
    /// under `seq`, where the awaiting run future polls it up.
    UpsertParam {
        artifact: ArtifactLocation,
        /// Bridge-allocated correlation id for the ack.
        seq: u64,
        upsert: lpa_agent::ParamUpsert,
    },
    /// The agent's `declare_space` write (dispatched by the host bridge,
    /// not the web layer): send ONE `PutSlotEdit` batch on the target
    /// node's def artifact — the SAME ops the dimensionality section's
    /// tiles dispatch — and record the outcome into the session's bridge
    /// cell under `seq`, where the awaiting run future polls it up.
    DeclareSpace {
        artifact: ArtifactLocation,
        /// Bridge-allocated correlation id for the ack (the same counter
        /// `UpsertParam` draws from — only one agent write is ever in
        /// flight).
        seq: u64,
        declaration: lpa_agent::SpaceDeclaration,
    },
}

impl ControllerOp for AgentOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::Send { .. } => ActionMeta::new(
                "Send",
                "Send a message to the shader agent.",
                ActionPriority::Primary,
            ),
            Self::Stop { .. } => ActionMeta::new(
                "Stop",
                "Stop the running agent turn.",
                ActionPriority::Secondary,
            ),
            Self::RevertToTurn { turn, .. } => ActionMeta::new(
                "Revert",
                format!("Restage the agent's edit {turn} as the shader source."),
                ActionPriority::Secondary,
            ),
            Self::ExportDebug { .. } => ActionMeta::new(
                "Export debug JSON",
                "Dump the model-facing transcript of this chat for debugging.",
                ActionPriority::Secondary,
            ),
            Self::UpsertParam { .. } => ActionMeta::new(
                "Upsert param",
                "Stage the agent's param record edit as a pending edit.",
                ActionPriority::Primary,
            ),
            Self::DeclareSpace { .. } => ActionMeta::new(
                "Declare space",
                "Stage the agent's dimensionality declaration as a pending edit.",
                ActionPriority::Primary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        // Editor-foreground, like the inline editor's Apply: preempts a
        // passive refresh so a Send/Stop never queues behind a slow pull.
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
