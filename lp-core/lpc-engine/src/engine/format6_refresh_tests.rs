//! A format-6 fixture's live edits (mapping rebuild M2 P4): the map2d file
//! is re-read through the same dispatch the loader uses, keep-last-good on
//! any error, and one file holds the layout and the patching (G1, Q14).
//!
//! Each project is loaded from memory as a device loads one: one fixture in
//! diagnostic `led_index` (a distinct color per lamp, no graphics backend)
//! on `bus:control.out`, and one output named `main` with one port of 12
//! lamps reading it. An edit is a file write followed by the registry's
//! refresh and the engine's apply, as the in-place editor's save is.
//!
//! **What moved, read from outside:** where the fixture's lamps sit on the
//! wire, from the output's published samples; and whether the layout was
//! rendered again, from the fixture's display-layout revision, which is
//! keyed on the fixture's mapping version (every cache that comes from the
//! lamps' positions is).

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlProduct, NodeName, NodeRuntimeStatus, Revision,
    TreePath,
};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpc_wire::{
    ControlProductGeometry, ControlProductProbeRequest, ControlProductProbeResult,
    GeometryDisplayLayout, ProjectProbeRequest, ProjectProbeResult, ProjectReadRequest,
    RevisionGateRead, RevisionGateResult, WireChannelSampleFormat,
};
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpFsMemory, LpPathBuf};

use super::test_support::read_probe_results;
use super::{Engine, EngineServices, ProjectLoader};
use crate::node::NodeEntryState;

/// An edit that changes only the patching re-patches without rendering the
/// layout again: the lamps move on the wire, and the layout's revision
/// stands.
#[test]
fn an_edit_to_the_patching_alone_re_patches_without_rendering_the_layout() {
    let mut rig = Rig::load(&strip_6(3, 0));
    assert_eq!(rig.wire(), lit_at(&[0, 1, 2]));
    let layout = rig.layout_revision();

    rig.edit(&strip_6(3, 4));
    assert_eq!(rig.wire(), lit_at(&[4, 5, 6]), "the start moved the lamps");
    assert_eq!(
        rig.layout_revision(),
        layout,
        "the layout was not rendered again"
    );
    assert_eq!(rig.status(), None);
}

/// An edit to the layout renders it again.
#[test]
fn an_edit_to_the_layout_renders_it_again() {
    let mut rig = Rig::load(&strip_6(3, 0));
    let layout = rig.layout_revision();

    rig.edit(&strip_6(4, 0));
    assert_eq!(rig.wire(), lit_at(&[0, 1, 2, 3]));
    assert_ne!(rig.layout_revision(), layout, "four lamps, drawn anew");
}

/// A document edited from format 5 to format 6, live, switches path
/// cleanly: from auto-flowed to placed by its own patching, and back.
#[test]
fn a_document_edited_across_formats_switches_path_cleanly() {
    let mut rig = Rig::load(&strip_5(3));
    assert_eq!(rig.wire(), lit_at(&[0, 1, 2]), "format 5: auto-flowed");
    assert!(!rig.manual());

    rig.edit(&strip_6(3, 5));
    assert_eq!(rig.wire(), lit_at(&[5, 6, 7]), "format 6: its own start");
    assert!(rig.manual());

    rig.edit(&strip_5(3));
    assert_eq!(rig.wire(), lit_at(&[0, 1, 2]), "format 5 again");
    assert!(!rig.manual());
    assert_eq!(rig.status(), None);
}

/// A format-6 edit the reader refuses keeps the last good mapping
/// lighting, and says why in the reader's words.
#[test]
fn a_refused_edit_keeps_the_last_good_mapping_and_says_why() {
    let mut rig = Rig::load(&strip_6(3, 2));
    let curve = strip_6(3, 2).replace("L 20 5", "C 5 0 15 0 20 5");
    rig.edit(&curve);
    assert_eq!(rig.wire(), lit_at(&[2, 3, 4]), "the last good mapping");
    let Some(NodeRuntimeStatus::Error(message)) = rig.status() else {
        panic!("the refusal is the fixture's status");
    };
    assert!(message.contains("cubic curve"), "{message}");

    rig.edit(&strip_6(3, 2));
    assert_eq!(rig.status(), None, "a good edit clears it");
}

