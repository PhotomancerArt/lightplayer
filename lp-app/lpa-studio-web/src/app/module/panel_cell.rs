//! [`PanelCell`]: one control in a cell on the connected board card's panel
//! — a knob, the Speed knob, a stepped knob or a toggle, its label and its
//! value centred under it, in its state's colours (the cell shape,
//! [`lpa_studio_core::UiPanelShape::Cell`]; the panel layout study,
//! `docs/ux/studies/panel-layout/`).
//!
//! **The whole cell is the control's target** (the guide's proposed P21):
//! the widget draws the label and the value inside its own gesture
//! surface, which fills the cell, so a knob turns from a vertical drag
//! anywhere in it and a toggle flips from a press anywhere in it. A press
//! that never moves (a tap) writes nothing and opens the control's details;
//! the Speed knob's tap waits out its double-click (1×) first. The cell
//! lightens under the pointer.
//!
//! Its two small actions are 24 px circles in its top corners
//! ([`PanelCorners`]): let go (↺) while held, details (ⓘ) on hover or
//! focus. They float over the knob's empty corners and never move the label
//! or the value.
//!
//! The card draws cells three to a line on a 300 px card, more on a wider
//! one; the light lines between them belong to the grid
//! ([`super::CompactPanel`]). The widgets are the production ones, with the
//! same writes as everywhere else in Studio.
//!
//! Walk hooks, on the cell: `data-panel-scope`, `data-panel-channel` and
//! `data-panel-state` (as [`super::ModulePanelControl`] carries them); the
//! widget's role inside it (`role="slider"`, `role="switch"`) is the one a
//! walk turns.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiCardControl, UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit,
    UiPanelWidget, UiSlotValueKind,
};

use crate::app::node::{KnobField, PanelEmit, ToggleField};

use super::PanelGesture;
use super::module_panel_control::mismatch;
use super::panel_corners::PanelCorners;
use super::panel_label::panel_label;
use super::panel_readout::PanelReadout;
use super::panel_state_look::{
    panel_state_hook, panel_state_label_class, panel_state_readout_class,
};
use super::panel_story_look::use_panel_story_look;

static NEXT_PANEL_CELL_ID: AtomicUsize = AtomicUsize::new(1);

/// One cell: the positioned box its corners float in.
const CELL_CLASS: &str = "ux-panel-cell tw:flex tw:min-w-0";

/// The widget's gesture surface in a cell: the whole cell, the widget at
/// the top, its label and its value under it, centred; the top padding
/// leaves the cell's corners free.
const CELL_SURFACE_CLASS: &str = "tw:flex tw:w-full tw:min-w-0 tw:flex-col tw:items-center tw:gap-[3px] tw:px-1 tw:pt-4 tw:pb-2.5";

