//! Kind: semantic identity of a value. See docs/design/lightplayer/quantity.md §3.

use crate::LpsType;
use crate::binding::Binding;
use crate::constraint::Constraint;
use crate::presentation::Presentation;
use crate::types::ChannelName;
use alloc::boxed::Box;
use alloc::string::String;
use lps_shared::StructMember;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_is_exhaustive_and_concrete() {
        for k in [
            Kind::Amplitude,
            Kind::Ratio,
            Kind::Phase,
            Kind::Count,
            Kind::Bool,
            Kind::Choice,
            Kind::Instant,
            Kind::Duration,
            Kind::Frequency,
            Kind::Angle,
            Kind::Color,
            Kind::ColorPalette,
            Kind::Gradient,
            Kind::Position2d,
            Kind::Position3d,
            Kind::Texture,
        ] {
            let _ = k.storage();
        }
    }

    #[test]
    fn float_scalar_storages() {
        assert_eq!(Kind::Amplitude.storage(), LpsType::Float);
        assert_eq!(Kind::Frequency.storage(), LpsType::Float);
    }

    #[test]
    fn int_scalar_storages() {
        assert_eq!(Kind::Count.storage(), LpsType::Int);
        assert_eq!(Kind::Choice.storage(), LpsType::Int);
    }

    #[test]
    fn position_storages() {
        assert_eq!(Kind::Position2d.storage(), LpsType::Vec2);
        assert_eq!(Kind::Position3d.storage(), LpsType::Vec3);
    }

    #[test]
    fn texture_storage_has_four_int_fields() {
        let s = Kind::Texture.storage();
        match s {
            LpsType::Struct { members, .. } => {
                assert_eq!(members.len(), 4);
                for m in members {
                    assert_eq!(m.ty, LpsType::Int);
                }
            }
            _ => panic!("Texture storage must be a Struct"),
        }
    }

    #[test]
    fn dimension_assignment() {
        assert_eq!(Kind::Instant.dimension(), Dimension::Time);
        assert_eq!(Kind::Duration.dimension(), Dimension::Time);
        assert_eq!(Kind::Frequency.dimension(), Dimension::Frequency);
        assert_eq!(Kind::Angle.dimension(), Dimension::Angle);
        assert_eq!(Kind::Amplitude.dimension(), Dimension::Dimensionless);
        assert_eq!(Kind::Phase.dimension(), Dimension::Dimensionless);
    }

    #[test]
    fn default_presentation_table() {
        assert_eq!(Kind::Amplitude.default_presentation(), Presentation::Fader);
        assert_eq!(Kind::Frequency.default_presentation(), Presentation::Knob);
        assert_eq!(Kind::Bool.default_presentation(), Presentation::Toggle);
        assert_eq!(
            Kind::Color.default_presentation(),
            Presentation::ColorPicker
        );
        assert_eq!(Kind::Position2d.default_presentation(), Presentation::XyPad);
        assert_eq!(
            Kind::Texture.default_presentation(),
            Presentation::TexturePreview
        );
    }

    #[test]
    fn default_bind_for_instant_is_time() {
        match Kind::Instant.default_bind() {
            Some(Binding::Bus { channel }) => assert_eq!(channel.0, "time"),
            other => panic!("expected Bus(time), got {other:?}"),
        }
    }

    #[test]
    fn default_bind_for_color_is_none() {
        assert!(Kind::Color.default_bind().is_none());
    }

    #[test]
    fn default_constraint_for_amplitude_is_unit_range() {
        match Kind::Amplitude.default_constraint() {
            Constraint::Range { min, max, step } => {
                assert_eq!(min, 0.0);
                assert_eq!(max, 1.0);
                assert!(step.is_none());
            }
            _ => panic!("expected Range[0,1]"),
        }
    }

    #[test]
    fn kind_serde_round_trips() {
        let k = Kind::Frequency;
        let s = serde_json::to_string(&k).unwrap();
        assert_eq!(s, "\"frequency\"");
        let back: Kind = serde_json::from_str(&s).unwrap();
        assert_eq!(k, back);
    }
}

