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
//! | Read, at default | accent arc at the authored default, subtle label | absent |
//! | Read, following | **violet** arc at the LIVE value, violet label | absent |
//! | Engaged (Latch) | **amber** arc + body ring, amber label | present |
//!
//! Amber (the `status-attention` family) is the spike's engaged proposal:
//! it is the one warm family Studio already owns, it is not violet (bound
//! means *wired*, engaged means *captured* — P6), not green (valid only),
//! and not the blue live family (transient edits). A dedicated
//! `status-engaged` token family is the eventual home.
//!
//! The **label is the detail trigger**, reusing the node face's
//! [`SlotDetailButton`] machinery verbatim: the whole control is the
//! popover's outline anchor, so opening merges the control's outline into
//! the aspect card and reads as diving into it, and a live top-layer copy
//! of the control paints over the anchor while the popup is open. Sections
//! come from [`UiPanelControlView::detail_aspects`].
//!
//! **Reset** (P2 clear) stays on the face: the small revert glyph sits
//! beside the label ONLY while engaged. It is a gesture, not a detail, so
//! it must not cost a popup to reach.
//!
//! Values render in **tabular numerals inside a reserved slot**, because a
//! panel whose row reflows while you drag a fader is not a control panel.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelWidget,
    UiSlotValueKind,
};

use crate::app::node::{
    HFaderField, KnobField, PanelEmit, SlotDetailButton, SlotUnitSuffix, ToggleField,
};
use crate::base::{PopoverPlacement, StudioIcon, StudioIconName};

use super::PanelGesture;

static NEXT_MODULE_CONTROL_ID: AtomicUsize = AtomicUsize::new(1);

/// Bare text button for the label trigger; the state color rides the inner
/// visual and the hover affordance is a dotted underline.
const LABEL_TRIGGER_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:leading-none tw:decoration-dotted tw:underline-offset-2 tw:hover:underline";

/// A fader's fixed footprint. Without it the control's width tracks its
/// value string and the whole row shifts mid-drag.
const FADER_CLASS: &str = "tw:grid tw:w-[184px] tw:flex-none tw:gap-1";

/// The value slot: tabular numerals in a reserved box, so 0.6 / 0.62 / 200
/// all occupy the same width and nothing moves during a drag.
const READOUT_CLASS: &str = "tw:inline-flex tw:min-w-[4.5ch] tw:flex-none tw:items-baseline tw:justify-center tw:gap-1 tw:font-mono tw:text-[0.7rem] tw:tabular-nums";

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
    /// Widget value writes. The spike keeps knob drags on the existing
    /// slot-edit path; M4 routes them to `PanelWrite`.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let aspects = view.detail_aspects(&scope);
    let UiPanelControlView {
        channel,
        control,
        state,
        source: _,
    } = view;
    let engaged = state.engaged();
    let label_class = panel_state_label_class(state);
    let readout_class = panel_state_readout_class(state);

    // The whole control is the popover's outline anchor (P2c item 3).
    let anchor_id = use_hook(|| {
        let id = NEXT_MODULE_CONTROL_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-module-control-{id}")
    });

    let is_fader = matches!(control.widget, UiPanelWidget::Fader { .. });
    let column_class = if is_fader {
        FADER_CLASS
    } else if play {
        "tw:flex tw:min-w-[76px] tw:flex-none tw:flex-col tw:items-center tw:gap-1.5"
    } else {
        "tw:flex tw:min-w-[64px] tw:flex-none tw:flex-col tw:items-center tw:gap-1"
    };
    let anchor_class = if is_fader {
        "tw:grid tw:h-full tw:w-full tw:content-start tw:gap-1"
    } else {
        "tw:flex tw:h-full tw:w-full tw:flex-col tw:items-center tw:gap-1"
    };

    let reset_scope = scope.clone();
    let reset_channel = channel.clone();
    let reset_label = control.label.clone();
    // Reset is a GESTURE, so it sits beside the label on the face rather
    // than inside the popup — one click, always.
    let reset = rsx! {
        if engaged && let Some(handler) = on_panel {
            button {
                class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-status-attention-foreground tw:opacity-70 tw:hover:opacity-100",
                r#type: "button",
                title: "Reset {reset_label} — drop the held value and follow the project again",
                aria_label: "Reset {reset_label}",
                onclick: move |event| {
                    event.stop_propagation();
                    handler.call(PanelGesture::ClearControl {
                        scope: reset_scope.clone(),
                        channel: reset_channel.clone(),
                    });
                },
                StudioIcon { name: StudioIconName::Revert, size: 10 }
            }
        }
    };

    let anchor_control = control.clone();
    let anchor_label = label_visual(&control.label, label_class);

    let label = rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1",
            SlotDetailButton {
                label: control.label.clone(),
                aspects,
                name_override: Some(control.label.clone()),
                initially_open: detail_initially_open,
                placement: PopoverPlacement::BottomMiddle,
                anchor_id: Some(anchor_id.clone()),
                anchor_visual: rsx! {
                    div { class: anchor_class,
                        ModulePanelControlBody {
                            control: anchor_control,
                            state,
                            readout_class,
                            label: anchor_label,
                            on_action,
                        }
                    }
                },
                trigger: Some(label_visual(&control.label, label_class)),
                trigger_class: LABEL_TRIGGER_CLASS.to_string(),
                trigger_open_class: LABEL_TRIGGER_CLASS.to_string(),
                on_action,
            }
            {reset}
        }
    };

    rsx! {
        div { id: "{anchor_id}", class: column_class,
            ModulePanelControlBody {
                control,
                state,
                readout_class,
                label,
                on_action,
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
) -> Element {
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);

    let readout = rsx! {
        span { class: "{READOUT_CLASS} {readout_class}",
            span { "{shown_display(&control.value.display, control.live_value.as_deref(), state)}" }
            SlotUnitSuffix { unit: control.unit.clone(), reserve: false }
        }
    };

    match control.widget.clone() {
        UiPanelWidget::Knob { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_value(&control.value.kind) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                KnobField {
                    value,
                    live_value: live_numeric(&control.live_value, following),
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    address: control.address.clone(),
                    emit,
                    on_action,
                }
                {label}
                {readout}
            }
        }
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_value(&control.value.kind) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-2",
                    {label}
                    {readout}
                }
                HFaderField {
                    value,
                    live_value: live_numeric(&control.live_value, following),
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    address: control.address.clone(),
                    emit,
                    on_action,
                }
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
                        live_value: live_bool(&control.live_value, following),
                        state: control.state.clone(),
                        bound: following,
                        engaged,
                        address: control.address.clone(),
                        on_action,
                    }
                }
                {label}
                {readout}
            }
        }
    }
}

