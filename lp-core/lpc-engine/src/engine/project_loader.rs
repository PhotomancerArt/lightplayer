//! Load authored `project.json` node-artifact trees into [`super::Engine`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lp_collection::VecSet;

use lpc_model::LpType;
use lpc_model::{ArtifactSpec, NodeInvocation, NodeKind};
use lpc_model::{AssetContentType, AssetLocation, NodeDefLocation, NodeDefState};
use lpc_model::{
    BindingDefs, BindingRef as AuthoredBindingRef, ChannelName, FixtureDef, FluidDef, Kind,
    LpValue, MappingConfig, NodeDef, NodeId, NodeName, PlaylistDef, ProjectNodeOrigin,
    ProjectNodePlacement, Revision, ShaderDef, ShaderSlotKind, SlotPath,
};
use lpc_registry::{AssetText, ParseCtx, ProjectRegistry};
use lpc_wire::{NodeRuntimeStatus, WireChildKind, WireSlotIndex};
use lpfs::LpFs;
use lpfs::lp_path::{LpPath, LpPathBuf};

use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::node::{NodeEntryState, TreeError};
use crate::nodes::fixture::mapping::resolve_svg_path_mapping;
use crate::nodes::{
    ButtonNode, ClockNode, ComputeShaderNode, ControlRadioNode, CorePlaceholderNode, FixtureNode,
    FluidNode, OutputNode, PlaylistNode, PlaylistRuntimeEntry, ShaderNode, TextureNode,
    playlist_output_path,
};

use super::{Engine, EngineServices, LoadedProjectRuntime};

/// Errors loading an authored project into [`Engine`].
#[derive(Debug)]
pub enum ProjectLoadError {
    Io { path: String, details: String },
    ProjectParse { file: String, error: String },
    UnknownKind { path: String, suffix: String },
    InvalidProjectReference { path: String, reason: String },
    TomlParse { path: String, error: String },
    InvalidNodeName { path: String, reason: String },
    Tree(TreeError),
}

impl core::fmt::Display for ProjectLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io { path, details } => write!(f, "io error at {path}: {details}"),
            Self::ProjectParse { file, error } => write!(f, "parse {file}: {error}"),
            Self::UnknownKind { path, suffix } => write!(f, "{path}: unknown node kind `{suffix}`"),
            Self::InvalidProjectReference { path, reason } => {
                write!(f, "project reference {path}: {reason}")
            }
            Self::TomlParse { path, error } => write!(f, "{path}: TOML parse failed: {error}"),
            Self::InvalidNodeName { path, reason } => write!(f, "{path}: invalid name: {reason}"),
            Self::Tree(e) => write!(f, "tree: {e}"),
        }
    }
}

impl core::error::Error for ProjectLoadError {}

#[derive(Clone)]
pub(super) struct ProjectedNode {
    pub(super) name: NodeName,
    pub(super) parent: Option<NodeId>,
    pub(super) def_location: NodeDefLocation,
    pub(super) use_location: lpc_model::NodeUseLocation,
    pub(super) id: NodeId,
    pub(super) kind: NodeKind,
    pub(super) provides_default_time_bus: bool,
    pub(super) ownership: ProjectedNodeOwnership,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectedNodeOwnership {
    Root,
    ProjectChild,
    PlaylistEntry { playlist: NodeId, entry: u32 },
}

impl ProjectedNodeOwnership {
    fn suppress_visual_default_output(self) -> bool {
        matches!(self, Self::PlaylistEntry { .. })
    }
}

/// Loads the authored project artifact tree into a core engine-backed runtime.
pub struct ProjectLoader;

impl ProjectLoader {
    pub fn load_from_root(
        root: &dyn LpFs,
        services: EngineServices,
    ) -> Result<LoadedProjectRuntime, ProjectLoadError> {
        Self::load_project_artifact(root, services, ArtifactSpec::path("/project.json"))
    }

    pub fn load_project_artifact(
        root: &dyn LpFs,
        services: EngineServices,
        project_specifier: ArtifactSpec,
    ) -> Result<LoadedProjectRuntime, ProjectLoadError> {
        let project_path = resolve_project_specifier(&project_specifier)?;
        let project_root = services.project_root().clone();
        let mut runtime = Engine::with_services(project_root.clone(), services);
        let mut registry = ProjectRegistry::new();
        let frame = Revision::new(1);
        let shapes = runtime.slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };

        let load_result = registry
            .load_root(root, project_path.as_path(), frame, &ctx)
            .map_err(|e| ProjectLoadError::ProjectParse {
                file: project_path.as_str().to_string(),
                error: e.to_string(),
            })?;
        Self::validate_loaded_root(&registry, &load_result.root, project_path.as_path())?;

        let projected_nodes =
            Self::build_runtime_spine(&registry, &mut runtime, project_specifier.clone(), frame)?;
        Self::attach_projected_nodes(root, &mut registry, &mut runtime, &projected_nodes, frame)?;

        Ok(LoadedProjectRuntime::new(runtime, registry))
    }

    fn validate_loaded_root(
        registry: &ProjectRegistry,
        root: &NodeDefLocation,
        path: &LpPath,
    ) -> Result<(), ProjectLoadError> {
        let entry = registry
            .def(root)
            .ok_or_else(|| ProjectLoadError::ProjectParse {
                file: path.as_str().to_string(),
                error: String::from("registry did not load the project root"),
            })?;

        match &entry.state {
            NodeDefState::Loaded(NodeDef::Project(_)) => Ok(()),
            NodeDefState::Loaded(other) => Err(ProjectLoadError::ProjectParse {
                file: path.as_str().to_string(),
                error: format!("root artifact must be Project, got {:?}", other.kind()),
            }),
            state => Err(project_load_error_for_root_state(path, state)),
        }
    }

    fn build_runtime_spine(
        registry: &ProjectRegistry,
        runtime: &mut Engine,
        project_specifier: ArtifactSpec,
        frame: Revision,
    ) -> Result<Vec<ProjectedNode>, ProjectLoadError> {
        let projected_nodes = Self::ensure_runtime_spine(registry, runtime, frame)?;

        let root = runtime.tree().root();
        {
            let entry = runtime
                .tree()
                .get(root)
                .ok_or(ProjectLoadError::Tree(TreeError::UnknownNode(root)))?;
            if entry.def_location.is_none() {
                return Err(ProjectLoadError::InvalidProjectReference {
                    path: artifact_specifier_label(&project_specifier),
                    reason: String::from("registry did not project a root node"),
                });
            }
        }
        runtime
            .attach_runtime_node(
                root,
                Box::new(CorePlaceholderNode::new_leaf(NodeKind::Project)),
                frame,
            )
            .map_err(|e| ProjectLoadError::InvalidProjectReference {
                path: artifact_specifier_label(&project_specifier),
                reason: format!("attach project runtime: {e}"),
            })?;

        Ok(projected_nodes)
    }

    pub(super) fn ensure_runtime_spine(
        registry: &ProjectRegistry,
        runtime: &mut Engine,
        frame: Revision,
    ) -> Result<Vec<ProjectedNode>, ProjectLoadError> {
        let mut project_nodes = registry
            .inventory()
            .tree
            .nodes
            .values()
            .cloned()
            .collect::<Vec<_>>();
        project_nodes.sort_by(|a, b| {
            a.key
                .segments
                .len()
                .cmp(&b.key.segments.len())
                .then_with(|| a.key.cmp(&b.key))
        });

        let mut projected_nodes = Vec::new();
        for project_node in project_nodes {
            let def_entry = registry.def(&project_node.def_location).ok_or_else(|| {
                ProjectLoadError::InvalidProjectReference {
                    path: def_location_label(&project_node.def_location),
                    reason: String::from("project tree references missing definition entry"),
                }
            })?;
            let kind = def_entry.state.kind().unwrap_or(NodeKind::Project);
            let provides_default_time_bus = def_entry
                .state
                .loaded_def()
                .is_some_and(node_provides_default_time_bus);
            let state_error = def_entry
                .state
                .is_error()
                .then(|| node_def_state_message(&project_node.def_location, &def_entry.state));

            let existing_node_id = runtime.project_runtime_index().node_id(&project_node.key);
            let (node_id, name, parent, ownership, inserted) = if project_node.key.is_root() {
                let root_id = runtime.tree().root();
                let root_entry = runtime
                    .tree_mut()
                    .get_mut(root_id)
                    .ok_or(ProjectLoadError::Tree(TreeError::UnknownNode(root_id)))?;
                root_entry.set_project_identity(
                    project_node.key.clone(),
                    project_node.def_location.clone(),
                );
                (
                    root_id,
                    NodeName::parse("project").map_err(|e| ProjectLoadError::InvalidNodeName {
                        path: def_location_label(&project_node.def_location),
                        reason: e.to_string(),
                    })?,
                    None,
                    ProjectedNodeOwnership::Root,
                    existing_node_id.is_none(),
                )
            } else {
                let parent_key = project_node.parent.as_ref().ok_or_else(|| {
                    ProjectLoadError::InvalidProjectReference {
                        path: def_location_label(&project_node.def_location),
                        reason: String::from("non-root project node has no parent"),
                    }
                })?;
                let parent = runtime
                    .project_runtime_index()
                    .node_id(parent_key)
                    .ok_or_else(|| ProjectLoadError::InvalidProjectReference {
                        path: def_location_label(&project_node.def_location),
                        reason: String::from("project node parent was not projected"),
                    })?;
                let (name, ownership) = projected_node_name_and_ownership(
                    &project_node.origin,
                    parent,
                    &project_node.def_location,
                )?;
                if let Some(node_id) = existing_node_id {
                    (node_id, name, Some(parent), ownership, false)
                } else {
                    let ty = match def_entry.state.loaded_def() {
                        Some(def) => node_kind_name(def, &project_node.def_location)?,
                        None => NodeName::parse("node").map_err(|e| {
                            ProjectLoadError::InvalidNodeName {
                                path: def_location_label(&project_node.def_location),
                                reason: e.to_string(),
                            }
                        })?,
                    };
                    let node_id = runtime
                        .tree_mut()
                        .add_child(
                            parent,
                            name.clone(),
                            ty,
                            WireChildKind::Input {
                                source: WireSlotIndex(0),
                            },
                            project_node_invocation(&project_node.origin),
                            frame,
                        )
                        .map_err(ProjectLoadError::Tree)?;
                    runtime
                        .tree_mut()
                        .get_mut(node_id)
                        .expect("add_child inserted the node")
                        .set_project_identity(
                            project_node.key.clone(),
                            project_node.def_location.clone(),
                        );
                    (node_id, name, Some(parent), ownership, true)
                }
            };

            if inserted {
                runtime.project_runtime_index_mut().insert_node(
                    project_node.key.clone(),
                    node_id,
                    project_node.def_location.clone(),
                );
            }
            if let Some(message) = state_error {
                mark_node_load_error(runtime, node_id, frame, message);
            }

            projected_nodes.push(ProjectedNode {
                name,
                parent,
                def_location: project_node.def_location,
                use_location: project_node.key,
                id: node_id,
                kind,
                provides_default_time_bus,
                ownership,
            });
        }
        runtime
            .project_runtime_index_mut()
            .rebuild_asset_consumers(&registry.inventory().tree);

        Ok(projected_nodes)
    }

