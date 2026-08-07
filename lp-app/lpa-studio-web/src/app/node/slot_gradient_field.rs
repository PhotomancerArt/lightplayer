//! Palette display for `Gradient`-hinted slot values — the strip, not
//! `Gradient { space: 1, method: 1, count: 3, stops: [...] }`.
//!
//! [`GradientSlotField`] is the config-slot-row end of the `Power` five-step
//! template (hint declared → bridged → dispatched by
//! [`SlotValueEditor`](crate::app::node::SlotValueEditor) → rendered here →
//! registered for option-presence width), and [`GradientValueDisplay`] is
//! the shared body every other read surface reuses (the wiring drawer's
//! value box, probe rows).
//!
//! **Read-only in this phase (M4 P2).** Picking, editing, and the live
//! cross-fade a running cycle performs are the panel widget's job (P3), so a
//! cycle here renders its member SET as mini strips — a read surface states
//! what the value IS, and a static picture of a blend the engine is halfway
//! through would be a lie about a value nobody can scrub.

use dioxus::prelude::*;
use lpa_studio_core::app::project::{format_gradient_summary, gradient_config_value};
use lpa_studio_core::{UiSlotFieldState, UiSlotValueKind};
use lpc_model::{Gradient, GradientConfig};

use crate::base::GradientStripCanvas;

/// How much room the surface gives a palette.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GradientDisplayDensity {
    /// A slot row or probe stat: full-width strip with the summary line
    /// under it.
    #[default]
    Row,
    /// A wiring-drawer value box: the same strip in a small box, with the
    /// summary shrunk to the box's own caption size.
    Compact,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GradientSlotField(kind: UiSlotValueKind, state: UiSlotFieldState) -> Element {
    let Some(config) = gradient_parts(&kind) else {
        return rsx! {};
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        div {
            class: gradient_field_class(&state),
            title: "{invalid_title}",
            GradientValueDisplay { config }
        }
    }
}

/// The palette itself: one strip for a held palette, the member set as mini
/// strips for a cycle, and one dense summary line under either.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GradientValueDisplay(
    config: GradientConfig,
    #[props(default)] density: GradientDisplayDensity,
) -> Element {
    let summary = format_gradient_summary(&config);

    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:grow tw:flex-col tw:gap-1",
            GradientStripBand { config }
            span { class: gradient_summary_class(density), "{summary}" }
        }
    }
}

/// The palette's PICTURE, with no words under it: one strip for a held
/// palette, and a cycle's members as equal segments of one band — the set
/// reads as a set at any width, and the row keeps the height of a single
/// strip.
///
/// Shared by the read surfaces' [`GradientValueDisplay`] and the panel's
/// swatch control (M4 P3), which puts its own compact chip in the control's
/// readout slot instead of a summary line.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GradientStripBand(config: GradientConfig) -> Element {
    let gradients: Vec<Gradient> = config.gradients().to_vec();
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-stretch tw:gap-0.5",
            for gradient in gradients {
                div { class: "tw:min-w-0 tw:grow tw:basis-0",
                    GradientStripCanvas { gradient }
                }
            }
        }
    }
}

/// The palette a slot value holds, or `None` when the value is not
/// gradient-shaped — the dispatch guard, mirroring `power_parts`.
///
/// The UI kind is handed back to `lpc-model`'s own parser (via
/// [`UiSlotValueKind::to_lp_value`]) rather than walked field by field here:
/// the storage recipe — tag numbering, the padded fixed-size arrays, the
/// `count` bound — belongs to the model, and a second reading of it in the
/// UI would be a copy that drifts.
pub(crate) fn gradient_parts(kind: &UiSlotValueKind) -> Option<GradientConfig> {
    gradient_config_value(&kind.to_lp_value())
}

/// The field box around a slot-row palette. Unlike the numeric fields this
/// stretches (a strip wants the row's width) and carries no inner padding on
/// the strip itself; the invalid/editable/read-only tinting is the same
/// family every other field wears.
fn gradient_field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:flex tw:min-w-0 tw:grow tw:items-stretch tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1"
    } else if state.editable {
        "tw:flex tw:min-w-0 tw:grow tw:items-stretch tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1"
    } else {
        "tw:flex tw:min-w-0 tw:grow tw:items-stretch tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1"
    }
}

fn gradient_summary_class(density: GradientDisplayDensity) -> &'static str {
    match density {
        GradientDisplayDensity::Row => {
            "tw:min-w-0 tw:truncate tw:text-xs tw:text-subtle-foreground"
        }
        GradientDisplayDensity::Compact => {
            "tw:min-w-0 tw:truncate tw:text-center tw:text-[10px] tw:text-subtle-foreground"
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiSlotValue;
    use lpc_model::{Colorspace, GradientStop, InterpMethod, ToLpValue};

    use super::*;

    fn ramp(stops: usize) -> Gradient {
        Gradient {
            space: Colorspace::Oklab,
            method: InterpMethod::Linear,
            stops: (0..stops)
                .map(|index| GradientStop {
                    at: index as f32 / (stops - 1) as f32,
                    c: [index as f32 / stops as f32, 0.1, -0.1],
                })
                .collect(),
        }
    }

    /// The dispatch guard accepts both storage forms and refuses everything
    /// else, so a mis-hinted slot falls back to the generic display instead
    /// of rendering an empty strip.
    #[test]
    fn guard_accepts_gradient_shapes_and_refuses_others() {
        let held = UiSlotValue::from_lp_value(&ramp(3).to_lp_value());
        assert_eq!(
            gradient_parts(&held.kind),
            Some(GradientConfig::Static(ramp(3)))
        );

        let cycle = GradientConfig::Cycle {
            set: vec![ramp(2), ramp(3)],
            step_seconds: 4.0,
            fade_seconds: 0.25,
        };
        let value = UiSlotValue::from_lp_value(&cycle.to_lp_value());
        assert_eq!(gradient_parts(&value.kind), Some(cycle));

        assert_eq!(gradient_parts(&UiSlotValue::f32(0.5).kind), None);
        assert_eq!(
            gradient_parts(
                &UiSlotValue::struct_value(
                    Some("Dim2u".to_string()),
                    vec![("width".to_string(), UiSlotValue::u32(16))],
                )
                .kind
            ),
            None
        );
    }

    /// Every member of a cycle gets a strip — the read surface shows the
    /// SET, never a blend of it.
    #[test]
    fn a_cycle_displays_every_member() {
        let config = GradientConfig::Cycle {
            set: vec![ramp(2), ramp(3), ramp(4)],
            step_seconds: 4.0,
            fade_seconds: 0.25,
        };
        assert_eq!(config.gradients().len(), 3);
        assert_eq!(GradientConfig::Static(ramp(3)).gradients().len(), 1);
    }
}
