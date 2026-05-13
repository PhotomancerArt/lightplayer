use lpc_model::{
    Revision, SlotAccess, SlotData, SlotEnum, SlotMapDyn, SlotOptionDyn, SlotRecord, WithRevision,
};
use lpc_wire::{
    decode_slot_data_toml_with_ignored_fields, encode_slot_data_access_toml, snapshot_slot_root,
    write_slot_data_json,
};

use crate::engine::MockRuntime;

#[test]
fn mock_disk_toml_roots_decode_through_slot_shapes() {
    let runtime = MockRuntime::new();

    for (name, kind, root) in persisted_roots(&runtime) {
        let shape = runtime.registry.get(&root.shape_id()).expect("root shape");
        let mut encoded =
            encode_slot_data_access_toml(shape, root.data(), &runtime.registry).unwrap();
        encoded
            .as_table_mut()
            .expect("root table")
            .insert("kind".to_string(), toml::Value::String(kind.to_string()));

        let toml_text = toml::to_string_pretty(&encoded).unwrap();
        println!("{name}.toml\n{toml_text}");

        let parsed: toml::Value = toml::from_str(&toml_text).unwrap();
        let decoded =
            decode_slot_data_toml_with_ignored_fields(shape, &parsed, &runtime.registry, &["kind"])
                .unwrap();
        let expected = snapshot_slot_root(&root.shape_id(), root.data(), &runtime.registry);

        assert_eq!(
            normalize_revisions(decoded),
            normalize_revisions(expected),
            "decoded TOML root {name}"
        );
    }
}

#[test]
fn mock_wire_json_roots_use_direct_slot_writer_shape() {
    let runtime = MockRuntime::new();

    for (name, _, root) in persisted_roots(&runtime) {
        let json = wrap_direct_json_data(&runtime, root);
        let decoded: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(decoded.get("data").is_some(), "direct JSON root {name}");
        let data: SlotData = serde_json::from_value(decoded["data"].clone()).unwrap();
        let expected = snapshot_slot_root(&root.shape_id(), root.data(), &runtime.registry);
        assert_eq!(data, expected, "direct JSON root {name}");
    }
}

#[test]
fn mock_toml_rejects_unknown_domain_fields() {
    let runtime = MockRuntime::new();
    let root = &runtime.shader_def as &dyn SlotAccess;
    let shape = runtime
        .registry
        .get(&root.shape_id())
        .expect("shader shape");
    let mut encoded = encode_slot_data_access_toml(shape, root.data(), &runtime.registry).unwrap();
    encoded.as_table_mut().expect("root table").insert(
        "surprise".to_string(),
        toml::Value::String("nope".to_string()),
    );

    let error =
        decode_slot_data_toml_with_ignored_fields(shape, &encoded, &runtime.registry, &["kind"])
            .unwrap_err();

    assert!(error.message().contains("unknown authored field"));
}

#[test]
fn mock_toml_requires_enum_discriminators() {
    let runtime = MockRuntime::new();
    let root = &runtime.fixture_def as &dyn SlotAccess;
    let shape = runtime
        .registry
        .get(&root.shape_id())
        .expect("fixture shape");
    let mut encoded = encode_slot_data_access_toml(shape, root.data(), &runtime.registry).unwrap();
    encoded["mapping"]
        .as_table_mut()
        .expect("mapping table")
        .remove("kind");

    let error =
        decode_slot_data_toml_with_ignored_fields(shape, &encoded, &runtime.registry, &["kind"])
            .unwrap_err();

    assert_eq!(error.path(), "mapping");
    assert!(error.message().contains("expected enum discriminator"));
}

#[test]
fn mock_toml_rejects_unknown_enum_discriminators() {
    let runtime = MockRuntime::new();
    let root = &runtime.fixture_def as &dyn SlotAccess;
    let shape = runtime
        .registry
        .get(&root.shape_id())
        .expect("fixture shape");
    let mut encoded = encode_slot_data_access_toml(shape, root.data(), &runtime.registry).unwrap();
    encoded["mapping"]
        .as_table_mut()
        .expect("mapping table")
        .insert(
            "kind".to_string(),
            toml::Value::String("hex_grid".to_string()),
        );

    let error =
        decode_slot_data_toml_with_ignored_fields(shape, &encoded, &runtime.registry, &["kind"])
            .unwrap_err();

    assert_eq!(error.path(), "mapping");
    assert!(error.message().contains("unknown enum variant"));
}

fn persisted_roots<'a>(
    runtime: &'a MockRuntime,
) -> Vec<(&'static str, &'static str, &'a dyn SlotAccess)> {
    vec![
        ("project", "project", &runtime.project),
        ("shader", "shader", &runtime.shader_def),
        ("texture", "texture", &runtime.texture_def),
        ("output", "output", &runtime.output_def),
        ("fixture", "fixture", &runtime.fixture_def),
    ]
}

fn wrap_direct_json_data(runtime: &MockRuntime, root: &dyn SlotAccess) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = lpc_wire::json::json_writer::JsonWriter::new(&mut out);
    let mut object = writer.object().unwrap();
    write_slot_data_json(
        object.prop("data").unwrap(),
        &root.shape_id(),
        root.data(),
        &runtime.registry,
    )
    .unwrap();
    object.finish().unwrap();
    out
}

fn normalize_revisions(data: SlotData) -> SlotData {
    match data {
        SlotData::Unit { .. } => SlotData::Unit {
            revision: Revision::default(),
        },
        SlotData::Value(value) => SlotData::Value(WithRevision::new(
            Revision::default(),
            value.value().clone(),
        )),
        SlotData::Record(record) => SlotData::Record(SlotRecord::with_revision(
            Revision::default(),
            record.fields.into_iter().map(normalize_revisions).collect(),
        )),
        SlotData::Map(map) => SlotData::Map(SlotMapDyn::with_revision(
            Revision::default(),
            map.entries
                .into_iter()
                .map(|(key, value)| (key, normalize_revisions(value)))
                .collect(),
        )),
        SlotData::Enum(en) => SlotData::Enum(SlotEnum::with_version(
            Revision::default(),
            en.variant,
            normalize_revisions(*en.data),
        )),
        SlotData::Option(option) => SlotData::Option(SlotOptionDyn {
            presence_revision: Revision::default(),
            data: option.data.map(|data| Box::new(normalize_revisions(*data))),
        }),
    }
}
