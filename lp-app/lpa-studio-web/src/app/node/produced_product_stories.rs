//! Stories for produced product views.

use dioxus::prelude::*;
use lpa_studio_core::UiProducedProduct;
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::produced_product_variants_fixture;
use crate::app::node::{ProducedProductView, ProducedProducts};

#[story(description = "Produced product variants shown as a node pane section would render them.")]
pub(crate) fn gallery() -> Element {
    rsx! {
        ProducedProducts { products: produced_product_variants_fixture() }
    }
}

#[story(description = "An output slot that has not resolved to a product yet.")]
pub(crate) fn empty_product() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::empty("output").with_detail("not resolved") }
    }
}

#[story(description = "A visual product with the primary preview texture.")]
pub(crate) fn visual_product() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::visual("output").with_detail("128 x 128") }
    }
}

#[story(description = "An open produced product detail popup.")]
pub(crate) fn detail_popup() -> Element {
    let product = produced_product_variants_fixture().remove(1);

    rsx! {
        div { class: "tw:min-h-56",
            ProducedProductView {
                product,
                initially_open: true,
            }
        }
    }
}

#[story(description = "A non-visual control product.")]
pub(crate) fn control_product() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::control("dmx").with_detail("24 channels") }
    }
}
