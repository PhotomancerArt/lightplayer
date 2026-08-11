use alloc::string::String;

use lp_collection::VecMap;

use super::OutputPortDef;
use super::output_name::OutputName;
use crate::{
    BindingDefs, ControlProductSlot, HwEndpointSpec, MapSlot, OptionSlot, Slotted, ValueSlot,
};

pub const DEFAULT_OUTPUT_ENDPOINT_SPEC: &str = "ws281x:local:D10";

/// Authored hardware output node definition.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct OutputDef {
    /// Control products this output drives each frame. Runtime dataflow
    /// input — resolved through the binding graph, never authored as a
    /// value (declared so the wiring is first-class schema, roadmap D8).
    ///
    /// `merge = "fragments"` is the consumer-declared policy that makes an
    /// output an N-producer receiver (D8/D17v): every provider on the bound
    /// channel becomes an **output fragment** rendered into its own
    /// sub-slice of this node's sample buffer, in the resolver's provider
    /// order. Two fixtures on one `control.out` is therefore a composition
    /// here — a strand followed by a panel on the same wire — where the same
    /// two producers on a *visual* slot stay the single-producer ambiguity
    /// they have always been. The policy lives on the receiver precisely so
    /// the two cases can differ while the producers are indistinguishable.
    #[slot(consumed, merge = "fragments", default_bind = "bus:control.out")]
    pub input: ControlProductSlot,
    /// Optional authored display name (D39): how patch entries and the
    /// studio refer to this output — "Box 5", "1". Auto-assigned a numeric
    /// default the first time a patch names this output; user-editable;
    /// never hardware identity. Project-unique among outputs (validated at
    /// resolve, like node-name collisions).
    pub name: OptionSlot<ValueSlot<OutputName>>,
    /// Physical wires this output drives, keyed by port index.
    ///
    /// The node's single control product is split across the ports in key
    /// order, each taking its authored `count` of lamps.
    pub ports: MapSlot<u32, OutputPortDef>,
    /// Authored slot bindings for output inputs.
    pub bindings: BindingDefs,
    /// Optional display pipeline options, shared by every port.
    pub options: OptionSlot<OutputDriverOptionsConfig>,
    /// Light every port of this output solid white, bypassing the graph.
    ///
    /// A `Debug` slot: diagnostics only ("is this pin wired to that strip?"),
    /// never authored into a project file and never saved. It survives the
    /// client that set it and dies on unload or reboot.
    #[slot(role = "debug")]
    pub test_pattern: ValueSlot<bool>,
    /// Pulse a set of this output's wire lamps over the live frame, so the
    /// lamps a patching selection is about to move are findable on the
    /// physical rig ("which strand IS /sector/2?").
    ///
    /// Inclusive lamp ranges in the output's flat wire numbering — the same
    /// numbering patch entries anchor with `at.lamp` — as text:
    /// `"0-29,45,90-119"`. Empty means off. Unparseable segments are skipped,
    /// never fatal: a diagnostic must not be able to stop an output pushing
    /// pixels.
    ///
    /// A `Debug` slot like `test_pattern`, and unlike it an OVERLAY, not a
    /// bypass: the graph keeps rendering and only the named lamps blink, so
    /// the selection reads in the context of the running show.
    #[slot(role = "debug")]
    pub highlight: ValueSlot<String>,
}

impl OutputDef {
    pub const KIND: &'static str = "output";

    /// An output driving `endpoint` as its only wire (no count = whole extent).
    pub fn new(endpoint: HwEndpointSpec) -> Self {
        Self::with_ports([(0, OutputPortDef::new(endpoint))])
    }

    /// An output driving the given `(port index, port)` pairs.
    pub fn with_ports(ports: impl IntoIterator<Item = (u32, OutputPortDef)>) -> Self {
        let mut entries = VecMap::new();
        for (index, port) in ports {
            entries.insert(index, port);
        }
        Self {
            input: ControlProductSlot::default(),
            name: OptionSlot::none(),
            ports: MapSlot::new(entries),
            bindings: BindingDefs::default(),
            options: OptionSlot::none(),
            test_pattern: ValueSlot::new(false),
            highlight: ValueSlot::new(String::new()),
        }
    }

    pub fn default_endpoint() -> HwEndpointSpec {
        HwEndpointSpec::from_static(DEFAULT_OUTPUT_ENDPOINT_SPEC)
    }

    pub fn port_count(&self) -> usize {
        self.ports.entries.len()
    }

    /// The authored name, when one is set.
    pub fn output_name(&self) -> Option<&OutputName> {
        self.name.data.as_ref().map(|slot| slot.value())
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
        let def = OutputDef::new(HwEndpointSpec::from_static("ws281x:local:D10"));
        assert_eq!(def.kind(), NodeKind::Output);
        assert_eq!(port_endpoint(&def, 0), "ws281x:local:D10");
    }

