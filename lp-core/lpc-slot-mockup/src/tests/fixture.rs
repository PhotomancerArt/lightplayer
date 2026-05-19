use lpc_model::{
    Revision, SlotAccess, SlotData, SlotMapKey, SlotPath, SlotPathSegment, SlotShapeId,
};
use lpc_view::SlotMirrorView;
use lpc_wire::{WireSlotChange, WireSlotPatch};
use std::sync::{Mutex, MutexGuard};

use crate::{
    engine::MockRuntime,
    wire::{collect_diff, full_sync, print_data_root, print_root},
};

pub struct Harness {
    _log_guard: MutexGuard<'static, ()>,
    pub runtime: MockRuntime,
    pub client: SlotMirrorView,
}

impl Harness {
    pub fn new() -> Self {
        let log_guard = log_guard();

        println!("server loaded");
        Self {
            _log_guard: log_guard,
            runtime: MockRuntime::new(),
            client: SlotMirrorView::default(),
        }
    }

    pub fn sync_full(&mut self) {
        println!("syncing full state to client");
        let sync = full_sync(&self.runtime);
        println!("full sync roots:");
        for root in &sync.roots {
            println!("  {} shape={}", root.name, root.shape);
        }
        self.client.apply_full_sync(sync).unwrap();
        println!("client full sync applied");
    }

    pub fn sync_diff(&mut self, root_name: &str, since: Revision) -> Vec<WireSlotPatch> {
        println!(
            "syncing diff for {root_name} since frame {}",
            since.as_i64()
        );
        let root = self.server_root(root_name);
        let patches = collect_diff(root_name, root, &self.runtime.registry, since);
        print_patches(&patches);
        self.client.apply_patches(&patches).unwrap();
        println!("client diff applied");
        patches
    }

    pub fn sync_registry(&mut self) {
        println!("syncing shape registry to client");
        let snapshot = self.runtime.registry.snapshot();
        println!(
            "registry frame={} shapes={}",
            snapshot.ids_revision.as_i64(),
            snapshot.shapes.len()
        );
        for (shape_id, shape) in &snapshot.shapes {
            println!(
                "  shape {shape_id} revision={} node={:?}",
                shape.changed_at().as_i64(),
                shape.value()
            );
        }
        self.client.apply_registry_snapshot(snapshot);
        println!("client registry applied");
    }

    pub fn print_client_shape(&self, shape_id: SlotShapeId) {
        let shape = self.client.registry.entry(&shape_id).expect("client shape");
        println!(
            "client shape {shape_id} revision={} node={:?}",
            shape.changed_at().as_i64(),
            shape.value()
        );
    }

    pub fn print_server_tree(&self, root_name: &str) {
        println!("server tree: {root_name}");
        print_lines(print_root(
            self.server_root(root_name),
            &self.runtime.registry,
        ));
    }

    pub fn print_client_tree(&self, root_name: &str) {
        println!("client tree: {root_name}");
        let shape = self
            .client
            .root_shapes
            .get(root_name)
            .expect("client shape");
        let data = self.client.roots.get(root_name).expect("client root");
        print_lines(print_data_root(shape, data, &self.client.registry));
    }

    pub fn server_root(&self, root_name: &str) -> &dyn SlotAccess {
        self.runtime
            .roots()
            .into_iter()
            .find(|(name, _)| *name == root_name)
            .map(|(_, root)| root)
            .expect("server root")
    }
}

pub fn log_guard() -> MutexGuard<'static, ()> {
    static TEST_LOG_LOCK: Mutex<()> = Mutex::new(());
    TEST_LOG_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

pub fn print_lines(lines: Vec<String>) {
    for line in lines {
        println!("  {line}");
    }
}

pub fn print_patches(patches: &[WireSlotPatch]) {
    println!("diff:");
    if patches.is_empty() {
        println!("  <empty>");
    }
    for patch in patches {
        println!(
            "  {} {} -> {}",
            patch.root,
            patch.path,
            describe_change(patch)
        );
    }
}

pub fn describe_change(patch: &WireSlotPatch) -> String {
    match &patch.change {
        WireSlotChange::Replace(data) => format!("replace {}", data.get()),
    }
}

