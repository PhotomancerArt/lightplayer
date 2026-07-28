//! The shader agent surface: per-shader-node chat sessions over
//! `lpa-agent`, owned by [`AgentController`] inside the studio controller.
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
pub mod ui_agent_view;

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
pub use ui_agent_view::{
    UiAgentAvailability, UiAgentDebugDump, UiAgentHistoryEntry, UiAgentModelView, UiAgentStatus,
    UiAgentToolRow, UiAgentTurn, UiAgentUsage, UiAgentView,
};
