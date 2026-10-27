//! The port tables a fixture patches against (mapping rebuild M2 P1): which
//! outputs a fixture reaches, by name, with each port where its wire is cut,
//! and a key that moves on any name or port edit.
//!
//! Each project is loaded from memory the way a device loads one, so the
//! outputs have authored defs, real bindings and the loader's scopes; the
//! question is asked through the engine's own host, outside a tick, as the
//! probes ask theirs.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactLocation, LpValue, MutationOp, NodeId, NodeName, SlotEdit, SlotPath, TreePath,
    current_revision,
};
use lpc_registry::ParseCtx;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

use super::engine::{control_patch_outputs_with_engine_host, port_table_questions};
use super::engine_services::planned_wire_slices;
use super::{EngineServices, LoadedProjectRuntime, ProjectLoader};
use crate::node::{OutputPortSlice, OutputPortTable, PortTablesKey, ReachableOutputs};

/// A port's start and capacity are the ones the output's wires are cut at.
#[test]
fn a_ports_start_and_capacity_are_where_its_wire_is_cut() {
    let mut project = Project::load(&[Output::named("main", MAIN_PORTS)]);
    let tables = project.reachable().outputs;
    let main = table(&tables, "main");

    let def = project.output_def("main");
    let wires = planned_wire_slices(&def);
    for (key, start, len) in &wires {
        let port = main
            .ports
            .iter()
            .find(|port| port.key == *key)
            .expect("every wire is a port in the table");
        assert_eq!(
            (port.start_samples, port.capacity_samples),
            (*start, *len),
            "port {key}"
        );
    }
    assert_eq!(
        wires.len(),
        3,
        "the wires: ports 0, 2 and 5 (port 1 has none)"
    );
}

/// Ports are offered in key order, gaps in the keys kept.
#[test]
fn ports_are_offered_in_key_order_with_gaps_kept() {
    let mut project = Project::load(&[Output::named("main", MAIN_PORTS)]);
    let tables = project.reachable().outputs;
    let keys: Vec<u32> = table(&tables, "main").ports.iter().map(|p| p.key).collect();
    assert_eq!(keys, vec![0, 1, 2, 5]);
}

/// A port authored with no lamps is offered with no room.
#[test]
fn a_port_authored_with_no_lamps_is_offered_with_no_room() {
    let mut project = Project::load(&[Output::named("main", MAIN_PORTS)]);
    let tables = project.reachable().outputs;
    assert_eq!(
        table(&tables, "main").ports,
        vec![
            slice(0, 0, Some(30)),
            slice(1, 30, Some(0)),
            slice(2, 30, Some(60)),
            slice(5, 90, None),
        ]
    );
}

/// The last port with no count takes the rest; another port with no count
/// leaves its output with no ports.
#[test]
fn only_the_last_port_may_take_the_rest() {
    let mut project = Project::load(&[
        Output::named("main", MAIN_PORTS),
        Output::named("broken", &[(0, None), (1, Some(4))]),
    ]);
    let tables = project.reachable().outputs;
    assert_eq!(
        table(&tables, "main").ports.last(),
        Some(&slice(5, 90, None))
    );
    assert_eq!(
        table(&tables, "broken").ports,
        Vec::new(),
        "an output whose wires can't be cut is offered, with no ports"
    );
}

/// An output that reads another bus channel is not offered.
#[test]
fn an_output_reading_another_channel_is_not_offered() {
    let mut project = Project::load(&[
        Output::named("main", MAIN_PORTS),
        Output::named("elsewhere", MAIN_PORTS).reading("bus:other.out"),
    ]);
    let reachable = project.reachable();
    assert_eq!(names(&reachable.outputs), vec!["main"]);
    assert_eq!(
        reachable.channel.map(|channel| channel.0),
        Some(String::from("control.out")),
        "the answer names the channel the fixture publishes"
    );
}

/// An output with no name is not offered.
#[test]
fn an_output_with_no_name_is_not_offered() {
    let mut project = Project::load(&[
        Output::named("main", MAIN_PORTS),
        Output::unnamed(MAIN_PORTS),
    ]);
    assert_eq!(names(&project.reachable().outputs), vec!["main"]);
    // Ticking registers the unnamed output's (absent) name; it stays out.
    project.tick(2);
    assert_eq!(names(&project.reachable().outputs), vec!["main"]);
}

