//! [`UiOfferFocus`]: where the user is, said in offer paths — what the ⌘K
//! palette ranks first and what the app agent's readout lists in full.
//!
//! Core works it out from place (`UiPlace`, which the web reports) and the
//! node it already knows is focused, and hangs it on the view's
//! [`crate::UiOfferTree`]. It is a reading of where the user is, never a
//! way to move them: nothing in core navigates.

use crate::OfferPath;

/// Where the user is, as offer prefixes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiOfferFocus {
    /// The focused node's prefix (`project/demo.module/orbit.shader`), when
    /// a project editor shows a focused node.
    pub node: Option<OfferPath>,
    /// The current page's areas: `project` in the project editor,
    /// `devices` on the gallery pages — and `project` there too while no
    /// project is open, where Home's own `project/new` and `project/open`
    /// are the only project verbs. Empty on pages that offer nothing (the
    /// docs, the boards catalog).
    pub areas: Vec<OfferPath>,
}

impl UiOfferFocus {
    /// Focus on nothing in particular: every offer is
    /// [`OfferNearness::Elsewhere`].
    pub fn none() -> Self {
        Self::default()
    }

    /// How near `path` (an offer's path) is to this focus.
    pub fn nearness(&self, path: &OfferPath) -> OfferNearness {
        if let Some(node) = &self.node
            && path.starts_with(node)
        {
            // A verb the node groups under a namespace of its own (a
            // fixture's `patch/assign`) is still its own; only a verb past
            // another node segment is a child's.
            return if path.is_own_verb_of(node) {
                OfferNearness::Own
            } else {
                OfferNearness::Under
            };
        }
        match self.areas.iter().any(|area| path.starts_with(area)) {
            true => OfferNearness::Area,
            false => OfferNearness::Elsewhere,
        }
    }
}

/// How near one offer is to the user's focus, nearest first (the derived
/// order is the ranking).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OfferNearness {
    /// A verb of the focused node itself, grouped ones included
    /// (`project/<fixture>/patch/reverse`).
    Own,
    /// A verb of a node inside the focused one.
    Under,
    /// Elsewhere on the current page's area.
    Area,
    /// Anywhere else.
    Elsewhere,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectNodeAddress;

    #[test]
    fn the_focused_nodes_verbs_come_before_its_childrens_before_the_page_before_the_rest() {
        let module = ProjectNodeAddress::parse("/demo.module").unwrap();
        let shader = ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap();
        let focus = UiOfferFocus {
            node: Some(OfferPath::project_node(&module)),
            areas: vec![OfferPath::project()],
        };

        let at = |path: &str| focus.nearness(&OfferPath::parse(path).unwrap());
        assert_eq!(at("project/demo.module/revert"), OfferNearness::Own);
        assert_eq!(
            at("project/demo.module/patch/reverse"),
            OfferNearness::Own,
            "a verb the node groups is still its own"
        );
        assert_eq!(
            at(&OfferPath::project_node(&shader).child("remove").to_string()),
            OfferNearness::Under
        );
        assert_eq!(at("project/save"), OfferNearness::Area);
        assert_eq!(at("devices/connect-usb"), OfferNearness::Elsewhere);
        assert_eq!(
            UiOfferFocus::none().nearness(&OfferPath::parse("project/save").unwrap()),
            OfferNearness::Elsewhere
        );
    }

    #[test]
    fn a_page_with_two_areas_counts_both_as_the_page() {
        let focus = UiOfferFocus {
            node: None,
            areas: vec![OfferPath::devices(), OfferPath::project()],
        };
        let at = |path: &str| focus.nearness(&OfferPath::parse(path).unwrap());
        assert_eq!(at("devices/connect-usb"), OfferNearness::Area);
        assert_eq!(at("project/new"), OfferNearness::Area);
        assert_eq!(at("library/x"), OfferNearness::Elsewhere);
    }
}
