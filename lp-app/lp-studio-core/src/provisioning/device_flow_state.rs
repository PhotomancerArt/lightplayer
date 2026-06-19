use lpa_link::{LinkEndpointId, LinkProviderId, LinkSessionId};
use serde::{Deserialize, Serialize};

use crate::{
    DeviceIssue, ProgressState, ProjectChoice, ProjectSelectionReason, ProvisioningReason,
    RecoveryReason,
};

/// Product-level journey through provider choice, provisioning, and readiness.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum DeviceFlowState {
    Empty,
    ChoosingProvider,
    ProviderSelected {
        provider_id: LinkProviderId,
    },
    RequestingAccess {
        provider_id: LinkProviderId,
    },
    AccessFailed {
        provider_id: LinkProviderId,
        issue: DeviceIssue,
    },
    EndpointGranted {
        provider_id: LinkProviderId,
        endpoint_id: LinkEndpointId,
    },
    OpeningLink {
        endpoint_id: LinkEndpointId,
    },
    LinkFailed {
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
    FlashConfirm {
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

impl Default for DeviceFlowState {
    fn default() -> Self {
        Self::ChoosingProvider
    }
}
