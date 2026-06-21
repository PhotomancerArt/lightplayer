use crate::{DeviceCapability, DeviceId};
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::link_session::LinkSessionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceSession {
    pub device_id: DeviceId,
    pub provider_id: LinkProviderId,
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub capabilities: Vec<DeviceCapability>,
}
