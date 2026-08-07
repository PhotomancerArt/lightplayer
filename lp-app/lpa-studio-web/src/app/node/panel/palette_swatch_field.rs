//! The palette swatch: a panel control's CLOSED face (M4 P3).
//!
//! The widget every other panel field is not — no range, no scalar gesture,
//! and a value that is a whole [`GradientConfig`]. What it draws is
//! mode-adaptive, because the two palette modes are two different facts:
//!
//! - a **held** palette is one full-width strip — that is the palette,
//!   completely;
//! - a **cycle** is its member SET as equal segments of one band, so the
//!   control says "these, in turn" at a glance. The step rides the
//!   control's readout chip (`↻ 4 · 20 s`, the P6 gate's plain-seconds
//!   Step voice), not the band.
//!
//! The band carries the chevron that says a chooser lives behind it, and
//! since P4 the band IS that chooser's trigger: the popover opens in
//! **anchored mode** (`base/popover.rs` `:86-98`) with the swatch FRAME as
//! the anchor, so the merged outline welds control and panel into one shape
//! — "diving into the control", the same join the label's detail popover
//! makes one level up.
//!
//! What the band draws is the palette that is PLAYING, not the authored one
//! ([`lpa_studio_core::UiPanelControl::shown_palette`]) — a driven palette
//! now reads back as a config, not only as the summary string the readout
//! prints. That is what lets a pick, which writes the panel channel rather
//! than the authored slot, show up on the control that made it.
//!
//! The live member ring — highlighting which member of a running cycle is
//! showing right now — is still NOT here, and it is not a styling omission:
//! a panel control has no PHASE reading in hand. The timebase φ lives on the
//! clock face's own probe (`UiPhasorReading`), and nothing on the panel path
//! carries it. Adding the ring means plumbing a φ read onto
//! `UiPanelControl` first.
//!
//! Colors are the panel families: violet when the backing slot is bound,
//! the gold `status-engaged` family when a panel writer holds the channel
//! (`docs/design/panel.md` P6; the family was minted at the M4 P6 gate).

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, ToLpValue, UiAction, UiPanelTarget, UiSlotFieldState,
};
use lpc_model::GradientConfig;

use crate::app::node::GradientStripBand;
use crate::app::node::slot_edit_actions::panel_or_slot_action;
use crate::base::{
    PopoverButton, PopoverPlacement, StudioIcon, StudioIconName, detail_popover_card_class,
};

use super::palette_chooser::{PaletteChooser, PaletteChooserTab, PaletteEditTarget};

static NEXT_SWATCH_ID: AtomicUsize = AtomicUsize::new(1);

