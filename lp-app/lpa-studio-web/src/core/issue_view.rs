use dioxus::prelude::*;
use lpa_studio_core::UiIssue;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IssueView(issue: UiIssue) -> Element {
    let message = issue.message;
    let detail = issue.detail;

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1",
            p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-status-error-foreground", "{message}" }
            if let Some(detail) = detail.as_ref() {
                p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-subtle-foreground", "{detail}" }
            }
        }
    }
}
