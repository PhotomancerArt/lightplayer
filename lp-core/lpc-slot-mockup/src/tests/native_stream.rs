use lpc_model::SlotShapeRegistry;
use lpc_wire::slot::{JsonSyntaxSource, SlotJsonWriter, SlotReader, SyntaxNode, TomlSyntaxSource};

#[derive(Debug, PartialEq)]
struct ManualWireConfig {
    brightness: f32,
    pin: u32,
    label: String,
    enabled: bool,
    rings: Vec<u32>,
    payload: Vec<u8>,
}

#[test]
fn native_stream_json_round_trips_manual_wire_config() {
    let config = ManualWireConfig {
        brightness: 0.25,
        pin: 18,
        label: "visual output".to_string(),
        enabled: true,
        rings: vec![1, 8, 12, 16],
        payload: vec![1, 2, 3, 253, 254, 255],
    };

    let mut json = Vec::new();
    write_manual_wire_config_json(&config, &mut json);

    let decoded = read_manual_wire_config_json(std::str::from_utf8(&json).unwrap());

    assert_eq!(decoded, config);
}

#[test]
fn native_stream_toml_uses_same_manual_reader_shape() {
    let toml: toml::Value = toml::from_str(
        r#"
brightness = 0.5
pin = 19
label = "aux output"
enabled = false
rings = [2, 4, 6]
payload = [6, "AQID/f7/"]
"#,
    )
    .unwrap();

    let mut source = TomlSyntaxSource::new(&toml).unwrap();
    let node = SyntaxNode::from_source(&mut source).unwrap();
    let registry = SlotShapeRegistry::default();
    let reader = SlotReader::new(&node, &registry);

    assert_eq!(
        read_manual_wire_config(&reader),
        ManualWireConfig {
            brightness: 0.5,
            pin: 19,
            label: "aux output".to_string(),
            enabled: false,
            rings: vec![2, 4, 6],
            payload: vec![1, 2, 3, 253, 254, 255],
        }
    );
}

fn read_manual_wire_config_json(json: &str) -> ManualWireConfig {
    let mut source = JsonSyntaxSource::new(json).unwrap();
    let node = SyntaxNode::from_source(&mut source).unwrap();
    let registry = SlotShapeRegistry::default();
    let reader = SlotReader::new(&node, &registry);
    read_manual_wire_config(&reader)
}

fn read_manual_wire_config(reader: &SlotReader<'_>) -> ManualWireConfig {
    ManualWireConfig {
        brightness: reader.prop("brightness").unwrap().f32().unwrap(),
        pin: reader.prop("pin").unwrap().u32().unwrap(),
        label: reader.prop("label").unwrap().string().unwrap().to_string(),
        enabled: reader.prop("enabled").unwrap().bool().unwrap(),
        rings: reader
            .prop("rings")
            .unwrap()
            .array_items()
            .unwrap()
            .into_iter()
            .map(|item| item.u32().unwrap())
            .collect(),
        payload: reader
            .prop("payload")
            .unwrap()
            .binary_base64_tuple()
            .unwrap(),
    }
}

fn write_manual_wire_config_json(config: &ManualWireConfig, out: &mut Vec<u8>) {
    let mut writer = SlotJsonWriter::new(out);
    let mut object = writer.object().unwrap();
    object
        .prop("brightness")
        .unwrap()
        .f32(config.brightness)
        .unwrap();
    object.prop("pin").unwrap().u32(config.pin).unwrap();
    object.prop("label").unwrap().string(&config.label).unwrap();
    object
        .prop("enabled")
        .unwrap()
        .bool(config.enabled)
        .unwrap();

    let mut rings = object.prop("rings").unwrap().array().unwrap();
    for ring in &config.rings {
        rings.item().unwrap().u32(*ring).unwrap();
    }
    rings.finish().unwrap();

    object
        .prop("payload")
        .unwrap()
        .binary_base64_tuple(&config.payload)
        .unwrap();
    object.finish().unwrap();
}