/// Editing a port's count moves the key, live, without a reload.
#[test]
fn editing_a_ports_count_moves_the_key_live() {
    let mut project = Project::load(&[Output::named("main", MAIN_PORTS)]);
    project.tick(3);
    let (before, key_before) = project.reachable_with_key();
    let (_, key_steady) = project.reachable_with_key();
    assert_eq!(
        key_before, key_steady,
        "nothing moves the key while nothing changes"
    );

    project.edit("/out_main.json", "ports[0].count.some", LpValue::U32(16));
    let (after, key_after) = project.reachable_with_key();
    assert_ne!(key_after, key_before, "a port-count edit moves the key");
    assert_eq!(
        table(&before.outputs, "main").ports[0],
        slice(0, 0, Some(30))
    );
    assert_eq!(
        &table(&after.outputs, "main").ports[..3],
        &[
            slice(0, 0, Some(48)),
            slice(1, 48, Some(0)),
            slice(2, 48, Some(60)),
        ],
        "the edit is in the tables, and every later port moved with it"
    );
    project.tick(1);
    let (_, key_ticked) = project.reachable_with_key();
    assert_eq!(
        key_ticked, key_after,
        "ticking after the edit moves nothing more"
    );
}

/// Renaming an output moves the key.
#[test]
fn renaming_an_output_moves_the_key() {
    let mut project = Project::load(&[Output::named("main", MAIN_PORTS)]);
    project.tick(3);
    let (_, key_before) = project.reachable_with_key();

    project.edit(
        "/out_main.json",
        "name.some",
        LpValue::String(String::from("lounge")),
    );
    project.tick(1);
    let (after, key_after) = project.reachable_with_key();
    assert_ne!(key_after, key_before);
    assert_eq!(names(&after.outputs), vec!["lounge"]);
}

/// Asking costs a project whose fixture holds an older mapping nothing:
/// project format 12 refuses that fixture at load, by name, and the
/// question is never asked.
#[test]
fn a_fixture_refused_for_an_older_mapping_never_asks() {
    let mut project = Project::load_map2d(
        &[
            Output::named("main", MAIN_PORTS),
            Output::named("other", &[(0, Some(4))]),
        ],
        OLDER_STRIP,
    );
    let status = project
        .rt
        .tree()
        .get(project.fixture)
        .map(|entry| entry.status.value().clone());
    assert!(
        matches!(
            &status,
            Some(lpc_model::NodeRuntimeStatus::Error(message))
                if message.contains(crate::nodes::fixture::fixture_node::OLDER_MAPPING_REFUSAL)
        ),
        "{status:?}"
    );
    let asked = port_table_questions::asked();
    project.tick_allowing_failure(5);
    project.edit("/out_main.json", "ports[0].count.some", LpValue::U32(16));
    project.tick_allowing_failure(3);
    assert_eq!(
        port_table_questions::asked(),
        asked,
        "a refused fixture never asks for port tables"
    );
}

/// A fixture inside a submodule that exports its channel to the root
/// reaches the root's output through the export (peach's shape: each
/// fixture writes `control.out` in its own module, which exports it).
#[test]
fn a_fixture_in_a_submodule_reaches_an_output_through_its_export() {
    let mut project = Project::load_in_submodule(&[Output::named("main", MAIN_PORTS)]);
    let reachable = project.reachable();
    assert_eq!(names(&reachable.outputs), vec!["main"]);
    assert_eq!(
        reachable.channel.map(|channel| channel.0),
        Some(String::from("control.out"))
    );
}

// --- helpers -----------------------------------------------------------------

/// Ports 0 (10 lamps), 1 (none), 2 (20 lamps) and 5 (the rest).
const MAIN_PORTS: &[(u32, Option<u32>)] = &[(0, Some(10)), (1, Some(0)), (2, Some(20)), (5, None)];

/// One output the project holds.
struct Output {
    name: Option<&'static str>,
    ports: &'static [(u32, Option<u32>)],
    channel: &'static str,
}

impl Output {
    fn named(name: &'static str, ports: &'static [(u32, Option<u32>)]) -> Self {
        Self {
            name: Some(name),
            ports,
            channel: "bus:control.out",
        }
    }

