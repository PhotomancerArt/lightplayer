//! One edge between nodes: whether it shows and what it says. The view
//! routes it, since only the view knows where the cards landed.

use crate::domain::node::NodeId;
use crate::ux::ui::Emphasis;

#[derive(Debug, Clone, PartialEq)]
pub struct UiCanvasEdge {
    pub from: NodeId,
    pub to: NodeId,
    /// The channel it carries, always shown (D53); empty for a fixture
    /// reaching an output only by place rules.
    pub channel: String,
    /// What it carries, shown while it belongs to the selection (D57).
    pub label: Option<String>,
    /// What it carries in full, for the note beside the cursor.
    pub carries: String,
    /// The target's port rows it lands beside, by key (`edges.rs`).
    pub lands_on: Vec<String>,
    /// Drawn in this layout and focus.
    pub shown: bool,
    pub emphasis: Emphasis,
    /// The pointer is on it (the view reported it).
    pub hovered: bool,
}
