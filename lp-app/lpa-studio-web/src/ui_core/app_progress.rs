use dioxus::prelude::*;
use lpa_studio_ux::UiProgress;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AppProgress(progress: UiProgress) -> Element {
    let label = progress.label;
    let detail = progress.detail;
    let percent = progress.percent;
    let timeout_ms = progress.timeout_ms.unwrap_or(0);
    let bar_class = if percent.is_some() {
        "ux-progress-fill ux-progress-fill-determinate"
    } else if progress.timeout_ms.is_some() {
        "ux-progress-fill ux-progress-fill-timeout"
    } else {
        "ux-progress-fill ux-progress-fill-indeterminate"
    };
    let fill_style = match (percent, progress.timeout_ms) {
        (Some(percent), _) => format!("width: {}%;", percent.min(100)),
        (None, Some(_)) => "width: 100%;".to_string(),
        (None, None) => String::new(),
    };
    let timeout_style = if timeout_ms > 0 {
        format!("animation-duration: {timeout_ms}ms;")
    } else {
        String::new()
    };

    rsx! {
        div { class: "ux-progress",
            div { class: "ux-progress-meta",
                span { "{label}" }
                if let Some(percent) = percent {
                    strong { "{percent.min(100)}%" }
                }
            }
            div { class: "ux-progress-track",
                div { class: "{bar_class}", style: "{fill_style}{timeout_style}" }
            }
            if let Some(detail) = detail.as_ref() {
                p { class: "ux-progress-detail", "{detail}" }
            }
        }
    }
}
