//! [`UiDetailPanel`]: today's card surfaces that are more than facts and
//! verbs, named, each carrying core's data for it — never an action. The
//! web draws each one inside the details that list it.

use lpa_devices::evidence::TerminalLine;

use crate::OfferPath;

/// One panel inside a bar's (or the status corner's) details.
#[derive(Clone, Debug, PartialEq)]
pub enum UiDetailPanel {
    /// The board's terminal: what it said, what the wire carried and what
    /// Studio did to it, oldest first, and how many lines fell off the
    /// front.
    Terminal {
        lines: Vec<TerminalLine>,
        dropped: u32,
    },
}

impl UiDetailPanel {
    /// The offers this panel's controls press.
    pub fn offer_paths(&self) -> Vec<&OfferPath> {
        match self {
            UiDetailPanel::Terminal { .. } => Vec::new(),
        }
    }
}
