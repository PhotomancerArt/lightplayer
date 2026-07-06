//! Stories for the console panel: toolbar filter states and empty states.

use dioxus::prelude::*;
use lpa_studio_core::{UiConsoleView, UiLogLevel, UiLogOrigin};
use lpa_studio_web_story_macros::story;

use crate::app::RuntimeLog;
use crate::app::story_fixtures::story_console;
use crate::core::story_fixtures::story_logs;

/// Everything visible: threshold at Trace, all origins on, no hidden rows,
/// and a connected device whose level selector shows Debug.
#[story]
pub(crate) fn console_verbose() -> Element {
    let mut console = story_console(story_logs(), 0, UiLogLevel::Trace, &[]);
    console.device_log_level = Some(UiLogLevel::Debug);
    rsx! {
        RuntimeLog {
            console,
            on_console: move |_| {},
        }
    }
}

/// An active filter: Debug threshold, the device origin toggled off, and the
/// hidden-count indicator showing the two excluded ring entries.
#[story]
pub(crate) fn console_filtered() -> Element {
    let entries = story_logs()
        .into_iter()
        .filter(|entry| {
            entry.level >= UiLogLevel::Debug && entry.source.origin != UiLogOrigin::Device
        })
        .collect();
    rsx! {
        RuntimeLog {
            console: story_console(entries, 2, UiLogLevel::Debug, &[UiLogOrigin::Device]),
            on_console: move |_| {},
        }
    }
}

/// A non-empty ring whose every entry is filtered out: the list shows the
/// "No messages at this level." empty state next to the hidden count.
#[story]
pub(crate) fn console_filtered_empty() -> Element {
    rsx! {
        RuntimeLog {
            console: story_console(Vec::new(), 12, UiLogLevel::Error, &[]),
            on_console: move |_| {},
        }
    }
}

/// A truly empty console: default filter state, "No messages yet.".
#[story]
pub(crate) fn console_empty() -> Element {
    rsx! {
        RuntimeLog {
            console: UiConsoleView::empty(),
            on_console: move |_| {},
        }
    }
}
