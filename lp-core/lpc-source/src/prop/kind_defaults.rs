//! [`Kind`]-associated defaults that live with Presentation and [`SrcBinding`], so
//! `lpc_model::prop::kind::Kind` does not depend on `lpc-source`.

use crate::presentation::Presentation;
use crate::prop::src_binding::SrcBinding;
use alloc::string::String;
use lpc_model::bus::ChannelName;
use lpc_model::prop::kind::Kind;

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

/// **Conventional** input [`SrcBinding`] when a slot’s `bind` is absent.
pub fn kind_default_bind(k: Kind) -> Option<SrcBinding> {
    match k {
        Kind::Instant => Some(SrcBinding::Bus(ChannelName(String::from("time")))),
        Kind::Texture => Some(SrcBinding::Bus(ChannelName(String::from("video/in/0")))),
        Kind::AudioLevel => Some(SrcBinding::Bus(ChannelName(String::from(
            "audio/in/0/level",
        )))),
        _ => None,
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
    fn default_bind_for_instant_is_time() {
        match kind_default_bind(Kind::Instant) {
            Some(SrcBinding::Bus(ChannelName(ch))) => assert_eq!(ch, "time"),
            other => panic!("expected Bus(time), got {other:?}"),
        }
    }

    #[test]
    fn default_bind_for_color_is_none() {
        assert!(kind_default_bind(Kind::Color).is_none());
    }

    #[test]
    fn audio_level_default_presentation_is_number_input() {
        assert_eq!(
            kind_default_presentation(Kind::AudioLevel),
            Presentation::NumberInput
        );
    }

    #[test]
    fn audio_level_default_bind_is_audio_in_level() {
        match kind_default_bind(Kind::AudioLevel) {
            Some(SrcBinding::Bus(ChannelName(ch))) => assert_eq!(ch, "audio/in/0/level"),
            other => panic!("expected Bus(audio/in/0/level), got {other:?}"),
        }
    }
}
