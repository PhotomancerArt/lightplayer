//! Saving: the lab's project back into the product's files.
//!
//! Only files the lab owns are written — module node maps, fixture and
//! output nodes, mappings and patches — and only when what they say has
//! changed (numbers compared as f32, the product's reading). A file the
//! lab didn't change is not written, so it stays byte for byte as it was;
//! nodes the lab doesn't edit (a shader, a clock) are never written at
//! all. A written file starts from the JSON it was read from, so fields
//! the lab knows nothing about are kept.
//!
//! The conversions, each fit-check's resolution:
//!
//! - **an auto rule** → the fixture writes the channel its output reads.
//!   If the output isn't the first reader of a channel the fixture can
//!   write, saving refuses and says why. An output reads the channel the
//!   model gives it (a new one its own, `control.<name>`, when another
//!   output beside it already reads `control.out`); a fixture reads the
//!   visual channel the model gives it.
//! - **a render source** (a shader) the lab opened is never written: the
//!   lab shows its GLSL and never edits it. One the lab made is written as
//!   a shader node with a stand-in GLSL file (`stand_in_shader.rs`).
//! - **a place rule** → a patch row: the object's path (`/sector/2`), the
//!   output's name (minted, `"1"`, `"2"`…, for an output that has none and
//!   isn't the default), and the port and pixel flattened onto the
//!   output's one wire. No auto rule → `flow: manual`.
//! - **a circle** → one ring shape per ring (each its own object, so the
//!   lamps stay put; the product has no addressable rings yet, fit-check
//!   §4). **A group** → its children, in order (the product has no groups
//!   yet). Both lose their grouping on disk.
//! - **node names** → a lab name the product would refuse (`tail rig`)
//!   is written as one it accepts (`tail_rig`).

use std::collections::{BTreeMap, BTreeSet};

use lab_mapping_model::{
    ColorOrder, Component, ComponentId, ComponentKind, Node, NodeId, NodeKind, ObjectId,
    PatchTarget, Project, Vec2, Wire,
};
use serde_json::{Map, Value, json};

use crate::error::FilesError;
use crate::json::{insert_sorted, num_f32, pretty, same, set};
use crate::map2d_doc::{required_format, shape_of};
use crate::patch_doc::{ObjectPath, PatchDoc, PatchEntry, PatchSource, valid_object_id};
use crate::product_project::{ProductProject, object_lamp_ranges, scopes, slugify, uniquify};
use crate::source::{file_name, join};

/// What saving did: the files to write (path, text), and the files no
/// longer referenced (a deleted node's), for the caller to delete. Both by
/// path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Saved {
    pub writes: Vec<(String, String)>,
    pub removed: Vec<String>,
}

impl ProductProject {
    /// Save `project`: the files that changed.
    pub fn save(&self, project: &Project) -> Result<Saved, FilesError> {
        Writer::new(self, project, false).run()
    }

    /// Every file the lab owns, changed or not — what the fit test reads
    /// back through the product to prove the writer.
    pub fn save_all(&self, project: &Project) -> Result<Saved, FilesError> {
        Writer::new(self, project, true).run()
    }
}

struct Writer<'a> {
    files: &'a ProductProject,
    project: &'a Project,
    force: bool,
    saved: Saved,
    /// Every node's name in its module and its file.
    places: BTreeMap<NodeId, (String, String)>,
    /// The channel each fixture writes and each output reads.
    channels: BTreeMap<NodeId, String>,
    /// Output names, the carried ones and any a patch row needs.
    labels: BTreeMap<NodeId, String>,
}