pub fn assert_shader_param(data: &SlotData, name: &str, expected: lpc_model::LpValue) {
    let SlotData::Record(shader_node) = data else {
        panic!("shader node record");
    };
    let SlotData::Record(params) = &shader_node.fields[0] else {
        panic!("shader params record");
    };
    let SlotData::Value(value) = &params.fields[shader_param_index(name)] else {
        panic!("shader param value");
    };
    assert_eq!(value.value(), &expected);
}

pub fn assert_shader_param_lacks(data: &SlotData, name: &str) {
    let SlotData::Record(shader_node) = data else {
        panic!("shader node record");
    };
    let SlotData::Record(params) = &shader_node.fields[0] else {
        panic!("shader params record");
    };
    assert!(shader_param_index(name) >= params.fields.len());
}

fn shader_param_index(name: &str) -> usize {
    match name {
        "exposure" => 0,
        "speed" => 1,
        _ => panic!("unknown shader param {name}"),
    }
}

pub fn assert_shader_param_def_type(data: &SlotData, name: &str, expected: &str) {
    let selected = select(data, &format!("param_defs[{name}]"));
    let SlotData::Record(param_def) = selected else {
        panic!("shader param def record");
    };
    let SlotData::Value(value_shape) = &param_def.fields[2] else {
        panic!("shader param def value_type");
    };
    assert_eq!(
        value_shape.value(),
        &lpc_model::LpValue::String(expected.to_string())
    );
}

pub fn assert_shader_param_def_label(data: &SlotData, name: &str, expected: &str) {
    let selected = select(data, &format!("param_defs[{name}]"));
    let SlotData::Record(param_def) = selected else {
        panic!("shader param def record");
    };
    let SlotData::Value(label) = &param_def.fields[0] else {
        panic!("shader param def label");
    };
    assert_eq!(
        label.value(),
        &lpc_model::LpValue::String(expected.to_string())
    );
}

pub fn assert_map_has_key(data: &SlotData, path: &str, key: SlotMapKey) {
    let selected = select(data, path);
    let SlotData::Map(map) = selected else {
        panic!("map at {path}");
    };
    assert!(map.entries.contains_key(&key));
}

pub fn select<'a>(data: &'a SlotData, path: &str) -> &'a SlotData {
    let mut current = data;
    if path.is_empty() {
        return current;
    }
    for segment in SlotPath::parse(path).unwrap().segments() {
        current = match current {
            SlotData::Record(record) => {
                let SlotPathSegment::Field(segment) = segment else {
                    panic!("expected record field segment {segment:?}");
                };
                let index = match segment.as_str() {
                    "source.shader.param_defs" | "param_defs" => 4,
                    "engine.shader_node.params" | "params" => 0,
                    "engine.fixture_node.touches" | "touches" => 0,
                    "mapping" => 3,
                    "paths" => 0,
                    "sample_diameter" => 1,
                    "center" => 0,
                    "diameter" => 1,
                    "start_ring_inclusive" => 2,
                    "end_ring_exclusive" => 3,
                    "ring_lamp_counts" => 4,
                    "offset_angle" => 5,
                    "order" => 6,
                    "transform" => 5,
                    "brightness" => 6,
                    _ => panic!("unknown test record segment {segment}"),
                };
                &record.fields[index]
            }
            SlotData::Map(map) => {
                let SlotPathSegment::Key(segment) = segment else {
                    panic!("expected map key segment {segment:?}");
                };
                map.entries.get(segment).expect("map entry")
            }
            SlotData::Enum(en) => {
                let SlotPathSegment::Field(segment) = segment else {
                    panic!("expected enum variant segment {segment:?}");
                };
                assert_eq!(en.variant.as_str(), segment.as_str());
                &en.data
            }
            SlotData::Option(option) => {
                let SlotPathSegment::Field(segment) = segment else {
                    panic!("expected option segment {segment:?}");
                };
                assert_eq!(segment.as_str(), "some");
                option.data.as_deref().expect("option some")
            }
            SlotData::Unit { .. } => panic!("cannot select through unit"),
            SlotData::Value(_) => panic!("cannot select through value"),
        };
    }
    current
}
