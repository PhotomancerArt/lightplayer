use lpa_link::{LinkEndpointId, LinkProviderId};
use serde::{Deserialize, Serialize};

use crate::{ActionDescriptor, ActionMeta};

/// Payload-free kind used for descriptors, help, and future agent tools.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StudioActionType {
    SelectLinkProvider,
    DiscoverDevices,
    ConnectDevice,
    DisconnectDevice,
    LoadDemoProject,
    RefreshStatus,
    ReadProjectInventory,
    SelectProjectNode,
}

impl StudioActionType {
    pub fn all() -> Vec<Self> {
        vec![
            Self::SelectLinkProvider,
            Self::DiscoverDevices,
            Self::ConnectDevice,
            Self::DisconnectDevice,
            Self::LoadDemoProject,
            Self::RefreshStatus,
            Self::ReadProjectInventory,
            Self::SelectProjectNode,
        ]
    }
}

/// Payload-bearing Studio action.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioActionKind {
    SelectLinkProvider { provider_id: LinkProviderId },
    DiscoverDevices,
    ConnectDevice { endpoint_id: LinkEndpointId },
    DisconnectDevice,
    LoadDemoProject,
    RefreshStatus,
    ReadProjectInventory,
    SelectProjectNode { node_id: Option<String> },
}

impl StudioActionKind {
    pub fn action_type(&self) -> StudioActionType {
        match self {
            Self::SelectLinkProvider { .. } => StudioActionType::SelectLinkProvider,
            Self::DiscoverDevices => StudioActionType::DiscoverDevices,
            Self::ConnectDevice { .. } => StudioActionType::ConnectDevice,
            Self::DisconnectDevice => StudioActionType::DisconnectDevice,
            Self::LoadDemoProject => StudioActionType::LoadDemoProject,
            Self::RefreshStatus => StudioActionType::RefreshStatus,
            Self::ReadProjectInventory => StudioActionType::ReadProjectInventory,
            Self::SelectProjectNode { .. } => StudioActionType::SelectProjectNode,
        }
    }

    pub fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor::for_type(self.action_type())
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
