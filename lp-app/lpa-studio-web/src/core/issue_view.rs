use dioxus::prelude::*;
use lpa_studio_core::UiIssue;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IssueView(issue: UiIssue) -> Element {
    let message = issue.message;
    let detail = issue.detail;

    rsx! {
        div { class: "ux-issue",
            p { class: "ux-panel-copy ux-panel-issue", "{message}" }
            if let Some(detail) = detail.as_ref() {
                p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
            }
        }
    }
}
