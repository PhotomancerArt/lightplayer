//! Bus pane view DTOs: channels, values, and linked writer/reader sites.

use crate::UiAction;

/// The bus pane body: every channel referenced by at least one binding.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusView {
    /// Channels in wire order (binding-index discovery order).
    pub channels: Vec<UiBusChannelView>,
}

impl UiBusView {
    /// A bus view with no channels (empty project or snapshot pending).
    pub fn empty() -> Self {
        Self {
            channels: Vec::new(),
        }
    }
}

/// One bus channel row.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusChannelView {
    /// Channel name (`time.seconds`, `trigger`, `visual.out`, …).
    pub name: String,
    /// Established semantic kind label, when known.
    pub kind: Option<String>,
    /// Resolved current value display, when the snapshot carried values.
    pub value: Option<String>,
    /// Resolution failure detail, when the value could not resolve.
    pub value_error: Option<String>,
    /// The primary-visual channel (`visual.out`) — the product's main
    /// output; previews hang off it (roadmap M6).
    pub primary_visual: bool,
    /// Sites publishing to this channel, highest priority first.
    pub writers: Vec<UiBusSiteView>,
    /// Sites consuming from this channel.
    pub readers: Vec<UiBusSiteView>,
}

/// One writer/reader site on a channel — always a navigation affordance:
/// clicking dispatches the focus action so the user lands on the node
/// (roadmap D7: the UI feels linked, no path hunting).
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusSiteView {
    /// Display label of the owning node.
    pub node_label: String,
    /// Anchor slot on the node, when the binding has one.
    pub slot: Option<String>,
    /// True when the binding came from default policy rather than authoring.
    pub default_origin: bool,
    /// Focus/reveal action for the owning node, when it is in the tree.
    pub focus: Option<UiAction>,
}