impl<'a> Writer<'a> {
    fn new(files: &'a ProductProject, project: &'a Project, force: bool) -> Self {
        Writer {
            files,
            project,
            force,
            saved: Saved::default(),
            places: BTreeMap::new(),
            channels: BTreeMap::new(),
            labels: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<Saved, FilesError> {
        self.place("module.json", self.project.nodes());
        self.choose_channels()?;
        self.choose_labels();
        let root = self.module_value(&self.files.root, "module.json", self.project.nodes());
        self.emit("module.json", Some(&self.files.root), &root);
        self.nodes(self.project.nodes())?;
        for (id, f) in &self.files.nodes {
            if self.project.node(id).is_none() {
                self.saved.removed.push(f.path.clone());
                self.saved
                    .removed
                    .extend(f.map.iter().map(|m| m.path.clone()));
                self.saved
                    .removed
                    .extend(f.patch.iter().map(|p| p.path.clone()));
            }
        }
        self.saved.writes.sort();
        self.saved.removed.sort();
        Ok(self.saved)
    }

    /// Name and file for every node: the carried ones, and fresh ones for
    /// what the lab made.
    fn place(&mut self, module_path: &str, nodes: &[Node]) {
        let mut names: BTreeSet<String> = nodes
            .iter()
            .filter_map(|n| self.files.nodes.get(&n.id).map(|f| f.name.clone()))
            .collect();
        for n in nodes {
            let (name, path) = match self.files.nodes.get(&n.id) {
                Some(f) => (f.name.clone(), f.path.clone()),
                None => {
                    let name = unique_name(&product_name(&n.name), &names);
                    names.insert(name.clone());
                    let file = match n.kind {
                        NodeKind::Module { .. } => format!("./{name}/module.json"),
                        _ => format!("./{name}.json"),
                    };
                    let path = join(module_path, &file);
                    (name, path)
                }
            };
            self.places.insert(n.id.clone(), (name, path.clone()));
            self.place(&path, n.children());
        }
    }

    /// Every output reads the channel the model says (an opened one, the
    /// channel its file named; a new one, `control.out` or a channel of its
    /// own, minted by the model). A fixture with an auto rule writes the
    /// channel its output reads.
    fn choose_channels(&mut self) -> Result<(), FilesError> {
        let p = self.project;
        for n in p.attach_order() {
            if let Some(o) = n.output() {
                self.channels.insert(n.id.clone(), o.reads.to_string());
            }
        }
        for n in p.fixtures() {
            let carried = self.files.nodes.get(&n.id).map_or_else(
                || "control.out".to_string(),
                |f| crate::channels::fixture_writes(&f.value),
            );
            let auto = p.auto_output(&n.id).filter(|o| p.output(o).is_some());
            let channel = match auto.as_ref() {
                None => carried,
                Some(x) if self.default_output(&n.id, &carried).as_ref() == Some(x) => carried,
                Some(x) => {
                    let reads = self.channels.get(x).cloned().unwrap_or_default();
                    if self.default_output(&n.id, &reads).as_ref() == Some(x) {
                        reads
                    } else {
                        return Err(FilesError::new(
                            &self.places[&n.id].1,
                            format!(
                                "{}'s auto rule names {}, which isn't the first output on any \
                                 channel {} can write; give {} a channel of its own",
                                n.name,
                                p.node_name(x),
                                n.name,
                                p.node_name(x)
                            ),
                        ));
                    }
                }
            };
            self.channels.insert(n.id.clone(), channel);
        }
        Ok(())
    }

    /// The first output, in attach order, reading `channel` where
    /// `fixture` writes it.
    fn default_output(&self, fixture: &NodeId, channel: &str) -> Option<NodeId> {
        let p = self.project;
        let scopes = scopes(p, self.files, fixture, channel.to_string());
        p.attach_order()
            .into_iter()
            .filter(|n| n.output().is_some())
            .find(|n| {
                let reads = self.channels.get(&n.id).cloned().unwrap_or_default();
                scopes.contains(&(p.parent_module(&n.id).cloned(), reads))
            })
            .map(|n| n.id.clone())
    }

    /// Carried names, and a new one (`"1"`, `"2"`, …) for an output a patch
    /// row must name.
    fn choose_labels(&mut self) {
        let p = self.project;
        for n in p.outputs() {
            if let Some(label) = n.output().and_then(|o| o.label.clone()) {
                self.labels.insert(n.id.clone(), label);
            }
        }
        for n in p.fixtures() {
            let channel = self.channels.get(&n.id).cloned().unwrap_or_default();
            let default = self.default_output(&n.id, &channel);
            let (rows, _) = patch_rows(p, n);
            for Row { output, .. } in &rows {
                if self.labels.contains_key(output) || Some(output) == default.as_ref() {
                    continue;
                }
                let taken: BTreeSet<&String> = self.labels.values().collect();
                let label = (1..)
                    .map(|i: u32| i.to_string())
                    .find(|l| !taken.contains(l))
                    .unwrap_or_default();
                self.labels.insert(output.clone(), label);
            }
        }
    }

    fn nodes(&mut self, nodes: &[Node]) -> Result<(), FilesError> {
        for n in nodes {
            let path = self.places[&n.id].1.clone();
            let old = self.files.nodes.get(&n.id).map(|f| &f.value);
            match &n.kind {
                NodeKind::Module { children } => {
                    let template = json!({"kind": "Module", "nodes": {}});
                    let value = self.module_value(old.unwrap_or(&template), &path, children);
                    self.emit(&path, old, &value);
                    self.nodes(children)?;
                }
                NodeKind::Fixture(_) => self.fixture(n, &path)?,
                NodeKind::Output(_) => {
                    let value = self.output_value(n, old);
                    self.emit(&path, old, &value);
                }
                NodeKind::Source(src) if old.is_none() => {
                    let name = &self.places[&n.id].0;
                    let glsl_path = join(&path, &format!("./{name}.glsl"));
                    let value = crate::stand_in_shader::node_value(src, &format!("{name}.glsl"));
                    self.emit(&path, None, &value);
                    self.saved
                        .writes
                        .push((glsl_path, crate::stand_in_shader::glsl(src)));
                }
                NodeKind::Source(_) | NodeKind::Other(_) => {}
            }
        }
        Ok(())
    }

    /// A module's node map: kept entries for the nodes still there, new
    /// ones for new nodes, gone ones dropped. Sorted, as the product
    /// writes it.
    fn module_value(&self, old: &Value, path: &str, children: &[Node]) -> Value {
        let mut value = old.clone();
        let old_nodes = old.get("nodes").and_then(Value::as_object);
        let mut nodes = Map::new();
        for n in children {
            let (name, child_path) = &self.places[&n.id];
            let entry = match old_nodes.and_then(|m| m.get(name)) {
                Some(e) if self.files.nodes.contains_key(&n.id) => e.clone(),
                _ => json!({ "ref": relative(path, child_path) }),
            };
            insert_sorted(&mut nodes, name, entry, false);
        }
        if let Some(obj) = value.as_object_mut() {
            obj.insert("nodes".into(), Value::Object(nodes));
        }
        value
    }

    fn output_value(&self, n: &Node, old: Option<&Value>) -> Value {
        let Some(o) = n.output() else {
            return Value::Null;
        };
        let mut value = old.cloned().unwrap_or_else(|| json!({"kind": "Output"}));
        let Some(obj) = value.as_object_mut() else {
            return value;
        };
        let had_name = obj.contains_key("name");
        match self.labels.get(&n.id) {
            Some(label) => insert_after(obj, "name", label.clone().into(), "kind"),
            None if had_name => {
                obj.remove("name");
            }
            None => {}
        }
        let old_ports = obj.get("ports").and_then(Value::as_object).cloned();
        let mut ports = Map::new();
        for port in &o.ports {
            let mut entry = old_ports
                .as_ref()
                .and_then(|m| m.get(&port.key))
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            entry.insert("endpoint".into(), endpoint(&port.name).into());
            set(&mut entry, "count", port.pixels.map(Value::from));
            insert_sorted(&mut ports, &port.key, Value::Object(entry), true);
        }
        obj.insert("ports".into(), Value::Object(ports));
        let channel = self.channels.get(&n.id).cloned().unwrap_or_default();
        if old.is_none() {
            obj.insert(
                "bindings".into(),
                json!({"input": {"source": format!("bus:{channel}")}}),
            );
        } else if old.is_some_and(|v| crate::channels::output_reads(v) != channel) {
            set_binding(&mut value, "input", "source", &channel);
        }
        value
    }

    fn fixture(&mut self, n: &Node, path: &str) -> Result<(), FilesError> {
        let Some(fx) = n.fixture() else {
            return Ok(());
        };
        // A fixture the lab can't hold yet is never written: its files
        // stay exactly as they were.
        if fx.unsupported.is_some() {
            return Ok(());
        }
        let carried = self.files.nodes.get(&n.id);
        let name = self.places[&n.id].0.clone();
        let mut value = carried.map(|f| f.value.clone()).unwrap_or_else(|| {
            json!({
                "kind": "Fixture",
                "render_size": {"width": 64, "height": 64},
                "bindings": {
                    "input": {"source": fx.reads.binding()},
                    "output": {"target": "bus:control.out"}
                },
                "sampling": "direct",
                "mapping": {"kind": "Map2d", "source": format!("{name}.map2d.json")},
            })
        });
        let map_path = match carried.and_then(|f| f.map.as_ref()) {
            Some(m) => m.path.clone(),
            None => join(path, &format!("./{name}.map2d.json")),
        };

        // Colour order, and the channel its auto rule needs.
        if let Some(obj) = value.as_object_mut() {
            let had = obj.contains_key("color_order");
            if had || fx.color_order != ColorOrder::default() {
                obj.insert("color_order".into(), fx.color_order.key().into());
            }
        }
        let channel = self.channels.get(&n.id).cloned().unwrap_or_default();
        if crate::channels::fixture_writes(&value) != channel {
            set_binding(&mut value, "output", "target", &channel);
        }
        if crate::channels::fixture_reads(&value) != fx.reads.as_str() {
            set_binding(&mut value, "input", "source", fx.reads.as_str());
        }

        // The patch.
        let addressed = self.patch(n, path, &name, &mut value)?;
        let old = carried.map(|f| &f.value);
        self.emit(path, old, &value);

        // The mapping. A document that is written anyway gives every
        // object its id (one a patch, a selection on the board or an agent
        // can name); one that hasn't changed is left as it is.
        let old_map = carried.and_then(|f| f.map.as_ref()).map(|m| &m.doc.value);
        let mut map = self.map_value(n, &addressed, false)?;
        if self.force || old_map.is_none_or(|o| !same(o, &map)) {
            map = self.map_value(n, &addressed, true)?;
        }
        self.emit(&map_path, old_map, &map);
        Ok(())
    }

    /// The fixture's patch file, written when it changed (or is needed for
    /// the first time). Returns the components its rows name, whose ids the
    /// mapping must carry.
    fn patch(
        &mut self,
        n: &Node,
        path: &str,
        name: &str,
        value: &mut Value,
    ) -> Result<BTreeSet<ComponentId>, FilesError> {
        let p = self.project;
        let Some(fx) = n.fixture() else {
            return Ok(BTreeSet::new());
        };
        let carried = self.files.nodes.get(&n.id);
        let channel = self.channels.get(&n.id).cloned().unwrap_or_default();
        let default = self.default_output(&n.id, &channel);
        let mut entries = Vec::new();
        let mut addressed = BTreeSet::new();
        let mut auto = false;
        // Rows read as a plain range stay one while their object still
        // covers exactly those lamps.
        let spans = object_lamp_ranges(p, &n.id);
        let total: u32 = spans.iter().map(|(_, _, c)| c).sum();
        let read_as_range = |object: &ObjectId| -> Option<PatchSource> {
            let &(start, count) = carried?.patch.as_ref()?.ranges.get(object)?;
            let &(_, s, c) = spans.iter().find(|(o, _, _)| o == object)?;
            let same = s == start && count.map_or(s + c == total, |want| want == c);
            same.then_some(PatchSource::Range(start, count))
        };
        let _ = fx;
        let (rows, flows) = patch_rows(p, n);
        auto |= flows;
        for Row {
            object,
            output,
            port,
            pixel,
            reversed,
            rotation,
        } in rows
        {
            let Some(out) = p.output(&output) else {
                continue;
            };
            let Some(start) = out.port_start(&port) else {
                continue;
            };
            let source = match read_as_range(&object) {
                Some(range) => range,
                None => {
                    let Some((path, owner)) = self.object_path(&n.id, &object) else {
                        continue;
                    };
                    addressed.insert(owner);
                    PatchSource::Path { path, range: None }
                }
            };
            let output_name = match self.labels.get(&output) {
                Some(l) => Some(l.clone()),
                None if Some(&output) == default.as_ref() => None,
                None => Some(p.node_name(&output)),
            };
            entries.push(PatchEntry {
                source,
                output: output_name,
                lamp: start + pixel,
                reversed,
                offset: rotation,
            });
        }
        let doc = PatchDoc {
            manual: !auto,
            entries,
        };
        let old = carried.and_then(|f| f.patch.as_ref());
        // A fixture that only auto-flows needs no patch file; nor does
        // a manual one with nothing on any wire.
        let needed = !doc.entries.is_empty() || (doc.manual && default.is_some());
        let patch_path = match old {
            Some(o) => o.path.clone(),
            None if needed => {
                let file = format!("{name}.patch.json");
                set_patch_ref(value, &file);
                join(path, &format!("./{file}"))
            }
            None => return Ok(addressed),
        };
        if self.force || old.is_none_or(|o| o.doc != doc) {
            self.saved.writes.push((patch_path, doc.text()));
        }
        Ok(addressed)
    }

    /// `/door`, `/sector/2`, `/circle1-r2`: what a patch row calls an
    /// object, and the product object (top-level component) it is in. A
    /// copy of a repeat's child (`panel@2`, or `panel` itself, copy 0) is
    /// the repeat's: `/<repeat>/2`.
    fn object_path(&self, fixture: &NodeId, object: &ObjectId) -> Option<(ObjectPath, ComponentId)> {
        let p = self.project;
        let c = p.component(&object.component)?;
        if let Some(parent) = p.parent_group(&c.id).and_then(|id| p.component(id))
            && let ComponentKind::Repeat { .. } = &parent.kind
        {
            // The product's repeats copy one shape: its object is a copy.
            let [k] = object.copies.as_slice() else {
                return object.copies.is_empty().then(|| {
                    (
                        ObjectPath {
                            id: self.product_id(fixture, parent),
                            instances: vec![0],
                        },
                        parent.id.clone(),
                    )
                });
            };
            return Some((
                ObjectPath {
                    id: self.product_id(fixture, parent),
                    instances: vec![*k],
                },
                parent.id.clone(),
            ));
        }
        let id = self.product_id(fixture, c);
        let id = match &c.kind {
            ComponentKind::Circle { rings, .. } if rings.len() > 1 => {
                format!("{id}-{}", object.key)
            }
            _ => id,
        };
        Some((
            ObjectPath {
                id,
                instances: Vec::new(),
            },
            c.id.clone(),
        ))
    }

    /// A component's id in its document: the carried one, else its lab id
    /// (or a slug of it).
    fn product_id(&self, fixture: &NodeId, c: &Component) -> String {
        if let Some(o) = self
            .files
            .nodes
            .get(fixture)
            .and_then(|f| f.map.as_ref())
            .and_then(|m| m.objects.get(&c.id))
        {
            return o.product_id.clone();
        }
        if valid_object_id(c.id.as_str()) {
            c.id.to_string()
        } else {
            slugify(c.id.as_str()).unwrap_or_else(|| "obj".into())
        }
    }

    /// The fixture's mapping document; `every_id` writes an id on every
    /// object, not only the ones that had one or that a patch row names.
    fn map_value(
        &self,
        n: &Node,
        addressed: &BTreeSet<ComponentId>,
        every_id: bool,
    ) -> Result<Value, FilesError> {
        let Some(fx) = n.fixture() else {
            return Ok(Value::Null);
        };
        let carried = self.files.nodes.get(&n.id).and_then(|f| f.map.as_ref());
        let mut value = carried.map(|m| m.doc.value.clone()).unwrap_or_else(|| {
            json!({"format": 1, "sample_diameter": 2.0, "canvas": [0.0, 0.0, 0.0, 0.0], "objects": []})
        });
        let path = carried.map_or("", |m| m.path.as_str());
        let Some(obj) = value.as_object_mut() else {
            return Ok(value);
        };

        // The canvas: the box, from its corner in the document.
        let min = carried.map_or(Vec2::default(), |m| m.canvas_min);
        let derived = carried.and_then(|m| m.derived_size) == Some(fx.size);
        if !derived {
            let old = obj.get("canvas").cloned();
            let at = |i: usize| old.as_ref().and_then(|c| c.get(i));
            obj.insert(
                "canvas".into(),
                Value::Array(vec![
                    num_f32(at(0), min.x),
                    num_f32(at(1), min.y),
                    num_f32(at(2), fx.size.x - min.x),
                    num_f32(at(3), fx.size.y - min.y),
                ]),
            );
        }

        // The objects, in wiring order.
        let mut flat: Vec<&Component> = Vec::new();
        flatten(&fx.components, &mut flat);
        let mut ids: BTreeSet<String> = BTreeSet::new();
        let mut objects = Vec::new();
        for c in flat {
            let pid = self.product_id(&n.id, c);
            let file = carried.and_then(|m| m.objects.get(&c.id));
            let rings = match &c.kind {
                ComponentKind::Circle { rings, .. } => rings.len().max(1),
                _ => 1,
            };
            for ring in 0..rings {
                let (id, old) = if rings > 1 {
                    let key = match &c.kind {
                        ComponentKind::Circle { rings, .. } => rings[ring].key.clone(),
                        _ => String::new(),
                    };
                    (format!("{pid}-{key}"), None)
                } else {
                    (pid.clone(), file.map(|f| &f.value))
                };
                let id = uniquify(&id, &ids);
                ids.insert(id.clone());
                let mut o = old.and_then(Value::as_object).cloned().unwrap_or_else(|| {
                    let mut m = Map::new();
                    m.insert("name".into(), c.id.as_str().into());
                    m
                });
                let write_id =
                    every_id || file.is_some_and(|f| f.had_id) || addressed.contains(&c.id);
                if write_id {
                    insert_after(&mut o, "id", id.into(), "name");
                } else {
                    o.remove("id");
                }
                let shape = shape_of(o.get("shape"), &c.kind, ring)
                    .map_err(|e| FilesError::new(path, format!("{}: {e}", c.id)))?;
                o.insert("shape".into(), shape);
                objects.push(Value::Object(o));
            }
        }
        let format = required_format(&objects).max(
            obj.get("format")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .min(5) as u32,
        );
        obj.insert("format".into(), format.into());
        obj.insert("objects".into(), Value::Array(objects));
        Ok(value)
    }

    /// Queue a file for writing if it is new, changed, or saving forces.
    fn emit(&mut self, path: &str, old: Option<&Value>, new: &Value) {
        if self.force || old.is_none_or(|o| !same(o, new)) {
            self.saved.writes.push((path.to_string(), pretty(new)));
        }
    }
}

/// Every component that becomes an object: a group gives its children.
fn flatten<'c>(list: &'c [Component], out: &mut Vec<&'c Component>) {
    for c in list {
        match &c.kind {
            ComponentKind::Group { children } => flatten(children, out),
            _ => out.push(c),
        }
    }
}

/// A node name the product accepts: letters, digits and `_`, not starting
/// with a digit (`node_name.rs`).
fn product_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 'n');
    }
    s
}

