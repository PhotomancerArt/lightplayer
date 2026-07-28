//! Combined node-cards gallery for the P2/P2b/P2c visual gates.
//!
//! All three faces side by side — shader (bound speed knob, chat, drawers),
//! fixture (lamp preview + dominant fader), playlist (entry strip with the
//! active child's card below as a sibling, P2c item 2) — the in-system
//! realization of the spike's v5 board on the flat section grammar with the
//! left-edge label rail (single treatment settled at the P2c re-gate).
//! `control-popovers` shows the anchored contiguous outline: the control's
//! LABEL is the trigger and the whole control merges with the aspect card
//! (P2c item 3, label trigger from the live-review round).

use dioxus::prelude::*;
use lpa_studio_core::{UiAgentStatus, UiSlotFieldState};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{
    bound_source, fader_control, fixture_node_view, knob_control, playlist_node_face_view,
    shader_node_view,
};
use crate::app::node::{NodePane, PanelControl};

#[story(
    description = "The gate board: shader, fixture, and playlist cards with their permanent faces — one flat section grammar (full-bleed sections, hairline dividers, left-edge label rail) across all three; the playlist's active child renders below it as a sibling card."
)]
fn gallery() -> Element {
    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-7xl tw:items-start tw:gap-6 tw:xl:grid-cols-3",
            NodePane {
                view: shader_node_view(true, UiAgentStatus::Idle),
                on_action: move |_| {},
            }
            NodePane {
                view: fixture_node_view(),
                on_action: move |_| {},
            }
            NodePane {
                view: playlist_node_face_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The same board with the shader's code drawer and the fixture's advanced drawer open: growth is downward-only from a stable top."
)]
fn gallery_drawers_open() -> Element {
    // Disclosure is core-owned: stories seed each DTO's card UI state.
    let mut shader = shader_node_view(true, UiAgentStatus::Idle);
    shader.card_ui.code_open = true;
    let mut fixture = fixture_node_view();
    fixture.card_ui.advanced_open = true;
    let mut playlist = playlist_node_face_view();
    playlist.card_ui.advanced_open = true;
    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-7xl tw:items-start tw:gap-6 tw:xl:grid-cols-3",
            NodePane {
                view: shader,
                on_action: move |_| {},
                face_platform: crate::base::Platform::Mac,
            }
            NodePane {
                view: fixture,
                on_action: move |_| {},
            }
            NodePane {
                view: playlist,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Diving into the control (P2c item 3): the LABEL is the trigger and the whole control's outline merges with the identical slot-row aspect card into one contiguous shape via the merged-outline system — bound knob and bound fader."
)]
fn control_popovers() -> Element {
    rsx! {
        div { class: "tw:flex tw:w-full tw:max-w-4xl tw:items-start tw:gap-8 tw:pb-[420px]",
            div { class: "tw:w-56 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
                PanelControl {
                    control: knob_control(
                        "speed",
                        1.6,
                        0.0,
                        4.0,
                        UiSlotFieldState::editable(),
                        bound_source(),
                    ),
                    detail_initially_open: true,
                    on_action: move |_| {},
                }
            }
            div { class: "tw:w-96 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
                PanelControl {
                    control: fader_control(184.0, UiSlotFieldState::editable(), bound_source()),
                    detail_initially_open: true,
                    on_action: move |_| {},
                }
            }
        }
    }
}
