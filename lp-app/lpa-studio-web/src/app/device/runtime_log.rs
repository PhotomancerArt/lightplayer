//! The Studio console panel: a filter toolbar over the scrolling log list.
//!
//! The toolbar renders the filter state carried by [`UiConsoleView`] (level
//! threshold, origin toggles, hidden count) and reports user gestures as
//! [`ConsoleCommand`]s; the actual filtering lives in `lpa-studio-core`.

use dioxus::prelude::*;
use lpa_studio_core::{ConsoleCommand, UiConsoleView, UiLogLevel};

use crate::core::LogList;

/// Cap on rendered console rows: the newest filtered entries kept in the DOM.
/// The core ring holds up to 1000 entries; rendering shows a 250-row tail to
/// keep the list light. Tail truncation is display-only and is NOT part of
/// the toolbar's "N hidden" count, which counts filter-hidden ring entries.
const RENDERED_LOG_TAIL: usize = 250;

/// The level-threshold dropdown's options, lowest to highest severity.
const LEVEL_OPTIONS: [UiLogLevel; 5] = [
    UiLogLevel::Trace,
    UiLogLevel::Debug,
    UiLogLevel::Info,
    UiLogLevel::Warn,
    UiLogLevel::Error,
];

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RuntimeLog(console: UiConsoleView, on_console: EventHandler<ConsoleCommand>) -> Element {
    let UiConsoleView {
        entries,
        hidden_count,
        min_level,
        origins,
        device_log_level,
    } = console;

    rsx! {
        section { class: "tw:rounded-md tw:border tw:border-border tw:bg-card",
            div { class: "tw:p-[18px] tw:pb-3",
                p { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Console" }
            }
            div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5 tw:px-[18px] tw:pb-3",
                select {
                    class: "tw:h-7 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-1.5 tw:text-xs tw:text-muted-foreground",
                    aria_label: "Minimum log level",
                    value: "{level_option_value(min_level)}",
                    onchange: move |event| {
                        if let Some(level) = level_from_option_value(&event.value()) {
                            on_console.call(ConsoleCommand::SetMinLevel(level));
                        }
                    },
                    for level in LEVEL_OPTIONS {
                        option {
                            value: level_option_value(level),
                            selected: level == min_level,
                            "{level_option_label(level)}"
                        }
                    }
                }
                for (origin, enabled) in origins {
                    button {
                        class: origin_chip_class(enabled),
                        r#type: "button",
                        aria_pressed: "{enabled}",
                        title: origin_chip_title(origin.label(), enabled),
                        onclick: move |_| {
                            on_console.call(ConsoleCommand::SetOriginEnabled(origin, !enabled));
                        },
                        "{origin.label()}"
                    }
                }
                div { class: "tw:flex tw:items-center tw:gap-1",
                    span { class: "tw:text-xs tw:text-dim-foreground", "device" }
                    select {
                        class: "tw:h-7 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-1.5 tw:text-xs tw:text-muted-foreground tw:disabled:opacity-50",
                        aria_label: "Device log level",
                        title: device_level_title(device_log_level.is_some()),
                        disabled: device_log_level.is_none(),
                        value: device_level_value(device_log_level),
                        onchange: move |event| {
                            if let Some(level) = level_from_option_value(&event.value()) {
                                on_console.call(ConsoleCommand::SetDeviceLogLevel(level));
                            }
                        },
                        if device_log_level.is_none() {
                            option { value: "", selected: true, "\u{2013}" }
                        }
                        for level in LEVEL_OPTIONS {
                            option {
                                value: level_option_value(level),
                                selected: Some(level) == device_log_level,
                                "{level_option_label(level)}"
                            }
                        }
                    }
                }
                div { class: "tw:ml-auto tw:flex tw:items-center tw:gap-2",
                    if hidden_count > 0 {
                        span { class: "tw:text-xs tw:text-dim-foreground", "{hidden_count} hidden" }
                    }
                    button {
                        class: "tw:h-7 tw:rounded-sm tw:border tw:border-border-strong tw:bg-transparent tw:px-2 tw:text-xs tw:text-muted-foreground tw:hover:bg-card-muted",
                        r#type: "button",
                        title: "Empty the console log",
                        onclick: move |_| on_console.call(ConsoleCommand::Clear),
                        "Clear"
                    }
                }
            }
            LogList {
                logs: entries,
                max_entries: RENDERED_LOG_TAIL,
                hidden_count,
                framed: false,
            }
        }
    }
}

/// Capitalized dropdown label for a level option.
fn level_option_label(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace => "Trace",
        UiLogLevel::Debug => "Debug",
        UiLogLevel::Info => "Info",
        UiLogLevel::Warn => "Warn",
        UiLogLevel::Error => "Error",
    }
}

/// Stable `option` value for a level, round-tripped by
/// [`level_from_option_value`] when the `select` change event fires.
fn level_option_value(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace => "trace",
        UiLogLevel::Debug => "debug",
        UiLogLevel::Info => "info",
        UiLogLevel::Warn => "warn",
        UiLogLevel::Error => "error",
    }
}

/// Parse a dropdown `option` value back to its level; `None` for anything
/// that is not one of the five known values.
fn level_from_option_value(value: &str) -> Option<UiLogLevel> {
    LEVEL_OPTIONS
        .into_iter()
        .find(|level| level_option_value(*level) == value)
}

/// Toggle-chip styling: enabled chips read like the raised secondary
/// controls, disabled chips drop to a transparent dimmed outline.
fn origin_chip_class(enabled: bool) -> &'static str {
    if enabled {
        "tw:h-7 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-2 tw:text-xs tw:text-soft-foreground"
    } else {
        "tw:h-7 tw:rounded-sm tw:border tw:border-border tw:bg-transparent tw:px-2 tw:text-xs tw:text-dim-foreground tw:hover:bg-card-muted"
    }
}

/// Hover text describing what clicking an origin chip will do.
fn origin_chip_title(origin_label: &str, enabled: bool) -> String {
    if enabled {
        format!("Hide {origin_label} messages")
    } else {
        format!("Show {origin_label} messages")
    }
}

/// The device-level select's value: the last level Studio asked the
/// connected server to apply, or empty (placeholder) while disconnected.
fn device_level_value(level: Option<UiLogLevel>) -> &'static str {
    level.map(level_option_value).unwrap_or("")
}

/// Hover text for the device-level select, explaining the disabled state.
fn device_level_title(connected: bool) -> &'static str {
    if connected {
        "Change the connected device's log verbosity until it reboots"
    } else {
        "Connect a device to change its log verbosity"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_option_values_round_trip_every_level() {
        for level in LEVEL_OPTIONS {
            assert_eq!(
                level_from_option_value(level_option_value(level)),
                Some(level)
            );
        }
    }

    #[test]
    fn level_from_option_value_rejects_unknown_values() {
        assert_eq!(level_from_option_value("verbose"), None);
        assert_eq!(level_from_option_value(""), None);
    }

    #[test]
    fn origin_chip_title_describes_the_toggle_direction() {
        assert_eq!(origin_chip_title("device", true), "Hide device messages");
        assert_eq!(origin_chip_title("device", false), "Show device messages");
    }

    #[test]
    fn device_level_value_is_empty_while_disconnected() {
        assert_eq!(device_level_value(None), "");
        assert_eq!(device_level_value(Some(UiLogLevel::Warn)), "warn");
    }

    #[test]
    fn device_level_title_explains_the_disabled_state() {
        assert!(device_level_title(false).starts_with("Connect a device"));
        assert!(device_level_title(true).starts_with("Change the connected"));
    }
}
