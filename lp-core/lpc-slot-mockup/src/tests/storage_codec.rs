use lpc_model::{
    Revision, SlotAccess, SlotData, SlotEnum, SlotMapDyn, SlotOptionDyn, SlotRecord, WithRevision,
    slot_codec::SlotWriter,
};
use lpc_wire::snapshot_slot_root;

use crate::engine::MockRuntime;

#[test]
fn mock_disk_toml_roots_decode_through_slot_shapes() {
    let runtime = MockRuntime::new();

    for (name, root) in persisted_roots(&runtime) {
        let encoded = runtime
            .registry
            .write_slot_toml_data(root.shape_id(), root.data())
            .unwrap();

        let decoded = runtime
            .registry
            .read_slot_toml(root.shape_id(), &encoded)
            .unwrap();
        let expected = snapshot_slot_root(&root.shape_id(), root.data(), &runtime.registry);
        let actual = snapshot_slot_root(&decoded.shape_id(), decoded.data(), &runtime.registry);

        assert_eq!(
            normalize_revisions(actual),
            normalize_revisions(expected),
            "decoded TOML root {name}"
        );
    }
}

#[test]
fn mock_wire_json_roots_use_direct_slot_writer_shape() {
    let runtime = MockRuntime::new();

    for (name, root) in persisted_roots(&runtime) {
        let json = wrap_direct_json_data(&runtime, root);
        let json = std::str::from_utf8(&json).unwrap();
        assert!(json.contains(r#""data""#), "direct JSON root {name}");
    }
}

#[test]
fn mock_toml_rejects_unknown_domain_fields() {
    let runtime = MockRuntime::new();
    let root = &runtime.shader_def as &dyn SlotAccess;
    let mut encoded = runtime
        .registry
        .write_slot_toml_data(root.shape_id(), root.data())
        .unwrap();
    encoded.as_table_mut().expect("root table").insert(
        "surprise".to_string(),
        toml::Value::String("nope".to_string()),
    );

    let error = runtime
        .registry
        .read_slot_toml(root.shape_id(), &encoded)
        .expect_err_without_debug();

    assert!(error.message().contains("surprise"));
}

#[test]
fn mock_toml_requires_enum_discriminators() {
    let runtime = MockRuntime::new();
    let root = &runtime.fixture_def as &dyn SlotAccess;
    let mut encoded = runtime
        .registry
        .write_slot_toml_data(root.shape_id(), root.data())
        .unwrap();
    encoded["mapping"]
        .as_table_mut()
        .expect("mapping table")
        .remove("kind");

    let error = runtime
        .registry
        .read_slot_toml(root.shape_id(), &encoded)
        .expect_err_without_debug();

    assert!(error.message().contains("kind"));
}

#[test]
fn mock_toml_rejects_unknown_enum_discriminators() {
    let runtime = MockRuntime::new();
    let root = &runtime.fixture_def as &dyn SlotAccess;
    let mut encoded = runtime
        .registry
        .write_slot_toml_data(root.shape_id(), root.data())
        .unwrap();
    encoded["mapping"]
        .as_table_mut()
        .expect("mapping table")
        .insert(
            "kind".to_string(),
            toml::Value::String("hex_grid".to_string()),
        );

    let error = runtime
        .registry
        .read_slot_toml(root.shape_id(), &encoded)
        .expect_err_without_debug();

    assert!(error.message().contains("hex_grid"));
}

fn persisted_roots<'a>(runtime: &'a MockRuntime) -> Vec<(&'static str, &'a dyn SlotAccess)> {
    vec![
        ("project", &runtime.project),
        ("shader", &runtime.shader_def),
        ("texture", &runtime.texture_def),
        ("output", &runtime.output_def),
        ("fixture", &runtime.fixture_def),
    ]
}

fn wrap_direct_json_data(runtime: &MockRuntime, root: &dyn SlotAccess) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = SlotWriter::new(&mut out);
    let mut object = writer.object().unwrap();
    runtime
        .registry
        .write_slot_json_value(root.shape_id(), root.data(), object.prop("data").unwrap())
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

trait ExpectErrWithoutDebug<T, E> {
    fn expect_err_without_debug(self) -> E;
}

impl<T, E> ExpectErrWithoutDebug<T, E> for Result<T, E> {
    fn expect_err_without_debug(self) -> E {
        match self {
            Ok(_) => panic!("expected error"),
            Err(error) => error,
        }
    }
}
