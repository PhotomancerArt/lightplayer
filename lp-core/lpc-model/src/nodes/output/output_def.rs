use crate::{BindingDefs, HwEndpointSpec, OptionSlot, Ratio, RatioSlot, Slotted, ValueSlot};

pub const DEFAULT_OUTPUT_ENDPOINT_SPEC: &str = "ws281x:rmt:D10";

/// Authored hardware output node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct OutputDef {
    pub endpoint: ValueSlot<HwEndpointSpec>,
    /// Authored slot bindings for output inputs.
    pub bindings: BindingDefs,
    /// Optional display pipeline options.
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}

impl OutputDef {
    pub const KIND: &'static str = "output";

    pub fn new(endpoint: HwEndpointSpec) -> Self {
        Self {
            endpoint: ValueSlot::new(endpoint),
            bindings: BindingDefs::default(),
            options: OptionSlot::none(),
        }
    }

    pub fn default_endpoint() -> HwEndpointSpec {
        HwEndpointSpec::from_static(DEFAULT_OUTPUT_ENDPOINT_SPEC)
    }

    pub fn endpoint(&self) -> &HwEndpointSpec {
        self.endpoint.value()
    }

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Output
    }

    pub fn options(&self) -> Option<&OutputDriverOptionsConfig> {
        self.options.data.as_ref()
    }
}

impl Default for OutputDef {
    fn default() -> Self {
        Self::new(Self::default_endpoint())
    }
}

/// Authored output driver options for the display pipeline.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct OutputDriverOptionsConfig {
    /// RGB white point balance.
    pub white_point: ValueSlot<[f32; 3]>,
    /// Global brightness multiplier.
    pub brightness: RatioSlot,
    /// Enable interpolation between frames.
    pub interpolation_enabled: ValueSlot<bool>,
    /// Enable temporal dithering.
    pub dithering_enabled: ValueSlot<bool>,
    /// Enable white point LUT.
    pub lut_enabled: ValueSlot<bool>,
}

impl Default for OutputDriverOptionsConfig {
    fn default() -> Self {
        Self {
            white_point: default_white_point_slot(),
            brightness: default_brightness_slot(),
            interpolation_enabled: default_true_slot(),
            dithering_enabled: default_true_slot(),
            lut_enabled: default_true_slot(),
        }
    }
}

fn default_white_point_slot() -> ValueSlot<[f32; 3]> {
    ValueSlot::new([0.9, 1.0, 1.0])
}

fn default_brightness_slot() -> RatioSlot {
    RatioSlot::new(Ratio(1.0))
}

fn default_true_slot() -> ValueSlot<bool> {
    ValueSlot::new(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::kind::NodeKind;
    use crate::{NodeDef, OutputDefView, SlotPath, SlotShapeRegistry};
    use alloc::format;

    #[test]
    fn test_output_def_kind() {
        let def = OutputDef::new(HwEndpointSpec::from_static("ws281x:rmt:D10"));
        assert_eq!(def.kind(), NodeKind::Output);
        assert_eq!(def.endpoint().as_str(), "ws281x:rmt:D10");
    }

    #[test]
    fn test_output_def_endpoint_json_deserialize() {
        let json = r#"{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "options": { "brightness": 0.25, "dithering_enabled": false }
}"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert_eq!(def.endpoint().as_str(), "ws281x:rmt:D10");
        let opts = def.options().unwrap();
        assert!((opts.brightness.value().0 - 0.25).abs() < 0.001);
        assert!(!*opts.dithering_enabled.value());
        assert!(*opts.interpolation_enabled.value());
    }

    #[test]
    fn output_def_rejects_legacy_pin_json() {
        let json = r#"{ "kind": "Output", "pin": 18 }"#;

        let err = NodeDef::read_json(&registry(), json).unwrap_err();

        assert!(format!("{err}").contains("pin"));
    }

    #[test]
    fn generated_output_def_view_compiles() {
        let registry = SlotShapeRegistry::default();

        let view = OutputDefView::compile(&registry).expect("output def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(
            view.endpoint().path(),
            &SlotPath::parse("endpoint").unwrap()
        );
        assert_eq!(view.options().path(), &SlotPath::parse("options").unwrap());
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
