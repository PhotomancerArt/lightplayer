//! Which project and node verbs a pane header draws as its own buttons.
//!
//! The project header and every node card draw the verbs directly under
//! their prefix (`UiOfferTree::verbs_of`) as icon buttons: Save and Revert
//! on the project, Revert, Remove and Ask agent on a card. Some verbs at
//! those same prefixes have a control of their own instead — the add-node
//! picker presses `add-node`, `import-pattern` and `paste-node`, the node's
//! detail popup presses `copy`, and the "Debug active" chip presses
//! `clear-debug` — so a header that drew them too would show the same verb
//! twice. They are still in the tree, where the agent and ⌘K find them.

use super::node::{ADD_NODE_VERB, COPY_NODE_VERB, IMPORT_PATTERN_VERB, PASTE_NODE_VERB};

/// `project/clear-debug`: clear every debug override in the project. Only
/// published while one is active.
pub const CLEAR_DEBUG_VERB: &str = "clear-debug";

/// Whether a header draws the verb `verb` (an offer path's last segment) as
/// a button of its own; `false` for the verbs another control presses.
pub fn is_header_verb(verb: &str) -> bool {
    ![
        ADD_NODE_VERB,
        IMPORT_PATTERN_VERB,
        PASTE_NODE_VERB,
        COPY_NODE_VERB,
        CLEAR_DEBUG_VERB,
    ]
    .contains(&verb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verbs_with_their_own_control_stay_out_of_the_header() {
        for verb in ["save", "revert", "remove", "ask-agent"] {
            assert!(is_header_verb(verb), "{verb}");
        }
        for verb in [
            "add-node",
            "import-pattern",
            "paste-node",
            "copy",
            "clear-debug",
        ] {
            assert!(!is_header_verb(verb), "{verb}");
        }
    }
}
