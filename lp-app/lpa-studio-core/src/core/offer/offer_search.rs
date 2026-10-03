//! [`UiOfferTree::search`]: the offers a typed query finds, best first —
//! what the ⌘K command palette lists.
//!
//! Ranking lives here, beside the tree, so the palette only renders it and
//! any other consumer (a test, the agent) gets the same order. The query
//! text and whether a palette is open are the web's own chrome.
//!
//! Where the user is counts too (M7): the tree carries its focus
//! ([`UiOfferTree::focus`]), and among equally good matches the focused
//! node's verbs come first, then the verbs of nodes inside it, then the
//! page's area, then the rest.

use crate::{OfferNearness, UiOffer, UiOfferTree};

impl UiOfferTree {
    /// The offers `query` finds, best first.
    ///
    /// Case-insensitive. An offer is found by its **label**, then its
    /// **path** (`project/demo.module/remove`), then its **summary**, and
    /// ranks by the first of those that matches. Within a field, the whole
    /// query as a substring beats the query's letters in order with gaps
    /// between them from a word's start (`rvt` finds "Revert"), and an
    /// earlier substring beats a later one. The summary is matched as a
    /// substring only: it is a sentence, and nearly any short query is a
    /// subsequence of one.
    ///
    /// Nearness to the user's focus ([`OfferNearness`]) decides between
    /// matches of the same field and the same kind — a whole-query
    /// substring of the label near the user beats one far from it — but
    /// never lifts a weaker match over a stronger one: "rev" still finds
    /// "Revert" anywhere before "Remove" on the focused node. Only then
    /// does the match's position or spread count.
    ///
    /// Ties keep publish order, and an empty (or all-whitespace) query
    /// returns every offer nearest first, in publish order within each
    /// nearness — publish order alone when the tree has no focus.
    pub fn search(&self, query: &str) -> Vec<&UiOffer> {
        let query = OfferQuery::new(query);
        let focus = self.focus();
        if query.is_empty() {
            let mut all: Vec<(OfferNearness, usize, &UiOffer)> = self
                .iter()
                .enumerate()
                .map(|(at, offer)| (focus.nearness(&offer.path), at, offer))
                .collect();
            all.sort_by_key(|(near, at, _)| (*near, *at));
            return all.into_iter().map(|(_, _, offer)| offer).collect();
        }
        let mut hits: Vec<(OfferRank, OfferNearness, usize, usize, &UiOffer)> = self
            .iter()
            .enumerate()
            .filter_map(|(at, offer)| {
                query.rank(offer).map(|rank| {
                    let near = focus.nearness(&offer.path);
                    (rank, near, rank.kind.detail(), at, offer)
                })
            })
            .collect();
        hits.sort_by_key(|(rank, near, detail, at, _)| {
            (rank.field, rank.kind.class(), *near, *detail, *at)
        });
        hits.into_iter().map(|(_, _, _, _, offer)| offer).collect()
    }
}

/// How well one offer matched, best first (the derived order is the
/// ranking: field, then kind, then position).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OfferRank {
    field: OfferField,
    kind: MatchKind,
}

/// Which of the offer's texts matched, in rank order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OfferField {
    Label,
    Path,
    Summary,
}

/// How a field matched, in rank order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MatchKind {
    /// The whole query, as a substring starting at this byte.
    Substring { at: usize },
    /// The query's letters in order from a word's start, spread over this
    /// many bytes.
    Subsequence { span: usize },
}

impl MatchKind {
    /// Which kind of match, strongest first: a substring (0) or letters in
    /// order (1).
    fn class(self) -> u8 {
        match self {
            Self::Substring { .. } => 0,
            Self::Subsequence { .. } => 1,
        }
    }

    /// Where the substring starts, or how far the letters spread: lower is
    /// better, within one class.
    fn detail(self) -> usize {
        match self {
            Self::Substring { at } => at,
            Self::Subsequence { span } => span,
        }
    }
}

/// A query, folded once for every offer it is matched against.
struct OfferQuery {
    /// Lowercased and trimmed: what a substring match looks for.
    text: String,
    /// The same letters without whitespace: what a subsequence match
    /// walks ("save rev" still finds "Save, then revert").
    letters: Vec<char>,
}

impl OfferQuery {
    fn new(query: &str) -> Self {
        let text = query.trim().to_lowercase();
        let letters = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        Self { text, letters }
    }

    fn is_empty(&self) -> bool {
        self.letters.is_empty()
    }

    /// The offer's best match, or `None` when nothing it says matches.
    fn rank(&self, offer: &UiOffer) -> Option<OfferRank> {
        let label = offer.label().to_lowercase();
        let path = offer.path.to_string().to_lowercase();
        let summary = offer.summary().to_lowercase();
        let ranked = |field, kind: Option<MatchKind>| kind.map(|kind| OfferRank { field, kind });
        ranked(OfferField::Label, self.match_in(&label, true))
            .or_else(|| ranked(OfferField::Path, self.match_in(&path, true)))
            .or_else(|| ranked(OfferField::Summary, self.match_in(&summary, false)))
    }

