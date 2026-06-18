use serde::{Deserialize, Serialize};

use crate::{ActionId, StudioActionType};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct InFlightAction {
    pub action_id: ActionId,
    pub action_type: StudioActionTypeName,
    pub label: String,
}

impl InFlightAction {
    pub fn new(
        action_id: ActionId,
        action_type: StudioActionType,
        label: impl Into<String>,
    ) -> Self {
        Self {
            action_id,
            action_type: StudioActionTypeName::from(action_type),
            label: label.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioActionTypeName {
    SelectLinkProvider,
    DiscoverDevices,
    ConnectDevice,
    DisconnectDevice,
    LoadDemoProject,
    RefreshStatus,
    ReadProjectInventory,
    SelectProjectNode,
}

impl From<StudioActionType> for StudioActionTypeName {
    fn from(value: StudioActionType) -> Self {
        match value {
            StudioActionType::SelectLinkProvider => Self::SelectLinkProvider,
            StudioActionType::DiscoverDevices => Self::DiscoverDevices,
            StudioActionType::ConnectDevice => Self::ConnectDevice,
            StudioActionType::DisconnectDevice => Self::DisconnectDevice,
            StudioActionType::LoadDemoProject => Self::LoadDemoProject,
            StudioActionType::RefreshStatus => Self::RefreshStatus,
            StudioActionType::ReadProjectInventory => Self::ReadProjectInventory,
            StudioActionType::SelectProjectNode => Self::SelectProjectNode,
        }
    }
}
