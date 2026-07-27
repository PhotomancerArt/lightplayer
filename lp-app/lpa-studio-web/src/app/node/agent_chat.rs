//! The shader agent chat pane (the `Agent` tab of the editor region).
//!
//! Renders the controller-owned [`UiAgentView`] DTO: transcript (user text
//! plain, assistant text through the safe [`MarkdownText`] subset —
//! re-parsed per frame, so streaming rides the same path), expandable tool
//! rows, a streaming indicator, the composer (Enter sends, Shift+Enter
//! newlines), a Stop button while a run is in flight, and the usage
//! footnote with its optional ~$ estimate. The chat DRAFT is view-local
//! (editing-model ADR D9, like editor text); everything else lives in
//! core — including the per-provider setup guidance the needs-setup empty
//! state renders. Hosts that unmount this pane while text may be pending
//! (the shader face's collapsible agent section) pass their OWN
//! `draft` signal, owned above the unmount boundary, so collapse never
//! destroys a half-typed draft; without one the pane keeps a local
//! signal (the editor tab, which never unmounts mid-draft).
//!
//! Status colors follow the standard palette: working (yellow) while
//! streaming or running a tool, error (red) for provider failures, good
//! (green) strictly for a valid compile.

use std::rc::Rc;

use dioxus::prelude::*;
use dioxus::{html::geometry::PixelsVector2D, prelude::dioxus_core::use_after_render};
use lpa_studio_core::{
    AgentProviderGuidance, UiAction, UiAgentAvailability, UiAgentStatus, UiAgentToolRow,
    UiAgentTurn, UiAgentView,
};

use crate::base::MarkdownText;

