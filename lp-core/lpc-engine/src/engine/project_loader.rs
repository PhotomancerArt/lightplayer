//! Load authored `project.toml` node-artifact trees into [`super::Engine`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::LpType;
use lpc_model::generate_compute_shader_header;
use lpc_model::nodes::project::project_def::ProjectDef;
use lpc_model::{ArtifactLocator, ArtifactReadRoot, NodeDefRef, NodeInvocation, NodeKind};
use lpc_model::{
    BindingDefs, BindingRef as AuthoredBindingRef, ChannelName, FluidDef, Kind, LpValue, NodeDef,
    NodeId, NodeName, Revision, ShaderDef, ShaderSlotKind, ShaderSource, SlotPath,
    SlotShapeRegistry,
};
use lpc_wire::{WireChildKind, WireSlotIndex};
use lpfs::lp_path::{LpPath, LpPathBuf};

use crate::artifact::ArtifactLocation;
use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::node::{NodeDefHandle, TreeError};
use crate::nodes::{
    ButtonNode, ClockNode, ComputeShaderNode, CorePlaceholderNode, FixtureNode, FluidNode,
    OutputNode, ShaderNode, TextureNode,
};

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
    source_base_path: LpPathBuf,
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
        let project_root = services.project_root().clone();
        let mut runtime = Engine::with_services(project_root.clone(), services);
        let project_def = load_project_def(root, &project_path, runtime.slot_shapes())?;
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
            let (artifact_path, source_base_path, config, artifact_id) = match &invocation.def {
                NodeDefRef::Path(artifact_locator) => {
                    let artifact_path =
                        resolve_child_artifact_locator(&project_path, artifact_locator)?;
                    let config =
                        load_node_def(root, artifact_path.as_path(), runtime.slot_shapes())?;
                    let artifact_id = runtime
                        .artifacts_mut()
                        .acquire_location(ArtifactLocation::file(artifact_path.clone()), frame);
                    (artifact_path.clone(), artifact_path, config, artifact_id)
                }
                NodeDefRef::Inline(def) => {
                    let artifact_path = inline_node_artifact_path(&project_path, &node_name);
                    let config = (**def).clone();
                    let artifact_id = runtime.artifacts_mut().acquire_location(
                        ArtifactLocation::inline_node(project_path.clone(), node_name.as_str()),
                        frame,
                    );
                    (artifact_path, project_path.clone(), config, artifact_id)
                }
            };
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
                source_base_path,
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
            if let NodeDef::Clock(config) = &node.config {
                runtime
                    .attach_runtime_node(node.id, Box::new(ClockNode::new(node.id)), frame)
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach clock runtime: {e}"),
                    })?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "seconds",
                    &config.bindings,
                    frame,
                )?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "delta_seconds",
                    &config.bindings,
                    frame,
                )?;
                register_clock_default_time_binding(runtime, node, &config.bindings, frame)?;
            }
        }

        for node in loaded_nodes {
            if let NodeDef::Button(config) = &node.config {
                runtime
                    .attach_runtime_node(node.id, Box::new(ButtonNode::new()), frame)
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach button runtime: {e}"),
                    })?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "down",
                    &config.bindings,
                    frame,
                )?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "held",
                    &config.bindings,
                    frame,
                )?;
                register_target_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "up",
                    &config.bindings,
                    frame,
                )?;
            }
        }

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
                let glsl_source =
                    read_shader_source(root, &node.source_base_path, config.shader_source())?;
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(ShaderNode::new(node.id, config.clone(), glsl_source)),
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
                for name in config.consumed_slots.entries.keys() {
                    register_optional_source_binding(
                        runtime,
                        loaded_nodes,
                        node,
                        name.as_str(),
                        &config.bindings,
                        frame,
                    )?;
                }
                register_visual_default_time_binding(runtime, node, config, frame)?;
            }
        }

        for node in loaded_nodes {
            if let NodeDef::ComputeShader(config) = &node.config {
                let source =
                    read_shader_source(root, &node.source_base_path, config.shader_source())?;
                let header = generate_compute_shader_header(config, runtime.slot_shapes())
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("generate compute shader header: {e}"),
                    })?;
                let glsl_source = format!("{header}\n{source}");
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(ComputeShaderNode::new(
                            node.id,
                            config.clone(),
                            glsl_source,
                            frame,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach compute shader runtime: {e}"),
                    })?;

                for name in config.consumed_slots.entries.keys() {
                    register_optional_source_binding(
                        runtime,
                        loaded_nodes,
                        node,
                        name.as_str(),
                        &config.bindings,
                        frame,
                    )?;
                }
                for name in config.produced_slots.entries.keys() {
                    register_target_binding(
                        runtime,
                        loaded_nodes,
                        node,
                        name.as_str(),
                        &config.bindings,
                        frame,
                    )?;
                }
            }
        }

        for node in loaded_nodes {
            if let NodeDef::Fluid(config) = &node.config {
                runtime
                    .attach_runtime_node(node.id, Box::new(FluidNode::new(node.id)), frame)
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach fluid runtime: {e}"),
                    })?;
                register_optional_source_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "time",
                    &config.bindings,
                    frame,
                )?;
                register_fluid_default_time_binding(runtime, loaded_nodes, node, config, frame)?;
                register_optional_source_binding(
                    runtime,
                    loaded_nodes,
                    node,
                    "emitters",
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

        for node in loaded_nodes {
            if let NodeDef::Fixture(config) = &node.config {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(FixtureNode::new(
                            node.id,
                            config.mapping.value().clone(),
                            *config.sampling.value(),
                            frame,
                        )),
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

fn load_project_def<R>(
    root: &R,
    path: &LpPathBuf,
    registry: &SlotShapeRegistry,
) -> Result<ProjectDef, ProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let text = read_utf8_file(root, path.as_path())?;
    match NodeDef::read_toml(registry, &text) {
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

fn load_node_def<R>(
    root: &R,
    path: &LpPath,
    registry: &SlotShapeRegistry,
) -> Result<NodeDef, ProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let text = read_utf8_file(root, path)?;
    match NodeDef::read_toml(registry, &text) {
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

fn inline_node_artifact_path(project_path: &LpPathBuf, node_name: &NodeName) -> LpPathBuf {
    LpPathBuf::from(format!(
        "{}#nodes.{}",
        project_path.as_str(),
        node_name.as_str()
    ))
}

fn read_shader_source<R>(
    root: &R,
    containing_file: &LpPathBuf,
    source: &ShaderSource,
) -> Result<String, ProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    match source {
        ShaderSource::Path(path) => {
            let shader_path =
                resolve_path_relative_to_file(containing_file, &path.value().as_path_buf())?;
            read_utf8_file(root, shader_path.as_path())
        }
        ShaderSource::Glsl(source) => Ok(source.value().clone()),
    }
}

fn node_kind_name(config: &NodeDef, path: &LpPath) -> Result<NodeName, ProjectLoadError> {
    let name = match config {
        NodeDef::ComputeShader(_) => "compute_shader",
        NodeDef::Shader(_) => "shader",
        _ => config.kind_name(),
    };
    NodeName::parse(name).map_err(|e| ProjectLoadError::InvalidNodeName {
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

enum AuthoredBindingSource<'a> {
    Value(&'a LpValue),
    Ref(&'a AuthoredBindingRef),
}

fn binding_source<'a>(bindings: &'a BindingDefs, slot: &str) -> Option<AuthoredBindingSource<'a>> {
    let binding = bindings.entries().get(slot)?;
    if let Some(value) = binding.value_literal() {
        return Some(AuthoredBindingSource::Value(value));
    }
    binding.source_ref().map(AuthoredBindingSource::Ref)
}

fn binding_target<'a>(bindings: &'a BindingDefs, slot: &str) -> Option<&'a AuthoredBindingRef> {
    bindings.entries().get(slot)?.target_ref()
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
                kind: binding_kind_for_slot(slot_name),
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

fn register_optional_source_binding(
    engine: &mut Engine,
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if binding_source(bindings, slot_name).is_none() {
        return Ok(());
    }
    register_source_binding(engine, loaded_nodes, current, slot_name, bindings, frame)
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
                priority: BindingPriority::authored(),
                kind: binding_kind_for_slot(slot_name),
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

fn binding_kind_for_slot(slot_name: &str) -> Kind {
    match slot_name {
        "time" | "seconds" | "delta_seconds" => Kind::Instant,
        _ => Kind::Color,
    }
}

fn register_clock_default_time_binding(
    engine: &mut Engine,
    current: &LoadedNode,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if binding_target(bindings, "seconds").is_some() {
        return Ok(());
    }
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::ProducedSlot {
                    node: current.id,
                    slot: SlotPath::parse("seconds").expect("clock seconds slot path"),
                },
                target: BindingTarget::BusChannel(ChannelName(String::from("time.seconds"))),
                priority: BindingPriority::default_fallback(),
                kind: Kind::Instant,
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("register clock default time binding: {e}"),
        })?;
    Ok(())
}

fn register_visual_default_time_binding(
    engine: &mut Engine,
    current: &LoadedNode,
    config: &ShaderDef,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if binding_source(&config.bindings, "time").is_some() {
        return Ok(());
    }
    let Some(slot) = config.consumed_slots.entries.get("time") else {
        return Ok(());
    };
    if *slot.kind.value() != ShaderSlotKind::Value
        || slot.value.value().as_lp_type() != Some(LpType::F32)
    {
        return Ok(());
    }
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::BusChannel(ChannelName(String::from("time.seconds"))),
                target: BindingTarget::ConsumedSlot {
                    node: current.id,
                    slot: SlotPath::parse("time").expect("visual shader time slot path"),
                },
                priority: BindingPriority::default_fallback(),
                kind: Kind::Instant,
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("register visual shader default time binding: {e}"),
        })?;
    Ok(())
}

fn register_fluid_default_time_binding(
    engine: &mut Engine,
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    config: &FluidDef,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if binding_source(&config.bindings, "time").is_some() || !has_default_time_bus(loaded_nodes) {
        return Ok(());
    }
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::BusChannel(ChannelName(String::from("time.seconds"))),
                target: BindingTarget::ConsumedSlot {
                    node: current.id,
                    slot: SlotPath::parse("time").expect("fluid time slot path"),
                },
                priority: BindingPriority::default_fallback(),
                kind: Kind::Instant,
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: format!("register fluid default time binding: {e}"),
        })?;
    Ok(())
}

