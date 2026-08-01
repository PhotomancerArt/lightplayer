//! Stories for the shader card face (permanent face + drawers).
//!
//! Full `NodePane` compositions exercising the `face: Some` branch —
//! preview hero, knob row, condensed agent chat, and the code/advanced
//! drawers. Coverage: idle, bound-knob, chat-streaming, code drawer open,
//! advanced drawer open.

use dioxus::prelude::*;
use lpa_studio_core::UiAgentStatus;
use lpa_studio_web_story_macros::story;

use crate::app::node::NodePane;
use crate::app::node::face_story_fixtures::shader_node_view;
use crate::base::Platform;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShaderCardCanvas(children: Element) -> Element {
    rsx! {
        div { class: "tw:w-full tw:max-w-md", {children} }
    }
}

#[story(
    description = "Idle shader card: preview hero, knob row (blue live-edited scale label, mirror toggle), condensed agent chat, collapsed code/advanced drawers."
)]
fn idle() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view(false, UiAgentStatus::Idle),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Bound knob on the face: speed rides a bus binding — violet arc, ring, and name on the knob itself."
)]
fn bound_knob() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view(true, UiAgentStatus::Idle),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Mid-run agent chat on the face: streaming cursor and Stop button in the condensed chat while the preview and knobs stay put."
)]
fn chat_streaming() -> Element {
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view: shader_node_view(true, UiAgentStatus::Streaming),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Code drawer open: the inline GLSL editor expands under the face — opening code never hides the preview or knobs."
)]
fn code_drawer_open() -> Element {
    // Disclosure is core-owned now: stories seed the DTO's card UI state
    // (`NodeCardUiState`), not component props.
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.code_open = true;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
                face_platform: Platform::Mac,
            }
        }
    }
}

#[story(
    description = "Advanced drawer open: today's slot rows (bound speed row included) behind the last lid."
)]
fn advanced_open() -> Element {
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.advanced_open = true;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Agent section collapsed to its summary row: sparkles icon + status-aware summary (turn count and cost estimate); expanding restores the chat with any half-typed draft intact."
)]
fn agent_collapsed() -> Element {
    let mut view = shader_node_view(true, UiAgentStatus::Idle);
    view.card_ui.agent_collapsed = true;
    rsx! {
        ShaderCardCanvas {
            NodePane {
                view,
                on_action: move |_| {},
            }
        }
    }
}
