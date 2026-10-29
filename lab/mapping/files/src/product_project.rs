//! A real LightPlayer project, opened: the lab's [`Project`] plus
//! everything the lab doesn't model, kept to be written back untouched.
//!
//! What is read (fit-check §6): `project.json` → the root `module.json` →
//! every node file it refers to, recursively. For a fixture, its mapping
//! (`*.map2d.json`), its patch (`*.patch.json`, if any), the channels it
//! reads and writes and its colour order; for an output, its ports, its
//! name and the channel it reads; for a module, its nodes and exports; for
//! a shader (or another node writing a `visual.*` channel), the channel
//! it writes and its GLSL, which the lab shows and never runs — it gets a
//! stand-in pattern picked from its GLSL. Every other node (a clock, a
//! button) opens as a plain card, and every file keeps its JSON — fields
//! the lab knows nothing about included.
//!
//! Ids:
//!
//! | Lab | Product |
//! |---|---|
//! | node id | the node's name (its key in its module), made unique in the project |
//! | component id | the mapping object's `id`, minted from its name when it has none (the product's `slugify`), made unique in the project |
//! | object `sector` / `2` | object path `/sector/2` |
//! | port key | port key (a number) |
//! | output label | output `name` |
//!
//! Placements: a fixture's patch rows become placements on their objects
//! (a wire at the port and pixel worked out from the row's flat wire
//! position, reversed and turned as the row says); unless its patch says
//! `flow: manual`, the fixture's own wire is auto, onto the default output
//! of the channel it writes (`channels.rs`).

use std::collections::{BTreeMap, BTreeSet};

use lab_mapping_model::{
    Channel, ColorOrder, Component, ComponentId, ComponentKind, Direction, Fixture, Node, NodeId,
    NodeKind, ObjectId, OtherNode, Output, PatchPlace, PatchTarget, Pattern, Port, Project,
    RenderSource, Unsupported, Vec2, Wire,
};
use serde_json::Value;

use crate::channels::{exports, fixture_reads, fixture_writes, output_reads, visual_writes};
use crate::error::FilesError;
use crate::map2d_doc::{Map2dDoc, unsupported_shape};
use crate::patch_doc::{PatchDoc, PatchSource, valid_object_id};
use crate::source::{Source, join};

/// The project format this copy reads (`PROJECT_FORMAT_VERSION`).
pub const PROJECT_FORMAT: u64 = 11;

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
    /// A fixture's mapping.
    pub map: Option<MapFile>,
    /// A fixture's patch, if it has one.
    pub patch: Option<PatchFile>,
}

#[derive(Debug, Clone)]
pub struct MapFile {
    pub path: String,
    pub doc: Map2dDoc,
    /// Each component it opened as, and the object it came from.
    pub objects: BTreeMap<ComponentId, MapObjectFile>,
    /// The box's corner in the document (its canvas's `min_x, min_y`).
    pub canvas_min: Vec2,
    /// The box the lab gave a document with no canvas (none is written
    /// back while the box stays this size).
    pub derived_size: Option<Vec2>,
}