fn has_default_time_bus(loaded_nodes: &[LoadedNode]) -> bool {
    loaded_nodes.iter().any(|node| match &node.config {
        NodeDef::Clock(config) => {
            binding_target(&config.bindings, "seconds").is_none_or(is_time_seconds_bus_target)
        }
        _ => false,
    })
}

fn is_time_seconds_bus_target(target: &AuthoredBindingRef) -> bool {
    matches!(target, AuthoredBindingRef::Bus(bus) if bus.slot().to_string() == "time.seconds")
}

fn binding_source_endpoint(
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    endpoint: AuthoredBindingSource<'_>,
) -> Result<BindingSource, ProjectLoadError> {
    match endpoint {
        AuthoredBindingSource::Value(value) => Ok(BindingSource::Literal(value.clone())),
        AuthoredBindingSource::Ref(binding_ref) => {
            binding_ref_source(loaded_nodes, current, binding_ref)
        }
    }
}

fn binding_ref_source(
    loaded_nodes: &[LoadedNode],
    current: &LoadedNode,
    binding_ref: &AuthoredBindingRef,
) -> Result<BindingSource, ProjectLoadError> {
    match binding_ref {
        AuthoredBindingRef::Unset => Err(ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: String::from("binding source cannot be unset"),
        }),
        AuthoredBindingRef::Bus(bus) => Ok(BindingSource::BusChannel(ChannelName(
            bus.slot().to_string(),
        ))),
        AuthoredBindingRef::Node(node_slot) => {
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
    endpoint: &AuthoredBindingRef,
) -> Result<BindingTarget, ProjectLoadError> {
    match endpoint {
        AuthoredBindingRef::Unset => Err(ProjectLoadError::InvalidSourcePath {
            path: current.artifact_path.as_str().to_string(),
            reason: String::from("binding target cannot be unset"),
        }),
        AuthoredBindingRef::Bus(bus) => Ok(BindingTarget::BusChannel(ChannelName(
            bus.slot().to_string(),
        ))),
        AuthoredBindingRef::Node(node_slot) => {
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
    extern crate std;

    use alloc::rc::Rc;
    use alloc::sync::Arc;
    use lpc_model::{NodeName, ProductRef, SlotData, SlotMapKey, TreePath};
    use lpc_shared::hardware::{
        HardwareAddress, HardwareRegistry, HardwareSystem, VirtualButtonDriver,
        default_esp32c6_hardware_manifest,
    };
    use lpc_wire::{
        ProjectProbeRequest, ProjectProbeResult, ProjectReadRequest, ProjectReadResult,
        RenderProductProbeRequest, RenderProductProbeResult, WireTextureFormat,
    };
    use lpfs::lp_path::AsLpPath;
    use lpfs::{LpFs, LpFsMemory, LpFsStd};
    use lps_shared::TextureStorageFormat;

    use super::*;
    use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
    use crate::engine::{ButtonService, resolve_with_engine_host};
    use crate::products::visual::RenderTextureRequest;

    fn flat_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        write_flat_basic_files(&fs);
        fs
    }

    fn examples_fluid_fs() -> LpFsStd {
        LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/fluid"))
    }

    fn examples_events_fs() -> LpFsStd {
        LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/events"))
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
    fn project_loader_loads_inline_clock_and_default_time_bus() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.clock]
def = { path = "./clock.toml" }

[nodes.shader]
def = { path = "./shader.toml" }
"#,
        )
        .expect("project.toml");
        fs.write_file(
            "/clock.toml".as_path(),
            br#"kind = "Clock"
"#,
        )
        .expect("clock.toml");
        fs.write_file(
            "/shader.toml".as_path(),
            br#"
kind = "Shader"
source = { path = "shader.glsl" }
render_order = 0

[consumed_slots.time]
kind = "value"
value = "f32"
default = 0.0
"#,
        )
        .expect("shader.toml");
        fs.write_file(
            "/shader.glsl".as_path(),
            b"vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
        )
        .expect("shader.glsl");

        let services = EngineServices::new(TreePath::parse("/clock.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        let root = rt.tree().root();
        let clock = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("clock").unwrap())
            .expect("clock node");
        assert!(
            rt.tree()
                .get(clock)
                .expect("clock")
                .state
                .value()
                .is_alive()
        );
        let shader = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("shader").unwrap())
            .expect("shader node");

        rt.tick(1000).expect("first tick");
        let first = resolve_with_engine_host(
            &mut rt,
            QueryKey::Bus(ChannelName(String::from("time.seconds"))),
            ResolveLogLevel::Off,
        )
        .expect("resolve time bus")
        .0;
        assert_eq!(
            *first.value_leaf().expect("time value").value(),
            LpValue::F32(0.0)
        );
        let shader_first = resolve_with_engine_host(
            &mut rt,
            QueryKey::ConsumedSlot {
                node: shader,
                slot: SlotPath::parse("time").expect("time slot"),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve visual shader time")
        .0;
        assert_eq!(
            *shader_first.value_leaf().expect("time value").value(),
            LpValue::F32(0.0)
        );

        rt.tick(1000).expect("second tick");
        let second = resolve_with_engine_host(
            &mut rt,
            QueryKey::Bus(ChannelName(String::from("time.seconds"))),
            ResolveLogLevel::Off,
        )
        .expect("resolve time bus")
        .0;
        assert_eq!(
            *second.value_leaf().expect("time value").value(),
            LpValue::F32(1.0)
        );
        let shader_second = resolve_with_engine_host(
            &mut rt,
            QueryKey::ConsumedSlot {
                node: shader,
                slot: SlotPath::parse("time").expect("time slot"),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve visual shader time")
        .0;
        assert_eq!(
            *shader_second.value_leaf().expect("time value").value(),
            LpValue::F32(1.0)
        );
    }

    #[test]
    fn project_loader_loads_inline_shader_def_and_source() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.shader.def]
kind = "Shader"
source = { glsl = "vec4 render(vec2 pos) { return vec4(1.0, 0.0, 0.0, 1.0); }" }
"#,
        )
        .expect("project.toml");

        let services = EngineServices::new(TreePath::parse("/inline.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let root = rt.tree().root();
        let shader = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("shader").unwrap())
            .expect("shader node");

        rt.tick(16).expect("tick");
        let production = resolve_with_engine_host(
            &mut rt,
            QueryKey::ProducedSlot {
                node: shader,
                slot: crate::nodes::shader_output_path(),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve shader output")
        .0;
        let LpValue::Product(ProductRef::Visual(product)) =
            production.value_leaf().expect("visual product").value()
        else {
            panic!("shader output should be a visual product");
        };

        let texture = rt
            .render_texture_for_test(
                *product,
                &RenderTextureRequest {
                    width: 2,
                    height: 2,
                    format: TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.0,
                },
            )
            .expect("render inline shader");
        assert!(
            texture
                .try_raw_bytes()
                .expect("bytes")
                .chunks_exact(8)
                .any(|px| px[0] != 0 || px[1] != 0),
            "inline shader should produce nonzero red output"
        );
    }

    #[test]
    fn malformed_node_toml_returns_error() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.broken]
def = { path = "./broken.toml" }
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
    fn unknown_child_kind_returns_toml_parse_error() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.weird]
def = { path = "./weird.toml" }
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
kind = "Fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false
transform = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]]

[bindings.input]
source = "..missing#output"

[bindings.output]
target = "bus#control.out"

[mapping]
kind = "PathPoints"
sample_diameter = 2.0

[mapping.paths.0]
kind = "RingArray"
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
kind = "Fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false
transform = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]]

