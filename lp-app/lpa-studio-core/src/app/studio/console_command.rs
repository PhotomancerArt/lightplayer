//! User commands that mutate console state (filter and ring).

use crate::{UiLogLevel, UiLogOrigin};

/// A console mutation requested by the UI (the P2 toolbar).
///
/// Routed as [`StudioCommand::Console`](super::StudioCommand) and applied by
/// the actor synchronously — no async work, no action metadata — and, unlike
/// `RefreshTick`, never coalesced away: each command is a distinct user
/// gesture whose order matters (e.g. `Clear` between two level changes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsoleCommand {
    /// Hide entries below this severity.
    SetMinLevel(UiLogLevel),
    /// Show or hide entries from one origin.
    SetOriginEnabled(UiLogOrigin, bool),
    /// Empty the log ring.
    Clear,
    /// Ask the connected server/device to change its runtime log level.
    ///
    /// Unlike the other console commands this is not a synchronous local
    /// mutation: the actor converts it at intake into a
    /// [`DeviceOp::SetLogLevel`](crate::DeviceOp) action (see
    /// `CommandPlan::from_batch`), so it runs on the normal async action
    /// path with its error handling. The web toolbar still sends it as a
    /// console command so components stay op-agnostic.
    SetDeviceLogLevel(UiLogLevel),
}
