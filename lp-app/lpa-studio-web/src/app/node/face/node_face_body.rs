//! Dispatch for a node card's face body: the kind-specific permanent face
//! plus the drawers under it.
//!
//! [`super::super::NodePane`] renders this when `UiNodeView.face` is `Some`
//! (shader/fixture/playlist today); nodes without a face keep the generic
//! tab/section body. The body owns the full-bleed container: faces and
//! drawers emit flat
//! [`super::NodeCardSection`]s that span the card edge-to-edge, divided by
//! 1px hairlines — one surface, no inner boxes. Children (the playlist's
//! active child included) render OUTSIDE the body, as sibling cards below
//! the pane (P2c item 2).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiNodeFace, UiNodeSection, UiPendingEdit};

use crate::app::node::NodeDirtyTint;
use crate::base::Platform;

use super::{FixtureFace, NodeCardDrawers, PlaylistFace, ShaderFace};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn NodeFaceBody(
    face: UiNodeFace,
    /// The main tab's sections — the advanced drawer hosts them unchanged.
    sections: Vec<UiNodeSection>,
    /// Open the code drawer on first render (stories).
    #[props(default = false)]
    code_drawer_open: bool,
    /// Open the advanced drawer on first render (stories).
    #[props(default = false)]
    advanced_drawer_open: bool,
    /// Pin this control's corner ⓘ popover open on first render (stories).
    #[props(default = None)]
    detail_open_control: Option<String>,
    /// Platform for code-editor shortcut hints; stories pin it.
    #[props(default = None)]
    platform: Option<Platform>,
    #[props(default)] pending_edits: Vec<UiPendingEdit>,
    #[props(default)] dirty_tint: NodeDirtyTint,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        // Full-bleed: sections reclaim the pane's padding and sit flush
        // under the header's bottom border.
        div { class: "tw:-mx-4 tw:-mb-4 tw:grid tw:min-w-0",
            {match face {
                UiNodeFace::Shader(shader) => rsx! {
                    ShaderFace {
                        face: shader.clone(),
                        detail_open_control,
                        on_action,
                    }
                    NodeCardDrawers {
                        code: shader.code_drawer,
                        sections,
                        code_initially_open: code_drawer_open,
                        advanced_initially_open: advanced_drawer_open,
                        platform,
                        pending_edits,
                        dirty_tint,
                        on_action,
                    }
                },
                UiNodeFace::Fixture(fixture) => rsx! {
                    FixtureFace {
                        face: fixture,
                        detail_initially_open: detail_open_control.is_some(),
                        on_action,
                    }
                    NodeCardDrawers {
                        sections,
                        advanced_initially_open: advanced_drawer_open,
                        platform,
                        pending_edits,
                        dirty_tint,
                        on_action,
                    }
                },
                UiNodeFace::Playlist(playlist) => rsx! {
                    PlaylistFace { face: playlist, on_action }
                    NodeCardDrawers {
                        sections,
                        advanced_initially_open: advanced_drawer_open,
                        platform,
                        pending_edits,
                        dirty_tint,
                        on_action,
                    }
                },
            }}
        }
    }
}
