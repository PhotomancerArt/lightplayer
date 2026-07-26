//! Stories for the playlist card face (the ENTRIES strip; the active child
//! renders below the card as a sibling — P2c item 2).
//!
//! Four entries with duration chips, one cue-tagged entry, and the
//! "ACTIVE" placard replacing the playing entry's thumbnail (Yona Q5:
//! ACTIVE, matching `PlaylistState.active_entry`). Coverage: the strip
//! alone, the active child's card stacked below as its own card, and the
//! empty playlist.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::{empty_playlist_node_view, playlist_node_face_view};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PlaylistCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Entry strip: four entries with m:ss duration chips, the cue-tagged Tide entry (hold), and the ACTIVE placard on Aurora — no child card (nothing derived yet, e.g. before the mirror loads)."
)]
fn strip() -> Element {
    let mut view = playlist_node_face_view();
    view.children = Vec::new();
    rsx! {
        PlaylistCardCanvas {
            NodePane { view, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "The active child's real card BELOW the playlist card, as a sibling (P2c item 2): Aurora's own shader card (face, knobs, drawers) — editing the playing entry is editing the child's card; one rendering of the active child, zero of the others."
)]
fn active_child_below() -> Element {
    rsx! {
        PlaylistCardCanvas {
            NodePane {
                view: playlist_node_face_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Empty playlist: the face's no-entries placeholder over the settings drawer."
)]
fn empty() -> Element {
    rsx! {
        PlaylistCardCanvas {
            NodePane {
                view: empty_playlist_node_view(),
                on_action: move |_| {},
            }
        }
    }
}
