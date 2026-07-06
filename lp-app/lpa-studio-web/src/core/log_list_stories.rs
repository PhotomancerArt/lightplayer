use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::core::LogList;
use crate::core::story_fixtures::story_logs;

#[story]
pub(crate) fn mixed_levels() -> Element {
    rsx! {
        LogList {
            logs: story_logs(),
            max_entries: 80,
        }
    }
}

#[story]
pub(crate) fn empty() -> Element {
    rsx! {
        LogList {
            logs: Vec::new(),
            max_entries: 80,
        }
    }
}

/// The ring holds entries but the display filter hides every one of them —
/// the empty state must read differently from a truly empty ring.
#[story]
pub(crate) fn filtered_empty() -> Element {
    rsx! {
        LogList {
            logs: Vec::new(),
            max_entries: 80,
            hidden_count: 12,
        }
    }
}
