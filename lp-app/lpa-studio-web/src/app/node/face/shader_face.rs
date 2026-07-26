//! The shader card's permanent face: output hero → controls → agent chat.
//!
//! Top-down per the spike (perf line deliberately absent — separate run of
//! work), recast in the flat section grammar (P2b item 1): the produced
//! visual renders full-bleed as the `output` section, the panel controls
//! sit directly on the card in the `controls` section, and the condensed
//! agent chat is the `agent` section — labeled with the sparkles role icon
//! and a plain-language subline so it reads as *an agent that edits this
//! shader* (item 2). The code and advanced drawers render below via
//! [`super::NodeCardDrawers`], never hiding the face.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiShaderFace as UiShaderFaceData};

use crate::app::node::produced_product_view::ProductPreview;
use crate::app::node::{AgentChatPane, NodeCardSection, PanelControl};
use crate::base::StudioIconName;

/// The agent section's plain-language role subline (item 2): active voice,
/// no jargon.
pub(crate) const AGENT_SUBLINE: &str = "edits this shader with you";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ShaderFace(
    face: UiShaderFaceData,
    /// Open this control's corner ⓘ popover on first render (stories).
    #[props(default = None)]
    detail_open_control: Option<String>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();

    rsx! {
        NodeCardSection { label: "output", first: true,
            ProductPreview {
                kind: preview.kind,
                preview: preview.preview.clone(),
                tracking: preview.tracking,
                frame: preview.frame,
                focus_action: None,
                on_action,
            }
        }
        if !face.controls.is_empty() {
            NodeCardSection { label: "controls",
                div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-4 tw:px-4 tw:py-3",
                    for control in face.controls.clone() {
                        PanelControl {
                            key: "{control.label}",
                            detail_initially_open: detail_open_control.as_deref() == Some(control.label.as_str()),
                            control,
                            on_action,
                        }
                    }
                }
            }
        }
        if let Some(agent) = face.agent.clone() {
            NodeCardSection {
                label: "agent",
                icon: Some(StudioIconName::Agent),
                subline: Some(AGENT_SUBLINE),
                AgentChatPane { view: agent, on_action }
            }
        }
    }
}
