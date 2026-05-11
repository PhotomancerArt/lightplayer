//! Load authored `project.toml` node-artifact trees into [`super::Engine`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::nodes::project::project_def::ProjectDef;
use lpc_model::{ArtifactLocator, NodeInvocation, NodeKind};
use lpc_model::{
    BindingDefs, BindingEndpoint, ChannelName, Kind, LpValue, NodeDef, NodeId, NodeName, Revision,
    SlotPath,
};
use lpc_source::ArtifactReadRoot;
use lpc_wire::{WireChildKind, WireSlotIndex};
use lpfs::lp_path::{LpPath, LpPathBuf};

use crate::artifact::ArtifactLocation;
use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::node::{NodeDefHandle, TreeError};
use crate::nodes::{CorePlaceholderNode, FixtureNode, OutputNode, ShaderNode, TextureNode};

use super::{Engine, EngineServices};

/// Errors loading an authored project into [`Engine`].
#[derive(Debug)]
pub enum ProjectLoadError {
    Io { path: String, details: String },
    ProjectToml { file: String, error: String },
    UnknownKind { path: String, suffix: String },
    InvalidSourcePath { path: String, reason: String },
    TomlParse { path: String, error: String },
    InvalidNodeName { path: String, reason: String },
    Tree(TreeError),
}

impl core::fmt::Display for ProjectLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, details } => write!(f, "io error at {path}: {details}"),
            Self::ProjectToml { file, error } => write!(f, "parse {file}: {error}"),
            Self::UnknownKind { path, suffix } => write!(f, "{path}: unknown node kind `{suffix}`"),
            Self::InvalidSourcePath { path, reason } => {
                write!(f, "source path {path}: {reason}")
            }
            Self::TomlParse { path, error } => write!(f, "{path}: TOML parse failed: {error}"),
            Self::InvalidNodeName { path, reason } => write!(f, "{path}: invalid name: {reason}"),
            Self::Tree(e) => write!(f, "tree: {e}"),
        }
    }
}

impl core::error::Error for ProjectLoadError {}

struct LoadedNode {
    name: NodeName,
    artifact_path: LpPathBuf,
    id: NodeId,
    config: NodeDef,
}

/// Loads the authored project artifact tree into a core engine-backed runtime.
pub struct ProjectLoader;

