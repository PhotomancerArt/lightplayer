use lpa_link::{LinkEndpointId, LinkProviderId};
use serde::{Deserialize, Serialize};

use crate::{ActionDescriptor, ActionMeta};

/// Payload-free kind used for descriptors, help, and future agent tools.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StudioActionType {
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

impl StudioActionType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::RefreshProviderCatalog,
            Self::StartProvisioning,
            Self::CancelProvisioning,
            Self::RetryProvisioning,
            Self::SelectLinkProvider,
            Self::RequestDeviceAccess,
            Self::DiscoverDevices,
            Self::ConnectDevice,
            Self::ConnectSelectedEndpoint,
            Self::ProbeTarget,
            Self::DisconnectDevice,
            Self::ResetDevice,
            Self::ConfirmFirmwareFlash,
            Self::FlashDeviceFirmware,
            Self::UploadDemoProject,
            Self::LoadDemoProject,
            Self::AcknowledgeProvisioningIssue,
            Self::RefreshStatus,
            Self::ReadProjectState,
            Self::ReadProjectInventory,
            Self::SelectProjectNode,
        ]
    }
}

/// Payload-bearing Studio ux.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioActionKind {
    RefreshProviderCatalog,
    StartProvisioning {
        provider_id: LinkProviderId,
    },
    CancelProvisioning,
    RetryProvisioning,
    SelectLinkProvider {
        provider_id: LinkProviderId,
    },
    RequestDeviceAccess,
    DiscoverDevices,
    ConnectDevice {
        endpoint_id: LinkEndpointId,
    },
    ConnectSelectedEndpoint,
    ProbeTarget {
        endpoint_id: Option<LinkEndpointId>,
    },
    DisconnectDevice,
    ResetDevice,
    ConfirmFirmwareFlash {
        endpoint_id: LinkEndpointId,
        firmware_id: Option<String>,
    },
    FlashDeviceFirmware {
        firmware_id: Option<String>,
    },
    UploadDemoProject,
    LoadDemoProject,
    AcknowledgeProvisioningIssue {
        issue_id: String,
    },
    RefreshStatus,
    ReadProjectState,
    ReadProjectInventory,
    SelectProjectNode {
        node_id: Option<String>,
    },
}

impl StudioActionKind {
    pub fn action_type(&self) -> StudioActionType {
        match self {
            Self::RefreshProviderCatalog => StudioActionType::RefreshProviderCatalog,
            Self::StartProvisioning { .. } => StudioActionType::StartProvisioning,
            Self::CancelProvisioning => StudioActionType::CancelProvisioning,
            Self::RetryProvisioning => StudioActionType::RetryProvisioning,
            Self::SelectLinkProvider { .. } => StudioActionType::SelectLinkProvider,
            Self::RequestDeviceAccess => StudioActionType::RequestDeviceAccess,
            Self::DiscoverDevices => StudioActionType::DiscoverDevices,
            Self::ConnectDevice { .. } => StudioActionType::ConnectDevice,
            Self::ConnectSelectedEndpoint => StudioActionType::ConnectSelectedEndpoint,
            Self::ProbeTarget { .. } => StudioActionType::ProbeTarget,
            Self::DisconnectDevice => StudioActionType::DisconnectDevice,
            Self::ResetDevice => StudioActionType::ResetDevice,
            Self::ConfirmFirmwareFlash { .. } => StudioActionType::ConfirmFirmwareFlash,
            Self::FlashDeviceFirmware { .. } => StudioActionType::FlashDeviceFirmware,
            Self::UploadDemoProject => StudioActionType::UploadDemoProject,
            Self::LoadDemoProject => StudioActionType::LoadDemoProject,
            Self::AcknowledgeProvisioningIssue { .. } => {
                StudioActionType::AcknowledgeProvisioningIssue
            }
            Self::RefreshStatus => StudioActionType::RefreshStatus,
            Self::ReadProjectState => StudioActionType::ReadProjectState,
            Self::ReadProjectInventory => StudioActionType::ReadProjectInventory,
            Self::SelectProjectNode { .. } => StudioActionType::SelectProjectNode,
        }
    }

    pub fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor::for_type(self.action_type())
    }
}

/// One dispatchable Studio ux plus metadata.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct StudioAction {
    pub meta: ActionMeta,
    pub kind: StudioActionKind,
}

impl StudioAction {
    pub fn new(meta: ActionMeta, kind: StudioActionKind) -> Self {
        Self { meta, kind }
    }
}