    /// How `haystack` (already lowercased) matches: a substring first, a
    /// subsequence when `loose` allows one.
    fn match_in(&self, haystack: &str, loose: bool) -> Option<MatchKind> {
        if let Some(at) = haystack.find(&self.text) {
            return Some(MatchKind::Substring { at });
        }
        if !loose {
            return None;
        }
        subsequence_span(haystack, &self.letters).map(|span| MatchKind::Subsequence { span })
    }
}

/// The fewest bytes `letters` spread over when found in order in
/// `haystack` starting at a word's first letter (the haystack's start, or
/// after anything that is not a letter or digit: a space, `/`, `.`);
/// `None` when no such run exists. The word start is what keeps a loose
/// match honest: "rev" finds "Remove", never the r·e…v scattered through
/// `project/save`.
fn subsequence_span(haystack: &str, letters: &[char]) -> Option<usize> {
    let (&first, rest) = letters.split_first()?;
    let mut previous = None;
    let mut best: Option<usize> = None;
    for (start, ch) in haystack.char_indices() {
        let word_start = previous.is_none_or(|previous: char| !previous.is_alphanumeric());
        previous = Some(ch);
        if ch != first || !word_start {
            continue;
        }
        let after = start + ch.len_utf8();
        if let Some(end) = letters_end(&haystack[after..], rest) {
            let span = after + end - start;
            best = Some(best.map_or(span, |best| best.min(span)));
        }
    }
    best
}

