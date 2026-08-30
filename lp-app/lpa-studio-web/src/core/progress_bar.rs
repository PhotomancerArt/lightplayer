//! The working bar. Aurora R2 (2026-08-29) gives the fill the spectrum:
//! `ux-iri-fill` is a slow horizontal sweep of the same stops the hover
//! ring turns, so "the app is doing something" and "you touched something"
//! read as one language. Work with no quantity also grows the conic
//! spinner beside its label — a bar that can never fill is a lie about
//! progress, and the spinner is honest about being a heartbeat.
//!
//! Unlike the hover ring these animations are not gesture-gated: they run
//! whenever the component is mounted, which is precisely the window work is
//! in flight. `prefers-reduced-motion` freezes both (style.css).

use dioxus::prelude::*;
use lpa_studio_core::UiProgress;

use crate::base::{conic_spinner_class, iridescent_fill_class, iridescent_fill_static_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProgressBar(progress: UiProgress) -> Element {
    let label = progress.label;
    let detail = progress.detail;
    let percent = progress.percent;
    let timeout_ms = progress.timeout_ms.unwrap_or(0);
    let held = iridescent_fill_static_class();
    let bar_class = if percent.is_some() {
        format!("tw:h-full tw:rounded-pill {}", iridescent_fill_class())
    } else if progress.timeout_ms.is_some() {
        format!(
            "tw:h-full tw:origin-left tw:rounded-pill {held} [animation:ux-progress-timeout_var(--ux-progress-timeout-duration)_linear_forwards]"
        )
    } else {
        format!(
            "tw:h-full tw:w-[35%] tw:rounded-pill {held} [animation:ux-progress-sweep_1.2s_ease-in-out_infinite]"
        )
    };
    // A quantity-free wait is the one that gets the heartbeat.
    let spinning = percent.is_none() && progress.timeout_ms.is_none();
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
                span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                    if spinning {
                        span { class: "{conic_spinner_class()}", aria_hidden: "true" }
                    }
                    span { "{label}" }
                }
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
