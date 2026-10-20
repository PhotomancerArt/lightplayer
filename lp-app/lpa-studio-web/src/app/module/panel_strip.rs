//! [`PanelStrip`]: one control across the connected board card's panel — a
//! fader or a palette, its label and its value on a row above a track that
//! keeps the line's whole width (the strip shape,
//! [`lpa_studio_core::UiPanelShape::Strip`]; the panel layout study,
//! `docs/ux/studies/panel-layout/`).
//!
//! The label row keeps both of its ends free, so the strip's corner actions
//! never move the label or the value. The widgets are the production ones:
//! a fader's track is [`HFaderField`] at the strip's width (a press on it
//! jumps there), a palette's is [`PaletteSwatchField`]'s band (it opens the
//! chooser).
//!
//! Walk hooks, on the strip: `data-panel-scope`, `data-panel-channel` and
//! `data-panel-state` (as [`super::ModulePanelControl`] carries them).

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiCardControl, UiPanelControl, UiPanelControlState, UiPanelControlView,
    UiPanelWidget,
};

use crate::app::node::{HFaderField, PaletteSwatchField, PanelEmit};

use super::PanelGesture;
use super::module_panel_control::mismatch;
use super::panel_label_trigger::{PanelLabelTrigger, label_visual};
use super::panel_readout::PanelReadout;
use super::panel_state_look::{
    panel_state_hook, panel_state_label_class, panel_state_readout_class,
};

static NEXT_PANEL_STRIP_ID: AtomicUsize = AtomicUsize::new(1);

/// One strip: the label row, then the track at the strip's whole width.
pub(crate) const STRIP_CLASS: &str =
    "ux-panel-cell tw:col-span-full tw:grid tw:min-w-0 tw:gap-0.5 tw:px-2 tw:pt-1 tw:pb-2";

/// The strip's live copy over its anchor while its details are open.
const STRIP_COPY_CLASS: &str = "tw:grid tw:h-full tw:w-full tw:min-w-0 tw:content-start tw:gap-0.5 tw:px-2 tw:pt-1 tw:pb-2";

/// The label row: the label at its start, the value at its end, on the
/// corners' line; both ends kept free for the corners (24 px each, 3 px in
/// from the strip's edge), whether or not one shows.
pub(crate) const STRIP_LABEL_ROW_CLASS: &str = "tw:flex tw:h-6 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:px-5 tw:[&_button]:max-w-full";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelStrip(
    /// The control and the scope it lives in.
    control: UiCardControl,
    /// The panel's gesture seam (let go).
    on_panel: EventHandler<PanelGesture>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let UiCardControl { scope, view } = control;
    let aspects = view.detail_aspects(&scope);
    let UiPanelControlView {
        channel,
        control,
        state,
        source: _,
    } = view;
    let label_class = panel_state_label_class(state);
    // The whole strip is the details' outline anchor.
    let anchor_id = use_hook(|| {
        let id = NEXT_PANEL_STRIP_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-panel-strip-{id}")
    });
    let label = rsx! {
        PanelLabelTrigger {
            label: control.label.clone(),
            aspects,
            color_class: label_class,
            anchor_id: anchor_id.clone(),
            anchor_visual: rsx! {
                div { class: STRIP_COPY_CLASS,
                    StripFace {
                        control: control.clone(),
                        state,
                        label: label_visual(&control.label, label_class),
                        on_action,
                    }
                }
            },
            engaged: state.engaged(),
            target: control.panel_target.clone(),
            on_panel,
            on_action,
        }
    };
    rsx! {
        div {
            id: "{anchor_id}",
            class: STRIP_CLASS,
            "data-panel-scope": "{scope}",
            "data-panel-channel": "{channel}",
            "data-panel-state": panel_state_hook(state),
            StripFace { control, state, label, on_action }
        }
    }
}

/// What a strip draws: the label row (label slot, value), then the track.
/// Rendered in the strip (label slot = the details trigger) and as its
/// top-layer copy (label slot = a static copy).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StripFace(
    control: UiPanelControl,
    state: UiPanelControlState,
    label: Element,
    on_action: EventHandler<UiAction>,
) -> Element {
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);
    let on_action = Some(on_action);
    let label_row = rsx! {
        div { class: STRIP_LABEL_ROW_CLASS,
            {label}
            PanelReadout {
                control: control.clone(),
                color_class: panel_state_readout_class(state),
            }
        }
    };
    match control.widget.clone() {
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_control(&control) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                {label_row}
                HFaderField {
                    value,
                    live_value: control.live_numeric(),
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    emit,
                    on_action,
                }
            }
        }
        UiPanelWidget::PaletteSwatch => {
            let Some(config) = control.shown_palette() else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                {label_row}
                PaletteSwatchField {
                    config,
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    on_action,
                }
            }
        }
        // A cell or the block never lands in a strip: the card lays out by
        // shape. Should one, it reads rather than misbehaves.
        UiPanelWidget::Knob { .. }
        | UiPanelWidget::Speed
        | UiPanelWidget::Toggle
        | UiPanelWidget::PatternPicker { .. } => mismatch(&control.label, &control.value.display),
    }
}
