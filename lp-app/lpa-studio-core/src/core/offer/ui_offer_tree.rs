//! [`UiOfferTree`]: every offer the view publishes, by path.

use std::collections::BTreeMap;

use lpa_devices::DeviceId;

use crate::{OfferPath, UiOffer, UiOfferFocus};

/// Every offer the view publishes, addressed by path, in **publish order**.
///
/// The order is the order core published in, never sorted by path: a
/// surface draws its verbs in that order (Save before Revert), and the
/// agent's readout lists them the same way. A path is published at most
/// once per build; publishing one twice is a bug in the publisher.
///
/// The tree also knows where each device's verbs live
/// ([`Self::device_prefix`]): a device card is handed a roster handle, and
/// its verbs are at `devices/<board ref>` — a ref only core can work out
/// (the MAC, the kind its endpoint names, and the de-duplication when two
/// entries answer to one MAC).
///
/// And it knows where the user is ([`Self::focus`]): the focused node and
/// the page's area, as prefixes. [`Self::search`] ranks by it, so the ⌘K
/// palette and any other consumer get the same focus-near-first order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiOfferTree {
    offers: Vec<UiOffer>,
    index: BTreeMap<OfferPath, usize>,
    devices: BTreeMap<DeviceId, OfferPath>,
    focus: UiOfferFocus,
}

impl UiOfferTree {
    /// An empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish `offer` at its path.
    ///
    /// A path published twice is a bug: it trips a debug assertion, and in
    /// a release build the last write wins (in the first one's place).
    pub fn publish(&mut self, offer: UiOffer) {
        debug_assert!(
            !self.index.contains_key(&offer.path),
            "duplicate offer published at `{}`",
            offer.path
        );
        if let Some(&at) = self.index.get(&offer.path) {
            self.offers[at] = offer;
            return;
        }
        self.index.insert(offer.path.clone(), self.offers.len());
        self.offers.push(offer);
    }

    /// Publish every offer of `other` after this tree's own, in `other`'s
    /// order — how a builder that collected a subtree's offers on the side
    /// splices them in behind the offers it publishes first.
    pub fn append(&mut self, other: UiOfferTree) {
        for offer in other.offers {
            self.publish(offer);
        }
        self.devices.extend(other.devices);
    }

    /// Say that `device`'s verbs live under `prefix` (`devices/<board ref>`).
    pub fn place_device(&mut self, device: DeviceId, prefix: OfferPath) {
        self.devices.insert(device, prefix);
    }

    /// Say where the user is (core works it out from place and the focused
    /// node, after publishing).
    pub fn set_focus(&mut self, focus: UiOfferFocus) {
        self.focus = focus;
    }

    /// Where the user is, as offer prefixes.
    pub fn focus(&self) -> &UiOfferFocus {
        &self.focus
    }

    /// Where `device`'s verbs live, when the roster has it: the prefix a
    /// card hands [`Self::verbs_of`].
    pub fn device_prefix(&self, device: DeviceId) -> Option<&OfferPath> {
        self.devices.get(&device)
    }

    /// The offer at `path`, if one is published.
    pub fn get(&self, path: &OfferPath) -> Option<&UiOffer> {
        self.index.get(path).map(|&at| &self.offers[at])
    }

    /// Every offer, in publish order: the walker.
    pub fn iter(&self) -> impl Iterator<Item = &UiOffer> {
        self.offers.iter()
    }

    /// The verbs directly under `prefix`: offers whose path is `prefix` plus
    /// exactly one segment, in publish order. A node card asks for
    /// `project/<node>`, and gets its own verbs, never its children's.
    pub fn verbs_of(&self, prefix: &OfferPath) -> impl Iterator<Item = &UiOffer> + '_ {
        let prefix = prefix.clone();
        self.offers.iter().filter(move |offer| {
            offer.path.len() == prefix.len() + 1 && offer.path.starts_with(&prefix)
        })
    }

    /// How many offers are published.
    pub fn len(&self) -> usize {
        self.offers.len()
    }

    /// Whether nothing is published.
    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }
}

impl<'a> IntoIterator for &'a UiOfferTree {
    type Item = &'a UiOffer;
    type IntoIter = core::slice::Iter<'a, UiOffer>;

    fn into_iter(self) -> Self::IntoIter {
        self.offers.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControllerId, ProjectNodeAddress, ProjectOp, UiAction};

    #[test]
    fn verbs_of_returns_only_direct_verbs_in_publish_order() {
        let node = OfferPath::project_node(&ProjectNodeAddress::parse("/demo.module").unwrap());
        let child = OfferPath::project_node(
            &ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap(),
        );
        let mut tree = UiOfferTree::new();
        tree.publish(offer(OfferPath::project().child("save")));
        tree.publish(offer(OfferPath::project().child("revert")));
        tree.publish(offer(node.clone().child("revert")));
        tree.publish(offer(child.clone().child("remove")));
        tree.publish(offer(child.clone().child("revert")));

        assert_eq!(
            paths(tree.verbs_of(&OfferPath::project())),
            ["project/save", "project/revert"],
            "a node's verbs are not the project's"
        );
        assert_eq!(paths(tree.verbs_of(&node)), ["project/demo.module/revert"]);
        assert_eq!(
            paths(tree.verbs_of(&child)),
            [
                "project/demo.module/orbit.shader/remove",
                "project/demo.module/orbit.shader/revert"
            ],
            "publish order, never sorted"
        );
        assert_eq!(tree.len(), 5);
        assert!(
            tree.get(&OfferPath::project().child("save")).is_some(),
            "get finds by path"
        );
        assert!(tree.get(&OfferPath::project().child("nope")).is_none());
    }

    #[test]
    fn append_keeps_both_orders() {
        let mut tree = UiOfferTree::new();
        tree.publish(offer(OfferPath::project().child("save")));
        let mut side = UiOfferTree::new();
        side.publish(offer(OfferPath::devices().child("connect-usb")));
        tree.append(side);

        assert_eq!(paths(tree.iter()), ["project/save", "devices/connect-usb"]);
    }

    #[test]
    fn a_device_is_found_by_its_handle() {
        let mut tree = UiOfferTree::new();
        let prefix = OfferPath::board(&crate::BoardRef::New(DeviceId(4)));
        tree.publish(offer(prefix.clone().child("forget")));
        tree.place_device(DeviceId(4), prefix.clone());

        let found = tree.device_prefix(DeviceId(4)).expect("placed");
        assert_eq!(paths(tree.verbs_of(found)), ["devices/new-4/forget"]);
        assert_eq!(tree.device_prefix(DeviceId(5)), None);

        let mut other = UiOfferTree::new();
        other.append(tree);
        assert_eq!(
            other.device_prefix(DeviceId(4)),
            Some(&prefix),
            "append keeps it"
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "duplicate offer published at `project/save`")]
    fn a_duplicate_publish_trips_the_debug_assertion() {
        let mut tree = UiOfferTree::new();
        tree.publish(offer(OfferPath::project().child("save")));
        tree.publish(offer(OfferPath::project().child("save")));
    }

    fn offer(path: OfferPath) -> UiOffer {
        UiOffer::new(
            path,
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
        )
    }

    fn paths<'a>(offers: impl Iterator<Item = &'a UiOffer>) -> Vec<String> {
        offers.map(|offer| offer.path.to_string()).collect()
    }
}