    fn unnamed(ports: &'static [(u32, Option<u32>)]) -> Self {
        Self {
            name: None,
            ports,
            channel: "bus:control.out",
        }
    }

    fn reading(mut self, channel: &'static str) -> Self {
        self.channel = channel;
        self
    }

    /// Its node (and file) name: `out_<name>`, or `out_<k>` unnamed.
    fn node(&self, index: usize) -> String {
        match self.name {
            Some(name) => format!("out_{name}"),
            None => format!("out_{index}"),
        }
    }

    fn json(&self) -> String {
        let name = self
            .name
            .map(|name| format!("\"name\": \"{name}\",\n"))
            .unwrap_or_default();
        let ports: Vec<String> = self
            .ports
            .iter()
            .map(|(key, count)| {
                let count = count
                    .map(|count| format!(", \"count\": {count}"))
                    .unwrap_or_default();
                format!("\"{key}\": {{ \"endpoint\": \"ws281x:local:D{key}\"{count} }}")
            })
            .collect();
        format!(
            "{{\n\"kind\": \"Output\",\n{name}\"ports\": {{ {} }},\n\"bindings\": {{ \"input\": {{ \"source\": \"{}\" }} }}\n}}",
            ports.join(", "),
            self.channel
        )
    }
}

/// A loaded project: one diagnostic fixture publishing on
/// `bus:control.out`, and the outputs asked for.
struct Project {
    fs: LpFsMemory,
    rt: LoadedProjectRuntime,
    fixture: NodeId,
}

/// The fixture's strand: four lamps, format 6, no start (the port tables
/// do not depend on where it lands).
const STRIP: &str = r#"{
  "format": 6,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, 4.0, 1.0],
  "parts": [
    { "id": "strand", "kind": { "grid": { "origin": [0.5, 0.5], "cols": 4, "rows": 1, "pitch": 1 } } }
  ]
}"#;

/// The same strand as map2d format 1, which a format-12 project refuses.
const OLDER_STRIP: &str = r#"{
  "format": 1,
  "sample_diameter": 1.0,
  "canvas": [0.0, 0.0, 4.0, 1.0],
  "objects": [
    { "name": "strand", "shape": { "grid": { "origin": [0.5, 0.5], "cols": 4, "rows": 1, "pitch": 1 } } }
  ]
}"#;

impl Project {
    fn load(outputs: &[Output]) -> Self {
        Self::load_map2d(outputs, STRIP)
    }

