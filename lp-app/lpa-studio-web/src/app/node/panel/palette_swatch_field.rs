//! The palette swatch: a panel control's CLOSED face (M4 P3).
//!
//! The widget every other panel field is not — no range, no scalar gesture,
//! and a value that is a whole [`GradientConfig`]. What it draws is
//! mode-adaptive, because the two palette modes are two different facts:
//!
//! - a **held** palette is one full-width strip — that is the palette,
//!   completely;
//! - a **cycle** is its member SET as equal segments of one band, so the
//!   control says "these, in turn" at a glance. The rate rides the
//!   control's readout chip (`↻ 4 · 3/min`, auto-denominated like every
//!   other periodic reading in Studio), not the band.
//!
//! The band carries the chevron that says a chooser lives behind it. In
//! this phase the chooser does not exist yet, so the band is deliberately
//! **not interactive**: a cursor that promises a click and does nothing is
//! worse than a picture. P4 turns the band into the popover trigger.
//!
//! The live member ring — highlighting which member of a running cycle is
//! showing right now — is NOT here, and it is not a styling omission: a
//! panel control has no phase reading in hand. A driven palette's live
//! value arrives as a formatted SUMMARY STRING
//! ([`lpa_studio_core::format_live_panel_value`]), the timebase φ lives on
//! the clock face's own probe (`UiPhasorReading`), and nothing on the panel
//! path carries either. Adding the ring means plumbing a φ read onto
//! `UiPanelControl` first.
//!
//! Colors are the existing panel families, unchanged: violet when the
//! backing slot is bound, amber when a panel writer holds the channel
//! (`docs/design/panel.md` P6 — the engaged family is decided at the P6
//! gate, and this phase mints nothing new).

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, ToLpValue, UiAction, UiPanelTarget, UiSlotFieldState,
};
use lpc_model::GradientConfig;

use crate::app::node::GradientStripBand;
use crate::app::node::slot_edit_actions::panel_or_slot_action;
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaletteSwatchField(
    /// The palette this control presents — the authored config, even when a
    /// channel is driving the slot (a live reading is text, not a config
    /// this could sample).
    config: GradientConfig,
    state: UiSlotFieldState,
    /// Violet bound treatment on the frame.
    #[props(default = false)]
    bound: bool,
    /// Amber ENGAGED treatment: a panel writer has captured this channel.
    /// Outranks the violet bound family, same rule as every other field.
    #[props(default = false)]
    engaged: bool,
) -> Element {
    let invalid_title = state.invalid.clone().unwrap_or_default();
    rsx! {
        div {
            class: "tw:relative tw:flex tw:min-w-0 tw:items-stretch tw:rounded-sm tw:border tw:p-0.5 tw:pr-5 {swatch_frame_class(&state, bound, engaged)}",
            title: "{invalid_title}",
            div { class: "tw:min-w-0 tw:grow", GradientStripBand { config } }
            // The "a chooser lives here" caret. Non-interactive in P3 —
            // P4 makes the whole frame the popover trigger.
            span {
                class: "tw:pointer-events-none tw:absolute tw:right-0.5 tw:top-1/2 tw:inline-flex tw:-translate-y-1/2 tw:text-subtle-foreground",
                StudioIcon { name: StudioIconName::Expanded, size: 12 }
            }
        }
    }
}

/// The frame's border family: amber when a panel writer holds the channel,
/// violet when the slot is bound, error when invalid, the ordinary strong
/// border otherwise. Same ladder (and the same tokens) as the fader's slot.
pub(crate) fn swatch_frame_class(
    state: &UiSlotFieldState,
    bound: bool,
    engaged: bool,
) -> &'static str {
    if engaged {
        "tw:border-[var(--studio-status-attention-border)]"
    } else if bound {
        "tw:border-[var(--studio-status-bound-border)]"
    } else if state.invalid.is_some() {
        "tw:border-[var(--studio-status-error-border)]"
    } else {
        "tw:border-[var(--studio-color-border-strong)]"
    }
}