    pub(super) fn attach_projected_nodes(
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        runtime: &mut Engine,
        projected_nodes: &[ProjectedNode],
        frame: Revision,
    ) -> Result<(), ProjectLoadError> {
        Self::attach_projected_nodes_filtered(fs, registry, runtime, projected_nodes, None, frame)
    }

    pub(super) fn attach_selected_projected_nodes(
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        runtime: &mut Engine,
        projected_nodes: &[ProjectedNode],
        targets: &VecSet<lpc_model::NodeUseLocation>,
        frame: Revision,
    ) -> Result<(), ProjectLoadError> {
        Self::attach_projected_nodes_filtered(
            fs,
            registry,
            runtime,
            projected_nodes,
            Some(targets),
            frame,
        )
    }

    fn attach_projected_nodes_filtered(
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        runtime: &mut Engine,
        projected_nodes: &[ProjectedNode],
        targets: Option<&VecSet<lpc_model::NodeUseLocation>>,
        frame: Revision,
    ) -> Result<(), ProjectLoadError> {
        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Clock {
                continue;
            }
            let NodeDef::Clock(config) = projected_node_config(registry, node)?.clone() else {
                continue;
            };
            runtime
                .attach_runtime_node(node.id, Box::new(ClockNode::new(node.id)), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach clock runtime: {e}"),
                })?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "seconds",
                &config.bindings,
                frame,
            )?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "delta_seconds",
                &config.bindings,
                frame,
            )?;
            register_clock_default_time_binding(runtime, node, &config.bindings, frame)?;
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Button {
                continue;
            }
            let NodeDef::Button(config) = projected_node_config(registry, node)?.clone() else {
                continue;
            };
            runtime
                .attach_runtime_node(node.id, Box::new(ButtonNode::new()), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach button runtime: {e}"),
                })?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "down",
                &config.bindings,
                frame,
            )?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "held",
                &config.bindings,
                frame,
            )?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "up",
                &config.bindings,
                frame,
            )?;
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::ControlRadio {
                continue;
            }
            let NodeDef::ControlRadio(config) = projected_node_config(registry, node)?.clone()
            else {
                continue;
            };
            runtime
                .attach_runtime_node(node.id, Box::new(ControlRadioNode::new()), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach control radio runtime: {e}"),
                })?;
            register_optional_source_binding(
                runtime,
                projected_nodes,
                node,
                "input",
                &config.bindings,
                frame,
            )?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "output",
                &config.bindings,
                frame,
            )?;
            runtime.add_demand_root(node.id);
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Texture {
                continue;
            }
            runtime
                .attach_runtime_node(node.id, Box::new(TextureNode::new(node.id)), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach texture runtime: {e}"),
                })?;
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Output {
                continue;
            }
            let NodeDef::Output(config) = projected_node_config(registry, node)?.clone() else {
                continue;
            };
            runtime
                .attach_runtime_node(node.id, Box::new(OutputNode::new()), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach output runtime: {e}"),
                })?;
            let sink_id = runtime
                .runtime_output_sink_buffer_id(node.id)
                .ok_or_else(|| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: String::from("output runtime node produced no sink buffer"),
                })?;
            runtime
                .services_mut()
                .register_output_sink(sink_id, &config);
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
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("bind output demand slot: {e}"),
                })?;
            register_source_binding(
                runtime,
                projected_nodes,
                node,
                "input",
                &config.bindings,
                frame,
            )?;
            runtime.add_demand_root(node.id);
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Shader {
                continue;
            }
            let NodeDef::Shader(config) = projected_node_config(registry, node)?.clone() else {
                continue;
            };
            let glsl_source = materialize_node_text_asset(
                fs,
                registry,
                node,
                AssetContentType::ShaderSource,
                "shader source",
            )?;
            let bindings = config.bindings.clone();
            let consumed_slot_names = config
                .consumed_slots
                .entries
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            let needs_default_time_binding = shader_needs_default_time_binding(&config);
            runtime
                .attach_runtime_node(
                    node.id,
                    Box::new(ShaderNode::new(node.id, config, glsl_source)),
                    frame,
                )
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach shader runtime: {e}"),
                })?;
            register_target_binding(runtime, projected_nodes, node, "output", &bindings, frame)?;
            register_visual_default_output_binding(runtime, node, &bindings, frame)?;
            for name in consumed_slot_names {
                register_optional_source_binding(
                    runtime,
                    projected_nodes,
                    node,
                    name.as_str(),
                    &bindings,
                    frame,
                )?;
            }
            if needs_default_time_binding {
                add_visual_default_time_binding(runtime, node, frame)?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::ComputeShader {
                continue;
            }
            let NodeDef::ComputeShader(config) = projected_node_config(registry, node)?.clone()
            else {
                continue;
            };
            let source = materialize_node_text_asset(
                fs,
                registry,
                node,
                AssetContentType::ComputeShaderSource,
                "compute shader source",
            )?;
            let bindings = config.bindings.clone();
            let consumed_slot_names = config
                .consumed_slots
                .entries
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            let produced_slot_names = config
                .produced_slots
                .entries
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            runtime
                .attach_runtime_node(
                    node.id,
                    Box::new(
                        ComputeShaderNode::from_asset_text(
                            node.id,
                            config,
                            source,
                            runtime.slot_shapes(),
                            frame,
                        )
                        .map_err(|e| {
                            ProjectLoadError::InvalidProjectReference {
                                path: node_label(node),
                                reason: format!("generate compute shader header: {e}"),
                            }
                        })?,
                    ),
                    frame,
                )
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach compute shader runtime: {e}"),
                })?;

            for name in consumed_slot_names {
                register_optional_source_binding(
                    runtime,
                    projected_nodes,
                    node,
                    name.as_str(),
                    &bindings,
                    frame,
                )?;
            }
            for name in produced_slot_names {
                register_target_binding(
                    runtime,
                    projected_nodes,
                    node,
                    name.as_str(),
                    &bindings,
                    frame,
                )?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Fluid {
                continue;
            }
            let NodeDef::Fluid(config) = projected_node_config(registry, node)?.clone() else {
                continue;
            };
            runtime
                .attach_runtime_node(node.id, Box::new(FluidNode::new(node.id)), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach fluid runtime: {e}"),
                })?;
            register_optional_source_binding(
                runtime,
                projected_nodes,
                node,
                "time",
                &config.bindings,
                frame,
            )?;
            register_fluid_default_time_binding(runtime, projected_nodes, node, &config, frame)?;
            register_optional_source_binding(
                runtime,
                projected_nodes,
                node,
                "emitters",
                &config.bindings,
                frame,
            )?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "output",
                &config.bindings,
                frame,
            )?;
            register_visual_default_output_binding(runtime, node, &config.bindings, frame)?;
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Playlist {
                continue;
            }
            let (
                idle_entry,
                default_fade,
                entries,
                time_source,
                output_target,
                entry_trigger_sources,
            ) = {
                let NodeDef::Playlist(config) = projected_node_config(registry, node)? else {
                    continue;
                };
                (
                    *config.idle_entry.value(),
                    config.default_fade.value().0,
                    playlist_runtime_entries(projected_nodes, node.id, config),
                    binding_source(&config.bindings, "time")
                        .map(|source| binding_source_endpoint(projected_nodes, node, source))
                        .transpose()?,
                    binding_target(&config.bindings, "output")
                        .map(|target| binding_target_endpoint(projected_nodes, node, target))
                        .transpose()?,
                    playlist_entry_trigger_sources(projected_nodes, node, config)?,
                )
            };
            if let Some(source) = time_source {
                register_source_binding_at_path(
                    runtime,
                    node,
                    "time",
                    source,
                    SlotPath::parse("time").expect("playlist time slot"),
                    frame,
                )?;
            }
            if let Some(target) = output_target.clone() {
                runtime
                    .add_binding(
                        BindingDraft {
                            source: BindingSource::ProducedSlot {
                                node: node.id,
                                slot: playlist_output_path(),
                            },
                            target,
                            priority: BindingPriority::authored(),
                            kind: binding_kind_for_slot("output"),
                            owner: node.id,
                        },
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("register output target binding: {e}"),
                    })?;
            }
            if output_target.is_none() {
                add_visual_default_output_binding(runtime, node, frame)?;
            }
            for (entry_index, source) in entry_trigger_sources {
                let target_slot = SlotPath::parse(&format!("entries[{entry_index}].trigger"))
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("invalid playlist entry trigger path: {e}"),
                    })?;
                register_source_binding_at_path(
                    runtime,
                    node,
                    "trigger",
                    source,
                    target_slot,
                    frame,
                )?;
            }
            runtime
                .attach_runtime_node(
                    node.id,
                    Box::new(PlaylistNode::new(
                        node.id,
                        idle_entry,
                        default_fade,
                        entries,
                    )),
                    frame,
                )
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach playlist placeholder runtime: {e}"),
                })?;
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Fixture {
                continue;
            }
            let NodeDef::Fixture(config) = projected_node_config(registry, node)?.clone() else {
                continue;
            };
            match resolve_fixture_mapping(fs, registry, node, &config) {
                Ok(mapping) => {
                    runtime
                        .attach_runtime_node(
                            node.id,
                            Box::new(FixtureNode::new(
                                node.id,
                                mapping,
                                *config.sampling.value(),
                                frame,
                            )),
                            frame,
                        )
                        .map_err(|e| ProjectLoadError::InvalidProjectReference {
                            path: node_label(node),
                            reason: format!("attach fixture runtime: {e}"),
                        })?;
                    mark_node_status(runtime, node.id, frame, NodeRuntimeStatus::Ok);
                }
                Err(error) => {
                    let message = error.to_string();
                    mark_node_load_error(runtime, node.id, frame, message);
                }
            }
            register_source_binding(
                runtime,
                projected_nodes,
                node,
                "input",
                &config.bindings,
                frame,
            )?;
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "output",
                &config.bindings,
                frame,
            )?;
        }

        Ok(())
    }
}

