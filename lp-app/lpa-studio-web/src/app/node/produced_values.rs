use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiProducedValue};

use crate::app::node::ProducedValueView;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedValues(
    values: Vec<UiProducedValue>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:grid-cols-[repeat(auto-fit,minmax(180px,1fr))] tw:gap-2",
            for value in values {
                ProducedValueView { key: "{value.label}", value, on_action }
            }
        }
    }
}
