//! One control on a node face's front panel.

use crate::{
    ProjectSlotAddress, UiPanelWidget, UiSlotAffordance, UiSlotAspect, UiSlotFieldState,
    UiSlotUnit, UiSlotValue,
};

/// The `(scope, channel)` a panel gesture writes (panel.md P1 identity),
/// carried when the control's backing slot consumes a bus channel. Present
/// = gestures dispatch `PanelWriteOp` down the runtime command channel (no
/// overlay, no dirty); absent = the control has no channel to write (an
/// unbound uniform) and keeps the authored-default slot-edit path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiPanelTarget {
    /// The scope the write lands in (where the consuming binding reads).
    pub scope: lpc_wire::WireScopeRef,
    /// The bus channel the control drives.
    pub channel: String,
    /// Whether a panel writer is currently engaged for this target (a
    /// Panel-origin provider row in the binding graph) — drives the
    /// control's clear affordance.
    pub engaged: bool,
}

/// A front-panel control projected from a slot that is on a panel —
/// since Q13 that means a slot bound to a bus channel.
///
/// Panel controls sit directly on the card (no box-in-box) and open the SAME
/// detail popover as their slot row (hover-revealed corner ⓘ). A control
/// with a [`Self::panel_target`] dispatches gestures as panel writes (the
/// runtime command channel, panel.md P8); one without falls back to the
/// standard slot write path (`SlotEditOp::SetValue` at [`Self::address`]).
/// Both flood paths coalesce in the studio actor.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPanelControl {
    /// Human-readable control label.
    pub label: String,
    /// Stable slot address for dispatching edits and resolving the detail
    /// popover. `None` for mock/story controls not backed by a project slot.
    pub address: Option<ProjectSlotAddress>,
    /// The widget family (and its range) this control renders as.
    pub widget: UiPanelWidget,
    /// Current typed value, shared with the slot row's editor.
    pub value: UiSlotValue,
    /// The bound channel's current reading, display-only (P6 item 1): the
    /// widget renders it in the bound-violet family while [`Self::value`]
    /// (the authored default) stays the edit target. Already quantized
    /// (≤2 decimals) and absent for monotonic/time-kind channels — see
    /// [`crate::UiBindingEndpoint::live_value`], the source it mirrors.
    pub live_value: Option<String>,
    /// The `(scope, channel)` a gesture writes down the panel command
    /// channel, present when the backing slot consumes a bus channel.
    pub panel_target: Option<UiPanelTarget>,
    /// Optional display unit rendered near the value (e.g. "Hz", "%").
    pub unit: Option<UiSlotUnit>,
    /// Interaction, dirty, bound/live, and validation state (violet when
    /// bound, live indicator for transient-persistence slots).
    pub state: UiSlotFieldState,
    /// Detail-popover sections for the control's corner ⓘ — the SAME aspect
    /// list the backing slot row renders (`UiConfigSlot::visible_aspects()`),
    /// so the panel control and the slot row open identical popovers. Also
    /// the source of the control's rolled-up affordance (violet when the
    /// binding aspect carries `Bound`). P2 adjustment: `UiSlotFieldState`
    /// alone cannot reconstruct the popover (binding endpoint, type info),
    /// and re-deriving here would fork the slot row's content.
    pub aspects: Vec<UiSlotAspect>,
}

impl UiPanelControl {
    /// The most important rolled-up affordance across the control's aspects
    /// (same merge as the slot row) — drives the widget's status treatment:
    /// `Bound` is the violet family, never green.
    pub fn primary_affordance(&self) -> UiSlotAffordance {
        self.aspects
            .iter()
            .filter_map(|aspect| aspect.affordance)
            .max()
            .unwrap_or(UiSlotAffordance::Info)
    }

    /// Whether the backing slot is bound (bus/producer wiring) — the violet
    /// treatment on the widget itself. Checked directly (not via the
    /// affordance merge) so a bound control that is ALSO edited keeps its
    /// violet widget while the edit chrome rides the dirty dot.
    pub fn bound(&self) -> bool {
        self.aspects
            .iter()
            .any(|aspect| aspect.affordance == Some(UiSlotAffordance::Bound))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        UiPanelControl, UiPanelWidget, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind,
        UiSlotFieldState, UiSlotValue,
    };

    fn control(aspects: Vec<UiSlotAspect>) -> UiPanelControl {
        UiPanelControl {
            label: "speed".to_string(),
            address: None,
            widget: UiPanelWidget::Knob {
                min: 0.0,
                max: 4.0,
                step: None,
            },
            value: UiSlotValue::f32(1.6),
            live_value: None,
            panel_target: None,
            unit: None,
            state: UiSlotFieldState::editable(),
            aspects,
        }
    }

    #[test]
    fn bound_stays_violet_even_when_also_edited() {
        let control = control(vec![
            UiSlotAspect::new(UiSlotAspectKind::Binding, "Binding")
                .with_affordance(UiSlotAffordance::Bound),
            UiSlotAspect::new(UiSlotAspectKind::EditState, "Edit state")
                .with_affordance(UiSlotAffordance::Edited),
        ]);

        // The affordance merge rolls up to the more severe Edited...
        assert_eq!(control.primary_affordance(), UiSlotAffordance::Edited);
        // ...but the widget's violet treatment reads the binding directly.
        assert!(control.bound());
    }

    #[test]
    fn unbound_control_without_aspects_is_quiet() {
        let control = control(Vec::new());

        assert_eq!(control.primary_affordance(), UiSlotAffordance::Info);
        assert!(!control.bound());
    }
}
