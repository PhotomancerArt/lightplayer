use lpa_link::{LinkEndpointId, LinkProviderId, LinkSessionId};
use serde::{Deserialize, Serialize};

use crate::{DeviceCapability, DeviceId};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceSession {
    pub device_id: DeviceId,
    pub provider_id: LinkProviderId,
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub capabilities: Vec<DeviceCapability>,
}
