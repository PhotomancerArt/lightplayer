//! Stories for the console panel: the compact toolbar, container-responsive
//! rows, filter states, and the two popovers open.
//!
//! Rows respond to the console's *container* width, so the width-wrapper divs
//! below (not the story viewport) decide whether a panel shows the narrow
//! two-line layout or the wide grid.

use dioxus::prelude::*;
use lpa_studio_core::{UiConsoleView, UiLogLevel, UiLogOrigin};
use lpa_studio_web_story_macros::story;

use crate::app::RuntimeLog;
use crate::app::device::{DeviceSettingsPopover, SourcesPopover};
use crate::app::story_fixtures::story_console;
use crate::core::story_fixtures::story_logs;

/// The console at its usual narrow home (~350px): compact toolbar, two-line
/// rows with warn/error accent bars, and a connected device (gear enabled).
#[story]
pub(crate) fn console_overview() -> Element {
    let mut console = story_console(story_logs(), 0, UiLogLevel::Trace, &[]);
    console.device_log_level = Some(UiLogLevel::Debug);
    rsx! {
        div { class: "tw:p-4",
            div { style: "width: 350px;",
                RuntimeLog { console, on_console: move |_| {} }
            }
        }
    }
}

/// The same console given room (600px): rows relayout into the four-column
/// time/level/source/message grid.
#[story]
pub(crate) fn console_wide() -> Element {
    let mut console = story_console(story_logs(), 0, UiLogLevel::Trace, &[]);
    console.device_log_level = Some(UiLogLevel::Debug);
    rsx! {
        div { class: "tw:p-4",
            div { style: "width: 600px;",
                RuntimeLog { console, on_console: move |_| {} }
            }
        }
    }
}

/// An active filter at 350px: Debug threshold, the device source toggled off
/// (Sources badge shows 1), and the right-aligned hidden-count sliver.
#[story]
pub(crate) fn console_filtered() -> Element {
    let entries = story_logs()
        .into_iter()
        .filter(|entry| {
            entry.level >= UiLogLevel::Debug && entry.source.origin != UiLogOrigin::Device
        })
        .collect();
    rsx! {
        div { class: "tw:p-4",
            div { style: "width: 350px;",
                RuntimeLog {
                    console: story_console(entries, 2, UiLogLevel::Debug, &[UiLogOrigin::Device]),
                    on_console: move |_| {},
                }
            }
        }
    }
}

/// A non-empty ring whose every entry is filtered out: the list shows the
/// "No messages at this level." empty state next to the hidden count.
#[story]
pub(crate) fn console_filtered_empty() -> Element {
    rsx! {
        div { class: "tw:p-4",
            div { style: "width: 350px;",
                RuntimeLog {
                    console: story_console(Vec::new(), 12, UiLogLevel::Error, &[]),
                    on_console: move |_| {},
                }
            }
        }
    }
}

/// A truly empty console: default filter state, "No messages yet.", no device.
#[story]
pub(crate) fn console_empty() -> Element {
    rsx! {
        div { class: "tw:p-4",
            div { style: "width: 350px;",
                RuntimeLog { console: UiConsoleView::empty(), on_console: move |_| {} }
            }
        }
    }
}

/// The Sources popover open, with the device source unchecked and the hidden
/// footer showing.
#[story]
pub(crate) fn console_sources_open() -> Element {
    let origins = vec![
        (UiLogOrigin::Studio, true),
        (UiLogOrigin::Link, true),
        (UiLogOrigin::Server, true),
        (UiLogOrigin::Device, false),
    ];
    rsx! {
        div { class: "tw:flex tw:justify-center tw:p-4",
            div { class: "tw:h-56",
                SourcesPopover { origins, on_console: move |_| {}, initially_open: true }
            }
        }
    }
}

/// The device-settings popover open, device connected at Debug.
#[story]
pub(crate) fn console_device_settings_open() -> Element {
    rsx! {
        div { class: "tw:flex tw:justify-center tw:p-4",
            div { class: "tw:h-56",
                DeviceSettingsPopover {
                    device_log_level: Some(UiLogLevel::Debug),
                    on_console: move |_| {},
                    initially_open: true,
                }
            }
        }
    }
}
