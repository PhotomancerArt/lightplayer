use lpc_model::{
    LpValue, SlotAccess, SlotData, SlotMapKey, SlotShape, SlotShapeRegistry, StaticSlotShape,
};
use lpc_source::node::{
    ProjectDef, fixture::FixtureDef, output::OutputDef, shader::ShaderDef, texture::TextureDef,
};
use lpc_wire::build_slot_full_sync;

#[test]
fn real_source_defs_sync_as_slot_roots() {
    let project: ProjectDef = read_basic_toml("project.toml");
    let texture: TextureDef = read_basic_toml("texture.toml");
    let shader: ShaderDef = read_basic_toml("shader.toml");
    let output: OutputDef = read_basic_toml("output.toml");
    let fixture: FixtureDef = read_basic_toml("fixture.toml");

    let mut registry = SlotShapeRegistry::default();
    lpc_source::slot_shapes::register_all_static_slot_shapes(&mut registry).unwrap();

    println!("server loaded");
    print_root(
        "project",
        ProjectDef::SHAPE_ID.slot_shape_from(&registry),
        &project.data().into_owned(&ProjectDef::SHAPE_ID, &registry),
        &registry,
    );
    print_root(
        "shader",
        ShaderDef::SHAPE_ID.slot_shape_from(&registry),
        &shader.data().into_owned(&ShaderDef::SHAPE_ID, &registry),
        &registry,
    );

    println!("syncing source roots");
    let sync = build_slot_full_sync(
        &registry,
        [
            ("project", &project as &dyn lpc_model::SlotAccess),
            ("texture", &texture as &dyn lpc_model::SlotAccess),
            ("shader", &shader as &dyn lpc_model::SlotAccess),
            ("output", &output as &dyn lpc_model::SlotAccess),
            ("fixture", &fixture as &dyn lpc_model::SlotAccess),
        ],
    );
    println!("full sync roots:");
    for root in &sync.roots {
        println!("  {} shape={}", root.name, root.shape);
    }

    let project_data = root_data(&sync, "project");
    assert_eq!(
        select(
            project_data,
            ProjectDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "nodes[shader].artifact"
        ),
        &SlotData::Value(lpc_model::Versioned::new(
            project.nodes.entries["shader"].artifact.changed_frame(),
            LpValue::String(String::from("./shader.toml")),
        )),
    );

    let shader_data = root_data(&sync, "shader");
    assert_value(
        select(
            shader_data,
            ShaderDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "glsl_path",
        ),
        LpValue::String(String::from("shader.glsl")),
    );
    assert_value(
        select(
            shader_data,
            ShaderDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "texture_loc",
        ),
        LpValue::String(String::from("..texture")),
    );
    assert_value(
        select(
            shader_data,
            ShaderDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "glsl_opts.add_sub",
        ),
        LpValue::String(String::from("wrapping")),
    );

    let shader_with_params: ShaderDef = toml::from_str(
        r#"
kind = "shader"
glsl_path = "shader.glsl"
texture_loc = "..texture"
render_order = 0

[param_defs.speed]
label = "Speed"
description = "Animation speed"
value_type = "f32"
default = 0.25

[param_defs.speed.min]
value = 0.0
"#,
    )
    .unwrap();
    let sync = build_slot_full_sync(
        &registry,
        [("shader", &shader_with_params as &dyn lpc_model::SlotAccess)],
    );
    let shader_data = root_data(&sync, "shader");
    assert_value(
        select(
            shader_data,
            ShaderDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "param_defs[speed].label",
        ),
        LpValue::String(String::from("Speed")),
    );

    let texture_sync = build_slot_full_sync(
        &registry,
        [("texture", &texture as &dyn lpc_model::SlotAccess)],
    );
    let texture_data = root_data(&texture_sync, "texture");
    assert!(matches!(
        select(
            texture_data,
            TextureDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "size"
        ),
        SlotData::Value(_)
    ));

    let output_sync = build_slot_full_sync(
        &registry,
        [("output", &output as &dyn lpc_model::SlotAccess)],
    );
    let output_data = root_data(&output_sync, "output");
    assert_value(
        select(
            output_data,
            OutputDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "options.some.brightness",
        ),
        LpValue::F32(0.12),
    );

    let fixture_sync = build_slot_full_sync(
        &registry,
        [("fixture", &fixture as &dyn lpc_model::SlotAccess)],
    );
    let fixture_data = root_data(&fixture_sync, "fixture");
    assert!(matches!(
        select(
            fixture_data,
            FixtureDef::SHAPE_ID.slot_shape_from(&registry),
            &registry,
            "mapping.path_points.paths[0].ring_array.ring_lamp_counts[8]"
        ),
        SlotData::Value(_)
    ));
}

fn read_basic_toml<T>(name: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/basic")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap();
    toml::from_str(&text).unwrap()
}

fn root_data<'a>(sync: &'a lpc_wire::WireSlotFullSync, name: &str) -> &'a SlotData {
    &sync
        .roots
        .iter()
        .find(|root| root.name == name)
        .unwrap()
        .data
}

fn assert_value(data: &SlotData, expected: LpValue) {
    let SlotData::Value(value) = data else {
        panic!("expected value, got {data:?}");
    };
    assert_eq!(value.value(), &expected);
}

