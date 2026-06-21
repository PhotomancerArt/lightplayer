use serde::{Deserialize, Serialize};

use crate::{
    ActionDescriptor, ActionMeta, LinkActionRequest, ProjectActionRequest, ServerActionRequest,
};

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

impl From<StudioActionType> for ActionDescriptor {
    fn from(action_type: StudioActionType) -> Self {
        Self::for_type(action_type)
    }
}

/// Payload-bearing Studio action.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioActionKind {
    Link(LinkActionRequest),
    Server(ServerActionRequest),
    Project(ProjectActionRequest),
}

impl StudioActionKind {
    pub fn action_type(&self) -> StudioActionType {
        match self {
            Self::Link(request) => request.action_type(),
            Self::Server(request) => request.action_type(),
            Self::Project(request) => request.action_type(),
        }
    }

    pub fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor::for_type(self.action_type())
    }
}

impl From<LinkActionRequest> for StudioActionKind {
    fn from(request: LinkActionRequest) -> Self {
        Self::Link(request)
    }
}

impl From<ServerActionRequest> for StudioActionKind {
    fn from(request: ServerActionRequest) -> Self {
        Self::Server(request)
    }
}

impl From<ProjectActionRequest> for StudioActionKind {
    fn from(request: ProjectActionRequest) -> Self {
        Self::Project(request)
    }
}

/// One dispatchable Studio action plus metadata.
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
