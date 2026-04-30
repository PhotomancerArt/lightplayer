//! **UI widget** hints for editing a slot’s value: orthogonal to
//! [`Constraint`](lpc_model::prop::constraint::Constraint), which is the **legal
//! range**; presentation is how to *show* the control
//! (`docs/design/lightplayer/quantity.md` §9 and the default table there).
//! v0 is an **enum only** (no per-variant config); constraints already carry
//! range/step/choices. Log scale, format strings, and similar are deferred
//! until a concrete need (`quantity.md` §9 “v0 is enum-only”).
//!
//! When a [`SrcSlot`](crate::prop::src_shape::SrcSlot)’s `present` is `None`, tools use
//! [`kind_default_presentation`](crate::prop::kind_defaults::kind_default_presentation)
//! (`quantity.md` §9 table).

/// A **widget kind** for inspector / panel generation. Values serialize as
/// snake_case strings (see module tests).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    /// Rotary control; default for several angular and frequency-like kinds (`quantity.md` §9 table).
    Knob,
    /// Linear fader; default for spans and many 0–1 kinds (`quantity.md` §9).
    Fader,
    /// On/off control; default for [`Kind::Bool`](lpc_model::prop::kind::Kind::Bool).
    Toggle,
    /// Typed or stepped numeric field; used for e.g. [`Kind::Instant`](lpc_model::prop::kind::Kind::Instant) and [`Kind::Count`](lpc_model::prop::kind::Kind::Count), and for [`Kind::Position3d`](lpc_model::prop::kind::Kind::Position3d) in v0 (three numbers, `quantity.md` §9).
    NumberInput,
    /// Labeled discrete options; default for [`Kind::Choice`](lpc_model::prop::kind::Kind::Choice).
    Dropdown,
    /// 2D point editor; default for [`Kind::Position2d`](lpc_model::prop::kind::Kind::Position2d).
    XyPad,
    /// Color space + coordinate editing for [`Kind::Color`](lpc_model::prop::kind::Kind::Color).
    ColorPicker,
    /// Edits palette entries; default for [`Kind::ColorPalette`](lpc_model::prop::kind::Kind::ColorPalette).
    PaletteEditor,
    /// Edits gradient stops; default for [`Kind::Gradient`](lpc_model::prop::kind::Kind::Gradient).
    GradientEditor,
    /// Preview of a [`Kind::Texture`](lpc_model::prop::kind::Kind::Texture) slot.
    TexturePreview,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_round_trips_serde() {
        for p in [
            Presentation::Knob,
            Presentation::Fader,
            Presentation::Toggle,
            Presentation::NumberInput,
            Presentation::Dropdown,
            Presentation::XyPad,
            Presentation::ColorPicker,
            Presentation::PaletteEditor,
            Presentation::GradientEditor,
            Presentation::TexturePreview,
        ] {
            let s = serde_json::to_string(&p).unwrap();
            let back: Presentation = serde_json::from_str(&s).unwrap();
            assert_eq!(p, back);
        }
    }

    #[test]
    fn presentation_serde_form_is_snake_case() {
        let s = serde_json::to_string(&Presentation::ColorPicker).unwrap();
        assert_eq!(s, "\"color_picker\"");
    }
}
