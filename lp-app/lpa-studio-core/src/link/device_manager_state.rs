use serde::{Deserialize, Serialize};

use crate::{
    ActionConfirmation, AvailableAction, ConnectedDeviceState, DeviceIssue, LinkActionRequest,
    LinkState, ProviderCatalog,
};

/// UI-independent read model for the Studio device/link surface.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceManagerState {
    pub providers: ProviderCatalog,
    pub active_flow: LinkState,
    pub current_device: Option<ConnectedDeviceState>,
    pub issues: Vec<DeviceIssue>,
}

impl DeviceManagerState {
    pub fn new() -> Self {
        Self {
            providers: ProviderCatalog::new(),
            active_flow: LinkState::default(),
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

    pub fn available_actions(&self) -> Vec<AvailableAction<LinkActionRequest>> {
        let mut actions = match &self.active_flow {
            LinkState::Empty | LinkState::ChoosingProvider => {
                let mut actions = Vec::with_capacity(self.providers.providers.len() + 1);
                actions.push(available(LinkActionRequest::RefreshProviderCatalog).tertiary());
                actions.extend(self.providers.providers.iter().map(|provider| {
                    let action = LinkActionRequest::StartProvisioning {
                        provider_id: provider.provider_id.clone(),
                    };
                    let available_action = available(action).primary();
                    if provider.availability.can_start() {
                        available_action
                    } else {
                        available_action.disabled()
                    }
                }));
                actions
            }
            LinkState::RequestingAccess { .. } | LinkState::Opening { .. } => {
                vec![available(LinkActionRequest::CancelProvisioning).tertiary()]
            }
            LinkState::AccessFailed { .. } | LinkState::OpenFailed { .. } => vec![
                available(LinkActionRequest::RetryProvisioning).primary(),
                available(LinkActionRequest::CancelProvisioning).tertiary(),
            ],
            LinkState::ProbingTarget { endpoint_id } => vec![
                available(LinkActionRequest::ProbeTarget {
                    endpoint_id: Some(endpoint_id.clone()),
                })
                .disabled(),
                available(LinkActionRequest::CancelProvisioning).tertiary(),
            ],
            LinkState::ProvisioningRequired {
                endpoint_id,
                reason: _,
            } => vec![
                available(LinkActionRequest::ConfirmFirmwareFlash {
                    endpoint_id: endpoint_id.clone(),
                    firmware_id: None,
                })
                .primary()
                .with_confirmation(firmware_flash_confirmation()),
                available(LinkActionRequest::Disconnect).tertiary(),
                available(LinkActionRequest::CancelProvisioning).tertiary(),
            ],
            LinkState::ConfirmingFirmwareFlash {
                endpoint_id,
                firmware_id,
            } => vec![
                available(LinkActionRequest::ConfirmFirmwareFlash {
                    endpoint_id: endpoint_id.clone(),
                    firmware_id: firmware_id.clone(),
                })
                .danger()
                .with_confirmation(firmware_flash_confirmation()),
                available(LinkActionRequest::CancelProvisioning).tertiary(),
            ],
            LinkState::Flashing { .. } => Vec::new(),
            LinkState::OpeningServer { .. }
            | LinkState::ServerReady { .. }
            | LinkState::ReadingProjectState { .. }
            | LinkState::ProjectSelectionRequired { .. }
            | LinkState::RecoveryRequired { .. }
            | LinkState::DeployingProject { .. }
            | LinkState::Ready { .. } => vec![
                available(LinkActionRequest::Disconnect).tertiary(),
                available(LinkActionRequest::Reset).tertiary(),
            ],
            LinkState::Degraded { .. } => vec![
                available(LinkActionRequest::RetryProvisioning).primary(),
                available(LinkActionRequest::Disconnect).tertiary(),
            ],
            LinkState::Disconnected { .. } => {
                vec![available(LinkActionRequest::RefreshProviderCatalog).primary()]
            }
        };

        actions.extend(self.issues.iter().map(|issue| {
            available(LinkActionRequest::AcknowledgeIssue {
                issue_id: issue.id.clone(),
            })
            .tertiary()
        }));
        actions
    }
}

impl Default for DeviceManagerState {
    fn default() -> Self {
        Self::new()
    }
}

fn available(action: LinkActionRequest) -> AvailableAction<LinkActionRequest> {
    AvailableAction::new(action.clone(), action.action_type().into())
}

fn firmware_flash_confirmation() -> ActionConfirmation {
    ActionConfirmation::new(
        "Flash firmware",
        "Flashing firmware can erase or replace the target device firmware.",
        "Flash",
        true,
    )
}