fn unique_name(base: &str, taken: &BTreeSet<String>) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

/// A port's endpoint: its name when it is one (`ws281x:local:D10`), else
/// a local WS281x pin named for it (`D0` → `ws281x:local:D0`).
fn endpoint(name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        let pin: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        format!("ws281x:local:{pin}")
    }
}

/// `child` as a ref from the module file at `module` (`./dome.json`).
fn relative(module: &str, child: &str) -> String {
    let dir = match module.rfind('/') {
        Some(i) => &module[..=i],
        None => "",
    };
    format!("./{}", child.strip_prefix(dir).unwrap_or(file_name(child)))
}

fn set_binding(value: &mut Value, slot: &str, key: &str, channel: &str) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let bindings = obj
        .entry("bindings")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(b) = bindings.as_object_mut() {
        b.insert(slot.into(), json!({ key: format!("bus:{channel}") }));
    }
}

fn set_patch_ref(value: &mut Value, file: &str) {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("patch".into(), json!({"kind": "File", "source": file}));
    }
}

/// Insert `key` right after `after` (where the product's writer puts it),
/// or replace it where it is.
fn insert_after(obj: &mut Map<String, Value>, key: &str, value: Value, after: &str) {
    if obj.contains_key(key) {
        obj.insert(key.into(), value);
        return;
    }
    let entries: Vec<(String, Value)> = std::mem::take(obj).into_iter().collect();
    let at = entries
        .iter()
        .position(|(k, _)| k == after)
        .map_or(0, |i| i + 1);
    let mut entries = entries;
    entries.insert(at, (key.to_string(), value));
    obj.extend(entries);
}

