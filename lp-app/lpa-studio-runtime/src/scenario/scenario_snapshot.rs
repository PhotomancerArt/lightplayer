use lpa_studio_core::{LinkState, StudioApp};
use serde::{Deserialize, Serialize};

/// Lightweight journey snapshot captured after scenario actions and events.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ScenarioSnapshot {
    pub label: String,
    pub flow: LinkState,
    pub issue_count: usize,
    pub log_count: usize,
    pub diagnostic_count: usize,
    pub project_id: Option<String>,
}

impl ScenarioSnapshot {
    pub fn from_app(label: impl Into<String>, app: &StudioApp) -> Self {
        Self {
            label: label.into(),
            flow: app.state().device_manager.active_flow.clone(),
            issue_count: app.state().device_manager.issues.len(),
            log_count: app.state().logs.len(),
            diagnostic_count: app.state().diagnostics.len(),
            project_id: app
                .state()
                .project_session
                .as_ref()
                .map(|project| project.project_id.clone()),
        }
    }
}
