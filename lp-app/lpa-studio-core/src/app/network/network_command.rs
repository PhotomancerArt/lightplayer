//! [`NetworkCommand`]: the network controller's inputs on the actor's
//! queue — the popover asking for a fresh read or a scan, the in-row test
//! dismissed, and every finished conversation.

use lpa_devices::DeviceId;

use super::device_network_ops::{NetworkAnswer, ScanAnswer};

/// One input to the network controller. `Debug` is derived and safe: no
/// variant carries a password (an answer is the board's status or scan,
/// which have none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkCommand {
    /// Read `device`'s status again (the popover opened).
    Refresh { device: DeviceId },
    /// Ask `device` what it hears (the connect page opened, or its
    /// refresh). Nothing is asked of a board that cannot scan.
    Scan { device: DeviceId },
    /// The in-row test of the network just added is done with (its Done
    /// button, or the popover moved on).
    DismissTest { device: DeviceId },
    /// A conversation on `device` ended.
    Answered {
        device: DeviceId,
        kind: NetworkStepKind,
        result: NetworkAnswer,
    },
    /// A scan on `device` ended.
    Scanned {
        device: DeviceId,
        result: ScanAnswer,
    },
    /// A connect over Wi‑Fi someone asked for ended (a card's "Connect over
    /// Wi‑Fi", or the add slot's address): the board at `host` answered, or
    /// why not. The studio controller's, not the settings controller's.
    WifiConnected {
        target: crate::app::devices::WifiConnectTarget,
        host: String,
        result: Result<(), crate::app::devices::WifiConnectFailure>,
    },
    /// A connect through lightplayer.app someone asked for ended (a card's
    /// "Connect through lightplayer.app"): the board answered, or why not.
    /// The studio controller's, like [`Self::WifiConnected`].
    RelayConnected {
        board: lpa_devices::BoardKey,
        result: Result<(), crate::app::devices::RelayConnectFailure>,
    },
}

/// Whether a finished conversation read or wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkStepKind {
    Read,
    Write,
}
