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

use lpc_engine::{ButtonService, EngineServices, Graphics, ProjectLoader, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::{
    NodeId, Revision, SlotShapeEntry, SlotShapeId, SlotShapeRegistrySnapshot, TreePath,
};
use lpc_wire::{
    ProjectReadRequest, ProjectReadResult, WireSlotRootSnapshot, WireSlotRootsSnapshot,
    WireTreeDelta,
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

    let response = runtime.read_project(ProjectReadRequest::default_debug(None));
    let mut shape_registry = None;
    let mut roots = Vec::new();
    let mut node_refs = BTreeMap::new();

    for result in response.results {
        match result {
            ProjectReadResult::Shapes(result) => {
                shape_registry = result.registry;
            }
            ProjectReadResult::Nodes(result) => {
                if let Some(snapshot) = result.slots {
                    roots = snapshot.roots;
                }
                for delta in result.tree_deltas {
                    if let WireTreeDelta::Created { id, path, .. } = delta {
                        node_refs.insert(path.to_string(), id);
                    }
                }
            }
            ProjectReadResult::Resources(_) | ProjectReadResult::Runtime(_) => {}
        }
    }

    let shape_registry = shape_registry.ok_or("project read did not include shape registry")?;

    for spec in STORY_NODES {
        let node_id = *node_refs
            .get(spec.path)
            .ok_or_else(|| format!("project read did not include node {}", spec.path))?;
        let node_roots = roots_for_node(&roots, node_id);
        if node_roots.is_empty() {
            return Err(format!("node {} had no slot roots", spec.path).into());
        }

        let shape_json = StoryShapeJson {
            source: StorySource::new(&args.project, response.revision.as_i64()),
            node: StoryNodeRef::new(spec, node_id),
            root_shapes: root_shape_refs(&node_roots),
            registry: shape_registry_for_roots(&shape_registry, &node_roots),
        };
        write_json(
            &args.out.join(format!("{}.shape.json", spec.slug)),
            &shape_json,
        )?;

        let slot_json = StorySlotJson {
            source: StorySource::new(&args.project, response.revision.as_i64()),
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