/// The band's trigger button: no chrome of its own — the FRAME around it is
/// the visual, and the frame is also the popover's outline anchor.
const BAND_TRIGGER_CLASS: &str = "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:border-0 tw:bg-transparent tw:p-0 tw:text-left";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaletteSwatchField(
    /// The palette this control presents: the EFFECTIVE config — what the
    /// channel is playing when one drives the slot, the authored value
    /// otherwise. Every chooser gesture is expressed as a whole replacement
    /// of this, so it has to be the live one: derived from a stale value,
    /// each gesture would silently discard the one before it.
    config: GradientConfig,
    state: UiSlotFieldState,
    /// Violet bound treatment on the frame.
    #[props(default = false)]
    bound: bool,
    /// Gold ENGAGED treatment: a panel writer has captured this channel.
    /// Outranks the violet bound family, same rule as every other field.
    #[props(default = false)]
    engaged: bool,
    /// Backing slot a pick edits when the control is not public.
    #[props(default = None)]
    address: Option<ProjectSlotAddress>,
    /// Panel channel a pick writes when the control IS public.
    #[props(default = None)]
    panel_target: Option<UiPanelTarget>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// Open the chooser on first render (stories).
    #[props(default = false)]
    chooser_initially_open: bool,
    /// Force the chooser's opening tab (stories); by default it opens on
    /// whichever kind the config already is.
    #[props(default = None)]
    chooser_initial_tab: Option<PaletteChooserTab>,
    /// Open the chooser straight into the editor takeover (stories).
    #[props(default = None)]
    chooser_initial_edit: Option<PaletteEditTarget>,
) -> Element {
    let invalid_title = state.invalid.clone().unwrap_or_default();
    // The anchored-outline id: the swatch FRAME is the anchor, so the
    // merged outline grows out of the whole control, not the band's button.
    let anchor_id = use_hook(|| {
        let id = NEXT_SWATCH_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-palette-swatch-{id}")
    });

    // A swatch that cannot write is a PICTURE: no caret, no cursor, no
    // popover. P3's reason for keeping the band inert still holds wherever
    // the gesture would go nowhere — a read-only slot, or a surface with no
    // dispatch conduit (the wiring drawer's preview copies).
    if !chooser_is_reachable(&state, address.as_ref(), on_action.is_some()) {
        return rsx! {
            div {
                class: "tw:grid tw:min-w-0 tw:rounded-sm tw:border tw:p-0.5 {swatch_frame_class(&state, bound, engaged)}",
                title: "{invalid_title}",
                GradientStripBand { config }
            }
        };
    }

    rsx! {
        div {
            id: "{anchor_id}",
            class: "tw:grid tw:min-w-0 tw:rounded-sm tw:border tw:p-0.5 {swatch_frame_class(&state, bound, engaged)}",
            title: "{invalid_title}",
            PopoverButton {
                class: BAND_TRIGGER_CLASS.to_string(),
                open_class: BAND_TRIGGER_CLASS.to_string(),
                trigger: swatch_band_visual(&config),
                label: "Choose a palette".to_string(),
                title: "Choose a palette".to_string(),
                popup_class: detail_popover_card_class().to_string(),
                placement: PopoverPlacement::BottomMiddle,
                initially_open: chooser_initially_open,
                // The chooser is the control's own body unfolding, so it
                // wears exactly the control's width — a panel a few px
                // narrower than its anchor reads as a mistake.
                match_anchor_width: true,
                anchor_id: Some(anchor_id.clone()),
                // The top-layer copy of the control while open: the same
                // band inside the frame's own padding, laid out exactly as
                // the in-flow trigger lays it out (flex row, band + caret) —
                // a grid here stacks the caret BELOW the band.
                anchor_visual: rsx! {
                    div { class: "tw:flex tw:h-full tw:w-full tw:min-w-0 tw:items-center tw:gap-1 tw:p-0.5",
                        {swatch_band_visual(&config)}
                    }
                },
                PaletteChooser {
                    config,
                    address,
                    panel_target,
                    on_action,
                    initial_tab: chooser_initial_tab,
                    initial_edit: chooser_initial_edit,
                }
            }
        }
    }
}

/// The control's face: the strip band plus the caret that says a chooser
/// lives behind it. Rendered twice while the popover is open (in-flow
/// placeholder + top-layer copy), so it stays a plain function of the
/// config.
fn swatch_band_visual(config: &GradientConfig) -> Element {
    rsx! {
        span { class: "tw:min-w-0 tw:grow",
            GradientStripBand { config: config.clone() }
        }
        span { class: "tw:inline-flex tw:flex-none tw:text-subtle-foreground", aria_hidden: "true",
            StudioIcon { name: StudioIconName::Expanded, size: 12 }
        }
    }
}

/// Whether opening the chooser could actually change anything: the slot has
/// to be editable AND there has to be somewhere to send the write. A caret
/// over a dead gesture is worse than no caret (P3's rule for the inert
/// band, kept).
fn chooser_is_reachable(
    state: &UiSlotFieldState,
    address: Option<&ProjectSlotAddress>,
    has_conduit: bool,
) -> bool {
    state.editable && address.is_some() && has_conduit
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
        "tw:border-[var(--studio-status-engaged-border)]"
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
pub(crate) fn palette_write_action(
    panel_target: &Option<UiPanelTarget>,
    address: ProjectSlotAddress,
    config: &GradientConfig,
) -> UiAction {
    panel_or_slot_action(panel_target, address, palette_lp_value(config))
}

/// The `LpValue` a palette gesture carries — the model's own storage, never
/// a shape rebuilt in the UI layer.
fn palette_lp_value(config: &GradientConfig) -> LpValue {
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

    /// The caret only appears where a pick can land: a read-only slot, or
    /// one with no address, keeps P3's inert picture.
    #[test]
    fn the_chooser_is_only_promised_where_a_pick_can_land() {
        let editable = UiSlotFieldState::editable();
        assert!(chooser_is_reachable(&editable, Some(&address()), true));
        // Nowhere to write: a preview copy with no address, or a surface
        // with no dispatch conduit.
        assert!(!chooser_is_reachable(&editable, None, true));
        assert!(!chooser_is_reachable(&editable, Some(&address()), false));
        // Projected/derived values stay pictures.
        assert!(!chooser_is_reachable(
            &UiSlotFieldState::readonly(),
            Some(&address()),
            true
        ));
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
        assert!(engaged.contains("engaged"));
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
