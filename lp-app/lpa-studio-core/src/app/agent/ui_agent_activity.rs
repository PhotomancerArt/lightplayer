//! [`UiAgentActivity`] and [`UiAgentPlace`]: what the page draws of the app
//! agent's activity — the controls lit for a moment, the reveal the user's
//! Show asked for, and each chat row's "where".
//!
//! Keyed by offer path, the one id the web and the agent already share: a
//! control that renders an offer knows its path, so it can tell whether it
//! is lit without any other bookkeeping. An edit also names the slots it
//! wrote, so a knob or a settings row knows by its own slot address.

use crate::app::agent::agent_activity::AgentActivityKind;
use crate::{OfferPath, ProjectSlotAddress};

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

    /// The newest edit light that changed the slot at `address`: one that
    /// wrote exactly there, or — when `or_below` — anywhere under it. A
    /// control that edits its slot as a whole (a knob over a value with
    /// parts, a row that is not expanded) asks `or_below`; a row whose
    /// children are drawn as their own rows asks only for its own slot, so
    /// a deep edit lights the row that changed, not every row above it.
    pub fn slot_lit(&self, address: &ProjectSlotAddress, or_below: bool) -> Option<&UiAgentLit> {
        self.lit.iter().rev().find(|lit| {
            lit.slots
                .iter()
                .any(|slot| slot == address || (or_below && slot.is_strictly_under(address)))
        })
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
    /// For an edit, the slots it wrote (see
    /// [`crate::AgentActivityEntry::slots`]); empty otherwise. Empty on an
    /// edit too when nothing it changed is a slot (a created node, a file),
    /// and then only the card says so.
    pub slots: Vec<ProjectSlotAddress>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_lit_by_an_edit_that_wrote_it() {
        let activity = edited(&["render_size.width"]);
        let width = slot("render_size.width");
        assert_eq!(activity.slot_lit(&width, false).map(|lit| lit.seq), Some(1));
        assert!(
            activity
                .slot_lit(&slot("render_size.height"), true)
                .is_none()
        );
    }

    #[test]
    fn a_slot_whose_parts_changed_is_lit_only_when_it_asks_or_below() {
        let activity = edited(&["render_size.width"]);
        let render_size = slot("render_size");
        assert!(
            activity.slot_lit(&render_size, false).is_none(),
            "a row with its parts drawn as rows stays dark"
        );
        assert!(
            activity.slot_lit(&render_size, true).is_some(),
            "a control that edits the whole value lights"
        );
        assert!(
            activity
                .slot_lit(&slot("render_size.width.x"), true)
                .is_none(),
            "an edit above a slot does not light it"
        );
    }

    #[test]
    fn a_press_lights_no_slot() {
        let activity = UiAgentActivity {
            lit: vec![UiAgentLit {
                path: OfferPath::project().child("save"),
                seq: 1,
                kind: AgentActivityKind::Pressed,
                slots: Vec::new(),
            }],
            reveal: None,
        };
        assert!(activity.slot_lit(&slot("render_size"), true).is_none());
    }

    fn edited(paths: &[&str]) -> UiAgentActivity {
        let node = crate::ProjectNodeAddress::parse("/demo.module/fixture.fixture").unwrap();
        UiAgentActivity {
            lit: vec![UiAgentLit {
                path: OfferPath::project_node(&node),
                seq: 1,
                kind: AgentActivityKind::Edited,
                slots: paths.iter().map(|path| slot(path)).collect(),
            }],
            reveal: None,
        }
    }

    fn slot(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            crate::ProjectNodeAddress::parse("/demo.module/fixture.fixture").unwrap(),
            crate::ProjectSlotRoot::Def,
            lpc_model::SlotPath::parse(path).unwrap(),
        )
    }
}
