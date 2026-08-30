//! Stories for the playlist card face (the ENTRIES strip; the active child
//! renders below the card as a sibling — P2c item 2).
//!
//! Four entries with duration chips, one cue-tagged entry, and the
//! "ACTIVE" badge riding the playing entry's thumbnail (Yona Q5: ACTIVE,
//! matching `PlaylistState.active_entry`; the thumbnail stays — hiding it
//! made the playing entry the one chip you couldn't recognize). Coverage:
//! the strip alone, the active child's card stacked below as its own card,
//! and the empty playlist.

use dioxus::prelude::*;
use lpa_studio_core::app::project::node::add_node_menu;
use lpa_studio_core::{ProjectNodeAddress, UiAttachTarget, UiNodeFace};
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
    description = "Entry strip: four entries with m:ss duration chips, the cue-tagged Tide entry (hold), and the ACTIVE badge over Aurora's thumbnail — no child card (nothing derived yet, e.g. before the mirror loads)."
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
    description = "The strip with the add chip at the end of the card row (authoring P5): a dashed card-family chip opening the SAME kind picker as the project header's '+', attaching into THIS playlist's entries. Additive-only per the faces ADR — delete stays on the pane header; one-active-child and chip-click-selects behavior unchanged."
)]
fn strip_with_add_chip() -> Element {
    let mut view = playlist_node_face_view();
    view.children = Vec::new();
    // Two entries so the strip's tail — where the chip lives — sits inside
    // the card frame instead of past the horizontal scroll edge.
    if let Some(UiNodeFace::Playlist(face)) = view.face.as_mut() {
        face.entries.truncate(2);
    }
    // The controller supplies the playlist's own picker (attach = this
    // playlist) on its node view (P4).
    view.add_node_menu = Some(add_node_menu(&UiAttachTarget::Playlist {
        node: ProjectNodeAddress::parse("/fyeah_sign.show/evening.playlist")
            .expect("valid story node address"),
    }));
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
