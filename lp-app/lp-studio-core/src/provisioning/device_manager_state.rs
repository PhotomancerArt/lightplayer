use serde::{Deserialize, Serialize};

use crate::{ConnectedDeviceState, DeviceFlowState, DeviceIssue, ProviderCatalog};

/// UI-independent read model for the Studio device/provisioning surface.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceManagerState {
    pub providers: ProviderCatalog,
    pub active_flow: DeviceFlowState,
    pub current_device: Option<ConnectedDeviceState>,
    pub issues: Vec<DeviceIssue>,
}

impl DeviceManagerState {
    pub fn new() -> Self {
        Self {
            providers: ProviderCatalog::new(),
            active_flow: DeviceFlowState::default(),
            current_device: None,
            issues: Vec::new(),
        }
    }

    pub fn push_issue(&mut self, issue: DeviceIssue) {
        self.issues.retain(|entry| entry.id != issue.id);
        self.issues.push(issue);
    }

    pub fn clear_issue(&mut self, issue_id: &str) {
        self.issues.retain(|issue| issue.id != issue_id);
    }
}

impl Default for DeviceManagerState {
    fn default() -> Self {
        Self::new()
    }
}
