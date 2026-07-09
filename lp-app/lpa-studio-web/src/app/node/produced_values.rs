use dioxus::prelude::*;
use lpa_studio_core::UiProducedValue;

use crate::app::node::ProducedValueView;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedValues(values: Vec<UiProducedValue>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fit,minmax(180px,1fr))] tw:gap-2",
            for value in values {
                ProducedValueView { key: "{value.label}", value }
            }
        }
    }
}
