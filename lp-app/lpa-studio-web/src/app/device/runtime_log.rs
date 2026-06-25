use dioxus::prelude::*;
use lpa_studio_core::UiLogEntry;

use crate::core::LogList;

const LOG_ENTRY_LIMIT: usize = 80;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RuntimeLog(logs: Vec<UiLogEntry>) -> Element {
    rsx! {
        section { class: "tw:rounded-md tw:border tw:border-border tw:bg-card",
            div { class: "tw:p-[18px] tw:pb-3",
                p { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "Console" }
            }
            LogList {
                logs,
                max_entries: LOG_ENTRY_LIMIT,
                framed: false,
            }
        }
    }
}
