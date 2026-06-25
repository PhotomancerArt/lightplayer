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
            class: "tw:m-0 tw:grid tw:max-h-80 tw:gap-0 tw:overflow-auto tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-0 tw:list-none",
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
                li { class: "tw:grid tw:grid-cols-[52px_72px_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2 tw:text-sm tw:text-subtle-foreground",
                    span { class: "tw:font-mono tw:text-xs tw:uppercase", "idle" }
                    strong { class: "tw:text-xs tw:text-dim-foreground", "studio" }
                    p { class: "tw:m-0 tw:min-w-0 tw:break-words", "No messages yet." }
                }
            } else {
                for entry in visible_logs.iter() {
                    li { class: log_class(entry.level),
                        span { class: "tw:font-mono tw:text-xs tw:uppercase", "{log_level_label(entry.level)}" }
                        strong { class: "tw:text-xs tw:text-dim-foreground tw:break-words", "{entry.source}" }
                        p { class: "tw:m-0 tw:min-w-0 tw:break-words", "{entry.message}" }
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
        UiLogLevel::Debug => {
            "tw:grid tw:grid-cols-[52px_72px_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2 tw:text-sm tw:text-subtle-foreground"
        }
        UiLogLevel::Info => {
            "tw:grid tw:grid-cols-[52px_72px_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-border-muted tw:px-3 tw:py-2 tw:text-sm tw:text-muted-foreground"
        }
        UiLogLevel::Warn => {
            "tw:grid tw:grid-cols-[52px_72px_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-status-warning-border tw:px-3 tw:py-2 tw:text-sm tw:text-status-warning-foreground"
        }
        UiLogLevel::Error => {
            "tw:grid tw:grid-cols-[52px_72px_minmax(0,1fr)] tw:gap-2 tw:border-b tw:border-status-error-border tw:px-3 tw:py-2 tw:text-sm tw:text-status-error-foreground"
        }
    }
}
