use crate::{DeviceCapability, DeviceId};
use lpa_link::LinkConnectionKind;
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::link_session::LinkSessionId;
use serde::{Deserialize, Serialize};

/// Coarse health for a connected device/server session.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum DeviceHealthState {
    Connecting,
    Connected,
    Degraded,
    Disconnected,
}

/// Product-level summary of the currently connected device.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ConnectedDeviceState {
    pub device_id: DeviceId,
    pub provider_id: LinkProviderId,
    pub endpoint_id: LinkEndpointId,
    pub session_id: LinkSessionId,
    pub connection_kind: LinkConnectionKind,
    pub capabilities: Vec<DeviceCapability>,
    pub health: DeviceHealthState,
}

impl ConnectedDeviceState {
    pub fn connected(
        device_id: DeviceId,
        provider_id: LinkProviderId,
        endpoint_id: LinkEndpointId,
        session_id: LinkSessionId,
        connection_kind: LinkConnectionKind,
        capabilities: Vec<DeviceCapability>,
    ) -> Self {
        Self {
            device_id,
            provider_id,
            endpoint_id,
            session_id,
            connection_kind,
            capabilities,
            health: DeviceHealthState::Connected,
        }
    }
}
