//! A real LightPlayer project, opened: the lab's [`Project`] plus
//! everything the lab doesn't model, kept to be written back untouched.
//!
//! What is read (fit-check §6): `project.json` → the root `module.json` →
//! every node file it refers to, recursively. For a fixture, its mapping
//! (`*.map2d.json`, map2d format 6, read by `lpc_mapping::map2d6::read`
//! into the fixture's `Mapping`: its parts and its patching), the channels
//! it reads and its colour order; for an output, its ports, its name and
//! the channel it reads; for a module, its nodes and exports; for a shader
//! (or another node writing a `visual.*` channel), the channel it writes
//! and its GLSL, which the lab shows and never runs — it gets a stand-in
//! pattern picked from its GLSL. Every other node (a clock, a button)
//! opens as a plain card, and every file keeps its JSON — fields the lab
//! knows nothing about included.
//!
//! Ids:
//!
//! | Lab | Product |
//! |---|---|
//! | node id | the node's name (its key in its module), made unique in the project |
//! | part id | the map2d's part id, as it is (a part is named with its fixture) |
//! | port key | port key (a number) |
//! | output label | output `name` |
//!
//! **Only format 6.** A project at another project format, or a mapping in
//! an older map2d format, is refused by name: Studio upgrades a project
//! when it opens it, so neither reaches here from Studio.

use std::collections::{BTreeMap, BTreeSet};

use lab_mapping_model::{
    Channel, ColorOrder, Fixture, Node, NodeId, NodeKind, OtherNode, Output, Pattern, Port,
    Project, RenderSource, Unsupported, Vec2,
};
use lpc_mapping::map2d6::{self, MAP2D6_FORMAT};
use lpc_mapping::part::render;
use serde_json::Value;

use crate::channels::{exports, fixture_reads, fixture_writes, output_reads, visual_writes};
use crate::error::FilesError;
use crate::source::{Source, join};

/// The project format this reads (`PROJECT_FORMAT_VERSION`).
pub const PROJECT_FORMAT: u64 = 12;

/// A project opened from the product's files.
#[derive(Debug, Clone)]
pub struct Opened {
    pub project: Project,
    pub files: ProductProject,
}

/// Everything read, as it was read: what saving starts from.
#[derive(Debug, Clone)]
pub struct ProductProject {
    /// `project.json`.
    pub manifest: Value,
    /// The root `module.json`.
    pub root: Value,
    /// Every node's file, by the lab's node id.
    pub nodes: BTreeMap<NodeId, NodeFile>,
}

#[derive(Debug, Clone)]
pub struct NodeFile {
    /// Where it lives, from the project's root.
    pub path: String,
    /// Its name in its module (the key in `nodes`).
    pub name: String,
    pub value: Value,
    /// The file as it was read: what a product reader (Studio's port
    /// tables read an output's def from it) reads, in its own key order.
    pub text: String,
    /// A fixture's mapping file.
    pub map: Option<MapFile>,
}

