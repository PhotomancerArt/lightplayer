use crate::{BindingDefs, ControlProductSlot, HwEndpointSpec, OptionSlot, Slotted, ValueSlot};

pub const DEFAULT_OUTPUT_ENDPOINT_SPEC: &str = "ws281x:rmt:D10";

/// Authored hardware output node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct OutputDef {
    /// Control product this output drives each frame. Runtime dataflow
    /// input — resolved through the binding graph, never authored as a
    /// value (declared so the wiring is first-class schema, roadmap D8).
    #[slot(consumed, default_bind = "bus:control.out")]
    pub input: ControlProductSlot,
    pub endpoint: ValueSlot<HwEndpointSpec>,
    /// Authored slot bindings for output inputs.
    pub bindings: BindingDefs,
    /// Optional display pipeline options.
    pub options: OptionSlot<OutputDriverOptionsConfig>,
    /// Light every channel of this output solid white, bypassing the graph.
    ///
    /// A `Debug` slot: diagnostics only ("is this pin wired to that strip?"),
    /// never authored into a project file and never saved. It survives the
    /// client that set it and dies on unload or reboot.
    #[slot(role = "debug")]
    pub test_pattern: ValueSlot<bool>,
}

impl OutputDef {
    pub const KIND: &'static str = "output";

    pub fn new(endpoint: HwEndpointSpec) -> Self {
        Self {
            input: ControlProductSlot::default(),
            endpoint: ValueSlot::new(endpoint),
            bindings: BindingDefs::default(),
            options: OptionSlot::none(),
            test_pattern: ValueSlot::new(false),
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
            interpolation_enabled: default_true_slot(),
            dithering_enabled: default_true_slot(),
            lut_enabled: default_true_slot(),
        }
    }
}

fn default_white_point_slot() -> ValueSlot<[f32; 3]> {
    ValueSlot::new([0.9, 1.0, 1.0])
}

fn default_true_slot() -> ValueSlot<bool> {
    ValueSlot::new(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::kind::NodeKind;
    use crate::{
        NodeDef, OutputDefView, SlotPath, SlotRole, SlotShape, SlotShapeRegistry, StaticSlotShape,
    };
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
  "options": { "white_point": [0.8, 1.0, 1.0], "dithering_enabled": false }
}"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert_eq!(def.endpoint().as_str(), "ws281x:rmt:D10");
        let opts = def.options().unwrap();
        assert!((opts.white_point.value()[0] - 0.8).abs() < 0.001);
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

    #[test]
    fn test_pattern_is_a_debug_slot() {
        let SlotShape::Record { fields, .. } = OutputDef::slot_shape() else {
            panic!("output def is a record");
        };

        let field = fields
            .iter()
            .find(|field| field.name.as_str() == "test_pattern")
            .expect("test_pattern field");

        assert_eq!(field.role, SlotRole::Debug);
        assert!(field.is_writable());
    }

    #[test]
    fn authored_test_pattern_is_ignored() {
        let json = r#"{ "kind": "Output", "endpoint": "ws281x:rmt:D10", "test_pattern": true }"#;

        let def = NodeDef::read_json(&registry(), json).unwrap();

        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert!(
            !*def.test_pattern.value(),
            "a Debug slot never takes an authored value (D2)"
        );
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }
}
