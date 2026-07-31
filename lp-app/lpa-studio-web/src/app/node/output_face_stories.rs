//! Stories for the output card face (node-actions P4).
//!
//! One diagnostic affordance — drive every pixel white instead of the
//! graph — over the lamp preview of what the output is actually being fed.
//! The on state takes the attention family: an override is a temporary
//! abnormal condition, not a fault and not a blessing. Violet stays the
//! bound/bus convention and green stays good/valid.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::{output_face, output_node_view};
use crate::app::node::{NodePane, OutputFace};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OutputCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Output card at rest: the test-pattern toggle beside the endpoint, over the lamp preview of what the graph is feeding this output, over the collapsed settings drawer."
)]
fn default() -> Element {
    rsx! {
        OutputCardCanvas {
            NodePane {
                view: output_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Test pattern on: the output is overridden with full white, re-sent every second while the toggle is on. Attention-coloured because it is an override someone has to remember to undo — and if they forget, the device's ~2 s TTL undoes it for them."
)]
fn pattern_on() -> Element {
    rsx! {
        OutputCardCanvas {
            OutputFace {
                face: output_face(),
                pattern_initially_on: true,
                on_action: move |_| {},
            }
        }
    }
}
