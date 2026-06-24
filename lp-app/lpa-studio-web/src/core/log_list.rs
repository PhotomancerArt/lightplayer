use std::rc::Rc;

use dioxus::prelude::*;
use dioxus::{html::geometry::PixelsVector2D, prelude::dioxus_core::use_after_render};
use lpa_studio_core::{UiLogEntry, UiLogLevel};

const LOG_STICKY_THRESHOLD_PX: f64 = 48.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn LogList(logs: Vec<UiLogEntry>, max_entries: usize) -> Element {
    let visible_logs = log_tail(logs, max_entries);
    let mut log_element = use_signal(|| None::<Rc<MountedData>>);
    let mut stick_to_bottom = use_signal(|| true);

    use_after_render(move || {
        if !stick_to_bottom() {
            return;
        }

        let Some(element) = log_element.read().as_ref().cloned() else {
            return;
        };

        spawn(async move {
            let Ok(scroll_size) = element.get_scroll_size().await else {
                return;
            };
            let coordinates = PixelsVector2D::new(0.0, scroll_size.height);
            let _ = element.scroll(coordinates, ScrollBehavior::Instant).await;
        });
    });

    rsx! {
        ol {
            class: "ux-log-list",
            onmounted: move |event| {
                log_element.set(Some(event.data()));
            },
            onscroll: move |event| {
                stick_to_bottom.set(is_log_near_bottom(
                    event.scroll_top(),
                    event.scroll_height(),
                    event.client_height(),
                ));
            },
            if visible_logs.is_empty() {
                li { class: "ux-log ux-log-empty",
                    span { "idle" }
                    strong { "studio" }
                    p { "No messages yet." }
                }
            } else {
                for entry in visible_logs.iter() {
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

fn log_tail(logs: Vec<UiLogEntry>, max_entries: usize) -> Vec<UiLogEntry> {
    let skip_count = logs.len().saturating_sub(max_entries);
    logs.into_iter().skip(skip_count).collect()
}

fn is_log_near_bottom(scroll_top: f64, scroll_height: i32, client_height: i32) -> bool {
    f64::from(scroll_height) - scroll_top - f64::from(client_height) <= LOG_STICKY_THRESHOLD_PX
}

fn log_level_label(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Debug => "debug",
        UiLogLevel::Info => "info",
        UiLogLevel::Warn => "warn",
        UiLogLevel::Error => "error",
    }
}

fn log_class(level: UiLogLevel) -> &'static str {
    match level {
        UiLogLevel::Debug => "ux-log ux-log-debug",
        UiLogLevel::Info => "ux-log ux-log-info",
        UiLogLevel::Warn => "ux-log ux-log-warn",
        UiLogLevel::Error => "ux-log ux-log-error",
    }
}
