use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::nodes::fixture::{
    Brightness, FixtureDiagnosticMode, FixturePower, FixtureSamplingConfig, MappingConfig,
    VisualConsumerSpace,
};
use crate::{
    Affine2dSlot, BindingDefs, Dim2u, Dim2uSlot, EnumSlot, FromLpValue, LpType, LpValue,
    OptionSlot, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, Slotted,
    StaticLpType, StaticSlotEnumOption, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
    VisualProductSlot,
};

/// Authored fixture node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct FixtureDef {
    /// Visual product this fixture samples each frame. Runtime dataflow
    /// input — resolved through the binding graph, never authored as a
    /// value (declared so the wiring is first-class schema, roadmap D8).
    #[slot(consumed, default_bind = "bus:visual.out")]
    pub input: VisualProductSlot,
    /// Full-frame render size used when materializing the fixture input.
    pub render_size: Dim2uSlot,
    /// Authored slot bindings for fixture inputs.
    pub bindings: BindingDefs,
    /// Visual sampling strategy.
    pub sampling: ValueSlot<FixtureSamplingConfig>,
    /// Fixture-level hardware diagnostic pattern.
    pub diagnostic_mode: ValueSlot<FixtureDiagnosticMode>,
    /// Fixture mapping definition.
    pub mapping: EnumSlot<MappingConfig>,
    /// Whether this fixture's strip/lamp order carries meaning ("does strip
    /// order mean something?", dimensionality-first-class vision D3).
    /// Defaults true (a bare strip is `{1D}`); a serpentine matrix author
    /// sets it false. Enforced by consumers, not the model — this slot only
    /// carries the authored bit. Model layer only: not yet read by the
    /// engine.
    pub strip_order_meaningful: ValueSlot<bool>,
    /// Reverse the wire order on the along-the-wire (1D) sampling path:
    /// lamp `k` reads strip position `N-1-k` instead of `k`. Only read
    /// when a wire-order 1D request actually happens (strip order
    /// meaningful and a 1D-primary source); the mapped 2D path never
    /// looks at it — map2d's per-object `reversed` is map geometry, a
    /// different thing. INTERIM by design (strip-order unification
    /// addendum, 2026-08-09): the mapping-patching work's per-range
    /// `reversed` (slice 1) absorbs this whole-fixture bit later.
    pub wire_reversed: ValueSlot<bool>,
    /// This fixture's consumer-side space policy (vision D14): the answer
    /// side of the two-sided space declaration, mirroring
    /// [`crate::ShaderSpace`] on the producer side. Defaults to `Auto`
    /// (defaults-only policy, never force). Model layer only: not yet read
    /// by the engine.
    pub consume: EnumSlot<VisualConsumerSpace>,
    /// Color order for RGB channels.
    pub color_order: ValueSlot<ColorOrder>,
    /// Texture-space 2D affine transform.
    pub transform: Affine2dSlot,
    /// Brightness amplitude (0–1) — the fixture card's front-panel fader
    /// ([`Brightness`] carries the slider hint). A linear light scale,
    /// applied after the gamma encode: half the slider is half the photons
    /// (which the eye reads as ~78% brightness). Default-bound to the bus
    /// `brightness` channel with `panel = "show"`: the fader is public with
    /// zero authoring, and every fixture in a scope shares the one master
    /// fader (one `(scope, channel)` is one control).
    #[slot(consumed, default_bind = "bus:brightness", panel = "show")]
    pub brightness: OptionSlot<ValueSlot<Brightness>>,
    /// Enable gamma correction.
    pub gamma_correction: OptionSlot<ValueSlot<bool>>,
    /// Lamp type and supply budget. Absent means the default guard applies
    /// ([`FixturePower::default`], 1000 mA); a stated budget of zero is the
    /// explicit unlimited opt-out.
    pub power: OptionSlot<ValueSlot<FixturePower>>,
}

impl Default for FixtureDef {
    fn default() -> Self {
        Self {
            input: VisualProductSlot::default(),
            render_size: default_render_size(),
            bindings: BindingDefs::default(),
            sampling: ValueSlot::new(FixtureSamplingConfig::default()),
            diagnostic_mode: ValueSlot::new(FixtureDiagnosticMode::default()),
            mapping: EnumSlot::default(),
            strip_order_meaningful: ValueSlot::new(true),
            wire_reversed: ValueSlot::new(false),
            consume: EnumSlot::default(),
            color_order: ValueSlot::default(),
            transform: Affine2dSlot::default(),
            brightness: default_brightness(),
            gamma_correction: default_gamma_correction(),
            power: OptionSlot::none(),
        }
    }
}

impl FixtureDef {
    pub const KIND: &'static str = "fixture";

    pub fn render_width(&self) -> u32 {
        self.render_size.value().width
    }

    pub fn render_height(&self) -> u32 {
        self.render_size.value().height
    }

    pub fn color_order(&self) -> ColorOrder {
        *self.color_order.value()
    }

