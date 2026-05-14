use lpc_model::SlotShapeRegistry;
use lpc_wire::slot::{
    JsonSyntaxSource, SlotJsonWriter, SlotReader, SyntaxError, SyntaxEventSource, TomlSyntaxSource,
};

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

    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(TomlSyntaxSource::new(&toml).unwrap(), &registry);

    assert_eq!(
        read_manual_wire_config(&mut reader).unwrap(),
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

#[test]
fn native_stream_discriminator_reports_valid_values() {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(
        JsonSyntaxSource::new(r#"{"kind":"Blark12","pin":18}"#).unwrap(),
        &registry,
    );

    let error = read_manual_node_def(&mut reader).unwrap_err();

    assert!(error.message().contains("Blark12"));
    assert!(error.message().contains("TextureDef"));
    assert!(error.message().contains("OutputDef"));
}

fn read_manual_wire_config_json(json: &str) -> ManualWireConfig {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(JsonSyntaxSource::new(json).unwrap(), &registry);
    read_manual_wire_config(&mut reader).unwrap()
}

fn read_manual_wire_config<S>(
    reader: &mut SlotReader<'_, S>,
) -> Result<ManualWireConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["brightness", "pin", "label", "enabled", "rings", "payload"];

    let mut brightness = None;
    let mut pin = None;
    let mut label = None;
    let mut enabled = None;
    let mut rings = None;
    let mut payload = None;

    let mut object = reader.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "brightness" => brightness = Some(prop.value().f32()?),
            "pin" => pin = Some(prop.value().u32()?),
            "label" => label = Some(prop.value().string()?),
            "enabled" => enabled = Some(prop.value().bool()?),
            "rings" => {
                let mut ring_values = Vec::new();
                let mut array = prop.value().array()?;
                while let Some(item) = array.next_item()? {
                    ring_values.push(item.u32()?);
                }
                rings = Some(ring_values);
            }
            "payload" => payload = Some(prop.value().binary_base64_tuple()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }

    Ok(ManualWireConfig {
        brightness: brightness.unwrap(),
        pin: pin.unwrap(),
        label: label.unwrap(),
        enabled: enabled.unwrap(),
        rings: rings.unwrap(),
        payload: payload.unwrap(),
    })
}

fn read_manual_node_def<S>(reader: &mut SlotReader<'_, S>) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    reader.start_object()?;
    let kind = reader.expect_discriminator("kind")?.string()?;
    match kind.as_str() {
        "TextureDef" | "OutputDef" => Ok(()),
        other => {
            Err(reader.invalid_discriminator_value("kind", other, &["TextureDef", "OutputDef"]))
        }
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
