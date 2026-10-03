//! [`AgentComposer`]: the message box under both agent chats.
//!
//! Enter sends, Shift+Enter newlines, and a Stop button stands in for Send
//! while a run is in flight. The DRAFT is view-local (editing-model ADR D9,
//! like editor text): the host owns the signal, so a host that unmounts the
//! composer (the shader face's collapsible section, the app drawer) keeps a
//! half-typed draft. What a send dispatches is the host's — core's own
//! send action for that chat; this only hands over the trimmed text.

use dioxus::prelude::*;

use crate::core::outline_action_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentComposer(
    draft: Signal<String>,
    /// Whether Send may fire (idle, and whatever else the host needs).
    can_send: bool,
    /// A run is in flight: Stop replaces Send.
    busy: bool,
    placeholder: String,
    /// The trimmed, non-empty draft; the draft clears after.
    on_send: EventHandler<String>,
    on_stop: EventHandler<()>,
) -> Element {
    let mut draft = draft;
    rsx! {
        // Roomy by default (~3 lines) and growing with the draft up to the
        // cap (`field-sizing: content`; browsers without it keep the fixed
        // min-height plus the manual resize handle) — the gate-feedback
        // "cramped box" fix.
        div { class: "tw:flex tw:min-w-0 tw:items-end tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card tw:px-3 tw:py-2",
            textarea {
                class: "tw:field-sizing-content tw:min-h-20 tw:max-h-40 tw:min-w-0 tw:flex-1 tw:resize-y tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-subtle tw:px-2.5 tw:py-2 tw:font-sans tw:text-sm tw:text-strong-foreground tw:outline-none",
                rows: 3,
                placeholder: "{placeholder}",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| {
                    if event.key() == Key::Enter && !event.modifiers().shift() {
                        event.prevent_default();
                        if can_send {
                            send_draft(&mut draft, on_send);
                        }
                    }
                },
            }
            if busy {
                button {
                    class: outline_action_class(true),
                    r#type: "button",
                    title: "Stop the running turn",
                    onclick: move |_| on_stop.call(()),
                    "Stop"
                }
            } else {
                button {
                    class: send_button_class(can_send),
                    r#type: "button",
                    disabled: !can_send,
                    title: "Send (Enter)",
                    onclick: move |_| send_draft(&mut draft, on_send),
                    "Send"
                }
            }
        }
    }
}

/// Hand the draft over (trimmed; an empty draft is ignored) and clear it.
/// Shared by the Enter key and the Send button so they cannot diverge.
pub(crate) fn send_draft(draft: &mut Signal<String>, on_send: EventHandler<String>) {
    let text = draft.peek().trim().to_string();
    if text.is_empty() {
        return;
    }
    on_send.call(text);
    draft.set(String::new());
}

/// Send button chrome: enabled/disabled in place, constant geometry.
pub(crate) fn send_button_class(enabled: bool) -> String {
    let state = if enabled {
        "tw:cursor-pointer tw:border-border-strong tw:text-strong-foreground tw:hover:bg-card-raised"
    } else {
        "tw:cursor-default tw:border-border-subtle tw:text-subtle-foreground tw:opacity-40"
    };
    format!(
        "tw:flex-none tw:rounded-xs tw:border tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:transition tw:duration-300 {state}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_button_keeps_geometry_across_enable_states() {
        for enabled in [true, false] {
            let class = send_button_class(enabled);
            assert!(class.contains("tw:px-3"));
            assert!(class.contains("tw:transition"));
        }
    }
}