impl ProjectLoader {
    pub fn load_from_root<R>(root: &R, services: EngineServices) -> Result<Engine, ProjectLoadError>
    where
        R: ArtifactReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        Self::load_project_artifact(root, services, ArtifactLocator::path("/project.toml"))
    }

    pub fn load_project_artifact<R>(
        root: &R,
        services: EngineServices,
        project_locator: ArtifactLocator,
    ) -> Result<Engine, ProjectLoadError>
    where
        R: ArtifactReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        let project_path = resolve_project_locator(&project_locator)?;
        let project_def = load_project_def(root, &project_path)?;

        let project_root = services.project_root().clone();
        let mut runtime = Engine::with_services(project_root.clone(), services);
        let frame = Revision::new(1);
        let root_id = runtime.tree().root();
        let project_artifact = runtime
            .artifacts_mut()
            .acquire_location(ArtifactLocation::file(project_path.clone()), frame);
        runtime
            .artifacts_mut()
            .load_with(&project_artifact, frame, |_location| {
                Ok(NodeDef::Project(project_def.clone()))
            })
            .map_err(|e| ProjectLoadError::InvalidSourcePath {
                path: project_path.as_str().to_string(),
                reason: format!("load project artifact payload: {e:?}"),
            })?;
        let project_invocation = NodeInvocation::new(project_locator);

        {
            let root_entry = runtime
                .tree_mut()
                .get_mut(root_id)
                .ok_or(ProjectLoadError::Tree(TreeError::UnknownNode(root_id)))?;
            root_entry.config = project_invocation;
            root_entry.def_handle = NodeDefHandle::artifact_root(project_artifact);
        }
        runtime
            .attach_runtime_node(
                root_id,
                Box::new(CorePlaceholderNode::new_leaf(NodeKind::Project)),
                frame,
            )
            .map_err(|e| ProjectLoadError::InvalidSourcePath {
                path: project_path.as_str().to_string(),
                reason: format!("attach project runtime: {e}"),
            })?;

        let mut loaded_nodes = Vec::new();
        for (name, invocation) in project_def.nodes.entries {
            let node_name =
                NodeName::parse(&name).map_err(|e| ProjectLoadError::InvalidNodeName {
                    path: project_path.as_str().to_string(),
                    reason: format!("{e}"),
                })?;
            let artifact_locator =
                invocation
                    .artifact_locator()
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: project_path.as_str().to_string(),
                        reason: format!(
                            "invalid artifact locator `{}`: {e}",
                            invocation.artifact.value()
                        ),
                    })?;
            let artifact_path = resolve_child_artifact_locator(&project_path, &artifact_locator)?;
            let config = load_node_def(root, artifact_path.as_path())?;
            let artifact_id = runtime
                .artifacts_mut()
                .acquire_location(ArtifactLocation::file(artifact_path.clone()), frame);
            runtime
                .artifacts_mut()
                .load_with(&artifact_id, frame, |_location| Ok(config.clone()))
                .map_err(|e| ProjectLoadError::InvalidSourcePath {
                    path: artifact_path.as_str().to_string(),
                    reason: format!("load node artifact payload: {e:?}"),
                })?;
            let ty = node_kind_name(&config, artifact_path.as_path())?;
            let leaf_id = runtime
                .tree_mut()
                .add_child(
                    root_id,
                    node_name.clone(),
                    ty,
                    WireChildKind::Input {
                        source: WireSlotIndex(0),
                    },
                    invocation,
                    artifact_id,
                    frame,
                )
                .map_err(ProjectLoadError::Tree)?;

            runtime.insert_artifact_node(artifact_path.clone(), leaf_id);
            loaded_nodes.push(LoadedNode {
                name: node_name,
                artifact_path,
                id: leaf_id,
                config,
            });
        }

        Self::attach_loaded_nodes(root, &mut runtime, &loaded_nodes, frame)?;

        Ok(runtime)
    }

    fn attach_loaded_nodes<R>(
        root: &R,
        runtime: &mut Engine,
        loaded_nodes: &[LoadedNode],
        frame: Revision,
    ) -> Result<(), ProjectLoadError>
    where
        R: ArtifactReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        for node in loaded_nodes {
            if let NodeDef::Texture(_config) = &node.config {
                runtime
                    .attach_runtime_node(node.id, Box::new(TextureNode::new(node.id)), frame)
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach texture runtime: {e}"),
                    })?;
            }
        }

        for node in loaded_nodes {
            if let NodeDef::Output(config) = &node.config {
                runtime
                    .attach_runtime_node(node.id, Box::new(OutputNode::new()), frame)
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach output runtime: {e}"),
                    })?;
                let sink_id = runtime
                    .runtime_output_sink_buffer_id(node.id)
                    .ok_or_else(|| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: String::from("output runtime node produced no sink buffer"),
                    })?;
                runtime.services_mut().register_output_sink(sink_id, config);
                runtime
                    .add_binding(
                        BindingDraft {
                            source: BindingSource::Literal(LpValue::F32(0.0)),
                            target: BindingTarget::ConsumedSlot {
                                node: node.id,
                                slot: demand_input_path(),
                            },
                            priority: BindingPriority::new(0),
                            kind: Kind::Color,
                            owner: node.id,
                        },
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("bind output demand slot: {e}"),
                    })?;
                register_source_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "input",
                    &config.bindings,
                    frame,
                )?;
                runtime.add_demand_root(node.id);
            }
        }

        for node in loaded_nodes {
            if let NodeDef::Shader(config) = &node.config {
                let shader_path =
                    resolve_path_relative_to_file(&node.artifact_path, &config.glsl_path_buf())?;
                let glsl_source = read_utf8_file(root, shader_path.as_path())?;
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(ShaderNode::new(node.id, glsl_source)),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach shader runtime: {e}"),
                    })?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "output",
                    &config.bindings,
                    frame,
                )?;
            }
        }

        for node in loaded_nodes {
            if let NodeDef::Fixture(config) = &node.config {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(FixtureNode::new(node.id, config.mapping.clone(), frame)),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach fixture runtime: {e}"),
                    })?;
                register_source_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "input",
                    &config.bindings,
                    frame,
                )?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "output",
                    &config.bindings,
                    frame,
                )?;
            }
        }

        Ok(())
    }
}