    pub fn brightness_u8(&self) -> u8 {
        self.brightness
            .data
            .as_ref()
            .map(|value| value.value().as_u8())
            .unwrap_or(Brightness::DEFAULT.as_u8())
    }

    pub fn gamma_correction(&self) -> bool {
        self.gamma_correction
            .data
            .as_ref()
            .is_none_or(|value| *value.value())
    }

    pub fn transform_matrix(&self) -> [[f32; 4]; 4] {
        let transform = self.transform.value();
        [
            [transform.m00, transform.m01, 0.0, transform.tx],
            [transform.m10, transform.m11, 0.0, transform.ty],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Fixture
    }
}

fn default_brightness() -> OptionSlot<ValueSlot<Brightness>> {
    OptionSlot::some(ValueSlot::new(Brightness::DEFAULT))
}

fn default_render_size() -> Dim2uSlot {
    Dim2uSlot::new(Dim2u {
        width: 16,
        height: 16,
    })
}

fn default_gamma_correction() -> OptionSlot<ValueSlot<bool>> {
    OptionSlot::some(ValueSlot::new(true))
}

/// Color order for RGB channels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ColorOrder {
    /// Red, Green, Blue.
    Rgb,
    /// Green, Red, Blue.
    #[default]
    Grb,
    /// Red, Blue, Green.
    Rbg,
    /// Green, Blue, Red.
    Gbr,
    /// Blue, Red, Green.
    Brg,
    /// Blue, Green, Red.
    Bgr,
}

impl ColorOrder {
    /// Get color order as string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorOrder::Rgb => "rgb",
            ColorOrder::Grb => "grb",
            ColorOrder::Rbg => "rbg",
            ColorOrder::Gbr => "gbr",
            ColorOrder::Brg => "brg",
            ColorOrder::Bgr => "bgr",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rgb" => Some(Self::Rgb),
            "grb" => Some(Self::Grb),
            "rbg" => Some(Self::Rbg),
            "gbr" => Some(Self::Gbr),
            "brg" => Some(Self::Brg),
            "bgr" => Some(Self::Bgr),
            _ => None,
        }
    }

    /// Get bytes per pixel.
    pub fn bytes_per_pixel(&self) -> usize {
        3
    }

    /// Write RGB values to buffer in the correct order.
    pub fn write_rgb(&self, buffer: &mut [u8], offset: usize, r: u8, g: u8, b: u8) {
        if offset + 3 > buffer.len() {
            return;
        }
        match self {
            ColorOrder::Rgb => {
                buffer[offset] = r;
                buffer[offset + 1] = g;
                buffer[offset + 2] = b;
            }
            ColorOrder::Grb => {
                buffer[offset] = g;
                buffer[offset + 1] = r;
                buffer[offset + 2] = b;
            }
            ColorOrder::Rbg => {
                buffer[offset] = r;
                buffer[offset + 1] = b;
                buffer[offset + 2] = g;
            }
            ColorOrder::Gbr => {
                buffer[offset] = g;
                buffer[offset + 1] = b;
                buffer[offset + 2] = r;
            }
            ColorOrder::Brg => {
                buffer[offset] = b;
                buffer[offset + 1] = r;
                buffer[offset + 2] = g;
            }
            ColorOrder::Bgr => {
                buffer[offset] = b;
                buffer[offset + 1] = g;
                buffer[offset + 2] = r;
            }
        }
    }

    /// Write 16-bit RGB values to buffer in the correct order.
    pub fn write_rgb_u16(&self, buffer: &mut [u16], offset: usize, r: u16, g: u16, b: u16) {
        if offset + 3 > buffer.len() {
            return;
        }
        match self {
            ColorOrder::Rgb => {
                buffer[offset] = r;
                buffer[offset + 1] = g;
                buffer[offset + 2] = b;
            }
            ColorOrder::Grb => {
                buffer[offset] = g;
                buffer[offset + 1] = r;
                buffer[offset + 2] = b;
            }
            ColorOrder::Rbg => {
                buffer[offset] = r;
                buffer[offset + 1] = b;
                buffer[offset + 2] = g;
            }
            ColorOrder::Gbr => {
                buffer[offset] = g;
                buffer[offset + 1] = b;
                buffer[offset + 2] = r;
            }
            ColorOrder::Brg => {
                buffer[offset] = b;
                buffer[offset + 1] = r;
                buffer[offset + 2] = g;
            }
            ColorOrder::Bgr => {
                buffer[offset] = b;
                buffer[offset + 1] = g;
                buffer[offset + 2] = r;
            }
        }
    }
}