/// The action a palette PICK dispatches: the whole config, down whichever
/// path the control's publicity selects — a `PanelWriteOp` on the config
/// channel when the slot consumes one (every reader of that channel takes
/// the config whole), an ordinary `SlotEditOp::SetValue` at
/// `consumed[<name>].gradient.some` when it does not.
///
/// No partial-field carry, unlike a phasor's period: a pick REPLACES the
/// palette, so there is nothing of the old config to preserve.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "P4's chooser is the only caller; the tests below pin both dispatch branches"
    )
)]
pub(crate) fn palette_write_action(
    panel_target: &Option<UiPanelTarget>,
    address: ProjectSlotAddress,
    config: &GradientConfig,
) -> UiAction {
    panel_or_slot_action(panel_target, address, palette_lp_value(config))
}

/// The `LpValue` a palette gesture carries — the model's own storage, never
/// a shape rebuilt in the UI layer.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "reached through palette_write_action, which P4 calls"
    )
)]
pub(crate) fn palette_lp_value(config: &GradientConfig) -> LpValue {
    config.to_lp_value()
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        PanelWriteOp, ProjectNodeAddress, ProjectSlotRoot, SlotEditOp, SlotPath,
    };
    use lpc_model::{Colorspace, Gradient, GradientStop, InterpMethod};

    use super::*;

    fn ramp() -> Gradient {
        Gradient {
            space: Colorspace::Oklab,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [0.9, 0.1, 0.1],
                },
            ],
        }
    }

    fn address() -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/show.module/aurora.shader").expect("node address"),
            ProjectSlotRoot::def(),
            SlotPath::parse("consumed[palette].gradient.some").expect("slot path"),
        )
    }

    fn target() -> Option<UiPanelTarget> {
        Some(UiPanelTarget {
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpa_studio_core::NodeId::new(1),
            },
            channel: "palette".to_string(),
            engaged: false,
        })
    }

    /// A public palette writes the WHOLE config onto its channel — the
    /// payload the engine's `resolve_gradient_config` takes whole.
    #[test]
    fn a_public_palette_pick_writes_the_config_onto_its_channel() {
        let config = GradientConfig::Cycle {
            set: vec![ramp(), ramp()],
            step_seconds: 20.0,
            fade_seconds: 0.5,
        };
        let action = palette_write_action(&target(), address(), &config);

        let write = action.op_as::<PanelWriteOp>().expect("a panel write");
        assert_eq!(write.channel, "palette");
        assert_eq!(write.ttl_ms, None);
        assert_eq!(
            write.value,
            config.to_lp_value(),
            "the config rides whole, never field by field"
        );
    }

    /// A slot-local palette edits its own config slot instead.
    #[test]
    fn a_slot_local_palette_pick_edits_the_gradient_slot() {
        let config = GradientConfig::Static(ramp());
        let action = palette_write_action(&None, address(), &config);

        let Some(SlotEditOp::SetValue {
            address: edited,
            value,
        }) = action.op_as::<SlotEditOp>()
        else {
            panic!("expected a slot SetValue, got {action:?}");
        };
        assert_eq!(edited.path.to_string(), "consumed[palette].gradient.some");
        assert_eq!(*value, config.to_lp_value());
    }

    #[test]
    fn the_frame_wears_the_existing_panel_families_and_never_green() {
        let clean = UiSlotFieldState::editable();
        assert!(swatch_frame_class(&clean, false, false).contains("border-strong"));
        assert!(swatch_frame_class(&clean, true, false).contains("bound"));
        // Engaged outranks bound, and green stays valid-only.
        let engaged = swatch_frame_class(&clean, true, true);
        assert!(engaged.contains("attention"));
        assert!(!engaged.contains("bound"));
        for family in [
            swatch_frame_class(&clean, false, false),
            swatch_frame_class(&clean, true, false),
            engaged,
            swatch_frame_class(&clean.clone().with_invalid("bad stops"), false, false),
        ] {
            assert!(!family.contains("good"), "green is valid-only: {family}");
        }
    }
}
