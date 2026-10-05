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
    /// Send one user message to the app-level chat (one per page).
    AppSend { text: String },
    /// The app agent's `edit_project` batch (dispatched by its host bridge,
    /// not the web layer): apply the edits in order through the project's
    /// own node and slot ops, then record one status per edit into the app
    /// bridge cell under `seq`.
    ApplyProjectEdits {
        seq: u64,
        input: lpa_agent::EditProjectInput,
    },
    /// Flip the app chat's abort flag (its Stop button).
    AppStop,
    /// The app agent's `read` (dispatched by its host bridge): answer from
    /// what the controller holds into the app bridge cell under `seq`.
    AppRead {
        seq: u64,
        input: lpa_agent::ReadInput,
    },
    /// The app agent's `act` (dispatched by its host bridge): press the
    /// offer its path names — or, when only the user may press it,
    /// put it on a card — and record the outcome under `seq`.
    AppAct {
        seq: u64,
        input: lpa_agent::ActInput,
    },
    /// The user's No on an app-agent card: mark it dismissed and let the
    /// assistant hear it.
    CardDismissed { card: String },
    /// The user's Show on an app-chat row (the `show/<target>` offer):
    /// bring the control at `target` into view and light it again. A node
    /// (or a node's verb) focuses that node's card, the way a tree-row
    /// click does; the page scrolls to the control. It never moves
    /// keyboard focus.
    Show { target: crate::OfferPath },
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
            Self::AppSend { .. } => ActionMeta::new(
                "Send",
                "Send a message to the LightPlayer assistant.",
                ActionPriority::Primary,
            ),
            Self::ApplyProjectEdits { .. } => ActionMeta::new(
                "Apply edits",
                "Apply the assistant's project edits as ordinary edits.",
                ActionPriority::Primary,
            ),
            Self::AppRead { .. } => ActionMeta::new(
                "Read",
                "Answer the assistant's read.",
                ActionPriority::Secondary,
            ),
            Self::AppAct { .. } => ActionMeta::new(
                "Act",
                "Press the action the assistant named.",
                ActionPriority::Secondary,
            ),
            Self::CardDismissed { .. } => ActionMeta::new(
                "Dismiss",
                "Don't do this; tell the assistant.",
                ActionPriority::Secondary,
            ),
            Self::Show { .. } => ActionMeta::new(
                "Show",
                "Bring this into view and light it.",
                ActionPriority::Tertiary,
            ),
            Self::AppStop => ActionMeta::new(
                "Stop",
                "Stop the assistant's running turn.",
                ActionPriority::Secondary,
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
