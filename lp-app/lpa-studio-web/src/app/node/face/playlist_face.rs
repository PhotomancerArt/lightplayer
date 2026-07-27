//! The playlist card's permanent face: the ENTRIES strip.
//!
//! Children belong OUTSIDE the node body (P2c item 2 — the current child
//! pattern is cleaner): the face is the strip section only, and the
//! currently active child's card renders BELOW the playlist card as a
//! sibling, exactly the way extracted children render under any node
//! ([`crate::app::node::NodeChildren`]). The playing entry's thumbnail is
//! replaced by the "ACTIVE" placard (live-blue family — Yona Q5: "ACTIVE",
//! matching `PlaylistState.active_entry` naming). Entries carry per-entry
//! duration chips and a cue tag (⚑) when trigger-driven.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiPlaylistEntry, UiPlaylistFace as UiPlaylistFaceData, UiProductKind,
    UiProductPreviewFrame, UiProductTrackingState,
};

use crate::app::node::NodeCardSection;
use crate::app::node::produced_product_view::ProductPreview;
use crate::base::{StudioIcon, StudioIconName};

/// Thumbnail aspect frame for strip entries (wide, like the spike's
/// 108 × 60 thumbs).
const STRIP_THUMB_FRAME: UiProductPreviewFrame = UiProductPreviewFrame::new(9, 5);

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PlaylistFace(
    face: UiPlaylistFaceData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if face.entries.is_empty() {
        return rsx! {
            NodeCardSection { label: "entries", first: true,
                p { class: "tw:m-0 tw:px-4 tw:py-3 tw:text-center tw:text-sm tw:text-subtle-foreground",
                    "No playlist entries yet."
                }
            }
        };
    }

    rsx! {
        NodeCardSection { label: "entries", first: true,
            div { class: "tw:flex tw:min-w-0 tw:gap-2 tw:overflow-x-auto tw:px-4 tw:py-3",
                for entry in face.entries.clone() {
                    PlaylistEntryChip {
                        key: "{entry.key}",
                        active: face.active == Some(entry.key),
                        entry,
                        on_action,
                    }
                }
            }
        }
    }
}

/// One strip entry: thumbnail (or the ACTIVE placard), name, cue tag, and
/// duration chip. With an entry action and a dispatcher present the chip is
/// a button — clicking selects/focuses the entry's child node (the reused
/// node-select action; activation-by-click has no wire op today).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PlaylistEntryChip(
    entry: UiPlaylistEntry,
    active: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let chip_class = if active {
        "tw:w-28 tw:flex-none tw:overflow-hidden tw:rounded-sm tw:border tw:border-status-live-border tw:bg-card-subtle tw:ring-1 tw:ring-status-live-border"
    } else {
        "tw:w-28 tw:flex-none tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-muted tw:bg-card-subtle"
    };
    let label_class = if active {
        "tw:flex tw:items-center tw:gap-1 tw:px-1.5 tw:py-1 tw:text-[11px] tw:text-status-live-foreground"
    } else {
        "tw:flex tw:items-center tw:gap-1 tw:px-1.5 tw:py-1 tw:text-[11px] tw:text-muted-foreground"
    };
    let duration = playlist_duration_label(&entry);
    let name = entry.name.clone();

    let body = rsx! {
        if active {
            // The playing entry's duplicate preview is replaced by the
            // placard — its real output renders on the child card below
            // the playlist card.
            div { class: "tw:grid tw:aspect-[9/5] tw:place-items-center tw:bg-status-live-bg tw:text-[10px] tw:font-bold tw:uppercase tw:tracking-[0.12em] tw:text-status-live-foreground",
                "ACTIVE"
            }
        } else if let Some(thumb) = entry.thumb.clone() {
            ProductPreview {
                kind: UiProductKind::Visual,
                preview: thumb,
                tracking: UiProductTrackingState::Tracking,
                frame: STRIP_THUMB_FRAME,
                focus_action: None,
                on_action: None,
            }
        } else {
            div { class: "tw:aspect-[9/5] tw:bg-page" }
        }
        div { class: label_class,
            span { class: "tw:min-w-0 tw:truncate", "{entry.name}" }
            if entry.cue {
                span {
                    class: "tw:inline-flex tw:flex-none tw:items-center tw:gap-0.5 tw:rounded-xs tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-1 tw:text-[9px] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-status-attention-foreground",
                    title: "Waits for a trigger",
                    StudioIcon { name: StudioIconName::Cue, size: 8 }
                    "cue"
                }
            }
            span { class: "tw:ml-auto tw:flex-none tw:font-mono tw:text-[10px] tw:text-subtle-foreground",
                "{duration}"
            }
        }
    };

    if let (Some(action), Some(handler)) = (entry.action.clone(), on_action) {
        return rsx! {
            button {
                class: "{chip_class} tw:cursor-pointer tw:p-0 tw:text-left tw:hover:border-border",
                r#type: "button",
                title: "Select {name}",
                onclick: move |event| {
                    event.stop_propagation();
                    handler.call(action.clone());
                },
                {body}
            }
        };
    }

    rsx! {
        div { class: chip_class, {body} }
    }
}

/// Duration column text: authored duration as m:ss, "hold" for cue entries
/// without one (play up to the cue, hold for the trigger), empty otherwise.
fn playlist_duration_label(entry: &UiPlaylistEntry) -> String {
    match entry.duration_ms {
        Some(duration_ms) => format_playlist_duration(duration_ms),
        None if entry.cue => "hold".to_string(),
        None => String::new(),
    }
}

/// m:ss with zero-padded seconds (`270_000` → "4:30").
fn format_playlist_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiPlaylistEntry;

    use super::{format_playlist_duration, playlist_duration_label};

    fn entry(key: u32, name: &str) -> UiPlaylistEntry {
        UiPlaylistEntry {
            key,
            name: name.to_string(),
            duration_ms: None,
            cue: false,
            thumb: None,
            action: None,
        }
    }

    #[test]
    fn durations_format_as_minutes_and_padded_seconds() {
        assert_eq!(format_playlist_duration(270_000), "4:30");
        assert_eq!(format_playlist_duration(180_000), "3:00");
        assert_eq!(format_playlist_duration(5_000), "0:05");
    }

    #[test]
    fn cue_entries_without_duration_read_hold() {
        let mut cue = entry(3, "Tide");
        cue.cue = true;
        assert_eq!(playlist_duration_label(&cue), "hold");

        let mut timed = entry(1, "Aurora");
        timed.duration_ms = Some(270_000);
        assert_eq!(playlist_duration_label(&timed), "4:30");

        assert_eq!(playlist_duration_label(&entry(0, "Sunrise")), "");
    }
}
