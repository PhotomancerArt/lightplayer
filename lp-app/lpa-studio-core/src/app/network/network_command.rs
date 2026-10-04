//! [`NetworkCommand`]: the network controller's inputs on the actor's
//! queue — the popover asking for a fresh read, and every finished
//! conversation.

use lpa_devices::DeviceId;

use super::device_network_ops::NetworkAnswer;

/// One input to the network controller. `Debug` is derived and safe: no
/// variant carries a password (an answer is the board's status, which has
/// none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkCommand {
    /// Read `device`'s status again (the popover opened).
    Refresh { device: DeviceId },
    /// A conversation on `device` ended.
    Answered {
        device: DeviceId,
        kind: NetworkStepKind,
        result: NetworkAnswer,
    },
}

/// Whether a finished conversation read or wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkStepKind {
    Read,
    Write,
}
