//! [`UiBoardPanelHeading`]: what a nested group of the connected card's
//! panel is called.

/// A nested group's heading: its node's name and kind (the kind label
/// `node_kind_icon` reads: "Module", "Playlist", …).
#[derive(Clone, Debug, PartialEq)]
pub struct UiBoardPanelHeading {
    /// The group's name: the node's own, or the playing pattern's.
    pub name: String,
    /// The node kind label of the node whose scope the group presents; the
    /// web turns it into the kind's icon.
    pub kind: String,
}