fn load_project_def<R>(root: &R, path: &LpPathBuf) -> Result<ProjectDef, ProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let text = read_utf8_file(root, path.as_path())?;
    match NodeDef::from_toml_str(&text) {
        Ok(NodeDef::Project(def)) => Ok(def),
        Ok(other) => Err(ProjectLoadError::UnknownKind {
            path: path.as_str().to_string(),
            suffix: other.kind_name().to_string(),
        }),
        Err(lpc_model::NodeDefParseError::UnknownKind { kind }) => {
            Err(ProjectLoadError::UnknownKind {
                path: path.as_str().to_string(),
                suffix: kind,
            })
        }
        Err(lpc_model::NodeDefParseError::Toml { error }) => Err(ProjectLoadError::ProjectToml {
            file: path.as_str().to_string(),
            error,
        }),
    }
}

fn load_node_def<R>(root: &R, path: &LpPath) -> Result<NodeDef, ProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let text = read_utf8_file(root, path)?;
    match NodeDef::from_toml_str(&text) {
        Ok(NodeDef::Project(_)) => Err(ProjectLoadError::UnknownKind {
            path: path.as_str().to_string(),
            suffix: "project".to_string(),
        }),
        Ok(def) => Ok(def),
        Err(lpc_model::NodeDefParseError::UnknownKind { kind }) => {
            Err(ProjectLoadError::UnknownKind {
                path: path.as_str().to_string(),
                suffix: kind,
            })
        }
        Err(lpc_model::NodeDefParseError::Toml { error }) => Err(ProjectLoadError::TomlParse {
            path: path.as_str().to_string(),
            error,
        }),
    }
}

fn resolve_project_locator(locator: &ArtifactLocator) -> Result<LpPathBuf, ProjectLoadError> {
    resolve_path_locator_from_dir(LpPath::new("/"), locator)
}

fn resolve_child_artifact_locator(
    containing_file: &LpPathBuf,
    locator: &ArtifactLocator,
) -> Result<LpPathBuf, ProjectLoadError> {
    let parent = containing_file
        .as_path()
        .parent()
        .unwrap_or(LpPath::new("/"));
    resolve_path_locator_from_dir(parent, locator)
}

fn resolve_path_locator_from_dir(
    base_dir: &LpPath,
    locator: &ArtifactLocator,
) -> Result<LpPathBuf, ProjectLoadError> {
    match locator {
        ArtifactLocator::Path(path) => {
            if path.is_absolute() {
                Ok(path.clone())
            } else {
                base_dir
                    .to_path_buf()
                    .join_relative(path.as_str())
                    .ok_or_else(|| ProjectLoadError::InvalidSourcePath {
                        path: path.as_str().to_string(),
                        reason: format!("path cannot be resolved relative to {base_dir:?}"),
                    })
            }
        }
        ArtifactLocator::Lib(lib) => Err(ProjectLoadError::InvalidSourcePath {
            path: lib.to_string(),
            reason: String::from("library artifact locators are not supported for nodes yet"),
        }),
    }
}

