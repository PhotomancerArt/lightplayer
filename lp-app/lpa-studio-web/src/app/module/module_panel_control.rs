//! One control on a module's panel, wearing its panel state.
//!
//! The three states `docs/design/panel.md` P-Q2 requires to be visibly
//! distinct, rendered with the existing knob v2 / fader / toggle widgets so
//! the panel and the node face speak one widget language:
//!
//! | state | widget | label | caption |
//! |---|---|---|---|
//! | Read, at default | accent arc at the authored default | subtle | `default` + the value's origin |
//! | Read, following | **violet** arc at the LIVE value | violet | `following …` (who is driving) |
//! | Engaged (Latch) | **amber** arc + amber body ring | amber | `held` + the reset gesture |
//!
//! Amber (the `status-attention` family) is the spike's engaged proposal:
//! it is the one warm family Studio already owns, it is not violet (bound
//! means *wired*, engaged means *captured* — P6), not green (valid only),
//! and not the blue live family (transient edits). A dedicated
//! `status-engaged` token family is the eventual home; the gate question is
//! whether amber reads as "captured" at a glance.
//!
//! **Reset** (P2 clear) is per control: the small revert glyph appears
//! beside the label ONLY while engaged, because there is nothing to clear
//! otherwise — the affordance's presence is itself part of the state
//! signal. The per-module reset lives on the group header
//! ([`super::ModulePanel`]).

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiPanelControlState, UiPanelControlView, UiPanelWidget, UiSlotValueKind,
};

use crate::app::node::{HFaderField, KnobField, PanelEmit, SlotUnitSuffix, ToggleField};
use crate::base::{StudioIcon, StudioIconName};

use super::PanelGesture;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModulePanelControl(
    /// The channel's control view: widget payload plus panel state.
    view: UiPanelControlView,
    /// The scope this control lives in — the other half of its identity
    /// (panel.md P1) and what a reset gesture carries.
    scope: String,
    /// Play mode renders the same control at a roomier size and drops the
    /// authoring-side captions to one line.
    #[props(default = false)]
    play: bool,
    /// Panel gestures (reset). Absent = the control is display-only.
    #[props(default = None)]
    on_panel: Option<EventHandler<PanelGesture>>,
    /// Widget value writes. The spike keeps knob drags on the existing
    /// slot-edit path; M4 routes them to `PanelWrite`.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let UiPanelControlView {
        channel,
        control,
        state,
        source,
    } = view;
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);
    let label_class = panel_state_label_class(state);
    let caption = control_caption(state, source.as_deref());
    let reset_scope = scope.clone();
    let reset_channel = channel.clone();

    let label = rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1",
            span { class: "tw:truncate tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em] {label_class}",
                "{control.label}"
            }
            // Reset exists only where there is a writer to remove (P2).
            if engaged && let Some(handler) = on_panel {
                button {
                    class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-status-attention-foreground tw:opacity-70 tw:hover:opacity-100",
                    r#type: "button",
                    title: "Reset {control.label} — drop the held value and follow the project again",
                    aria_label: "Reset {control.label}",
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
        }
    };

    let readout_class = panel_state_readout_class(state);
    let readout = rsx! {
        span { class: "tw:inline-flex tw:items-baseline tw:gap-1 tw:font-mono tw:text-[0.7rem] {readout_class}",
            span { "{shown_display(&control.value.display, control.live_value.as_deref(), state)}" }
            SlotUnitSuffix { unit: control.unit.clone(), reserve: false }
        }
    };

    let caption_row = rsx! {
        if let Some(caption) = caption.clone() {
            span {
                class: "tw:max-w-[17ch] tw:truncate tw:text-center tw:text-[0.6rem] tw:leading-none tw:text-dim-foreground",
                title: "{caption}",
                "{caption}"
            }
        }
    };

    let column_class = if play {
        "tw:flex tw:min-w-[76px] tw:flex-none tw:flex-col tw:items-center tw:gap-1.5"
    } else {
        "tw:flex tw:min-w-[64px] tw:flex-none tw:flex-col tw:items-center tw:gap-1"
    };

    match control.widget.clone() {
        UiPanelWidget::Knob { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_value(&control.value.kind) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                div { class: column_class,
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
                    {caption_row}
                }
            }
        }
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = PanelEmit::for_value(&control.value.kind) else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                div { class: "tw:grid tw:min-w-0 tw:gap-1",
                    div { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-2",
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
                    if let Some(caption) = caption {
                        span { class: "tw:text-[0.6rem] tw:leading-none tw:text-dim-foreground", "{caption}" }
                    }
                }
            }
        }
        UiPanelWidget::Toggle => {
            let UiSlotValueKind::Bool(value) = control.value.kind else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                div { class: column_class,
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
                    {caption_row}
                }
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

/// The caption under the control: what state it is in and, in Read, whose
/// value is on screen (P2 — the UI distinguishes inherited / authored /
/// default).
fn control_caption(state: UiPanelControlState, source: Option<&str>) -> Option<String> {
    match (state, source) {
        (UiPanelControlState::Engaged, Some(source)) => Some(format!("held · was {source}")),
        (UiPanelControlState::Engaged, None) => Some("held".to_string()),
        (_, Some(source)) => Some(source.to_string()),
        (UiPanelControlState::ReadDefault, None) => Some("default".to_string()),
        (UiPanelControlState::ReadFollowing, None) => Some("following".to_string()),
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
            span { class: "tw:font-mono tw:text-[0.7rem] tw:text-muted-foreground", "{display}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiPanelControlState;

    use super::{
        control_caption, panel_state_label_class, panel_state_readout_class, shown_display,
    };

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

    #[test]
    fn captions_say_what_the_state_is_and_what_it_displaced() {
        assert_eq!(
            control_caption(UiPanelControlState::Engaged, Some("clock")),
            Some("held · was clock".to_string())
        );
        assert_eq!(
            control_caption(UiPanelControlState::ReadFollowing, Some("clock")),
            Some("clock".to_string())
        );
        assert_eq!(
            control_caption(UiPanelControlState::ReadDefault, None),
            Some("default".to_string())
        );
    }
}
