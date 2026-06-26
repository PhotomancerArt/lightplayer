//! Stories for produced value stat views.

use dioxus::prelude::*;
use lpa_studio_core::{UiProducedValue, UiSlotUnit};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::produced_value_variants_fixture;
use crate::app::node::{ProducedValueView, ProducedValues};

#[story(description = "Produced values rendered as compact stat boxes.")]
pub(crate) fn gallery() -> Element {
    rsx! {
        ProducedValues { values: produced_value_variants_fixture() }
    }
}

#[story(description = "A numeric produced value with a short unit detail.")]
pub(crate) fn numeric_stat() -> Element {
    rsx! {
        ProducedValueView {
            value: UiProducedValue::new("Entry time", "3.33").with_unit(UiSlotUnit::seconds())
        }
    }
}

#[story(description = "A produced value with binding metadata available from the icon menu.")]
pub(crate) fn bound_stat() -> Element {
    let value = produced_value_variants_fixture().remove(2);

    rsx! {
        ProducedValueView { value }
    }
}

#[story(description = "An open produced value detail popup.")]
pub(crate) fn detail_popup() -> Element {
    let value = produced_value_variants_fixture().remove(2);

    rsx! {
        div { class: "tw:min-h-48",
            ProducedValueView {
                value,
                initially_open: true,
            }
        }
    }
}
