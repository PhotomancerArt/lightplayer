//! A serial board that did not become ready, as a typed error: what the
//! session settled on is kept, so a caller can act on WHY (a board on
//! another wire, say) without reading the error's words.

use lpa_link::{DeviceState, IncompatibleReason};

/// [`crate::client::cli_connect::cli_connect`]'s error for a device whose
/// session did not reach `Ready`. Its words are the session's own.
#[derive(Clone, Debug)]
pub struct DeviceNotReady {
    pub state: DeviceState,
    message: String,
}

impl DeviceNotReady {
    pub fn new(state: DeviceState) -> Self {
        let message = state
            .unavailable_message()
            .unwrap_or_else(|| format!("{state:?}"));
        Self { state, message }
    }

    /// The wire the board's hello named, when it said hello on another
    /// wire than this build's.
    pub fn other_wire(&self) -> Option<u32> {
        match &self.state {
            DeviceState::Incompatible {
                reason: IncompatibleReason::ProtoMismatch { hello },
            } => Some(hello.proto),
            DeviceState::Incompatible {
                reason: IncompatibleReason::HelloOnOtherWire { proto },
            } => Some(*proto),
            _ => None,
        }
    }
}

impl std::fmt::Display for DeviceNotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "device did not become ready: {}", self.message)
    }
}

impl std::error::Error for DeviceNotReady {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hello_on_another_wire_names_that_wire() {
        let error = DeviceNotReady::new(DeviceState::Incompatible {
            reason: IncompatibleReason::HelloOnOtherWire { proto: 31 },
        });
        assert_eq!(error.other_wire(), Some(31));
        assert!(
            error
                .to_string()
                .starts_with("device did not become ready: ")
        );
    }

    #[test]
    fn a_board_that_never_said_hello_names_no_wire() {
        let error = DeviceNotReady::new(DeviceState::Incompatible {
            reason: IncompatibleReason::NoHello,
        });
        assert_eq!(error.other_wire(), None);
    }
}