/// Scroll slack under which the transcript stays glued to its bottom.
const CHAT_STICKY_THRESHOLD_PX: f64 = 48.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AgentChatPane(
    view: UiAgentView,
    /// Whether the shader source is resolved (sending needs it; the tab
    /// wrapper dispatches the fetch).
    #[props(default = true)]
    source_resolved: bool,
    /// Open tool rows expanded on first render (stories).
    #[props(default = false)]
    tool_rows_expanded: bool,
    /// Parent-owned composer draft signal (see the module doc). `None`
    /// keeps a pane-local draft.
    #[props(default = None)]
    draft: Option<Signal<String>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The OpenRouter Connect CTA (the funnel-critical path): present ⇒ the
    /// needs-setup empty state leads with the one-click connect button.
    #[props(default)]
    on_connect: Option<EventHandler<()>>,
    /// Transient connect-flow failure to surface in the empty state.
    #[props(default)]
    connect_error: Option<String>,
) -> Element {
    if view.availability == UiAgentAvailability::NeedsKey {
        return rsx! {
            NeedsKeyState {
                guidance: view.setup,
                on_connect,
                connect_error,
            }
        };
    }

    let local_draft = use_signal(String::new);
    let mut draft = draft.unwrap_or(local_draft);
    let mut transcript_element = use_signal(|| None::<Rc<MountedData>>);
    let mut stick_to_bottom = use_signal(|| true);
    let busy = view.busy();
    let can_send = !busy && source_resolved;

    // Sticky-bottom autoscroll, same pattern as the console's LogList.
    use_after_render(move || {
        if !stick_to_bottom() {
            return;
        }
        let Some(element) = transcript_element.read().as_ref().cloned() else {
            return;
        };
        spawn(async move {
            let Ok(scroll_size) = element.get_scroll_size().await else {
                return;
            };
            let coordinates = PixelsVector2D::new(0.0, scroll_size.height);
            let _ = element.scroll(coordinates, ScrollBehavior::Instant).await;
        });
    });

    let key_send_view = view.clone();
    let click_send_view = view.clone();
    let stop_view = view.clone();
    let on_stop = move |_| {
        if let Some(handler) = on_action {
            handler.call(stop_view.stop_action());
        }
    };

    // Show the streaming cursor inside the last assistant bubble; a busy
    // run without a trailing assistant bubble gets a thinking row instead.
    let streaming_tail = matches!(view.status, UiAgentStatus::Streaming)
        && matches!(view.turns.last(), Some(UiAgentTurn::Assistant { .. }));
    let thinking_row = busy && !streaming_tail;
    let turn_count = view.turns.len();

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            // Transcript.
            div {
                class: "tw:grid tw:max-h-80 tw:min-h-40 tw:content-start tw:gap-2 tw:overflow-auto tw:bg-card tw:px-3 tw:py-3",
                onmounted: move |event| {
                    transcript_element.set(Some(event.data()));
                },
                onscroll: move |event| {
                    stick_to_bottom.set(is_near_bottom(
                        event.scroll_top(),
                        event.scroll_height(),
                        event.client_height(),
                    ));
                },
                if view.turns.is_empty() {
                    div { class: "tw:flex tw:flex-col tw:items-center tw:gap-1 tw:px-4 tw:py-8 tw:text-center",
                        p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                            "Describe what the lights should do."
                        }
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground",
                            "The agent edits this shader; changes land as unsaved edits you can Save or revert."
                        }
                    }
                }
                for (index, turn) in view.turns.iter().enumerate() {
                    match turn {
                        UiAgentTurn::User { text } => rsx! {
                            div { key: "{index}", class: "tw:justify-self-end tw:max-w-[85%] tw:whitespace-pre-wrap tw:break-words tw:rounded-md tw:bg-card-subtle tw:px-3 tw:py-2 tw:text-sm tw:text-strong-foreground",
                                "{text}"
                            }
                        },
                        UiAgentTurn::Assistant { text } => rsx! {
                            div { key: "{index}", class: "tw:max-w-[95%] tw:break-words tw:text-sm tw:leading-relaxed tw:text-muted-foreground",
                                MarkdownText { text: text.clone() }
                                if streaming_tail && index + 1 == turn_count {
                                    span { class: "tw:ml-0.5 tw:inline-block tw:h-3.5 tw:w-1.5 tw:animate-pulse tw:bg-status-working-foreground tw:align-middle" }
                                }
                            }
                        },
                        UiAgentTurn::Tool(row) => rsx! {
                            ToolRow { key: "{index}-{row.id}", row: row.clone(), default_open: tool_rows_expanded }
                        },
                        UiAgentTurn::Notice { text } => rsx! {
                            p { key: "{index}", class: "tw:m-0 tw:text-xs tw:italic tw:text-dim-foreground", "{text}" }
                        },
                    }
                }
                if thinking_row {
                    div { class: "tw:flex tw:items-center tw:gap-2 tw:text-xs tw:text-status-working-foreground",
                        span { class: "tw:h-1.5 tw:w-1.5 tw:animate-pulse tw:rounded-full tw:bg-status-working-foreground" }
                        if matches!(view.status, UiAgentStatus::RunningTool) {
                            "Running experiment…"
                        } else {
                            "Thinking…"
                        }
                    }
                }
            }
            // Error strip (retry = just send again).
            if let UiAgentStatus::Error { message, retryable } = &view.status {
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-t tw:border-border-muted tw:bg-status-error-bg tw:px-3 tw:py-1.5 tw:text-xs tw:text-status-error-foreground",
                    span { class: "tw:flex-none tw:font-bold", "Agent error" }
                    span { class: "tw:min-w-0 tw:truncate", title: "{message}", "{message}" }
                    span { class: "tw:flex-none tw:text-status-error-foreground/70",
                        if *retryable { "Try sending again." } else { "Check your key or model in Settings, then retry." }
                    }
                }
            }
            // Composer.
            div { class: "tw:flex tw:items-end tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card-subtle tw:px-3 tw:py-2",
                textarea {
                    class: "tw:min-h-9 tw:max-h-40 tw:flex-1 tw:resize-y tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card tw:px-2.5 tw:py-2 tw:font-sans tw:text-sm tw:text-strong-foreground tw:outline-none tw:focus:border-accent-border",
                    rows: 1,
                    placeholder: if source_resolved { "Ask for a change… (Enter sends, Shift+Enter for a new line)" } else { "Loading shader source…" },
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter && !event.modifiers().shift() {
                            event.prevent_default();
                            if can_send {
                                dispatch_send(&key_send_view, on_action, &mut draft);
                            }
                        }
                    },
                }
                if busy {
                    button {
                        class: "tw:flex-none tw:cursor-pointer tw:rounded-xs tw:border tw:border-status-error-border tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-status-error-foreground tw:hover:bg-status-error-bg",
                        r#type: "button",
                        title: "Stop the running turn",
                        onclick: on_stop,
                        "Stop"
                    }
                } else {
                    button {
                        class: send_button_class(can_send),
                        r#type: "button",
                        disabled: !can_send,
                        title: "Send (Enter)",
                        onclick: move |_| dispatch_send(&click_send_view, on_action, &mut draft),
                        "Send"
                    }
                }
            }
            // Usage footnote (+ estimated cost when rates are known).
            if view.usage.input_tokens > 0 || view.usage.output_tokens > 0 {
                p { class: "tw:m-0 tw:border-t tw:border-border-subtle tw:bg-card-subtle tw:px-3 tw:py-1 tw:text-right tw:text-[10px] tw:text-dim-foreground",
                    "{view.usage.input_tokens} in · {view.usage.output_tokens} out tokens this session"
                    if let Some(cost) = view.estimated_cost.as_deref() {
                        span { title: "estimate based on configured rates", " · {cost}" }
                    }
                }
            }
        }
    }
}