impl NodeFile {
    /// The files this node alone stands for: its own, its mapping's, and a
    /// shader's GLSL. Deleting the node deletes them (ruling DD36).
    pub fn own_files(&self) -> Vec<String> {
        let mut out = vec![self.path.clone()];
        out.extend(self.map.iter().map(|m| m.path.clone()));
        if self.value.get("kind").and_then(Value::as_str) == Some("Shader")
            && let Some(glsl) = self.value.get("source").and_then(Value::as_str)
        {
            out.push(crate::source::join(&self.path, &format!("./{glsl}")));
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct MapFile {
    pub path: String,
    /// The file as it was read.
    pub text: String,
    /// The mapping it holds, as read: what saving compares the fixture's
    /// against (a mapping that is the same is not written).
    pub mapping: lpc_mapping::part::Mapping,
    /// The box the lab gave a document with no canvas (none is written
    /// back while the box stays this size).
    pub derived_size: Option<Vec2>,
}

impl Opened {
    /// Open a project from its files.
    pub fn open(src: &dyn Source) -> Result<Opened, FilesError> {
        let manifest = read_json(src, "project.json")?;
        let format = manifest.get("format").and_then(Value::as_u64);
        if format != Some(PROJECT_FORMAT) {
            return Err(FilesError::new(
                "project.json",
                match format {
                    Some(f) if f < PROJECT_FORMAT => format!(
                        "this project is at project format {f}, from an older LightPlayer; \
                         the Graph reads project format {PROJECT_FORMAT} (Studio upgrades a \
                         project when it opens it)"
                    ),
                    Some(f) => format!(
                        "this project is at project format {f}, from a newer LightPlayer; \
                         the Graph reads project format {PROJECT_FORMAT}"
                    ),
                    None => format!(
                        "this project says no format; the Graph reads project format \
                         {PROJECT_FORMAT}"
                    ),
                },
            ));
        }
        let root = read_json(src, "module.json")?;
        let name = manifest
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("project");
        let mut project = Project::new(name);
        let mut files = ProductProject {
            manifest: manifest.clone(),
            root: root.clone(),
            nodes: BTreeMap::new(),
        };
        let mut reader = Reader {
            src,
            files: &mut files,
            taken_nodes: BTreeSet::new(),
        };
        *project.nodes_mut() = reader.module("module.json", &root, None)?;
        Ok(Opened { project, files })
    }
}

struct Reader<'a> {
    src: &'a dyn Source,
    files: &'a mut ProductProject,
    taken_nodes: BTreeSet<NodeId>,
}

impl Reader<'_> {
    /// A module's nodes, in name order (the product's: its node map is
    /// sorted).
    fn module(
        &mut self,
        path: &str,
        value: &Value,
        parent: Option<&NodeId>,
    ) -> Result<Vec<Node>, FilesError> {
        let mut entries: Vec<(&String, &Value)> = value
            .get("nodes")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = Vec::new();
        for (name, entry) in entries {
            let file_ref = entry.get("ref").and_then(Value::as_str).ok_or_else(|| {
                FilesError::new(
                    path,
                    format!("node {name} has no ref (inline nodes aren't read)"),
                )
            })?;
            let node_path = join(path, file_ref);
            let node_text = read_text(self.src, &node_path)?;
            let node_value: Value = serde_json::from_str(&node_text)
                .map_err(|e| FilesError::new(&node_path, e.to_string()))?;
            let id = self.node_id(name, parent);
            self.taken_nodes.insert(id.clone());
            let kind = node_value
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let mut file = NodeFile {
                path: node_path.clone(),
                name: name.clone(),
                value: node_value.clone(),
                text: node_text,
                map: None,
            };
            let kind = match kind.as_str() {
                "Module" => NodeKind::Module {
                    children: self.module(&node_path, &node_value, Some(&id))?,
                },
                "Fixture" => {
                    let fixture = self.fixture(&node_path, &node_value, &mut file)?;
                    NodeKind::Fixture(fixture)
                }
                "Output" => NodeKind::Output(output_of(&node_value)),
                other => match visual_writes(&node_value, other) {
                    Some(writes) => {
                        let glsl = node_value
                            .get("source")
                            .and_then(Value::as_str)
                            .and_then(|f| self.src.read(&join(&node_path, f)));
                        NodeKind::Source(RenderSource {
                            kind: other.to_string(),
                            pattern: Pattern::pick(glsl.as_deref().unwrap_or(name)),
                            glsl,
                            writes: Channel::new(writes),
                        })
                    }
                    None => NodeKind::Other(OtherNode::new(other)),
                },
            };
            self.files.nodes.insert(id.clone(), file);
            out.push(Node {
                id,
                name: name.clone(),
                kind,
                pinned: None,
            });
        }
        Ok(out)
    }

    /// The node's name, unless another node in the project has it: then
    /// its module's id before it.
    fn node_id(&self, name: &str, parent: Option<&NodeId>) -> NodeId {
        let mut id = NodeId::new(name);
        if self.taken_nodes.contains(&id)
            && let Some(p) = parent
        {
            id = NodeId::new(format!("{p}.{name}"));
        }
        let base = id.clone();
        let mut n = 2;
        while self.taken_nodes.contains(&id) {
            id = NodeId::new(format!("{base}-{n}"));
            n += 1;
        }
        id
    }

    fn fixture(
        &mut self,
        path: &str,
        value: &Value,
        file: &mut NodeFile,
    ) -> Result<Fixture, FilesError> {
        let mapping = value.get("mapping");
        // A fixture mapped some other way (none yet): its box alone,
        // edited in Nodes.
        if mapping.and_then(|m| m.get("kind")).and_then(Value::as_str) != Some("Map2d") {
            let what = match mapping.and_then(|m| m.get("kind")).and_then(Value::as_str) {
                None | Some("Unset") => "a fixture with no mapping".to_string(),
                Some(other) => format!("its {other} mapping"),
            };
            let mut fixture = Fixture::square();
            fixture.unsupported = Some(Unsupported {
                what,
                edit_in: "Nodes".into(),
            });
            fixture.color_order = color_order_of(value);
            fixture.reads = Channel::new(fixture_reads(value));
            return Ok(fixture);
        }
        let source = mapping
            .and_then(|m| m.get("source"))
            .and_then(Value::as_str)
            .ok_or_else(|| FilesError::new(path, "mapping has no source"))?;
        let map_path = join(path, source);
        let text = read_text(self.src, &map_path)?;
        match map2d6::peek_format(&text) {
            Ok(MAP2D6_FORMAT) => {}
            Ok(older) if older < MAP2D6_FORMAT => {
                return Err(FilesError::new(
                    &map_path,
                    format!(
                        "this mapping is map2d format {older}, from an older LightPlayer; the \
                         Graph reads map2d format {MAP2D6_FORMAT} (Studio upgrades a project \
                         when it opens it)"
                    ),
                ));
            }
            Ok(newer) => {
                return Err(FilesError::new(
                    &map_path,
                    format!(
                        "this mapping is map2d format {newer}, from a newer LightPlayer; the \
                         Graph reads map2d format {MAP2D6_FORMAT}"
                    ),
                ));
            }
            Err(why) => return Err(FilesError::new(&map_path, why.to_string())),
        }
        let mapping = map2d6::read(&text).map_err(|e| FilesError::new(&map_path, e.to_string()))?;

        // The box: the canvas, from (0, 0). A canvas that starts elsewhere
        // keeps its corner on disk; the lab's box reaches back to (0, 0).
        let (size, derived_size) = match mapping.canvas {
            Some([x, y, w, h]) => (Vec2::new(f64::from(x + w), f64::from(y + h)), None),
            None => {
                let size = derived_box(&mapping);
                (size, Some(size))
            }
        };
        let mut fixture = Fixture::new(size);
        fixture.render_size = render_size_of(value);
        fixture.mapping = mapping.clone();
        fixture.color_order = color_order_of(value);
        fixture.reads = Channel::new(fixture_reads(value));
        file.map = Some(MapFile {
            path: map_path,
            text,
            mapping,
            derived_size,
        });
        Ok(fixture)
    }
}

impl ProductProject {
    /// The outputs a fixture's product reaches: every output, in attach
    /// order, reading the bus channel the fixture writes — in its module,
    /// or in a parent its module exports the channel to. What its port
    /// tables are made of (only the named ones can be patched to).
    pub fn reachable_outputs(&self, project: &Project, fixture: &NodeId) -> Vec<NodeId> {
        let Some(value) = self.nodes.get(fixture).map(|f| &f.value) else {
            return Vec::new();
        };
        let scopes = self.scopes(project, fixture, fixture_writes(value));
        project
            .attach_order()
            .into_iter()
            .filter(|n| n.output().is_some())
            .filter(|n| {
                let reads = self
                    .nodes
                    .get(&n.id)
                    .map_or_else(|| "control.out".to_string(), |f| output_reads(&f.value));
                let module = project.parent_module(&n.id).cloned();
                scopes.contains(&(module, reads))
            })
            .map(|n| n.id.clone())
            .collect()
    }

