use dioxus::prelude::*;
use lpa_studio_core::UiLogEntry;

use crate::core::LogList;

const LOG_ENTRY_LIMIT: usize = 80;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RuntimeLog(logs: Vec<UiLogEntry>) -> Element {
    rsx! {
        section { class: "ux-log-panel",
            div { class: "ux-log-heading",
                p { "Console" }
            }
            LogList {
                logs,
                max_entries: LOG_ENTRY_LIMIT,
            }
        }
    }
}
