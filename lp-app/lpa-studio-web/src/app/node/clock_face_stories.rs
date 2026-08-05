//! Stories for the clock card face: the published time product plus the
//! read-only phasor listing (parent D10).
//!
//! Coverage is the three states the listing has to be right in, and the
//! middle one is the whole reason these stories exist:
//!
//! - **unread** — no probe has answered yet;
//! - **empty** — the runtime answered and NOTHING is riding this timebase.
//!   A normal state (a project whose shaders declare no phasor has none),
//!   which must not read as a failure;
//! - **rows** — private and shared integrators together, since the visual
//!   difference between them is the listing's one load-bearing fact.

use dioxus::prelude::*;
use lpa_studio_core::UiTimebaseState;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::{clock_face, clock_node_view, phasor_row};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ClockCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Live listing: two integrators on one clock. `plasma`'s is private to its own slot; `bus:speed`'s is SHARED — violet, marked, and retuned for every reader of that channel at once. Each row carries its period, the raw [0,1) phase bar, and the completed-cycle count."
)]
fn default() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(
                    UiTimebaseState::Live,
                    vec![
                        phasor_row(
                            "plasma",
                            "phase",
                            false,
                            0.62,
                            17,
                            20.0,
                        ),
                        phasor_row("bus:speed", "in fyeah_sign", true, 0.18, 3, 100.0),
                        phasor_row(
                            "quad-strips",
                            "phase",
                            false,
                            0.04,
                            0,
                            0.0,
                        ),
                    ],
                )),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Nothing is riding the timebase. A NORMAL state, not a failure: a project whose shaders declare no phasor has none, and so does one whose phasors have all gone idle. The line says exactly that instead of showing an empty box."
)]
fn no_phasors() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Live, Vec::new())),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The card just mounted and no timebase probe has answered yet. Deliberately distinct from the empty listing above — 'no read landed' and 'nothing is running' are different sentences."
)]
fn unread() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Unread, Vec::new())),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The runtime resolves no timebase for this product — the node just left the tree, or has never produced. A structured answer, not an error card."
)]
fn unknown() -> Element {
    rsx! {
        ClockCardCanvas {
            NodePane {
                view: clock_node_view(clock_face(UiTimebaseState::Unknown, Vec::new())),
                on_action: move |_| {},
            }
        }
    }
}