/// One `iterate` call: a compact one-liner that expands to the summary
/// detail. Collapsed/expanded is view-local (like the tab selection).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolRow(row: UiAgentToolRow, #[props(default = false)] default_open: bool) -> Element {
    let mut open = use_signal(|| default_open);
    let summary = row.summary_line();
    let has_detail = !row.detail.is_empty();
    rsx! {
        div { class: "tw:min-w-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted",
            button {
                class: "tw:flex tw:w-full tw:cursor-pointer tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-left tw:font-mono tw:text-xs tw:text-muted-foreground",
                r#type: "button",
                title: if has_detail { "Show experiment detail" } else { "Experiment running" },
                onclick: move |_| open.set(!open()),
                span { class: tool_dot_class(&row) }
                span { class: "tw:min-w-0 tw:flex-1 tw:truncate", "{summary}" }
                if has_detail {
                    span { class: "tw:flex-none tw:text-dim-foreground",
                        if open() { "▾" } else { "▸" }
                    }
                }
            }
            if open() && has_detail {
                pre { class: "tw:m-0 tw:max-h-56 tw:overflow-auto tw:border-t tw:border-border-subtle tw:px-2.5 tw:py-2 tw:font-mono tw:text-[11px] tw:leading-snug tw:text-subtle-foreground tw:whitespace-pre-wrap tw:break-words",
                    "{row.detail}"
                }
            }
        }
    }
}

/// Send the composer draft (trimmed; empty drafts are ignored) and clear
/// it. Shared by the Enter key and the Send button so they cannot diverge.
fn dispatch_send(
    view: &UiAgentView,
    on_action: Option<EventHandler<UiAction>>,
    draft: &mut Signal<String>,
) {
    let text = draft.peek().trim().to_string();
    if text.is_empty() {
        return;
    }
    if let Some(handler) = on_action {
        handler.call(view.send_action(&text));
    }
    draft.set(String::new());
}

