//! Tree deltas ride a project read only when what they carry changed
//! (lean-wire follow-ups F3, `tree_entry_stamps`).
//!
//! Every engine call takes a node's runtime out of its tree entry and puts
//! it back, and until F3 that put-back restamped the entry, so every alive
//! node's `entry_changed` delta rode every lens read. These tests pin the
//! three halves of the replacement: a steady read carries none, a real
//! status change carries exactly its own, and a status a render probe
//! changes mid-read — after that read already sent its tree — reaches the
//! next read even when no tick moved the revision in between.

extern crate std;

use alloc::sync::Arc;
use alloc::vec::Vec;

use lpc_model::{
    ArtifactLocation, LpValue, NodeDefLocation, NodeId, ProductRef, Revision, TreePath,
};
use lpc_registry::ParseCtx;
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, NodeRuntimeStatus, ProjectProbeRequest, ProjectReadEvent,
    ProjectReadNodeEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest, ReadLevel,
    RenderProductProbeRequest, WireTextureFormat, WireTreeDelta,
};
use lpfs::{FsEvent, FsEventKind, LpFs, LpFsMemory, LpPath, LpPathBuf};

use super::test_support::{EngineTestBuilder, collect_read_events, output, produced_slot};
use super::{Engine, EngineServices, LoadedProjectRuntime, ProjectLoader};
use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
use crate::nodes::shader_output_path;
use crate::products::visual::VisualProduct;

/// Ticks put every alive node back into its entry, over and over; none of
/// that is a change a client can see.
#[test]
fn a_steady_read_sends_no_entry_changed() {
    let mut h = failing_producer_project();
    h.set_failing("shader", false);
    h.tick(16).expect("tick");

    let first = collect_read_events(&mut h.engine, &h.registry, nodes_read(None));
    let mut since = served_revision(&first);
    for _ in 0..3 {
        h.tick(16).expect("tick");
        let events = collect_read_events(&mut h.engine, &h.registry, nodes_read(Some(since)));
        assert_eq!(
            entry_changes(&events),
            Vec::new(),
            "steady read at {since:?}"
        );
        since = served_revision(&events);
    }
}

#[test]
fn a_real_status_change_sends_exactly_that_entry() {
    let mut h = failing_producer_project();
    h.set_failing("shader", false);
    h.tick(16).expect("tick");
    let first = collect_read_events(&mut h.engine, &h.registry, nodes_read(None));
    let since = served_revision(&first);

    h.set_failing("shader", true);
    h.tick(16)
        .expect("the tolerant consumer keeps the tick alive");
    let events = collect_read_events(&mut h.engine, &h.registry, nodes_read(Some(since)));
    let changes = entry_changes(&events);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].0, h.node("shader"));
    assert!(
        matches!(&changes[0].1, NodeRuntimeStatus::Fault(message) if message.contains("intentional")),
        "{changes:?}"
    );

    // Delivered once: the next read is steady again.
    let since = served_revision(&events);
    h.tick(16).expect("tick");
    let events = collect_read_events(&mut h.engine, &h.registry, nodes_read(Some(since)));
    assert_eq!(entry_changes(&events), Vec::new());
}

/// The `declare_space` e2e's case in miniature. A working shader has been
/// `ok` for a while; an edit breaks its source, and nothing ticks, so the
/// compile runs in a render probe — after that read already sent its tree.
/// The next read, at the SAME revision, must still deliver the error, and
/// once delivered it stops riding.
#[test]
fn a_compile_error_raised_mid_read_reaches_the_next_read() {
    let (mut rt, fs) = basic_project();
    let shader = node_for_def_path(&rt, "/shader.json");
    rt.tick(40).expect("tick 1: compile window request");
    rt.tick(40).expect("tick 2: compiled and rendered");
    let product = shader_visual_product(&mut rt, shader);
    let (mut engine, mut registry) = rt.into_parts();
    assert_eq!(entry_status(&engine, shader), &NodeRuntimeStatus::Ok);

    // Two reads settle the client, a tick apart: the shader's stamp is now
    // older than the revision the next reads serve.
    let events = collect_read_events(&mut engine, &registry, nodes_read(None));
    let since = served_revision(&events);
    engine.tick(&registry, 40).expect("tick 3");
    let events = collect_read_events(&mut engine, &registry, nodes_read(Some(since)));
    let served = served_revision(&events);
    assert_eq!(entry_changes(&events), Vec::new(), "steady");

    // The edit: status stays `ok` until something compiles the new source.
    fs.write_file(LpPath::new("/shader.glsl"), BROKEN_SHADER.as_bytes())
        .expect("write shader");
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/shader.glsl"),
            kind: FsEventKind::Modify,
        }],
        served,
        &ParseCtx { shapes: &shapes },
    );
    engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply the edit");
    assert_eq!(entry_status(&engine, shader), &NodeRuntimeStatus::Ok);

    // Probe reads at the served revision until one compiles the edit; that
    // read's tree went out before the compile, so it cannot carry the error.
    let mut compiled = false;
    for _ in 0..3 {
        let events = collect_read_events(
            &mut engine,
            &registry,
            lens_read(Some(served), probe(product)),
        );
        assert_eq!(
            served_revision(&events),
            served,
            "no tick moved the revision"
        );
        assert!(
            !status_deltas(&events)
                .iter()
                .any(|(_, status)| is_error(status)),
            "an error can only arrive after the read that raised it: {:?}",
            status_deltas(&events)
        );
        if is_error(entry_status(&engine, shader)) {
            compiled = true;
            break;
        }
    }
    assert!(compiled, "a render probe compiled the broken source");

    // The next read, still at the served revision: the error arrives.
    let events = collect_read_events(&mut engine, &registry, nodes_read(Some(served)));
    let changes = entry_changes(&events);
    assert!(
        changes
            .iter()
            .any(|(id, status)| *id == shader && is_error(status)),
        "the mid-read compile error reaches the next read: {changes:?}"
    );

    // Ticks move the revision past the stamp; then the shader is steady.
    let mut since = served;
    let mut steady = false;
    for _ in 0..3 {
        engine.tick(&registry, 40).expect("tick");
        let events = collect_read_events(&mut engine, &registry, nodes_read(Some(since)));
        steady = !entry_changes(&events).iter().any(|(id, _)| *id == shader);
        since = served_revision(&events);
    }
    assert!(steady, "the error stops riding once delivered");
}

