//! Presentation enum (UI widget hint).
//! See docs/design/lightplayer/quantity.md §9.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    Knob,
    Fader,
    Toggle,
    NumberInput,
    Dropdown,
    XyPad,
    ColorPicker,
    PaletteEditor,
    GradientEditor,
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
