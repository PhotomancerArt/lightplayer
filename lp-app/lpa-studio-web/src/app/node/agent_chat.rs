//! The shader agent chat pane (the `Agent` tab of the editor region).
//!
//! Renders the controller-owned [`UiAgentView`] DTO through the shared
//! agent chat parts (`app::agent`): the transcript (with this agent's
//! inline edit snapshots), the error strip, the edit-history filmstrip
//! (this pane's own), the composer, and the footnote with this pane's
//! export buttons. The chat DRAFT is view-local (editing-model ADR D9,
//! like editor text); everything else lives in core — including the
//! per-provider setup guidance the needs-setup empty state renders. Hosts
//! that unmount this pane while text may be pending (the shader face's
//! collapsible agent section) pass their OWN `draft` signal, owned above
//! the unmount boundary, so collapse never destroys a half-typed draft;
//! without one the pane keeps a local signal (the editor tab, which never
//! unmounts mid-draft).

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiAgentAvailability, UiAgentHistoryEntry, UiAgentView, UiProductPreview,
};

use crate::app::agent::{
    AgentChatFooter, AgentComposer, AgentErrorStrip, AgentNeedsKey, AgentTranscript,
};
use crate::app::node::ProductPreviewCanvas;

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
            AgentNeedsKey {
                title: "Shader agent",
                lede: "Chat with an agent that edits this shader for you.",
                guidance: view.setup,
                on_connect,
                connect_error,
            }
        };
    }

    let local_draft = use_signal(String::new);
    let draft = draft.unwrap_or(local_draft);
    let busy = view.busy();
    let can_send = !busy && source_resolved;
    let send_view = view.clone();
    let stop_view = view.clone();

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            AgentTranscript {
                turns: view.turns.clone(),
                status: view.status.clone(),
                history: view.history.clone(),
                tool_rows_expanded,
                running_label: "Running experiment…",
                on_action,
                empty: rsx! {
                    div { class: "tw:flex tw:flex-col tw:items-center tw:gap-1 tw:px-4 tw:py-8 tw:text-center",
                        p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                            "Describe what the lights should do."
                        }
                        p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground",
                            "The agent edits this shader; changes land as unsaved edits you can Save or revert."
                        }
                    }
                },
            }
            AgentErrorStrip { status: view.status.clone() }
            // Edit-history filmstrip (P4): one thumb chip per staged edit,
            // right above the composer. Clicking restages that edit —
            // confirm-less, it only stages (Save-gated like any agent
            // edit); disabled mid-run (a revert would race the agent).
            if !view.history.is_empty() {
                HistoryStrip { view: view.clone(), busy, on_action }
            }
            AgentComposer {
                draft,
                can_send,
                busy,
                placeholder: if source_resolved { "Ask for a change… (Enter sends, Shift+Enter for a new line)" } else { "Loading shader source…" },
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
                ExportButtons { view: view.clone(), busy, on_action }
            }
        }
    }
}

/// The staged-edit history filmstrip: horizontally scrollable thumb chips
/// (oldest first, newest at the end — reading order matches the
/// transcript), each a one-click revert. A dropped-count label owns the
/// strip's honesty when the retention cap has trimmed old edits.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HistoryStrip(
    view: UiAgentView,
    busy: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:items-end tw:gap-1.5 tw:overflow-x-auto tw:border-t tw:border-border-muted tw:bg-card tw:px-3 tw:py-1.5",
            span { class: "tw:flex-none tw:self-center tw:text-[10px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-dim-foreground",
                "Edits"
            }
            if view.history_dropped > 0 {
                span {
                    class: "tw:flex-none tw:self-center tw:text-[10px] tw:text-dim-foreground",
                    title: "Older edits fell off the session's history cap",
                    "+{view.history_dropped} older"
                }
            }
            for entry in view.history.iter() {
                HistoryChip {
                    key: "{entry.turn}",
                    entry: entry.clone(),
                    action: view.revert_action(entry.turn),
                    busy,
                    on_action,
                }
            }
        }
    }
}

/// One history chip: the 32-px preview thumb (or a numbered placeholder
/// while none landed), the turn number, and the engine-verdict dot.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HistoryChip(
    entry: UiAgentHistoryEntry,
    action: UiAction,
    busy: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let intent = entry.note.as_deref().unwrap_or("this edit");
    let title = if busy {
        format!("Turn {}: {intent} — stop the run to revert", entry.turn)
    } else {
        format!(
            "Revert to turn {}: {intent} (stages the source; Save keeps it)",
            entry.turn
        )
    };
    rsx! {
        button {
            class: history_chip_class(busy),
            r#type: "button",
            disabled: busy,
            title: "{title}",
            onclick: move |_| {
                if let Some(handler) = on_action {
                    handler.call(action.clone());
                }
            },
            div { class: "tw:relative tw:h-8 tw:w-8 tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card",
                match &entry.thumb {
                    Some(UiProductPreview::VisualSrgb8 { width, height, revision, bytes }) => rsx! {
                        ProductPreviewCanvas {
                            width: *width,
                            height: *height,
                            revision: *revision,
                            bytes: bytes.clone(),
                        }
                    },
                    _ => rsx! {
                        span { class: "tw:absolute tw:inset-0 tw:flex tw:items-center tw:justify-center tw:font-mono tw:text-[11px] tw:text-dim-foreground",
                            "{entry.turn}"
                        }
                    },
                }
            }
            span { class: "tw:flex tw:items-center tw:gap-1 tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                span { class: history_dot_class(entry.engine_ok) }
                "{entry.turn}"
            }
        }
    }
}