/// The needs-setup empty state: the one-click OpenRouter Connect CTA
/// (success switches the provider, so it leads regardless of the current
/// selection), then the selected provider's onboarding guidance
/// (core-supplied copy, same source as the settings popover) plus the
/// pointer at the settings gear.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn NeedsKeyState(
    guidance: Option<AgentProviderGuidance>,
    #[props(default)] on_connect: Option<EventHandler<()>>,
    #[props(default)] connect_error: Option<String>,
) -> Element {
    let provider_label = guidance.map(|g| g.label).unwrap_or("a provider");
    rsx! {
        div { class: "tw:flex tw:flex-col tw:items-center tw:gap-1.5 tw:bg-card tw:px-6 tw:py-10 tw:text-center",
            p { class: "tw:m-0 tw:text-sm tw:font-bold tw:text-strong-foreground", "Shader agent" }
            p { class: "tw:m-0 tw:max-w-96 tw:text-sm tw:text-muted-foreground",
                "Chat with an agent that edits this shader for you."
            }
            if let Some(on_connect) = on_connect {
                button {
                    class: "tw:mt-2 tw:flex-none tw:cursor-pointer tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:text-accent tw:transition tw:duration-300 tw:hover:bg-accent-wash",
                    r#type: "button",
                    title: "Sign in on openrouter.ai and come right back — no key to paste",
                    onclick: move |_| on_connect.call(()),
                    "Connect OpenRouter — use your own account"
                }
                p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:text-dim-foreground",
                    "Pay-as-you-go from your OpenRouter credits; every major model."
                }
            }
            if let Some(error) = connect_error.as_deref() {
                p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:font-bold tw:text-status-warning-foreground",
                    "{error}"
                }
            }
            p { class: "tw:m-0 tw:mt-2 tw:max-w-96 tw:text-sm tw:text-muted-foreground",
                "Or finish setting up {provider_label} in Settings (the gear icon, top right)."
            }
            if let Some(guidance) = guidance {
                p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:text-muted-foreground", "{guidance.setup}" }
                if let Some(note) = guidance.note {
                    p { class: "tw:m-0 tw:max-w-96 tw:text-xs tw:text-dim-foreground", "{note}" }
                }
                div { class: "tw:flex tw:flex-wrap tw:justify-center tw:gap-x-3",
                    for (label , url) in guidance.links {
                        a {
                            key: "{url}",
                            class: "tw:text-xs tw:text-accent tw:underline",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "{label}"
                        }
                    }
                }
            }
            p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground",
                "Keys are stored in this browser and sent only to the provider you configure."
            }
        }
    }
}

/// The tool row's status dot: working while running, good (valid compile)
/// or error once done.
fn tool_dot_class(row: &UiAgentToolRow) -> &'static str {
    if !row.done {
        return "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:animate-pulse tw:bg-status-working-foreground";
    }
    if row.error.is_some() || row.shader_ok == Some(false) {
        return "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-error-foreground";
    }
    "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-good-foreground"
}

/// Send button chrome: enabled/disabled in place, constant geometry.
fn send_button_class(enabled: bool) -> String {
    let state = if enabled {
        "tw:cursor-pointer tw:border-accent-border tw:text-accent tw:hover:bg-accent-wash"
    } else {
        "tw:cursor-default tw:border-border-subtle tw:text-subtle-foreground tw:opacity-40"
    };
    format!(
        "tw:flex-none tw:rounded-xs tw:border tw:bg-transparent tw:px-3 tw:py-1.5 tw:text-xs tw:font-bold tw:transition tw:duration-300 {state}"
    )
}

fn is_near_bottom(scroll_top: f64, scroll_height: i32, client_height: i32) -> bool {
    f64::from(scroll_height) - scroll_top - f64::from(client_height) <= CHAT_STICKY_THRESHOLD_PX
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> UiAgentToolRow {
        UiAgentToolRow::started("tu_1")
    }

    #[test]
    fn tool_dot_tracks_running_error_and_valid_states() {
        let running = row();
        assert!(tool_dot_class(&running).contains("status-working"));

        let mut ok = row();
        ok.done = true;
        ok.shader_ok = Some(true);
        assert!(tool_dot_class(&ok).contains("status-good"));

        let mut failed = row();
        failed.done = true;
        failed.shader_ok = Some(false);
        assert!(tool_dot_class(&failed).contains("status-error"));

        let mut errored = row();
        errored.done = true;
        errored.error = Some("host refused".into());
        assert!(tool_dot_class(&errored).contains("status-error"));
    }

    #[test]
    fn send_button_keeps_geometry_across_enable_states() {
        for enabled in [true, false] {
            let class = send_button_class(enabled);
            assert!(class.contains("tw:px-3"));
            assert!(class.contains("tw:transition"));
        }
    }

    #[test]
    fn near_bottom_threshold_is_forgiving() {
        assert!(is_near_bottom(952.0, 1400, 400));
        assert!(!is_near_bottom(0.0, 1400, 400));
    }
}
