//! The Studio console panel: a compact filter toolbar over the scrolling log
//! list.
//!
//! The toolbar renders the filter state carried by [`UiConsoleView`] and
//! reports user gestures as [`ConsoleCommand`]s; the actual filtering lives in
//! `lpa-studio-core`. Two "levels" are deliberately kept distinct so they
//! cannot be confused: the **display filter** (a funnel-marked `Level+`
//! threshold select — what the console *shows*) sits in the toolbar, while the
//! **device log level** (what the connected device *emits*, over the wire)
//! lives inside the gear popover with copy that spells out the difference.

use dioxus::prelude::*;
use lpa_studio_core::{ConsoleCommand, UiConsoleView, UiLogLevel, UiLogOrigin};

use crate::base::{IconPopoverButton, PopoverButton, PopoverPlacement, StudioIcon, StudioIconName};
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

const TOOLBAR_CONTROL: &str = "tw:inline-flex tw:h-7 tw:items-center tw:gap-1 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:px-2 tw:text-xs tw:text-muted-foreground";
const TOOLBAR_CONTROL_OPEN: &str = "tw:inline-flex tw:h-7 tw:items-center tw:gap-1 tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:px-2 tw:text-xs tw:text-soft-foreground";
const GEAR_TRIGGER: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:p-0 tw:text-muted-foreground";
const GEAR_TRIGGER_OPEN: &str = "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-raised tw:p-0 tw:text-soft-foreground";
const SOURCES_POPUP: &str = "tw:grid tw:w-44 tw:gap-0 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-raised tw:p-1 tw:text-sm tw:shadow-lg";
const DEVICE_POPUP: &str = "tw:grid tw:w-56 tw:gap-1.5 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-raised tw:p-3 tw:text-sm tw:shadow-lg";

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
            div { class: "tw:flex tw:items-start tw:gap-2 tw:px-3 tw:pb-2 tw:pt-3",
                p { class: "tw:m-0 tw:pt-1 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Console" }
                // Controls cluster right-aligned; wraps as a unit so a wrapped
                // control lands under the others, not orphaned on the left.
                div { class: "tw:ml-auto tw:flex tw:flex-wrap tw:items-center tw:justify-end tw:gap-1.5",
                    DisplayFilterSelect { min_level, on_console }
                    SourcesPopover { origins, on_console }
                    DeviceSettingsPopover { device_log_level, on_console }
                    button {
                        class: "tw:inline-flex tw:h-7 tw:items-center tw:rounded-sm tw:border tw:border-border-strong tw:bg-transparent tw:px-2 tw:text-xs tw:text-muted-foreground tw:hover:bg-card-muted",
                        r#type: "button",
                        title: "Empty the console log",
                        onclick: move |_| on_console.call(ConsoleCommand::Clear),
                        "Clear"
                    }
                }
            }
            if hidden_count > 0 {
                div { class: "tw:px-3 tw:pb-1 tw:text-right tw:text-[11px] tw:text-dim-foreground",
                    "{hidden_count} hidden"
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

/// The display-threshold control, styled as a *filter* (funnel glyph +
/// `Level+` phrasing) so it cannot be mistaken for the device-level select in
/// the gear popover.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DisplayFilterSelect(min_level: UiLogLevel, on_console: EventHandler<ConsoleCommand>) -> Element {
    rsx! {
        span {
            class: "tw:relative tw:inline-flex tw:h-7 tw:items-center tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:text-xs tw:text-muted-foreground",
            title: "Hide messages below this level (display filter only)",
            span { class: "tw:pointer-events-none tw:absolute tw:left-1.5 tw:flex tw:text-dim-foreground",
                StudioIcon { name: StudioIconName::Filter, size: 11 }
            }
            select {
                class: "tw:h-7 tw:appearance-none tw:bg-transparent tw:pl-6 tw:pr-5 tw:text-xs tw:text-muted-foreground tw:focus:outline-none",
                aria_label: "Minimum displayed log level",
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
                        "{level_threshold_label(level)}"
                    }
                }
            }
            span { class: "tw:pointer-events-none tw:absolute tw:right-1.5 tw:text-[9px] tw:text-dim-foreground", "▾" }
        }
    }
}