[bindings.input]
source = "/texture#output"

[bindings.output]
target = "bus#control.out"

[mapping]
kind = "PathPoints"
sample_diameter = 2.0

[mapping.paths.0]
kind = "RingArray"
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

    #[test]
    fn project_loader_attaches_compute_shader_node() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.compute]
def = { path = "./compute.toml" }
"#,
        )
        .expect("project.toml");
        fs.write_file(
            "/compute.toml".as_path(),
            br#"
kind = "ComputeShader"
source = { path = "compute.glsl" }

[consumed_slots.time]
kind = "value"
value = "f32"
default = 0.25

[produced_slots.phase]
kind = "value"
value = "f32"
"#,
        )
        .expect("compute.toml");
        fs.write_file(
            "/compute.glsl".as_path(),
            b"void tick() { phase = time + 2.0; }",
        )
        .expect("compute.glsl");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let node = rt
            .artifact_node_id(LpPath::new("/compute.toml"))
            .expect("compute node");

        let production = resolve_with_engine_host(
            &mut rt,
            QueryKey::ProducedSlot {
                node,
                slot: SlotPath::parse("phase").expect("phase"),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve phase")
        .0;

        assert_eq!(
            *production.value_leaf().expect("value").value(),
            LpValue::F32(2.25)
        );
    }

    #[test]
    fn fluid_example_loads_compute_fluid_fixture_flow() {
        let fs = examples_fluid_fs();
        let fs: &dyn LpFs = &fs;
        let services = EngineServices::new(TreePath::parse("/fluid.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(fs, services).expect("load fluid example");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let root = rt.tree().root();

        let compute = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("compute").unwrap())
            .expect("compute node");
        let fluid = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("fluid").unwrap())
            .expect("fluid node");
        let fixture = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("fixture").unwrap())
            .expect("fixture node");
        let output = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("output").unwrap())
            .expect("output node");

        for id in [compute, fluid, fixture, output] {
            assert!(rt.tree().get(id).expect("entry").state.value().is_alive());
        }

        let (emitters, _) = resolve_with_engine_host(
            &mut rt,
            QueryKey::ProducedSlot {
                node: compute,
                slot: SlotPath::parse("emitters").expect("emitters"),
            },
            ResolveLogLevel::Off,
        )
        .expect("compute emitters");
        let SlotData::Map(map) = emitters.data() else {
            panic!("compute emitters should be a map");
        };
        assert!(!map.entries.is_empty());
        rt.tick(16).expect("tick fluid graph");

        let (fluid_output, _) = resolve_with_engine_host(
            &mut rt,
            QueryKey::ProducedSlot {
                node: fluid,
                slot: SlotPath::parse("output").expect("output"),
            },
            ResolveLogLevel::Off,
        )
        .expect("fluid output");
        let LpValue::Product(ProductRef::Visual(product)) =
            fluid_output.value_leaf().expect("visual product").value()
        else {
            panic!("fluid output should be a visual product");
        };
        let texture = rt
            .render_texture_for_test(
                *product,
                &RenderTextureRequest {
                    width: 16,
                    height: 16,
                    format: TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.0,
                },
            )
            .expect("fluid texture");
        assert!(
            texture
                .try_raw_bytes()
                .expect("bytes")
                .chunks_exact(8)
                .any(|px| px[..6].iter().any(|byte| *byte != 0)),
            "fluid visual should contain nonzero RGB data"
        );

        let probe_response = rt.read_project(ProjectReadRequest {
            since: None,
            queries: alloc::vec::Vec::new(),
            probes: alloc::vec![ProjectProbeRequest::RenderProduct(
                RenderProductProbeRequest {
                    product: *product,
                    width: 16,
                    height: 16,
                    format: WireTextureFormat::Srgb8,
                },
            )],
            mutations: alloc::vec::Vec::new(),
        });
        let Some(ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
            format,
            bytes,
            ..
        })) = probe_response.probes.first()
        else {
            panic!("fluid visual probe should return a texture");
        };
        assert_eq!(*format, WireTextureFormat::Srgb8);
        assert_eq!(bytes.len(), 16 * 16 * 3);
        assert!(
            bytes.iter().any(|byte| *byte != 0),
            "fluid visual probe should contain nonzero display bytes"
        );

        let response = rt.read_project(ProjectReadRequest::default_debug(None));
        let ProjectReadResult::Nodes(nodes) = &response.results[1] else {
            panic!("node read result");
        };
        let slots = nodes.slots.as_ref().expect("slot roots");
        assert!(
            slots
                .roots
                .iter()
                .any(|root| root.name == format!("node.{}.state", compute.0)),
            "compute state should be visible in debug read"
        );
        assert!(
            slots
                .roots
                .iter()
                .any(|root| root.name == format!("node.{}.state", fluid.0)),
            "fluid state should be visible in debug read"
        );
    }

    #[test]
    fn events_example_merges_bus_maps_into_visual_shader() {
        let fs = examples_events_fs();
        let fs: &dyn LpFs = &fs;
        let services = EngineServices::new(TreePath::parse("/events.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(fs, services).expect("load events example");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let root = rt.tree().root();

        let shader = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("shader").unwrap())
            .expect("shader node");

        rt.tick(16).expect("tick trigger graph");
        let (shader_output, _) = resolve_with_engine_host(
            &mut rt,
            QueryKey::ProducedSlot {
                node: shader,
                slot: SlotPath::parse("output").expect("output"),
            },
            ResolveLogLevel::Off,
        )
        .expect("shader output");
        let LpValue::Product(ProductRef::Visual(product)) =
            shader_output.value_leaf().expect("visual product").value()
        else {
            panic!("shader output should be a visual product");
        };
        let first = render_test_texture_bytes(&mut rt, *product);
        assert_bright_event_pixels(&first);

        rt.tick(500).expect("advance trigger graph");
        let second = render_test_texture_bytes(&mut rt, *product);
        assert_bright_event_pixels(&second);
        assert_ne!(
            first, second,
            "event example should blink as scheduled events fire and clear"
        );
    }

    #[test]
    fn button_node_publishes_held_and_up_from_virtual_d9() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.button]
