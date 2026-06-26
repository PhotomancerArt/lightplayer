//! Stories for produced product views.

use dioxus::prelude::*;
use lpa_studio_core::{UiProducedProduct, UiProductPreview, UiProductTrackingState};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::{
    produced_product_variants_fixture, visual_error_product, visual_preview_product,
};
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

#[story(description = "A visual product that exists but is not being tracked.")]
pub(crate) fn visual_untracked() -> Element {
    rsx! {
        ProducedProductView { product: UiProducedProduct::visual("output").with_detail("64 x 36 preview") }
    }
}

#[story(description = "A visual product waiting for its first tracked preview.")]
pub(crate) fn visual_pending() -> Element {
    rsx! {
        ProducedProductView {
            product: UiProducedProduct::visual("output")
                .with_detail("64 x 36 preview")
                .with_preview(UiProductPreview::Pending)
                .with_tracking(UiProductTrackingState::Tracking)
        }
    }
}

#[story(description = "A visual product with loaded RGB preview bytes.")]
pub(crate) fn visual_loaded() -> Element {
    rsx! {
        ProducedProductView { product: visual_preview_product("output") }
    }
}

#[story(description = "A visual product with cached preview bytes that is not being tracked now.")]
pub(crate) fn visual_paused() -> Element {
    rsx! {
        ProducedProductView {
            product: visual_preview_product("output")
                .with_tracking(UiProductTrackingState::Paused)
        }
    }
}

#[story(description = "A visual product whose preview probe failed.")]
pub(crate) fn visual_error() -> Element {
    rsx! {
        ProducedProductView { product: visual_error_product("output") }
    }
}

#[story(description = "An open produced product detail popup.")]
pub(crate) fn detail_popup() -> Element {
    let product = produced_product_variants_fixture().remove(3);

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
