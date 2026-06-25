//! Stories for produced value stat views.

use dioxus::prelude::*;
use lpa_studio_core::UiProducedValue;
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::produced_value_variants_fixture;
use crate::app::node::{ProducedValueView, ProducedValues};

#[story(description = "Produced values rendered as compact stat boxes.")]
pub(crate) fn overview() -> Element {
    rsx! {
        ProducedValues { values: produced_value_variants_fixture() }
    }
}

#[story(description = "A numeric produced value with a short unit detail.")]
pub(crate) fn numeric_stat() -> Element {
    rsx! {
        ProducedValueView { value: UiProducedValue::new("Entry time", "320").with_detail("s") }
    }
}

#[story(description = "A produced value with binding metadata available from the icon menu.")]
pub(crate) fn bound_stat() -> Element {
    let value = produced_value_variants_fixture().remove(2);

    rsx! {
        ProducedValueView { value }
    }
}