fn should_attach_projected_node(
    node: &ProjectedNode,
    targets: Option<&VecSet<lpc_model::NodeUseLocation>>,
) -> bool {
    targets.is_none_or(|targets| targets.contains(&node.use_location))
}

fn mark_node_load_error(runtime: &mut Engine, node_id: NodeId, frame: Revision, message: String) {
    if let Some(entry) = runtime.tree_mut().get_mut(node_id) {
        entry.set_status(NodeRuntimeStatus::Error(message.clone()), frame);
        entry.set_state(NodeEntryState::Failed { reason: message }, frame);
    }
}

fn project_load_error_for_root_state(path: &LpPath, state: &NodeDefState) -> ProjectLoadError {
    match state {
        NodeDefState::NotFound | NodeDefState::Deleted | NodeDefState::ReadError { .. } => {
            ProjectLoadError::Io {
                path: path.as_str().to_string(),
                details: node_def_state_message(
                    &NodeDefLocation::artifact_root(lpc_model::ArtifactLocation::file(
                        path.as_str(),
                    )),
                    state,
                ),
            }
        }
        NodeDefState::ParseError(lpc_model::NodeDefParseError::UnknownKind { kind }) => {
            ProjectLoadError::UnknownKind {
                path: path.as_str().to_string(),
                suffix: kind.clone(),
            }
        }
        NodeDefState::ParseError(err) => ProjectLoadError::ProjectParse {
            file: path.as_str().to_string(),
            error: err.to_string(),
        },
        NodeDefState::ValidationError(err) => ProjectLoadError::ProjectParse {
            file: path.as_str().to_string(),
            error: err.message.clone(),
        },
        NodeDefState::Loaded(_) => ProjectLoadError::ProjectParse {
            file: path.as_str().to_string(),
            error: String::from("root artifact is not a Project"),
        },
    }
}

fn node_def_state_message(location: &NodeDefLocation, state: &NodeDefState) -> String {
    match state {
        NodeDefState::Loaded(_) => String::from("loaded"),
        NodeDefState::NotFound => format!("definition not found: {}", def_location_label(location)),
        NodeDefState::Deleted => format!("definition deleted: {}", def_location_label(location)),
        NodeDefState::ReadError { message } => {
            format!(
                "definition read error at {}: {message}",
                def_location_label(location)
            )
        }
        NodeDefState::ParseError(err) => {
            format!(
                "definition parse error at {}: {err}",
                def_location_label(location)
            )
        }
        NodeDefState::ValidationError(err) => {
            format!(
                "definition validation error at {}: {}",
                def_location_label(location),
                err.message
            )
        }
    }
}

fn mark_node_status(
    runtime: &mut Engine,
    node_id: NodeId,
    frame: Revision,
    status: NodeRuntimeStatus,
) {
    if let Some(entry) = runtime.tree_mut().get_mut(node_id) {
        entry.set_status(status, frame);
    }
}

fn projected_node_name_and_ownership(
    origin: &ProjectNodeOrigin,
    parent: NodeId,
    def_location: &NodeDefLocation,
) -> Result<(NodeName, ProjectedNodeOwnership), ProjectLoadError> {
    match origin {
        ProjectNodeOrigin::Root => Ok((
            NodeName::parse("project").map_err(|e| ProjectLoadError::InvalidNodeName {
                path: def_location_label(def_location),
                reason: e.to_string(),
            })?,
            ProjectedNodeOwnership::Root,
        )),
        ProjectNodeOrigin::Invocation { role, .. } => match role {
            ProjectNodePlacement::ProjectChild { name } => Ok((
                NodeName::parse(name).map_err(|e| ProjectLoadError::InvalidNodeName {
                    path: def_location_label(def_location),
                    reason: e.to_string(),
                })?,
                ProjectedNodeOwnership::ProjectChild,
            )),
            ProjectNodePlacement::PlaylistEntry { entry, name } => {
                let fallback = format!("entry_{entry}");
                Ok((
                    NodeName::parse(name.as_deref().unwrap_or(&fallback)).map_err(|e| {
                        ProjectLoadError::InvalidNodeName {
                            path: def_location_label(def_location),
                            reason: e.to_string(),
                        }
                    })?,
                    ProjectedNodeOwnership::PlaylistEntry {
                        playlist: parent,
                        entry: *entry,
                    },
                ))
            }
        },
    }
}

fn project_node_invocation(origin: &ProjectNodeOrigin) -> NodeInvocation {
    match origin {
        ProjectNodeOrigin::Root => NodeInvocation::Unset,
        ProjectNodeOrigin::Invocation { invocation, .. } => invocation.clone(),
    }
}

fn node_label(node: &ProjectedNode) -> String {
    def_location_label(&node.def_location)
}

fn def_location_label(location: &NodeDefLocation) -> String {
    location.artifact.file_path().as_str().to_string()
}

fn artifact_specifier_label(specifier: &ArtifactSpec) -> String {
    match specifier {
        ArtifactSpec::Path(path) => path.as_str().to_string(),
        ArtifactSpec::Lib(lib) => lib.to_string(),
    }
}

fn resolve_project_specifier(specifier: &ArtifactSpec) -> Result<LpPathBuf, ProjectLoadError> {
    resolve_path_specifier_from_dir(LpPath::new("/"), specifier)
}

fn resolve_path_specifier_from_dir(
    base_dir: &LpPath,
    specifier: &ArtifactSpec,
) -> Result<LpPathBuf, ProjectLoadError> {
    match specifier {
        ArtifactSpec::Path(path) => {
            if path.is_absolute() {
                Ok(path.clone())
            } else {
                base_dir
                    .to_path_buf()
                    .join_relative(path.as_str())
                    .ok_or_else(|| ProjectLoadError::InvalidProjectReference {
                        path: path.as_str().to_string(),
                        reason: format!("path cannot be resolved relative to {base_dir:?}"),
                    })
            }
        }
        ArtifactSpec::Lib(lib) => Err(ProjectLoadError::InvalidProjectReference {
            path: lib.to_string(),
            reason: String::from("library artifact specifiers are not supported for nodes yet"),
        }),
    }
}

fn playlist_runtime_entries(
    projected_nodes: &[ProjectedNode],
    playlist: NodeId,
    config: &PlaylistDef,
) -> Vec<PlaylistRuntimeEntry> {
    projected_nodes
        .iter()
        .filter_map(|node| match node.ownership {
            ProjectedNodeOwnership::PlaylistEntry {
                playlist: owner,
                entry,
            } if owner == playlist => Some(PlaylistRuntimeEntry {
                index: entry,
                child: node.id,
                output_slot: SlotPath::parse("output").expect("playlist child output path"),
                duration: config
                    .entries
                    .entries
                    .get(&entry)
                    .and_then(|entry| entry.duration.data.as_ref())
                    .map(|duration| duration.value().0),
                fade_after: config
                    .entries
                    .entries
                    .get(&entry)
                    .and_then(|entry| entry.fade_after.data.as_ref())
                    .map(|fade| fade.value().0),
            }),
            _ => None,
        })
        .collect()
}

fn playlist_entry_trigger_sources(
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    config: &PlaylistDef,
) -> Result<Vec<(u32, BindingSource)>, ProjectLoadError> {
    let mut sources = Vec::new();
    for (entry_index, entry) in &config.entries.entries {
        let Some(source) = binding_source(&entry.bindings, "trigger") else {
            continue;
        };
        sources.push((
            *entry_index,
            binding_source_endpoint(projected_nodes, current, source)?,
        ));
    }
    Ok(sources)
}

