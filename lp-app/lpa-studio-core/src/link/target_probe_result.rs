use lpa_link::LinkEndpointId;
use serde::{Deserialize, Serialize};

use crate::{DeviceCapability, DeviceIssue, ProvisioningReason};

/// Coarse classification for a target reached through a provider endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum TargetKind {
    LightPlayerServer,
    Bootloader,
    BlankDevice,
    UnsupportedDevice,
    Unknown,
}

/// Result of probing an endpoint before deciding whether link is needed.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TargetProbeResult {
    pub endpoint_id: LinkEndpointId,
    pub kind: TargetKind,
    pub server_version: Option<String>,
    pub capabilities: Vec<DeviceCapability>,
    pub provisioning_reason: Option<ProvisioningReason>,
    pub issue: Option<DeviceIssue>,
}

impl TargetProbeResult {
    pub fn server(endpoint_id: impl Into<LinkEndpointId>, server_version: Option<String>) -> Self {
        Self {
            endpoint_id: endpoint_id.into(),
            kind: TargetKind::LightPlayerServer,
            server_version,
            capabilities: Vec::new(),
            provisioning_reason: None,
            issue: None,
        }
    }
}
