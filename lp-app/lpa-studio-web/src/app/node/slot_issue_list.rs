//! Issue list presentation for config slots.

use dioxus::prelude::*;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotIssueList(issues: Vec<String>) -> Element {
    if issues.is_empty() {
        return rsx! {};
    }

    rsx! {
        ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
            for issue in issues {
                li { class: "tw:text-xs tw:font-medium tw:text-status-error-foreground", "{issue}" }
            }
        }
    }
}
