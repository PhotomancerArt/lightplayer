use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiProducedProduct};

use crate::app::node::ProducedProductView;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedProducts(
    products: Vec<UiProducedProduct>,
    #[props(default)] focus_action: Option<UiAction>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:justify-items-center tw:gap-2 tw:p-2",
            for product in products.into_iter() {
                ProducedProductView {
                    key: "{product.name}",
                    product,
                    focus_action: focus_action.clone(),
                    on_action,
                }
            }
        }
    }
}
