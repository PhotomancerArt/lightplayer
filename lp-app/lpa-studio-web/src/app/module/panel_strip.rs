//! [`PanelStrip`]: one control across the connected board card's panel — a
//! fader or a palette, its label and its value on a row above a track that
//! keeps the line's whole width (the strip shape,
//! [`lpa_studio_core::UiPanelShape::Strip`]; the panel layout study,
//! `docs/ux/studies/panel-layout/`).
//!
//! **The whole strip is the control's target** (the guide's proposed P21).
//! A fader: a press on its track jumps there ([`HFaderField`]'s native
//! input), and a drag that starts anywhere else in the strip moves the
//! value sideways from where it was, the track's width the whole range; a
//! vertical swipe still scrolls the page. A palette: its band opens the
//! chooser ([`PaletteSwatchField`]). On either, a tap on the label row
//! (a press that never moves) opens the control's details. The strip
//! lightens under the pointer.
//!
//! Its two small actions ([`PanelCorners`]) sit at the ends of the label
//! row: let go (↺) before the label while held, details (ⓘ) after the value
//! on hover or focus. The row keeps both ends free whether or not a circle
//! shows, so neither ever moves the label or the value.
//!
//! Walk hooks, on the strip: `data-panel-scope`, `data-panel-channel` and
//! `data-panel-state` (as [`super::ModulePanelControl`] carries them); the
//! fader's `input[type="range"]` inside it is the one a walk turns.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiCardControl, UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelWidget,
};

use crate::app::node::{HFaderField, PaletteSwatchField, PanelEmit};

use super::PanelGesture;
use super::module_panel_control::mismatch;
use super::panel_cell::cell_class;
use super::panel_corners::PanelCorners;
use super::panel_label::panel_label;
use super::panel_readout::PanelReadout;
use super::panel_state_look::{
    panel_state_hook, panel_state_label_class, panel_state_readout_class,
};
use super::panel_story_look::use_panel_story_look;

static NEXT_PANEL_STRIP_ID: AtomicUsize = AtomicUsize::new(1);

/// One strip: the positioned box its corners float in, the grid's whole
/// row.
const STRIP_CLASS: &str = "ux-panel-cell tw:col-span-full tw:flex tw:min-w-0";

/// The strip's gesture surface: the whole strip, the label row over the
/// track. The label row lines up with the corners (3 px down, 24 px tall).
const STRIP_SURFACE_CLASS: &str =
    "tw:grid tw:w-full tw:min-w-0 tw:content-start tw:gap-0.5 tw:px-2 tw:pt-[3px] tw:pb-2";

/// The strip's live copy over its anchor while its details are open.
const STRIP_COPY_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0";

/// The label row: the label at its start, the value at its end, on the
/// corners' line; both ends kept free for the corners (24 px each, 3 px in
/// from the strip's edge), whether or not one shows.
const STRIP_LABEL_ROW_CLASS: &str =
    "tw:flex tw:h-6 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:px-5";

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
    let look = use_panel_story_look();
    // The details' open state: the ⓘ toggles it, a tap on the label row
    // opens it.
    let mut open = use_signal(|| look.opens(&scope, &channel));
    let on_tap = use_callback(move |()| open.set(true));
    // The whole strip is the details' outline anchor.
    let anchor_id = use_hook(|| {
        let id = NEXT_PANEL_STRIP_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-panel-strip-{id}")
    });
    let class = cell_class(STRIP_CLASS, look.hovers(&scope, &channel), open(), false);
    rsx! {
        div {
            id: "{anchor_id}",
            class,
            "data-panel-scope": "{scope}",
            "data-panel-channel": "{channel}",
            "data-panel-state": panel_state_hook(state),
            StripFace {
                control: control.clone(),
                state,
                on_action,
                on_tap: Some(on_tap),
            }
            PanelCorners {
                label: control.label.clone(),
                aspects,
                anchor_id: anchor_id.clone(),
                anchor_visual: rsx! {
                    div { class: STRIP_COPY_CLASS,
                        StripFace { control: control.clone(), state, on_action }
                    }
                },
                open,
                engaged: state.engaged(),
                target: control.panel_target.clone(),
                on_panel,
                on_action,
            }
        }
    }
}

/// What a strip draws: the label row (label, value), then the track, as
/// one gesture surface. Rendered in the strip (with its tap) and as its
/// top-layer copy while the details are open (without).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StripFace(
    control: UiPanelControl,
    state: UiPanelControlState,
    on_action: EventHandler<UiAction>,
    #[props(default)] on_tap: Option<EventHandler<()>>,
) -> Element {
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);
    let on_action = Some(on_action);
    let label = panel_label(&control.label, panel_state_label_class(state));
    let readout = rsx! {
        PanelReadout {
            control: control.clone(),
            color_class: panel_state_readout_class(state),
        }
    };
    match control.widget.clone() {
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_control(&control) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
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
                    body: rsx! {
                        div { class: STRIP_LABEL_ROW_CLASS,
                            {label}
                            {readout}
                        }
                    },
                    surface_class: STRIP_SURFACE_CLASS,
                    on_tap,
                }
            }
        }
        // A palette has no drag: its band is its chooser, and its label row
        // is the tap that opens its details.
        UiPanelWidget::PaletteSwatch => {
            let Some(config) = control.shown_palette() else {
                return mismatch(&control.label, &control.value.display);
            };
            let row_class = match on_tap {
                Some(_) => format!("{STRIP_LABEL_ROW_CLASS} tw:cursor-pointer"),
                None => STRIP_LABEL_ROW_CLASS.to_string(),
            };
            rsx! {
                div { class: "ux-panel-surface {STRIP_SURFACE_CLASS}", "data-panel-surface": "",
                    div {
                        class: row_class,
                        onclick: move |_| {
                            if let Some(on_tap) = on_tap {
                                on_tap.call(());
                            }
                        },
                        {label}
                        {readout}
                    }
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
        }
        // A cell or the block never lands in a strip: the card lays out by
        // shape. Should one, it reads rather than misbehaves.
        UiPanelWidget::Knob { .. }
        | UiPanelWidget::Speed
        | UiPanelWidget::Toggle
        | UiPanelWidget::PatternPicker { .. } => mismatch(&control.label, &control.value.display),
    }
}
