//! Stories for the fixture card face.
//!
//! The face is the thing being lit (LED sample-point preview) plus one
//! dominant horizontal brightness fader. Coverage: default and the
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::fixture_node_view;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Fixture card: ring lamp preview (what the LEDs receive) with the dominant brightness fader below; advanced drawer collapsed."
)]
fn default() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Advanced drawer open: mapping/input/driver/channel slot rows (bound input row included) under the face."
)]
fn advanced_open() -> Element {
    rsx! {
        FixtureCardCanvas {
            NodePane {
                view: fixture_node_view(),
                on_action: move |_| {},
                face_advanced_drawer_open: true,
            }
        }
    }
}
