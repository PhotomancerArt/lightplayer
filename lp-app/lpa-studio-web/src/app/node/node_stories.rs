use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::node_story_fixtures::{error_node_view, playlist_node_view};

#[story(description = "A composed node pane showing the current node anatomy direction.")]
pub(crate) fn node_pane() -> Element {
    rsx! {
        NodePane { view: playlist_node_view() }
    }
}

#[story(description = "Node pane with an error status and projection issues.")]
pub(crate) fn error_node() -> Element {
    rsx! {
        NodePane { view: error_node_view() }
    }
}
