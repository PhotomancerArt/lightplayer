//! [`AgentTranscript`]: the chat scrollback both agent chats draw — the
//! shader agent's tab and the app chat's drawer.
//!
//! User text plain, assistant text through the safe [`MarkdownText`]
//! subset (re-parsed per frame, so streaming rides the same path),
//! thinking strips, notices, cards (through [`AgentCardView`], the real
//! control), and expandable tool rows: the shader agent's experiment rows
//! with their inline edit snapshot, and the app agent's `edit_project`
//! rows with their per-edit list. Sticky-bottom autoscroll, and a working
//! row while a run is in flight without a streaming tail.
//!
//! Status colors follow the standard palette: working (yellow) while
//! streaming or running a tool, error (red) for provider failures and
//! refused edits, good (green) strictly for a valid outcome.

use std::rc::Rc;

use dioxus::prelude::*;
use dioxus::{html::geometry::PixelsVector2D, prelude::dioxus_core::use_after_render};
use lpa_studio_core::{
    ActionEnablement, OfferPath, UiAction, UiAgentEditBatch, UiAgentEditOutcome,
    UiAgentHistoryEntry, UiAgentStatus, UiAgentToolRow, UiAgentTurn, UiNoticeLevel,
    UiProductPreview,
};

use crate::app::agent::AgentCardView;
use crate::app::node::ProductPreviewCanvas;
use crate::base::{MarkdownText, StudioIcon, StudioIconName};
use crate::core::use_offer_at;

/// Scroll slack under which the transcript stays glued to its bottom.
const CHAT_STICKY_THRESHOLD_PX: f64 = 48.0;

/// One chat's scrollback.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentTranscript(
    turns: Vec<UiAgentTurn>,
    status: UiAgentStatus,
    /// The shader agent's staged-edit history: a tool row that staged an
    /// edit shows its snapshot inline. Empty for the app chat.
    #[props(default)]
    history: Vec<UiAgentHistoryEntry>,
    /// Open tool rows expanded on first render (stories).
    #[props(default = false)]
    tool_rows_expanded: bool,
    /// The working row's words while a tool runs ("Running experiment…").
    running_label: &'static str,
    /// Fill the host's height (the app drawer) instead of the shader
    /// card's fixed-height scrollback.
    #[props(default = false)]
    fill: bool,
    /// What an empty conversation shows.
    empty: Element,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut transcript_element = use_signal(|| None::<Rc<MountedData>>);
    let mut stick_to_bottom = use_signal(|| true);
    let busy = matches!(
        status,
        UiAgentStatus::Streaming | UiAgentStatus::RunningTool
    );

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

    // Show the streaming cursor inside the last assistant bubble; a busy
    // run without a trailing assistant bubble gets a thinking row instead —
    // unless a REAL thinking strip is already streaming at the tail.
    let streaming_tail = matches!(status, UiAgentStatus::Streaming)
        && matches!(turns.last(), Some(UiAgentTurn::Assistant { .. }));
    let thinking_tail = matches!(
        turns.last(),
        Some(UiAgentTurn::Thinking { done: false, .. })
    );
    let thinking_row = busy && !streaming_tail && !thinking_tail;
    let turn_count = turns.len();

    rsx! {
        div {
            class: transcript_class(!turns.is_empty(), fill),
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
            if turns.is_empty() {
                {empty}
            }
            for (index, turn) in turns.iter().enumerate() {
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
                            history_entry: history_entry_for_row(&history, row),
                            on_action,
                        }
                    },
                    UiAgentTurn::Thinking { text, done } => rsx! {
                        ThinkingTurn { key: "{index}-thinking", text: text.clone(), done: *done }
                    },
                    UiAgentTurn::Notice { text, level } => rsx! {
                        p { key: "{index}", class: notice_class(*level), "{text}" }
                    },
                    // Cards come from the app agent's `act` (the shader
                    // agent has none); the transcript draws one wherever it
                    // holds one, as the real control.
                    UiAgentTurn::Card(card) => rsx! {
                        AgentCardView {
                            key: "{index}-{card.id}",
                            card: card.clone(),
                            on_action: move |action| {
                                if let Some(handler) = on_action {
                                    handler.call(action);
                                }
                            },
                        }
                    },
                }
            }
            if thinking_row {
                div { class: "tw:flex tw:items-center tw:gap-2 tw:text-xs tw:text-status-working-foreground",
                    span { class: "tw:h-1.5 tw:w-1.5 tw:animate-pulse tw:rounded-full tw:bg-status-working-foreground" }
                    if matches!(status, UiAgentStatus::RunningTool) {
                        "{running_label}"
                    } else {
                        "Thinking…"
                    }
                }
            }
        }
    }
}

