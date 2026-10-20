//! One control on a module's panel, wearing its panel state.
//!
//! **A control is a widget, a label, and a value. Nothing else.** This is a
//! control panel, not a spec sheet: the provenance captions that used to
//! sit under every control ("control · Master speed", "held · was inherited
//! · …", "authored default") are gone from the face and live in the
//! control's detail popup instead, behind its label.
//!
//! State is carried entirely by color and by the presence of the reset
//! glyph — the three states `docs/design/panel.md` P-Q2 requires to be
//! visibly distinct:
//!
//! | state | widget + label + value | reset glyph |
//! |---|---|---|
//! | Read, at default | neutral arc at the authored default, subtle label | absent |
//! | Read, following | **violet** arc at the LIVE value, violet label | absent |
//! | Engaged (Latch) | **gold** arc + body ring at the HELD value, gold label | present |
//!
//! In every state the face carries exactly ONE number
//! ([`UiPanelControl::shown_display`]): the live reading when the channel
//! has one, the authored value when it does not. The authored value it
//! displaces is a row in the detail popup, not a parenthetical beside it.
//!
//! Gold (the `status-engaged` family, minted at the M4 P6 gate) is the
//! engaged treatment: laddered like every status family, it is not violet
//! (bound means *wired*, engaged means *captured* — P6), not green (valid
//! only), not the blue live family (transient edits), and deliberately not
//! attention-orange — that stays device/roster health.
//!
//! The **label is the detail trigger** and the let-go glyph hangs off it
//! while engaged ([`PanelLabelTrigger`]): the whole control is the
//! popover's outline anchor, so opening merges the control's outline into
//! the aspect card and reads as diving into it, and a live top-layer copy
//! of the control paints over the anchor while the popup is open. Sections
//! come from [`UiPanelControlView::detail_aspects`].
//!
//! Values render in **tabular numerals inside a reserved slot**
//! ([`PanelReadout`]), because a panel whose row reflows while you drag a
//! fader is not a control panel.
//!
//! **Walk hooks**, on the control wherever a panel draws it (the editor,
//! the play page, the board card): `data-panel-scope` and
//! `data-panel-channel` (its identity, panel.md P1) and `data-panel-state`
//! (`read-default`, `read-following`, `engaged`). A walk reads a control
//! written in one session as held in the next — the board's own word, since
//! a fresh session has no local echo.
//!
//! The connected board card lays its panel out by shape with its own
//! pieces ([`super::PanelCell`], [`super::PanelStrip`]); this control draws
//! the editor's and the play page's panel, and the card's block (the
//! Pattern instrument).

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelWidget,
    UiSlotValueKind,
};

use crate::app::node::{HFaderField, KnobField, PaletteSwatchField, PanelEmit, ToggleField};

use super::PanelGesture;
use super::panel_label_trigger::{PanelLabelTrigger, label_visual};
use super::panel_readout::PanelReadout;
use super::panel_state_look::{
    panel_state_hook, panel_state_label_class, panel_state_readout_class,
};

static NEXT_MODULE_CONTROL_ID: AtomicUsize = AtomicUsize::new(1);

/// A fader's fixed footprint. Without it the control's width tracks its
/// value string and the whole row shifts mid-drag.
const FADER_CLASS: &str = "tw:grid tw:w-[184px] tw:flex-none tw:gap-1";

