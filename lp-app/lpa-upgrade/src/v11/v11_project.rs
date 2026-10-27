//! A format-11 project's fixtures and outputs, in load order, and the bus
//! routing between them — read from the files alone.
//!
//! Frozen from the engine at origin/main b9f862fb2 (the project loader and
//! the scoped bus, `docs/design/modules.md` R1–R7):
//!
//! - **Load order** is `NodeId` order: the loader sorts every node use by
//!   its depth, then by its use path (module node keys, compared as
//!   strings), and numbers them in that order
//!   (`ProjectLoader::ensure_runtime_spine`). "The first output" and "the
//!   lowest `NodeId`" mean the first in this order; `out_10` comes before
//!   `out_2`.
//! - **Scopes** are modules. A fixture writes its bus channel into its own
//!   module's scope (R4). A module's `exports` publish an inner channel
//!   into the containing scope: the key is the outer channel, the value the
//!   inner `bus:` channel (R7).
//! - **An output reads** the nearest enclosing scope, starting at its own,
//!   that has a writer for its channel (R5, writer-shadowing).
//! - **Provider order** within a scope is by the binding's owner: a
//!   fixture's own place in load order, or the exporting module's, its
//!   inner providers expanded in place in the same order.
//! - **The default output** of a channel is the first output, in load
//!   order, reading that channel in the same scope.

use std::cmp::Ordering;

use super::v11_files::{read_json, refused, resolve_path, scalar};
use crate::json::JsonNode;
use crate::project_files::ProjectFiles;
use crate::upgrade_error::UpgradeError;

/// The bus channel a fixture writes and an output reads when the def does
/// not say (`FixtureState::output`, `OutputDef::input`).
const DEFAULT_CONTROL_CHANNEL: &str = "control.out";

/// The project's fixtures and outputs, and the scopes between them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct V11Project {
    /// Every fixture, in load order.
    pub(crate) fixtures: Vec<V11Fixture>,
    /// Every output, in load order.
    pub(crate) outputs: Vec<V11Output>,
    scopes: Vec<V11Scope>,
}

/// One fixture node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct V11Fixture {
    /// The fixture's definition file, from the project root (`/dome/dome.json`).
    pub(crate) node: String,
    /// The bus channel it writes.
    pub(crate) channel: String,
    pub(crate) mapping: V11Mapping,
    /// The patch document's project path, when the fixture names one.
    pub(crate) patch_file: Option<String>,
    scope: usize,
    order: LoadOrder,
}

/// What a fixture's `mapping` names.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum V11Mapping {
    /// No mapping: no lamps.
    Unset,
    /// A `*.map2d.json` document at this project path.
    Map2d { file: String },
    /// Hand-placed lamps inline in the fixture def: each path's
    /// `(key, first_channel, lamps)`, in key order.
    PathPoints { paths: Vec<(u32, u32, u32)> },
}

/// One output node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct V11Output {
    /// The output's definition file, from the project root (`/out_a.json`).
    pub(crate) node: String,
    /// The authored name patch rows address it by, if any.
    pub(crate) name: Option<String>,
    /// The bus channel it reads.
    pub(crate) channel: String,
    /// `(port key, count in lamps)`, in key order (numeric).
    pub(crate) ports: Vec<(u32, Option<u32>)>,
    scope: usize,
    order: LoadOrder,
}

/// One module's scope.
#[derive(Debug, Clone, PartialEq)]
struct V11Scope {
    parent: Option<usize>,
    /// The module's file, for refusals.
    module: String,
    /// The channels child modules publish into this scope: `(outer channel,
    /// child scope, inner channel, the module node's load order)`.
    exports: Vec<(String, usize, String, LoadOrder)>,
}

/// A node's place in load order: its use path from the root, compared by
/// depth first, then key by key.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadOrder(Vec<String>);

