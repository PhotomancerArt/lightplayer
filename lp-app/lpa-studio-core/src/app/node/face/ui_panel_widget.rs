//! Widget families for node face panel controls.

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
}
