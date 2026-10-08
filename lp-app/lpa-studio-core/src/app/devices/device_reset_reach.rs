//! How a card's Reset reaches its board, and the words for when it cannot.
//!
//! The device model decides the route (`lpa_devices::Device::resets_by_request`):
//! a USB or serial link pulses its reset lines, a network link — Bluetooth,
//! the LAN, the relay — asks the board to restart itself with the wire's
//! `Reboot`. The offer only needs to know which, because the request is an
//! edit-tier one on the board (`lpa-server`'s access gate) and a pulse on a
//! cable is not.

/// How `reset-board` restarts the board, as far as its offer needs to know.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetReach {
    /// The link's reset lines (USB, a serial bridge), or a runtime's own
    /// restart: physical connection is access, so nothing more is asked.
    Lines,
    /// A restart request over a network link, which the board honours only
    /// at the edit tier. `author`: this link holds it.
    Request { author: bool },
}

/// Why Reset is disabled on a board reached over Bluetooth, Wi‑Fi or the
/// relay whose link does not hold the author (edit) tier: the board would
/// refuse the request.
pub const RESET_NEEDS_AUTHOR: &str =
    "Reset needs an author password when the board isn't plugged in — unlock with one";

/// Why a network link that has not answered yet offers no Reset: there is
/// nobody to ask, and no password has been checked.
pub const RESET_WAITS_FOR_ANSWER: &str = "Reset is ready once the board answers";