/// The cell's live copy over its anchor while its details are open.
const CELL_COPY_CLASS: &str = "tw:flex tw:h-full tw:w-full tw:min-w-0";

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
    let look = use_panel_story_look();
    // The details' open state: the ⓘ toggles it, a tap on the cell opens it.
    let mut open = use_signal(|| look.opens(&scope, &channel));
    let on_tap = use_callback(move |()| open.set(true));
    // The whole cell is the details' outline anchor.
    let anchor_id = use_hook(|| {
        let id = NEXT_PANEL_CELL_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-panel-cell-{id}")
    });
    let toggle = matches!(control.widget, UiPanelWidget::Toggle);
    let class = cell_class(CELL_CLASS, look.hovers(&scope, &channel), open(), toggle);
    rsx! {
        div {
            id: "{anchor_id}",
            class,
            "data-panel-scope": "{scope}",
            "data-panel-channel": "{channel}",
            "data-panel-state": panel_state_hook(state),
            CellFace {
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
                    div { class: CELL_COPY_CLASS,
                        CellFace { control: control.clone(), state, on_action }
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

/// The classes of a cell or a strip (`base`): drawn hovered (a story's
/// look), its details open (its ⓘ stays in view), a toggle (its ⓘ shows
/// where nothing hovers, `style.css`).
pub(crate) fn cell_class(base: &str, hovered: bool, details_open: bool, toggle: bool) -> String {
    let mut class = base.to_string();
    for (on, name) in [
        (hovered, "is-hovered"),
        (details_open, "is-details-open"),
        (toggle, "is-toggle"),
    ] {
        if on {
            class.push(' ');
            class.push_str(name);
        }
    }
    class
}

/// What a cell draws: the widget's gesture surface, the whole cell, with
/// the label and the value inside it. Rendered in the cell (with its tap)
/// and as its top-layer copy while the details are open (without).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn CellFace(
    control: UiPanelControl,
    state: UiPanelControlState,
    on_action: EventHandler<UiAction>,
    #[props(default)] on_tap: Option<EventHandler<()>>,
) -> Element {
    let engaged = state.engaged();
    let following = matches!(state, UiPanelControlState::ReadFollowing);
    let on_action = Some(on_action);
    let body = rsx! {
        {panel_label(&control.label, panel_state_label_class(state))}
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
                    body,
                    surface_class: CELL_SURFACE_CLASS,
                    on_tap,
                }
            }
        }
        // The clock's Speed knob: its own mapping (log ¼×–8×, 1× at the
        // top), its value said as a speed, its double-click 1×.
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
                    body,
                    surface_class: CELL_SURFACE_CLASS,
                    on_tap,
                }
            }
        }
        // A toggle's tap is its flip: its details come from its ⓘ.
        UiPanelWidget::Toggle => {
            let UiSlotValueKind::Bool(value) = control.value.kind else {
                return mismatch(&control.label, &control.value.display);
            };
            rsx! {
                ToggleField {
                    value,
                    live_value: control.live_bool(),
                    state: control.state.clone(),
                    bound: following,
                    engaged,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    on_action,
                    body,
                    surface_class: CELL_SURFACE_CLASS,
                }
            }
        }
        // A strip or the block never lands in a cell: the card lays out by
        // shape. Should one, it reads rather than misbehaves.
        UiPanelWidget::Fader { .. }
        | UiPanelWidget::PaletteSwatch
        | UiPanelWidget::PatternPicker { .. } => mismatch(&control.label, &control.value.display),
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelEmit, UiPanelTarget,
        UiSlotFieldState, UiSlotValue,
    };

    use super::*;

    /// ↺ is drawn only on a held control with something to clear; ⓘ always.
    #[test]
    fn let_go_shows_only_on_a_held_control_with_a_target() {
        let quiet = render(knob(UiPanelControlState::ReadDefault, true));
        assert!(!quiet.contains("data-panel-let-go"), "{quiet}");
        assert!(quiet.contains("data-panel-details"), "{quiet}");
        let held = render(knob(UiPanelControlState::Engaged, true));
        assert!(held.contains("data-panel-let-go"), "{held}");
        assert!(held.contains("Reset hue"), "{held}");
        let untargeted = render(knob(UiPanelControlState::Engaged, false));
        assert!(!untargeted.contains("data-panel-let-go"), "{untargeted}");
    }

    /// The label and the value are inside the knob's own gesture surface:
    /// the whole cell drags.
    #[test]
    fn the_label_and_value_are_inside_the_gesture_surface() {
        let html = render(knob(UiPanelControlState::ReadDefault, true));
        let surface = html.find(r#"role="slider""#).expect("the knob's surface");
        let label = html.find(">hue<").expect("the label");
        let corners = html.find("data-panel-details").expect("the ⓘ");
        assert!(surface < label && label < corners, "{html}");
        assert!(html.contains("data-panel-surface"), "{html}");
        // The label is no trigger: no glyph, no underline.
        assert!(!html.contains("tw:decoration-dotted"), "{html}");
    }

    /// A toggle's cell says it is one, so where nothing hovers its ⓘ stays
    /// in view (its tap is its flip); the stylesheet holds the rule.
    #[test]
    fn a_toggle_cell_keeps_its_details_in_view_where_nothing_hovers() {
        assert!(cell_class(CELL_CLASS, false, false, true).contains("is-toggle"));
        assert!(!cell_class(CELL_CLASS, false, false, false).contains("is-toggle"));
        let css = include_str!("../../style.css");
        let rule = css
            .split("@media (hover: none)")
            .nth(1)
            .expect("a rule for where nothing hovers");
        assert!(
            rule[..rule.find('}').expect("the rule")]
                .contains(".ux-panel-cell.is-toggle .ux-panel-corner-details"),
            "{rule}"
        );
    }

    /// ⓘ shows on hover, while the control has focus, and while its details
    /// are open.
    #[test]
    fn details_show_on_hover_focus_and_while_open() {
        let css = include_str!("../../style.css");
        for rule in [
            ".ux-panel-cell:hover .ux-panel-corner-details",
            ".ux-panel-cell:focus-within .ux-panel-corner-details",
            ".ux-panel-cell.is-details-open .ux-panel-corner-details",
            ".ux-panel-cell.is-hovered .ux-panel-corner-details",
        ] {
            assert!(css.contains(rule), "{rule} missing from style.css");
        }
        let open = cell_class(CELL_CLASS, false, true, false);
        assert!(open.contains("is-details-open"), "{open}");
    }

    fn knob(state: UiPanelControlState, targeted: bool) -> UiCardControl {
        let target = targeted.then(|| UiPanelTarget {
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpa_studio_core::NodeId::new(1),
            },
            channel: "hue".to_string(),
            engaged: state.engaged(),
        });
        UiCardControl {
            scope: "/".to_string(),
            view: UiPanelControlView::new(
                "hue",
                UiPanelControl {
                    emit: UiPanelEmit::Value,
                    label: "hue".to_string(),
                    address: None,
                    widget: UiPanelWidget::Knob {
                        min: 0.0,
                        max: 1.0,
                        step: None,
                    },
                    value: UiSlotValue::f32(0.5),
                    live_value: None,
                    live_gradient: None,
                    panel_target: target,
                    unit: None,
                    state: UiSlotFieldState::editable(),
                    aspects: Vec::new(),
                },
            )
            .with_state(state, None::<String>),
        }
    }

    fn render(control: UiCardControl) -> String {
        crate::app::board_card::card_test_fixtures::render(Harness, HarnessProps { control })
    }

    #[component]
    #[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
    fn Harness(control: UiCardControl) -> Element {
        rsx! {
            PanelCell { control, on_panel: |_| {}, on_action: |_| {} }
        }
    }
}
