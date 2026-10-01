//! [`UiAppAgentView`]: the app-level chat as the view renders it.

use crate::app::agent::agent_controller::AgentController;
use crate::app::agent::agent_op::AgentOp;
use crate::app::agent::ui_agent_view::{
    UiAgentAvailability, UiAgentModelView, UiAgentStatus, UiAgentTurn, UiAgentUsage,
};
use crate::app::settings::agent_provider::AgentProviderGuidance;
use crate::{ControllerId, UiAction};

/// The app chat's DTO, carried on [`crate::UiStudioView::app_agent`].
#[derive(Clone, Debug, PartialEq)]
pub struct UiAppAgentView {
    pub availability: UiAgentAvailability,
    /// Onboarding guidance while `availability` is `NeedsKey`.
    pub setup: Option<AgentProviderGuidance>,
    pub status: UiAgentStatus,
    pub turns: Vec<UiAgentTurn>,
    pub usage: UiAgentUsage,
    /// Display-ready cost (reported by the provider when it reports).
    pub estimated_cost: Option<String>,
    pub model: UiAgentModelView,
}

impl Default for UiAppAgentView {
    fn default() -> Self {
        Self {
            availability: UiAgentAvailability::NeedsKey,
            setup: None,
            status: UiAgentStatus::Idle,
            turns: Vec::new(),
            usage: UiAgentUsage::default(),
            estimated_cost: None,
            model: UiAgentModelView::default(),
        }
    }
}

impl UiAppAgentView {
    /// True while a run is in flight.
    pub fn busy(&self) -> bool {
        matches!(
            self.status,
            UiAgentStatus::Streaming | UiAgentStatus::RunningTool
        )
    }

    /// The Send action for one composed message.
    pub fn send_action(&self, text: &str) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::AppSend {
                text: text.to_string(),
            },
        )
    }

    /// The Stop action for the running turn.
    pub fn stop_action(&self) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::AppStop,
        )
    }
}