const BROKEN_SHADER: &str = "vec4 render(vec2 pos) {\n    return not_a_thing;\n}\n";

/// A shader that fails its produce while its switch is on (it starts on),
/// feeding a consumer that tolerates the failure.
fn failing_producer_project() -> super::test_support::EngineTestHarness {
    EngineTestBuilder::new()
        .failing_producer(
            "shader",
            output("outputs[0]", 0.75),
            "intentional produce failure",
        )
        .tolerant_fixture("fixture", NodeRuntimeStatus::Ok)
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build()
}

fn nodes_read(since: Option<Revision>) -> ProjectReadRequest {
    lens_read(since, Vec::new())
}

fn lens_read(since: Option<Revision>, probes: Vec<ProjectProbeRequest>) -> ProjectReadRequest {
    ProjectReadRequest {
        since,
        queries: Vec::from([ProjectReadQuery::Nodes(NodeReadQuery {
            level: ReadLevel::Detail,
            nodes: NodeReadSelection::All,
            include_slots: false,
        })]),
        probes,
    }
}

fn probe(product: VisualProduct) -> Vec<ProjectProbeRequest> {
    Vec::from([ProjectProbeRequest::RenderProduct(
        RenderProductProbeRequest {
            product,
            width: 8,
            height: 8,
            format: WireTextureFormat::Srgb8,
            space: None,
            policy: None,
        },
    )])
}

fn served_revision(events: &[ProjectReadEvent]) -> Revision {
    events
        .iter()
        .find_map(|event| match event {
            ProjectReadEvent::Begin { revision } => Some(*revision),
            _ => None,
        })
        .expect("a read begins")
}

/// Every `entry_changed` delta in a read, as `(node, status)`.
fn entry_changes(events: &[ProjectReadEvent]) -> Vec<(NodeId, NodeRuntimeStatus)> {
    tree_deltas(events)
        .filter_map(|delta| match delta {
            WireTreeDelta::EntryChanged { id, status, .. } => Some((*id, status.clone())),
            _ => None,
        })
        .collect()
}

/// Every status a read's tree deltas carry (`created` and `entry_changed`).
fn status_deltas(events: &[ProjectReadEvent]) -> Vec<(NodeId, NodeRuntimeStatus)> {
    tree_deltas(events)
        .filter_map(|delta| match delta {
            WireTreeDelta::Created { id, status, .. }
            | WireTreeDelta::EntryChanged { id, status, .. } => Some((*id, status.clone())),
            WireTreeDelta::ChildrenChanged { .. } => None,
        })
        .collect()
}

fn tree_deltas(events: &[ProjectReadEvent]) -> impl Iterator<Item = &WireTreeDelta> {
    events
        .iter()
        .filter_map(|event| match event {
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas { deltas }),
                ..
            } => Some(deltas),
            _ => None,
        })
        .flatten()
}

fn entry_status(engine: &Engine, node: NodeId) -> &NodeRuntimeStatus {
    engine.tree().get(node).expect("entry").status.value()
}

fn is_error(status: &NodeRuntimeStatus) -> bool {
    matches!(status, NodeRuntimeStatus::Error(_))
}

/// `projects/test/basic` in memory (so a test can edit it) with the real
/// CPU graphics backend, not ticked yet.
fn basic_project() -> (LoadedProjectRuntime, LpFsMemory) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../projects/test/basic");
    let fs = LpFsMemory::new();
    for file in std::fs::read_dir(&dir).expect("projects/test/basic") {
        let path = file.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("name");
        let bytes = std::fs::read(&path).expect("project file");
        fs.write_file(LpPath::new(&alloc::format!("/{name}")), &bytes)
            .expect("write project file");
    }
    let services = EngineServices::new(TreePath::parse("/basic.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load projects/test/basic");
    rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lp_shader::ShaderFrontend::LpsGlsl,
    ))));
    (rt, fs)
}

fn node_for_def_path(rt: &LoadedProjectRuntime, path: &str) -> NodeId {
    let location = NodeDefLocation::artifact_root(ArtifactLocation::file(path));
    rt.project_runtime_index()
        .runtime_nodes_for_def(&location)
        .first()
        .copied()
        .unwrap_or_else(|| panic!("node for def path {path}"))
}

fn shader_visual_product(rt: &mut LoadedProjectRuntime, shader: NodeId) -> VisualProduct {
    let (production, _) = rt
        .resolve_with_engine_host(
            QueryKey::ProducedSlot {
                node: shader,
                slot: shader_output_path(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve shader output slot");
    let LpValue::Product(ProductRef::Visual(product)) = production
        .value_leaf()
        .expect("visual product value")
        .value()
    else {
        panic!("shader output slot should be a visual product");
    };
    *product
}