/// An instrument's footprint (the Pattern instrument): it takes the panel
/// row's full width and wraps onto its own line rather than sharing one
/// with the knobs.
const INSTRUMENT_CLASS: &str = "tw:grid tw:min-w-0 tw:basis-full tw:gap-1.5";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModulePanelControl(
    /// The channel's control view: widget payload plus panel state.
    view: UiPanelControlView,
    /// The scope this control lives in — the other half of its identity
    /// (panel.md P1), what a reset gesture carries, and what the detail
    /// popup names.
    scope: String,
    /// Play mode renders the same control at a roomier size.
    #[props(default = false)]
    play: bool,
    /// Panel gestures (reset). Absent = the control is display-only.
    #[props(default = None)]
    on_panel: Option<EventHandler<PanelGesture>>,
    /// Open this control's detail popup on first render (stories).
    #[props(default = false)]
    detail_initially_open: bool,
    /// Widget value writes: a control carrying a `panel_target` dispatches
    /// `PanelWriteOp` (panel.md P8); an unbound one falls back to the
    /// existing slot-edit path.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let aspects = view.detail_aspects(&scope);
    let UiPanelControlView {
        // The channel rides inside `control.panel_target` for dispatch;
        // the bare name is the map key upstream, and the walk hook here.
        channel,
        control,
        state,
        source: _,
    } = view;
    let state_hook = panel_state_hook(state);
    let engaged = state.engaged();
    let label_class = panel_state_label_class(state);
    let readout_class = panel_state_readout_class(state);

    // The whole control is the popover's outline anchor (P2c item 3).
    let anchor_id = use_hook(|| {
        let id = NEXT_MODULE_CONTROL_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-module-control-{id}")
    });

    // The WIDE controls: a fader and a palette swatch both want a fixed,
    // roomy footprint with the label above, where a knob or a toggle is a
    // narrow column.
    let is_wide = matches!(
        control.widget,
        UiPanelWidget::Fader { .. } | UiPanelWidget::PaletteSwatch
    );
    // The Pattern instrument is wider still: a whole instrument, not a
    // control in a column. It takes the panel row's full width and wraps
    // below the narrow controls at phone widths.
    let is_instrument = matches!(control.widget, UiPanelWidget::PatternPicker { .. });
    let column_class = if is_instrument {
        INSTRUMENT_CLASS
    } else if is_wide {
        FADER_CLASS
    } else if play {
        "tw:flex tw:min-w-[76px] tw:flex-none tw:flex-col tw:items-center tw:gap-1.5"
    } else {
        "tw:flex tw:min-w-[64px] tw:flex-none tw:flex-col tw:items-center tw:gap-1"
    };
    let anchor_class = if is_instrument {
        "tw:grid tw:h-full tw:w-full tw:content-start tw:gap-1.5"
    } else if is_wide {
        "tw:grid tw:h-full tw:w-full tw:content-start tw:gap-1"
    } else {
        "tw:flex tw:h-full tw:w-full tw:flex-col tw:items-center tw:gap-1"
    };

    let label = rsx! {
        PanelLabelTrigger {
            label: control.label.clone(),
            aspects,
            color_class: label_class,
            anchor_id: anchor_id.clone(),
            anchor_visual: rsx! {
                div { class: anchor_class,
                    ModulePanelControlBody {
                        control: control.clone(),
                        state,
                        readout_class,
                        label: label_visual(&control.label, label_class),
                        on_action,
                    }
                }
            },
            initially_open: detail_initially_open,
            engaged,
            target: control.panel_target.clone(),
            on_panel,
            on_action,
        }
    };

    rsx! {
        div {
            id: "{anchor_id}",
            class: column_class,
            "data-panel-scope": "{scope}",
            "data-panel-channel": "{channel}",
            "data-panel-state": state_hook,
            ModulePanelControlBody {
                control,
                state,
                readout_class,
                label,
                on_action,
                on_panel,
            }
        }
    }
}