#[derive(Debug, Clone)]
pub struct MapObjectFile {
    /// Its id in the document: the file's, or the one the product would
    /// mint from its name.
    pub product_id: String,
    /// The file gave it an id (so saving writes one).
    pub had_id: bool,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct PatchFile {
    pub path: String,
    pub doc: PatchDoc,
    /// Rows written as a plain lamp range (`"range": {start, count}`) that
    /// cover one whole object, by that object: the lab places it as the
    /// object, and writes the row back as the same range while the object
    /// still covers exactly those lamps, so an unedited save changes nothing.
    pub ranges: BTreeMap<ObjectId, (u32, Option<u32>)>,
}

impl Opened {
    /// Open a project from its files.
    pub fn open(src: &dyn Source) -> Result<Opened, FilesError> {
        let manifest = read_json(src, "project.json")?;
        let format = manifest.get("format").and_then(Value::as_u64);
        if format != Some(PROJECT_FORMAT) {
            return Err(FilesError::new(
                "project.json",
                format!("the lab reads project format {PROJECT_FORMAT}, not {format:?}"),
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
        let mut patches: Vec<(NodeId, String, PatchDoc)> = Vec::new();
        let mut reader = Reader {
            src,
            files: &mut files,
            patches: &mut patches,
            taken_components: BTreeSet::new(),
            taken_nodes: BTreeSet::new(),
        };
        *project.nodes_mut() = reader.module("module.json", &root, None)?;
        // Rules, now that every output is known. A patch with a row the
        // lab can't place leaves its fixture as its box alone, and the rest
        // of the project opens round it (plan M4).
        for (fixture, path, doc) in patches {
            let rules = match rules_of(&project, &files, &fixture, Some(&doc)) {
                Ok((rules, ranges)) => {
                    if let Some(patch) =
                        files.nodes.get_mut(&fixture).and_then(|f| f.patch.as_mut())
                    {
                        patch.ranges = ranges;
                    }
                    rules
                }
                Err(RulesRefusal::Unsupported(why)) => {
                    if let Some(f) = project.fixture_mut(&fixture) {
                        f.components.clear();
                        f.unsupported = Some(why);
                    }
                    continue;
                }
                Err(RulesRefusal::Broken(what)) => return Err(FilesError::new(&path, what)),
            };
            if let Some(f) = project.fixture_mut(&fixture) {
                f.places = rules;
            }
        }
        let unpatched: Vec<NodeId> = project
            .fixtures()
            .iter()
            .filter(|n| files.nodes.get(&n.id).is_some_and(|f| f.patch.is_none()))
            .filter(|n| n.fixture().is_some_and(|f| f.unsupported.is_none()))
            .map(|n| n.id.clone())
            .collect();
        for fixture in unpatched {
            let (rules, _) = rules_of(&project, &files, &fixture, None).map_err(|r| match r {
                RulesRefusal::Unsupported(why) => FilesError::new("", why.what),
                RulesRefusal::Broken(what) => FilesError::new("", what),
            })?;
            if let Some(f) = project.fixture_mut(&fixture) {
                f.places = rules;
            }
        }
        Ok(Opened { project, files })
    }
}

struct Reader<'a> {
    src: &'a dyn Source,
    files: &'a mut ProductProject,
    patches: &'a mut Vec<(NodeId, String, PatchDoc)>,
    taken_components: BTreeSet<ComponentId>,
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
            let node_value = read_json(self.src, &node_path)?;
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
                map: None,
                patch: None,
            };
            let kind = match kind.as_str() {
                "Module" => NodeKind::Module {
                    children: self.module(&node_path, &node_value, Some(&id))?,
                },
                "Fixture" => {
                    let fixture = self.fixture(&id, &node_path, &node_value, &mut file)?;
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
        id: &NodeId,
        path: &str,
        value: &Value,
        file: &mut NodeFile,
    ) -> Result<Fixture, FilesError> {
        let mapping = value.get("mapping");
        // A fixture mapped some other way (the product's older inline point
        // lists, or none yet): its box alone, edited in Nodes.
        if mapping.and_then(|m| m.get("kind")).and_then(Value::as_str) != Some("Map2d") {
            let what = match mapping.and_then(|m| m.get("kind")).and_then(Value::as_str) {
                Some("PathPoints") => "its point-list mapping".to_string(),
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
        // A drawing the lab can't hold yet: the fixture is its box alone,
        // and its files are kept as they are (no map, no patch read).
        if let Some(what) = unsupported_shape(&text) {
            let mut fixture = Fixture::new(canvas_size(&text));
            fixture.unsupported = Some(Unsupported {
                what: format!("its {what} shape"),
                edit_in: "Map".into(),
            });
            fixture.color_order = color_order_of(value);
            fixture.reads = Channel::new(fixture_reads(value));
            return Ok(fixture);
        }
        let doc = Map2dDoc::parse(&text).map_err(|e| FilesError::new(&map_path, e))?;
        let render_size = render_size_of(value);

        // Ids: the file's, else the product's slug of the name, unique in
        // the document — then unique in the project for the lab.
        let mut doc_ids: BTreeSet<String> =
            doc.objects.iter().filter_map(|o| o.id.clone()).collect();
        let mut components = Vec::new();
        let mut objects = BTreeMap::new();
        for (i, o) in doc.objects.iter().enumerate() {
            let product_id = match &o.id {
                Some(id) if valid_object_id(id) => id.clone(),
                Some(id) => {
                    return Err(FilesError::new(
                        &map_path,
                        format!("object id {id:?} isn't a valid id"),
                    ));
                }
                None => {
                    let base = slugify(&o.name).unwrap_or_else(|| format!("obj-{}", i + 1));
                    let id = uniquify(&base, &doc_ids);
                    doc_ids.insert(id.clone());
                    id
                }
            };
            let lab_id = self.component_id(&product_id);
            objects.insert(
                lab_id.clone(),
                MapObjectFile {
                    product_id,
                    had_id: o.id.is_some(),
                    value: o.value.clone(),
                },
            );
            components.push(Component {
                id: lab_id,
                kind: o.kind.clone(),
            });
        }

        // The box: the canvas, from (0, 0). A canvas that starts elsewhere
        // keeps its corner on disk; the lab's box reaches back to (0, 0).
        let (size, canvas_min, derived_size) = match doc.canvas {
            Some([x, y, w, h]) => (Vec2::new(x + w, y + h), Vec2::new(x, y), None),
            None => {
                let size = derived_box(&components);
                (size, Vec2::default(), Some(size))
            }
        };
        let mut fixture = Fixture::new(size);
        fixture.render_size = render_size;
        fixture.components = components;
        fixture.color_order = color_order_of(value);
        fixture.reads = Channel::new(fixture_reads(value));
        file.map = Some(MapFile {
            path: map_path,
            doc,
            objects,
            canvas_min,
            derived_size,
        });

        // The patch is read now and turned into rules once every output
        // is known.
        if let Some(patch) = value.get("patch")
            && patch.get("kind").and_then(Value::as_str) == Some("File")
            && let Some(source) = patch.get("source").and_then(Value::as_str)
        {
            let patch_path = join(path, source);
            let text = read_text(self.src, &patch_path)?;
            let doc = PatchDoc::parse(&text).map_err(|e| FilesError::new(&patch_path, e))?;
            self.patches
                .push((id.clone(), patch_path.clone(), doc.clone()));
            file.patch = Some(PatchFile {
                ranges: BTreeMap::new(),
                path: patch_path,
                doc,
            });
        }
        Ok(fixture)
    }

    fn component_id(&mut self, product_id: &str) -> ComponentId {
        let mut id = ComponentId::new(product_id);
        let mut n = 2;
        while self.taken_components.contains(&id) {
            id = ComponentId::new(format!("{product_id}-{n}"));
            n += 1;
        }
        self.taken_components.insert(id.clone());
        id
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

/// A mapping document's box, from `(0, 0)` to its canvas's far corner;
/// the default square when it says none.
fn canvas_size(text: &str) -> Vec2 {
    let canvas = serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("canvas").cloned());
    let n = |c: &Value, i: usize| c.get(i).and_then(Value::as_f64);
    match canvas {
        Some(c) => match (n(&c, 0), n(&c, 1), n(&c, 2), n(&c, 3)) {
            (Some(x), Some(y), Some(w), Some(h)) => Vec2::new(x + w, y + h),
            _ => Fixture::square().size,
        },
        None => Fixture::square().size,
    }
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

/// Why a fixture's patch didn't become rules.
pub(crate) enum RulesRefusal {
    /// A row the lab can't place yet: the fixture opens as its box.
    Unsupported(Unsupported),
    /// A patch that doesn't hold together: the project doesn't open.
    Broken(String),
}

impl From<String> for RulesRefusal {
    fn from(what: String) -> Self {
        RulesRefusal::Broken(what)
    }
}

impl From<&str> for RulesRefusal {
    fn from(what: &str) -> Self {
        RulesRefusal::Broken(what.to_string())
    }
}

/// A fixture's rules from its patch (or its lack of one), and the rows
/// that were plain lamp ranges covering one whole object
/// ([`PatchFile::ranges`]).
pub(crate) fn rules_of(
    project: &Project,
    files: &ProductProject,
    fixture: &NodeId,
    patch: Option<&PatchDoc>,
) -> Result<(Vec<PatchPlace>, BTreeMap<ObjectId, (u32, Option<u32>)>), RulesRefusal> {
    let default = default_output(project, files, fixture);
    let mut rules = Vec::new();
    let mut ranges = BTreeMap::new();
    let spans = object_lamp_ranges(project, fixture);
    let manual = patch.is_some_and(|d| d.manual);
    for (i, e) in patch
        .map(|d| d.entries.as_slice())
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let part = || {
            RulesRefusal::Unsupported(Unsupported {
                what: format!("patch row {} (it places part of an object)", i + 1),
                edit_in: "Patch".into(),
            })
        };
        let object = match &e.source {
            PatchSource::Path { path, range: None } => {
                object_of(project, files, fixture, &path.id, &path.instances).ok_or_else(|| {
                    format!("row {i}: {} names no object the lab opened", path.text())
                })?
            }
            // A plain range is the object it covers, when it covers one
            // whole object exactly.
            PatchSource::Range(start, count) => {
                let total: u32 = spans.iter().map(|(_, _, n)| n).sum();
                let end = count.map_or(total, |c| start + c);
                let (object, _, _) = spans
                    .iter()
                    .find(|(_, s, n)| s == start && s + n == end)
                    .ok_or_else(part)?;
                ranges.insert(object.clone(), (*start, *count));
                object.clone()
            }
            PatchSource::Path { .. } => return Err(part()),
        };
        let output = match &e.output {
            Some(name) => project
                .attach_order()
                .into_iter()
                .find(|n| n.output().is_some_and(|o| o.label.as_deref() == Some(name)))
                .map(|n| n.id.clone())
                .ok_or_else(|| format!("row {i}: no output is named {name:?}"))?,
            None => default
                .clone()
                .ok_or_else(|| format!("row {i}: the default output, but there is none"))?,
        };
        let out = project.output(&output).ok_or("an output that isn't one")?;
        let (port, pixel) = match out.port_at(e.lamp) {
            Some((p, pixel)) => (out.ports[p].key.clone(), pixel),
            // Past every port: on the last one, off its end (dark on the
            // device too).
            None => {
                let last = out
                    .ports
                    .last()
                    .ok_or_else(|| format!("row {i}: an output with no ports"))?;
                let start = out.port_start(&last.key).unwrap_or(0);
                (last.key.clone(), e.lamp - start)
            }
        };
        // One placement per object: the first row wins.
        let target = PatchTarget::Object(object);
        if rules.iter().any(|r: &PatchPlace| r.on == target) {
            continue;
        }
        rules.push(PatchPlace {
            on: target,
            wire: Some(Wire::At {
                output,
                port,
                pixel,
            }),
            direction: e.reversed.then_some(Direction::Reversed),
            turn: (e.offset != 0).then_some(e.offset),
        });
    }
    if !manual && let Some(output) = default {
        rules.push(PatchPlace {
            wire: Some(Wire::Auto { output }),
            ..PatchPlace::on(PatchTarget::Fixture)
        });
    }
    Ok((rules, ranges))
}

/// Each of a fixture's objects, with where its lamps start in the
/// fixture's own numbering and how many it has, in wiring order.
pub(crate) fn object_lamp_ranges(project: &Project, fixture: &NodeId) -> Vec<(ObjectId, u32, u32)> {
    fn walk(list: &[Component], start: &mut u32, out: &mut Vec<(ObjectId, u32, u32)>) {
        for c in list {
            for o in lab_mapping_model::domain::object::objects_of(c) {
                let n = o.lamps.len() as u32;
                out.push((o.id, *start, n));
                *start += n;
            }
            walk(c.children(), start, out);
        }
    }
    let mut out = Vec::new();
    if let Some(f) = project.fixture(fixture) {
        walk(&f.components, &mut 0, &mut out);
    }
    out
}

/// The object a patch path names: `/door` (a path, a polygon, a
/// one-ring circle, a grid, a ring set, a filled polygon), `/sector/2` (a
/// copy of a repeat).
fn object_of(
    project: &Project,
    files: &ProductProject,
    fixture: &NodeId,
    product_id: &str,
    instances: &[u32],
) -> Option<ObjectId> {
    let map = files.nodes.get(fixture)?.map.as_ref()?;
    let (id, _) = map
        .objects
        .iter()
        .find(|(_, o)| o.product_id == product_id)?;
    let key = match (&project.component(id)?.kind, instances) {
        (ComponentKind::Path { .. }, []) => lab_mapping_model::domain::object::PATH_KEY.to_string(),
        (ComponentKind::Polygon { .. }, []) => {
            lab_mapping_model::domain::object::POLYGON_KEY.to_string()
        }
        (ComponentKind::Circle { rings, .. }, []) if rings.len() == 1 => rings[0].key.clone(),
        (ComponentKind::Grid(_), []) => lab_mapping_model::domain::object::GRID_KEY.to_string(),
        (ComponentKind::RingSet(_), []) => lab_mapping_model::domain::object::RINGS_KEY.to_string(),
        (ComponentKind::Filled(_), []) => lab_mapping_model::domain::object::FILLED_KEY.to_string(),
        (ComponentKind::Repeat { shape, count, .. }, [k])
            if *k < *count && !matches!(**shape, ComponentKind::Repeat { .. }) =>
        {
            k.to_string()
        }
        _ => return None,
    };
    Some(ObjectId::new(id, key))
}

/// The output a fixture auto-flows onto: the first output, in attach
/// order, reading the channel it writes — in its module, or in a parent
/// its module exports the channel to.
pub(crate) fn default_output(
    project: &Project,
    files: &ProductProject,
    fixture: &NodeId,
) -> Option<NodeId> {
    let value = &files.nodes.get(fixture)?.value;
    let scopes = scopes(project, files, fixture, fixture_writes(value));
    project
        .attach_order()
        .into_iter()
        .filter(|n| n.output().is_some())
        .find(|n| {
            let reads = files
                .nodes
                .get(&n.id)
                .map_or_else(|| "control.out".to_string(), |f| output_reads(&f.value));
            let module = project.parent_module(&n.id).cloned();
            scopes.contains(&(module, reads))
        })
        .map(|n| n.id.clone())
}

/// Where a node's channel can be read: (module, channel name there), its
/// own module first, then up through every module that exports it.
pub(crate) fn scopes(
    project: &Project,
    files: &ProductProject,
    node: &NodeId,
    channel: String,
) -> Vec<(Option<NodeId>, String)> {
    let mut out = Vec::new();
    let mut module = project.parent_module(node).cloned();
    let mut channel = channel;
    loop {
        out.push((module.clone(), channel.clone()));
        let Some(m) = module else { break };
        let exported = files
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

/// A box for a document with no canvas: from (0, 0) out past its lamps.
fn derived_box(components: &[Component]) -> Vec2 {
    let mut max = Vec2::new(1.0, 1.0);
    for c in components {
        for o in lab_mapping_model::domain::object::objects_of(c) {
            for l in o.lamps {
                max = Vec2::new(max.x.max(l.pos.x), max.y.max(l.pos.y));
            }
        }
    }
    max
}

/// The product's `slugify`: lowercased, runs of anything else one `-`,
/// starting with a letter, at most 24 bytes.
pub fn slugify(name: &str) -> Option<String> {
    let mut slug = String::new();
    let mut dash = false;
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-';
        if slug.is_empty() {
            if c.is_ascii_lowercase() {
                slug.push(c);
            }
            continue;
        }
        if keep {
            if dash {
                slug.push('-');
                dash = false;
            }
            slug.push(c);
        } else {
            dash = true;
        }
    }
    slug.truncate(24);
    while slug.ends_with('-') {
        slug.pop();
    }
    valid_object_id(&slug).then_some(slug)
}

/// The product's `uniquify`: `base`, else `base-2`, `base-3`, … within 24
/// bytes.
pub fn uniquify(base: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    for n in 2u32.. {
        let suffix = format!("-{n}");
        let mut stem = base.to_string();
        stem.truncate(24usize.saturating_sub(suffix.len()));
        while stem.ends_with('-') {
            stem.pop();
        }
        let id = format!("{stem}{suffix}");
        if !taken.contains(&id) {
            return id;
        }
    }
    unreachable!("more objects than u32")
}

fn read_text(src: &dyn Source, path: &str) -> Result<String, FilesError> {
    src.read(path)
        .ok_or_else(|| FilesError::new(path, "missing"))
}

fn read_json(src: &dyn Source, path: &str) -> Result<Value, FilesError> {
    let text = read_text(src, path)?;
    serde_json::from_str(&text).map_err(|e| FilesError::new(path, e.to_string()))
}
