//! [`PanelCell`]: one control in a cell on the connected board card's panel
//! — a knob, the Speed knob, a stepped knob or a toggle, its label and its
//! value centred under it, in its state's colours (the cell shape,
//! [`lpa_studio_core::UiPanelShape::Cell`]; the panel layout study,
//! `docs/ux/studies/panel-layout/`).
//!
//! The card draws cells three to a line on a 300 px card, more on a wider
//! one; the light lines between them belong to the grid
//! ([`super::CompactPanel`]). The widgets are the production ones, with the
//! same gestures and writes as everywhere else in Studio.
//!
//! Walk hooks, on the cell: `data-panel-scope`, `data-panel-channel` and
//! `data-panel-state` (as [`super::ModulePanelControl`] carries them).

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiCardControl, UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit,
    UiPanelWidget, UiSlotValueKind,
};

use crate::app::node::{KnobField, PanelEmit, ToggleField};

use super::PanelGesture;
use super::module_panel_control::mismatch;
use super::panel_label_trigger::{PanelLabelTrigger, label_visual};
use super::panel_readout::PanelReadout;
use super::panel_state_look::{
    panel_state_hook, panel_state_label_class, panel_state_readout_class,
};

static NEXT_PANEL_CELL_ID: AtomicUsize = AtomicUsize::new(1);

/// One cell: the widget at the top, its label and its value under it,
/// centred; the top padding leaves the cell's corners free.
pub(crate) const CELL_CLASS: &str = "ux-panel-cell tw:flex tw:min-w-0 tw:flex-col tw:items-center tw:gap-[3px] tw:px-1 tw:pt-4 tw:pb-2.5 tw:[&>span]:max-w-full tw:[&_button]:max-w-full";

/// The cell's live copy over its anchor while its details are open: the
/// same column, filling the anchor's rect.
const CELL_COPY_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0 tw:flex-col tw:items-center tw:gap-[3px] tw:px-1 tw:pt-4 tw:pb-2.5";

/// The value slot in a cell sits on its own line box: no leading.
const CELL_READOUT_CLASS: &str = "tw:leading-none";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PanelCell(
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
    // The whole cell is the details' outline anchor.
    let anchor_id = use_hook(|| {
        let id = NEXT_PANEL_CELL_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-panel-cell-{id}")
    });
    let label = rsx! {
        PanelLabelTrigger {
            label: control.label.clone(),
            aspects,
            color_class: label_class,
            anchor_id: anchor_id.clone(),
            anchor_visual: rsx! {
                div { class: CELL_COPY_CLASS,
                    CellFace {
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
            class: CELL_CLASS,
            "data-panel-scope": "{scope}",
            "data-panel-channel": "{channel}",
            "data-panel-state": panel_state_hook(state),
            CellFace { control, state, label, on_action }
        }
    }
}

/// What a cell draws: the widget, then the label slot, then the value.
/// Rendered in the cell (label slot = the details trigger) and as its
/// top-layer copy (label slot = a static copy).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CellFace(
    control: UiPanelControl,
    state: UiPanelControlState,
    label: Element,
    on_action: EventHandler<UiAction>,
) -> Element {
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);
    let on_action = Some(on_action);
    let readout = rsx! {
        PanelReadout {
            control: control.clone(),
            color_class: panel_state_readout_class(state),
            class: CELL_READOUT_CLASS,
        }
    };
    match control.widget.clone() {
        UiPanelWidget::Knob { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_control(&control) else {
                return mismatch(&control.label, &control.value.display);
            };
            // A phasor's speed knob drags its felt speed (up = faster).
            let invert = matches!(control.emit, UiPanelEmit::PhasorPeriod { .. });
            rsx! {
                KnobField {
                    value,
                    live_value: control.live_numeric(),
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    invert,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    emit,
                    on_action,
                }
                {label}
                {readout}
            }
        }
        // The clock's Speed knob: its own mapping (log ¼×–8×, 1× at the
        // top), its value said as a speed.
        UiPanelWidget::Speed => {
            let Some((value, emit)) = PanelEmit::for_control(&control) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                KnobField {
                    value,
                    live_value: control.live_numeric(),
                    min: 0.25,
                    max: 8.0,
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    speed: true,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    emit,
                    on_action,
                }
                {label}
                {readout}
            }
        }
        UiPanelWidget::Toggle => {
            let UiSlotValueKind::Bool(value) = control.value.kind else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                span { class: "tw:flex tw:h-[46px] tw:items-center",
                    ToggleField {
                        value,
                        live_value: control.live_bool(),
                        state: control.state.clone(),
                        bound: following,
                        engaged,
                        address: control.address.clone(),
                        panel_target: control.panel_target.clone(),
                        on_action,
                    }
                }
                {label}
                {readout}
            }
        }
        // A strip or the block never lands in a cell: the card lays out by
        // shape. Should one, it reads rather than misbehaves.
        UiPanelWidget::Fader { .. }
        | UiPanelWidget::PaletteSwatch
        | UiPanelWidget::PatternPicker { .. } => mismatch(&control.label, &control.value.display),
    }
}