    #[test]
    fn test_output_def_endpoint_json_deserialize() {
        let json = r#"{
  "kind": "Output",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } },
  "options": { "white_point": [0.8, 1.0, 1.0], "dithering_enabled": false }
}"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert_eq!(port_endpoint(&def, 0), "ws281x:local:D10");
        let opts = def.options().unwrap();
        assert!((opts.white_point.value()[0] - 0.8).abs() < 0.001);
        assert!(!*opts.dithering_enabled.value());
        assert!(*opts.interpolation_enabled.value());
    }

    #[test]
    fn output_def_ports_round_trip_json() {
        let json = r#"{
  "kind": "Output",
  "ports": {
    "0": { "endpoint": "ws281x:local:IO18", "count": 100 },
    "2": { "endpoint": "ws281x:local:IO16" }
  }
}"#;

        let def = NodeDef::read_json(&registry(), json).unwrap();

        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert_eq!(def.port_count(), 2);
        let first = def.ports.entries.get(&0).expect("port 0");
        assert_eq!(first.endpoint().as_str(), "ws281x:local:IO18");
        assert_eq!(first.count(), Some(100));
        let second = def.ports.entries.get(&2).expect("port 2");
        assert_eq!(second.endpoint().as_str(), "ws281x:local:IO16");
        assert_eq!(second.count(), None);

        let written = NodeDef::Output(def).write_json(&registry()).expect("write");
        assert_eq!(
            NodeDef::read_json(&registry(), &written).expect("re-read"),
            NodeDef::read_json(&registry(), json).expect("re-read source"),
            "ports survive a write/read round trip: {written}"
        );
    }

    /// The authored `name` slot (D39): parses, validates, round-trips, and
    /// stays absent when unset (single-output projects never grow one).
    #[test]
    fn output_def_name_slot_round_trips_and_stays_optional() {
        let json = r#"{
  "kind": "Output",
  "name": "Box 5",
  "ports": { "0": { "endpoint": "ws281x:local:D10" } }
}"#;
        let def = NodeDef::read_json(&registry(), json).unwrap();
        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert_eq!(def.output_name().unwrap().as_str(), "Box 5");
        let written = NodeDef::Output(def).write_json(&registry()).expect("write");
        assert!(written.contains("Box 5"), "{written}");

        let unnamed = OutputDef::default();
        assert_eq!(unnamed.output_name(), None);
        let written = NodeDef::Output(unnamed)
            .write_json(&registry())
            .expect("write");
        assert!(!written.contains("\"name\""), "{written}");
    }

    #[test]
    fn output_def_rejects_legacy_pin_json() {
        let json = r#"{ "kind": "Output", "pin": 18 }"#;

        let err = NodeDef::read_json(&registry(), json).unwrap_err();

        assert!(format!("{err}").contains("pin"));
    }

    /// Format 2 authored a single top-level `endpoint`. Format 3 replaced it
    /// with wire entries, and the loader must say so rather than silently
    /// producing an output with no wires (A1: version-and-refuse, never
    /// migrate).
    #[test]
    fn output_def_rejects_legacy_endpoint_json() {
        let json = r#"{ "kind": "Output", "endpoint": "ws281x:local:D10" }"#;

        let err = NodeDef::read_json(&registry(), json).unwrap_err();

        assert!(format!("{err}").contains("endpoint"), "{err}");
    }

    /// Format 9 spelled the wire map `channels` (D45 retired the word: a
    /// "channel" is a 512-limited DMX unit, not a lamp wire). The v9→v10
    /// upgrade step rewrites the key; a v10 build refuses the old spelling
    /// rather than silently producing an output with no wires.
    #[test]
    fn output_def_rejects_legacy_channels_json() {
        let json =
            r#"{ "kind": "Output", "channels": { "0": { "endpoint": "ws281x:local:D10" } } }"#;

        let err = NodeDef::read_json(&registry(), json).unwrap_err();

        assert!(format!("{err}").contains("channels"), "{err}");
    }

    #[test]
    fn generated_output_def_view_compiles() {
        let registry = SlotShapeRegistry::default();

        let view = OutputDefView::compile(&registry).expect("output def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert!(view.is_valid_for(&registry));
        assert_eq!(view.ports().path(), &SlotPath::parse("ports").unwrap());
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
        let json = r#"{ "kind": "Output", "ports": { "0": { "endpoint": "ws281x:local:D10" } }, "test_pattern": true }"#;

        let def = NodeDef::read_json(&registry(), json).unwrap();

        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert!(
            !*def.test_pattern.value(),
            "a Debug slot never takes an authored value (D2)"
        );
    }

    #[test]
    fn highlight_is_a_debug_slot() {
        let SlotShape::Record { fields, .. } = OutputDef::slot_shape() else {
            panic!("output def is a record");
        };

        let field = fields
            .iter()
            .find(|field| field.name.as_str() == "highlight")
            .expect("highlight field");

        assert_eq!(field.role, SlotRole::Debug);
        assert!(field.is_writable());
    }

    #[test]
    fn authored_highlight_is_ignored() {
        let json = r#"{ "kind": "Output", "channels": { "0": { "endpoint": "ws281x:local:D10" } }, "highlight": "0-9" }"#;

        let def = NodeDef::read_json(&registry(), json).unwrap();

        let NodeDef::Output(def) = def else {
            panic!("expected output def");
        };
        assert!(
            def.highlight.value().is_empty(),
            "a Debug slot never takes an authored value (D2)"
        );
    }

    fn registry() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn port_endpoint(def: &OutputDef, key: u32) -> &str {
        def.ports
            .entries
            .get(&key)
            .unwrap_or_else(|| panic!("port {key}"))
            .endpoint()
            .as_str()
    }
}