fn resolve_path_relative_to_file(
    containing_file: &LpPathBuf,
    path: &LpPathBuf,
) -> Result<LpPathBuf, ProjectLoadError> {
    let parent = containing_file
        .as_path()
        .parent()
        .unwrap_or(LpPath::new("/"));
    parent
        .to_path_buf()
        .join_relative(path.as_str())
        .ok_or_else(|| ProjectLoadError::InvalidSourcePath {
            path: path.as_str().to_string(),
            reason: format!(
                "path cannot be resolved relative to {}",
                containing_file.as_str()
            ),
        })
}

fn node_kind_name(config: &NodeDef, path: &LpPath) -> Result<NodeName, ProjectLoadError> {
    NodeName::parse(config.kind_name()).map_err(|e| ProjectLoadError::InvalidNodeName {
        path: path.as_str().to_string(),
        reason: format!("{e}"),
    })
}

fn resolve_node_loc<'a>(
    loaded_nodes: &'a [LoadedNode],
    current: &'a LoadedNode,
    loc: &lpc_model::RelativeNodeRef,
    expected: &str,
) -> Result<&'a LoadedNode, ProjectLoadError> {
    resolve_relative_node_ref(loaded_nodes, current, loc).ok_or_else(|| {
        ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("unknown {expected} node ref `{loc}`"),
        }
    })
}

fn resolve_relative_node_ref<'a>(
    loaded_nodes: &'a [LoadedNode],
    current: &'a LoadedNode,
    parsed: &lpc_model::RelativeNodeRef,
) -> Option<&'a LoadedNode> {
    if parsed.parent_hops() == 0 && parsed.segments().is_empty() {
        return Some(current);
    }
    if parsed.parent_hops() == 1 && parsed.segments().len() == 1 {
        let target = &parsed.segments()[0];
        return loaded_nodes.iter().find(|node| &node.name == target);
    }
    None
}

fn demand_input_path() -> SlotPath {
    SlotPath::parse("in").expect("valid demand input path")
}

fn binding_source<'a>(bindings: &'a BindingDefs, slot: &str) -> Option<&'a BindingEndpoint> {
    bindings.entries().get(slot)?.source.as_ref()
}

fn binding_target<'a>(bindings: &'a BindingDefs, slot: &str) -> Option<&'a BindingEndpoint> {
    bindings.entries().get(slot)?.target.as_ref()
}

fn register_source_binding(
    engine: &mut Engine,
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let source =
        binding_source(bindings, slot_name).ok_or_else(|| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("{slot_name} source binding is missing"),
        })?;
    let source = binding_source_endpoint(loaded_nodes, current, source)?;
    let target_slot =
        SlotPath::parse(slot_name).map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("invalid target slot `{slot_name}`: {e}"),
        })?;
    engine
        .add_binding(
            BindingDraft {
                source,
                target: BindingTarget::ConsumedSlot {
                    node: current.id,
                    slot: target_slot,
                },
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("register {slot_name} source binding: {e}"),
        })?;
    Ok(())
}

fn register_target_binding(
    engine: &mut Engine,
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let Some(target) = binding_target(bindings, slot_name) else {
        return Ok(());
    };
    let target = binding_target_endpoint(loaded_nodes, current, target)?;
    let source_slot =
        SlotPath::parse(slot_name).map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("invalid source slot `{slot_name}`: {e}"),
        })?;
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::ProducedSlot {
                    node: current.id,
                    slot: source_slot,
                },
                target,
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("register {slot_name} target binding: {e}"),
        })?;
    Ok(())
}

fn binding_source_endpoint(
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    endpoint: &BindingEndpoint,
) -> Result<BindingSource, ProjectLoadError> {
    match endpoint {
        BindingEndpoint::Literal(value) => Ok(BindingSource::Literal(value.clone())),
        BindingEndpoint::Bus(bus) => Ok(BindingSource::BusChannel(ChannelName(
            bus.slot().to_string(),
        ))),
        BindingEndpoint::Node(node_slot) => {
            let node = resolve_node_loc(loaded_nodes, current, node_slot.node(), "binding source")?;
            Ok(BindingSource::ProducedSlot {
                node: node.id,
                slot: node_slot.slot().clone(),
            })
        }
    }
}