/// The footer's export affordances: copy the chat as markdown (anytime the
/// transcript is non-empty), and the debug-JSON download. The JSON is
/// core-built from the PARKED session runtime, so the button dispatches
/// [`UiAgentView::export_debug_action`] and the actual download fires when
/// the DTO comes back with a fresh [`lpa_studio_core::UiAgentDebugDump`]
/// `seq` — a re-rendered stale DTO never re-downloads (paint-key pattern,
/// like the preview canvas).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ExportButtons(
    view: UiAgentView,
    busy: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let downloaded_seq = use_hook(|| std::rc::Rc::new(std::cell::Cell::new(0_u64)));
    if let Some(dump) = &view.debug {
        if downloaded_seq.get() < dump.seq {
            downloaded_seq.set(dump.seq);
            crate::app::node::agent_chat_export::download_json(
                &crate::app::node::agent_chat_export::debug_dump_file_name(&view),
                &dump.json,
            );
        }
    }
    let have_log = !view.turns.is_empty();
    let copy_view = view.clone();
    let dump_view = view.clone();
    // The raw transcript is parked only between runs — idle-only.
    let can_dump = have_log && !busy;
    rsx! {
        button {
            class: export_button_class(have_log),
            r#type: "button",
            disabled: !have_log,
            title: "Copy the chat as a markdown log",
            onclick: move |_| {
                crate::app::node::agent_chat_export::copy_to_clipboard(
                    crate::app::node::agent_chat_export::chat_markdown(&copy_view),
                );
            },
            "Copy log"
        }
        button {
            class: export_button_class(can_dump),
            r#type: "button",
            disabled: !can_dump,
            title: if busy { "Debug export is available when the run finishes" } else { "Download the raw model transcript (JSON) for debugging" },
            onclick: move |_| {
                if let Some(handler) = on_action {
                    handler.call(dump_view.export_debug_action());
                }
            },
            "Debug JSON"
        }
    }
}

/// Export button chrome: quiet text buttons, enabled/disabled in place.
fn export_button_class(enabled: bool) -> String {
    let state = if enabled {
        "tw:cursor-pointer tw:border-border-subtle tw:text-muted-foreground tw:hover:border-border-strong tw:hover:bg-card-raised tw:hover:text-strong-foreground"
    } else {
        "tw:cursor-default tw:border-border-subtle tw:text-subtle-foreground tw:opacity-40"
    };
    format!(
        "tw:flex-none tw:rounded-xs tw:border tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-[10px] tw:font-bold tw:transition tw:duration-300 {state}"
    )
}

/// History chip chrome: clickable revert normally, inert while a run is in
/// flight — constant geometry either way.
fn history_chip_class(busy: bool) -> String {
    let state = if busy {
        "tw:cursor-default tw:opacity-50"
    } else {
        "tw:cursor-pointer tw:hover:border-border-strong tw:hover:bg-card-raised"
    };
    format!(
        "tw:flex tw:flex-none tw:flex-col tw:items-center tw:gap-0.5 tw:rounded-xs tw:border tw:border-transparent tw:bg-transparent tw:p-1 tw:transition tw:duration-300 {state}"
    )
}

/// The chip's engine-verdict dot: good/error once resolved, dim while the
/// verdict is still unknown.
fn history_dot_class(engine_ok: Option<bool>) -> &'static str {
    match engine_ok {
        Some(true) => "tw:h-1 tw:w-1 tw:flex-none tw:rounded-full tw:bg-status-good-foreground",
        Some(false) => "tw:h-1 tw:w-1 tw:flex-none tw:rounded-full tw:bg-status-error-foreground",
        None => "tw:h-1 tw:w-1 tw:flex-none tw:rounded-full tw:bg-dim-foreground",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_dot_tracks_the_engine_verdict() {
        assert!(history_dot_class(Some(true)).contains("status-good"));
        assert!(history_dot_class(Some(false)).contains("status-error"));
        assert!(!history_dot_class(None).contains("status-"));
    }

    #[test]
    fn history_chip_keeps_geometry_while_busy() {
        for busy in [true, false] {
            let class = history_chip_class(busy);
            assert!(class.contains("tw:p-1"));
            assert!(class.contains("tw:flex-col"));
        }
        assert!(history_chip_class(true).contains("tw:cursor-default"));
        assert!(history_chip_class(false).contains("tw:cursor-pointer"));
    }
}
