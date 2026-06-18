use crate::{ActionHistoryPolicy, StudioActionType};

/// High-level grouping for UI help and future agent tool presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionCategory {
    Device,
    Runtime,
    Project,
    Navigation,
}

/// Human and machine-readable description of an action type.
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
            StudioActionType::SelectLinkProvider => Self::new(
                action_type,
                "Select link provider",
                "Choose which low-level link provider Studio should use.",
                ActionCategory::Device,
                ActionHistoryPolicy::Ephemeral,
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
            StudioActionType::DisconnectDevice => Self::new(
                action_type,
                "Disconnect device",
                "Close the current link/device session.",
                ActionCategory::Device,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::LoadDemoProject => Self::new(
                action_type,
                "Load demo project",
                "Write and load the built-in Studio demo project.",
                ActionCategory::Project,
                ActionHistoryPolicy::Never,
            ),
            StudioActionType::RefreshStatus => Self::new(
                action_type,
                "Refresh status",
                "Read lightweight runtime status from the current connection.",
                ActionCategory::Runtime,
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
            StudioActionType::DiscoverDevices,
            StudioActionType::ConnectDevice,
            StudioActionType::DisconnectDevice,
            StudioActionType::LoadDemoProject,
            StudioActionType::RefreshStatus,
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
}