pub const MAX_PALETTE_LEN: u32 = 16;
pub const MAX_GRADIENT_STOPS: u32 = 16;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Dimension {
    Dimensionless,
    Time,
    Frequency,
    Angle,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Unit {
    None,
    Seconds,
    Hertz,
    Radians,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum Colorspace {
    Oklch,
    Oklab,
    LinearRgb,
    Srgb,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum InterpMethod {
    Linear,
    Cubic,
    Step,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Amplitude,
    Ratio,
    Phase,
    Count,
    Bool,
    Choice,

    Instant,
    Duration,
    Frequency,
    Angle,

    Color,
    ColorPalette,
    Gradient,
    Position2d,
    Position3d,

    Texture,
}

impl Kind {
    pub fn storage(self) -> LpsType {
        match self {
            Self::Amplitude
            | Self::Ratio
            | Self::Phase
            | Self::Instant
            | Self::Duration
            | Self::Frequency
            | Self::Angle => LpsType::Float,
            Self::Count | Self::Choice => LpsType::Int,
            Self::Bool => LpsType::Bool,
            Self::Position2d => LpsType::Vec2,
            Self::Position3d => LpsType::Vec3,
            Self::Color => color_struct(),
            Self::ColorPalette => color_palette_struct(),
            Self::Gradient => gradient_struct(),
            Self::Texture => texture_struct(),
        }
    }

    pub fn dimension(self) -> Dimension {
        match self {
            Self::Instant | Self::Duration => Dimension::Time,
            Self::Frequency => Dimension::Frequency,
            Self::Angle => Dimension::Angle,
            _ => Dimension::Dimensionless,
        }
    }

    pub fn default_constraint(self) -> Constraint {
        use Constraint::*;
        match self {
            Self::Amplitude | Self::Ratio => Range {
                min: 0.0,
                max: 1.0,
                step: None,
            },
            Self::Phase => Range {
                min: 0.0,
                max: 1.0,
                step: None,
            },
            Self::Count => Range {
                min: 0.0,
                max: 2_147_483_647.0,
                step: None,
            },
            _ => Free,
        }
    }

    pub fn default_presentation(self) -> Presentation {
        use Presentation::*;
        match self {
            Self::Instant | Self::Count => NumberInput,
            Self::Duration | Self::Amplitude | Self::Ratio => Fader,
            Self::Frequency | Self::Angle | Self::Phase => Knob,
            Self::Bool => Toggle,
            Self::Choice => Dropdown,
            Self::Color => ColorPicker,
            Self::ColorPalette => PaletteEditor,
            Self::Gradient => GradientEditor,
            Self::Position2d => XyPad,
            Self::Position3d => NumberInput,
            Self::Texture => TexturePreview,
        }
    }

    pub fn default_bind(self) -> Option<Binding> {
        match self {
            Self::Instant => Some(Binding::Bus {
                channel: ChannelName(String::from("time")),
            }),
            Self::Texture => Some(Binding::Bus {
                channel: ChannelName(String::from("video/in/0")),
            }),
            _ => None,
        }
    }
}

fn color_struct() -> LpsType {
    LpsType::Struct {
        name: Some(String::from("Color")),
        members: alloc::vec![
            StructMember {
                name: Some(String::from("space")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("coords")),
                ty: LpsType::Vec3,
            },
        ],
    }
}

fn color_palette_struct() -> LpsType {
    LpsType::Struct {
        name: Some(String::from("ColorPalette")),
        members: alloc::vec![
            StructMember {
                name: Some(String::from("space")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("count")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("entries")),
                ty: LpsType::Array {
                    element: Box::new(LpsType::Vec3),
                    len: MAX_PALETTE_LEN,
                },
            },
        ],
    }
}

fn gradient_struct() -> LpsType {
    let stop = LpsType::Struct {
        name: Some(String::from("GradientStop")),
        members: alloc::vec![
            StructMember {
                name: Some(String::from("at")),
                ty: LpsType::Float,
            },
            StructMember {
                name: Some(String::from("c")),
                ty: LpsType::Vec3,
            },
        ],
    };
    LpsType::Struct {
        name: Some(String::from("Gradient")),
        members: alloc::vec![
            StructMember {
                name: Some(String::from("space")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("method")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("count")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("stops")),
                ty: LpsType::Array {
                    element: Box::new(stop),
                    len: MAX_GRADIENT_STOPS,
                },
            },
        ],
    }
}

fn texture_struct() -> LpsType {
    LpsType::Struct {
        name: Some(String::from("Texture")),
        members: alloc::vec![
            StructMember {
                name: Some(String::from("format")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("width")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("height")),
                ty: LpsType::Int,
            },
            StructMember {
                name: Some(String::from("handle")),
                ty: LpsType::Int,
            },
        ],
    }
}
