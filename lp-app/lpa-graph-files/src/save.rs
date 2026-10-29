//! Saving: the Graph's project back into the product's files.
//!
//! Only files the Graph owns are written — module node maps, fixture and
//! output nodes, and each fixture's mapping — and only when what they say
//! has changed. A file nothing changed is not written, so it stays byte for
//! byte as it was; nodes the Graph doesn't edit (a shader, a clock) are
//! never written at all.
//!
//! - **A fixture's mapping** is written by `lpc-mapping`'s own format-6
//!   writer ([`map2d6::write`]: one canonical form, honest counters,
//!   overrides sorted), from the fixture's `Mapping` — its parts, ids,
//!   counters, ring keys, starts, flows and overrides, all of it (mapping
//!   rebuild M4 P05). It is written when the mapping differs from the one
//!   the file holds; the canvas follows the fixture's box. Nothing here
//!   shapes a mapping's JSON by hand.
//! - **A node file** (module, fixture, output) starts from the JSON it was
//!   read from, so fields the Graph knows nothing about are kept, and is
//!   written when what it says changed (numbers compared as f32, the
//!   product's reading).
//! - **A render source** (a shader) the Graph opened is never written: it
//!   shows its GLSL and never edits it. One the Graph made is written as a
//!   shader node with a stand-in GLSL file (`stand_in_shader.rs`).
//! - **Node names**: a name the product would refuse (`tail rig`) is
//!   written as one it accepts (`tail_rig`).
//!
//! One mapping file holds a fixture's parts and its patching (G1, Q14), so
//! no edit to a mapping removes a file. A deleted node's files (its own, its
//! mapping, a shader's GLSL: [`crate::NodeFile::own_files`]) are reported in
//! [`Saved::removed`], for the caller to delete (Studio does, ruling DD36).

use std::collections::{BTreeMap, BTreeSet};

use lab_mapping_model::{ColorOrder, Fixture, Node, NodeId, NodeKind, Project, Vec2};
use lpc_mapping::map2d6;
use lpc_mapping::part::Mapping;
use serde_json::{Map, Value, json};

use crate::channels::{fixture_reads, output_reads};
use crate::error::FilesError;
use crate::json::{insert_sorted, pretty, same, set};
use crate::product_project::{MapFile, ProductProject};
use crate::source::{file_name, join};

/// What saving did: the files to write (path, text), and the files no
/// longer referenced (a deleted node's), for the caller to delete. Both by
/// path, sorted.
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

    /// Every file the Graph owns, changed or not: what a round trip reads
    /// back to prove the writer says everything (the landing oracle).
    pub fn save_all(&self, project: &Project) -> Result<Saved, FilesError> {
        Writer::new(self, project, true).run()
    }

    /// The nodes `project` holds that these files don't (the Graph made
    /// them), each with the file saving writes it to.
    pub fn new_nodes(&self, project: &Project) -> Vec<(NodeId, String)> {
        let mut w = Writer::new(self, project, false);
        w.place("module.json", project.nodes());
        w.places
            .into_iter()
            .filter(|(id, _)| !self.nodes.contains_key(id))
            .map(|(id, (_, path))| (id, path))
            .collect()
    }

    /// Files read back after a save (`self`) key a node by its name in its
    /// module; one the Graph made has the id the Graph minted (`output3`).
    /// Key each of `new` ([`ProductProject::new_nodes`] before that save) by
    /// the Graph's id again, found by the file it was written to, so the
    /// next save knows it as written. Without it the next save takes the
    /// Graph's node for another one: it writes the node's file again and
    /// reports the same file gone (which Studio then deletes).
    pub fn adopt_new_nodes(&mut self, new: &[(NodeId, String)]) {
        for (id, path) in new {
            if self.nodes.contains_key(id) {
                continue;
            }
            let read_as = self
                .nodes
                .iter()
                .find(|(k, f)| &f.path == path && !new.iter().any(|(n, _)| n == *k))
                .map(|(k, _)| k.clone());
            if let Some(k) = read_as
                && let Some(file) = self.nodes.remove(&k)
            {
                self.nodes.insert(id.clone(), file);
            }
        }
    }
}

struct Writer<'a> {
    files: &'a ProductProject,
    project: &'a Project,
    force: bool,
    saved: Saved,
    /// Every node's name in its module and its file.
    places: BTreeMap<NodeId, (String, String)>,
}

impl<'a> Writer<'a> {
    fn new(files: &'a ProductProject, project: &'a Project, force: bool) -> Self {
        Writer {
            files,
            project,
            force,
            saved: Saved::default(),
            places: BTreeMap::new(),
        }
    }

    fn run(mut self) -> Result<Saved, FilesError> {
        self.place("module.json", self.project.nodes());
        let root = self.module_value(&self.files.root, "module.json", self.project.nodes());
        self.emit("module.json", Some(&self.files.root), &root);
        self.nodes(self.project.nodes())?;
        for (id, f) in &self.files.nodes {
            if self.project.node(id).is_none() {
                self.saved.removed.extend(f.own_files());
            }
        }
        self.saved.writes.sort();
        self.saved.removed.sort();
        Ok(self.saved)
    }

