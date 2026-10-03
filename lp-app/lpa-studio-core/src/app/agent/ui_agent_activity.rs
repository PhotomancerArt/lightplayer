//! [`UiAgentActivity`] and [`UiAgentPlace`]: what the page draws of the app
//! agent's activity — the controls lit for a moment, the reveal the user's
//! Show asked for, and each chat row's "where".
//!
//! Keyed by offer path, the one id the web and the agent already share: a
//! control that renders an offer knows its path, so it can tell whether it
//! is lit without any other bookkeeping.

use crate::OfferPath;
use crate::app::agent::agent_activity::AgentActivityKind;

/// The app agent's activity as the page draws it, carried on
/// [`crate::UiAppAgentView::activity`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiAgentActivity {
    /// The controls lit right now, oldest first. A control matches by its
    /// offer path (a node card by its node's prefix).
    pub lit: Vec<UiAgentLit>,
    /// The last reveal the user's Show asked for; a new `generation` is a
    /// new request.
    pub reveal: Option<UiAgentReveal>,
}

impl UiAgentActivity {
    /// The newest light on `path`, if it is lit.
    pub fn lit_at(&self, path: &OfferPath) -> Option<&UiAgentLit> {
        self.lit.iter().rev().find(|lit| &lit.path == path)
    }
}

/// One lit control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentLit {
    /// The control's offer path (or the node card's prefix).
    pub path: OfferPath,
    /// The entry's seq: a re-light of the same control is a new seq, which
    /// is how the page knows to start its light over.
    pub seq: u64,
    pub kind: AgentActivityKind,
}

/// The user pressed Show: bring the control at `path` into view. The page
/// scrolls; it never moves keyboard focus.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentReveal {
    pub path: OfferPath,
    /// Changes on every Show (it is the re-light's seq).
    pub generation: u64,
}

/// Where a chat row's press or edit happened: "pressed **Save** in the
/// project header", and the Show offer that brings it into view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentPlace {
    /// The control's name (`Save`; a node's name for an edit).
    pub label: String,
    /// Where it is, as a phrase after the label (`in the project header`,
    /// `on the fixture card`).
    pub place: String,
    /// The Show offer's path (`show/project/save`), while the control is
    /// there to show; `None` once it is gone (Save after saving, a removed
    /// node).
    pub show: Option<OfferPath>,
}