    /// [`Self::load`] with the fixture inside a submodule `strip_mod`,
    /// which exports `control.out` to the root, where the outputs are.
    fn load_in_submodule(outputs: &[Output]) -> Self {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 12\n}\n")
            .expect("manifest");
        let mut nodes = vec![String::from(
            "\"strip_mod\": { \"ref\": \"./strip_mod/module.json\" }",
        )];
        for (index, output) in outputs.iter().enumerate() {
            let node = output.node(index);
            nodes.push(format!("\"{node}\": {{ \"ref\": \"./{node}.json\" }}"));
            fs.write_file(format!("/{node}.json").as_path(), output.json().as_bytes())
                .expect("output def");
        }
        fs.write_file(
            "/module.json".as_path(),
            format!(
                "{{ \"kind\": \"Module\", \"nodes\": {{ {} }} }}",
                nodes.join(", ")
            )
            .as_bytes(),
        )
        .expect("module");
        fs.write_file(
            "/strip_mod/module.json".as_path(),
            br#"{ "kind": "Module", "nodes": { "strip": { "ref": "./strip.json" } },
  "exports": { "control.out": "bus:control.out" } }"#,
        )
        .expect("submodule");
        write_strip(&fs, "/strip_mod", STRIP);
        let services = EngineServices::new(TreePath::parse("/ports.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        let strip_mod = rt
            .tree()
            .lookup_sibling(
                rt.tree().root(),
                NodeName::parse("strip_mod").expect("name"),
            )
            .expect("submodule node");
        let fixture = rt
            .tree()
            .lookup_sibling(strip_mod, NodeName::parse("strip").expect("name"))
            .expect("fixture node");
        Self { fs, rt, fixture }
    }

    fn load_map2d(outputs: &[Output], map2d: &str) -> Self {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 12\n}\n")
            .expect("manifest");
        let mut nodes = vec![String::from("\"strip\": { \"ref\": \"./strip.json\" }")];
        for (index, output) in outputs.iter().enumerate() {
            let node = output.node(index);
            nodes.push(format!("\"{node}\": {{ \"ref\": \"./{node}.json\" }}"));
            fs.write_file(format!("/{node}.json").as_path(), output.json().as_bytes())
                .expect("output def");
        }
        fs.write_file(
            "/module.json".as_path(),
            format!(
                "{{ \"kind\": \"Module\", \"nodes\": {{ {} }} }}",
                nodes.join(", ")
            )
            .as_bytes(),
        )
        .expect("module");
        write_strip(&fs, "", map2d);
        let services = EngineServices::new(TreePath::parse("/ports.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        let fixture = rt
            .tree()
            .lookup_sibling(rt.tree().root(), NodeName::parse("strip").expect("name"))
            .expect("fixture node");
        Self { fs, rt, fixture }
    }

    fn reachable(&mut self) -> ReachableOutputs {
        self.reachable_with_key().0
    }

    fn reachable_with_key(&mut self) -> (ReachableOutputs, PortTablesKey) {
        let fixture = self.fixture;
        let (engine, registry) = self.rt.split_mut();
        control_patch_outputs_with_engine_host(engine, registry, fixture)
    }

    fn tick(&mut self, frames: u32) {
        for _ in 0..frames {
            self.rt.tick(16).expect("tick");
        }
    }

    /// Tick, a refused fixture's frame failure allowed.
    fn tick_allowing_failure(&mut self, frames: u32) {
        for _ in 0..frames {
            let _ = self.rt.tick(16);
        }
    }

    /// Edit one slot of a file the way an authoring edit does (Studio's
    /// slot edit: a registry mutation, then an apply), with no reload.
    fn edit(&mut self, file: &str, slot: &str, value: LpValue) {
        let shapes = self.rt.engine().slot_shapes().clone();
        let (engine, registry) = self.rt.split_mut();
        let result = registry
            .mutate(
                &self.fs,
                MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file(file),
                    edit: SlotEdit::assign_value(SlotPath::parse(slot).expect("slot"), value),
                },
                current_revision(),
                &ParseCtx { shapes: &shapes },
            )
            .expect("edit");
        engine
            .apply_project_changes(&self.fs, registry, &result.changes)
            .expect("apply the edit");
    }

    fn output_def(&self, name: &str) -> lpc_model::nodes::output::OutputDef {
        let file = format!("/out_{name}.json");
        let text = self.fs.read_file(file.as_path()).expect("output file");
        let def = lpc_model::NodeDef::read_json(
            self.rt.engine().slot_shapes(),
            core::str::from_utf8(&text).expect("utf8"),
        )
        .expect("output def");
        let lpc_model::NodeDef::Output(def) = def else {
            panic!("an output def");
        };
        def
    }
}

/// The diagnostic fixture `strip` (and its map2d) in `dir`, publishing on
/// `bus:control.out`.
fn write_strip(fs: &LpFsMemory, dir: &str, map2d: &str) {
    fs.write_file(
        format!("{dir}/strip.json").as_path(),
        br#"{
  "kind": "Fixture",
  "render_size": { "width": 4, "height": 1 },
  "bindings": { "output": { "target": "bus:control.out" } },
  "sampling": "direct",
  "diagnostic_mode": "led_index",
  "mapping": { "kind": "Map2d", "source": "strip.map2d.json" },
  "color_order": "rgb",
  "gamma_correction": false
}"#,
    )
    .expect("fixture def");
    fs.write_file(
        format!("{dir}/strip.map2d.json").as_path(),
        map2d.as_bytes(),
    )
    .expect("map2d");
}

fn table<'a>(tables: &'a [OutputPortTable], name: &str) -> &'a OutputPortTable {
    tables
        .iter()
        .find(|table| table.name == name)
        .unwrap_or_else(|| panic!("output {name:?} is offered: {tables:?}"))
}

fn names(tables: &[OutputPortTable]) -> Vec<&str> {
    tables.iter().map(|table| table.name.as_str()).collect()
}

fn slice(key: u32, start_samples: u32, capacity_samples: Option<u32>) -> OutputPortSlice {
    OutputPortSlice {
        key,
        start_samples,
        capacity_samples,
    }
}
