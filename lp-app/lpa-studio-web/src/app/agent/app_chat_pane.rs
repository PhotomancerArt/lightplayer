//! [`AppChatPane`]: the app chat — the one assistant a page holds, which
//! builds and changes the project and hands the user a card when only
//! their click can do it.
//!
//! Renders core's [`UiAppAgentView`] through the same parts the shader
//! agent's tab uses: the transcript (edit rows for `edit_project`, cards
//! through `AgentCardView` as the real control), the error strip, the
//! composer and the footnote with the session's cost. Before a provider is
//! set up it is the not-configured state, which is never a dead end (the
//! OpenRouter Connect button leads).

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAgentAvailability, UiAppAgentView};

use crate::app::agent::{
    AgentChatFooter, AgentComposer, AgentErrorStrip, AgentNeedsKey, AgentTranscript,
};

/// The app chat's name and one-line role, here and in the drawer header.
pub(crate) const APP_CHAT_TITLE: &str = "Assistant";
pub(crate) const APP_CHAT_LEDE: &str =
    "Tell it what you want your lights to do. It builds and changes your project for you.";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AppChatPane(
    view: UiAppAgentView,
    /// The composer draft, owned by the web app (it survives the drawer
    /// closing, and the home page's front door writes it too).
    draft: Signal<String>,
    /// Open tool rows expanded on first render (stories).
    #[props(default = false)]
    tool_rows_expanded: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The OpenRouter Connect CTA for the not-configured state.
    #[props(default)]
    on_connect: Option<EventHandler<()>>,
    #[props(default)] connect_error: Option<String>,
) -> Element {
    if view.availability == UiAgentAvailability::NeedsKey {
        return rsx! {
            div { class: "tw:min-h-0 tw:flex-1 tw:overflow-auto tw:bg-card",
                AgentNeedsKey {
                    title: "Set up the assistant",
                    lede: APP_CHAT_LEDE,
                    guidance: view.setup,
                    on_connect,
                    connect_error,
                }
            }
        };
    }

    let busy = view.busy();
    let send_view = view.clone();
    let stop_view = view.clone();

    rsx! {
        div { class: "tw:flex tw:min-h-0 tw:min-w-0 tw:flex-1 tw:flex-col",
            AgentTranscript {
                turns: view.turns.clone(),
                status: view.status.clone(),
                tool_rows_expanded,
                running_label: "Working…",
                fill: true,
                on_action,
                empty: rsx! {
                    div { class: "tw:flex tw:flex-col tw:items-center tw:gap-1.5 tw:px-4 tw:py-10 tw:text-center",
                        p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                            "What do you want your lights to do?"
                        }
                        p { class: "tw:m-0 tw:max-w-80 tw:text-xs tw:leading-relaxed tw:text-dim-foreground",
                            "Say it in your own words — “a strip of 250 LEDs on my XIAO that cycles a few patterns”. The assistant makes the same edits you could, and when a step needs your click (connecting or flashing a board) it puts the button here for you."
                        }
                    }
                },
            }
            AgentErrorStrip { status: view.status.clone() }
            AgentComposer {
                draft,
                can_send: !busy,
                busy,
                placeholder: "What do you want your lights to do? (Enter sends)",
                on_send: move |text: String| {
                    if let Some(handler) = on_action {
                        handler.call(send_view.send_action(&text));
                    }
                },
                on_stop: move |()| {
                    if let Some(handler) = on_action {
                        handler.call(stop_view.stop_action());
                    }
                },
            }
            AgentChatFooter {
                model: view.model.clone(),
                busy,
                usage: view.usage,
                cost: view.estimated_cost.clone(),
            }
        }
    }
}