    /// Where a node's channel can be read: (module, channel name there),
    /// its own module first, then up through every module that exports it.
    fn scopes(
        &self,
        project: &Project,
        node: &NodeId,
        channel: String,
    ) -> Vec<(Option<NodeId>, String)> {
        let mut out = Vec::new();
        let mut module = project.parent_module(node).cloned();
        let mut channel = channel;
        loop {
            out.push((module.clone(), channel.clone()));
            let Some(m) = module else { break };
            let exported = self
                .nodes
                .get(&m)
                .map(|f| exports(&f.value))
                .unwrap_or_default()
                .into_iter()
                .find(|(_, inside)| *inside == channel);
            let Some((outside, _)) = exported else { break };
            channel = outside;
            module = project.parent_module(&m).cloned();
        }
        out
    }
}

/// A fixture's colour order, or the product's default.
fn color_order_of(value: &Value) -> ColorOrder {
    value
        .get("color_order")
        .and_then(Value::as_str)
        .and_then(ColorOrder::from_key)
        .unwrap_or_default()
}

/// A fixture's `render_size`, when it gives one.
fn render_size_of(value: &Value) -> Option<(u32, u32)> {
    let r = value.get("render_size")?;
    let n = |k: &str| {
        r.get(k)
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
    };
    Some((n("width")?, n("height")?)).filter(|(w, h)| *w > 0 && *h > 0)
}

/// An output's ports (in key order), and its name.
fn output_of(value: &Value) -> Output {
    let mut ports: Vec<(u64, Port)> = value
        .get("ports")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, p)| {
            let k: u64 = key.parse().ok()?;
            Some((
                k,
                Port {
                    key: key.clone(),
                    name: p
                        .get("endpoint")
                        .and_then(Value::as_str)
                        .unwrap_or("ws281x:local:D10")
                        .to_string(),
                    pixels: p.get("count").and_then(Value::as_u64).map(|n| n as u32),
                },
            ))
        })
        .collect();
    ports.sort_by_key(|(k, _)| *k);
    Output {
        ports: ports.into_iter().map(|(_, p)| p).collect(),
        label: value
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        reads: Channel::new(output_reads(value)),
    }
}

/// A box for a document with no canvas: from (0, 0) out past its lamps.
fn derived_box(mapping: &lpc_mapping::part::Mapping) -> Vec2 {
    let mut max = Vec2::new(1.0, 1.0);
    if let Ok(tree) = render(mapping) {
        for p in tree.positions() {
            max = Vec2::new(max.x.max(f64::from(p[0])), max.y.max(f64::from(p[1])));
        }
    }
    max
}

fn read_text(src: &dyn Source, path: &str) -> Result<String, FilesError> {
    src.read(path)
        .ok_or_else(|| FilesError::new(path, "missing"))
}

fn read_json(src: &dyn Source, path: &str) -> Result<Value, FilesError> {
    let text = read_text(src, path)?;
    serde_json::from_str(&text).map_err(|e| FilesError::new(path, e.to_string()))
}