/// The label visual shared by the trigger and its top-layer copy: the
/// control name in the panel's label typography plus the small info glyph
/// that stands for "this opens details", both in the state color.
fn label_visual(label: &str, color_class: &'static str) -> Element {
    rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1 {color_class}",
            span { class: "tw:truncate tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em]",
                "{label}"
            }
            span { class: "tw:inline-flex tw:flex-none tw:opacity-60",
                StudioIcon { name: StudioIconName::InfoBare, size: 9 }
            }
        }
    }
}

/// Label color per panel state. Three families, none of them green.
fn panel_state_label_class(state: UiPanelControlState) -> &'static str {
    match state {
        UiPanelControlState::ReadDefault => "tw:text-subtle-foreground",
        UiPanelControlState::ReadFollowing => "tw:text-status-bound-foreground",
        UiPanelControlState::Engaged => "tw:text-status-attention-foreground",
    }
}

/// Readout color per panel state: the held value leads in amber, a followed
/// value in violet, an untouched default stays quiet.
fn panel_state_readout_class(state: UiPanelControlState) -> &'static str {
    match state {
        UiPanelControlState::ReadDefault => "tw:text-dim-foreground",
        UiPanelControlState::ReadFollowing => "tw:text-status-bound-foreground",
        UiPanelControlState::Engaged => "tw:text-status-attention-foreground",
    }
}

/// The number the readout shows: a followed control displays the live
/// resolved value (P6 item 1 — watch the LFO move the knob); everything
/// else shows its own value.
fn shown_display(value: &str, live: Option<&str>, state: UiPanelControlState) -> String {
    match (state, live) {
        (UiPanelControlState::ReadFollowing, Some(live)) => live.to_string(),
        _ => value.to_string(),
    }
}

/// The live reading as a number for the widget geometry — only meaningful
/// while the control is following something.
fn live_numeric(live: &Option<String>, following: bool) -> Option<f32> {
    following.then(|| live.as_deref()?.parse().ok()).flatten()
}

/// The live reading as a bool for the toggle pill.
fn live_bool(live: &Option<String>, following: bool) -> Option<bool> {
    following.then(|| live.as_deref()?.parse().ok()).flatten()
}

/// Read-only fallback when the widget family and the value family disagree.
fn mismatch(label: &str, display: &str) -> Element {
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
        UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelWidget, UiSlotAspectKind,
        UiSlotFieldState, UiSlotValue,
    };

    use super::{panel_state_label_class, panel_state_readout_class, shown_display};

    fn view(state: UiPanelControlState, source: Option<&str>) -> UiPanelControlView {
        UiPanelControlView::new(
            "speed",
            UiPanelControl {
                label: "speed".to_string(),
                address: None,
                widget: UiPanelWidget::Knob {
                    min: 0.0,
                    max: 1.0,
                    step: None,
                },
                value: UiSlotValue::f32(0.5),
                live_value: None,
                unit: None,
                state: UiSlotFieldState::editable(),
                aspects: Vec::new(),
            },
        )
        .with_state(state, source)
    }

    #[test]
    fn the_three_panel_states_wear_three_different_families() {
        let families: Vec<&str> = [
            UiPanelControlState::ReadDefault,
            UiPanelControlState::ReadFollowing,
            UiPanelControlState::Engaged,
        ]
        .into_iter()
        .map(panel_state_label_class)
        .collect();

        // P-Q2's requirement, pinned: three visibly distinct states.
        assert_eq!(families.len(), 3);
        assert_ne!(families[0], families[1]);
        assert_ne!(families[1], families[2]);
        assert_ne!(families[0], families[2]);
        // Engaged must NOT reuse the bound-violet family (P6).
        assert!(families[1].contains("bound"));
        assert!(!families[2].contains("bound"));
        assert!(families[2].contains("attention"));
        // And green stays valid-only, everywhere.
        for family in families.iter().chain(
            [
                panel_state_readout_class(UiPanelControlState::ReadDefault),
                panel_state_readout_class(UiPanelControlState::ReadFollowing),
                panel_state_readout_class(UiPanelControlState::Engaged),
            ]
            .iter(),
        ) {
            assert!(!family.contains("good"), "green is valid-only: {family}");
        }
    }

    #[test]
    fn a_following_control_reads_out_the_live_value_not_its_default() {
        assert_eq!(
            shown_display("1.60", Some("2.72"), UiPanelControlState::ReadFollowing),
            "2.72"
        );
        // Engaged shows the held value; the live reading is its own.
        assert_eq!(
            shown_display("1.60", Some("2.72"), UiPanelControlState::Engaged),
            "1.60"
        );
        assert_eq!(
            shown_display("1.60", None, UiPanelControlState::ReadDefault),
            "1.60"
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
