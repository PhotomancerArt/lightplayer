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
    RefreshProviderCatalog,
    StartProvisioning,
    CancelProvisioning,
    RetryProvisioning,
    SelectLinkProvider,
    RequestDeviceAccess,
    DiscoverDevices,
    ConnectDevice,
    ConnectSelectedEndpoint,
    ProbeTarget,
    DisconnectDevice,
    ResetDevice,
    ConfirmFirmwareFlash,
    FlashDeviceFirmware,
    UploadDemoProject,
    LoadDemoProject,
    AcknowledgeProvisioningIssue,
    RefreshStatus,
    ReadProjectState,
    ReadProjectInventory,
    SelectProjectNode,
}

impl From<StudioActionType> for StudioActionTypeName {
    fn from(value: StudioActionType) -> Self {
        match value {
            StudioActionType::RefreshProviderCatalog => Self::RefreshProviderCatalog,
            StudioActionType::StartProvisioning => Self::StartProvisioning,
            StudioActionType::CancelProvisioning => Self::CancelProvisioning,
            StudioActionType::RetryProvisioning => Self::RetryProvisioning,
            StudioActionType::SelectLinkProvider => Self::SelectLinkProvider,
            StudioActionType::RequestDeviceAccess => Self::RequestDeviceAccess,
            StudioActionType::DiscoverDevices => Self::DiscoverDevices,
            StudioActionType::ConnectDevice => Self::ConnectDevice,
            StudioActionType::ConnectSelectedEndpoint => Self::ConnectSelectedEndpoint,
            StudioActionType::ProbeTarget => Self::ProbeTarget,
            StudioActionType::DisconnectDevice => Self::DisconnectDevice,
            StudioActionType::ResetDevice => Self::ResetDevice,
            StudioActionType::ConfirmFirmwareFlash => Self::ConfirmFirmwareFlash,
            StudioActionType::FlashDeviceFirmware => Self::FlashDeviceFirmware,
            StudioActionType::UploadDemoProject => Self::UploadDemoProject,
            StudioActionType::LoadDemoProject => Self::LoadDemoProject,
            StudioActionType::AcknowledgeProvisioningIssue => Self::AcknowledgeProvisioningIssue,
            StudioActionType::RefreshStatus => Self::RefreshStatus,
            StudioActionType::ReadProjectState => Self::ReadProjectState,
            StudioActionType::ReadProjectInventory => Self::ReadProjectInventory,
            StudioActionType::SelectProjectNode => Self::SelectProjectNode,
        }
    }
}
