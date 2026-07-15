//! Pull-model snapshot of a device session.

use crate::LinkSession;

use super::device_state::DeviceState;

/// Point-in-time view of a [`DeviceSession`]: the state machine position,
/// the underlying link session record (whose `status` carries the
/// `LinkSessionStatus::Error` vocabulary for failed sessions), and a bounded
/// tail of recent non-protocol serial lines for context.
///
/// [`DeviceSession`]: super::DeviceSession
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceSnapshot {
    pub state: DeviceState,
    pub session: LinkSession,
    pub recent_lines: Vec<String>,
}
