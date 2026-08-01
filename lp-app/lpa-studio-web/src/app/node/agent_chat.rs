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
    AgentProviderGuidance, SettingsCommand, UiAction, UiAgentAvailability, UiAgentHistoryEntry,
    UiAgentModelView, UiAgentStatus, UiAgentToolRow, UiAgentTurn, UiAgentView, UiModelOption,
    UiProductPreview,
};

use crate::app::node::ProductPreviewCanvas;
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
    // run without a trailing assistant bubble gets a thinking row instead —
    // unless a REAL thinking strip is already streaming at the tail.
    let streaming_tail = matches!(view.status, UiAgentStatus::Streaming)
        && matches!(view.turns.last(), Some(UiAgentTurn::Assistant { .. }));
    let thinking_tail = matches!(
        view.turns.last(),
        Some(UiAgentTurn::Thinking { done: false, .. })
    );
    let thinking_row = busy && !streaming_tail && !thinking_tail;
    let turn_count = view.turns.len();
    // Total prompt tokens (fresh + cache writes + cache reads) — the
    // footnote's honest "in" figure now that prompt caching splits usage
    // into disjoint buckets.
    let input_tokens = view.usage.total_input_tokens();

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            // Transcript.
            div {
                class: transcript_class(!view.turns.is_empty()),
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
                            div { key: "{index}", class: "tw:justify-self-end tw:max-w-[85%] tw:whitespace-pre-wrap tw:break-words tw:rounded-md tw:bg-card tw:px-3 tw:py-2 tw:text-sm tw:text-strong-foreground",
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
                            ToolRow {
                                key: "{index}-{row.id}",
                                row: row.clone(),
                                default_open: tool_rows_expanded,
                                history_entry: history_entry_for_row(&view.history, row),
                            }
                        },
                        UiAgentTurn::Thinking { text, done } => rsx! {
                            ThinkingTurn { key: "{index}-thinking", text: text.clone(), done: *done }
                        },
                        UiAgentTurn::Notice { text, level } => rsx! {
                            p { key: "{index}", class: notice_class(*level), "{text}" }
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
            // Edit-history filmstrip (P4): one thumb chip per staged edit,
            // right above the composer. Clicking restages that edit —
            // confirm-less, it only stages (Save-gated like any agent
            // edit); disabled mid-run (a revert would race the agent).
            if !view.history.is_empty() {
                HistoryStrip { view: view.clone(), busy, on_action }
            }
            // Composer. Roomy by default (~3 lines) and growing with the
            // draft up to the cap (`field-sizing: content`; browsers
            // without it keep the fixed min-height plus the manual resize
            // handle) — the gate-feedback "cramped box" fix.
            div { class: "tw:flex tw:min-w-0 tw:items-end tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card tw:px-3 tw:py-2",
                textarea {
                    class: "tw:field-sizing-content tw:min-h-20 tw:max-h-40 tw:min-w-0 tw:flex-1 tw:resize-y tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-subtle tw:px-2.5 tw:py-2 tw:font-sans tw:text-sm tw:text-strong-foreground tw:outline-none tw:focus:border-accent-border",
                    rows: 3,
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
            // Footnote: the session's model chip (left) and the usage
            // figures (right; + estimated cost when rates are known). The
            // "in" figure is TOTAL prompt tokens (fresh + cache writes +
            // cache reads) — post-caching, the raw `input_tokens` bucket is
            // only the uncached remainder and would read absurdly low.
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-t tw:border-border-subtle tw:bg-card tw:px-3 tw:py-1",
                ModelChip { model: view.model.clone(), busy }
                ExportButtons { view: view.clone(), busy, on_action }
                span { class: "tw:min-w-0 tw:flex-1" }
                if !view.usage.is_zero() {
                    p { class: "tw:m-0 tw:flex-none tw:text-right tw:text-[10px] tw:text-dim-foreground",
                        "{input_tokens} in · {view.usage.output_tokens} out tokens this session"
                        if let Some(cost) = view.estimated_cost.as_deref() {
                            span { title: "estimate based on configured rates", " · {cost}" }
                        }
                    }
                }
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

/// One thinking segment: while streaming (`done == false`) a dim strip
/// shows "Thinking…" with the live text (the transcript's sticky-bottom
/// autoscroll keeps the newest line in view); once done it collapses to a
/// one-line "Thought for a bit ▸" expander, following the tool-row
/// expand/collapse grammar. Expanded/collapsed is view-local.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ThinkingTurn(text: String, done: bool) -> Element {
    let mut open = use_signal(|| false);
    let has_text = !text.is_empty();
    if !done {
        // Streaming: collapsed by default — a quiet pulsing one-liner the
        // user can expand to watch the thinking live (gate feedback: the
        // full stream was too loud always-on).
        return rsx! {
            div { class: "tw:min-w-0 tw:max-w-[95%]",
                button {
                    class: "tw:flex tw:cursor-pointer tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:p-0 tw:text-left tw:text-xs tw:text-status-working-foreground",
                    r#type: "button",
                    title: if has_text { "Show the thinking as it streams" } else { "Waiting for the model" },
                    onclick: move |_| open.set(!open()),
                    span { class: "tw:h-1.5 tw:w-1.5 tw:flex-none tw:animate-pulse tw:rounded-full tw:bg-status-working-foreground" }
                    "Thinking…"
                    if has_text {
                        span { class: "tw:flex-none",
                            if open() { "▾" } else { "▸" }
                        }
                    }
                }
                if open() && has_text {
                    p { class: "tw:m-0 tw:mt-1 tw:whitespace-pre-wrap tw:break-words tw:text-xs tw:italic tw:leading-snug tw:text-dim-foreground",
                        "{text}"
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "tw:min-w-0 tw:max-w-[95%]",
            button {
                class: "tw:flex tw:cursor-pointer tw:items-center tw:gap-1.5 tw:border-0 tw:bg-transparent tw:p-0 tw:text-left tw:text-xs tw:text-dim-foreground tw:hover:text-muted-foreground",
                r#type: "button",
                title: if has_text { "Show what the model thought" } else { "The provider kept this thinking private" },
                onclick: move |_| open.set(!open()),
                span { class: "tw:italic", "Thought for a bit" }
                if has_text {
                    span { class: "tw:flex-none",
                        if open() { "▾" } else { "▸" }
                    }
                }
            }
            if open() && has_text {
                p { class: "tw:m-0 tw:mt-1 tw:whitespace-pre-wrap tw:break-words tw:text-xs tw:italic tw:leading-snug tw:text-dim-foreground",
                    "{text}"
                }
            }
        }
    }
}

/// One `iterate` call: a compact one-liner that expands to the summary
/// detail. Collapsed/expanded is view-local (like the tab selection).
/// Staged-edit calls carry their history entry, so the transcript — the
/// history the user actually scrolls — shows the edit's snapshot inline
/// at the row's right edge (the filmstrip stays for one-click reverts):
/// the 32-px thumb once the verdict resolved ok and a preview landed, a
/// dim numbered placeholder until then (constant geometry either way).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolRow(
    row: UiAgentToolRow,
    #[props(default = false)] default_open: bool,
    /// The staged edit's history entry (matched by `edit_turn`); `None`
    /// for non-staging calls and cap-dropped records.
    #[props(default = None)]
    history_entry: Option<UiAgentHistoryEntry>,
) -> Element {
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
                if let Some(entry) = &history_entry {
                    div {
                        class: "tw:relative tw:h-8 tw:w-8 tw:flex-none tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card",
                        title: if entry.thumb.is_some() { "Edit {entry.turn} — how it looked" } else { "Edit {entry.turn} — no snapshot" },
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
                                span { class: "tw:absolute tw:inset-0 tw:flex tw:items-center tw:justify-center tw:font-mono tw:text-[11px] tw:text-dim-foreground tw:opacity-60",
                                    "{entry.turn}"
                                }
                            },
                        }
                    }
                }
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

/// The staged edit's history entry for one tool row, matched by the
/// shared edit ordinal (`None` for non-staging calls and records the
/// retention cap dropped).
fn history_entry_for_row(
    history: &[UiAgentHistoryEntry],
    row: &UiAgentToolRow,
) -> Option<UiAgentHistoryEntry> {
    let turn = row.edit_turn?;
    history.iter().find(|entry| entry.turn == turn).cloned()
}

/// The footnote's compact model selector: shows the session's model
/// without opening settings and switches it in place. Options come from
/// P8's fetched `/models` list (the selector requests a fetch when it
/// opens; the store debounces); a selection dispatches the SAME
/// [`SettingsCommand::SetAgentModel`] mutation the popover's model field
/// uses and applies from the NEXT run (providers rebuild at run start).
/// Custom free-text ids stay a settings-popover affair — a disabled tail
/// option points there. Disabled while a run is in flight (switching
/// mid-run is out of scope); without a configured model the chip is a
/// plain pointer at Settings.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModelChip(model: UiAgentModelView, busy: bool) -> Element {
    // The settings dispatch context (installed by the web shell; absent
    // under stories, which render the chip inert).
    let on_settings = try_consume_context::<Callback<SettingsCommand>>();
    let Some(effective) = model.effective.clone() else {
        return rsx! {
            span { class: "tw:flex-none tw:text-[10px] tw:font-bold tw:text-status-warning-foreground",
                title: "The provider needs a model id — set one in Settings (the gear icon, top right)",
                "model: set in Settings"
            }
        };
    };
    let options = model_chip_options(&model);
    let request_models = move || {
        if let Some(handler) = on_settings {
            handler.call(SettingsCommand::RequestModels { force: false });
        }
    };
    rsx! {
        // The font classes live on this wrapper: the app stylesheet's
        // unlayered `select { font: inherit }` beats layered utilities on
        // the select itself, so the chip's type is set by inheritance.
        span { class: "tw:min-w-0 tw:flex-none tw:font-mono tw:text-[10px]",
            select {
                class: model_chip_class(busy),
                disabled: busy,
                title: if busy { "Model for the next run — wait for this run to finish to switch" } else { "Model for the next run — custom ids in Settings" },
                value: "{effective}",
                // Opening the selector is the fetch trigger
                // (store-debounced); pointer and keyboard opens both land
                // here.
                onpointerdown: move |_| request_models(),
                onfocus: move |_| request_models(),
                onchange: move |event| {
                    let value = event.value();
                    if let Some(handler) = on_settings
                        && value != SETTINGS_MODEL_VALUE
                    {
                        handler.call(SettingsCommand::SetAgentModel(Some(value)));
                    }
                },
                for option in options {
                    option {
                        key: "{option.id}",
                        value: "{option.id}",
                        selected: option.id == effective,
                        "{option.label.as_deref().unwrap_or(&option.id)}"
                    }
                }
                option {
                    value: SETTINGS_MODEL_VALUE,
                    disabled: true,
                    if model.loading { "fetching model list…" } else { "Custom / more — Settings" }
                }
            }
        }
    }
}

/// The sentinel value of the chip's inert Settings-pointer option (never
/// dispatched; the option is disabled and exists as a signpost).
const SETTINGS_MODEL_VALUE: &str = "__settings__";

/// The chip's option list: the fetched models, with the effective id
/// prepended when the list does not carry it (an unlisted override, or no
/// fetch yet — the select must always be able to show the truth).
fn model_chip_options(model: &UiAgentModelView) -> Vec<UiModelOption> {
    let mut options = model.options.clone();
    if let Some(effective) = &model.effective
        && !options.iter().any(|option| &option.id == effective)
    {
        options.insert(
            0,
            UiModelOption {
                id: effective.clone(),
                label: None,
                detail: None,
            },
        );
    }
    options
}

/// Model-chip chrome: an unobtrusive borderless select that reveals its
/// affordance on hover; inert while a run is in flight — constant
/// geometry either way.
fn model_chip_class(busy: bool) -> String {
    let state = if busy {
        "tw:cursor-default tw:opacity-50"
    } else {
        "tw:cursor-pointer tw:hover:border-border-subtle tw:hover:text-muted-foreground"
    };
    format!(
        "tw:min-w-0 tw:max-w-56 tw:flex-none tw:truncate tw:rounded-xs tw:border tw:border-transparent tw:bg-transparent tw:px-1 tw:py-0.5 tw:font-mono tw:text-[10px] tw:text-dim-foreground tw:outline-none tw:transition tw:duration-300 {state}"
    )
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
        "tw:cursor-pointer tw:border-border-subtle tw:text-muted-foreground tw:hover:border-accent-border tw:hover:bg-accent-wash tw:hover:text-accent"
    } else {
        "tw:cursor-default tw:border-border-subtle tw:text-subtle-foreground tw:opacity-40"
    };
    format!(
        "tw:flex-none tw:rounded-xs tw:border tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-[10px] tw:font-bold tw:transition tw:duration-300 {state}"
    )
}

/// Transcript chrome: the chat scrollback owns the DISTINCT (subtle)
/// background — the strips around it share the node card's — and locks to
/// a fixed height once a conversation exists, so streaming text scrolls
/// inside instead of reflowing the card line by line. The empty state
/// stays short.
fn transcript_class(has_turns: bool) -> String {
    let height = if has_turns { "tw:h-80" } else { "tw:min-h-40" };
    format!(
        "tw:grid {height} tw:content-start tw:gap-2 tw:overflow-auto tw:bg-card-subtle tw:px-3 tw:py-3"
    )
}

/// Notice presentation by level: dim italics normally, warning-toned when
/// the run ended incomplete (the reader must not scroll past it).
fn notice_class(level: lpa_studio_core::UiNoticeLevel) -> &'static str {
    match level {
        lpa_studio_core::UiNoticeLevel::Warning => {
            "tw:m-0 tw:text-xs tw:italic tw:text-status-warning-foreground"
        }
        _ => "tw:m-0 tw:text-xs tw:italic tw:text-dim-foreground",
    }
}

/// History chip chrome: clickable revert normally, inert while a run is in
/// flight — constant geometry either way.
fn history_chip_class(busy: bool) -> String {
    let state = if busy {
        "tw:cursor-default tw:opacity-50"
    } else {
        "tw:cursor-pointer tw:hover:border-accent-border tw:hover:bg-accent-wash"
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

    #[test]
    fn tool_rows_adopt_their_history_entry_by_edit_turn() {
        let entry = UiAgentHistoryEntry {
            turn: 2,
            note: Some("warm the palette".into()),
            thumb: None,
            engine_ok: Some(true),
        };
        let history = vec![entry.clone()];

        let mut staged = row();
        staged.edit_turn = Some(2);
        assert_eq!(history_entry_for_row(&history, &staged), Some(entry));

        // A cap-dropped record and a non-staging call both stay bare.
        staged.edit_turn = Some(1);
        assert_eq!(history_entry_for_row(&history, &staged), None);
        assert_eq!(history_entry_for_row(&history, &row()), None);
    }

    #[test]
    fn model_chip_options_include_the_unlisted_current_model() {
        let listed = UiModelOption {
            id: "claude-sonnet-5".into(),
            label: Some("Claude Sonnet 5".into()),
            detail: None,
        };
        let mut model = UiAgentModelView {
            effective: Some("claude-sonnet-5".into()),
            options: vec![listed.clone()],
            loading: false,
        };
        // Listed current model: no duplicate.
        assert_eq!(model_chip_options(&model), vec![listed.clone()]);

        // Unlisted override (or no fetch yet): prepended so the select can
        // show the truth.
        model.effective = Some("my-custom-model".into());
        let options = model_chip_options(&model);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].id, "my-custom-model");
        assert_eq!(options[0].label, None);
        assert_eq!(options[1], listed);
    }

    #[test]
    fn model_chip_keeps_geometry_across_run_states() {
        for busy in [true, false] {
            let class = model_chip_class(busy);
            assert!(class.contains("tw:text-[10px]"));
            assert!(class.contains("tw:transition"));
        }
        assert!(model_chip_class(true).contains("tw:cursor-default"));
        assert!(model_chip_class(false).contains("tw:cursor-pointer"));
    }
}
