use lpa_link::{LinkEndpointId, LinkProviderId};
use serde::{Deserialize, Serialize};

use crate::RecoveryAction;

/// Severity for a link issue.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum DeviceIssueSeverity {
    Info,
    Warning,
    Error,
}

/// Machine-readable link failure category.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum DeviceIssueKind {
    RuntimeUnsupported,
    ProviderUnavailable,
    PermissionCanceled,
    PermissionDenied,
    NoEndpoint,
    EndpointOpenFailed,
    UnknownTarget,
    UnsupportedTarget,
    ServerTimeout,
    IncompatibleFirmware,
    FirmwareArtifactMissing,
    ProjectDeployFailed,
    ProjectLoadFailed,
    FlashFailed,
    ConnectionLost,
    ActionFailed,
}

/// User- and agent-readable issue raised during link.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceIssue {
    pub id: String,
    pub kind: DeviceIssueKind,
    pub severity: DeviceIssueSeverity,
    pub message: String,
    pub provider_id: Option<LinkProviderId>,
    pub endpoint_id: Option<LinkEndpointId>,
    pub recovery_actions: Vec<RecoveryAction>,
}

impl DeviceIssue {
    pub fn new(
        id: impl Into<String>,
        kind: DeviceIssueKind,
        severity: DeviceIssueSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            severity,
            message: message.into(),
            provider_id: None,
            endpoint_id: None,
            recovery_actions: Vec::new(),
        }
    }

    pub fn error(id: impl Into<String>, kind: DeviceIssueKind, message: impl Into<String>) -> Self {
        Self::new(id, kind, DeviceIssueSeverity::Error, message)
    }

    pub fn with_provider(mut self, provider_id: impl Into<LinkProviderId>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    pub fn with_endpoint(mut self, endpoint_id: impl Into<LinkEndpointId>) -> Self {
        self.endpoint_id = Some(endpoint_id.into());
        self
    }

    pub fn with_recovery_actions(mut self, recovery_actions: Vec<RecoveryAction>) -> Self {
        self.recovery_actions = recovery_actions;
        self
    }
}
