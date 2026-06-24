use dioxus::prelude::*;
use lpa_studio_core::UiMetric;
use lpa_studio_web_story_macros::story;

use crate::core::MetricGrid;
use crate::core::story_fixtures::story_metrics;

#[story]
pub(crate) fn compact() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Metric grid" }
            }
            MetricGrid {
                metrics: story_metrics(),
            }
        }
    }
}

#[story]
pub(crate) fn dense() -> Element {
    rsx! {
        section { class: "ux-panel",
            div { class: "ux-panel-heading",
                h2 { "Dense metrics" }
            }
            MetricGrid {
                metrics: vec![
                    UiMetric::new("Runtime", "ESP32-C6"),
                    UiMetric::new("Protocol", "fw-browser-post-message-v1"),
                    UiMetric::new("Project", "studio-demo"),
                    UiMetric::new("Nodes", 9),
                    UiMetric::new("FPS", "936"),
                    UiMetric::new("Memory", "207k free"),
                    UiMetric::new("Link", "browser worker"),
                    UiMetric::new("Session", "worker-1"),
                ],
            }
        }
    }
}
