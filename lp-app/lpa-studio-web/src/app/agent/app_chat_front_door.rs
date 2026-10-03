//! [`AppChatFrontDoor`]: the home page's big box — "What do you want your
//! lights to do?" — the app chat's front door when no project is open
//! (plan A3).
//!
//! It is the same session as the header's drawer, not a second chat: a
//! send dispatches core's app-chat send and opens the drawer, where the
//! conversation goes on. Before a provider is set up, a send keeps the
//! text in the shared draft and opens the drawer on its not-configured
//! state, so the request is still there once the user has connected —
//! never a dead end.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAgentAvailability, UiAppAgentView};

use super::agent_composer::{send_button_class, send_draft};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AppChatFrontDoor(
    view: UiAppAgentView,
    open: Signal<bool>,
    draft: Signal<String>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut open = open;
    let mut draft = draft;
    let ready = view.availability == UiAgentAvailability::Ready;
    let busy = view.busy();
    let has_text = !draft.read().trim().is_empty();
    let can_send = !busy && has_text;
    let conversation = !view.turns.is_empty();
    let send_view = view.clone();
    let on_send = EventHandler::new(move |text: String| {
        if let Some(handler) = on_action {
            handler.call(send_view.send_action(&text));
        }
        open.set(true);
    });
    // Before setup the draft stays put: the drawer opens on the setup
    // state and the composer there shows the same text afterwards.
    let submit = move || {
        if ready {
            send_draft(&mut draft, on_send);
        } else {
            open.set(true);
        }
    };
    let mut key_submit = submit;
    let mut click_submit = submit;

    rsx! {
        section { class: "tw:grid tw:w-[min(640px,100%)] tw:gap-2 tw:text-left",
            label {
                class: "tw:text-center tw:text-[17px] tw:font-semibold tw:text-strong-foreground",
                r#for: "app-chat-front-door",
                "What do you want your lights to do?"
            }
            div { class: "tw:flex tw:min-w-0 tw:items-end tw:gap-2 tw:rounded-md tw:border tw:border-border-strong tw:bg-card tw:p-2 tw:shadow-lg",
                textarea {
                    id: "app-chat-front-door",
                    class: "tw:field-sizing-content tw:min-h-16 tw:max-h-48 tw:min-w-0 tw:flex-1 tw:resize-none tw:border-0 tw:bg-transparent tw:px-2 tw:py-1.5 tw:font-sans tw:text-[15px] tw:text-strong-foreground tw:outline-none",
                    rows: 2,
                    placeholder: "A strip of 250 LEDs on my XIAO that cycles a few calm patterns…",
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter && !event.modifiers().shift() {
                            event.prevent_default();
                            if can_send {
                                key_submit();
                            }
                        }
                    },
                }
                button {
                    class: send_button_class(can_send),
                    r#type: "button",
                    disabled: !can_send,
                    title: if ready { "Ask the assistant (Enter)" } else { "Set up the assistant, then ask" },
                    onclick: move |_| click_submit(),
                    "Ask"
                }
            }
            p { class: "tw:m-0 tw:text-center tw:text-xs tw:text-dim-foreground",
                if ready {
                    "The assistant builds the project and asks you to click when a board needs you."
                } else {
                    "You'll connect an AI provider first — your words stay here."
                }
                if conversation {
                    " "
                    button {
                        class: "tw:cursor-pointer tw:border-0 tw:bg-transparent tw:p-0 tw:text-xs tw:text-muted-foreground tw:underline tw:hover:text-strong-foreground",
                        r#type: "button",
                        onclick: move |_| open.set(true),
                        "Open the conversation"
                    }
                }
            }
        }
    }
}
