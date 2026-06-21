use lpa_link::{LinkEndpointId, LinkProviderId, LinkSessionId};
use serde::{Deserialize, Serialize};

use crate::{
    DeviceIssue, ProgressState, ProjectChoice, ProjectSelectionReason, ProvisioningReason,
    RecoveryReason,
};

/// Product-level journey through provider choice, link, and readiness.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LinkState {
    Empty,
    ChoosingProvider,
    RequestingAccess {
        provider_id: LinkProviderId,
    },
    AccessFailed {
        provider_id: LinkProviderId,
        issue: DeviceIssue,
    },
    Opening {
        endpoint_id: LinkEndpointId,
    },
    OpenFailed {
        endpoint_id: LinkEndpointId,
        issue: DeviceIssue,
    },
    ProbingTarget {
        endpoint_id: LinkEndpointId,
    },
    ProvisioningRequired {
        endpoint_id: LinkEndpointId,
        reason: ProvisioningReason,
    },
    ConfirmingFirmwareFlash {
        endpoint_id: LinkEndpointId,
        firmware_id: Option<String>,
    },
    Flashing {
        endpoint_id: LinkEndpointId,
        progress: Option<ProgressState>,
    },
    OpeningServer {
        endpoint_id: LinkEndpointId,
    },
    ServerReady {
        session_id: LinkSessionId,
    },
    ReadingProjectState {
        session_id: LinkSessionId,
    },
    ProjectSelectionRequired {
        session_id: LinkSessionId,
        reason: ProjectSelectionReason,
        projects: Vec<ProjectChoice>,
    },
    RecoveryRequired {
        session_id: LinkSessionId,
        reason: RecoveryReason,
    },
    DeployingProject {
        project_id: String,
        progress: Option<ProgressState>,
    },
    Ready {
        project_id: String,
    },
    Degraded {
        issue: DeviceIssue,
    },
    Disconnected {
        reason: Option<String>,
    },
}

impl Default for LinkState {
    fn default() -> Self {
        Self::ChoosingProvider
    }
}
