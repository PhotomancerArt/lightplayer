//! Pull-model snapshot of a device session.

use crate::{LinkEndpointStatus, LinkSession, LinkSessionStatus};

use super::device_state::DeviceState;

/// Point-in-time view of a [`DeviceSession`]: the state machine position,
/// the underlying link session record (whose `status` carries the
/// `LinkSessionStatus::Error` vocabulary for failed sessions), the derived
/// endpoint status, and a bounded tail of recent non-protocol serial lines
/// for context.
///
/// [`DeviceSession`]: super::DeviceSession
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceSnapshot {
    pub state: DeviceState,
    pub session: LinkSession,
    /// The `LinkEndpointStatus` vocabulary as observed through this session
    /// (endpoint records themselves are immutable provider catalog data, so
    /// the live status is derived here): `Error` whenever the session
    /// failed, `Connected` when ready, `Available` after a clean close.
    pub endpoint_status: LinkEndpointStatus,
    pub recent_lines: Vec<String>,
}

impl DeviceSnapshot {
    pub(super) fn derive_endpoint_status(
        state: &DeviceState,
        session: &LinkSession,
    ) -> LinkEndpointStatus {
        if let LinkSessionStatus::Error { message } = &session.status {
            return LinkEndpointStatus::Error {
                message: message.clone(),
            };
        }
        match state {
            DeviceState::Ready { .. } => LinkEndpointStatus::Connected,
            DeviceState::Gone => LinkEndpointStatus::Available,
            _ => LinkEndpointStatus::InUse,
        }
    }
}