/// The origin filter: a single "Sources" popover with a per-origin checkbox
/// list and a badge counting hidden sources.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SourcesPopover(
    origins: Vec<(UiLogOrigin, bool)>,
    on_console: EventHandler<ConsoleCommand>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let hidden = disabled_origin_count(&origins);
    rsx! {
        PopoverButton {
            class: TOOLBAR_CONTROL.to_string(),
            open_class: TOOLBAR_CONTROL_OPEN.to_string(),
            label: "Filter sources".to_string(),
            title: "Show or hide messages by source".to_string(),
            popup_class: SOURCES_POPUP.to_string(),
            placement: PopoverPlacement::BottomEnd,
            initially_open,
            trigger: rsx! {
                span { "Sources" }
                if hidden > 0 {
                    span { class: "tw:rounded-full tw:bg-status-warning-bg tw:px-1 tw:text-[10px] tw:font-bold tw:text-status-warning-foreground", "{hidden}" }
                }
                span { class: "tw:text-[9px] tw:text-dim-foreground", "▾" }
            },
            for (origin, enabled) in origins.iter().copied() {
                button {
                    class: "tw:flex tw:w-full tw:items-center tw:gap-2 tw:rounded-sm tw:border-0 tw:bg-transparent tw:px-2 tw:py-1 tw:text-xs tw:text-soft-foreground tw:hover:bg-card-muted",
                    r#type: "button",
                    aria_pressed: "{enabled}",
                    onclick: move |_| on_console.call(ConsoleCommand::SetOriginEnabled(origin, !enabled)),
                    span {
                        class: if enabled {
                            "tw:flex tw:h-3.5 tw:w-3.5 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-accent tw:bg-accent tw:text-[9px] tw:font-bold tw:text-accent-text-on-fill"
                        } else {
                            "tw:h-3.5 tw:w-3.5 tw:rounded-xs tw:border tw:border-border-strong tw:bg-card"
                        },
                        if enabled { "✓" }
                    }
                    "{origin.label()}"
                }
            }
            if hidden > 0 {
                div { class: "tw:border-t tw:border-border-muted tw:px-2 tw:pb-1 tw:pt-1.5 tw:text-[11px] tw:text-dim-foreground",
                    "{hidden_sources_note(hidden)}"
                }
            }
        }
    }
}

/// The device-settings gear: a popover holding the device log-level select,
/// or a disabled gear button when no device is connected.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceSettingsPopover(
    device_log_level: Option<UiLogLevel>,
    on_console: EventHandler<ConsoleCommand>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let Some(level) = device_log_level else {
        return rsx! {
            button {
                class: "tw:inline-flex tw:h-7 tw:w-7 tw:items-center tw:justify-center tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:p-0 tw:text-dim-foreground tw:opacity-50",
                r#type: "button",
                disabled: true,
                title: "Connect a device to change its log verbosity",
                StudioIcon { name: StudioIconName::Settings, size: 13 }
            }
        };
    };
    rsx! {
        IconPopoverButton {
            class: GEAR_TRIGGER.to_string(),
            open_class: GEAR_TRIGGER_OPEN.to_string(),
            icon: StudioIconName::Settings,
            icon_size: 13,
            label: "Device settings".to_string(),
            title: "Device log settings".to_string(),
            popup_class: DEVICE_POPUP.to_string(),
            placement: PopoverPlacement::BottomEnd,
            initially_open,
            p { class: "tw:m-0 tw:text-xs tw:font-bold tw:text-heading", "Device log level" }
            select {
                class: "tw:h-7 tw:w-full tw:rounded-sm tw:border tw:border-border-strong tw:bg-card-muted tw:px-1.5 tw:text-xs tw:text-muted-foreground",
                aria_label: "Device log level",
                value: "{level_option_value(level)}",
                onchange: move |event| {
                    if let Some(next) = level_from_option_value(&event.value()) {
                        on_console.call(ConsoleCommand::SetDeviceLogLevel(next));
                    }
                },
                for option_level in LEVEL_OPTIONS {
                    option {
                        value: level_option_value(option_level),
                        selected: option_level == level,
                        "{level_option_label(option_level)}"
                    }
                }
            }
            p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                "How much the connected device logs at all. Applies until it reboots — independent of the display filter."
            }
        }
    }
}

/// Plain capitalized level label (device popover): the device level is an
/// exact setting, not a threshold.
fn level_option_label(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace => "Trace",
        UiLogLevel::Debug => "Debug",
        UiLogLevel::Info => "Info",
        UiLogLevel::Warn => "Warn",
        UiLogLevel::Error => "Error",
    }
}

/// Threshold-phrased label for the display filter: `Level+` means "this level
/// and everything above it", except `Error` (nothing is above it).
fn level_threshold_label(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Trace => "Trace+",
        UiLogLevel::Debug => "Debug+",
        UiLogLevel::Info => "Info+",
        UiLogLevel::Warn => "Warn+",
        UiLogLevel::Error => "Error",
    }
}

/// Stable `option` value for a level, round-tripped by
/// [`level_from_option_value`] when a `select` change event fires.
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

/// Number of origins currently toggled off — the Sources badge count.
fn disabled_origin_count(origins: &[(UiLogOrigin, bool)]) -> usize {
    origins.iter().filter(|(_, enabled)| !enabled).count()
}

/// Footer text under the Sources list, pluralized.
fn hidden_sources_note(hidden: usize) -> String {
    if hidden == 1 {
        "1 source hidden".to_string()
    } else {
        format!("{hidden} sources hidden")
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
    fn threshold_labels_mark_every_level_but_error_as_inclusive() {
        assert_eq!(level_threshold_label(UiLogLevel::Info), "Info+");
        assert_eq!(level_threshold_label(UiLogLevel::Error), "Error");
    }

    #[test]
    fn disabled_origin_count_counts_only_toggled_off_sources() {
        let origins = [
            (UiLogOrigin::Studio, true),
            (UiLogOrigin::Link, false),
            (UiLogOrigin::Server, true),
            (UiLogOrigin::Device, false),
        ];
        assert_eq!(disabled_origin_count(&origins), 2);
    }

    #[test]
    fn hidden_sources_note_pluralizes() {
        assert_eq!(hidden_sources_note(1), "1 source hidden");
        assert_eq!(hidden_sources_note(3), "3 sources hidden");
    }
}