fn resolve_fixture_mapping(
    fs: &dyn LpFs,
    registry: &mut ProjectRegistry,
    node: &ProjectedNode,
    config: &FixtureDef,
) -> Result<MappingConfig, ProjectLoadError> {
    match config.mapping.value() {
        MappingConfig::SvgPath {
            sample_diameter, ..
        } => {
            let svg = materialize_node_text_asset(
                fs,
                registry,
                node,
                AssetContentType::FixtureSvg,
                "fixture SVG",
            )?;
            resolve_svg_path_mapping(
                &svg.text,
                config.render_width(),
                config.render_height(),
                sample_diameter.value().0,
            )
            .map_err(|e| ProjectLoadError::InvalidProjectReference {
                path: node_label(node),
                reason: format!("resolve svg fixture mapping: {e}"),
            })
        }
        other => Ok(other.clone()),
    }
}

fn node_kind_name(
    config: &NodeDef,
    location: &NodeDefLocation,
) -> Result<NodeName, ProjectLoadError> {
    let name = match config {
        NodeDef::ComputeShader(_) => "compute_shader",
        NodeDef::ControlRadio(_) => "control_radio",
        NodeDef::Shader(_) => "shader",
        _ => config.kind_name(),
    };
    NodeName::parse(name).map_err(|e| ProjectLoadError::InvalidNodeName {
        path: def_location_label(location),
        reason: format!("{e}"),
    })
}

fn projected_node_config<'a>(
    registry: &'a ProjectRegistry,
    node: &ProjectedNode,
) -> Result<&'a NodeDef, ProjectLoadError> {
    let entry = registry.def(&node.def_location).ok_or_else(|| {
        ProjectLoadError::InvalidProjectReference {
            path: node_label(node),
            reason: format!("missing definition payload for node {:?}", node.id),
        }
    })?;
    match &entry.state {
        NodeDefState::Loaded(def) => Ok(def),
        other => Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(node),
            reason: format!("definition payload is not loaded: {other:?}"),
        }),
    }
}

fn materialize_node_text_asset(
    fs: &dyn LpFs,
    registry: &mut ProjectRegistry,
    node: &ProjectedNode,
    content_type: AssetContentType,
    label: &str,
) -> Result<AssetText, ProjectLoadError> {
    let source = asset_for_node_content_type(registry, node, content_type)?;
    registry.materialize_asset_text(fs, &source).map_err(|e| {
        ProjectLoadError::InvalidProjectReference {
            path: node_label(node),
            reason: format!("materialize {label}: {e:?}"),
        }
    })
}

fn asset_for_node_content_type(
    registry: &ProjectRegistry,
    node: &ProjectedNode,
    content_type: AssetContentType,
) -> Result<AssetLocation, ProjectLoadError> {
    let mut matches = Vec::new();
    for (source, consumers) in &registry.inventory().tree.asset_consumers {
        if !consumers
            .iter()
            .any(|consumer| consumer == &node.use_location)
        {
            continue;
        }
        let Some(entry) = registry.asset(source) else {
            continue;
        };
        if entry.content_type == content_type {
            matches.push(source.clone());
        }
    }

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(node),
            reason: format!("node has no referenced {content_type:?} asset"),
        }),
        _ => Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(node),
            reason: format!("node has multiple referenced {content_type:?} assets"),
        }),
    }
}

fn node_provides_default_time_bus(config: &NodeDef) -> bool {
    match config {
        NodeDef::Clock(config) => {
            binding_target(&config.bindings, "seconds").is_none_or(is_time_seconds_bus_target)
        }
        _ => false,
    }
}

fn resolve_node_loc<'a>(
    projected_nodes: &'a [ProjectedNode],
    current: &'a ProjectedNode,
    loc: &lpc_model::RelativeNodeRef,
    expected: &str,
) -> Result<&'a ProjectedNode, ProjectLoadError> {
    resolve_relative_node_ref(projected_nodes, current, loc).ok_or_else(|| {
        ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("unknown {expected} node ref `{loc}`"),
        }
    })
}

fn resolve_relative_node_ref<'a>(
    projected_nodes: &'a [ProjectedNode],
    current: &'a ProjectedNode,
    parsed: &lpc_model::RelativeNodeRef,
) -> Option<&'a ProjectedNode> {
    let mut node = Some(current);
    let mut virtual_parent = None;
    for _ in 0..parsed.parent_hops() {
        let parent = node?.parent?;
        if let Some(parent_node) = projected_nodes
            .iter()
            .find(|candidate| candidate.id == parent)
        {
            node = Some(parent_node);
            virtual_parent = None;
        } else {
            node = None;
            virtual_parent = Some(parent);
        }
    }
    for segment in parsed.segments() {
        let parent = node.map(|node| node.id).or(virtual_parent)?;
        node = projected_nodes
            .iter()
            .find(|candidate| candidate.parent == Some(parent) && &candidate.name == segment);
        virtual_parent = None;
    }
    node
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
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let source = binding_source(bindings, slot_name).ok_or_else(|| {
        ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("{slot_name} source binding is missing"),
        }
    })?;
    let source = binding_source_endpoint(projected_nodes, current, source)?;
    let target_slot =
        SlotPath::parse(slot_name).map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("invalid target slot `{slot_name}`: {e}"),
        })?;
    register_source_binding_at_path(engine, current, slot_name, source, target_slot, frame)
}

fn register_source_binding_at_path(
    engine: &mut Engine,
    current: &ProjectedNode,
    binding_slot_name: &str,
    source: BindingSource,
    target_slot: SlotPath,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    engine
        .add_binding(
            BindingDraft {
                source,
                target: BindingTarget::ConsumedSlot {
                    node: current.id,
                    slot: target_slot,
                },
                priority: BindingPriority::new(0),
                kind: binding_kind_for_slot(binding_slot_name),
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register {binding_slot_name} source binding: {e}"),
        })?;
    Ok(())
}

fn register_optional_source_binding(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if binding_source(bindings, slot_name).is_none() {
        return Ok(());
    }
    register_source_binding(engine, projected_nodes, current, slot_name, bindings, frame)
}

fn register_target_binding(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let Some(target) = binding_target(bindings, slot_name) else {
        return Ok(());
    };
    let target = binding_target_endpoint(projected_nodes, current, target)?;
    let source_slot =
        SlotPath::parse(slot_name).map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
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
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register {slot_name} target binding: {e}"),
        })?;
    Ok(())
}

fn register_visual_default_output_binding(
    engine: &mut Engine,
    current: &ProjectedNode,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if current.ownership.suppress_visual_default_output()
        || binding_target(bindings, "output").is_some()
    {
        return Ok(());
    }
    add_visual_default_output_binding(engine, current, frame)
}

fn add_visual_default_output_binding(
    engine: &mut Engine,
    current: &ProjectedNode,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    engine
        .add_binding(
            BindingDraft {
                source: BindingSource::ProducedSlot {
                    node: current.id,
                    slot: SlotPath::parse("output").expect("visual output slot path"),
                },
                target: BindingTarget::BusChannel(ChannelName(String::from("visual.out"))),
                priority: BindingPriority::default_fallback(),
                kind: Kind::Color,
                owner: current.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register visual default output binding: {e}"),
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
    current: &ProjectedNode,
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
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register clock default time binding: {e}"),
        })?;
    Ok(())
}

fn shader_needs_default_time_binding(config: &ShaderDef) -> bool {
    if binding_source(&config.bindings, "time").is_some() {
        return false;
    }
    let Some(slot) = config.consumed_slots.entries.get("time") else {
        return false;
    };
    *slot.kind.value() == ShaderSlotKind::Value
        && slot.value.value().as_lp_type() == Some(LpType::F32)
}

fn add_visual_default_time_binding(
    engine: &mut Engine,
    current: &ProjectedNode,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
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
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register visual shader default time binding: {e}"),
        })?;
    Ok(())
}

fn register_fluid_default_time_binding(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    config: &FluidDef,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if binding_source(&config.bindings, "time").is_some() || !has_default_time_bus(projected_nodes)
    {
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
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register fluid default time binding: {e}"),
        })?;
    Ok(())
}

fn has_default_time_bus(projected_nodes: &[ProjectedNode]) -> bool {
    projected_nodes
        .iter()
        .any(|node| node.provides_default_time_bus)
}

fn is_time_seconds_bus_target(target: &AuthoredBindingRef) -> bool {
    matches!(target, AuthoredBindingRef::Bus(bus) if bus.slot().to_string() == "time.seconds")
}

fn binding_source_endpoint(
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    endpoint: AuthoredBindingSource<'_>,
) -> Result<BindingSource, ProjectLoadError> {
    match endpoint {
        AuthoredBindingSource::Value(value) => Ok(BindingSource::Literal(value.clone())),
        AuthoredBindingSource::Ref(binding_ref) => {
            binding_ref_source(projected_nodes, current, binding_ref)
        }
    }
}

fn binding_ref_source(
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    binding_ref: &AuthoredBindingRef,
) -> Result<BindingSource, ProjectLoadError> {
    match binding_ref {
        AuthoredBindingRef::Unset => Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: String::from("binding source cannot be unset"),
        }),
        AuthoredBindingRef::Bus(bus) => Ok(BindingSource::BusChannel(ChannelName(
            bus.slot().to_string(),
        ))),
        AuthoredBindingRef::Node(node_slot) => {
            let node =
                resolve_node_loc(projected_nodes, current, node_slot.node(), "binding source")?;
            Ok(BindingSource::ProducedSlot {
                node: node.id,
                slot: node_slot.slot().clone(),
            })
        }
    }
}