/// Where `letters` end when taken in order from `text`, each at its first
/// chance: the byte after the last one (`0` for no letters).
fn letters_end(text: &str, letters: &[char]) -> Option<usize> {
    let mut wanted = letters.iter().peekable();
    if wanted.peek().is_none() {
        return Some(0);
    }
    for (at, ch) in text.char_indices() {
        if wanted.peek() == Some(&&ch) {
            wanted.next();
            if wanted.peek().is_none() {
                return Some(at + ch.len_utf8());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{
        ActionConfirmation, ControllerId, OfferPath, ProjectNodeAddress, ProjectOp, UiAction,
        UiOffer, UiOfferFocus, UiOfferTree,
    };

    #[test]
    fn an_empty_query_returns_everything_in_publish_order() {
        let tree = dirty_project_tree();

        for query in ["", "   "] {
            assert_eq!(
                paths(tree.search(query)),
                [
                    "project/save",
                    "project/revert",
                    "project/demo.module/revert",
                    "project/demo.module/orbit.shader/remove",
                ],
                "{query:?}"
            );
        }
    }

    #[test]
    fn matching_ignores_case() {
        let tree = dirty_project_tree();

        assert_eq!(paths(tree.search("SAVE")), paths(tree.search("save")));
        assert_eq!(paths(tree.search("SAVE"))[0], "project/save");
    }

    #[test]
    fn a_label_match_ranks_above_a_path_match_above_a_summary_match() {
        let mut tree = UiOfferTree::new();
        tree.publish(offer("project/save", "Save", "Keeps the orbit edits."));
        tree.publish(offer("project/orbit.shader/remove", "Remove", ""));
        tree.publish(offer("project/focus", "Orbit view", ""));

        assert_eq!(
            paths(tree.search("orbit")),
            [
                "project/focus",
                "project/orbit.shader/remove",
                "project/save"
            ],
            "label, then path, then summary — never publish order across fields"
        );
    }

    #[test]
    fn letters_in_order_find_a_label() {
        let tree = dirty_project_tree();

        assert_eq!(
            paths(tree.search("rvt")),
            ["project/revert", "project/demo.module/revert"],
            "r…v…t finds \"Revert to saved\" and \"Revert\", in publish order"
        );
        assert!(
            tree.search("zzz").is_empty(),
            "nothing matches, nothing listed"
        );
    }

    #[test]
    fn a_substring_beats_letters_in_order_and_an_earlier_one_beats_a_later() {
        let tree = dirty_project_tree();

        // "rev" is a substring of both reverts, and only letters-in-order
        // in "Remove" (r·e…v): the reverts lead, the removal follows.
        assert_eq!(
            paths(tree.search("rev")),
            [
                "project/revert",
                "project/demo.module/revert",
                "project/demo.module/orbit.shader/remove",
            ]
        );

        let mut tree = UiOfferTree::new();
        tree.publish(offer("project/later", "Undo save", ""));
        tree.publish(offer("project/sooner", "Save copy", ""));
        assert_eq!(
            paths(tree.search("save")),
            ["project/sooner", "project/later"],
            "a prefix match beats a later one"
        );
    }

    #[test]
    fn a_summary_is_matched_as_a_substring_only() {
        let mut tree = UiOfferTree::new();
        tree.publish(offer("project/save", "Save", "Writes the project to disk."));

        assert_eq!(paths(tree.search("to disk")), ["project/save"]);
        assert!(
            tree.search("wtd").is_empty(),
            "w…t…d is in the summary, but a sentence holds nearly any short query"
        );
    }

    #[test]
    fn spaces_in_the_query_do_not_break_letters_in_order() {
        let tree = dirty_project_tree();

        assert_eq!(paths(tree.search("rev saved")), ["project/revert"]);
    }

    #[test]
    fn with_a_focus_the_focused_nodes_verbs_lead_an_empty_query() {
        let mut tree = dirty_project_tree();
        tree.publish(offer("devices/connect-usb", "Connect via USB", ""));
        tree.set_focus(UiOfferFocus {
            node: Some(OfferPath::project_node(
                &ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap(),
            )),
            area: Some(OfferPath::project()),
        });

        assert_eq!(
            paths(tree.search("")),
            [
                "project/demo.module/orbit.shader/remove",
                "project/save",
                "project/revert",
                "project/demo.module/revert",
                "devices/connect-usb",
            ],
            "the focused node, then the page's area in publish order, then elsewhere"
        );
    }

    #[test]
    fn a_match_near_the_focus_beats_an_equal_match_far_from_it() {
        let mut tree = UiOfferTree::new();
        for node in ["/clock.clock", "/fixture.fixture", "/orbit.shader"] {
            let address = ProjectNodeAddress::parse(node).unwrap();
            tree.publish(UiOffer::new(
                OfferPath::project_node(&address).child("remove"),
                "remove",
                UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay)
                    .with_label("Remove node"),
            ));
        }
        tree.publish(offer("devices/remove-all", "Remove every board", ""));
        let unfocused = paths(tree.search("remove"));
        assert_eq!(
            unfocused[0], "project/clock.clock/remove",
            "no focus: publish order"
        );

        tree.set_focus(UiOfferFocus {
            node: Some(OfferPath::project_node(
                &ProjectNodeAddress::parse("/fixture.fixture").unwrap(),
            )),
            area: Some(OfferPath::devices()),
        });
        assert_eq!(
            paths(tree.search("remove")),
            [
                "project/fixture.fixture/remove",
                "devices/remove-all",
                "project/clock.clock/remove",
                "project/orbit.shader/remove",
            ],
            "the focused node's Remove, then the page's, then the rest"
        );
    }

    #[test]
    fn nearness_never_lifts_a_weaker_match_over_a_stronger_one() {
        let mut tree = dirty_project_tree();
        tree.set_focus(UiOfferFocus {
            node: Some(OfferPath::project_node(
                &ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap(),
            )),
            area: Some(OfferPath::project()),
        });

        // "rev" is a substring of both reverts and only letters-in-order in
        // the focused node's "Remove": the substrings still lead.
        assert_eq!(
            paths(tree.search("rev")),
            [
                "project/revert",
                "project/demo.module/revert",
                "project/demo.module/orbit.shader/remove",
            ]
        );
    }

    /// The tree a dirty project publishes: Save, Revert to saved, and a
    /// node's revert and (Lasting) remove.
    fn dirty_project_tree() -> UiOfferTree {
        let module = ProjectNodeAddress::parse("/demo.module").unwrap();
        let shader = ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap();
        let project = |op| UiAction::from_op(ControllerId::new("studio|project"), op);
        let mut tree = UiOfferTree::new();
        tree.publish(UiOffer::new(
            OfferPath::project().child("save"),
            "save",
            project(ProjectOp::SaveOverlay),
        ));
        tree.publish(UiOffer::new(
            OfferPath::project().child("revert"),
            "revert",
            project(ProjectOp::RevertAllEdits).with_label("Revert to saved"),
        ));
        tree.publish(UiOffer::new(
            OfferPath::project_node(&module).child("revert"),
            "revert",
            project(ProjectOp::RevertAllEdits).with_label("Revert"),
        ));
        tree.publish(UiOffer::new(
            OfferPath::project_node(&shader).child("remove"),
            "remove",
            project(ProjectOp::RevertAllEdits)
                .with_label("Remove")
                .lasting(ActionConfirmation::new(
                    "Remove Orbit?",
                    "Its edits are discarded.",
                    "remove",
                )),
        ));
        tree
    }

    fn offer(path: &str, label: &str, summary: &str) -> UiOffer {
        UiOffer::new(
            OfferPath::parse(path).unwrap(),
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay)
                .with_label(label)
                .with_summary(summary),
        )
    }

    fn paths(offers: Vec<&UiOffer>) -> Vec<String> {
        offers.iter().map(|offer| offer.path.to_string()).collect()
    }
}
