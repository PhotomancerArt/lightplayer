//! Stories for the button card face (node-actions P4).
//!
//! The face is one affordance — simulate the transition a finger would
//! make — over the button's hardware identity. Stories are static by
//! definition, so the held state is mounted directly rather than gestured
//! into: the real one is reached by holding past the 300 ms window, and it
//! renews on the wire until pointer-up.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{button_face, button_node_view};
use crate::app::node::{ButtonFace, NodePane};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ButtonCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Button card at rest: the press control — a skeuomorphic momentary button in the knob family — beside the endpoint and message id, over the collapsed settings drawer. A tap sends a click; holding past ~300 ms becomes a real hold that renews until release."
)]
fn default() -> Element {
    rsx! {
        ButtonCardCanvas {
            NodePane {
                view: button_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Held: the pointer stayed down past the window, so the button is sustaining a real press on the runtime (renewed every second, auto-released by the device if this tab stops asking). The cap sits depressed with a live-blue ring — the same family as the playlist's ACTIVE placard, something happening in the runtime right now."
)]
fn held() -> Element {
    rsx! {
        ButtonCardCanvas {
            ButtonFace {
                face: button_face(),
                held_initially: true,
                on_action: move |_| {},
            }
        }
    }
}
