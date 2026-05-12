//! Legacy [`Kind`]-associated presentation defaults.
//!
//! New slot-domain code should prefer semantic slot leaves with their own
//! metadata. This module remains only for older source-shape tests and tooling.

use crate::presentation::Presentation;
use lpc_model::value::kind::Kind;

/// **Presentation** when a [`super::src_shape::SrcSlot`] omits `present`.
pub fn kind_default_presentation(k: Kind) -> Presentation {
    use Presentation::*;
    match k {
        Kind::Instant | Kind::Count => NumberInput,
        Kind::Duration | Kind::Amplitude | Kind::Ratio => Fader,
        Kind::Frequency | Kind::Angle | Kind::Phase => Knob,
        Kind::Bool => Toggle,
        Kind::Choice => Dropdown,
        Kind::Color => ColorPicker,
        Kind::ColorPalette => PaletteEditor,
        Kind::Gradient => GradientEditor,
        Kind::Position2d => XyPad,
        Kind::Position3d => NumberInput,
        Kind::Texture => TexturePreview,
        Kind::AudioLevel => NumberInput,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_presentation_table() {
        assert_eq!(
            kind_default_presentation(Kind::Amplitude),
            Presentation::Fader
        );
        assert_eq!(
            kind_default_presentation(Kind::Frequency),
            Presentation::Knob
        );
        assert_eq!(kind_default_presentation(Kind::Bool), Presentation::Toggle);
        assert_eq!(
            kind_default_presentation(Kind::Color),
            Presentation::ColorPicker
        );
        assert_eq!(
            kind_default_presentation(Kind::Position2d),
            Presentation::XyPad
        );
        assert_eq!(
            kind_default_presentation(Kind::Texture),
            Presentation::TexturePreview
        );
    }

    #[test]
    fn audio_level_default_presentation_is_number_input() {
        assert_eq!(
            kind_default_presentation(Kind::AudioLevel),
            Presentation::NumberInput
        );
    }
}
