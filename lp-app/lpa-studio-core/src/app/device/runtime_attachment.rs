//! What runtime the studio is currently attached to.
//!
//! One enum answers "what are we talking to?" (D22: the sim is not a
//! device):
//!
//! - [`RuntimeAttachment::Sim`] — the browser-worker simulator. Connect +
//!   worker io, no boot, no readiness states, no management plane.
//! - [`RuntimeAttachment::Device`] — real hardware, owned end to end by an
//!   `lpa_link` [`DeviceSession`] (state machine, hello-first readiness,
//!   management, reconnect-that-rebuilds).
//!
//! `LinkController` is bypassed on every path that lands here (P6 deletes
//! it).

use std::rc::Rc;

use lpa_link::{DeviceSession, DeviceState, LinkConnection, LinkConnector, LinkSession};

use crate::UiError;

/// The studio's current runtime attachment.
pub enum RuntimeAttachment {
    /// Nothing attached.
    None,
    /// The browser-worker simulator (BrowserWorker): a live provider
    /// session whose server io is the worker post-message channel.
    Sim(SimAttachment),
    /// Attached hardware, driven through its [`DeviceSession`].
    Device(DeviceHandle),
}

impl RuntimeAttachment {
    pub fn is_device(&self) -> bool {
        matches!(self, Self::Device(_))
    }

    pub fn is_sim(&self) -> bool {
        matches!(self, Self::Sim(_))
    }
}

/// The simulator attachment: connector + session + connection handoff.
/// No states — boot-ready IS the session (D22).
pub struct SimAttachment {
    pub connector: Rc<LinkConnector>,
    pub session: LinkSession,
    pub connection: LinkConnection,
}

/// A hardware attachment. In product code this is always a live
/// [`DeviceSession`]; tests that only exercise view/derivation logic can
/// stub the session with a fixed [`DeviceState`] instead of scripting a
/// whole fake device.
pub enum DeviceHandle {
    Session(DeviceSession),
    #[cfg(test)]
    Stub(StubDevice),
}

/// Test-only hardware stand-in: a fixed device state, no wire.
#[cfg(test)]
pub struct StubDevice {
    pub state: DeviceState,
}

impl DeviceHandle {
    /// The session's current device state.
    pub fn state(&self) -> DeviceState {
        match self {
            Self::Session(session) => session.state(),
            #[cfg(test)]
            Self::Stub(stub) => stub.state.clone(),
        }
    }

    /// The live session, when this handle holds one.
    pub fn session(&self) -> Option<&DeviceSession> {
        match self {
            Self::Session(session) => Some(session),
            #[cfg(test)]
            Self::Stub(_) => None,
        }
    }

    /// Close the underlying session (attachment teardown).
    pub async fn close(self) -> Result<(), UiError> {
        match self {
            Self::Session(session) => session
                .close()
                .await
                .map_err(|error| UiError::Link(error.to_string())),
            #[cfg(test)]
            Self::Stub(_) => Ok(()),
        }
    }
}

/// A `Ready { hello }` device state for stubbed hardware in tests.
#[cfg(test)]
pub fn ready_state_for_test() -> DeviceState {
    DeviceState::Ready {
        hello: lpc_wire::ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            fw: lpc_wire::FwProvenance {
                package: "fw-test".to_string(),
                commit: "0000000".to_string(),
                dirty: false,
                profile: "test".to_string(),
            },
            device_uid: None,
        },
    }
}