impl ToLpValue for ColorOrder {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for ColorOrder {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(&value)
                .ok_or_else(|| ValueRootError::new("expected RGB color order value")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ColorOrder {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("ColorOrder");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::String,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Dropdown {
                options: &[
                    StaticSlotEnumOption {
                        value: "rgb",
                        label: "RGB",
                    },
                    StaticSlotEnumOption {
                        value: "grb",
                        label: "GRB",
                    },
                    StaticSlotEnumOption {
                        value: "rbg",
                        label: "RBG",
                    },
                    StaticSlotEnumOption {
                        value: "gbr",
                        label: "GBR",
                    },
                    StaticSlotEnumOption {
                        value: "brg",
                        label: "BRG",
                    },
                    StaticSlotEnumOption {
                        value: "bgr",
                        label: "BGR",
                    },
                ],
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("rgb", "RGB"),
                    SlotEnumOption::new("grb", "GRB"),
                    SlotEnumOption::new("rbg", "RBG"),
                    SlotEnumOption::new("gbr", "GBR"),
                    SlotEnumOption::new("brg", "BRG"),
                    SlotEnumOption::new("bgr", "BGR"),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NodeKind;
    use crate::nodes::fixture::mapping::PathSpec;
    use crate::{Affine2d, FixtureDefView, MapSlot, SlotPath, SlotShapeRegistry};
    use lp_collection::VecMap;

    #[test]
    fn test_fixture_def_kind() {
        let mut paths = VecMap::new();
        paths.insert(0, EnumSlot::new(PathSpec::point_list(0, [[0.5, 0.5]])));
        let def = FixtureDef {
            input: VisualProductSlot::default(),
            render_size: default_render_size(),
            bindings: BindingDefs::default(),
            sampling: ValueSlot::new(FixtureSamplingConfig::TextureArea),
            diagnostic_mode: ValueSlot::new(FixtureDiagnosticMode::Off),
            mapping: EnumSlot::new(MappingConfig::path_points(MapSlot::new(paths), 2.0)),
            strip_order_meaningful: ValueSlot::new(true),
            wire_reversed: ValueSlot::new(false),
            consume: EnumSlot::default(),
            color_order: ValueSlot::new(ColorOrder::Rgb),
            transform: Affine2dSlot::new(Affine2d::identity()),
            brightness: OptionSlot::none(),
            gamma_correction: OptionSlot::none(),
            power: OptionSlot::none(),
        };
        assert_eq!(def.kind(), NodeKind::Fixture);
    }

    #[test]
    fn test_color_order_as_str() {
        assert_eq!(ColorOrder::Rgb.as_str(), "rgb");
        assert_eq!(ColorOrder::Grb.as_str(), "grb");
        assert_eq!(ColorOrder::Bgr.as_str(), "bgr");
    }

    #[test]
    fn test_color_order_bytes_per_pixel() {
        assert_eq!(ColorOrder::Rgb.bytes_per_pixel(), 3);
        assert_eq!(ColorOrder::Grb.bytes_per_pixel(), 3);
    }

    #[test]
    fn test_color_order_write_rgb() {
        let mut buffer = [0u8; 10];

        ColorOrder::Rgb.write_rgb(&mut buffer, 0, 100, 200, 255);
        assert_eq!(buffer[0], 100);
        assert_eq!(buffer[1], 200);
        assert_eq!(buffer[2], 255);

        ColorOrder::Grb.write_rgb(&mut buffer, 3, 100, 200, 255);
        assert_eq!(buffer[3], 200);
        assert_eq!(buffer[4], 100);
        assert_eq!(buffer[5], 255);

        ColorOrder::Bgr.write_rgb(&mut buffer, 6, 100, 200, 255);
        assert_eq!(buffer[6], 255);
        assert_eq!(buffer[7], 200);
        assert_eq!(buffer[8], 100);
    }

    #[test]
    fn test_color_order_write_rgb_bounds_check() {
        let mut buffer = [0u8; 2];
        ColorOrder::Rgb.write_rgb(&mut buffer, 0, 100, 200, 255);
    }

    #[test]
    fn generated_fixture_def_view_compiles() {
        let registry = SlotShapeRegistry::default();

        let view = FixtureDefView::compile(&registry).expect("fixture def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(
            view.render_size().path(),
            &SlotPath::parse("render_size").unwrap()
        );
        assert_eq!(
            view.color_order().path(),
            &SlotPath::parse("color_order").unwrap()
        );
        assert_eq!(
            view.brightness().path(),
            &SlotPath::parse("brightness").unwrap()
        );
        assert_eq!(
            view.gamma_correction().path(),
            &SlotPath::parse("gamma_correction").unwrap()
        );
        assert_eq!(
            view.strip_order_meaningful().path(),
            &SlotPath::parse("strip_order_meaningful").unwrap()
        );
        assert_eq!(view.consume().path(), &SlotPath::parse("consume").unwrap());
    }

    #[test]
    fn fixture_def_defaults_strip_order_meaningful_true_and_consume_auto() {
        let def = FixtureDef::default();
        assert!(*def.strip_order_meaningful.value());
        assert_eq!(*def.consume.value(), VisualConsumerSpace::Auto);
    }
}
