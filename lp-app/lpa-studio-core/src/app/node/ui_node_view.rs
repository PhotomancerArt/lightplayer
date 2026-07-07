//! Complete node pane data.

use crate::{UiAction, UiNodeChild, UiNodeHeader, UiNodeTab, UiNodeTabBody, UiPaneAction};

/// The full data model for a Studio node pane.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNodeView {
    /// Stable id used by renderers for keys and future actions.
    pub node_id: String,
    /// Header identity and status metadata.
    pub header: UiNodeHeader,
    /// Contextual header actions (the pane grammar's actions slot):
    /// controller-produced, currently the node-subtree batch revert while
    /// the header's dirty summary announces pending edits.
    pub header_actions: Vec<UiPaneAction>,
    /// Tabs rendered inside the node pane.
    pub tabs: Vec<UiNodeTab>,
    /// Child nodes extracted from the config slot tree.
    pub children: Vec<UiNodeChild>,
    /// Whether this node is the focused/selected node.
    pub focused: bool,
    /// Action that focuses this node as the current Studio selection.
    pub action: Option<UiAction>,
    /// Whether the pane starts collapsed.
    pub collapsed: bool,
    /// Projection or runtime issues for the whole node.
    pub issues: Vec<String>,
}

impl UiNodeView {
    /// Create a node pane view.
    pub fn new(header: UiNodeHeader, tabs: Vec<UiNodeTab>) -> Self {
        let node_id = header.path.clone();
        Self {
            node_id,
            header,
            header_actions: Vec::new(),
            tabs,
            children: Vec::new(),
            focused: false,
            action: None,
            collapsed: false,
            issues: Vec::new(),
        }
    }

    /// Override the stable id.
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = node_id.into();
        self
    }

    /// Set the contextual header actions.
    pub fn with_header_actions(mut self, actions: Vec<UiPaneAction>) -> Self {
        self.header_actions = actions;
        self
    }

    /// Set extracted child nodes.
    pub fn with_children(mut self, children: Vec<UiNodeChild>) -> Self {
        self.children = children;
        self
    }

    /// Returns true when any tab contains node anatomy sections.
    pub fn has_sections(&self) -> bool {
        self.tabs.iter().any(|tab| match &tab.body {
            UiNodeTabBody::Sections(sections) => sections.iter().any(|section| !section.is_empty()),
            UiNodeTabBody::Text { .. } => false,
        })
    }

    /// Returns true when this node has extracted children.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}
