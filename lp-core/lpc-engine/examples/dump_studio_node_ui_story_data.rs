//! Dump real project-read data for the Studio node UI story spike.
//!
//! This is intentionally a small development tool instead of story-only mock
//! data. The stories can then show the exact shape and slot JSON they are using
//! as grounding material while the visual design is still exploratory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use lpc_engine::{
    ButtonService, EngineProjectReadSource, EngineServices, Graphics, ProjectLoader, RadioService,
};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::{
    NodeId, Revision, SlotShapeEntry, SlotShapeId, SlotShapeRegistrySnapshot, TreePath,
};
use lpc_shared::transport::ProjectReadEventSink;
use lpc_view::{ProjectReadApplier, ProjectView};
use lpc_wire::{
    ProjectReadEvent, ProjectReadNodeEvent, ProjectReadQueryEvent, ProjectReadRequest,
    WireSlotRootSnapshot, WireSlotRootsSnapshot,
};
use lpfs::{LpFsMemory, LpPath};
use serde::Serialize;

const STORY_NODES: &[StoryNodeSpec] = &[
    StoryNodeSpec {
        slug: "clock",
        title: "Clock",
        path: "/fyeah_sign.show/clock.clock",
    },
    StoryNodeSpec {
        slug: "fixture",
        title: "Fixture",
        path: "/fyeah_sign.show/fixture.fixture",
    },
    StoryNodeSpec {
        slug: "shader",
        title: "blast",
        path: "/fyeah_sign.show/playlist.playlist/blast.shader",
    },
    StoryNodeSpec {
        slug: "playlist",
        title: "Playlist",
        path: "/fyeah_sign.show/playlist.playlist",
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse()?;
    fs::create_dir_all(&args.out)?;

    let fs = load_project_files(&args.project)?;
    let mut services = EngineServices::new(TreePath::parse("/fyeah_sign.show")?);
    let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
    let hardware = Rc::new(HardwareSystem::with_virtual_drivers(registry));
    let button_service: Rc<dyn ButtonService> = hardware.clone();
    let radio_service: Rc<dyn RadioService> = hardware;
    services.set_button_service(Some(button_service));
    services.set_radio_service(Some(radio_service));

    let mut runtime = ProjectLoader::load_from_root(&fs, services)?;
    runtime.set_graphics(Some(Arc::new(Graphics::new())));
    for _ in 0..102 {
        runtime.tick(33)?;
    }

    // Stream a full project read and apply it progressively to a client-side
    // `ProjectView` (the same path the live clients use since M6/P5 deleted the
    // aggregate response). The raw `SlotRoot` events carry the wire snapshots we
    // dump verbatim; the applied view gives the shape registry and node lookups.
    let (mut engine, registry) = runtime.into_parts();
    let events = block_on(async {
        let mut sink = CollectingEventSink::default();
        EngineProjectReadSource::new(&mut engine, &registry)
            .stream_project_read_events(ProjectReadRequest::default_debug(None), &mut sink)
            .await
            .map_err(|error| format!("project read stream: {error:?}"))?;
        Ok::<_, String>(sink.events)
    })?;

    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    for event in &events {
        applier
            .apply(event.clone())
            .map_err(|error| format!("apply project read event: {error}"))?;
    }
    let revision = view.revision.as_i64();

    let roots: Vec<WireSlotRootSnapshot> = events
        .into_iter()
        .filter_map(|event| match event {
            ProjectReadEvent::Query {
                event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(root)),
                ..
            } => Some(root),
            _ => None,
        })
        .collect();
    let shape_registry = view.slots.registry.snapshot();

    for spec in STORY_NODES {
        let node_path = TreePath::parse(spec.path)
            .map_err(|error| format!("story node path {}: {error}", spec.path))?;
        let node_id = view
            .tree
            .lookup_path(&node_path)
            .ok_or_else(|| format!("project read did not include node {}", spec.path))?;
        let node_roots = roots_for_node(&roots, node_id);
        if node_roots.is_empty() {
            return Err(format!("node {} had no slot roots", spec.path).into());
        }

        let shape_json = StoryShapeJson {
            source: StorySource::new(&args.project, revision),
            node: StoryNodeRef::new(spec, node_id),
            root_shapes: root_shape_refs(&node_roots),
            registry: shape_registry_for_roots(&shape_registry, &node_roots),
        };
        write_json(
            &args.out.join(format!("{}.shape.json", spec.slug)),
            &shape_json,
        )?;

        let slot_json = StorySlotJson {
            source: StorySource::new(&args.project, revision),
            node: StoryNodeRef::new(spec, node_id),
            roots: WireSlotRootsSnapshot {
                roots: node_roots.into_iter().cloned().collect(),
            },
        };
        write_json(
            &args.out.join(format!("{}.slots.json", spec.slug)),
            &slot_json,
        )?;
    }

    Ok(())
}

fn load_project_files(root: &Path) -> Result<LpFsMemory, Box<dyn std::error::Error>> {
    let mut fs = LpFsMemory::new();
    for (path, bytes) in read_project_files(root)? {
        fs.write_file_mut(LpPath::new(&path), &bytes)
            .map_err(|error| format!("write project file {path}: {error}"))?;
    }
    Ok(fs)
}