fn binding_target_endpoint(
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    endpoint: &BindingEndpoint,
) -> Result<BindingTarget, ProjectLoadError> {
    match endpoint {
        BindingEndpoint::Literal(_) => Err(ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: String::from("binding target cannot be a literal"),
        }),
        BindingEndpoint::Bus(bus) => Ok(BindingTarget::BusChannel(ChannelName(
            bus.slot().to_string(),
        ))),
        BindingEndpoint::Node(node_slot) => {
            let node = resolve_node_loc(loaded_nodes, current, node_slot.node(), "binding target")?;
            Ok(BindingTarget::ConsumedSlot {
                node: node.id,
                slot: node_slot.slot().clone(),
            })
        }
    }
}

fn read_utf8_file<R>(root: &R, path: &LpPath) -> Result<String, ProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let data = root.read_file(path).map_err(|e| ProjectLoadError::Io {
        path: path.as_str().to_string(),
        details: format!("{e:?}"),
    })?;
    String::from_utf8(data).map_err(|e| ProjectLoadError::InvalidSourcePath {
        path: path.as_str().to_string(),
        reason: format!("shader source is not UTF-8: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use lpc_model::NodeName;
    use lpc_model::TreePath;
    use lpfs::lp_path::AsLpPath;
    use lpfs::{LpFs, LpFsMemory};

    use super::*;

    fn flat_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        write_flat_basic_files(&fs);
        fs
    }

    #[test]
    fn project_toml_loads_into_runtime_with_expected_nodes() {
        let fs = flat_project();
        let root_path = TreePath::parse("/demo.show").expect("path");
        let services = EngineServices::new(root_path.clone());
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        let root = rt.tree().root();

        let tex_id = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("texture").unwrap())
            .expect("texture id");
        let sh_id = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("shader").unwrap())
            .expect("shader id");
        let out_id = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("output").unwrap())
            .expect("output id");
        let fix_id = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("fixture").unwrap())
            .expect("fixture id");

        assert_eq!(
            rt.artifact_node_id(LpPath::new("/texture.toml")),
            Some(tex_id)
        );

        for id in [tex_id, sh_id, out_id, fix_id] {
            let entry = rt.tree().get(id).expect("entry");
            assert!(
                entry.state.value().is_alive(),
                "node {id:?} should be alive",
            );
        }

        let root_entry = rt.tree().get(root).expect("root entry");
        assert!(
            root_entry.state.value().is_alive(),
            "project root should be alive"
        );
        assert_eq!(
            rt.tree()
                .get(fix_id)
                .and_then(|entry| entry.path.0.last())
                .map(|s| s.ty.to_string())
                .as_deref(),
            Some("fixture")
        );

        assert!(
            rt.demand_roots().contains(&out_id),
            "output must be demand root"
        );
        assert!(
            !rt.demand_roots().contains(&fix_id),
            "fixture is driven by output demand"
        );
        assert!(
            !rt.demand_roots().contains(&tex_id),
            "texture is not demand root"
        );
    }

    #[test]
    fn malformed_node_toml_returns_error() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "project"

[nodes.broken]
artifact = "./broken.toml"
"#,
        )
        .expect("project.toml");
        fs.write_file("/broken.toml".as_path(), b"not valid toml {{{")
            .expect("broken.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, ProjectLoadError::TomlParse { .. }),
            "expected TomlParse, got {err:?}"
        );
    }

    #[test]
    fn missing_project_toml_returns_io_error() {
        let fs = LpFsMemory::new();
        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, ProjectLoadError::Io { .. }),
            "expected Io, got {err:?}"
        );
    }

    #[test]
    fn unknown_child_kind_returns_error() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "project"

