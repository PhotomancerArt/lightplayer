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
//!
//! The hero carries a slim product header —
//! [`ProductIdentity`](crate::app::node::ProductIdentity) plus the detail
//! popover — because a full-bleed hero replaces the boxed
//! [`ProducedProductView`](crate::app::node::ProducedProductView) pane, and
//! the pane is what used to say what the product is and where its output
//! goes (2026-08-05: "we don't show the output detail / link status
//! either"). Same chrome as the fixture face, which merges it into the
//! toggle bar it already had.
//!
//! The agent section is COLLAPSIBLE on core-owned state
//! (`NodeCardUiState.agent_collapsed`); its collapsed row shows a
//! status-aware summary ([`agent_section_summary`]). The composer DRAFT
//! survives the collapse: the collapsed branch unmounts
//! [`AgentChatPane`], so the draft signal lives HERE — above the unmount
//! boundary — seeded from the core-mirrored draft on mount and mirrored
//! back (`NodeUiOp::SetDraft`) whenever the section collapses. Typing
//! itself stays view-local: per-keystroke ops through the actor would
//! rebuild the whole editor DTO per character (see the
//! `NodeCardUiState` module doc).

use dioxus::prelude::*;
use lpa_studio_core::{NodeUiOp, UiAction, UiAgentView, UiShaderFace as UiShaderFaceData};

use crate::app::node::{
    AgentChatPane, NodeCardSection, PanelControl, ProductIdentity, ProductPreview, SlotDetailButton,
};
use crate::base::StudioIconName;

use super::node_ui_action;

/// The agent section's plain-language role subline (item 2): active voice,
/// no jargon.
pub(crate) const AGENT_SUBLINE: &str = "describe a change in plain language";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ShaderFace(
    face: UiShaderFaceData,
    /// The node's address path — the card UI state key the collapse and
    /// draft-mirror ops carry.
    node: String,
    /// Whether the agent section is collapsed to its summary row
    /// (core-owned state).
    #[props(default = false)]
    agent_collapsed: bool,
    /// The core-mirrored composer draft: seeds the face-owned draft
    /// signal on mount (restore-on-remount; see the module doc).
    #[props(default)]
    composer_draft: String,
    /// Open this control's label-trigger detail popover on first render
    /// (stories).
    #[props(default = None)]
    detail_open_control: Option<String>,
    /// Open the produced output's header detail popover on first render
    /// (stories).
    #[props(default = false)]
    output_detail_initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();
    // The OpenRouter connect wiring, #142 parity with `ShaderEditorTabs`:
    // App-provided context (absent under stories, which render the
    // needs-setup CTA inert) so the agent section's empty state leads with
    // the one-click Connect button (P6 item 5).
    let openrouter_error = try_consume_context::<Signal<Option<String>>>();
    // The composer draft, owned ABOVE the collapse boundary so the
    // collapsed branch's unmount of `AgentChatPane` cannot destroy it.
    let draft = use_signal(move || composer_draft);
    let toggle_node = node.clone();
    let on_toggle_agent = move |()| {
        let Some(handler) = on_action else {
            return;
        };
        // The shared choreography (mirror-then-flip on collapse, flip
        // alone on expand) lives in core so the face e2e drives the exact
        // same op sequence — see `NodeUiOp::toggle_agent_section`.
        for op in NodeUiOp::toggle_agent_section(&toggle_node, agent_collapsed, &draft.peek()) {
            handler.call(node_ui_action(op));
        }
    };

    rsx! {
        NodeCardSection { label: "output", first: true,
            // The visual's own chrome above the full-bleed hero: name,
            // publish chip when the render is wired to a bus channel, and
            // the detail popover at the far end. The hero replaced the
            // boxed product pane, which is what used to carry all three —
            // the fixture face lost the same chrome the same way (its
            // header rides its toggle bar; there is no bar here, so the
            // clock's slim row is the shape).
            div { class: "ux-product-header",
                ProductIdentity { product: preview.clone() }
                span { class: "tw:ml-auto tw:inline-flex tw:flex-none tw:items-center tw:gap-1",
                    SlotDetailButton {
                        label: preview.name.clone(),
                        aspects: preview.visible_aspects(),
                        initially_open: output_detail_initially_open,
                        on_action,
                        authoring: preview.authoring.clone(),
                    }
                }
            }
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
            NodeCardSection { label: "settings",
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
                summary: Some(agent_section_summary(&agent)),
                open: Some(!agent_collapsed),
                on_toggle: on_toggle_agent,
                AgentChatPane {
                    view: agent,
                    draft: Some(draft),
                    on_action,
                    on_connect: move |_| crate::openrouter_oauth::begin_connect(openrouter_error),
                    connect_error: openrouter_error.and_then(|error| error()),
                }
            }
        }
    }
}

/// The collapsed agent row's status-aware summary: what the agent is up
/// to ("running…"), or what the session amounts to so far ("3 turns ·
/// ~$0.02"), or "idle" before any conversation.
fn agent_section_summary(agent: &UiAgentView) -> String {
    if agent.busy() {
        return "running…".to_string();
    }
    let turns = agent.turns.len();
    if turns == 0 {
        return "idle".to_string();
    }
    let noun = if turns == 1 { "turn" } else { "turns" };
    match agent.estimated_cost.as_deref() {
        Some(cost) => format!("{turns} {noun} · {cost}"),
        None => format!("{turns} {noun}"),
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        ArtifactLocation, UiAgentAvailability, UiAgentStatus, UiAgentTurn, UiAgentView,
    };

    use super::agent_section_summary;

    fn view(status: UiAgentStatus) -> UiAgentView {
        let mut view = UiAgentView::empty(
            ArtifactLocation::file("/aurora.glsl"),
            UiAgentAvailability::Ready,
        );
        view.status = status;
        view
    }

    #[test]
    fn summary_reads_idle_before_any_conversation() {
        assert_eq!(agent_section_summary(&view(UiAgentStatus::Idle)), "idle");
    }

    #[test]
    fn summary_reads_running_while_a_turn_is_in_flight() {
        assert_eq!(
            agent_section_summary(&view(UiAgentStatus::Streaming)),
            "running…"
        );
        assert_eq!(
            agent_section_summary(&view(UiAgentStatus::RunningTool)),
            "running…"
        );
    }

    #[test]
    fn summary_counts_turns_and_appends_the_cost_estimate() {
        let mut idle = view(UiAgentStatus::Idle);
        idle.turns = vec![UiAgentTurn::User {
            text: "make it pulse".into(),
        }];
        assert_eq!(agent_section_summary(&idle), "1 turn");

        idle.turns.push(UiAgentTurn::Assistant {
            text: "Done.".into(),
        });
        idle.estimated_cost = Some("~$0.0162".into());
        assert_eq!(agent_section_summary(&idle), "2 turns · ~$0.0162");
    }
}