impl Ord for LoadOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for LoadOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl V11Project {
    /// Walk the module tree from `/module.json`.
    pub(crate) fn read(files: &ProjectFiles) -> Result<Self, UpgradeError> {
        let mut project = Self {
            fixtures: Vec::new(),
            outputs: Vec::new(),
            scopes: Vec::new(),
        };
        let root = read_json(files, "/module.json", "project.json")?;
        project.read_module(files, "/module.json", &root, None, &[])?;
        project.fixtures.sort_by(|a, b| a.order.cmp(&b.order));
        project.outputs.sort_by(|a, b| a.order.cmp(&b.order));
        project.refuse_duplicate_output_names()?;
        Ok(project)
    }

    /// The fixtures `output` reads, in provider order.
    pub(crate) fn providers(&self, output: &V11Output) -> Result<Vec<usize>, UpgradeError> {
        let mut scope = Some(output.scope);
        while let Some(current) = scope {
            if self.has_writer(current, &output.channel) {
                return self.providers_in(current, &output.channel);
            }
            scope = self.scopes[current].parent;
        }
        Ok(Vec::new())
    }

    /// Whether `output` is the default output of its channel: the first in
    /// load order reading that channel in its scope.
    pub(crate) fn is_default(&self, output: &V11Output) -> bool {
        self.outputs
            .iter()
            .find(|other| other.scope == output.scope && other.channel == output.channel)
            .is_some_and(|first| first.node == output.node)
    }

    /// Every authored output name in the project — what the engine's
    /// dangling-name check compares a patch row's output against.
    pub(crate) fn output_names(&self) -> Vec<&str> {
        self.outputs
            .iter()
            .filter_map(|output| output.name.as_deref())
            .collect()
    }

    fn has_writer(&self, scope: usize, channel: &str) -> bool {
        self.fixtures
            .iter()
            .any(|fixture| fixture.scope == scope && fixture.channel == channel)
            || self.scopes[scope]
                .exports
                .iter()
                .any(|(outer, ..)| outer == channel)
    }

