use dioxus::prelude::*;
use lpa_studio_core::UiMetric;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MetricGrid(metrics: Vec<UiMetric>) -> Element {
    rsx! {
        dl { class: "tw:grid tw:grid-cols-[repeat(auto-fit,minmax(130px,1fr))] tw:gap-2 tw:m-0",
            for metric in metrics {
                div { class: "tw:min-w-0 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
                    dt { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:leading-tight tw:text-subtle-foreground", "{metric.label}" }
                    dd { class: "tw:m-0 tw:mt-1 tw:text-sm tw:font-bold tw:leading-tight tw:text-status-neutral-foreground tw:break-words", "{metric.value}" }
                }
            }
        }
    }
}
