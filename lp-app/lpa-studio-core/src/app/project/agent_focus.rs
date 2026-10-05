//! What the user is looking at in the open project: the node whose card
//! has focus, or the node the patch surface has selected — for the app
//! agent's "you are looking at" and the ⌘K palette's focus.
//!
//! Both facts were already core's (node focus, `UiSelection`); this only
//! reads them. Which one applies depends on the page's view, which the web
//! reports as place (`UiPlace`).

use super::agent_project_edits::node_display_name;
use super::node::NodeController;
use crate::{OfferPath, ProjectController, UiProjectView};

/// The node the user is looking at, as the agent's readout names it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentNodeFocus {
    /// Where its verbs live (`project/demo.module/orbit.shader`).
    pub prefix: OfferPath,
    /// Its name as the agent addresses it (`playlist/palette_waves`).
    pub name: String,
    /// Its kind (`Shader`).
    pub kind: String,
    /// Its status label, and the detail when there is one.
    pub status: String,
    /// What its card has open, in plain words (`code`, `assistant chat`).
    pub open: Vec<&'static str>,
}

impl ProjectController {
    /// The node the user is looking at in `view`: the focused card in the
    /// node workspace, the selection's node in the patch and mapping
    /// views, and none in play mode (it shows a panel, not a node).
    pub(crate) fn looked_at_node(&self, view: UiProjectView) -> Option<&NodeController> {
        match view {
            UiProjectView::Nodes => self.focused_node(),
            UiProjectView::Patch | UiProjectView::Mapping => self.selected_node(),
            UiProjectView::Play => None,
        }
    }

    /// The node at `view`'s focus, as the agent's readout names it.
    pub(crate) fn agent_node_focus(&self, view: UiProjectView) -> Option<AgentNodeFocus> {
        let node = self.looked_at_node(view)?;
        let status = node.status();
        let status = match &status.detail {
            Some(detail) => format!("{}: {detail}", status.label),
            None => status.label.clone(),
        };
        Some(AgentNodeFocus {
            prefix: OfferPath::project_node(node.address()),
            name: match node_display_name(node.address()) {
                name if name.is_empty() => "the project's root module".to_string(),
                name => name,
            },
            kind: node.kind().to_string(),
            status,
            open: self.open_sections(node),
        })
    }

    /// Where the verbs of the node the agent names (`read`'s name: a path
    /// of names or a bare name) live.
    pub(crate) fn agent_node_prefix(&self, wanted: &str) -> Result<OfferPath, String> {
        self.agent_node(wanted)
            .map(|node| OfferPath::project_node(node.address()))
    }

    /// The node whose offers live at `prefix`
    /// (`project/demo.module/fixture.fixture`), anywhere in the tree.
    pub(crate) fn node_at_prefix(&self, prefix: &OfferPath) -> Option<&NodeController> {
        let mut stack: Vec<&NodeController> = self.root_nodes().iter().collect();
        while let Some(node) = stack.pop() {
            if OfferPath::project_node(node.address()) == *prefix {
                return Some(node);
            }
            stack.extend(node.children().iter());
        }
        None
    }

    /// The name the chat calls the node at `prefix` by (`playlist/spiral`;
    /// empty for the project's root module).
    pub(crate) fn node_name_at_prefix(&self, prefix: &OfferPath) -> Option<String> {
        self.node_at_prefix(prefix)
            .map(|node| node_display_name(node.address()))
    }

    /// The node whose card has focus (first in tree order).
    fn focused_node(&self) -> Option<&NodeController> {
        let mut stack: Vec<&NodeController> = self.root_nodes().iter().rev().collect();
        while let Some(node) = stack.pop() {
            if self.is_focused_node(node) {
                return Some(node);
            }
            stack.extend(node.children().iter().rev());
        }
        None
    }

    /// The node the patch surface's one selection is part of.
    fn selected_node(&self) -> Option<&NodeController> {
        let node = self.patch_selection_single()?.node()?;
        self.node_by_runtime_id(node)
    }

    /// What `node`'s card has open beyond its face, in plain words.
    fn open_sections(&self, node: &NodeController) -> Vec<&'static str> {
        let Some(card) = self.node_card_ui_state(&node.address().to_string()) else {
            return Vec::new();
        };
        let mut open = Vec::new();
        for (is_open, name) in [
            (card.code_open, "code"),
            (card.space_open, "dimensionality"),
            (card.wiring_open, "wiring"),
            (card.advanced_open, "advanced"),
            (card.debug_open, "debug"),
            (!card.agent_collapsed, "shader agent chat"),
        ] {
            if is_open {
                open.push(name);
            }
        }
        open
    }
}
