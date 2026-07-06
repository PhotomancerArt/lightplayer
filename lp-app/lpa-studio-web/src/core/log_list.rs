//! The console's scrolling log list: timestamped, per-level styled rows with
//! sticky bottom autoscroll.
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
    #[props(default = true)] framed: bool,
) -> Element {
    let visible_logs = log_tail(logs, max_entries);
    let mut log_element = use_signal(|| None::<Rc<MountedData>>);
    let mut stick_to_bottom = use_signal(|| true);
    let list_class = if framed {
        "tw:m-0 tw:grid tw:max-h-80 tw:gap-0 tw:overflow-auto tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-0 tw:list-none"
    } else {
        "tw:m-0 tw:grid tw:max-h-80 tw:gap-0 tw:overflow-auto tw:bg-transparent tw:p-0 tw:list-none"
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
                li { class: "tw:grid tw:grid-cols-[64px_52px_minmax(72px,128px)_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2 tw:text-sm tw:text-subtle-foreground",
                    span { class: "tw:font-mono tw:text-xs tw:text-dim-foreground", "--:--:--" }
                    span { class: "tw:font-mono tw:text-xs tw:uppercase", "idle" }
                    strong { class: "tw:text-xs tw:text-dim-foreground", "studio" }
                    p { class: "tw:m-0 tw:min-w-0 tw:break-words", "{empty_log_message(hidden_count)}" }
                }
            } else {
                for entry in visible_logs.iter() {
                    li { class: log_class(entry.level),
                        span { class: "tw:font-mono tw:text-xs tw:text-dim-foreground", "{format_log_time(entry.timestamp)}" }
                        span { class: "tw:font-mono tw:text-xs tw:uppercase", "{log_level_label(entry.level)}" }
                        strong {
                            class: "tw:min-w-0 tw:truncate tw:text-xs tw:text-dim-foreground",
                            title: log_source_title(&entry.source),
                            "{entry.source.origin.label()}"
                            if let Some(detail) = entry.source.detail.as_ref() {
                                span { class: "tw:font-normal tw:text-subtle-foreground", " · {detail}" }
                            }
                        }
                        p { class: "tw:m-0 tw:min-w-0 tw:break-words", "{entry.message}" }
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

/// The empty-state row text: an empty ring reads differently from a ring
/// whose every entry is filtered out by the current level/origin filter.
fn empty_log_message(hidden_count: usize) -> &'static str {
    if hidden_count > 0 {
        "No messages at this level."
    } else {
        "No messages yet."
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

fn log_class(level: UiLogLevel) -> &'static str {
    match level {
        // Trace reuses Debug's classes: the theme has no dimmer text token
        // than `subtle-foreground`.
        UiLogLevel::Trace | UiLogLevel::Debug => {
            "tw:grid tw:grid-cols-[64px_52px_minmax(72px,128px)_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2 tw:text-sm tw:text-subtle-foreground"
        }
        UiLogLevel::Info => {
            "tw:grid tw:grid-cols-[64px_52px_minmax(72px,128px)_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2 tw:text-sm tw:text-muted-foreground"
        }
        UiLogLevel::Warn => {
            "tw:grid tw:grid-cols-[64px_52px_minmax(72px,128px)_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-status-warning-border tw:px-3 tw:py-2 tw:text-sm tw:text-status-warning-foreground"
        }
        UiLogLevel::Error => {
            "tw:grid tw:grid-cols-[64px_52px_minmax(72px,128px)_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-status-error-border tw:px-3 tw:py-2 tw:text-sm tw:text-status-error-foreground"
        }
    }
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
    fn empty_log_message_distinguishes_filtered_from_truly_empty() {
        assert_eq!(empty_log_message(0), "No messages yet.");
        assert_eq!(empty_log_message(3), "No messages at this level.");
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
