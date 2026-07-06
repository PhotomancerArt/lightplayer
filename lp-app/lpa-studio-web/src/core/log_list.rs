//! The console's scrolling log list: timestamped, per-level styled rows with
//! sticky bottom autoscroll.
//!
//! Rows are **container-responsive**: the list is a CSS container
//! (`@container`) and each row morphs on the container's own width, not the
//! viewport. Below 560px (the console's usual home in the narrow device
//! column) rows are two-line — a dim meta line (time · level · source) over a
//! full-width message, with a colored left accent for warn/error. At 560px and
//! wider the same DOM relayouts into the four-column time/level/source/message
//! grid. One row markup, two layouts, switched by `grid-template-areas`.
//!
//! Timestamps render as `HH:MM:SS` in UTC rather than the browser's local
//! zone: story-baseline PNGs and unit tests must be machine-independent, and
//! an injectable timezone would have to thread through every shell story for
//! no rendering benefit (documented P2 decision).

use std::rc::Rc;

use dioxus::prelude::*;
use dioxus::{html::geometry::PixelsVector2D, prelude::dioxus_core::use_after_render};
use lpa_studio_core::{UiLogEntry, UiLogLevel, UiLogSource};

const LOG_STICKY_THRESHOLD_PX: f64 = 48.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogList(
    logs: Vec<UiLogEntry>,
    max_entries: usize,
    /// Ring entries hidden by the console's display filter. The list only
    /// uses this to pick the empty-state message: a non-zero count with no
    /// visible rows means "filtered empty", not "nothing ever logged".
    #[props(default = 0)]
    hidden_count: usize,
    /// The active display threshold, used to name it in the filtered-empty
    /// message ("No messages at Debug or above."). `None` in standalone
    /// stories that don't model a filter.
    #[props(default = None)]
    min_level: Option<UiLogLevel>,
    #[props(default = true)] framed: bool,
) -> Element {
    let visible_logs = log_tail(logs, max_entries);
    let mut log_element = use_signal(|| None::<Rc<MountedData>>);
    let mut stick_to_bottom = use_signal(|| true);
    // `@container` makes rows respond to the list's own width (see module docs).
    let list_class = if framed {
        "tw:@container tw:m-0 tw:grid tw:max-h-80 tw:gap-0 tw:overflow-auto tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-0 tw:list-none"
    } else {
        "tw:@container tw:m-0 tw:grid tw:max-h-80 tw:gap-0 tw:overflow-auto tw:bg-transparent tw:p-0 tw:list-none"
    };

    use_after_render(move || {
        if !stick_to_bottom() {
            return;
        }

        let Some(element) = log_element.read().as_ref().cloned() else {
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

    rsx! {
        ol {
            class: "{list_class}",
            onmounted: move |event| {
                log_element.set(Some(event.data()));
            },
            onscroll: move |event| {
                stick_to_bottom.set(is_log_near_bottom(
                    event.scroll_top(),
                    event.scroll_height(),
                    event.client_height(),
                ));
            },
            if visible_logs.is_empty() {
                {
                    let (primary, hint) = empty_log_state(hidden_count, min_level);
                    rsx! {
                        li { class: "tw:flex tw:flex-col tw:items-center tw:gap-1 tw:px-4 tw:py-8 tw:text-center",
                            p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground", "{primary}" }
                            if let Some(hint) = hint {
                                p { class: "tw:m-0 tw:text-xs tw:text-dim-foreground", "{hint}" }
                            }
                        }
                    }
                }
            } else {
                for entry in visible_logs.iter() {
                    li { class: row_class(entry.level),
                        span { class: "{TIME_CELL}", "{format_log_time(entry.timestamp)}" }
                        span { class: "{LEVEL_CELL}", "{log_level_label(entry.level)}" }
                        strong {
                            class: "{SOURCE_CELL}",
                            title: log_source_title(&entry.source),
                            "{entry.source.origin.label()}"
                            if let Some(detail) = entry.source.detail.as_ref() {
                                span { class: "tw:font-normal tw:text-subtle-foreground", " · {detail}" }
                            }
                        }
                        p { class: "{MESSAGE_CELL}", "{entry.message}" }
                    }
                }
            }
        }
    }
}

fn log_tail(logs: Vec<UiLogEntry>, max_entries: usize) -> Vec<UiLogEntry> {
    let skip_count = logs.len().saturating_sub(max_entries);
    logs.into_iter().skip(skip_count).collect()
}

fn is_log_near_bottom(scroll_top: f64, scroll_height: i32, client_height: i32) -> bool {
    f64::from(scroll_height) - scroll_top - f64::from(client_height) <= LOG_STICKY_THRESHOLD_PX
}

/// The empty-state text: a primary line plus an optional dim hint. An empty
/// ring ("nothing logged yet") reads differently from a ring whose entries are
/// all hidden by the filter, and a level threshold is named when it is the
/// thing doing the hiding.
fn empty_log_state(
    hidden_count: usize,
    min_level: Option<UiLogLevel>,
) -> (String, Option<&'static str>) {
    if hidden_count == 0 {
        return ("No messages yet.".to_string(), None);
    }
    match min_level {
        // Threshold above the floor: the level filter is (at least partly) why
        // the list is empty, so name it.
        Some(level) if level > UiLogLevel::Trace => (
            format!("No messages at {} or above.", level_display_name(level)),
            Some("Lower the level filter to see more."),
        ),
        // Everything is shown by level, so the hiding is entirely by source.
        _ => (
            "No messages from the enabled sources.".to_string(),
            Some("Enable more sources to see them."),
        ),
    }
}

/// Title-case level name for prose (the row labels use lowercase).
fn level_display_name(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace => "Trace",
        UiLogLevel::Debug => "Debug",
        UiLogLevel::Info => "Info",
        UiLogLevel::Warn => "Warn",
        UiLogLevel::Error => "Error",
    }
}

/// `HH:MM:SS` (UTC) for a row's fractional-epoch-seconds timestamp.
fn format_log_time(timestamp_secs: f64) -> String {
    let (hours, minutes, seconds) = utc_hms(timestamp_secs);
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

/// Split epoch seconds into UTC hours/minutes/seconds of day. Pure arithmetic
/// (no browser clock APIs) so it is unit-testable and deterministic.
fn utc_hms(timestamp_secs: f64) -> (u32, u32, u32) {
    const SECONDS_PER_DAY: f64 = 86_400.0;
    let day_secs = timestamp_secs.rem_euclid(SECONDS_PER_DAY) as u64;
    let hours = (day_secs / 3600) as u32;
    let minutes = (day_secs % 3600 / 60) as u32;
    let seconds = (day_secs % 60) as u32;
    (hours, minutes, seconds)
}

/// Hover text for the (possibly truncated) source cell: both dimensions when
/// a detail is present, just the origin label otherwise.
fn log_source_title(source: &UiLogSource) -> String {
    match &source.detail {
        Some(detail) => format!("{} · {detail}", source.origin.label()),
        None => source.origin.label().to_string(),
    }
}

fn log_level_label(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace => "trace",
        UiLogLevel::Debug => "debug",
        UiLogLevel::Info => "info",
        UiLogLevel::Warn => "warn",
        UiLogLevel::Error => "error",
    }
}

/// Shared row geometry, switched by container width. Narrow (<560px): three
/// auto columns with a `time level source` meta line over a full-width
/// `message`, plus a 2px left accent bar. Wide (≥560px): the four-column
/// `time level source message` grid with no left accent (the level tone moves
/// to the bottom border, matching the pre-container design).
const ROW_BASE: &str = "tw:grid tw:gap-x-2 tw:gap-y-0.5 tw:border-b tw:border-l-2 tw:px-3 tw:py-1.5 tw:text-sm \
    tw:grid-cols-[auto_auto_minmax(0,1fr)] tw:[grid-template-areas:'time_level_source'_'message_message_message'] \
    tw:@min-[560px]:grid-cols-[64px_52px_minmax(72px,128px)_minmax(0,1fr)] \
    tw:@min-[560px]:[grid-template-areas:'time_level_source_message'] \
    tw:@min-[560px]:gap-2 tw:@min-[560px]:border-l-0 tw:@min-[560px]:py-2";

const TIME_CELL: &str = "tw:[grid-area:time] tw:font-mono tw:text-[11px] tw:text-dim-foreground tw:@min-[560px]:text-xs";
const LEVEL_CELL: &str =
    "tw:[grid-area:level] tw:font-mono tw:text-[11px] tw:uppercase tw:@min-[560px]:text-xs";
const SOURCE_CELL: &str = "tw:[grid-area:source] tw:min-w-0 tw:truncate tw:text-[11px] tw:text-dim-foreground tw:@min-[560px]:text-xs";
const MESSAGE_CELL: &str = "tw:[grid-area:message] tw:m-0 tw:min-w-0 tw:break-words";

/// Per-level tone: text color (whole row), plus the narrow left accent and the
/// wide bottom border. Warn/error carry the accent narrow and the colored
/// bottom border wide; quieter levels stay on the muted border with no accent.
fn row_class(level: UiLogLevel) -> String {
    let tone = match level {
        // Trace reuses Debug's tone: the theme has no dimmer text token than
        // `subtle-foreground`.
        UiLogLevel::Trace | UiLogLevel::Debug => {
            "tw:border-border-muted tw:border-l-transparent tw:text-subtle-foreground"
        }
        UiLogLevel::Info => {
            "tw:border-border-muted tw:border-l-transparent tw:text-muted-foreground"
        }
        UiLogLevel::Warn => {
            "tw:border-border-muted tw:border-l-status-warning-border tw:text-status-warning-foreground tw:@min-[560px]:border-b-status-warning-border"
        }
        UiLogLevel::Error => {
            "tw:border-border-muted tw:border-l-status-error-border tw:text-status-error-foreground tw:@min-[560px]:border-b-status-error-border"
        }
    };
    format!("{ROW_BASE} {tone}")
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiLogOrigin, UiLogSource};

    use super::*;

    #[test]
    fn format_log_time_zero_pads_every_component() {
        // 1970-01-01 01:02:03 UTC.
        assert_eq!(format_log_time(3_723.0), "01:02:03");
        assert_eq!(format_log_time(0.0), "00:00:00");
        // Fractional sub-second part is dropped, not rounded up.
        assert_eq!(format_log_time(59.9), "00:00:59");
    }

    #[test]
    fn format_log_time_uses_utc_seconds_of_day() {
        // 2024-07-03 09:46:40 UTC — the shared story fixture timestamp.
        assert_eq!(format_log_time(1_720_000_000.0), "09:46:40");
        // End of day wraps to the next day's 00:00:00.
        assert_eq!(format_log_time(86_400.0), "00:00:00");
        assert_eq!(format_log_time(86_399.0), "23:59:59");
    }

    #[test]
    fn utc_hms_splits_day_seconds() {
        assert_eq!(utc_hms(86_399.0), (23, 59, 59));
        assert_eq!(utc_hms(43_200.0), (12, 0, 0));
    }

    #[test]
    fn empty_log_state_distinguishes_truly_empty_from_filtered() {
        assert_eq!(
            empty_log_state(0, Some(UiLogLevel::Info)),
            ("No messages yet.".to_string(), None)
        );
        // A threshold above the floor names the level.
        let (primary, hint) = empty_log_state(3, Some(UiLogLevel::Debug));
        assert_eq!(primary, "No messages at Debug or above.");
        assert!(hint.is_some());
        // At the floor, the hiding must be by source.
        let (primary, _) = empty_log_state(3, Some(UiLogLevel::Trace));
        assert_eq!(primary, "No messages from the enabled sources.");
    }

    #[test]
    fn log_tail_keeps_the_newest_entries() {
        let logs: Vec<UiLogEntry> = (0..5)
            .map(|index| {
                UiLogEntry::new(
                    f64::from(index),
                    UiLogLevel::Info,
                    UiLogOrigin::Studio,
                    format!("message {index}"),
                )
            })
            .collect();

        let tail = log_tail(logs, 2);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].message, "message 3");
        assert_eq!(tail[1].message, "message 4");
    }

    #[test]
    fn log_source_title_includes_detail_when_present() {
        let bare = UiLogSource::new(UiLogOrigin::Server);
        let detailed = UiLogSource::with_detail(UiLogOrigin::Device, "fw_core::serial");

        assert_eq!(log_source_title(&bare), "server");
        assert_eq!(log_source_title(&detailed), "device · fw_core::serial");
    }
}
