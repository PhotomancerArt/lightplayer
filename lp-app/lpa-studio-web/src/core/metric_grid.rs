use dioxus::prelude::*;
use lpa_studio_core::UiMetric;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MetricGrid(metrics: Vec<UiMetric>) -> Element {
    rsx! {
        dl { class: "ux-metrics",
            for metric in metrics {
                div {
                    dt { "{metric.label}" }
                    dd { "{metric.value}" }
                }
            }
        }
    }
}