    /// The fixtures writing `channel` in `scope`, exports expanded in place,
    /// ordered by the binding's owner.
    fn providers_in(&self, scope: usize, channel: &str) -> Result<Vec<usize>, UpgradeError> {
        enum Owner<'a> {
            Fixture(usize),
            Export(usize, &'a str),
        }
        let mut owners: Vec<(&LoadOrder, Owner<'_>)> = Vec::new();
        for (index, fixture) in self.fixtures.iter().enumerate() {
            if fixture.scope == scope && fixture.channel == channel {
                owners.push((&fixture.order, Owner::Fixture(index)));
            }
        }
        for (outer, child, inner, order) in &self.scopes[scope].exports {
            if outer == channel {
                owners.push((order, Owner::Export(*child, inner)));
            }
        }
        owners.sort_by(|a, b| a.0.cmp(b.0));
        let mut providers = Vec::new();
        for (_, owner) in owners {
            match owner {
                Owner::Fixture(index) => providers.push(index),
                Owner::Export(child, inner) => {
                    if !self.has_writer(child, inner) {
                        return Err(refused(
                            &self.scopes[child].module,
                            &format!(
                                "exports {inner:?}, which nothing inside it writes; the frozen \
                                 landing does not model an export read from outside its module"
                            ),
                        ));
                    }
                    providers.extend(self.providers_in(child, inner)?);
                }
            }
        }
        Ok(providers)
    }

    fn read_module(
        &mut self,
        files: &ProjectFiles,
        path: &str,
        module: &JsonNode,
        parent: Option<usize>,
        use_path: &[String],
    ) -> Result<usize, UpgradeError> {
        let scope = self.scopes.len();
        self.scopes.push(V11Scope {
            parent,
            module: path.to_string(),
            exports: Vec::new(),
        });
        let Some(nodes) = module.get("nodes") else {
            return Ok(scope);
        };
        let nodes = nodes
            .object()
            .ok_or_else(|| refused(path, "\"nodes\" is not an object"))?;
        for (key, entry) in nodes {
            let reference = entry.get("ref").and_then(JsonNode::as_str).ok_or_else(|| {
                refused(
                    path,
                    &format!("node {key:?} is not a {{\"ref\": …}}; inline nodes are not read"),
                )
            })?;
            let child_path = resolve_path(path, &reference)?;
            let child = read_json(files, &child_path, path)?;
            let mut child_use = use_path.to_vec();
            child_use.push(key.clone());
            let order = LoadOrder(child_use.clone());
            match child.get("kind").and_then(JsonNode::as_str).as_deref() {
                Some("Module") => {
                    let child_scope =
                        self.read_module(files, &child_path, &child, Some(scope), &child_use)?;
                    for (outer, inner) in module_exports(&child_path, &child)? {
                        self.scopes[scope]
                            .exports
                            .push((outer, child_scope, inner, order.clone()));
                    }
                }
                Some("Fixture") => {
                    let fixture = read_fixture(&child_path, &child, scope, order)?;
                    self.fixtures.push(fixture);
                }
                Some("Output") => {
                    let output = read_output(&child_path, &child, scope, order)?;
                    self.outputs.push(output);
                }
                Some("Playlist") => refuse_lamps_inside(files, &child_path, &child)?,
                _ => {}
            }
        }
        Ok(scope)
    }

    fn refuse_duplicate_output_names(&self) -> Result<(), UpgradeError> {
        for (index, output) in self.outputs.iter().enumerate() {
            let Some(name) = &output.name else { continue };
            if let Some(other) = self.outputs[..index]
                .iter()
                .find(|other| other.name.as_ref() == Some(name))
            {
                return Err(refused(
                    &output.node,
                    &format!(
                        "is named {name:?}, like {}; the engine routes rows naming it to both",
                        other.node
                    ),
                ));
            }
        }
        Ok(())
    }
}

/// `(outer channel, inner channel)` for each of a module's exports.
fn module_exports(path: &str, module: &JsonNode) -> Result<Vec<(String, String)>, UpgradeError> {
    let Some(exports) = module.get("exports") else {
        return Ok(Vec::new());
    };
    let exports = exports
        .object()
        .ok_or_else(|| refused(path, "\"exports\" is not an object"))?;
    exports
        .iter()
        .map(|(outer, inner)| {
            let inner = inner
                .as_str()
                .and_then(|text| text.strip_prefix("bus:").map(String::from))
                .ok_or_else(|| refused(path, &format!("export {outer:?} is not a bus: channel")))?;
            Ok((outer.clone(), inner))
        })
        .collect()
}

fn read_fixture(
    path: &str,
    def: &JsonNode,
    scope: usize,
    order: LoadOrder,
) -> Result<V11Fixture, UpgradeError> {
    let channel = bus_binding(path, def, "output", "target")?;
    let mapping = match def.get("mapping") {
        None => V11Mapping::Unset,
        Some(mapping) => match mapping.get("kind").and_then(JsonNode::as_str).as_deref() {
            Some("Unset") => V11Mapping::Unset,
            Some("Map2d") => V11Mapping::Map2d {
                file: resolve_path(path, &asset_source(path, mapping, "mapping")?)?,
            },
            Some("PathPoints") => V11Mapping::PathPoints {
                paths: path_points(path, mapping)?,
            },
            other => {
                return Err(refused(
                    path,
                    &format!("its mapping kind {other:?} is not one format 11 knows"),
                ));
            }
        },
    };
    let patch_file = match def.get("patch") {
        None => None,
        Some(patch) => match patch.get("kind").and_then(JsonNode::as_str).as_deref() {
            Some("Unset") => None,
            Some("File") => Some(resolve_path(path, &asset_source(path, patch, "patch")?)?),
            other => {
                return Err(refused(
                    path,
                    &format!("its patch kind {other:?} is not one format 11 knows"),
                ));
            }
        },
    };
    Ok(V11Fixture {
        node: path.to_string(),
        channel,
        mapping,
        patch_file,
        scope,
        order,
    })
}

fn read_output(
    path: &str,
    def: &JsonNode,
    scope: usize,
    order: LoadOrder,
) -> Result<V11Output, UpgradeError> {
    let channel = bus_binding(path, def, "input", "source")?;
    let name = match def.get("name") {
        None => None,
        Some(name) => Some(
            name.as_str()
                .ok_or_else(|| refused(path, "its \"name\" is not a string"))?,
        ),
    };
    let mut ports = Vec::new();
    if let Some(entries) = def.get("ports") {
        let entries = entries
            .object()
            .ok_or_else(|| refused(path, "\"ports\" is not an object"))?;
        for (key, port) in entries {
            let key: u32 = key
                .parse()
                .map_err(|_| refused(path, &format!("port key {key:?} is not a number")))?;
            let count = match port.get("count") {
                None => None,
                Some(count) => Some(
                    scalar::<u32>(count)
                        .ok_or_else(|| refused(path, &format!("port {key}'s count")))?,
                ),
            };
            ports.push((key, count));
        }
    }
    ports.sort_by_key(|(key, _)| *key);
    Ok(V11Output {
        node: path.to_string(),
        name,
        channel,
        ports,
        scope,
        order,
    })
}

/// A node's bus binding `slot` (`bindings.<slot>.<field>`), or the default
/// control channel when the def leaves it to the slot's declared default.
fn bus_binding(
    path: &str,
    def: &JsonNode,
    slot: &str,
    field: &str,
) -> Result<String, UpgradeError> {
    let Some(binding) = def.get("bindings").and_then(|bindings| bindings.get(slot)) else {
        return Ok(String::from(DEFAULT_CONTROL_CHANNEL));
    };
    let fields = binding
        .object()
        .ok_or_else(|| refused(path, &format!("binding {slot:?} is not an object")))?;
    if fields.len() != 1 || fields[0].0 != field {
        return Err(refused(
            path,
            &format!(
                "binding {slot:?} is not a plain {{\"{field}\": \"bus:…\"}}; the frozen landing \
                 reads no other binding"
            ),
        ));
    }
    fields[0]
        .1
        .as_str()
        .and_then(|text| text.strip_prefix("bus:").map(String::from))
        .ok_or_else(|| refused(path, &format!("binding {slot:?} is not a bus: channel")))
}

fn asset_source(path: &str, config: &JsonNode, what: &str) -> Result<String, UpgradeError> {
    config
        .get("source")
        .and_then(JsonNode::as_str)
        .ok_or_else(|| refused(path, &format!("its {what} names no source file")))
}

/// `(key, first_channel, lamps)` of every `PointList` path, in key order.
fn path_points(path: &str, mapping: &JsonNode) -> Result<Vec<(u32, u32, u32)>, UpgradeError> {
    let mut out = Vec::new();
    let Some(paths) = mapping.get("paths") else {
        return Ok(out);
    };
    let paths = paths
        .object()
        .ok_or_else(|| refused(path, "its mapping's \"paths\" is not an object"))?;
    for (key, spec) in paths {
        let key: u32 = key
            .parse()
            .map_err(|_| refused(path, &format!("path key {key:?} is not a number")))?;
        if spec.get("kind").and_then(JsonNode::as_str).as_deref() != Some("PointList") {
            return Err(refused(path, &format!("path {key} is not a PointList")));
        }
        let first_channel = spec
            .get("first_channel")
            .and_then(scalar::<u32>)
            .ok_or_else(|| refused(path, &format!("path {key} has no first_channel")))?;
        let lamps = match spec.get("points") {
            None => 0,
            Some(points) => points
                .object()
                .ok_or_else(|| refused(path, &format!("path {key}'s points are not an object")))?
                .len() as u32,
        };
        out.push((key, first_channel, lamps));
    }
    out.sort_by_key(|(key, ..)| *key);
    Ok(out)
}

/// A playlist's entries are dormant alternatives; a fixture or output inside
/// one is outside what the frozen landing models.
fn refuse_lamps_inside(
    files: &ProjectFiles,
    path: &str,
    def: &JsonNode,
) -> Result<(), UpgradeError> {
    let mut refs: Vec<String> = Vec::new();
    if let Some(entries) = def.get("entries").and_then(JsonNode::object) {
        for (_, entry) in entries {
            if let Some(reference) = entry
                .get("node")
                .and_then(|node| node.get("ref"))
                .and_then(JsonNode::as_str)
            {
                refs.push(reference);
            }
        }
    }
    if let Some(nodes) = def.get("nodes").and_then(JsonNode::object) {
        for (_, entry) in nodes {
            if let Some(reference) = entry.get("ref").and_then(JsonNode::as_str) {
                refs.push(reference);
            }
        }
    }
    for reference in refs {
        let child_path = resolve_path(path, &reference)?;
        let child = read_json(files, &child_path, path)?;
        match child.get("kind").and_then(JsonNode::as_str).as_deref() {
            Some("Fixture") | Some("Output") => {
                return Err(refused(
                    &child_path,
                    "sits inside a playlist entry; the frozen landing reads fixtures and outputs \
                     in modules only",
                ));
            }
            Some("Module") | Some("Playlist") => refuse_lamps_inside(files, &child_path, &child)?,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load order is depth first, then key by key as strings: `out_10`
    /// before `out_2`, a module's child after every root node.
    #[test]
    fn load_order_is_depth_then_keys_as_strings() {
        let order = |keys: &[&str]| LoadOrder(keys.iter().map(|key| key.to_string()).collect());
        assert!(order(&["out_10"]) < order(&["out_2"]));
        assert!(order(&["z"]) < order(&["a", "f"]));
        assert!(order(&["a", "y"]) < order(&["b", "x"]));
    }

    /// A module export's key is the channel it publishes outside; its value
    /// the inner channel it mirrors.
    #[test]
    fn an_export_publishes_its_key_from_its_inner_channel() {
        let files = project(&[
            (
                "module.json",
                r#"{"kind":"Module","nodes":{"m":{"ref":"./m/module.json"},"o":{"ref":"./o.json"}}}"#,
            ),
            (
                "m/module.json",
                r#"{"kind":"Module","nodes":{"f":{"ref":"./f.json"}},"exports":{"control.outer":"bus:control.inner"}}"#,
            ),
            (
                "m/f.json",
                r#"{"kind":"Fixture","bindings":{"output":{"target":"bus:control.inner"}}}"#,
            ),
            (
                "o.json",
                r#"{"kind":"Output","bindings":{"input":{"source":"bus:control.outer"}}}"#,
            ),
        ]);
        let project = V11Project::read(&files).unwrap();
        assert_eq!(project.providers(&project.outputs[0]).unwrap(), vec![0]);
    }

    /// Provider order follows the binding's owner: a nested fixture exported
    /// by module `a` comes before the root's fixture `b`.
    #[test]
    fn an_exported_fixture_takes_its_modules_place_in_provider_order() {
        let files = project(&[
            (
                "module.json",
                r#"{"kind":"Module","nodes":{"a":{"ref":"./a/module.json"},"b":{"ref":"./b.json"},"o":{"ref":"./o.json"}}}"#,
            ),
            (
                "a/module.json",
                r#"{"kind":"Module","nodes":{"f":{"ref":"./f.json"}},"exports":{"control.out":"bus:control.out"}}"#,
            ),
            ("a/f.json", r#"{"kind":"Fixture"}"#),
            ("b.json", r#"{"kind":"Fixture"}"#),
            ("o.json", r#"{"kind":"Output"}"#),
        ]);
        let project = V11Project::read(&files).unwrap();
        let names: Vec<&str> = project
            .providers(&project.outputs[0])
            .unwrap()
            .into_iter()
            .map(|index| project.fixtures[index].node.as_str())
            .collect();
        assert_eq!(names, vec!["/a/f.json", "/b.json"]);
    }

    /// An output in a submodule with no writer of its own reads outward.
    #[test]
    fn an_output_reads_the_nearest_scope_with_a_writer() {
        let files = project(&[
            (
                "module.json",
                r#"{"kind":"Module","nodes":{"f":{"ref":"./f.json"},"m":{"ref":"./m/module.json"}}}"#,
            ),
            ("f.json", r#"{"kind":"Fixture"}"#),
            (
                "m/module.json",
                r#"{"kind":"Module","nodes":{"o":{"ref":"./o.json"}}}"#,
            ),
            ("m/o.json", r#"{"kind":"Output"}"#),
        ]);
        let project = V11Project::read(&files).unwrap();
        assert_eq!(project.providers(&project.outputs[0]).unwrap(), vec![0]);
    }

    fn project(files: &[(&str, &str)]) -> ProjectFiles {
        files
            .iter()
            .map(|(path, text)| (path.to_string(), text.as_bytes().to_vec()))
            .collect()
    }
}
