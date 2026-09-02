//! Project-level fault status: "is any node in this project in
//! [`lpc_wire::NodeRuntimeStatus::Fault`], and since when".
//!
//! The verdict is deliberately PROJECT-level rather than per-output or
//! per-dependency-path (D1): a fault anywhere means every output of the
//! project paints the fault pattern, so an operator never has to know which
//! strand hangs off which broken node to read "this show is not running".

use alloc::string::String;
use alloc::vec::Vec;

/// Every node currently in `Fault`, and when the project first was.
///
/// Derived at the end of each tick from the tree's entry statuses, and read
/// by outputs on the NEXT tick — one frame of lag, irrelevant under the
/// 1 s persistence and much cheaper than ordering the derivation inside the
/// demand walk.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectFault {
    /// Frame time (seconds, the same clock as
    /// [`crate::node::TickContext::time_seconds`]) at which the project
    /// first had a node in `Fault`, CONTINUOUSLY until now. It survives the
    /// set changing membership; only an empty set resets it.
    pub since_seconds: f32,
    /// Every node currently in `Fault`: `(tree path, message)`, in tree
    /// order so the value is steady frame over frame for status diffing.
    pub nodes: Vec<(String, String)>,
}

/// What an output does while its project is faulted.
///
/// Engine state, not project data, and not persisted (D2): the default is
/// the honest one, and the knob exists so an installation that would rather
/// go dark than show a diagnostic can say so.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FaultPresentation {
    /// Paint the red breathe over the composed buffer. A fault is never
    /// black.
    #[default]
    Pattern,
    /// Paint nothing — whatever the graph produced (usually black) goes to
    /// the wire, which is the behaviour before this policy existed.
    Black,
}