/// The error strip under a transcript (retry = just send again).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentErrorStrip(status: UiAgentStatus) -> Element {
    let UiAgentStatus::Error { message, retryable } = status else {
        return rsx! {};
    };
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:border-t tw:border-border-muted tw:bg-status-error-bg tw:px-3 tw:py-1.5 tw:text-xs tw:text-status-error-foreground",
            span { class: "tw:flex-none tw:font-bold", "Agent error" }
            span { class: "tw:min-w-0 tw:truncate", title: "{message}", "{message}" }
            span { class: "tw:flex-none tw:text-status-error-foreground/70",
                if retryable { "Try sending again." } else { "Check your key or model in Settings, then retry." }
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

/// One tool call: a compact one-liner that expands to its detail.
/// Collapsed/expanded is view-local (like the tab selection).
///
/// The shader agent's staged-edit calls carry their history entry, so the
/// transcript — the history the user actually scrolls — shows the edit's
/// snapshot inline at the row's right edge (the filmstrip stays for
/// one-click reverts): the 32-px thumb once the verdict resolved ok and a
/// preview landed, a dim numbered placeholder until then (constant geometry
/// either way). The app agent's `edit_project` calls expand to their list
/// of edits instead of the summary JSON, refused ones with the reason.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToolRow(
    row: UiAgentToolRow,
    #[props(default = false)] default_open: bool,
    /// The staged edit's history entry (matched by `edit_turn`); `None`
    /// for non-staging calls and cap-dropped records.
    #[props(default = None)]
    history_entry: Option<UiAgentHistoryEntry>,
    /// Presses the row's Show links (core's `show/<target>` offers).
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut open = use_signal(|| default_open);
    // Where the press happened (M8): an `act` row's own Show, and an edit
    // row's one Show per node it changed.
    let act_show = row.place.as_ref().and_then(|place| place.show.clone());
    let edit_shows = edit_row_shows(&row);
    let summary = row.summary_line();
    let has_detail = row.edits.is_some() || !row.detail.is_empty();
    let expand_title = if row.edits.is_some() {
        "Show each edit"
    } else if has_detail {
        "Show experiment detail"
    } else {
        "Experiment running"
    };
    rsx! {
        div { class: "tw:min-w-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted",
            div { class: "tw:flex tw:min-w-0 tw:items-center",
            button {
                class: "tw:flex tw:min-w-0 tw:flex-1 tw:cursor-pointer tw:items-center tw:gap-2 tw:border-0 tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-left tw:font-mono tw:text-xs tw:text-muted-foreground",
                r#type: "button",
                title: "{expand_title}",
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
            if let Some(show) = act_show {
                AgentShowLink { show, text: "Show", on_action }
            }
            }
            if !edit_shows.is_empty() {
                div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-x-1 tw:gap-y-0.5 tw:px-1 tw:pb-1",
                    for (show, label) in edit_shows {
                        AgentShowLink {
                            key: "{show}",
                            show,
                            text: format!("Show {label}"),
                            on_action,
                        }
                    }
                }
            }
            if open() && has_detail {
                match &row.edits {
                    Some(edits) => rsx! {
                        EditList { edits: edits.clone(), note: row.note.clone() }
                    },
                    None => rsx! {
                        pre { class: "tw:m-0 tw:max-h-56 tw:overflow-auto tw:border-t tw:border-border-subtle tw:px-2.5 tw:py-2 tw:font-mono tw:text-[11px] tw:leading-snug tw:text-subtle-foreground tw:whitespace-pre-wrap tw:break-words",
                            "{row.detail}"
                        }
                    },
                }
            }
        }
    }
}

