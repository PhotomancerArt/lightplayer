//! Widget families for node face panel controls.

use super::ui_panel_shape::UiPanelShape;

/// The interactive widget family a panel control renders as.
///
/// Ranges ride the widget (not the slot row) because the face's panel is the
/// authoritative gesture surface; the same slot's row editor keeps its own
/// hint-driven field.
#[derive(Clone, Debug, PartialEq)]
pub enum UiPanelWidget {
    /// Rotary knob over a bounded numeric range (SVG value arc, v2 styling).
    Knob {
        /// Minimum knob value.
        min: f32,
        /// Maximum knob value.
        max: f32,
        /// Optional preferred knob step.
        step: Option<f32>,
    },
    /// Linear fader over a bounded numeric range (the fixture face's
    /// dominant horizontal brightness fader).
    Fader {
        /// Minimum fader value.
        min: f32,
        /// Maximum fader value.
        max: f32,
        /// Optional preferred fader step.
        step: Option<f32>,
    },
    /// Boolean toggle.
    Toggle,
    /// A palette: the closed face of the chooser (M4 P3).
    ///
    /// Unlike every widget above it, this one has no range and no scalar
    /// gesture — the value it presents is a whole
    /// [`lpc_model::GradientConfig`], and a gesture replaces the config
    /// outright rather than moving a number inside it
    /// ([`crate::UiPanelEmit::Gradient`]). The FACE it renders is
    /// mode-adaptive: a held palette is one full-width strip, a cycle is its
    /// member set plus the step rate.
    PaletteSwatch,
    /// The clock's speed on a panel: one knob over the transport's rate
    /// leaf (`clock.rate`), log ¼×–8× with 1× at the top centre and a detent
    /// at each octave (the card panel pass, 2026-10-19).
    ///
    /// It carries **no min/max**: the widget owns that mapping and its
    /// detents, as the tape's speed fader does. The rest of the transport —
    /// run/pause and the scrub strip — is not on a panel at all: the clock
    /// node's own card keeps the whole tape ([`crate::UiClockFace::transport`]),
    /// the debug tool. On a panel, one control is one widget, over one
    /// channel: this control is the rate's own, nothing grouped.
    Speed,
    /// A playlist's Pattern instrument (multi-pattern vision D15): the
    /// set's names with tap-to-play, the cycle switch and step, per-pattern
    /// on/off, next/prev.
    ///
    /// A whole instrument with several gestures, not all of them panel
    /// writes — tap and next/prev are `PlaylistActivateOp` runtime
    /// commands — so each one rides the picker as a ready action.
    PatternPicker {
        /// Names, states, cycle and every gesture's action.
        picker: crate::UiPatternPicker,
    },
}

impl UiPanelWidget {
    /// How this widget takes its room on a panel. The match is exhaustive
    /// on purpose: a new widget must say its shape.
    #[must_use]
    pub fn shape(&self) -> UiPanelShape {
        match self {
            Self::Knob { .. } | Self::Toggle | Self::Speed => UiPanelShape::Cell,
            Self::Fader { .. } | Self::PaletteSwatch => UiPanelShape::Strip,
            Self::PatternPicker { .. } => UiPanelShape::Block,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_widget_names_its_shape() {
        let knob = UiPanelWidget::Knob {
            min: 0.0,
            max: 1.0,
            step: None,
        };
        let stepped = UiPanelWidget::Knob {
            min: 0.0,
            max: 8.0,
            step: Some(1.0),
        };
        let fader = UiPanelWidget::Fader {
            min: 0.0,
            max: 1.0,
            step: None,
        };
        let picker = UiPanelWidget::PatternPicker {
            picker: crate::UiPatternPicker {
                verbs: crate::OfferPath::root("project"),
                entries: Vec::new(),
                active: None,
                cycle: lpc_model::PlaylistCycle::Hold,
                cycle_target: None,
                skip_target: None,
            },
        };
        assert_eq!(knob.shape(), UiPanelShape::Cell);
        assert_eq!(stepped.shape(), UiPanelShape::Cell);
        assert_eq!(UiPanelWidget::Toggle.shape(), UiPanelShape::Cell);
        assert_eq!(UiPanelWidget::Speed.shape(), UiPanelShape::Cell);
        assert_eq!(fader.shape(), UiPanelShape::Strip);
        assert_eq!(UiPanelWidget::PaletteSwatch.shape(), UiPanelShape::Strip);
        assert_eq!(picker.shape(), UiPanelShape::Block);
    }
}
