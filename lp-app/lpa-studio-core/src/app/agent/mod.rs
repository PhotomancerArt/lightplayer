//! The agent surfaces over `lpa-agent`, owned by [`AgentController`] inside
//! the studio controller: per-shader-node chat sessions, and the one
//! app-level chat ([`AppAgentSession`]) that builds and edits a project.
//!
//! The humble split: transcript, status, and usage live here as controller
//! state and reach the web layer as [`UiAgentView`] DTOs decorated onto the
//! shader editor's [`crate::UiAssetEditor`]. Chat gestures ride the normal
//! action queue as [`AgentOp`]s; a spawned run reports progress back through
//! [`AgentFeedback`] commands.

pub mod agent_chat_session;
pub mod agent_controller;
pub mod agent_debug_export;
pub mod agent_feedback;
pub mod agent_host_bridge;
pub mod agent_op;
pub mod agent_pricing;
pub mod agent_provider_config;
pub mod agent_session_key;
pub mod agent_transcript_mirror;
pub mod app_agent_host_bridge;
pub mod app_agent_readout;
pub mod app_agent_reference;
pub mod app_agent_session;
/// App-agent evals, stage A (test-only).
#[cfg(test)]
pub(crate) mod evals;
pub mod ui_agent_card;
pub mod ui_agent_edit_batch;
pub mod ui_agent_view;
pub mod ui_app_agent_view;

pub use agent_chat_session::{AgentEditRecord, MAX_EDIT_RECORDS};
pub use agent_controller::{
    AgentController, AgentModelsFetchFuture, AgentRunContext, AgentTaskFuture, AgentTimerFactory,
    AgentTimerFuture, AgentViewContext, instant_agent_timer,
};
pub use agent_feedback::AgentFeedback;
pub use agent_host_bridge::{AgentBridgeState, AgentHostBridge};
pub use agent_op::AgentOp;
pub use agent_pricing::{AgentCostRates, format_cost_usd};
pub use agent_provider_config::AgentProviderConfig;
pub use agent_session_key::AgentSessionKey;
pub use app_agent_host_bridge::{AppAgentBridgeState, AppAgentHostBridge};
pub use app_agent_session::AppAgentSession;
pub use ui_agent_card::{UiAgentCard, UiAgentCardState};
pub use ui_agent_edit_batch::{UiAgentEditBatch, UiAgentEditLine, UiAgentEditOutcome};
pub use ui_agent_view::{
    UiAgentAvailability, UiAgentDebugDump, UiAgentHistoryEntry, UiAgentModelView, UiAgentStatus,
    UiAgentToolRow, UiAgentTurn, UiAgentUsage, UiAgentView,
};
pub use ui_app_agent_view::UiAppAgentView;