fn binding_target_endpoint(
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    endpoint: &AuthoredBindingRef,
) -> Result<BindingTarget, ProjectLoadError> {
    match endpoint {
        AuthoredBindingRef::Unset => Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: String::from("binding target cannot be unset"),
        }),
        AuthoredBindingRef::Bus(bus) => Ok(BindingTarget::BusChannel(ChannelName(
            bus.slot().to_string(),
        ))),
        AuthoredBindingRef::Node(node_slot) => {
            let node =
                resolve_node_loc(projected_nodes, current, node_slot.node(), "binding target")?;
            Ok(BindingTarget::ConsumedSlot {
                node: node.id,
                slot: node_slot.slot().clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::cell::Cell;

    use alloc::rc::Rc;
    use alloc::sync::Arc;
    use lpc_hardware::{
        HardwareSystem, HwAddress, HwRegistry, VirtualButtonDriver, VirtualRadioDriver,
        default_esp32c6_hardware_manifest,
    };
    use lpc_model::{
        ArtifactLocation, NodeDefLocation, NodeName, ProductRef, SlotData, SlotMapKey, TreePath,
    };
    use lpc_shared::time::TimeProvider;
    use lpc_wire::{
        ProjectProbeRequest, ProjectProbeResult, ProjectReadRequest, RenderProductProbeRequest,
        RenderProductProbeResult, WireTextureFormat,
    };
    use lpfs::lp_path::AsLpPath;
    use lpfs::{LpFs, LpFsMemory, LpFsStd};
    use lps_shared::TextureStorageFormat;

    use super::*;
    use crate::dataflow::binding::{BindingPriority, BindingSource, BindingTarget};
    use crate::dataflow::resolver::{Production, QueryKey, ResolveLogLevel};
    use crate::engine::test_support::{read_into_view, read_probe_results};
    use crate::engine::{ButtonService, RadioService};
    use crate::products::visual::RenderTextureRequest;

    fn node_for_def_path(rt: &Engine, path: &str) -> Option<NodeId> {
        let location = NodeDefLocation::artifact_root(ArtifactLocation::file(path));
        rt.project_runtime_index()
            .runtime_nodes_for_def(&location)
            .first()
            .copied()
    }

    fn flat_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        write_flat_basic_files(&fs);
        fs
    }

    fn svg_fixture_project(svg: &[u8]) -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "fixture": {
      "ref": "./fixture.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "render_size": {
    "width": 20,
    "height": 10
  },
  "sampling": "direct",
  "bindings": {
    "input": {
      "source": "bus#visual.out"
    },
    "output": {
      "target": "bus#control.out"
    }
  },
  "mapping": {
    "kind": "SvgPath",
    "source": "./mapping.svg",
    "sample_diameter": 2.0
  }
}
"#,
        )
        .expect("fixture.json");
        fs.write_file("/mapping.svg".as_path(), svg)
            .expect("mapping.svg");
        fs
    }

    #[test]
    fn fixture_svg_path_mapping_loads_from_project() {
        let fs = svg_fixture_project(
            br#"
<svg viewBox="0 0 20 10">
  <g><polyline points="0 0 10 0"/><text>path:2,count:2</text></g>
  <g><path d="M10,0 L20,0"/><text><tspan>path:1,count:3</tspan></text></g>
</svg>
"#,
        );

        let services = EngineServices::new(TreePath::parse("/svg_fixture.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load svg fixture project");
        assert!(node_for_def_path(&rt, "/fixture.json").is_some());
    }

    #[test]
    fn fixture_svg_path_mapping_rejects_ungrouped_mapping_text() {
        let fs = svg_fixture_project(
            br#"
<svg viewBox="0 0 20 10">
  <path d="M0,0 L10,0"/>
  <text>path:1,count:3</text>
</svg>
"#,
        );

        let services = EngineServices::new(TreePath::parse("/svg_fixture.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load with bad fixture");
        assert_fixture_node_error(&rt, "not inside a valid group");
    }

    #[test]
    fn fixture_svg_path_mapping_rejects_curve_commands() {
        let fs = svg_fixture_project(
            br#"
<svg viewBox="0 0 20 10">
  <g><path d="M0,0 C1,1 2,2 3,3"/><text>path:1,count:3</text></g>
</svg>
"#,
        );

        let services = EngineServices::new(TreePath::parse("/svg_fixture.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load with bad fixture");
        assert_fixture_node_error(&rt, "unsupported SVG path command");
    }

    fn assert_fixture_node_error(rt: &Engine, expected: &str) {
        assert_node_for_def_error(rt, "/fixture.json", expected);
    }

    fn assert_node_for_def_error(rt: &Engine, path: &str, expected: &str) {
        let node = node_for_def_path(rt, path).expect("runtime node");
        let entry = rt.tree().get(node).expect("runtime entry");
        assert!(matches!(
            entry.status.value(),
            NodeRuntimeStatus::Error(message) if message.contains(expected)
        ));
        assert!(matches!(
            entry.state.value(),
            NodeEntryState::Failed { reason } if reason.contains(expected)
        ));
    }

    fn playlist_project_fs() -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "playlist": {
      "ref": "./playlist.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/playlist.json".as_path(),
            br#"
{
  "kind": "Playlist",
  "default_fade": 0.35,
  "entries": {
    "1": {
      "name": "idle",
      "node": {
        "ref": "./idle.json"
      }
    },
    "2": {
      "name": "active",
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
      },
      "bindings": {
        "trigger": {
          "source": "bus#trigger"
        }
      }
    }
  }
}
"#,
        )
        .expect("playlist.json");
        fs.write_file(
            "/idle.json".as_path(),
            br#"
{
  "kind": "Shader",
  "source": {
    "path": "idle.glsl"
  }
}
"#,
        )
        .expect("idle.json");
        fs.write_file(
            "/active.json".as_path(),
            br#"
{
  "kind": "Shader",
  "source": {
    "path": "active.glsl"
  },
  "bindings": {
    "time": {
      "source": "..#entry_time"
    }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0.0
    }
  }
}
"#,
        )
        .expect("active.json");
        fs.write_file(
            "/idle.glsl".as_path(),
            b"vec4 render(vec2 pos) { return vec4(0.0, pos, 1.0); }",
        )
        .expect("idle.glsl");
        fs.write_file(
            "/active.glsl".as_path(),
            b"vec4 render(vec2 pos) { return vec4(time, pos.x, pos.y, 1.0); }",
        )
        .expect("active.glsl");
        fs
    }

    fn button_playlist_project_fs() -> LpFsMemory {
        let fs = playlist_project_fs();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "button": {
      "ref": "./button.json"
    },
    "playlist": {
      "ref": "./playlist.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/clock.json".as_path(),
            br#"{
  "kind": "Clock"
}"#,
        )
        .expect("clock.json");
        fs.write_file(
            "/button.json".as_path(),
            br#"
{
  "kind": "Button",
  "endpoint": "button:gpio:D9",
  "stable_ms": 1,
  "bindings": {
    "down": {
      "target": "bus#trigger"
    }
  }
}
"#,
        )
        .expect("button.json");
        fs.write_file(
            "/playlist.json".as_path(),
            br#"
{
  "kind": "Playlist",
  "default_fade": 0.35,
  "bindings": {
    "time": {
      "source": "bus#time.seconds"
    }
  },
  "entries": {
    "1": {
      "name": "idle",
      "node": {
        "ref": "./idle.json"
      }
    },
    "2": {
      "name": "active",
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
      },
      "bindings": {
        "trigger": {
          "source": "bus#trigger"
        }
      }
    }
  }
}
"#,
        )
        .expect("playlist.json");
        fs
    }

    fn examples_fluid_fs() -> LpFsStd {
        LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/fluid"))
    }

    fn examples_events_fs() -> LpFsStd {
        LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/events"))
    }

    fn examples_button_playlist_fs() -> LpFsStd {
        LpFsStd::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/button-playlist"),
        )
    }

    fn examples_button_sign_fs() -> LpFsStd {
        LpFsStd::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/button-sign"),
        )
    }

    fn examples_fyeah_sign_fs() -> LpFsStd {
        LpFsStd::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/fyeah-sign"),
        )
    }

    fn examples_fyeah_button_fs() -> LpFsStd {
        LpFsStd::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/fyeah-button"),
        )
    }

    #[test]
    fn project_json_loads_into_runtime_with_expected_nodes() {
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

        assert_eq!(node_for_def_path(&rt, "/texture.json"), Some(tex_id));

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
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "shader": {
      "ref": "./shader.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/clock.json".as_path(),
            br#"{
  "kind": "Clock"
}"#,
        )
        .expect("clock.json");
        fs.write_file(
            "/shader.json".as_path(),
            br#"
{
  "kind": "Shader",
  "source": {
    "path": "shader.glsl"
  },
  "render_order": 0,
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0.0
    }
  }
}
"#,
        )
        .expect("shader.json");
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
        let first = rt
            .resolve_with_engine_host(
                QueryKey::Bus(ChannelName(String::from("time.seconds"))),
                ResolveLogLevel::Off,
            )
            .expect("resolve time bus")
            .0;
        assert_eq!(
            *first.value_leaf().expect("time value").value(),
            LpValue::F32(0.0)
        );
        let shader_first = rt
            .resolve_with_engine_host(
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
        let second = rt
            .resolve_with_engine_host(
                QueryKey::Bus(ChannelName(String::from("time.seconds"))),
                ResolveLogLevel::Off,
            )
            .expect("resolve time bus")
            .0;
        assert_eq!(
            *second.value_leaf().expect("time value").value(),
            LpValue::F32(1.0)
        );
        let shader_second = rt
            .resolve_with_engine_host(
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
    fn project_loader_rejects_inline_child_def() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "shader": {
      "def": {
        "kind": "Shader",
        "source": "shader.glsl"
      }
    }
  }
}
"#,
        )
        .expect("project.json");

        let services = EngineServices::new(TreePath::parse("/inline.show").expect("path"));
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(err) => err,
            Ok(_) => panic!("inline child definitions are not supported"),
        };
        assert!(format!("{err:?}").contains("def"), "{err:?}");
    }
    #[test]
    fn top_level_shader_gets_default_visual_output_binding() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "shader": {
      "ref": "./shader.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/shader.json".as_path(),
            br#"
{
  "kind": "Shader",
  "source": {
    "path": "shader.glsl"
  }
}
"#,
        )
        .expect("shader.json");
        fs.write_file(
            "/shader.glsl".as_path(),
            b"vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
        )
        .expect("shader.glsl");

        let services = EngineServices::new(TreePath::parse("/default_visual.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        let shader = node_for_def_path(&rt, "/shader.json").expect("shader node");

        assert!(rt.tree().bindings().any(|binding| {
            matches!(
                (&binding.source, &binding.target),
                (
                    BindingSource::ProducedSlot { node, slot },
                    BindingTarget::BusChannel(channel),
                ) if *node == shader
                    && slot == &SlotPath::parse("output").expect("output")
                    && channel.0 == "visual.out"
                    && binding.priority == BindingPriority::default_fallback()
            )
        }));
    }

    #[test]
    fn top_level_sibling_node_refs_resolve_through_root() {
        let fs = flat_project();
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "color_order": "rgb",
  "brightness": 255,
  "gamma_correction": false,
  "transform": [
    [
      1.0,
      0.0,
      0.0
    ],
    [
      0.0,
      1.0,
      0.0
    ],
    [
      0.0,
      0.0,
      1.0
    ]
  ],
  "bindings": {
    "input": {
      "source": "..texture#output"
    },
    "output": {
      "target": "bus#control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "RingArray",
        "center": [
          0.5,
          0.5
        ],
        "diameter": 1.0,
        "start_ring_inclusive": 0,
        "end_ring_exclusive": 1,
        "offset_angle": 0.0,
        "order": "inner_first",
        "ring_lamp_counts": {
          "0": 1
        }
      }
    }
  }
}
"#,
        )
        .expect("fixture.json");

        let services = EngineServices::new(TreePath::parse("/sibling_ref.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        let texture = node_for_def_path(&rt, "/texture.json").expect("texture node");
        let fixture = node_for_def_path(&rt, "/fixture.json").expect("fixture node");

        assert!(rt.tree().bindings().any(|binding| {
            matches!(
                (&binding.source, &binding.target),
                (
                    BindingSource::ProducedSlot { node, slot },
                    BindingTarget::ConsumedSlot { node: target, slot: target_slot },
                ) if *node == texture
                    && slot == &SlotPath::parse("output").expect("output")
                    && *target == fixture
                    && target_slot == &SlotPath::parse("input").expect("input")
            )
        }));
    }

    #[test]
    fn playlist_entry_children_do_not_get_default_visual_output_binding() {
        let fs = playlist_project_fs();
        let services = EngineServices::new(TreePath::parse("/playlist.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load playlist");
        let root = rt.tree().root();
        let playlist = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("playlist").unwrap())
            .expect("playlist");
        let active = rt
            .tree()
            .lookup_sibling(playlist, NodeName::parse("active").unwrap())
            .expect("active");

        assert!(!rt.tree().bindings().any(|binding| {
            matches!(
                (&binding.source, &binding.target),
                (
                    BindingSource::ProducedSlot { node, slot },
                    BindingTarget::BusChannel(channel),
                ) if *node == active
                    && slot == &SlotPath::parse("output").expect("output")
                    && channel.0 == "visual.out"
                    && binding.priority == BindingPriority::default_fallback()
            )
        }));
    }

    #[test]
    fn playlist_entries_load_as_children_and_bind_entry_trigger() {
        let fs = playlist_project_fs();
        let services = EngineServices::new(TreePath::parse("/playlist.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load playlist");
        let root = rt.tree().root();
        let playlist = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("playlist").unwrap())
            .expect("playlist");
        let idle = rt
            .tree()
            .lookup_sibling(playlist, NodeName::parse("idle").unwrap())
            .expect("idle");
        let active = rt
            .tree()
            .lookup_sibling(playlist, NodeName::parse("active").unwrap())
            .expect("active");

        assert_eq!(rt.tree().get(idle).expect("idle").parent, Some(playlist));
        assert_eq!(
            rt.tree().get(active).expect("active").parent,
            Some(playlist)
        );
        assert!(rt.tree().bindings().any(|binding| {
            matches!(
                (&binding.source, &binding.target),
                (
                    BindingSource::BusChannel(source),
                    BindingTarget::ConsumedSlot { node, slot },
                ) if source.0 == "trigger"
                    && *node == playlist
                    && slot == &SlotPath::parse("entries[2].trigger").expect("trigger")
                    && binding.priority == BindingPriority::authored()
            )
        }));
    }

    #[test]
    fn playlist_entry_trigger_restarts_active_entry_and_returns_idle() {
        let fs = button_playlist_project_fs();
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let control = driver.clone();
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(driver));
        let hardware = Rc::new(hardware);
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let mut services = EngineServices::new(TreePath::parse("/button_playlist.show").unwrap());
        services.set_button_service(Some(button_service));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load playlist");
        let playlist = rt
            .tree()
            .lookup_sibling(rt.tree().root(), NodeName::parse("playlist").unwrap())
            .expect("playlist");

        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            -1.0
        );

        control.set_pressed(HwAddress::gpio(20), true);
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 2);
        assert_eq!(resolve_playlist_f32(&mut rt, playlist, "entry_time"), 0.0);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            0.0
        );

        rt.tick(1000).expect("advance time");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 2);
        assert!(resolve_playlist_f32(&mut rt, playlist, "entry_time") >= 1.0);
        assert!(resolve_playlist_f32(&mut rt, playlist, "entry_progress") >= 0.25);

        control.set_pressed(HwAddress::gpio(20), false);
        let _ = resolve_playlist_u32(&mut rt, playlist, "active_entry");
        let _ = resolve_playlist_u32(&mut rt, playlist, "active_entry");
        control.set_pressed(HwAddress::gpio(20), true);
        let _ = resolve_playlist_u32(&mut rt, playlist, "active_entry");
        let _ = resolve_playlist_u32(&mut rt, playlist, "active_entry");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 2);
        assert_eq!(resolve_playlist_f32(&mut rt, playlist, "entry_time"), 0.0);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            0.0
        );

        rt.tick(5000).expect("advance past duration");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            -1.0
        );
    }

    #[test]
    fn malformed_child_node_json_projects_error_node() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "broken": {
      "ref": "./broken.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file("/broken.json".as_path(), b"not valid json {{{")
            .expect("broken.json");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load project");

        assert_node_for_def_error(&rt, "/broken.json", "parse error");
    }

    #[test]
    fn missing_project_json_returns_io_error() {
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
    fn unknown_child_kind_projects_error_node() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "weird": {
      "ref": "./weird.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/weird.json".as_path(),
            br#"{
  "kind": "banana"
}"#,
        )
        .expect("weird.json");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load project");

        assert_node_for_def_error(&rt, "/weird.json", "unknown node kind");
    }

    #[test]
    fn missing_sibling_node_loc_names_missing_ref() {
        let fs = flat_project();
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "color_order": "rgb",
  "brightness": 255,
  "gamma_correction": false,
  "transform": [
    [
      1.0,
      0.0,
      0.0
    ],
    [
      1.0,
      1.0,
      0.0
    ],
    [
      0.0,
      0.0,
      1.0
    ]
  ],
  "bindings": {
    "input": {
      "source": "..missing#output"
    },
    "output": {
      "target": "bus#control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "RingArray",
        "center": [
          0.5,
          0.5
        ],
        "diameter": 1.0,
        "start_ring_inclusive": 0,
        "end_ring_exclusive": 1,
        "offset_angle": 0.0,
        "order": "inner_first",
        "ring_lamp_counts": {
          "0": 1
        }
      }
    }
  }
}
"#,
        )
        .expect("fixture.json");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(
                err,
                ProjectLoadError::InvalidProjectReference { ref reason, .. }
                    if reason.contains("unknown binding source node ref `..missing`")
            ),
            "expected missing binding source ref, got {err:?}"
        );
    }

    #[test]
    fn slash_node_ref_projects_error_node() {
        let fs = flat_project();
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "color_order": "rgb",
  "brightness": 255,
  "gamma_correction": false,
  "transform": [
    [
      1.0,
      0.0,
      0.0
    ],
    [
      1.0,
      1.0,
      0.0
    ],
    [
      0.0,
      0.0,
      1.0
    ]
  ],
  "bindings": {
    "input": {
      "source": "/texture#output"
    },
    "output": {
      "target": "bus#control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "RingArray",
        "center": [
          0.5,
          0.5
        ],
        "diameter": 1.0,
        "start_ring_inclusive": 0,
        "end_ring_exclusive": 1,
        "offset_angle": 0.0,
        "order": "inner_first",
        "ring_lamp_counts": {
          "0": 1
        }
      }
    }
  }
}
"#,
        )
        .expect("fixture.json");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load project");

        assert_node_for_def_error(&rt, "/fixture.json", "node locations use dot syntax");
    }

    #[test]
    fn project_loader_attaches_compute_shader_node() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "compute": {
      "ref": "./compute.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/compute.json".as_path(),
            br#"
{
  "kind": "ComputeShader",
  "source": {
    "path": "compute.glsl"
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0.25
    }
  },
  "produced": {
    "phase": {
      "kind": "value",
      "value": "f32"
    }
  }
}
"#,
        )
        .expect("compute.json");
        fs.write_file(
            "/compute.glsl".as_path(),
            b"void tick() { phase = time + 2.0; }",
        )
        .expect("compute.glsl");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let node = node_for_def_path(&rt, "/compute.json").expect("compute node");

        let production = rt
            .resolve_with_engine_host(
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

        let (emitters, _) = rt
            .resolve_with_engine_host(
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

        let (fluid_output, _) = rt
            .resolve_with_engine_host(
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

        // Read state through the same event-stream + progressive-apply path the
        // live clients use (the aggregate response was deleted in M6/P5).
        let (mut engine, registry) = rt.into_parts();

        let probe_results = read_probe_results(
            &mut engine,
            &registry,
            ProjectReadRequest {
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
            },
        );
        let Some(ProjectProbeResult::RenderProduct(RenderProductProbeResult::Texture {
            format,
            bytes,
            ..
        })) = probe_results.first()
        else {
            panic!("fluid visual probe should return a texture");
        };
        assert_eq!(*format, WireTextureFormat::Srgb8);
        assert_eq!(bytes.len(), 16 * 16 * 3);
        assert!(
            bytes.iter().any(|byte| *byte != 0),
            "fluid visual probe should contain nonzero display bytes"
        );

        let (view, _) = read_into_view(
            &mut engine,
            &registry,
            ProjectReadRequest::default_debug(None),
        );
        assert!(
            view.slots
                .roots
                .contains_key(&format!("node.{}.state", compute.0)),
            "compute state should be visible in debug read"
        );
        assert!(
            view.slots
                .roots
                .contains_key(&format!("node.{}.state", fluid.0)),
            "fluid state should be visible in debug read"
        );
    }

    // Previously quarantined as a "render/JIT data race" because it rendered black
    // under heavy parallel load (`just ci`). Root cause: "black" is a *swallowed
    // shader-compile failure* — `ShaderNode::ensure_compiled` fills the target with
    // zeros and returns Ok when compilation fails (shader_node.rs), and a compile
    // panic is caught and funneled to the same fallback. The brightness assertion
    // below now surfaces any such compile/runtime error instead of an opaque black
    // frame, so a future flake reports the real cranelift/frontend message.
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
        let (shader_output, _) = rt
            .resolve_with_engine_host(
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
        assert_bright_event_pixels(&mut rt, &first);

        rt.tick(500).expect("advance trigger graph");
        let second = render_test_texture_bytes(&mut rt, *product);
        assert_bright_event_pixels(&mut rt, &second);
        assert_ne!(
            first, second,
            "event example should blink as scheduled events fire and clear"
        );
    }

    #[test]
    fn button_playlist_example_renders_idle_and_active_after_press() {
        let fs = examples_button_playlist_fs();
        let fs: &dyn LpFs = &fs;
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let control = driver.clone();
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(driver));
        let hardware = Rc::new(hardware);
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let time = Rc::new(TestTimeProvider::new());
        let time_provider: Rc<dyn TimeProvider> = time.clone();
        let mut services =
            EngineServices::new(TreePath::parse("/button_playlist.show").expect("path"));
        services.set_button_service(Some(button_service));
        services.set_time_provider(Some(time_provider));

        let mut rt =
            ProjectLoader::load_from_root(fs, services).expect("load button playlist example");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));
        let root = rt.tree().root();
        let playlist = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("playlist").unwrap())
            .expect("playlist node");

        tick_with_test_time(&mut rt, &time, 16, "tick idle graph");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            -1.0
        );
        let idle_product = resolve_visual_product(&mut rt, playlist, "output");
        let idle = render_test_texture_bytes(&mut rt, idle_product);
        assert_nonzero_rgb(&idle, "idle playlist visual");

        control.set_pressed(HwAddress::gpio(20), true);
        tick_with_test_time(&mut rt, &time, 16, "press candidate");
        tick_with_test_time(&mut rt, &time, 30, "press stable");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 2);
        assert_eq!(resolve_playlist_f32(&mut rt, playlist, "entry_time"), 0.0);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            0.0
        );
        let active_product = resolve_visual_product(&mut rt, playlist, "output");
        let active = render_test_texture_bytes(&mut rt, active_product);
        assert_nonzero_rgb(&active, "active playlist visual");
        assert_ne!(idle, active, "active trigger should change the visual");

        tick_with_test_time(&mut rt, &time, 1000, "advance active");
        assert!(resolve_playlist_f32(&mut rt, playlist, "entry_time") >= 1.0);
        assert!(resolve_playlist_f32(&mut rt, playlist, "entry_progress") >= 0.25);

        control.set_pressed(HwAddress::gpio(20), false);
        tick_with_test_time(&mut rt, &time, 16, "release candidate");
        tick_with_test_time(&mut rt, &time, 30, "release stable");
        control.set_pressed(HwAddress::gpio(20), true);
        tick_with_test_time(&mut rt, &time, 16, "second press candidate");
        tick_with_test_time(&mut rt, &time, 30, "second press stable");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 2);
        assert_eq!(resolve_playlist_f32(&mut rt, playlist, "entry_time"), 0.0);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            0.0
        );

        tick_with_test_time(&mut rt, &time, 5000, "advance past active duration");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        assert_eq!(
            resolve_playlist_f32(&mut rt, playlist, "entry_progress"),
            -1.0
        );
    }

    #[test]
    fn button_sign_example_loads_with_control_radio_node() {
        let fs = examples_button_sign_fs();
        let fs: &dyn LpFs = &fs;
        let services = EngineServices::new(TreePath::parse("/button_sign.show").expect("path"));

        let rt = ProjectLoader::load_from_root(fs, services).expect("load button sign example");
        let root = rt.tree().root();
        let radio = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("radio").unwrap())
            .expect("radio node");

        assert!(
            rt.tree()
                .get(radio)
                .expect("radio")
                .state
                .value()
                .is_alive()
        );
        assert!(
            rt.demand_roots().contains(&radio),
            "radio must be a demand root"
        );
        assert!(rt.tree().bindings().any(|binding| {
            matches!(
                (&binding.source, &binding.target),
                (
                    BindingSource::BusChannel(source),
                    BindingTarget::ConsumedSlot { node, slot },
                ) if source.0 == "trigger"
                    && *node == radio
                    && slot == &SlotPath::parse("input").expect("input")
            )
        }));
        assert!(rt.tree().bindings().any(|binding| {
            matches!(
                (&binding.source, &binding.target),
                (
                    BindingSource::ProducedSlot { node, slot },
                    BindingTarget::BusChannel(target),
                ) if *node == radio
                    && slot == &SlotPath::parse("output").expect("output")
                    && target.0 == "trigger"
            )
        }));
    }

    #[test]
    fn button_sign_example_ticks_without_radio_trigger_cycle() {
        let fs = examples_button_sign_fs();
        let fs: &dyn LpFs = &fs;
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let hardware = Rc::new(HardwareSystem::with_virtual_drivers(registry));
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware.clone();
        let mut services = EngineServices::new(TreePath::parse("/button_sign.show").expect("path"));
        services.set_button_service(Some(button_service));
        services.set_radio_service(Some(radio_service));

        let mut rt = ProjectLoader::load_from_root(fs, services).expect("load button sign example");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));

        rt.tick(16).expect("tick button-sign without radio cycle");
    }

    #[test]
    fn fyeah_sign_example_ticks_without_radio_trigger_cycle() {
        let fs = examples_fyeah_sign_fs();
        let fs: &dyn LpFs = &fs;
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let hardware = Rc::new(HardwareSystem::with_virtual_drivers(registry));
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware.clone();
        let mut services = EngineServices::new(TreePath::parse("/fyeah_sign.show").expect("path"));
        services.set_button_service(Some(button_service));
        services.set_radio_service(Some(radio_service));

        let mut rt = ProjectLoader::load_from_root(fs, services).expect("load fyeah sign example");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));

        rt.tick(16).expect("tick fyeah-sign without radio cycle");
    }

    #[test]
    fn fyeah_button_example_ticks_without_radio_trigger_cycle() {
        let fs = examples_fyeah_button_fs();
        let fs: &dyn LpFs = &fs;
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let hardware = Rc::new(HardwareSystem::with_virtual_drivers(registry));
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware.clone();
        let mut services =
            EngineServices::new(TreePath::parse("/fyeah_button.show").expect("path"));
        services.set_button_service(Some(button_service));
        services.set_radio_service(Some(radio_service));

        let mut rt =
            ProjectLoader::load_from_root(fs, services).expect("load fyeah button example");
        rt.set_graphics(Some(Arc::new(crate::Graphics::new())));

        rt.tick(16).expect("tick fyeah-button without radio cycle");
    }

    #[test]
    fn button_node_publishes_held_and_up_from_virtual_d9() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "button": {
      "ref": "./button.json"
    }
  }
}
"#,
        )
        .expect("project");
        fs.write_file(
            "/button.json".as_path(),
            br#"
{
  "kind": "Button",
  "endpoint": "button:gpio:D9",
  "stable_ms": 1
}
"#,
        )
        .expect("button");

        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
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

        control.set_pressed(HwAddress::gpio(20), true);
        let held = resolve_button_map(&mut rt, button, "held");
        assert!(!held.entries.contains_key(&SlotMapKey::U32(1)));

        rt.tick(1).expect("next frame");
        let held = resolve_button_map(&mut rt, button, "held");
        assert!(held.entries.contains_key(&SlotMapKey::U32(1)));

        control.set_pressed(HwAddress::gpio(20), false);
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

    #[test]
    fn control_radio_bidirectional_bus_binding_broadcasts_button_event() {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "button": {
      "ref": "./button.json"
    },
    "radio": {
      "ref": "./radio.json"
    }
  }
}
"#,
        )
        .expect("project");
        fs.write_file(
            "/button.json".as_path(),
            br#"
{
  "kind": "Button",
  "endpoint": "button:gpio:D9",
  "stable_ms": 1,
  "bindings": {
    "down": {
      "target": "bus#trigger"
    }
  }
}
"#,
        )
        .expect("button");
        fs.write_file(
            "/radio.json".as_path(),
            br#"
{
  "kind": "ControlRadio",
  "endpoint": "radio:virtual:0",
  "channel": 1,
  "repeat_count": 2,
  "bindings": {
    "input": {
      "source": "bus#trigger"
    },
    "output": {
      "target": "bus#trigger"
    }
  }
}
"#,
        )
        .expect("radio");

        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let button_driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let button_control = button_driver.clone();
        let radio_driver = VirtualRadioDriver::new(Rc::clone(&registry), 0);
        let radio_control = radio_driver.clone();
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(button_driver));
        hardware.add_radio_driver(Box::new(radio_driver));
        let hardware = Rc::new(hardware);
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware.clone();

        let mut services = EngineServices::new(TreePath::parse("/radio.show").expect("path"));
        services.set_button_service(Some(button_service));
        services.set_radio_service(Some(radio_service));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load radio project");
        let root = rt.tree().root();
        let radio = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("radio").unwrap())
            .expect("radio node");

        button_control.set_pressed(HwAddress::gpio(20), true);
        let first = resolve_node_map(&mut rt, radio, "output", "radio output");
        assert!(first.entries.is_empty());

        rt.tick(1).expect("button candidate frame");
        rt.tick(1).expect("button stable frame");
        let output = resolve_node_map(&mut rt, radio, "output", "radio output");
        assert!(output.entries.contains_key(&SlotMapKey::U32(1)));

        let sent = radio_control.take_sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0].kind(),
            lpc_hardware::RadioMessageKind::ControlMessage
        );
        assert_eq!(sent[0].payload(), &[1, 0, 0, 0, 1, 0, 0, 0]);
    }

    fn render_test_texture_bytes(
        rt: &mut LoadedProjectRuntime,
        product: lpc_model::VisualProduct,
    ) -> Vec<u8> {
        rt.render_texture_for_test(
            product,
            &RenderTextureRequest {
                width: 64,
                height: 64,
                format: TextureStorageFormat::Rgba16Unorm,
                time_seconds: 0.0,
            },
        )
        .expect("texture")
        .try_raw_bytes()
        .expect("bytes")
        .to_vec()
    }

    fn assert_nonzero_rgb(bytes: &[u8], label: &str) {
        assert!(
            bytes
                .chunks_exact(8)
                .any(|px| px[..6].iter().any(|byte| *byte != 0)),
            "{label} should contain nonzero RGB data"
        );
    }

    fn assert_bright_event_pixels(rt: &mut LoadedProjectRuntime, bytes: &[u8]) {
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

        if max_rgb <= 10_000 {
            // A black/dim frame here means a shader compile failed and was swallowed
            // into a zero-filled fallback render. Surface the real error(s) so the
            // failure is diagnosable instead of an opaque "not bright" assertion.
            let errors = collect_node_compile_errors(rt);
            panic!(
                "trigger event circles should render bright RGB pixels, but max_rgb={max_rgb} \
                 (a shader likely failed to compile and rendered a black fallback). \
                 Node compile/runtime errors: {errors:?}"
            );
        }
    }

    /// Collect compile/runtime errors the engine otherwise hides behind a black
    /// fallback render. Compute shaders (`event_a`/`event_b`) compile during tick,
    /// but the visual shader compiles lazily at render time, so refresh node
    /// statuses with a zero-delta tick before reading them.
    fn collect_node_compile_errors(rt: &mut LoadedProjectRuntime) -> Vec<String> {
        let _ = rt.tick(0);
        rt.tree()
            .entries()
            .filter_map(|entry| match entry.status.value() {
                NodeRuntimeStatus::Error(message) => Some(format!("{:?}: {message}", entry.path)),
                _ => None,
            })
            .collect()
    }

    fn resolve_button_map(
        rt: &mut LoadedProjectRuntime,
        button: NodeId,
        slot: &str,
    ) -> lpc_model::SlotMapDyn {
        resolve_node_map(rt, button, slot, "button slot")
    }

    fn resolve_node_map(
        rt: &mut LoadedProjectRuntime,
        node: NodeId,
        slot: &str,
        label: &str,
    ) -> lpc_model::SlotMapDyn {
        let (production, _) = rt
            .resolve_with_engine_host(
                QueryKey::ProducedSlot {
                    node,
                    slot: SlotPath::parse(slot).expect("map slot"),
                },
                ResolveLogLevel::Off,
            )
            .expect("map production");
        let SlotData::Map(map) = production.data().clone() else {
            panic!("{label} should be a map");
        };
        map
    }

    fn resolve_visual_product(
        rt: &mut LoadedProjectRuntime,
        node: NodeId,
        slot: &str,
    ) -> lpc_model::VisualProduct {
        let production = resolve_playlist_slot(rt, node, slot);
        let LpValue::Product(ProductRef::Visual(product)) =
            production.value_leaf().expect("visual product").value()
        else {
            panic!("slot {slot} should be a visual product");
        };
        *product
    }

    fn resolve_playlist_u32(rt: &mut LoadedProjectRuntime, playlist: NodeId, slot: &str) -> u32 {
        let production = resolve_playlist_slot(rt, playlist, slot);
        let LpValue::U32(value) = production.value_leaf().expect("playlist value").value() else {
            panic!("playlist slot {slot} should be u32");
        };
        *value
    }

    fn resolve_playlist_f32(rt: &mut LoadedProjectRuntime, playlist: NodeId, slot: &str) -> f32 {
        let production = resolve_playlist_slot(rt, playlist, slot);
        let LpValue::F32(value) = production.value_leaf().expect("playlist value").value() else {
            panic!("playlist slot {slot} should be f32");
        };
        *value
    }

    fn resolve_playlist_slot(
        rt: &mut LoadedProjectRuntime,
        playlist: NodeId,
        slot: &str,
    ) -> Production {
        rt.resolve_with_engine_host(
            QueryKey::ProducedSlot {
                node: playlist,
                slot: SlotPath::parse(slot).expect("playlist slot"),
            },
            ResolveLogLevel::Off,
        )
        .expect("playlist production")
        .0
    }

    struct TestTimeProvider {
        now_ms: Cell<u64>,
    }

    impl TestTimeProvider {
        fn new() -> Self {
            Self {
                now_ms: Cell::new(0),
            }
        }

        fn advance(&self, delta_ms: u64) {
            self.now_ms.set(self.now_ms.get().saturating_add(delta_ms));
        }
    }

    impl TimeProvider for TestTimeProvider {
        fn now_ms(&self) -> u64 {
            self.now_ms.get()
        }
    }

    fn tick_with_test_time(
        rt: &mut LoadedProjectRuntime,
        time: &TestTimeProvider,
        delta_ms: u32,
        label: &str,
    ) {
        time.advance(u64::from(delta_ms));
        rt.tick(delta_ms)
            .unwrap_or_else(|err| panic!("{label}: {err}"));
    }

    fn write_flat_basic_files(fs: &LpFsMemory) {
        fs.write_file(
            "/project.json".as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "name": "basic",
  "nodes": {
    "output": {
      "ref": "./output.json"
    },
    "texture": {
      "ref": "./texture.json"
    },
    "shader": {
      "ref": "./shader.json"
    },
    "fixture": {
      "ref": "./fixture.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/texture.json".as_path(),
            br#"
{
  "kind": "Texture",
  "size": {
    "width": 16,
    "height": 16
  },
  "bindings": {
    "input": {
      "source": "bus#visual.out"
    }
  }
}
"#,
        )
        .expect("texture.json");
        fs.write_file(
            "/shader.json".as_path(),
            br#"
{
  "kind": "Shader",
  "source": {
    "path": "shader.glsl"
  },
  "render_order": 0,
  "bindings": {
    "output": {
      "target": "bus#visual.out"
    }
  }
}
"#,
        )
        .expect("shader.json");
        fs.write_file(
            "/shader.glsl".as_path(),
            b"vec4 render(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
        )
        .expect("shader.glsl");
        fs.write_file(
            "/output.json".as_path(),
            br#"
{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": {
    "input": {
      "source": "bus#control.out"
    }
  }
}
"#,
        )
        .expect("output.json");
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "color_order": "rgb",
  "brightness": 255,
  "gamma_correction": false,
  "transform": [
    [
      1.0,
      0.0,
      0.0
    ],
    [
      0.0,
      1.0,
      0.0
    ],
    [
      0.0,
      0.0,
      1.0
    ]
  ],
  "bindings": {
    "input": {
      "source": "bus#visual.out"
    },
    "output": {
      "target": "bus#control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "RingArray",
        "center": [
          0.5,
          0.5
        ],
        "diameter": 1.0,
        "start_ring_inclusive": 0,
        "end_ring_exclusive": 1,
        "offset_angle": 0.0,
        "order": "inner_first",
        "ring_lamp_counts": {
          "0": 1
        }
      }
    }
  }
}
"#,
        )
        .expect("fixture.json");
    }
}
