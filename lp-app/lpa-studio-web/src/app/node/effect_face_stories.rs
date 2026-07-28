//! Stories for the effect card face (effects-are-projects ADR).
//!
//! The face is the embedded project's output mirror plus its promoted
//! controls — the effect's curated public API, aliasing inner-child slots.
//! Coverage: default (clean/bound-violet/dirty knobs plus one broken-alias
//! disabled control and the provenance line) and the advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::effect_node_view;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EffectCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Effect card: output-mirror hero, promoted knobs (clean, dirty-dot, bound-violet, and one broken-alias disabled control), provenance line; advanced drawer collapsed."
)]
fn default() -> Element {
    rsx! {
        EffectCardCanvas {
            NodePane {
                view: effect_node_view(),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Advanced drawer open: the effect project's generic slot rows (name/provenance) under the face."
)]
fn advanced_open() -> Element {
    let mut view = effect_node_view();
    view.card_ui.advanced_open = true;
    rsx! {
        EffectCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}
