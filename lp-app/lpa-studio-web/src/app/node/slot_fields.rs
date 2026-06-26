//! Basic field renderers for the first config slot editor slice.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotFieldState, UiSlotOption, UiSlotUnit};

use crate::app::node::SlotUnitSuffix;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StringSlotField(value: String, state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: field_class(&state), "{value}" }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IntSlotField(
    value: i32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            span { class: "tw:font-mono", "{value}" }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn UIntSlotField(
    value: u32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            span { class: "tw:font-mono", "{value}" }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FloatSlotField(
    value: f32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            span { class: "tw:font-mono", "{format_float(value)}" }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoolSlotField(value: bool, state: UiSlotFieldState) -> Element {
    let true_class = bool_option_class(&state, value);
    let false_class = bool_option_class(&state, !value);

    rsx! {
        span { class: "tw:inline-grid tw:grid-cols-2 tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:text-xs tw:font-bold",
            span { class: true_class, "true" }
            span { class: false_class, "false" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn Vec2SlotField(value: [f32; 2], state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: "tw:grid tw:min-w-0 tw:grid-cols-2 tw:gap-1",
            VectorComponentField { label: "x", value: value[0], state: state.clone() }
            VectorComponentField { label: "y", value: value[1], state }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn Vec3SlotField(value: [f32; 3], state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: "tw:grid tw:min-w-0 tw:grid-cols-3 tw:gap-1",
            VectorComponentField { label: "x", value: value[0], state: state.clone() }
            VectorComponentField { label: "y", value: value[1], state: state.clone() }
            VectorComponentField { label: "z", value: value[2], state }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DropdownSlotField(
    value: String,
    options: Vec<UiSlotOption>,
    state: UiSlotFieldState,
) -> Element {
    let label = options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.as_str())
        .unwrap_or(value.as_str());

    rsx! {
        span { class: field_class(&state),
            span { class: "tw:min-w-0 tw:truncate", "{label}" }
            span { class: "tw:text-subtle-foreground", "v" }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn XySlotField(value: [f32; 2], state: UiSlotFieldState) -> Element {
    let x = value[0].clamp(0.0, 1.0) * 100.0;
    let y = (1.0 - value[1].clamp(0.0, 1.0)) * 100.0;
    let point_style = format!("left: {x:.1}%; top: {y:.1}%;");
    let pad_class = xy_pad_class(&state);

    rsx! {
        span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
            span { class: pad_class,
                span { class: "tw:absolute tw:left-1/2 tw:top-0 tw:h-full tw:w-px tw:bg-border-muted" }
                span { class: "tw:absolute tw:left-0 tw:top-1/2 tw:h-px tw:w-full tw:bg-border-muted" }
                span { class: "tw:absolute tw:h-2 tw:w-2 tw:-translate-x-1/2 tw:-translate-y-1/2 tw:rounded-full tw:border tw:border-accent-border tw:bg-accent", style: "{point_style}" }
            }
            Vec2SlotField {
                value,
                state,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn VectorComponentField(label: &'static str, value: f32, state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            small { class: "tw:text-[0.64rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono", "{format_float(value)}" }
        }
    }
}

fn field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
    } else {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-subtle-foreground"
    }
}

fn numeric_field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
    } else {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-subtle-foreground"
    }
}

fn bool_option_class(state: &UiSlotFieldState, active: bool) -> &'static str {
    match (state.invalid.is_some(), active) {
        (true, true) => "tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-status-error-foreground",
        (true, false) => "tw:bg-page tw:px-2 tw:py-1 tw:text-subtle-foreground",
        (false, true) => "tw:bg-accent-bg tw:px-2 tw:py-1 tw:text-accent",
        (false, false) => "tw:bg-page tw:px-2 tw:py-1 tw:text-subtle-foreground",
    }
}

fn xy_pad_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:relative tw:h-14 tw:w-14 tw:flex-none tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg"
    } else {
        "tw:relative tw:h-14 tw:w-14 tw:flex-none tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page"
    }
}

fn format_float(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.3}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}