fn read_project_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, std::io::Error> {
    let mut files = BTreeMap::new();
    read_project_files_recursive(root, root, &mut files)?;
    Ok(files)
}

fn read_project_files_recursive(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), std::io::Error> {
    let mut entries = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();

    for path in entries {
        if path.is_dir() {
            read_project_files_recursive(root, &path, files)?;
            continue;
        }

        let relative = path.strip_prefix(root).expect("project-relative path");
        let project_path = format!("/{}", relative.to_string_lossy());
        files.insert(project_path, fs::read(&path)?);
    }

    Ok(())
}

fn roots_for_node(roots: &[WireSlotRootSnapshot], node_id: NodeId) -> Vec<&WireSlotRootSnapshot> {
    let prefix = format!("node.{node_id}.");
    roots
        .iter()
        .filter(|root| root.name.starts_with(&prefix))
        .collect()
}

fn root_shape_refs<'a>(roots: &'a [&'a WireSlotRootSnapshot]) -> Vec<StoryRootShape<'a>> {
    roots
        .iter()
        .map(|root| StoryRootShape {
            name: root.name.as_str(),
            shape: root.shape,
        })
        .collect()
}

fn shape_registry_for_roots<'a>(
    registry: &'a SlotShapeRegistrySnapshot,
    roots: &[&WireSlotRootSnapshot],
) -> StoryShapeRegistry<'a> {
    let mut queue = roots.iter().map(|root| root.shape).collect::<Vec<_>>();
    let mut shapes = BTreeMap::new();
    let mut missing_refs = Vec::new();

    while let Some(id) = queue.pop() {
        if shapes.contains_key(&id) {
            continue;
        }

        let Some(entry) = registry.shapes.get(&id) else {
            if !missing_refs.contains(&id) {
                missing_refs.push(id);
            }
            continue;
        };

        queue.extend(entry.shape.referenced_shape_ids());
        shapes.insert(id, entry);
    }

    missing_refs.sort();
    StoryShapeRegistry {
        ids_revision: registry.ids_revision,
        shapes,
        missing_refs,
    }
}

/// A sink that records every emitted project-read event in order.
#[derive(Default)]
struct CollectingEventSink {
    events: Vec<ProjectReadEvent>,
}

impl ProjectReadEventSink for CollectingEventSink {
    type Error = std::convert::Infallible;

    async fn send_project_read_event(
        &mut self,
        event: ProjectReadEvent,
    ) -> Result<(), Self::Error> {
        self.events.push(event);
        Ok(())
    }
}

/// Minimal synchronous executor: the project-read stream never yields `Pending`
/// against an in-memory sink, so a busy-poll block_on is sufficient here.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn noop_waker() -> Waker {
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        unsafe fn noop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match std::future::Future::poll(std::pin::Pin::as_mut(&mut future), &mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}

struct Args {
    project: PathBuf,
    out: PathBuf,
}

impl Args {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut project = PathBuf::from("projects/test/fyeah-sign");
        let mut out = PathBuf::from("lp-app/lpa-studio-web/src/stories/data/node_ui");
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--project" => {
                    project = args.next().ok_or("--project requires a path")?.into();
                }
                "--out" => {
                    out = args.next().ok_or("--out requires a path")?.into();
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }

        Ok(Self { project, out })
    }
}

fn print_usage() {
    eprintln!(
        "usage: cargo run -p lpc-engine --example dump_studio_node_ui_story_data -- \\\n+  [--project projects/test/fyeah-sign] \\\n+  [--out lp-app/lpa-studio-web/src/stories/data/node_ui]"
    );
}

#[derive(Clone, Copy)]
struct StoryNodeSpec {
    slug: &'static str,
    title: &'static str,
    path: &'static str,
}

#[derive(Serialize)]
struct StorySource {
    project: String,
    read_revision: i64,
    request: &'static str,
}

impl StorySource {
    fn new(project: &Path, read_revision: i64) -> Self {
        Self {
            project: project.display().to_string(),
            read_revision,
            request: "ProjectReadRequest::default_debug(None) after 102 x 33ms ticks",
        }
    }
}

#[derive(Serialize)]
struct StoryNodeRef {
    id: NodeId,
    title: &'static str,
    path: &'static str,
}

impl StoryNodeRef {
    fn new(spec: &StoryNodeSpec, id: NodeId) -> Self {
        Self {
            id,
            title: spec.title,
            path: spec.path,
        }
    }
}

#[derive(Serialize)]
struct StoryRootShape<'a> {
    name: &'a str,
    shape: SlotShapeId,
}

#[derive(Serialize)]
struct StoryShapeRegistry<'a> {
    ids_revision: Revision,
    shapes: BTreeMap<SlotShapeId, &'a SlotShapeEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    missing_refs: Vec<SlotShapeId>,
}

#[derive(Serialize)]
struct StoryShapeJson<'a> {
    source: StorySource,
    node: StoryNodeRef,
    root_shapes: Vec<StoryRootShape<'a>>,
    registry: StoryShapeRegistry<'a>,
}

#[derive(Serialize)]
struct StorySlotJson {
    source: StorySource,
    node: StoryNodeRef,
    roots: WireSlotRootsSnapshot,
}
