use crate::{ActionHistoryPolicy, StudioActionType};

/// High-level grouping for UI help and future agent tool presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionCategory {
    Device,
    Runtime,
    Project,
    Navigation,
}

/// Human and machine-readable description of an ux type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionDescriptor {
    pub action_type: StudioActionType,
    pub label: &'static str,
    pub summary: &'static str,
    pub category: ActionCategory,
    pub history_policy: ActionHistoryPolicy,
}

impl ActionDescriptor {
    pub fn for_type(action_type: StudioActionType) -> Self {
        match action_type {
            StudioActionType::RefreshProviderCatalog => Self::new(
                action_type,
                "Refresh provider catalog",
                "Ask the runtime which Studio providers are available.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::StartProvisioning => Self::new(
                action_type,
                "Start link",
                "Begin the device link flow with a selected provider.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::CancelProvisioning => Self::new(
                action_type,
                "Cancel link",
                "Cancel the active link flow and return to provider choice.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::RetryProvisioning => Self::new(
                action_type,
                "Retry link",
                "Retry the active provider/device link flow.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::SelectLinkProvider => Self::new(
                action_type,
                "Select link provider",
                "Choose which low-level link provider Studio should use.",
                ActionCategory::Device,
                ActionHistoryPolicy::Ephemeral,
            ),
            StudioActionType::RequestDeviceAccess => Self::new(
                action_type,
                "Request device access",
                "Ask the selected provider for user permission or device access.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::DiscoverDevices => Self::new(
                action_type,
                "Discover devices",
                "Ask the selected provider for available endpoints.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ConnectDevice => Self::new(
                action_type,
                "Connect device",
                "Open a link session and client connection for an endpoint.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ConnectSelectedEndpoint => Self::new(
                action_type,
                "Connect selected endpoint",
                "Open a link session for the selected provider endpoint.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ProbeTarget => Self::new(
                action_type,
                "Probe target",
                "Classify the selected endpoint before deciding whether link is needed.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::DisconnectDevice => Self::new(
                action_type,
                "Disconnect device",
                "Close the current link/device session.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ResetDevice => Self::new(
                action_type,
                "Reset device",
                "Ask the current link to reset or reboot the connected device.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ConfirmFirmwareFlash => Self::new(
                action_type,
                "Confirm firmware flash",
                "Confirm and start a firmware flashing operation for an endpoint.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::FlashDeviceFirmware => Self::new(
                action_type,
                "Flash device firmware",
                "Write a selected firmware image to the connected device.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::UploadDemoProject => Self::new(
                action_type,
                "Upload demo project",
                "Write the built-in Studio demo project through the server protocol.",
                ActionCategory::Project,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::LoadDemoProject => Self::new(
                action_type,
                "Load demo project",
                "Write and load the built-in Studio demo project.",
                ActionCategory::Project,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::AcknowledgeProvisioningIssue => Self::new(
                action_type,
                "Acknowledge link issue",
                "Dismiss a link issue from the Studio read model.",
                ActionCategory::Device,
                ActionHistoryPolicy::Ephemeral,
            ),
            StudioActionType::RefreshStatus => Self::new(
                action_type,
                "Refresh status",
                "Read lightweight runtime status from the current connection.",
                ActionCategory::Runtime,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ReadProjectState => Self::new(
                action_type,
                "Read project state",
                "Inspect the connected server before attaching, loading, or recovering a project.",
                ActionCategory::Project,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::ReadProjectInventory => Self::new(
                action_type,
                "Read project inventory",
                "Read effective project inventory from the loaded project.",
                ActionCategory::Project,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::SelectProjectNode => Self::new(
                action_type,
                "Select project node",
                "Select a project node in the Studio read model.",
                ActionCategory::Navigation,
                ActionHistoryPolicy::Ephemeral,
            ),
        }
    }

    pub fn catalog() -> Vec<Self> {
        StudioActionType::all()
            .into_iter()
            .map(Self::for_type)
            .collect()
    }

    fn new(
        action_type: StudioActionType,
        label: &'static str,
        summary: &'static str,
        category: ActionCategory,
        history_policy: ActionHistoryPolicy,
    ) -> Self {
        Self {
            action_type,
            label,
            summary,
            category,
            history_policy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_actions_are_not_undoable() {
        for action_type in [
            StudioActionType::RefreshProviderCatalog,
            StudioActionType::StartProvisioning,
            StudioActionType::CancelProvisioning,
            StudioActionType::RetryProvisioning,
            StudioActionType::DiscoverDevices,
            StudioActionType::ConnectDevice,
            StudioActionType::ConnectSelectedEndpoint,
            StudioActionType::ProbeTarget,
            StudioActionType::DisconnectDevice,
            StudioActionType::RequestDeviceAccess,
            StudioActionType::ResetDevice,
            StudioActionType::ConfirmFirmwareFlash,
            StudioActionType::FlashDeviceFirmware,
            StudioActionType::UploadDemoProject,
            StudioActionType::LoadDemoProject,
            StudioActionType::RefreshStatus,
            StudioActionType::ReadProjectState,
            StudioActionType::ReadProjectInventory,
        ] {
            assert!(
                ActionDescriptor::for_type(action_type)
                    .history_policy
                    .never()
            );
        }
    }

    #[test]
    fn navigation_actions_are_ephemeral() {
        let descriptor = ActionDescriptor::for_type(StudioActionType::SelectProjectNode);

        assert!(descriptor.history_policy.ephemeral());
    }

    #[test]
    fn issue_acknowledgement_is_ephemeral() {
        let descriptor = ActionDescriptor::for_type(StudioActionType::AcknowledgeProvisioningIssue);

        assert!(descriptor.history_policy.ephemeral());
    }
}
