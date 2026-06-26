use dioxus::prelude::*;
use lpa_studio_core::UiProgress;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProgressBar(progress: UiProgress) -> Element {
    let label = progress.label;
    let detail = progress.detail;
    let percent = progress.percent;
    let timeout_ms = progress.timeout_ms.unwrap_or(0);
    let bar_class = if percent.is_some() {
        "tw:h-full tw:rounded-pill tw:bg-accent"
    } else if progress.timeout_ms.is_some() {
        "tw:h-full tw:origin-left tw:rounded-pill tw:bg-accent [animation:ux-progress-timeout_var(--ux-progress-timeout-duration)_linear_forwards]"
    } else {
        "tw:h-full tw:w-[35%] tw:rounded-pill tw:bg-accent [animation:ux-progress-sweep_1.2s_ease-in-out_infinite]"
    };
    let fill_style = match (percent, progress.timeout_ms) {
        (Some(percent), _) => format!("width: {}%;", percent.min(100)),
        (None, Some(_)) => "width: 100%;".to_string(),
        (None, None) => String::new(),
    };
    let timeout_style = if timeout_ms > 0 {
        format!("--ux-progress-timeout-duration: {timeout_ms}ms;")
    } else {
        String::new()
    };

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2",
            div { class: "tw:flex tw:items-center tw:justify-between tw:gap-3 tw:text-sm tw:font-bold tw:text-status-working-foreground",
                span { "{label}" }
                if let Some(percent) = percent {
                    strong { "{percent.min(100)}%" }
                }
            }
            div { class: "tw:h-2 tw:overflow-hidden tw:rounded-pill tw:border tw:border-border-strong tw:bg-track",
                div { class: "{bar_class}", style: "{fill_style}{timeout_style}" }
            }
            if let Some(detail) = detail.as_ref() {
                p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-subtle-foreground", "{detail}" }
            }
        }
    }
}