/// A Show link on a chat row: the core offer at `show/<target>`, pressed
/// like any other (it focuses a node's card, lights the control and asks
/// the page to scroll to it). Drawn disabled with the reason when the
/// control is on another page; absent when the tree no longer offers it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AgentShowLink(
    show: OfferPath,
    #[props(into)] text: String,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some(offer) = use_offer_at(show)() else {
        return rsx! {};
    };
    let enabled = offer.is_enabled();
    let title = match &offer.action.meta().enablement {
        ActionEnablement::Disabled { reason } => reason.clone(),
        ActionEnablement::Enabled => offer.summary().to_string(),
    };
    let press = offer.action.clone();
    rsx! {
        button {
            class: SHOW_LINK_CLASS,
            r#type: "button",
            disabled: !enabled,
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                if let Some(handler) = on_action {
                    handler.call(press.clone());
                }
            },
            StudioIcon { name: StudioIconName::Show, size: 12 }
            "{text}"
        }
    }
}

/// The Show link: a quiet text link that reads as a link, not a button.
const SHOW_LINK_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1 tw:rounded-xs tw:border-0 tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-[11px] tw:font-semibold tw:text-muted-foreground tw:hover:bg-card-subtle tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:text-dim-foreground tw:disabled:hover:bg-transparent";

/// An edit row's Show links: one per node its edits landed on, in the
/// order they appear, each named by the node (`Show fixture`).
fn edit_row_shows(row: &UiAgentToolRow) -> Vec<(OfferPath, String)> {
    let mut shows: Vec<(OfferPath, String)> = Vec::new();
    for line in row.edits.iter().flat_map(|edits| &edits.lines) {
        let Some(place) = &line.place else {
            continue;
        };
        let Some(show) = &place.show else {
            continue;
        };
        if !shows.iter().any(|(at, _)| at == show) {
            shows.push((show.clone(), place.label.clone()));
        }
    }
    shows
}

