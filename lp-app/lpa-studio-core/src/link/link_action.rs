use lpa_link::{LinkEndpointId, LinkProviderId};
use serde::{Deserialize, Serialize};

use crate::StudioActionType;

/// User or agent intent owned by the Studio link manager.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkActionRequest {
    RefreshProviderCatalog,
    StartProvisioning {
        provider_id: LinkProviderId,
    },
    CancelProvisioning,
    RetryProvisioning,
    SelectProvider {
        provider_id: LinkProviderId,
    },
    RequestDeviceAccess,
    DiscoverDevices,
    ConnectEndpoint {
        endpoint_id: LinkEndpointId,
    },
    ConnectSelectedEndpoint,
    ProbeTarget {
        endpoint_id: Option<LinkEndpointId>,
    },
    Disconnect,
    Reset,
    ConfirmFirmwareFlash {
        endpoint_id: LinkEndpointId,
        firmware_id: Option<String>,
    },
    FlashFirmware {
        firmware_id: Option<String>,
    },
    AcknowledgeIssue {
        issue_id: String,
    },
}

impl LinkActionRequest {
    pub fn action_type(&self) -> StudioActionType {
        match self {
            Self::RefreshProviderCatalog => StudioActionType::RefreshProviderCatalog,
            Self::StartProvisioning { .. } => StudioActionType::StartProvisioning,
            Self::CancelProvisioning => StudioActionType::CancelProvisioning,
            Self::RetryProvisioning => StudioActionType::RetryProvisioning,
            Self::SelectProvider { .. } => StudioActionType::SelectLinkProvider,
            Self::RequestDeviceAccess => StudioActionType::RequestDeviceAccess,
            Self::DiscoverDevices => StudioActionType::DiscoverDevices,
            Self::ConnectEndpoint { .. } => StudioActionType::ConnectDevice,
            Self::ConnectSelectedEndpoint => StudioActionType::ConnectSelectedEndpoint,
            Self::ProbeTarget { .. } => StudioActionType::ProbeTarget,
            Self::Disconnect => StudioActionType::DisconnectDevice,
            Self::Reset => StudioActionType::ResetDevice,
            Self::ConfirmFirmwareFlash { .. } => StudioActionType::ConfirmFirmwareFlash,
            Self::FlashFirmware { .. } => StudioActionType::FlashDeviceFirmware,
            Self::AcknowledgeIssue { .. } => StudioActionType::AcknowledgeProvisioningIssue,
        }
    }
}