/// A fixture that names a patch document can't take a format-6 map2d by a
/// live edit either: refused by name, the last good mapping kept.
#[test]
fn a_live_edit_to_format_6_beside_a_patch_document_is_refused_by_name() {
    let mut rig = Rig::load_patched(&strip_5(3));
    rig.edit(&strip_6(3, 4));
    let Some(NodeRuntimeStatus::Error(message)) = rig.status() else {
        panic!("the refusal is the fixture's status");
    };
    assert!(
        message.contains("holds its own patching: remove its patch document"),
        "{message}"
    );
    assert!(!rig.manual(), "still the format-5 fixture it was");
}

// --- the rig ------------------------------------------------------------------

/// The output's lamps, lamp `k` lit where `lit` holds `k`.
fn lit_at(lit: &[usize]) -> Vec<bool> {
    (0..PORT_LAMPS).map(|k| lit.contains(&k)).collect()
}

/// Lamps on the output's one port.
const PORT_LAMPS: usize = 12;

/// A format-6 document: one straight path of `lamps` lamps, starting at
/// lamp `start` of port 0 of `main`.
fn strip_6(lamps: u32, start: u32) -> String {
    format!(
        r#"{{
  "format": 6,
  "canvas": [0.0, 0.0, 20.0, 10.0],
  "next": {{ "path": 2 }},
  "start": {{ "output": {{ "set": "main" }}, "port": {{ "set": 0 }},
             "channel": {{ "set": {{ "lamps": {start} }} }} }},
  "parts": [ {{ "id": "path1", "kind": {{ "path": {{ "d": "M 0 5 L 20 5", "count": {lamps} }} }} }} ]
}}
"#
    )
}

/// Today's document of the same strip, unpatched.
fn strip_5(lamps: u32) -> String {
    format!(
        r#"{{
  "format": 5,
  "canvas": [0.0, 0.0, 20.0, 10.0],
  "objects": [ {{ "name": "path", "shape": {{ "path": {{ "points": [[0, 5], [20, 5]], "count": {lamps} }} }} }} ]
}}
"#
    )
}

struct Rig {
    fs: LpFsMemory,
    engine: Engine,
    registry: ProjectRegistry,
    /// The revision the next edit is stamped with: each edit is newer than
    /// the last, as a save's is.
    edits: i64,
}

impl Rig {
    fn load(map2d: &str) -> Self {
        Self::load_with(map2d, false)
    }

    /// [`Self::load`], with a patch document beside the map2d.
    fn load_patched(map2d: &str) -> Self {
        Self::load_with(map2d, true)
    }

