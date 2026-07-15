//! Observation events emitted by a device session.
//!
//! Snapshot-pull ([`DeviceSession::snapshot`]) stays the primary observation
//! surface; the sink exists for consumers that want push notification of
//! state transitions and the device console feed. In P3 the management event
//! sink (`LinkManagementEventSink`) folds into this vocabulary — `Progress`
//! is already carved out for it.
//!
//! [`DeviceSession::snapshot`]: super::DeviceSession::snapshot

use std::rc::Rc;

use super::device_state::DeviceState;

/// One observable device-session event.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceEvent {
    /// The session transitioned into `state`.
    State { state: DeviceState },
    /// One non-protocol serial line from the device (boot output and runtime
    /// logs). The classifier feed doubles as the console feed.
    LogLine { line: String },
    /// Progress of a long-running device operation (management, P3).
    Progress { label: String, percent: Option<u8> },
}

/// Cloneable in-process event sink (`Rc`-based, deliberately `!Send` — the
/// whole app-side async stack is single-actor).
#[derive(Clone)]
pub struct DeviceEventSink {
    on_event: Rc<dyn Fn(DeviceEvent)>,
}

impl DeviceEventSink {
    pub fn new(on_event: impl Fn(DeviceEvent) + 'static) -> Self {
        Self {
            on_event: Rc::new(on_event),
        }
    }

    /// A sink that discards every event.
    pub fn noop() -> Self {
        Self::new(|_| {})
    }

    pub fn emit(&self, event: DeviceEvent) {
        (self.on_event)(event);
    }
}
