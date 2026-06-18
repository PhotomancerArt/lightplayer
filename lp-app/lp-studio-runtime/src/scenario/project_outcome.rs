use lp_studio_core::DeviceIssue;
use lpc_wire::{WireProjectHandle, WireProjectInventoryReadResponse};
use serde::{Deserialize, Serialize};

/// Scripted result of deploying and loading the Studio demo project.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum ProjectOutcome {
    Succeeds {
        handle: WireProjectHandle,
        inventory: WireProjectInventoryReadResponse,
    },
    DeployFails {
        issue: DeviceIssue,
    },
    LoadFails {
        issue: DeviceIssue,
    },
}

impl ProjectOutcome {
    pub fn succeeds() -> Self {
        Self::Succeeds {
            handle: WireProjectHandle::new(1),
            inventory: WireProjectInventoryReadResponse::default(),
        }
    }

    pub fn deploy_fails(issue: DeviceIssue) -> Self {
        Self::DeployFails { issue }
    }

    pub fn load_fails(issue: DeviceIssue) -> Self {
        Self::LoadFails { issue }
    }

    pub fn handle(&self) -> WireProjectHandle {
        match self {
            Self::Succeeds { handle, .. } => *handle,
            Self::DeployFails { .. } | Self::LoadFails { .. } => WireProjectHandle::new(1),
        }
    }

    pub fn inventory(&self) -> WireProjectInventoryReadResponse {
        match self {
            Self::Succeeds { inventory, .. } => inventory.clone(),
            Self::DeployFails { .. } | Self::LoadFails { .. } => {
                WireProjectInventoryReadResponse::default()
            }
        }
    }
}