    fn load_with(map2d: &str, patched: bool) -> Self {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", "{\n  \"format\": 11\n}\n");
        write(
            &fs,
            "/module.json",
            r#"{ "kind": "Module", "nodes": {
  "fixture": { "ref": "./fixture.json" },
  "output": { "ref": "./output.json" }
} }"#,
        );
        let patch = if patched {
            write(
                &fs,
                "/fixture.patch.json",
                r#"{ "format": 3, "entries": [] }"#,
            );
            ",\n  \"patch\": { \"kind\": \"File\", \"source\": \"fixture.patch.json\" }"
        } else {
            ""
        };
        write(
            &fs,
            "/fixture.json",
            &format!(
                r#"{{
  "kind": "Fixture",
  "render_size": {{ "width": 20, "height": 10 }},
  "bindings": {{ "output": {{ "target": "bus:control.out" }} }},
  "sampling": "direct",
  "diagnostic_mode": "led_index",
  "mapping": {{ "kind": "Map2d", "source": "fixture.map2d.json" }}{patch},
  "color_order": "rgb",
  "brightness": 1.0,
  "gamma_correction": false
}}"#
            ),
        );
        write(
            &fs,
            "/output.json",
            &format!(
                r#"{{
  "kind": "Output",
  "name": "main",
  "ports": {{ "0": {{ "endpoint": "ws281x:local:D10", "count": {PORT_LAMPS} }} }},
  "bindings": {{ "input": {{ "source": "bus:control.out" }} }}
}}"#
            ),
        );
        write(&fs, "/fixture.map2d.json", map2d);
        let services = EngineServices::new(TreePath::parse("/format6.show").expect("path"));
        let (engine, registry) = ProjectLoader::load_from_root(&fs, services)
            .expect("load")
            .into_parts();
        let mut rig = Self {
            fs,
            engine,
            registry,
            edits: 1,
        };
        rig.tick(3);
        rig
    }

    /// Rewrite the map2d file and apply it as the in-place editor's save
    /// does, then let two frames settle (re-patch, then re-plan).
    fn edit(&mut self, map2d: &str) {
        self.fs
            .write_file_mut("/fixture.map2d.json".as_path(), map2d.as_bytes())
            .expect("write the map2d");
        let shapes = self.engine.slot_shapes().clone();
        let events = vec![FsEvent {
            path: LpPathBuf::from("/fixture.map2d.json"),
            kind: FsEventKind::Modify,
        }];
        self.edits += 1;
        let changes = self.registry.refresh_artifacts(
            &self.fs,
            &events,
            Revision::new(self.edits),
            &ParseCtx { shapes: &shapes },
        );
        self.engine
            .apply_project_changes(&self.fs, &mut self.registry, &changes)
            .expect("apply the edit");
        self.tick(2);
    }

    fn tick(&mut self, frames: usize) {
        for _ in 0..frames {
            self.engine.tick(&self.registry, 16).expect("tick");
        }
    }

    fn fixture(&self) -> lpc_model::NodeId {
        let tree = self.engine.tree();
        tree.lookup_sibling(tree.root(), NodeName::parse("fixture").expect("name"))
            .expect("fixture node")
    }

    /// Which of the output's lamps are lit.
    fn wire(&self) -> Vec<bool> {
        let tree = self.engine.tree();
        let output = tree
            .lookup_sibling(tree.root(), NodeName::parse("output").expect("name"))
            .expect("output node");
        let buffer = self
            .engine
            .runtime_output_sink_buffer_id(output)
            .expect("sink buffer");
        let samples: Vec<u16> = self
            .engine
            .runtime_buffers()
            .get(buffer)
            .expect("sink buffer")
            .value()
            .samples16()
            .expect("u16 samples")
            .to_vec();
        let mut lit: Vec<bool> = samples
            .chunks_exact(3)
            .map(|lamp| lamp.iter().any(|sample| *sample != 0))
            .collect();
        lit.resize(PORT_LAMPS, false);
        lit
    }

    fn runtime<T>(&self, read: impl FnOnce(&dyn crate::node::NodeRuntime) -> T) -> T {
        let entry = self.engine.tree().get(self.fixture()).expect("entry");
        let NodeEntryState::Alive(runtime) = entry.state.value() else {
            panic!("the fixture is alive");
        };
        read(runtime.as_ref())
    }

    fn status(&self) -> Option<NodeRuntimeStatus> {
        self.runtime(|runtime| runtime.runtime_status())
    }

    fn manual(&self) -> bool {
        self.runtime(|runtime| runtime.control_patch_manual())
    }

    /// The fixture's display-layout revision: keyed on its mapping version,
    /// so it moves exactly when the layout is rendered again.
    fn layout_revision(&mut self) -> Revision {
        let product = ControlProduct::new(self.fixture(), 0, ControlExtent::new(1, 64));
        let results = read_probe_results(
            &mut self.engine,
            &self.registry,
            ProjectReadRequest {
                since: None,
                queries: vec![],
                probes: vec![ProjectProbeRequest::ControlProduct(
                    ControlProductProbeRequest {
                        product,
                        sample_format: WireChannelSampleFormat::U16,
                        geometry: RevisionGateRead::Always,
                    },
                )],
            },
        );
        let Some(ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            geometry:
                RevisionGateResult::Changed(ControlProductGeometry {
                    display_layout:
                        GeometryDisplayLayout::Layout(ControlDisplayLayout::Layout2d(layout)),
                    ..
                }),
            ..
        })) = results.first()
        else {
            panic!("a control preview with a layout, got {results:?}");
        };
        layout.revision
    }
}

fn write(fs: &LpFsMemory, path: &str, text: &str) {
    fs.write_file(path.as_path(), text.as_bytes())
        .unwrap_or_else(|error| panic!("write {path}: {error:?}"));
}