fn select<'a>(
    data: &'a SlotData,
    shape: &'a SlotShape,
    registry: &'a SlotShapeRegistry,
    path: &str,
) -> &'a SlotData {
    let mut data = data;
    let mut shape = shape;
    for segment in path.split('.') {
        if let Some((field, key)) = segment.split_once('[') {
            let key = key.strip_suffix(']').expect("closed key");
            (data, shape) = select_field(data, shape, registry, field);
            (data, shape) = select_key(data, shape, registry, key);
        } else {
            (data, shape) = select_field(data, shape, registry, segment);
        }
    }
    data
}

fn select_field<'a>(
    data: &'a SlotData,
    shape: &'a SlotShape,
    registry: &'a SlotShapeRegistry,
    field: &str,
) -> (&'a SlotData, &'a SlotShape) {
    match (data, resolve_shape(shape, registry)) {
        (SlotData::Record(record), SlotShape::Record { fields, .. }) => {
            let (index, field_shape) = fields
                .iter()
                .enumerate()
                .find(|(_, candidate)| candidate.name.as_str() == field)
                .expect("record field");
            (&record.fields[index], &field_shape.shape)
        }
        (SlotData::Enum(en), SlotShape::Enum { variants, .. }) => {
            assert_eq!(en.variant.as_str(), field);
            let variant = variants
                .iter()
                .find(|candidate| candidate.name.as_str() == field)
                .expect("enum variant");
            (&en.data, &variant.shape)
        }
        (SlotData::Option(option), SlotShape::Option { some, .. }) => {
            assert_eq!(field, "some");
            (option.data.as_ref().expect("option some"), some)
        }
        _ => panic!("cannot select field {field} through {data:?}"),
    }
}

fn select_key<'a>(
    data: &'a SlotData,
    shape: &'a SlotShape,
    registry: &'a SlotShapeRegistry,
    key: &str,
) -> (&'a SlotData, &'a SlotShape) {
    let SlotShape::Map {
        key: key_shape,
        value,
        ..
    } = resolve_shape(shape, registry)
    else {
        panic!("expected map shape");
    };
    let key = match key_shape {
        lpc_model::SlotMapKeyShape::String => SlotMapKey::String(String::from(key)),
        lpc_model::SlotMapKeyShape::I32 => SlotMapKey::I32(key.parse().unwrap()),
        lpc_model::SlotMapKeyShape::U32 => SlotMapKey::U32(key.parse().unwrap()),
    };
    let SlotData::Map(map) = data else {
        panic!("expected map data");
    };
    (map.entries.get(&key).expect("map key"), value)
}

fn resolve_shape<'a>(shape: &'a SlotShape, registry: &'a SlotShapeRegistry) -> &'a SlotShape {
    match shape {
        SlotShape::Ref { id } => registry.get(id).expect("shape ref"),
        other => other,
    }
}

fn print_root(name: &str, shape: &SlotShape, data: &SlotData, registry: &SlotShapeRegistry) {
    println!("server tree: {name}");
    print_data("", shape, data, registry);
}

fn print_data(prefix: &str, shape: &SlotShape, data: &SlotData, registry: &SlotShapeRegistry) {
    match (resolve_shape(shape, registry), data) {
        (SlotShape::Record { fields, .. }, SlotData::Record(record)) => {
            for (index, field) in fields.iter().enumerate() {
                let path = join_path(prefix, field.name.as_str());
                println!("  {path}");
                print_data(&path, &field.shape, &record.fields[index], registry);
            }
        }
        (SlotShape::Map { value, .. }, SlotData::Map(map)) => {
            for (key, child) in &map.entries {
                let path = format!("{prefix}[{}]", map_key_label(key));
                println!("  {path}");
                print_data(&path, value, child, registry);
            }
        }
        (SlotShape::Enum { variants, .. }, SlotData::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|candidate| candidate.name == en.variant)
                .expect("variant");
            let path = join_path(prefix, en.variant.as_str());
            println!("  {path}");
            print_data(&path, &variant.shape, &en.data, registry);
        }
        (SlotShape::Option { some, .. }, SlotData::Option(option)) => {
            if let Some(data) = &option.data {
                let path = join_path(prefix, "some");
                println!("  {path}");
                print_data(&path, some, data, registry);
            }
        }
        _ => {}
    }
}

fn join_path(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        String::from(field)
    } else {
        format!("{prefix}.{field}")
    }
}

fn map_key_label(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

trait ShapeLookup {
    fn slot_shape_from(self, registry: &SlotShapeRegistry) -> &SlotShape;
}

impl ShapeLookup for lpc_model::SlotShapeId {
    fn slot_shape_from(self, registry: &SlotShapeRegistry) -> &SlotShape {
        registry.get(&self).expect("registered shape")
    }
}

trait OwnedSlotData {
    fn into_owned(
        self,
        shape_id: &lpc_model::SlotShapeId,
        registry: &SlotShapeRegistry,
    ) -> SlotData;
}

impl OwnedSlotData for lpc_model::SlotDataAccess<'_> {
    fn into_owned(
        self,
        shape_id: &lpc_model::SlotShapeId,
        registry: &SlotShapeRegistry,
    ) -> SlotData {
        lpc_wire::snapshot_slot_root(shape_id, self, registry)
    }
}
