//! Stories for produced value stat views.

use dioxus::prelude::*;
use lpa_studio_core::app::project::format_gradient_summary;
use lpa_studio_core::{UiProducedValue, UiSlotUnit};
use lpa_studio_web_story_macros::story;
use lpc_model::GradientConfig;

use crate::app::node::node_story_fixtures::{
    palette_cycle, produced_value_variants_fixture, sunset_gradient,
};
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
            value: UiProducedValue::new("Seconds", "123.435").with_unit(UiSlotUnit::seconds())
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

#[story(
    description = "Palette probe rows: a produced gradient reads as the strip plus its summary line, and a cycle as its member set — the composite branch would have listed space/method/count and 24 padded stops instead."
)]
pub(crate) fn gradient_stat() -> Element {
    rsx! {
        ProducedValues {
            values: vec![
                gradient_produced_value("Palette", GradientConfig::Static(sunset_gradient())),
                gradient_produced_value("Cycle", palette_cycle()),
            ],
        }
    }
}

/// A produced value carrying a palette, built the way the slot controller
/// builds one: the summary as the compact reading, the config for the strip.
fn gradient_produced_value(label: &str, config: GradientConfig) -> UiProducedValue {
    let mut value = UiProducedValue::new(label, format_gradient_summary(&config));
    value.gradient = Some(config);
    value
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
