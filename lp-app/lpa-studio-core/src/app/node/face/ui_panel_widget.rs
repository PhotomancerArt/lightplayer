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
    /// The clock's tape transport, as ONE grouped control (plan
    /// 2026-08-04-2355-clock-tape-hero, P8).
    ///
    /// The first widget with more than one gesture on its faceplate: a log
    /// ׼–×8 speed fader, a run/pause button, and a drag-scrub strip. It
    /// carries **no min/max** — the widget owns that mapping and its octave
    /// detents ([`crate::UiClockTransport`] is the whole instrument's
    /// state, and [`lpc_model::CLOCK_TRANSPORT_SHAPE_NAME`] is the model
    /// shape it presents).
    ///
    /// Three facts, three different homes, per the settled grouping
    /// contract:
    ///
    /// - **Rendering is a shape fact** — this variant means "draw the whole
    ///   faceplate"; wiring never subtracts a dimension from it.
    /// - **Membership is a wiring fact** — the derivation only emits the
    ///   control when at least one leaf is panel-public.
    /// - **Dispatch is a per-leaf fact** — each dimension resolves its own
    ///   target-or-address through [`crate::UiPanelControl::wires`].
    Transport {
        /// The instrument's live values and per-dimension slot addresses —
        /// the SAME block the clock card's own tape hero renders, so the
        /// panel copy and the card copy can never disagree.
        transport: crate::UiClockTransport,
    },
}
