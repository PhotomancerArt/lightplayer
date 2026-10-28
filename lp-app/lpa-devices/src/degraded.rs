//! Why a running board is not running well, said once ([`Degraded::line`])
//! and kept apart by what it is about ([`DegradedCause`]), so a card can
//! word it: a node of the project it runs faulted, a project its last start
//! never finished loading (which may not be the one it runs now), or its
//! crash-recovery state.

use serde::{Deserialize, Serialize};

/// The ONE line a card says about a degraded board, and what it is about.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Degraded {
    /// The line, deterministic frame over frame: "Degraded: node /x
    /// faulted — <why>", the board's own load notice, or the recovery
    /// state.
    pub line: String,
    /// What the line is about.
    pub cause: DegradedCause,
}

/// What a [`Degraded`] line is about.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DegradedCause {
    /// A node of the project the board runs faulted: its tree path and the
    /// board's message (the first, when several did).
    NodeFault { node: String, message: String },
    /// A project load the board's last start never finished, in the
    /// board's own words, which name that project ("Zook dome didn't fit in
    /// memory at startup, so no project is running"). The board keeps it
    /// for the whole boot, so a project loaded since may be running now.
    LoadNotice,
    /// The board's crash-recovery state: watched or disabled paths, safe
    /// mode.
    Recovery,
}

impl Degraded {
    /// A recovery line (a fixture's, a story's).
    pub fn recovery(line: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            cause: DegradedCause::Recovery,
        }
    }

    /// A node of the running project faulted.
    pub fn node_fault(node: impl Into<String>, message: impl Into<String>) -> Self {
        let node = node.into();
        let message = message.into();
        Self {
            line: format!("Degraded: node {node} faulted — {message}"),
            cause: DegradedCause::NodeFault { node, message },
        }
    }

    /// The board's load notice, as it said it.
    pub fn load_notice(notice: impl Into<String>) -> Self {
        Self {
            line: notice.into(),
            cause: DegradedCause::LoadNotice,
        }
    }
}