/// One patch row: an object at its address.
struct Row {
    object: ObjectId,
    output: NodeId,
    port: String,
    pixel: u32,
    reversed: bool,
    rotation: u32,
}

/// A fixture's patch rows, and whether its file says `flow: auto`.
///
/// Placements the rows can say — the fixture's own wire (auto, or unset
/// for manual) and objects at an address of their own, reversed or turned
/// — are written as they are. Anything else (a component placed, an
/// object reversed or turned while it follows, an object taken off the
/// wire under auto) can't be said in today's rows (plan D14: patch
/// format 4), so the fixture is written expanded: every object that lands
/// at the address its placements give it, and auto kept only while every
/// object lands (else auto would put back what is meant to be dark).
fn patch_rows(p: &Project, n: &Node) -> (Vec<Row>, bool) {
    let Some(fx) = n.fixture() else {
        return (Vec::new(), false);
    };
    let auto = p.auto_output(&n.id).is_some_and(|o| p.output(&o).is_some());
    let sayable = fx.places.iter().all(|pl| match &pl.on {
        PatchTarget::Fixture => {
            pl.direction.is_none() && matches!(pl.wire, None | Some(Wire::Auto { .. }))
        }
        PatchTarget::Object(_) => matches!(pl.wire, Some(Wire::At { .. })),
        PatchTarget::Component(_) => false,
    });
    if sayable {
        let rows = fx
            .places
            .iter()
            .filter_map(|pl| {
                let (
                    PatchTarget::Object(object),
                    Some(Wire::At {
                        output,
                        port,
                        pixel,
                    }),
                ) = (&pl.on, &pl.wire)
                else {
                    return None;
                };
                let n = p.object(object).map_or(1, |o| o.lamps.len() as u32).max(1);
                Some(Row {
                    object: object.clone(),
                    output: output.clone(),
                    port: port.clone(),
                    pixel: *pixel,
                    reversed: pl.direction.is_some_and(|d| d.is_reversed()),
                    rotation: pl.turn.unwrap_or(0) % n,
                })
            })
            .collect();
        return (rows, auto);
    }
    let patch = p.effective_patch();
    let mut rows = Vec::new();
    for r in patch.runs.iter().filter(|r| r.fixture == n.id) {
        let Some(out) = p.output(&r.output) else {
            continue;
        };
        let Some((i, pixel)) = out.port_at(r.wire) else {
            continue;
        };
        rows.push(Row {
            object: r.object.clone(),
            output: r.output.clone(),
            port: out.ports[i].key.clone(),
            pixel,
            reversed: r.reversed,
            rotation: r.rotation,
        });
    }
    let all_land = p
        .objects_in(&n.id)
        .iter()
        .filter(|o| !o.lamps.is_empty())
        .all(|o| rows.iter().any(|r| r.object == o.id));
    (rows, auto && all_land)
}