    /// Name and file for every node: the carried ones, and fresh ones for
    /// what the Graph made.
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
                NodeKind::Fixture(fx) => self.fixture(n, fx, &path)?,
                NodeKind::Output(_) => {
                    let value = output_value(n, old);
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

    /// A fixture's node file and its mapping. A fixture the Graph can't
    /// hold is never written: its files stay exactly as they were.
    fn fixture(&mut self, n: &Node, fx: &Fixture, path: &str) -> Result<(), FilesError> {
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
                    "output": {"target": format!("bus:{}", self.new_fixture_writes(&n.id))}
                },
                "sampling": "direct",
                "mapping": {"kind": "Map2d", "source": format!("{name}.map2d.json")},
            })
        });
        if let Some(obj) = value.as_object_mut() {
            let had = obj.contains_key("color_order");
            if had || fx.color_order != ColorOrder::default() {
                obj.insert("color_order".into(), fx.color_order.key().into());
            }
        }
        if fixture_reads(&value) != fx.reads.as_str() {
            set_binding(&mut value, "input", "source", fx.reads.as_str());
        }
        let old = carried.map(|f| &f.value);
        self.emit(path, old, &value);

        let opened = carried.and_then(|f| f.map.as_ref());
        let map_path = match opened {
            Some(m) => m.path.clone(),
            None => join(path, &format!("./{name}.map2d.json")),
        };
        let mapping = mapping_to_write(fx, opened);
        if !self.force && opened.is_some_and(|m| m.mapping == mapping) {
            return Ok(());
        }
        let text = map2d6::write(&mapping)
            .map_err(|e| FilesError::new(&map_path, format!("{}: {e}", n.name)))?;
        if self.force || opened.is_none_or(|m| m.text != text) {
            self.saved.writes.push((map_path, text));
        }
        Ok(())
    }

    /// The channel a fixture the Graph made writes: the one the first
    /// output its mapping names reads, else `control.out`.
    fn new_fixture_writes(&self, fixture: &NodeId) -> String {
        self.project
            .control_outputs(fixture)
            .first()
            .and_then(|o| self.project.output(o))
            .map_or_else(
                || "control.out".to_string(),
                |o| o.reads.as_str().to_string(),
            )
    }

    /// Queue a file for writing if it is new, changed, or saving forces.
    fn emit(&mut self, path: &str, old: Option<&Value>, new: &Value) {
        if self.force || old.is_none_or(|o| !same(o, new)) {
            self.saved.writes.push((path.to_string(), pretty(new)));
        }
    }
}

/// The fixture's mapping as its file says it: its `Mapping`, with the
/// canvas its box gives. A box the Graph didn't resize keeps the canvas
/// (or no canvas) it opened with; a resized one keeps its corner and
/// reaches its new size.
fn mapping_to_write(fx: &Fixture, opened: Option<&MapFile>) -> Mapping {
    let mut mapping = fx.mapping.clone();
    let kept = match fx.mapping.canvas {
        Some([x, y, w, h]) => Vec2::new(f64::from(x + w), f64::from(y + h)) == fx.size,
        None => opened.and_then(|m| m.derived_size) == Some(fx.size),
    };
    if !kept {
        let [x, y] = fx.mapping.canvas.map_or([0.0, 0.0], |c| [c[0], c[1]]);
        mapping.canvas = Some([x, y, fx.size.x as f32 - x, fx.size.y as f32 - y]);
    }
    mapping
}

/// An output's node file: its ports, its name, the channel it reads.
fn output_value(n: &Node, old: Option<&Value>) -> Value {
    let Some(o) = n.output() else {
        return Value::Null;
    };
    let mut value = old.cloned().unwrap_or_else(|| json!({"kind": "Output"}));
    let Some(obj) = value.as_object_mut() else {
        return value;
    };
    let had_name = obj.contains_key("name");
    match &o.label {
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
    let channel = o.reads.as_str();
    if old.is_none() {
        obj.insert(
            "bindings".into(),
            json!({"input": {"source": format!("bus:{channel}")}}),
        );
    } else if old.is_some_and(|v| output_reads(v) != channel) {
        set_binding(&mut value, "input", "source", channel);
    }
    value
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

/// Insert `key` right after `after` (where the product's writer puts it),
/// or replace it where it is.
fn insert_after(obj: &mut Map<String, Value>, key: &str, value: Value, after: &str) {
    if obj.contains_key(key) {
        obj.insert(key.into(), value);
        return;
    }
    let mut entries: Vec<(String, Value)> = std::mem::take(obj).into_iter().collect();
    let at = entries
        .iter()
        .position(|(k, _)| k == after)
        .map_or(0, |i| i + 1);
    entries.insert(at, (key.to_string(), value));
    obj.extend(entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_the_product_refuses_is_written_as_one_it_accepts() {
        assert_eq!(product_name("tail rig"), "tail_rig");
        assert_eq!(product_name("3d"), "n3d");
        let taken: BTreeSet<String> = ["out".to_string()].into();
        assert_eq!(unique_name("out", &taken), "out_2");
    }

    #[test]
    fn a_port_named_by_its_pin_is_a_local_ws281x_endpoint() {
        assert_eq!(endpoint("D0"), "ws281x:local:D0");
        assert_eq!(endpoint("ws281x:local:D10"), "ws281x:local:D10");
        assert_eq!(relative("dome/module.json", "dome/x.json"), "./x.json");
    }
}