artifact = "./button.toml"
"#,
        )
        .expect("project");
        fs.write_file(
            "/button.toml".as_path(),
            br#"
kind = "Button"
endpoint = "button:gpio:D9"
stable_ms = 1
"#,
        )
        .expect("button");

        let registry = Rc::new(HardwareRegistry::new(default_esp32c6_hardware_manifest()));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let control = driver.clone();
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(driver));
        let hardware = Rc::new(hardware);
        let button_service: Rc<dyn ButtonService> = hardware.clone();

        let mut services = EngineServices::new(TreePath::parse("/button.show").expect("path"));
        services.set_button_service(Some(button_service));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load button project");
        let root = rt.tree().root();
        let button = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("button").unwrap())
            .expect("button node");

        control.set_pressed(HardwareAddress::gpio(20), true);
        let held = resolve_button_map(&mut rt, button, "held");
        assert!(!held.entries.contains_key(&SlotMapKey::U32(1)));

        rt.tick(1).expect("next frame");
        let held = resolve_button_map(&mut rt, button, "held");
        assert!(held.entries.contains_key(&SlotMapKey::U32(1)));

        control.set_pressed(HardwareAddress::gpio(20), false);
        rt.tick(1).expect("release candidate frame");
        assert!(
            resolve_button_map(&mut rt, button, "held")
                .entries
                .contains_key(&SlotMapKey::U32(1))
        );

        rt.tick(1).expect("release stable frame");
        let up = resolve_button_map(&mut rt, button, "up");
        assert!(up.entries.contains_key(&SlotMapKey::U32(1)));
        let held = resolve_button_map(&mut rt, button, "held");
        assert!(held.entries.is_empty());
    }

    fn render_test_texture_bytes(rt: &mut Engine, product: lpc_model::VisualProduct) -> Vec<u8> {
        rt.render_texture_for_test(
            product,
            &RenderTextureRequest {
                width: 64,
                height: 64,
                format: TextureStorageFormat::Rgba16Unorm,
                time_seconds: 0.0,
            },
        )
        .expect("events texture")
        .try_raw_bytes()
        .expect("bytes")
        .to_vec()
    }

    fn assert_bright_event_pixels(bytes: &[u8]) {
        let max_rgb = bytes
            .chunks_exact(8)
            .flat_map(|px| {
                [
                    u16::from_le_bytes([px[0], px[1]]),
                    u16::from_le_bytes([px[2], px[3]]),
                    u16::from_le_bytes([px[4], px[5]]),
                ]
            })
            .max()
            .unwrap_or(0);

        assert!(
            max_rgb > 10_000,
            "trigger event circles should render bright RGB pixels"
        );
    }

    fn resolve_button_map(rt: &mut Engine, button: NodeId, slot: &str) -> lpc_model::SlotMapDyn {
        let (production, _) = resolve_with_engine_host(
            rt,
            QueryKey::ProducedSlot {
                node: button,
                slot: SlotPath::parse(slot).expect("button slot"),
            },
            ResolveLogLevel::Off,
        )
        .expect("button production");
        let SlotData::Map(map) = production.data().clone() else {
            panic!("button slot should be a map");
        };
        map
    }

    fn write_flat_basic_files(fs: &LpFsMemory) {
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"
name = "basic"

[nodes.output]
def = { path = "./output.toml" }

[nodes.texture]
def = { path = "./texture.toml" }

[nodes.shader]
def = { path = "./shader.toml" }

[nodes.fixture]
def = { path = "./fixture.toml" }
"#,
        )
        .expect("project.toml");
        fs.write_file(
            "/texture.toml".as_path(),
            br#"
kind = "Texture"
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
kind = "Shader"
source = { path = "shader.glsl" }
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
kind = "Output"
endpoint = "ws281x:rmt:D10"

[bindings.input]
source = "bus#control.out"
"#,
        )
        .expect("output.toml");
        fs.write_file(
            "/fixture.toml".as_path(),
            br#"
kind = "Fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false
transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]

[bindings.input]
source = "bus#visual.out"

[bindings.output]
target = "bus#control.out"

[mapping]
kind = "PathPoints"
sample_diameter = 2.0

[mapping.paths.0]
kind = "RingArray"
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