[nodes.weird]
artifact = "./weird.toml"
"#,
        )
        .expect("project.toml");
        fs.write_file("/weird.toml".as_path(), br#"kind = "banana""#)
            .expect("weird.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, ProjectLoadError::UnknownKind { .. }),
            "expected UnknownKind, got {err:?}"
        );
    }

    #[test]
    fn missing_sibling_node_loc_names_missing_ref() {
        let fs = flat_project();
        fs.write_file(
            "/fixture.toml".as_path(),
            br#"
kind = "fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false

[bindings.input]
source = "..missing#output"

[bindings.output]
target = "bus#control.out"

[transform]
m00 = 1.0
m01 = 0.0
m10 = 1.0
m11 = 1.0
tx = 0.0
ty = 0.0

[mapping]
kind = "path_points"
sample_diameter = 2.0

[mapping.paths.0]
kind = "ring_array"
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 1
offset_angle = 0.0
order = "inner_first"

[mapping.paths.0.ring_lamp_counts]
0 = 1
"#,
        )
        .expect("fixture.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(
                err,
                ProjectLoadError::InvalidSourcePath { ref reason, .. }
                    if reason.contains("unknown binding source node ref `..missing`")
            ),
            "expected missing binding source ref, got {err:?}"
        );
    }

    #[test]
    fn slash_node_ref_is_rejected_during_parse() {
        let fs = flat_project();
        fs.write_file(
            "/fixture.toml".as_path(),
            br#"
kind = "fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false

[bindings.input]
source = "/texture#output"

[bindings.output]
target = "bus#control.out"

[transform]
m00 = 1.0
m01 = 0.0
m10 = 1.0
m11 = 1.0
tx = 0.0
ty = 0.0

[mapping]
kind = "path_points"
sample_diameter = 2.0

[mapping.paths.0]
kind = "ring_array"
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 1
offset_angle = 0.0
order = "inner_first"

[mapping.paths.0.ring_lamp_counts]
0 = 1
"#,
        )
        .expect("fixture.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(
                err,
                ProjectLoadError::TomlParse { ref error, .. }
                    if error.contains("node locations use dot syntax")
            ),
            "expected invalid slash node ref parse error, got {err:?}"
        );
    }

    fn write_flat_basic_files(fs: &LpFsMemory) {
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "project"
name = "basic"

[nodes.output]
artifact = "./output.toml"

[nodes.texture]
artifact = "./texture.toml"

[nodes.shader]
artifact = "./shader.toml"

[nodes.fixture]
artifact = "./fixture.toml"
"#,
        )
        .expect("project.toml");
        fs.write_file(
            "/texture.toml".as_path(),
            br#"
kind = "texture"
[size]
width = 16
height = 16

[bindings.input]
source = "bus#visual.out"
"#,
        )
        .expect("texture.toml");
        fs.write_file(
            "/shader.toml".as_path(),
            br#"
kind = "shader"
glsl_path = "shader.glsl"
render_order = 0

[bindings.output]
target = "bus#visual.out"
"#,
        )
        .expect("shader.toml");
        fs.write_file(
            "/shader.glsl".as_path(),
            b"vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
        )
        .expect("shader.glsl");
        fs.write_file(
            "/output.toml".as_path(),
            br#"
kind = "output"
pin = 4

[bindings.input]
source = "bus#control.out"
"#,
        )
        .expect("output.toml");
        fs.write_file(
            "/fixture.toml".as_path(),
            br#"
kind = "fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false

[bindings.input]
source = "bus#visual.out"

[bindings.output]
target = "bus#control.out"

[transform]
m00 = 1.0
m01 = 0.0
m10 = 0.0
m11 = 1.0
tx = 0.0
ty = 0.0

[mapping]
kind = "path_points"
sample_diameter = 2.0

[mapping.paths.0]
kind = "ring_array"
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 1
offset_angle = 0.0
order = "inner_first"

[mapping.paths.0.ring_lamp_counts]
0 = 1
"#,
        )
        .expect("fixture.toml");
    }
}