/// An `edit_project` row's expanded list: the agent's note, then one line
/// per edit — a check for what landed, a cross and the app's reason for
/// what it refused or never tried — then the save's outcome.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditList(edits: UiAgentEditBatch, note: Option<String>) -> Element {
    rsx! {
        div { class: "tw:grid tw:max-h-72 tw:gap-1 tw:overflow-auto tw:border-t tw:border-border-subtle tw:px-2.5 tw:py-2",
            if let Some(note) = note {
                p { class: "tw:m-0 tw:pb-0.5 tw:text-xs tw:italic tw:text-muted-foreground", "{note}" }
            }
            for (index, line) in edits.lines.iter().enumerate() {
                div { key: "{index}", class: "tw:grid tw:min-w-0 tw:grid-cols-[auto_minmax(0,1fr)] tw:items-baseline tw:gap-x-2",
                    match &line.outcome {
                        UiAgentEditOutcome::Applied { .. } => rsx! {
                            span { class: "tw:font-mono tw:text-[11px] tw:text-status-good-foreground", "✓" }
                        },
                        _ => rsx! {
                            span { class: "tw:font-mono tw:text-[11px] tw:text-status-error-foreground", "✗" }
                        },
                    }
                    span { class: "tw:min-w-0 tw:break-words tw:font-mono tw:text-[11px] tw:leading-snug tw:text-subtle-foreground",
                        "{line.text()}"
                    }
                    match &line.outcome {
                        UiAgentEditOutcome::Rejected { reason } => rsx! {
                            span {}
                            span { class: "tw:min-w-0 tw:break-words tw:text-[11px] tw:leading-snug tw:text-status-error-foreground",
                                "refused: {reason}"
                            }
                        },
                        UiAgentEditOutcome::Skipped { reason } => rsx! {
                            span {}
                            span { class: "tw:min-w-0 tw:break-words tw:text-[11px] tw:leading-snug tw:text-status-warning-foreground",
                                "not tried: {reason}"
                            }
                        },
                        UiAgentEditOutcome::Applied { .. } => rsx! {},
                    }
                }
            }
            if edits.saved {
                p { class: "tw:m-0 tw:pt-0.5 tw:text-[11px] tw:text-dim-foreground", "Saved the project." }
            }
            if let Some(error) = &edits.save_error {
                p { class: "tw:m-0 tw:pt-0.5 tw:text-[11px] tw:text-status-error-foreground",
                    "Not saved: {error}"
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

/// Transcript chrome: the chat scrollback owns the DISTINCT (subtle)
/// background — the strips around it share the host card's. In the shader
/// card it locks to a fixed height once a conversation exists, so streaming
/// text scrolls inside instead of reflowing the card line by line (the
/// empty state stays short); in the app drawer it fills the drawer.
fn transcript_class(has_turns: bool, fill: bool) -> String {
    let height = if fill {
        "tw:min-h-0 tw:flex-1"
    } else if has_turns {
        "tw:h-80"
    } else {
        "tw:min-h-40"
    };
    format!(
        "tw:grid {height} tw:content-start tw:gap-2 tw:overflow-auto tw:bg-card-subtle tw:px-3 tw:py-3"
    )
}

/// Notice presentation by level: dim italics normally, warning-toned when
/// the run ended incomplete (the reader must not scroll past it).
fn notice_class(level: UiNoticeLevel) -> &'static str {
    match level {
        UiNoticeLevel::Warning => "tw:m-0 tw:text-xs tw:italic tw:text-status-warning-foreground",
        _ => "tw:m-0 tw:text-xs tw:italic tw:text-dim-foreground",
    }
}

/// The tool row's status dot: working while running, then error for a
/// failed call, a compile error or a refused edit, good otherwise.
fn tool_dot_class(row: &UiAgentToolRow) -> &'static str {
    if !row.done {
        return "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:animate-pulse tw:bg-status-working-foreground";
    }
    if row.has_problem() {
        return "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-error-foreground";
    }
    "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-good-foreground"
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
    fn a_refused_edit_turns_the_row_dot_red() {
        let mut edits = row().for_tool("edit_project");
        edits.done = true;
        edits.edits = UiAgentEditBatch::from_summary(&serde_json::json!({ "rows": [
            { "edit": "set", "target": "o", "path": "x", "ok": false, "reason": "no" }
        ] }));
        assert!(tool_dot_class(&edits).contains("status-error"));
    }

    #[test]
    fn an_edit_row_links_each_node_it_changed_once() {
        let mut edits = row().for_tool("edit_project");
        edits.done = true;
        edits.edits = UiAgentEditBatch::from_summary(&serde_json::json!({ "rows": [
            { "edit": "set", "target": "fixture", "path": "a", "ok": true },
            { "edit": "set", "target": "fixture", "path": "b", "ok": true },
            { "edit": "set", "target": "output", "path": "c", "ok": true },
            { "edit": "set_target", "target": "board", "ok": true }
        ] }));
        let place = |node: &str| lpa_studio_core::UiAgentPlace {
            label: node.to_string(),
            place: format!("on the {node} card"),
            show: Some(OfferPath::parse(&format!("show/project/{node}.x")).unwrap()),
        };
        let lines = &mut edits.edits.as_mut().unwrap().lines;
        lines[0].place = Some(place("fixture"));
        lines[1].place = Some(place("fixture"));
        lines[2].place = Some(place("output"));
        let shows: Vec<String> = edit_row_shows(&edits)
            .into_iter()
            .map(|(_, label)| label)
            .collect();
        assert_eq!(shows, ["fixture", "output"]);
    }

    #[test]
    fn near_bottom_threshold_is_forgiving() {
        assert!(is_near_bottom(952.0, 1400, 400));
        assert!(!is_near_bottom(0.0, 1400, 400));
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
    fn the_drawer_transcript_fills_and_the_card_one_locks_its_height() {
        assert!(transcript_class(true, true).contains("tw:flex-1"));
        assert!(transcript_class(true, false).contains("tw:h-80"));
        assert!(transcript_class(false, false).contains("tw:min-h-40"));
    }
}
