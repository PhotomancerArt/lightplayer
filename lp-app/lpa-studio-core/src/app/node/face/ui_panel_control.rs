//! One control on a node face's front panel.

use crate::{
    ProjectSlotAddress, UiPanelWidget, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind,
    UiSlotAspectRow, UiSlotFieldState, UiSlotUnit, UiSlotValue,
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

/// How a numeric gesture's `f32` is TYPED on its way out of a control.
///
/// Every control until the M2 time break emitted the number itself, so the
/// dispatching layer could read the family straight off the slot's current
/// value. A phasor's period cannot: the value it edits is one field of a
/// whole [`lpc_model::PhasorConfig`] record — the slot's shape when the
/// knob edits locally, and the config channel's payload when it writes a
/// panel value — so the number has to be *re-wrapped* before it goes
/// anywhere, and only the projection knows the shaping to wrap it with.
///
/// Deliberately NOT a widget distinction: a period knob is an ordinary
/// knob, and the two dispatch paths (slot edit / panel write) are the same
/// two every other control uses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum UiPanelEmit {
    /// The gesture value IS the value written — every scalar control.
    #[default]
    Value,
    /// The gesture value is a phasor **period in seconds**, written wrapped
    /// in a whole `PhasorConfig` carrying this slot's own shaping.
    ///
    /// Waveform and phase offset are never panel-editable (settled D11 v1 —
    /// a waveform is how ONE consumer reads a shared phase, so a panel that
    /// set it would be setting it for everybody), but they must survive a
    /// period edit intact, which is why they ride along here.
    PhasorPeriod {
        waveform: lpc_model::Waveform,
        phase_offset: f32,
    },
    /// The gesture value is a whole [`lpc_model::GradientConfig`], written
    /// as-is.
    ///
    /// The palette chooser is the one control that carries NO field along:
    /// a phasor's period rides inside a record whose other fields must
    /// survive the edit, but a palette pick replaces the config outright —
    /// the same rule the engine reads it by (`resolve_gradient_config`
    /// takes a driven config *whole*, never as a partial overlay, so a
    /// palette never shows a set nobody authored together).
    ///
    /// Carries no payload for exactly that reason, and marks the control as
    /// non-numeric: no `f32` gesture ladder applies to it.
    Gradient,
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
    /// How a gesture's number is typed on the way out — see [`UiPanelEmit`].
    /// `Value` for every control but a phasor's period knob.
    pub emit: UiPanelEmit,
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

    /// The value the control's FACE shows: the live reading when one is
    /// present, else the authored value (GV fix 3).
    ///
    /// One number, never "0.82 (0.5)" — the parenthetical authored default
    /// read as a second value and reflowed the control's width mid-drag.
    /// The authored value keeps a home in the detail popup
    /// ([`Self::detail_aspects`]), which is where the control's provenance
    /// already lives.
    pub fn shown_display(&self) -> &str {
        self.live_value
            .as_deref()
            .unwrap_or(self.value.display.as_str())
    }

    /// The live reading the widget's geometry follows — the arc, fill, or
    /// pill sits at what the channel actually reads, whatever put it there
    /// (an automation writer, or this panel holding it).
    pub fn live_numeric(&self) -> Option<f32> {
        self.live_value.as_deref()?.parse().ok()
    }

    /// The live reading as a toggle state.
    pub fn live_bool(&self) -> Option<bool> {
        self.live_value.as_deref()?.parse().ok()
    }

    /// The palette this control presents, when its value is one — the
    /// [`crate::UiPanelWidget::PaletteSwatch`] payload.
    ///
    /// The parse is the model's own ([`gradient_config_value`]), reached
    /// through the value kind's `LpValue` mirror, so neither panel renderer
    /// walks the padded storage itself. `None` for any other value family,
    /// which is what a swatch/value disagreement falls back on.
    ///
    /// [`gradient_config_value`]: crate::app::project::gradient_config_value
    pub fn gradient_config(&self) -> Option<lpc_model::GradientConfig> {
        crate::app::project::gradient_config_value(&self.value.kind.to_lp_value())
    }

    /// The palette a SWATCH control presents — [`Self::gradient_config`]
    /// gated on the widget family, so a knob over some other struct-shaped
    /// slot never renders strips and a swatch over a non-palette value
    /// falls back to the read-only display.
    ///
    /// Both panel renderers ask here rather than each writing the guard: a
    /// control is one derivation with two presentations.
    pub fn swatch_palette(&self) -> Option<lpc_model::GradientConfig> {
        if !matches!(self.widget, crate::UiPanelWidget::PaletteSwatch) {
            return None;
        }
        self.gradient_config()
    }

    /// The control's popover sections: the backing slot row's aspects plus
    /// the AUTHORED value, which the face no longer shows whenever a live
    /// reading displaces it (GV fix 3). Without this row the authored
    /// default — still what an unwritten channel falls back to (R6), and
    /// still the edit target behind a panel write — would be nowhere.
    pub fn detail_aspects(&self) -> Vec<UiSlotAspect> {
        let mut aspects = self.aspects.clone();
        let row = UiSlotAspectRow::new("Authored value", self.value.display.clone());
        match aspects
            .iter_mut()
            .find(|aspect| aspect.kind == UiSlotAspectKind::TypeInfo)
        {
            Some(info) => info.rows.push(row),
            None => {
                aspects.push(UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info").with_row(row))
            }
        }
        aspects
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
            emit: crate::UiPanelEmit::Value,
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

    /// GV fix 3: the face shows ONE number, and the authored value it can
    /// displace keeps a home in the popup.
    #[test]
    fn the_face_shows_one_value_and_the_popup_keeps_the_authored_one() {
        let mut control = control(vec![UiSlotAspect::new(
            crate::UiSlotAspectKind::TypeInfo,
            "Info",
        )]);
        assert_eq!(control.shown_display(), "1.6");
        assert_eq!(control.live_numeric(), None);

        control.live_value = Some("0.82".to_string());
        assert_eq!(
            control.shown_display(),
            "0.82",
            "the live reading leads, with no parenthetical beside it"
        );
        assert_eq!(control.live_numeric(), Some(0.82));

        let aspects = control.detail_aspects();
        assert!(
            aspects.iter().any(|aspect| aspect
                .rows
                .iter()
                .any(|row| row.label == "Authored value" && row.value == "1.6")),
            "the displaced authored value is in the popup: {aspects:?}"
        );
        assert_eq!(
            control.aspects.len(),
            1,
            "and the control's own aspect list is untouched"
        );
    }

    /// A control with no aspects at all (story fixtures) still surfaces the
    /// authored value rather than dropping it on the floor.
    #[test]
    fn the_authored_value_row_lands_even_without_a_type_info_aspect() {
        let control = control(Vec::new());
        let aspects = control.detail_aspects();
        assert_eq!(aspects.len(), 1);
        assert_eq!(aspects[0].rows[0].label, "Authored value");
    }
}