/// The control itself — widget, label slot, value — with no popover wiring.
/// Rendered on the face (label slot = the trigger button) and as the
/// popup's top-layer anchor copy (label slot = a static copy).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ModulePanelControlBody(
    control: UiPanelControl,
    state: UiPanelControlState,
    readout_class: &'static str,
    /// The label element: the interactive trigger in flow, a static copy in
    /// the top-layer visual.
    label: Element,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// Panel gestures (reset) for an instrument that carries its own resets
    /// (the Pattern instrument); `None` on the popup's top-layer copy.
    #[props(default)]
    on_panel: Option<EventHandler<PanelGesture>>,
) -> Element {
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);

    // A phasor speed knob drags on an inverted axis (up = faster) — same
    // treatment as the node-face control (G2 feedback).
    let phasor = matches!(
        control.emit,
        lpa_studio_core::UiPanelEmit::PhasorPeriod { .. }
    );
    let palette = control.shown_palette();
    let readout = rsx! {
        PanelReadout { control: control.clone(), color_class: readout_class }
    };

    match control.widget.clone() {
        UiPanelWidget::Knob { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_control(&control) else {
                return mismatch(&control.label, &control.value.display);
            };
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
                    invert: phasor,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    emit,
                    on_action,
                }
                {label}
                {readout}
            }
        }
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_control(&control) else {
                return mismatch(&control.label, &control.value.display);
            };
            let fader = rsx! {
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
            };
            rsx! {
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                    {label}
                    {readout}
                }
                {fader}
            }
        }
        UiPanelWidget::PaletteSwatch => {
            let Some(config) = palette else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
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
        // The clock's Speed knob: a knob over the rate, its own mapping
        // (log ¼×–8×, 1× at the top, the octave stops), its value said as a
        // speed (`2×`).
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
        // The playlist's Pattern instrument: the group's legend already
        // names it, and it carries its own per-channel resets, so it takes
        // no label row and no readout.
        UiPanelWidget::PatternPicker { picker } => {
            rsx! {
                super::PatternPicker { picker, on_action, on_panel }
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
    }
}

/// Read-only fallback when the widget family and the value family disagree
/// (the card's cells and strips use it too).
pub(super) fn mismatch(label: &str, display: &str) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-[64px] tw:flex-none tw:flex-col tw:items-center tw:gap-1",
            span { class: "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground",
                "{label}"
            }
            span { class: "tw:font-mono tw:text-[0.7rem] tw:tabular-nums tw:text-muted-foreground",
                "{display}"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit, UiPanelWidget,
        UiSlotAspectKind, UiSlotFieldState, UiSlotValue,
    };

    fn view(state: UiPanelControlState, source: Option<&str>) -> UiPanelControlView {
        UiPanelControlView::new(
            "speed",
            UiPanelControl {
                emit: UiPanelEmit::Value,
                label: "speed".to_string(),
                address: None,
                widget: UiPanelWidget::Knob {
                    min: 0.0,
                    max: 1.0,
                    step: None,
                },
                value: UiSlotValue::f32(0.5),
                live_value: None,
                live_gradient: None,
                panel_target: None,
                unit: None,
                state: UiSlotFieldState::editable(),
                aspects: Vec::new(),
            },
        )
        .with_state(state, source)
    }

    /// GV fix 3: one value on the face, in every state — the live reading
    /// when the channel has one, the authored value when it does not.
    #[test]
    fn the_readout_shows_the_live_reading_in_every_state() {
        for state in [
            UiPanelControlState::ReadFollowing,
            UiPanelControlState::Engaged,
            UiPanelControlState::ReadDefault,
        ] {
            let mut view = view(state, None);
            view.control.live_value = Some("2.72".to_string());
            assert_eq!(
                view.control.shown_display(),
                "2.72",
                "the live reading leads in {state:?}"
            );
            assert_eq!(
                view.control.live_numeric(),
                Some(2.72),
                "and the widget geometry follows it in {state:?}"
            );
        }
        // No live reading: the authored value is the one value shown.
        assert_eq!(
            view(UiPanelControlState::ReadDefault, None)
                .control
                .shown_display(),
            "0.5"
        );
    }

    /// The captions left the face — so everything they said has to be in
    /// the popup, or the information is simply gone.
    #[test]
    fn provenance_moved_into_the_detail_popup() {
        let held = view(UiPanelControlState::Engaged, Some("lfo · speed"));
        let aspects = held.detail_aspects("/aurora.module");
        assert_eq!(aspects[0].kind, UiSlotAspectKind::PanelState);
        assert_eq!(aspects[0].title, "Held");
        // What the held value displaced — the old "held · was …" caption.
        assert!(
            aspects[0]
                .rows
                .iter()
                .any(|row| row.value.contains("lfo · speed"))
        );
        // The identity behind every reset gesture (P1).
        assert!(
            aspects[1]
                .rows
                .iter()
                .any(|row| row.value == "/aurora.module")
        );

        let following = view(UiPanelControlState::ReadFollowing, Some("lfo · hue"));
        assert_eq!(following.detail_aspects("/x")[0].title, "Following");
        let quiet = view(UiPanelControlState::ReadDefault, None);
        assert_eq!(quiet.detail_aspects("/x")[0].title, "At default");
    }
}
