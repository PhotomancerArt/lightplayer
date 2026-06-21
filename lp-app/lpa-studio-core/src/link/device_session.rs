use crate::{DeviceCapability, DeviceId};
use lpa_link::LinkProviderKind;
use lpa_link::provider::endpoint::LinkEndpointId;
use lpa_link::provider::session::LinkSessionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceSession {
    pub device_id: DeviceId,
    pub provider_id: LinkProviderKind,
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub capabilities: Vec<DeviceCapability>,
}
