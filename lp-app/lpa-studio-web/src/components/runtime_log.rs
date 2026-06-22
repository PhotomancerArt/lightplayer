use dioxus::prelude::*;
use lpa_studio_ux::{UxLogEntry, UxLogLevel};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn RuntimeLog(logs: Vec<UxLogEntry>) -> Element {
    rsx! {
        section { class: "ux-log-panel",
            div { class: "ux-panel-heading",
                p { "Runtime" }
                h2 { "Recent activity" }
            }
            if logs.is_empty() {
                p { class: "ux-panel-copy", "No runtime messages yet." }
            } else {
                ol { class: "ux-log-list",
                    for entry in logs.iter().rev().take(8) {
                        li { class: log_class(entry.level),
                            span { "{log_level_label(entry.level)}" }
                            strong { "{entry.source}" }
                            p { "{entry.message}" }
                        }
                    }
                }
            }
        }
    }
}

fn log_level_label(level: UxLogLevel) -> &'static str {
    match level {
        UxLogLevel::Debug => "debug",
        UxLogLevel::Info => "info",
        UxLogLevel::Warn => "warn",
        UxLogLevel::Error => "error",
    }
}

fn log_class(level: UxLogLevel) -> &'static str {
    match level {
        UxLogLevel::Debug => "ux-log ux-log-debug",
        UxLogLevel::Info => "ux-log ux-log-info",
        UxLogLevel::Warn => "ux-log ux-log-warn",
        UxLogLevel::Error => "ux-log ux-log-error",
    }
}
