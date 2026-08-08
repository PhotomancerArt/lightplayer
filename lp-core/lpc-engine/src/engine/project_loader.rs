//! Load authored module node-artifact trees into [`super::Engine`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lp_collection::VecSet;

use lpc_model::{ArtifactSpec, NodeInvocation, NodeKind};
use lpc_model::{
    BindingDefs, BindingRef as AuthoredBindingRef, ChannelName, Kind, LpValue, NodeDef, NodeId,
    NodeName, ProjectNodeOrigin, ProjectNodePlacement, Revision, SlotPath,
};
use lpc_model::{NodeDefLocation, NodeDefState};
use lpc_model::{
    SlotDirection, SlotName, SlotPathSegment, SlotShape, StaticSlotShape, well_known_channel,
};
// `FixtureDef`/`MappingConfig` back `resolve_fixture_mapping` (node-fixture
// only); `PlaylistDef` backs `playlist_runtime_entries` (node-playlist only)
// — both model types, but their sole consumers here are gated, so the
// imports follow the same gate rather than dangling unused.
#[cfg(feature = "node-playlist")]
use lpc_model::PlaylistDef;
#[cfg(feature = "node-fixture")]
use lpc_model::{FixtureDef, MappingConfig};
// `AssetContentType`/`AssetLocation`/`AssetText` are used only by the
// asset-backed node kinds (shader/compute-shader source, fixture map2d) via
// `materialize_node_text_asset`/`asset_for_node_content_type` — same gate.
#[cfg(any(feature = "node-shader", feature = "node-fixture"))]
use lpc_model::{AssetContentType, AssetLocation};
#[cfg(any(feature = "node-shader", feature = "node-fixture"))]
use lpc_registry::AssetText;
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpc_wire::{NodeRuntimeStatus, WireChildKind, WireSlotIndex};
use lpfs::LpFs;
use lpfs::lp_path::{LpPath, LpPathBuf};

use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::node::{NodeEntryState, TreeError};
// `Output`/`Project` are never gated (see `lpc-engine/Cargo.toml`), so these
// two stay unconditional; every other node type below is feature-gated —
// one `use` per gate so a disabled feature doesn't drag in a type that no
// longer exists in this build.
#[cfg(feature = "node-button")]
use crate::nodes::ButtonNode;
#[cfg(feature = "node-clock")]
use crate::nodes::ClockNode;
#[cfg(feature = "node-radio")]
use crate::nodes::ControlRadioNode;
#[cfg(feature = "node-fluid")]
use crate::nodes::FluidNode;
use crate::nodes::OutputNode;
#[cfg(feature = "node-texture")]
use crate::nodes::TextureNode;
#[cfg(feature = "node-fixture")]
use crate::nodes::fixture::mapping::mapping_from_map2d_doc;
#[cfg(feature = "node-shader")]
use crate::nodes::{ComputeShaderNode, ShaderNode};
#[cfg(feature = "node-fixture")]
use crate::nodes::{FixtureMap2dSource, FixtureMapping, FixtureNode};
#[cfg(feature = "node-playlist")]
use crate::nodes::{PlaylistNode, PlaylistRuntimeEntry};

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
    pub(super) ownership: ProjectedNodeOwnership,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectedNodeOwnership {
    Root,
    ProjectChild,
    PlaylistEntry { playlist: NodeId, entry: u32 },
}

/// Loads the authored project artifact tree into a core engine-backed runtime.
pub struct ProjectLoader;

impl ProjectLoader {
    pub fn load_from_root(
        root: &dyn LpFs,
        services: EngineServices,
    ) -> Result<LoadedProjectRuntime, ProjectLoadError> {
        Self::load_project_artifact(root, services, ArtifactSpec::path("/module.json"))
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
        Self::register_projected_bindings(&mut registry, &mut runtime, &projected_nodes, frame)?;

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
            NodeDefState::Loaded(NodeDef::Module(_)) => Ok(()),
            NodeDefState::Loaded(other) => Err(ProjectLoadError::ProjectParse {
                file: path.as_str().to_string(),
                error: format!("root artifact must be Module, got {:?}", other.kind()),
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
            .attach_runtime_node(root, crate::nodes::ModuleNode::boxed(root), frame)
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
            // A definition that failed to parse has no kind to report, so the
            // node is projected as a container and matches none of the
            // per-kind attach loops below — it is present in the tree but
            // drives nothing. `mark_node_load_error` is what makes that
            // visible; do not read this fallback as "it is a project".
            let kind = def_entry.state.kind().unwrap_or(NodeKind::Module);
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

            // Structural scope (modules.md R1/R2), recomputed identically on
            // BOTH entry points — fresh load and apply both run this spine
            // pass, so an edited project can never wear different scopes
            // than a reloaded one. Ownership already carries the answer:
            // project children live in their parent module's scope; a
            // playlist entry's child lives in that entry's sink scope.
            {
                let scope = match ownership {
                    ProjectedNodeOwnership::Root => None,
                    ProjectedNodeOwnership::ProjectChild => {
                        parent.map(|owner| crate::node::ScopeRef::Module { owner })
                    }
                    ProjectedNodeOwnership::PlaylistEntry { playlist, entry } => {
                        Some(crate::node::ScopeRef::Sink {
                            owner: playlist,
                            entry,
                        })
                    }
                };
                // The root introduces the root scope even while its def is
                // broken (R1: the engine always answers); other nodes
                // introduce iff their def is known module-kinded — the
                // failed-def kind fallback above must not mint scopes.
                let introduces = matches!(ownership, ProjectedNodeOwnership::Root)
                    || matches!(def_entry.state.kind(), Some(NodeKind::Module));
                let entry = runtime
                    .tree_mut()
                    .get_mut(node_id)
                    .ok_or(ProjectLoadError::Tree(TreeError::UnknownNode(node_id)))?;
                entry.scope = scope;
                entry.introduces_scope = introduces;
            }
            if let Some(message) = state_error {
                mark_node_load_error(
                    runtime,
                    node_id,
                    frame,
                    &def_location_label(&project_node.def_location),
                    message,
                );
            }

            projected_nodes.push(ProjectedNode {
                name,
                parent,
                def_location: project_node.def_location,
                use_location: project_node.key,
                id: node_id,
                kind,
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
        // Only read when node-shader or node-fixture is on (the asset-backed
        // kinds, via `materialize_node_text_asset`); the signature must stay
        // stable across gate combinations, so this is a scoped allow rather
        // than a `#[cfg]` on the parameter itself.
        #[cfg_attr(
            not(any(feature = "node-shader", feature = "node-fixture")),
            allow(unused_variables, reason = "read only by the asset-backed node kinds")
        )]
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
            if node.kind != NodeKind::Module || node.ownership == ProjectedNodeOwnership::Root {
                // Root already wears its ModuleNode from the spine pass —
                // it must have a runtime even when its def is broken.
                continue;
            }
            // Broken defs project with the module FALLBACK kind — they must
            // stay error nodes, not wear a live module runtime.
            let Ok(NodeDef::Module(_)) = projected_node_config(registry, node) else {
                continue;
            };
            // Never feature-gated: every build carries the module runtime.
            runtime
                .attach_runtime_node(node.id, crate::nodes::ModuleNode::boxed(node.id), frame)
                .map_err(|e| ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("attach module runtime: {e}"),
                })?;
        }
        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Clock {
                continue;
            }
            #[cfg(feature = "node-clock")]
            {
                let NodeDef::Clock(_) = projected_node_config(registry, node)? else {
                    continue;
                };
                runtime
                    .attach_runtime_node(node.id, Box::new(ClockNode::new(node.id)), frame)
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach clock runtime: {e}"),
                    })?;
            }
            #[cfg(not(feature = "node-clock"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(NodeKind::Clock)),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach clock placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Button {
                continue;
            }
            #[cfg(feature = "node-button")]
            {
                let NodeDef::Button(_) = projected_node_config(registry, node)? else {
                    continue;
                };
                runtime
                    .attach_runtime_node(node.id, Box::new(ButtonNode::new()), frame)
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach button runtime: {e}"),
                    })?;
            }
            #[cfg(not(feature = "node-button"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::Button,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach button placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::ControlRadio {
                continue;
            }
            #[cfg(feature = "node-radio")]
            {
                let NodeDef::ControlRadio(_) = projected_node_config(registry, node)? else {
                    continue;
                };
                runtime
                    .attach_runtime_node(node.id, Box::new(ControlRadioNode::new()), frame)
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach control radio runtime: {e}"),
                    })?;
                runtime.add_demand_root(node.id);
            }
            #[cfg(not(feature = "node-radio"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::ControlRadio,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach control radio placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Texture {
                continue;
            }
            #[cfg(feature = "node-texture")]
            {
                runtime
                    .attach_runtime_node(node.id, Box::new(TextureNode::new(node.id)), frame)
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach texture runtime: {e}"),
                    })?;
            }
            #[cfg(not(feature = "node-texture"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::Texture,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach texture placeholder runtime: {e}"),
                    })?;
            }
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
                .register_output_sink(sink_id, node.id, &config);
            runtime.add_demand_root(node.id);
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Shader {
                continue;
            }
            #[cfg(feature = "node-shader")]
            {
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
            }
            #[cfg(not(feature = "node-shader"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::Shader,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach shader placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::ComputeShader {
                continue;
            }
            #[cfg(feature = "node-shader")]
            {
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
            }
            #[cfg(not(feature = "node-shader"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::ComputeShader,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach compute shader placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Fluid {
                continue;
            }
            #[cfg(feature = "node-fluid")]
            {
                let NodeDef::Fluid(_) = projected_node_config(registry, node)? else {
                    continue;
                };
                runtime
                    .attach_runtime_node(node.id, Box::new(FluidNode::new(node.id)), frame)
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach fluid runtime: {e}"),
                    })?;
            }
            #[cfg(not(feature = "node-fluid"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(NodeKind::Fluid)),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach fluid placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Playlist {
                continue;
            }
            #[cfg(feature = "node-playlist")]
            {
                let (idle_entry, default_fade, entries) = {
                    let NodeDef::Playlist(config) = projected_node_config(registry, node)? else {
                        continue;
                    };
                    (
                        *config.idle_entry.value(),
                        config.default_fade.value().0,
                        playlist_runtime_entries(projected_nodes, node.id, config),
                    )
                };
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
            #[cfg(not(feature = "node-playlist"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::Playlist,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach playlist gate-off placeholder runtime: {e}"),
                    })?;
            }
        }

        for node in projected_nodes {
            if !should_attach_projected_node(node, targets) {
                continue;
            }
            if node.kind != NodeKind::Fixture {
                continue;
            }
            #[cfg(feature = "node-fixture")]
            {
                let NodeDef::Fixture(config) = projected_node_config(registry, node)?.clone()
                else {
                    continue;
                };
                match resolve_fixture_mapping(fs, registry, node, &config) {
                    Ok((mapping, map2d_source)) => {
                        let mut fixture =
                            FixtureNode::new(node.id, mapping, *config.sampling.value(), frame)
                                .with_render_defaults(
                                    config.render_width(),
                                    config.render_height(),
                                    *config.color_order.value(),
                                );
                        if let Some(source) = map2d_source {
                            fixture = fixture.with_map2d_source(source);
                        }
                        runtime
                            .attach_runtime_node(node.id, Box::new(fixture), frame)
                            .map_err(|e| ProjectLoadError::InvalidProjectReference {
                                path: node_label(node),
                                reason: format!("attach fixture runtime: {e}"),
                            })?;
                        mark_node_status(runtime, node.id, frame, NodeRuntimeStatus::Ok);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        mark_node_load_error(runtime, node.id, frame, &node_label(node), message);
                    }
                }
            }
            #[cfg(not(feature = "node-fixture"))]
            {
                runtime
                    .attach_runtime_node(
                        node.id,
                        Box::new(crate::nodes::CorePlaceholderNode::new_leaf(
                            NodeKind::Fixture,
                        )),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("attach fixture placeholder runtime: {e}"),
                    })?;
            }
        }

        Ok(())
    }

    /// The loader's binding phase: register every binding the projected
    /// nodes contribute. Load runs it after all nodes attach; incremental
    /// apply re-runs it against a cleared index so the runtime's bindings
    /// always match what a fresh load would produce (incremental binding
    /// apply, Option C).
    pub(super) fn register_projected_bindings(
        registry: &mut ProjectRegistry,
        runtime: &mut Engine,
        projected_nodes: &[ProjectedNode],
        frame: Revision,
    ) -> Result<(), ProjectLoadError> {
        for node in projected_nodes {
            register_node_bindings(registry, runtime, projected_nodes, node, frame)?;
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

/// Record a per-node load failure without failing the project load.
///
/// A broken definition stays in inventory so Studio can show it, and the load
/// deliberately continues — but a headless device has no Studio, and until this
/// logged, a node with an unparseable file simply never appeared: no error, no
/// output, nothing in 150 s of serial. The warning is what makes the drop
/// observable on a device.
fn mark_node_load_error(
    runtime: &mut Engine,
    node_id: NodeId,
    frame: Revision,
    label: &str,
    message: String,
) {
    log::warn!("ProjectLoader: node {label} did not load: {message}");
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

// Only the Fixture real-attach arm reports `Ok` explicitly (every other
// kind relies on the `Created` default); follow that one caller's gate.
#[cfg(feature = "node-fixture")]
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

#[cfg(feature = "node-playlist")]
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
                trigger_ids: config
                    .entries
                    .entries
                    .get(&entry)
                    .and_then(|entry| entry.trigger_ids.data.as_ref())
                    .map(|ids| ids.value().0.clone()),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(feature = "node-fixture")]
fn resolve_fixture_mapping(
    fs: &dyn LpFs,
    registry: &mut ProjectRegistry,
    node: &ProjectedNode,
    config: &FixtureDef,
) -> Result<(FixtureMapping, Option<FixtureMap2dSource>), ProjectLoadError> {
    match config.mapping.value() {
        MappingConfig::Map2d { .. } => {
            let text = materialize_node_text_asset(
                fs,
                registry,
                node,
                AssetContentType::FixtureMap2d,
                "fixture map2d document",
            )?;
            let doc = lpc_mapping::Map2dDoc::from_json(&text.text).map_err(|e| {
                ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("resolve map2d fixture mapping: {e}"),
                }
            })?;
            let mapping =
                mapping_from_map2d_doc(&doc, config.render_width(), config.render_height())
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("resolve map2d fixture mapping: {e}"),
                    })?;
            // Keep the source so the runtime node can re-resolve on asset
            // refresh (the in-place editor's apply path).
            let source = FixtureMap2dSource {
                location: text.location,
                revision: text.revision,
                render_width: config.render_width(),
                render_height: config.render_height(),
            };
            // Document geometry stays compact: never expanded into slots,
            // never serialized, never slot-addressed.
            Ok((FixtureMapping::Compact(mapping), Some(source)))
        }
        // Hand-authored `PathPoints` (and an unset mapping) keep the slot
        // form — Studio edits individual lamps there.
        other => Ok((FixtureMapping::Slots(other.clone()), None)),
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

// Called from the Shader/ComputeShader loops (node-shader, GLSL/compute
// source assets) and from `resolve_fixture_mapping` (node-fixture, the
// map2d document) — the only two asset-backed node kinds. `Texture` reads
// no text asset (image bytes go through a different path), so it does not
// need this helper.
#[cfg(any(feature = "node-shader", feature = "node-fixture"))]
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

#[cfg(any(feature = "node-shader", feature = "node-fixture"))]
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

/// Def and state record shapes for a node kind, when static ones exist.
fn kind_shapes(kind: NodeKind) -> (Option<SlotShape>, Option<SlotShape>) {
    use lpc_model::nodes::button::ButtonState;
    use lpc_model::nodes::clock::ClockDef;
    use lpc_model::nodes::clock::ClockState;
    use lpc_model::nodes::fixture::FixtureDef;
    use lpc_model::nodes::fixture::FixtureState;
    use lpc_model::nodes::fluid::FluidDef;
    use lpc_model::nodes::fluid::FluidState;
    use lpc_model::nodes::output::OutputDef;
    use lpc_model::nodes::playlist::PlaylistDef;
    use lpc_model::nodes::playlist::PlaylistState;
    use lpc_model::nodes::radio::ControlRadioDef;
    use lpc_model::nodes::radio::ControlRadioState;
    use lpc_model::nodes::shader::ShaderState;
    use lpc_model::nodes::shader::{ComputeShaderDef, ShaderDef};
    use lpc_model::nodes::texture::TextureDef;
    use lpc_model::nodes::texture::TextureState;
    let def_shape = match kind {
        NodeKind::Button => Some(lpc_model::nodes::button::ButtonDef::slot_shape()),
        NodeKind::Clock => Some(ClockDef::slot_shape()),
        NodeKind::Fixture => Some(FixtureDef::slot_shape()),
        NodeKind::Fluid => Some(FluidDef::slot_shape()),
        NodeKind::Playlist => Some(PlaylistDef::slot_shape()),
        NodeKind::ControlRadio => Some(ControlRadioDef::slot_shape()),
        NodeKind::Shader => Some(ShaderDef::slot_shape()),
        NodeKind::ComputeShader => Some(ComputeShaderDef::slot_shape()),
        NodeKind::Output => Some(OutputDef::slot_shape()),
        NodeKind::Texture => Some(TextureDef::slot_shape()),
        _ => None,
    };
    let state_shape = match kind {
        NodeKind::Button => Some(ButtonState::slot_shape()),
        NodeKind::Clock => Some(ClockState::slot_shape()),
        NodeKind::Fixture => Some(FixtureState::slot_shape()),
        NodeKind::Fluid => Some(FluidState::slot_shape()),
        NodeKind::Playlist => Some(PlaylistState::slot_shape()),
        NodeKind::ControlRadio => Some(ControlRadioState::slot_shape()),
        NodeKind::Shader => Some(ShaderState::slot_shape()),
        NodeKind::Texture => Some(TextureState::slot_shape()),
        _ => None,
    };
    (def_shape, state_shape)
}

/// Declared direction of a node's root slot, looked up in the kind's def and
/// state record shapes (first path segment). `None` when the slot is dynamic
/// (shader consumed entries) or otherwise undeclared — the guardrail only
/// fires on declared mislabels (ADR 2026-07-09 declarative-default-bindings).
fn declared_slot_direction(kind: NodeKind, slot: &lpc_model::SlotPath) -> Option<SlotDirection> {
    let SlotPathSegment::Field(name) = slot.segments().first()? else {
        return None;
    };
    let name = name.as_str();
    let field_direction = |shape: SlotShape| match shape {
        SlotShape::Record { fields, .. } => fields
            .iter()
            .find(|field| field.name.as_str() == name)
            .map(|field| field.semantics.direction),
        _ => None,
    };
    let (def_shape, state_shape) = kind_shapes(kind);
    def_shape
        .and_then(field_direction)
        .or_else(|| state_shape.and_then(field_direction))
}

/// The loader's binding phase: register every binding one projected node
/// contributes — authored entries, loader plumbing (the output demand
/// literal), and slot-declared defaults. Mirrors what the attach arms
/// registered before the phases were split; behavior parity is pinned by the
/// characterization tests.
fn register_node_bindings(
    registry: &mut ProjectRegistry,
    runtime: &mut Engine,
    projected_nodes: &[ProjectedNode],
    node: &ProjectedNode,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    // Module nodes register their R7 output interface: authored bindings
    // (the contention pick), authored exports, and — for NON-root modules —
    // the automatic `output` → `visual.out` publish at fallback priority
    // that makes an embedded module drop-in (root is excluded: its
    // containing scope does not exist; its mirror is what playback reads).
    // Unloaded/errored defs project with the module fallback kind and
    // register nothing — the node renders as an error node; the load must
    // not fail.
    if node.kind == NodeKind::Module {
        if let Ok(NodeDef::Module(config)) = projected_node_config(registry, node) {
            let config = config.clone();
            for (key, entry) in config.bindings.entries().iter() {
                let name = key.as_str();
                entry
                    .validate()
                    .map_err(|e| ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!("binding `{name}`: {e}"),
                    })?;
                if name != "output" || entry.target_ref().is_none() {
                    return Err(ProjectLoadError::InvalidProjectReference {
                        path: node_label(node),
                        reason: format!(
                            "binding `{name}` names no bindable module slot \
                             (a module takes a `target` binding on `output`)"
                        ),
                    });
                }
            }
            register_target_binding(
                runtime,
                projected_nodes,
                node,
                "output",
                &config.bindings,
                frame,
            )?;
            register_module_exports(runtime, node, &config, frame)?;
            register_module_output_default(runtime, node, &config.bindings, frame)?;
        }
        return Ok(());
    }
    let config = projected_node_config(registry, node)?.clone();
    // Kind-owned loader plumbing that no authored bindings entry drives: the
    // output demand literal.
    match &config {
        NodeDef::Output(_) => {
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
        }
        _ => {}
    }
    // Slot-declared `default_bind` on the artifact-declared consumed slots of
    // BOTH shader kinds — a compute shader's `time` slot is default-bound by
    // the starter itself, and registering only the render-shader arm made
    // that silently unwired
    // (docs/defects/2026-08-04-compute-shader-default-bind-ignored.md).
    // Produced slots are deliberately excluded: publishing a produced slot
    // still takes an authored `target` entry, so a `default_bind` there is
    // inert.
    let default_bind_slots = match &config {
        NodeDef::Shader(shader) => Some((&shader.consumed_slots, &shader.bindings)),
        NodeDef::ComputeShader(compute) => Some((&compute.consumed_slots, &compute.bindings)),
        _ => None,
    };
    if let Some((consumed_slots, bindings)) = default_bind_slots {
        for (name, slot) in consumed_slots.entries.iter() {
            let Some(endpoint) = slot.default_bind.data.as_ref() else {
                continue;
            };
            register_default_bind(
                runtime,
                projected_nodes,
                node,
                bindings,
                frame,
                name,
                SlotDirection::Consumed,
                &endpoint.value().to_string(),
            )?;
        }
    }
    // Dynamic (artifact-declared) slot names: shader/compute consumed slots
    // take source bindings, compute produced slots take target bindings.
    let (dynamic_consumed, dynamic_produced): (Vec<&str>, Vec<&str>) = match &config {
        NodeDef::Shader(shader) => (
            shader
                .consumed_slots
                .entries
                .keys()
                .map(String::as_str)
                .collect(),
            Vec::new(),
        ),
        NodeDef::ComputeShader(compute) => (
            compute
                .consumed_slots
                .entries
                .keys()
                .map(String::as_str)
                .collect(),
            compute
                .produced_slots
                .entries
                .keys()
                .map(String::as_str)
                .collect(),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    // Every authored binding entry registers or errs — an entry naming a slot
    // nobody resolves was the silent-drop defect
    // (docs/defects/2026-08-02-authored-source-bindings-silently-dropped.md).
    let bindings = node_def_bindings(&config);
    for (key, entry) in bindings.entries().iter() {
        let name = key.as_str();
        entry
            .validate()
            .map_err(|e| ProjectLoadError::InvalidProjectReference {
                path: node_label(node),
                reason: format!("binding `{name}`: {e}"),
            })?;
        let has_target = entry.target_ref().is_some();
        if !has_target && dynamic_consumed.contains(&name) {
            register_source_binding(runtime, projected_nodes, node, name, bindings, frame)?;
            continue;
        }
        if has_target && dynamic_produced.contains(&name) {
            register_target_binding(runtime, projected_nodes, node, name, bindings, frame)?;
            continue;
        }
        let slot = match resolve_declared_binding_path(node.kind, name) {
            Ok(slot) => slot,
            // A shader/compute slot namespace is OPEN — its consumed/produced
            // records are authored and tooling repairs a binding that arrives
            // before its record (the agent's declared-orphan → `upsert_param`
            // flow). An unresolved key there is that legal orphan state, and
            // the studio's uniform surface owns the feedback; registering
            // nothing preserves it. Every other kind's slot set is closed, so
            // an unresolved key can only be a mistake — fail the load with
            // the slot's name.
            Err(_) if matches!(node.kind, NodeKind::Shader | NodeKind::ComputeShader) => {
                continue;
            }
            Err(reason) => {
                return Err(ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason,
                });
            }
        };
        if has_target {
            register_target_binding_at_path(
                runtime,
                projected_nodes,
                node,
                name,
                bindings,
                slot,
                frame,
            )?;
        } else {
            let source = binding_source(bindings, name).expect("validate: source or value present");
            let source = binding_source_endpoint(projected_nodes, node, source)?;
            register_source_binding_at_path(
                runtime,
                projected_nodes,
                node,
                name,
                source,
                slot,
                frame,
            )?;
        }
    }
    register_declared_defaults(runtime, projected_nodes, node, bindings, frame)?;
    Ok(())
}

/// The authored bindings map every non-module node kind carries.
fn node_def_bindings(config: &NodeDef) -> &BindingDefs {
    match config {
        NodeDef::Module(config) => &config.bindings,
        NodeDef::Button(config) => &config.bindings,
        NodeDef::Clock(config) => &config.bindings,
        NodeDef::Texture(config) => &config.bindings,
        NodeDef::Shader(config) => &config.bindings,
        NodeDef::ComputeShader(config) => &config.bindings,
        NodeDef::Fluid(config) => &config.bindings,
        NodeDef::Playlist(config) => &config.bindings,
        NodeDef::ControlRadio(config) => &config.bindings,
        NodeDef::Output(config) => &config.bindings,
        NodeDef::Fixture(config) => &config.bindings,
    }
}

/// Resolve one authored binding key against the kind's declared def/state
/// record shapes, normalizing option wrappers to the interior `some` path
/// the runtime's accessors actually resolve (`brightness` on a fixture →
/// `brightness.some`). Nested structure is verified as far as it is
/// statically declared; segments past an opaque leaf (value, custom codec,
/// shape ref) pass through unchanged. A key that names no declared slot is
/// an error — the loud replacement for the silent drop recorded in
/// `docs/defects/2026-08-02-authored-source-bindings-silently-dropped.md`.
fn resolve_declared_binding_path(kind: NodeKind, name: &str) -> Result<SlotPath, String> {
    let path =
        SlotPath::parse(name).map_err(|e| format!("invalid binding slot `{name}`: {e:?}"))?;
    let mut segments = path.segments().iter();
    let Some(SlotPathSegment::Field(first)) = segments.next() else {
        return Err(format!(
            "binding slot `{name}` must start with a field name"
        ));
    };
    let (def_shape, state_shape) = kind_shapes(kind);
    let field = [def_shape, state_shape]
        .into_iter()
        .flatten()
        .find_map(|shape| match shape {
            SlotShape::Record { fields, .. } => fields
                .into_iter()
                .find(|field| field.name.as_str() == first.as_str()),
            _ => None,
        })
        .ok_or_else(|| format!("binding `{name}` names no declared slot on {kind:?}"))?;
    let some_segment = || SlotPathSegment::Field(SlotName::parse("some").expect("valid name"));
    let mut out = alloc::vec![SlotPathSegment::Field(first.clone())];
    // `None` = walked past what the shape declares; stop verifying.
    let mut shape = Some(field.shape);
    for segment in segments {
        // Auto-descend option wrappers the author left implicit.
        let spells_some =
            matches!(segment, SlotPathSegment::Field(field) if field.as_str() == "some");
        while !spells_some && matches!(shape, Some(SlotShape::Option { .. })) {
            let Some(SlotShape::Option { some, .. }) = shape else {
                unreachable!("matched above");
            };
            out.push(some_segment());
            shape = Some(*some);
        }
        let next = match (shape, segment) {
            (Some(SlotShape::Option { some, .. }), SlotPathSegment::Field(_)) => Some(*some),
            (Some(SlotShape::Record { fields, .. }), SlotPathSegment::Field(field)) => Some(
                fields
                    .into_iter()
                    .find(|candidate| candidate.name == *field)
                    .ok_or_else(|| {
                        format!("binding `{name}`: no declared field `{field}` on {kind:?}")
                    })?
                    .shape,
            ),
            (Some(SlotShape::Enum { variants, .. }), SlotPathSegment::Field(field)) => Some(
                variants
                    .into_iter()
                    .find(|candidate| candidate.name == *field)
                    .ok_or_else(|| {
                        format!("binding `{name}`: no declared variant `{field}` on {kind:?}")
                    })?
                    .shape,
            ),
            (Some(SlotShape::Map { value, .. }), SlotPathSegment::Key(_)) => Some(*value),
            (Some(SlotShape::Map { .. }), SlotPathSegment::Field(field)) => {
                return Err(format!(
                    "binding `{name}`: map slot takes a bracketed key, not field `{field}`"
                ));
            }
            (_, _) => None,
        };
        out.push(segment.clone());
        shape = next;
    }
    // A binding lands on the value-bearing interior of an option wrapper.
    while let Some(SlotShape::Option { some, .. }) = shape {
        out.push(some_segment());
        shape = Some(*some);
    }
    Ok(SlotPath::from_segments(out))
}

/// Slot-declared default bindings for a node kind: (slot name, declared
/// direction, `bus:` endpoint) triples from the def and state shapes.
///
/// Nested records are walked too, emitting the DOTTED slot name
/// (`transport.rate`) that `register_default_bind` then normalizes into an
/// accessor path. A leaf's declaration is what wires it: a promoted record
/// (`ClockDef::transport`, `panel = "show"`) carries no endpoint of its own
/// — its three leaves each name their own `clock.*` channel.
fn declared_default_binds(kind: NodeKind) -> Vec<(String, SlotDirection, String)> {
    let mut out = Vec::new();
    let (def_shape, state_shape) = kind_shapes(kind);
    for shape in [def_shape, state_shape].into_iter().flatten() {
        collect_default_binds("", &shape, &mut out);
    }
    out
}

/// Depth-first walk of a record shape's declared `default_bind`s, keyed by
/// dotted slot path. Descends into inline record fields only — a `Ref` field
/// names a catalog shape that declares its own bindings against its own
/// root, so following one here would attribute another shape's wiring to
/// this node.
fn collect_default_binds(
    prefix: &str,
    shape: &SlotShape,
    out: &mut Vec<(String, SlotDirection, String)>,
) {
    let SlotShape::Record { fields, .. } = shape else {
        return;
    };
    for field in fields {
        let name = if prefix.is_empty() {
            field.name.as_str().to_string()
        } else {
            format!("{prefix}.{}", field.name)
        };
        if let Some(endpoint) = &field.default_bind {
            out.push((name.clone(), field.semantics.direction, endpoint.clone()));
        }
        collect_default_binds(&name, &field.shape, out);
    }
}

/// Materialize slot-declared default bindings (ADR 2026-07-09): one generic
/// pass replacing the five per-kind loader helpers. Authored bindings for
/// the same slot win; produced defaults are suppressed for entry-owned
/// children (ownership context stays a loader rule, not slot metadata);
/// everything registers unconditionally at fallback priority — an unfilled
/// channel (readers, no writer) is surfaced on the bus instead of hidden.
fn register_declared_defaults(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    for (name, direction, endpoint) in declared_default_binds(current.kind) {
        register_default_bind(
            engine,
            projected_nodes,
            current,
            bindings,
            frame,
            &name,
            direction,
            &endpoint,
        )?;
    }
    Ok(())
}

/// Register one declarative default binding: produced slots publish to the
/// endpoint's channel, consumed/local slots source from it.
#[allow(clippy::too_many_arguments, reason = "loader registration plumbing")]
fn register_default_bind(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    bindings: &BindingDefs,
    frame: Revision,
    name: &str,
    direction: SlotDirection,
    endpoint: &str,
) -> Result<(), ProjectLoadError> {
    let channel = match lpc_model::BindingRef::parse(endpoint) {
        Ok(lpc_model::BindingRef::Bus(bus)) => bus.channel().clone(),
        _ => {
            return Err(ProjectLoadError::InvalidProjectReference {
                path: node_label(current),
                reason: format!("invalid default_bind `{endpoint}` on slot `{name}`"),
            });
        }
    };
    // Declared slots normalize through the shape walk (an option-wrapped
    // slot binds its `.some` interior — the accessor path the runtime
    // resolves); dynamic shader slot names are already accessor paths.
    let slot = match resolve_declared_binding_path(current.kind, name) {
        Ok(slot) => slot,
        Err(_) => SlotPath::parse(name).map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("invalid default_bind slot `{name}`: {e}"),
        })?,
    };
    let draft = if direction == SlotDirection::Produced {
        if binding_target(bindings, name).is_some() {
            return Ok(());
        }
        let source = BindingSource::ProducedSlot {
            node: current.id,
            slot,
        };
        let target = BindingTarget::BusChannel(channel);
        BindingDraft {
            kind: binding_kind(&source, &target, name),
            source,
            target,
            priority: BindingPriority::default_fallback(),
            owner: current.id,
        }
    } else {
        if binding_source(bindings, name).is_some() {
            return Ok(());
        }
        let source = BindingSource::BusChannel(channel);
        let target = BindingTarget::ConsumedSlot {
            node: current.id,
            slot,
        };
        BindingDraft {
            kind: binding_kind(&source, &target, name),
            source,
            target,
            priority: BindingPriority::default_fallback(),
            owner: current.id,
        }
    };
    assert_draft_directions(projected_nodes, current, &draft)?;
    engine
        .add_binding(draft, frame)
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register {name} default binding: {e}"),
        })?;
    Ok(())
}

fn projected_node_kind(projected_nodes: &[ProjectedNode], id: NodeId) -> Option<NodeKind> {
    projected_nodes
        .iter()
        .find(|node| node.id == id)
        .map(|node| node.kind)
}

/// Load-time direction guardrail: a draft whose source names a produced slot
/// (or whose target names a consumed slot) must reference a slot whose
/// declared shape direction is compatible. Undeclared slots pass through;
/// declared mislabels fail the load with a clear reason instead of silently
/// generating wrong wiring.
fn assert_draft_directions(
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    draft: &BindingDraft,
) -> Result<(), ProjectLoadError> {
    if let BindingSource::ProducedSlot { node, slot } = &draft.source
        && let Some(kind) = projected_node_kind(projected_nodes, *node)
        && let Some(direction) = declared_slot_direction(kind, slot)
        && direction != SlotDirection::Produced
    {
        return Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!(
                "binding source slot `{slot}` on {kind:?} is declared {direction:?}, expected Produced"
            ),
        });
    }
    if let BindingTarget::ConsumedSlot { node, slot } = &draft.target
        && let Some(kind) = projected_node_kind(projected_nodes, *node)
        && let Some(direction) = declared_slot_direction(kind, slot)
        && direction == SlotDirection::Produced
    {
        return Err(ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!(
                "binding target slot `{slot}` on {kind:?} is declared Produced and cannot consume"
            ),
        });
    }
    Ok(())
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
    register_source_binding_at_path(
        engine,
        projected_nodes,
        current,
        slot_name,
        source,
        target_slot,
        frame,
    )
}

fn register_source_binding_at_path(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    binding_slot_name: &str,
    source: BindingSource,
    target_slot: SlotPath,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let target = BindingTarget::ConsumedSlot {
        node: current.id,
        slot: target_slot,
    };
    let draft = BindingDraft {
        kind: binding_kind(&source, &target, binding_slot_name),
        source,
        target,
        priority: BindingPriority::new(0),
        owner: current.id,
    };
    assert_draft_directions(projected_nodes, current, &draft)?;
    engine
        .add_binding(draft, frame)
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register {binding_slot_name} source binding: {e}"),
        })?;
    Ok(())
}

fn register_target_binding(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let source_slot =
        SlotPath::parse(slot_name).map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("invalid source slot `{slot_name}`: {e}"),
        })?;
    register_target_binding_at_path(
        engine,
        projected_nodes,
        current,
        slot_name,
        bindings,
        source_slot,
        frame,
    )
}

#[allow(clippy::too_many_arguments, reason = "loader registration plumbing")]
fn register_target_binding_at_path(
    engine: &mut Engine,
    projected_nodes: &[ProjectedNode],
    current: &ProjectedNode,
    slot_name: &str,
    bindings: &BindingDefs,
    source_slot: SlotPath,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    let Some(target) = binding_target(bindings, slot_name) else {
        return Ok(());
    };
    let target = binding_target_endpoint(projected_nodes, current, target)?;
    let source = BindingSource::ProducedSlot {
        node: current.id,
        slot: source_slot,
    };
    let draft = BindingDraft {
        kind: binding_kind(&source, &target, slot_name),
        source,
        target,
        priority: BindingPriority::authored(),
        owner: current.id,
    };
    assert_draft_directions(projected_nodes, current, &draft)?;
    engine
        .add_binding(draft, frame)
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(current),
            reason: format!("register {slot_name} target binding: {e}"),
        })?;
    Ok(())
}

/// The kind a binding establishes on the channels it touches.
///
/// A bus endpoint's well-known registry kind is authoritative — the first
/// registered binding stamps the channel's kind, and the old slot-name
/// guess stamped e.g. `trigger` as Color because only the time-family
/// names were listed (2026-07-16: bus pane showed "trigger COLOR").
/// Endpoints outside the registry fall back to the slot-name heuristic.
/// An embedded module contributes its visual to its host by default (R7):
/// the mirror's produced `output` publishes `visual.out` at fallback
/// priority. Per R4 that lands in the module NODE's own nearest scope (the
/// parent's) — the node sits there; only its children are inside the scope
/// it introduces. The ROOT module is skipped: its containing scope does
/// not exist, and its mirror is what playback reads. An authored `output`
/// binding (the contention pick) suppresses the default, same as every
/// declared default bind.
fn register_module_output_default(
    engine: &mut Engine,
    node: &ProjectedNode,
    bindings: &BindingDefs,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    if node.ownership == ProjectedNodeOwnership::Root {
        return Ok(());
    }
    if binding_target(bindings, "output").is_some() {
        return Ok(());
    }
    let slot = SlotPath::parse("output").expect("module output path");
    let channel = lpc_model::ChannelName(String::from(lpc_model::PRIMARY_VISUAL_CHANNEL));
    let source = BindingSource::ProducedSlot {
        node: node.id,
        slot,
    };
    let target = BindingTarget::BusChannel(channel);
    engine
        .add_binding(
            BindingDraft {
                kind: binding_kind(&source, &target, "output"),
                source,
                target,
                priority: BindingPriority::default_fallback(),
                owner: node.id,
            },
            frame,
        )
        .map_err(|e| ProjectLoadError::InvalidProjectReference {
            path: node_label(node),
            reason: format!("register module output mirror default: {e}"),
        })?;
    Ok(())
}

/// Authored exports (R7): each entry republishes an inner-scope channel
/// outward under the export's name. The binding's SOURCE is the inner
/// channel — module-owned bus reads resolve from the introduced scope (the
/// engine host's reading-scope rule) — and its TARGET is the export-named
/// channel in the module's own nearest scope. Deliberate curation, so it
/// registers at authored priority.
fn register_module_exports(
    engine: &mut Engine,
    node: &ProjectedNode,
    config: &lpc_model::ModuleDef,
    frame: Revision,
) -> Result<(), ProjectLoadError> {
    for (name, inner) in config.exports.entries.iter() {
        let inner_channel = match inner.value() {
            lpc_model::BindingRef::Bus(bus) => bus.channel().clone(),
            other => {
                return Err(ProjectLoadError::InvalidProjectReference {
                    path: node_label(node),
                    reason: format!("export `{name}` must name a bus channel, got {other:?}"),
                });
            }
        };
        let source = BindingSource::BusChannel(inner_channel);
        let target = BindingTarget::BusChannel(lpc_model::ChannelName(String::from(name.as_str())));
        engine
            .add_binding(
                BindingDraft {
                    kind: binding_kind(&source, &target, name),
                    source,
                    target,
                    priority: BindingPriority::new(0),
                    owner: node.id,
                },
                frame,
            )
            .map_err(|e| ProjectLoadError::InvalidProjectReference {
                path: node_label(node),
                reason: format!("register module export `{name}`: {e}"),
            })?;
    }
    Ok(())
}

fn binding_kind(source: &BindingSource, target: &BindingTarget, slot_name: &str) -> Kind {
    let channel = match (source, target) {
        (BindingSource::BusChannel(channel), _) => Some(channel),
        (_, BindingTarget::BusChannel(channel)) => Some(channel),
        _ => None,
    };
    if let Some(known) = channel.and_then(|channel| well_known_channel(&channel.0)) {
        return known.kind;
    }
    match slot_name {
        "time" | "seconds" | "delta_seconds" => Kind::Instant,
        // A palette slot bound to a channel that is not the well-known
        // `palette` still carries a gradient, not a color.
        "palette" | "gradient" => Kind::Gradient,
        _ => Kind::Color,
    }
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
            bus.channel().0.clone(),
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
            bus.channel().0.clone(),
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
mod binding_kind_tests {
    use lpc_model::{ChannelName, Kind, NodeId, SlotPath};

    use super::binding_kind;
    use crate::dataflow::binding::{BindingSource, BindingTarget};

    fn consumed(slot: &str) -> BindingTarget {
        BindingTarget::ConsumedSlot {
            node: NodeId::new(1),
            slot: SlotPath::parse(slot).expect("slot path"),
        }
    }

    fn produced(slot: &str) -> BindingSource {
        BindingSource::ProducedSlot {
            node: NodeId::new(1),
            slot: SlotPath::parse(slot).expect("slot path"),
        }
    }

    #[test]
    fn well_known_channel_kind_beats_the_slot_name_guess() {
        // `trigger` is Instant in the registry; the old slot-name guess
        // stamped it Color (only the time-family names were listed).
        let source = BindingSource::BusChannel(ChannelName("trigger".into()));
        assert_eq!(
            binding_kind(&source, &consumed("trigger"), "trigger"),
            Kind::Instant
        );
        let target = BindingTarget::BusChannel(ChannelName("visual.out".into()));
        assert_eq!(
            binding_kind(&produced("output"), &target, "output"),
            Kind::Color
        );
    }

    #[test]
    fn unregistered_channels_fall_back_to_the_slot_name_guess() {
        let target = BindingTarget::BusChannel(ChannelName("wobble".into()));
        assert_eq!(
            binding_kind(&produced("seconds"), &target, "seconds"),
            Kind::Instant
        );
        assert_eq!(
            binding_kind(&produced("output"), &target, "output"),
            Kind::Color
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::products::visual::{ConsumerPolicy, VisualSpace};
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

    fn fixture_project_fs() -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "fixture": {
      "ref": "./fixture.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs
    }

    #[test]
    fn fixture_map2d_mapping_loads_from_project() {
        let fs = fixture_project_fs();
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "render_size": { "width": 20, "height": 10 },
  "sampling": "direct",
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  },
  "mapping": { "kind": "Map2d", "source": "./fixture.map2d.json" }
}
"#,
        )
        .expect("fixture.json");
        fs.write_file(
            "/fixture.map2d.json".as_path(),
            br#"
{
  "format": 1,
  "objects": [
    { "name": "run", "shape": { "path": { "points": [[0,0],[20,10]], "count": 3 } } }
  ]
}
"#,
        )
        .expect("fixture.map2d.json");

        let services = EngineServices::new(TreePath::parse("/svg_fixture.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load map2d fixture project");
        assert!(node_for_def_path(&rt, "/fixture.json").is_some());
    }

    /// Device-side loud refusal: a mapping written by a newer LightPlayer —
    /// unknown shape variant and all — must fail the fixture with the honest
    /// "unsupported format" message, not with an opaque parse error. The
    /// format peek in `Map2dDoc::from_json` is what makes that true; this
    /// pins the message a user actually sees on the device.
    #[test]
    fn fixture_map2d_mapping_rejects_newer_format() {
        let fs = fixture_project_fs();
        fs.write_file(
            "/fixture.json".as_path(),
            br#"
{
  "kind": "Fixture",
  "render_size": { "width": 20, "height": 10 },
  "sampling": "direct",
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  },
  "mapping": { "kind": "Map2d", "source": "./fixture.map2d.json" }
}
"#,
        )
        .expect("fixture.json");
        fs.write_file(
            "/fixture.map2d.json".as_path(),
            br#"
{
  "format": 99,
  "objects": [
    { "name": "sector", "shape": { "helix": { "turns": 5, "count": 300 } } }
  ]
}
"#,
        )
        .expect("fixture.map2d.json");

        let services = EngineServices::new(TreePath::parse("/svg_fixture.show").expect("path"));
        let rt = ProjectLoader::load_from_root(&fs, services).expect("load with bad fixture");
        assert_fixture_node_error(
            &rt,
            "unsupported map2d format 99 (this build reads up to 2)",
        );
    }

    /// A node whose *definition file* will not parse must be reported, not
    /// dropped.
    ///
    /// Such a node has no kind, so it is projected as a container and matches
    /// none of the per-kind attach loops — it never runs, and the load still
    /// succeeds. On the desk this looked like one of four LED strips simply
    /// not existing: no error, no output, nothing in 150 s of serial. The
    /// failed status here (and the warning `mark_node_load_error` logs beside
    /// it) is the only trace such a node leaves.
    #[test]
    fn a_node_whose_definition_does_not_parse_is_marked_failed() {
        let fs = char_project(&[(
            "output",
            // Endpoint specs are `capability:target:config`; the config part
            // is empty here, exactly as a mis-edited `outputN.json` had it.
            r#"{ "kind": "Output", "channels": { "0": { "endpoint": "ws281x:local:" } },
                 "bindings": { "input": { "source": "bus:control.out" } } }"#,
        )]);

        let rt = load_project(&fs);

        assert_node_for_def_error(&rt, "/output.json", "empty part");
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
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
  "bindings": {
    "trigger": {
      "source": "bus:trigger"
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
      "trigger_ids": [1],
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
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
      "source": "node:..#entry_time"
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
            b"vec4 render_2d(vec2 pos) { return vec4(0.0, pos, 1.0); }",
        )
        .expect("idle.glsl");
        fs.write_file(
            "/active.glsl".as_path(),
            b"vec4 render_2d(vec2 pos) { return vec4(time, pos.x, pos.y, 1.0); }",
        )
        .expect("active.glsl");
        fs
    }

    fn button_playlist_project_fs() -> LpFsMemory {
        let fs = playlist_project_fs();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
  "endpoint": "button:local:D9",
  "stable_ms": 1,
  "bindings": {
    "down": {
      "target": "bus:trigger"
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
      "source": "bus:time"
    },
    "trigger": {
      "source": "bus:trigger"
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
      "trigger_ids": [1],
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
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

    fn examples_basic_fs() -> LpFsStd {
        LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/basic"))
    }

    fn examples_plasma_fs() -> LpFsStd {
        LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plasma"))
    }

    fn examples_plasma_duo_fs() -> LpFsStd {
        LpFsStd::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plasma-duo"),
        )
    }

    /// The published output-channel bytes for the output node behind `path` —
    /// the core-path end product the M4 differential tests compare.
    fn output_buffer_bytes(rt: &Engine, path: &str) -> alloc::vec::Vec<u8> {
        let out = node_for_def_path(rt, path).expect("output node");
        let entry = rt.tree().get(out).expect("output entry");
        let NodeEntryState::Alive(node) = entry.state.value() else {
            panic!("output node alive");
        };
        let id = node
            .runtime_output_sink_buffer_id()
            .expect("output sink buffer");
        rt.runtime_buffers()
            .get(id)
            .expect("runtime buffer")
            .value()
            .bytes
            .clone()
    }

    fn loaded_basic_runtime() -> LoadedProjectRuntime {
        let fs = examples_basic_fs();
        let services = EngineServices::new(TreePath::parse("/basic.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load examples/basic");
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        rt
    }

    /// Records every pressure broadcast the engine delivers, so tests can pin
    /// WHEN the seam fires relative to compiles.
    struct PressureProbe {
        levels: Rc<core::cell::RefCell<alloc::vec::Vec<crate::node::PressureLevel>>>,
    }

    impl crate::node::NodeRuntime for PressureProbe {
        fn destroy(
            &mut self,
            _ctx: &mut crate::node::DestroyCtx,
        ) -> Result<(), crate::node::NodeError> {
            Ok(())
        }

        fn handle_memory_pressure(
            &mut self,
            level: crate::node::PressureLevel,
            _ctx: &mut crate::node::MemPressureCtx,
        ) -> Result<(), crate::node::NodeError> {
            self.levels.borrow_mut().push(level);
            Ok(())
        }
    }

    fn attach_pressure_probe(
        rt: &mut Engine,
    ) -> Rc<core::cell::RefCell<alloc::vec::Vec<crate::node::PressureLevel>>> {
        let levels = Rc::new(core::cell::RefCell::new(alloc::vec::Vec::new()));
        let root = rt.tree().root();
        let frame = rt.revision();
        let id = rt
            .tree_mut()
            .add_child(
                root,
                NodeName::parse("pressure_probe").expect("name"),
                NodeName::parse("probe").expect("ty"),
                lpc_wire::WireChildKind::Input {
                    source: lpc_wire::WireSlotIndex(0),
                },
                lpc_model::NodeInvocation::new(lpc_model::ArtifactSpec::path("probe.json")),
                frame,
            )
            .expect("add probe child");
        rt.attach_runtime_node(
            id,
            alloc::boxed::Box::new(PressureProbe {
                levels: Rc::clone(&levels),
            }),
            frame,
        )
        .expect("attach probe");
        levels
    }

    /// The compile window is keyed to the ENGINE's frame revision, not the
    /// ambient [`lpc_model::current_revision`] counter.
    ///
    /// `open_compile_window` stamps the tick's revision, and a node only
    /// compiles when its render context reports that same revision. Reading
    /// the render revision from the process-global ambient counter instead
    /// broke the match whenever anything advanced that counter between the
    /// tick's `advance_revision` and the render — a second engine in the
    /// process, or parallel tests sharing a binary. The node then deferred
    /// the compile it had just been granted a window for, and a studio
    /// apply-then-read-status round trip saw no compile error at all.
    ///
    /// The probe stands in for that other advancer: pressure is broadcast at
    /// the top of the tick, after the frame revision is stamped and before
    /// any render, so bumping the ambient counter there reproduces the
    /// desync deterministically. See
    /// `docs/defects/2026-08-03-render-context-revision-read-from-ambient-counter.md`.
    #[test]
    fn compile_window_survives_an_ambient_revision_bump_inside_the_tick() {
        struct RevisionBumper;

        impl crate::node::NodeRuntime for RevisionBumper {
            fn destroy(
                &mut self,
                _ctx: &mut crate::node::DestroyCtx,
            ) -> Result<(), crate::node::NodeError> {
                Ok(())
            }

            fn handle_memory_pressure(
                &mut self,
                _level: crate::node::PressureLevel,
                _ctx: &mut crate::node::MemPressureCtx,
            ) -> Result<(), crate::node::NodeError> {
                lpc_model::advance_revision();
                Ok(())
            }
        }

        let mut rt = loaded_basic_runtime();
        let root = rt.tree().root();
        let frame = rt.revision();
        let id = rt
            .tree_mut()
            .add_child(
                root,
                NodeName::parse("revision_bumper").expect("name"),
                NodeName::parse("probe").expect("ty"),
                lpc_wire::WireChildKind::Input {
                    source: lpc_wire::WireSlotIndex(0),
                },
                lpc_model::NodeInvocation::new(lpc_model::ArtifactSpec::path("probe.json")),
                frame,
            )
            .expect("add bumper child");
        rt.attach_runtime_node(id, alloc::boxed::Box::new(RevisionBumper), frame)
            .expect("attach bumper");

        rt.tick(40).expect("deferral tick");
        rt.tick(40).expect("window tick");

        assert!(
            output_buffer_bytes(&rt, "/output.json")
                .iter()
                .any(|byte| *byte != 0),
            "the compile ran inside the window frame even though the ambient \
             revision moved after the frame revision was stamped"
        );
    }

    /// After a `High` pressure broadcast at a safe point, the next frame's
    /// published output bytes are bit-identical to an engine that never got
    /// the broadcast. Core path only — display-pipeline temporal state
    /// (dither, interpolation) is firmware-side and exempt anyway; see
    /// docs/adr/2026-08-03-gravy-features-out-of-core-correctness-tests.md.
    ///
    /// This started life (#303) as the drop→rebuild differential: the fixture
    /// and output nodes dropped their per-LED buffers at `High`, and this
    /// pinned that the lazy `ensure_*` seams rebuilt them to identical bytes.
    /// M6 P4 removed those drops — they freed nothing at the compile instant,
    /// because the compile runs at RENDER time and every dropped buffer was
    /// rebuilt earlier in the same tick
    /// (`docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`).
    /// The assertion is unchanged and now pins the stronger, simpler property:
    /// a pressure broadcast is **inert** on the core path. Per-node no-drop
    /// coverage lives next to each handler
    /// (`memory_pressure_does_not_drop_the_fixtures_derived_caches`,
    /// `memory_pressure_does_not_drop_the_control_samples`); if a future
    /// droppable is added back at `High`, this test is the identity guard it
    /// must satisfy.
    #[test]
    fn memory_pressure_broadcast_leaves_the_core_path_bit_identical() {
        let warm = || {
            let mut rt = loaded_basic_runtime();
            // Frame 1 defers the shader compile (window request); frame 2
            // opens the window, compiles, and renders for real.
            rt.tick(40).expect("tick 1");
            rt.tick(40).expect("tick 2");
            rt
        };

        let mut reference = warm();
        let mut dropped = warm();

        dropped
            .broadcast_memory_pressure(crate::node::PressureLevel::High)
            .expect("broadcast pressure");

        reference.tick(40).expect("reference tick");
        dropped.tick(40).expect("dropped tick");

        let expected = output_buffer_bytes(&reference, "/output.json");
        let actual = output_buffer_bytes(&dropped, "/output.json");
        assert!(
            expected.iter().any(|byte| *byte != 0),
            "reference output must not be black — a black baseline proves nothing"
        );
        assert_eq!(
            expected, actual,
            "a memory-pressure broadcast must not change core-path output bytes"
        );
    }

    /// The boot path: the first frame defers the compile and requests a
    /// window; the engine broadcasts exactly one `High` pressure at the top of
    /// the next tick, and the compile runs inside that same frame.
    #[test]
    fn compile_window_broadcasts_pressure_before_the_boot_compile() {
        let mut rt = loaded_basic_runtime();
        let levels = attach_pressure_probe(&mut rt);

        rt.tick(40).expect("boot tick");
        assert!(
            levels.borrow().is_empty(),
            "no pressure before any node requested a compile window"
        );
        assert!(
            output_buffer_bytes(&rt, "/output.json")
                .iter()
                .all(|byte| *byte == 0),
            "the deferral frame renders the black fallback"
        );

        rt.tick(40).expect("window tick");
        assert_eq!(
            &*levels.borrow(),
            &[crate::node::PressureLevel::High],
            "exactly one High broadcast opens the compile window"
        );
        assert!(
            output_buffer_bytes(&rt, "/output.json")
                .iter()
                .any(|byte| *byte != 0),
            "the compile ran inside the window frame, after the broadcast"
        );

        rt.tick(40).expect("steady tick");
        assert_eq!(
            levels.borrow().len(),
            1,
            "steady state broadcasts nothing — the window is not a per-frame event"
        );
    }

    /// The switch path: activating a playlist entry whose shader never
    /// compiled requests a fresh window, and pressure fires again before that
    /// compile — the worse peak the seam exists for (the transient would land
    /// on already-allocated per-LED buffers).
    #[test]
    fn playlist_switch_requests_a_second_compile_window() {
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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let levels = attach_pressure_probe(&mut rt);

        // Boot: deferral frame, then the window frame compiles the idle shader.
        tick_with_test_time(&mut rt, &time, 16, "boot deferral");
        tick_with_test_time(&mut rt, &time, 16, "boot window");
        let boot_broadcasts = levels.borrow().len();
        assert_eq!(boot_broadcasts, 1, "boot opens one compile window");

        // Switch: the active-entry shader has never compiled. The switch
        // frame defers it; the following tick must broadcast again.
        control.set_pressed(HwAddress::gpio(20), true);
        tick_with_test_time(&mut rt, &time, 16, "press candidate");
        tick_with_test_time(&mut rt, &time, 30, "press stable — switch, deferral");
        tick_with_test_time(&mut rt, &time, 16, "switch window");
        assert_eq!(
            levels.borrow().len(),
            2,
            "the playlist switch must open a second compile window"
        );
        assert!(
            levels
                .borrow()
                .iter()
                .all(|level| *level == crate::node::PressureLevel::High),
            "compile windows broadcast High, never Critical"
        );
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
    fn project_loader_loads_inline_clock_and_default_time_product_bus() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
      "default": 0.0,
      "default_bind": "bus:time"
    }
  }
}
"#,
        )
        .expect("shader.json");
        fs.write_file(
            "/shader.glsl".as_path(),
            b"vec4 render_2d(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
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

        let read_time_bus = |rt: &mut LoadedProjectRuntime| {
            rt.resolve_with_engine_host(
                QueryKey::Bus {
                    scope: None,
                    channel: ChannelName(String::from("time")),
                },
                ResolveLogLevel::Off,
            )
            .expect("resolve time bus")
            .0
            .value_leaf()
            .expect("time value")
            .value()
            .clone()
        };
        let read_clock_seconds = |rt: &mut LoadedProjectRuntime| {
            rt.resolve_with_engine_host(
                QueryKey::ProducedSlot {
                    node: clock,
                    slot: SlotPath::parse("seconds").expect("seconds slot"),
                },
                ResolveLogLevel::Off,
            )
            .expect("resolve clock seconds")
            .0
            .value_leaf()
            .expect("seconds value")
            .value()
            .clone()
        };

        rt.tick(1000).expect("first tick");
        // The channel carries a HANDLE, not a number: the value is stable
        // across ticks while the timebase behind it advances. Raw seconds stay
        // readable on the clock's own produced slot (card face, probes).
        let handle = LpValue::Product(lpc_model::ProductRef::Time(lpc_model::TimeProduct::new(
            clock, 0,
        )));
        assert_eq!(read_time_bus(&mut rt), handle);
        assert_eq!(read_clock_seconds(&mut rt), LpValue::F32(0.0));

        rt.tick(1000).expect("second tick");
        assert_eq!(read_time_bus(&mut rt), handle);
        assert_eq!(read_clock_seconds(&mut rt), LpValue::F32(1.0));

        // The stale f32 uniform still RESOLVES the product (kind mismatch is a
        // conversion failure, not a resolve failure) — and the shader card
        // says so out loud instead of silently freezing (#316, D12).
        let shader_time = rt
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
            *shader_time.value_leaf().expect("time value").value(),
            handle
        );
        // Nothing consumes this shader's visual, so pull on its produced
        // output to make it run its uniform fill.
        rt.resolve_with_engine_host(
            QueryKey::ProducedSlot {
                node: shader,
                slot: SlotPath::parse("output").expect("output slot"),
            },
            ResolveLogLevel::Off,
        )
        .expect("resolve shader output");
        let status = rt.tree().get(shader).expect("shader entry").status.value();
        let lpc_model::NodeRuntimeStatus::Warn(message) = status else {
            panic!("an f32 uniform on bus:time must warn, got {status:?}");
        };
        assert!(
            message.contains("input \"time\" using its default"),
            "the warn names the input and the fallback: {message}"
        );
    }

    /// The happy path the break exists for: an ordinary clock, an ordinary
    /// phasor uniform, and no authored wiring anywhere. The clock's
    /// `product` default-publish carries `bus:time`, the shader's evaluator
    /// resolves it in the reader's scope, and the uniform walks its cycle.
    #[test]
    fn a_phasor_uniform_rides_the_clocks_default_time_product_with_no_authoring() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "compute": { "ref": "./compute.json" }
  }
}
"#,
        )
        .expect("module.json");
        fs.write_file("/clock.json".as_path(), br#"{ "kind": "Clock" }"#)
            .expect("clock.json");
        fs.write_file(
            "/compute.json".as_path(),
            br#"
{
  "kind": "ComputeShader",
  "source": { "path": "compute.glsl" },
  "consumed": {
    "wave": { "kind": "phasor", "value": "f32",
              "phasor": { "period_seconds": 4.0, "waveform": "ramp",
                          "phase_offset": 0.0 } }
  },
  "produced": { "out_wave": { "kind": "value", "value": "f32" } }
}
"#,
        )
        .expect("compute.json");
        fs.write_file(
            "/compute.glsl".as_path(),
            b"void tick() { out_wave = wave; }",
        )
        .expect("compute.glsl");

        let services = EngineServices::new(TreePath::parse("/phasor.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(&fs, services).expect("load");
        rt.engine_mut()
            .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));
        let compute = sibling(&rt, "compute");

        let read_wave = |rt: &mut LoadedProjectRuntime| {
            rt.resolve_with_engine_host(
                QueryKey::ProducedSlot {
                    node: compute,
                    slot: SlotPath::parse("out_wave").expect("slot"),
                },
                ResolveLogLevel::Off,
            )
            .expect("resolve out_wave")
            .0
            .value_leaf()
            .expect("value")
            .value()
            .clone()
        };

        let mut seen = alloc::vec::Vec::new();
        for _ in 0..4 {
            rt.tick(500).expect("tick");
            seen.push(read_wave(&mut rt));
        }

        assert_eq!(
            rt.tree().get(compute).expect("entry").status.value(),
            &lpc_model::NodeRuntimeStatus::Ok,
            "no authored wiring is needed and nothing warns"
        );
        let LpValue::F32(last) = seen.last().expect("a sample") else {
            panic!("expected f32 samples: {seen:?}");
        };
        assert!(
            *last > 0.0,
            "the phasor advanced off the top of its cycle: {seen:?}"
        );
    }

    #[test]
    fn project_loader_rejects_inline_child_def() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
            b"vec4 render_2d(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
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
      "source": "node:../texture#output"
    },
    "output": {
      "target": "bus:control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "PointList",
        "first_channel": 0,
        "points": {
          "0": [
            0.5,
            0.5
          ]
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
    fn playlist_entry_children_publish_into_their_sink_scope_only() {
        // The old ownership-suppression rule is GONE: entry children
        // default-publish `visual.out` like every producer — into their
        // entry's sink scope, where writer-shadowing (R2/R5) keeps them
        // invisible to any enclosing read by construction.
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

        // The binding exists…
        assert!(rt.tree().bindings().any(|binding| {
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
        // …its owner writes a SINK scope…
        let active_scope = rt.tree().node_scope(active).expect("active scope");
        assert!(active_scope.is_sink());
        // …and a root-scoped read never selects it: the winning provider
        // set for the root scope contains no entry-child publisher.
        let root_scope = rt.tree().node_scope(root).expect("root scope");
        let winners = rt.tree().providers_for_bus_read(
            Some(root_scope),
            &lpc_model::ChannelName(String::from("visual.out")),
        );
        assert!(
            winners.iter().all(|(_, entry)| entry.owner != active),
            "sink-scope publishers must be invisible to root-scope demand"
        );
    }

    #[test]
    fn playlist_entries_load_as_children_and_bind_root_trigger() {
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
                    && slot == &SlotPath::parse("trigger").expect("trigger")
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
    fn playlist_duplicate_trigger_id_claims_resolve_to_lowest_entry_index() {
        let fs = button_playlist_project_fs();
        fs.write_file(
            "/playlist.json".as_path(),
            br#"
{
  "kind": "Playlist",
  "default_fade": 0.35,
  "bindings": {
    "time": {
      "source": "bus:time"
    },
    "trigger": {
      "source": "bus:trigger"
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
      "trigger_ids": [1],
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
      }
    },
    "3": {
      "name": "second",
      "trigger_ids": [1],
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
      }
    }
  }
}
"#,
        )
        .expect("playlist.json");
        let (mut rt, playlist, control) = load_button_playlist(&fs);

        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        control.set_pressed(HwAddress::gpio(20), true);
        let _ = resolve_playlist_u32(&mut rt, playlist, "active_entry");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 2);
    }

    #[test]
    fn playlist_trigger_id_not_claimed_by_any_entry_does_nothing() {
        let fs = button_playlist_project_fs();
        fs.write_file(
            "/playlist.json".as_path(),
            br#"
{
  "kind": "Playlist",
  "default_fade": 0.35,
  "bindings": {
    "time": {
      "source": "bus:time"
    },
    "trigger": {
      "source": "bus:trigger"
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
      "trigger_ids": [9],
      "duration": 4.0,
      "node": {
        "ref": "./active.json"
      }
    }
  }
}
"#,
        )
        .expect("playlist.json");
        let (mut rt, playlist, control) = load_button_playlist(&fs);

        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        control.set_pressed(HwAddress::gpio(20), true);
        let _ = resolve_playlist_u32(&mut rt, playlist, "active_entry");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
        rt.tick(1000).expect("advance time");
        assert_eq!(resolve_playlist_u32(&mut rt, playlist, "active_entry"), 1);
    }

    #[test]
    fn malformed_child_node_json_projects_error_node() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
    fn missing_project_json_refuses_via_the_manifest_gate() {
        // D-A: no `project.json` container manifest = hard refuse before
        // anything parses; the error names the manifest, not a deep Io path.
        let fs = LpFsMemory::new();
        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        let text = err.to_string();
        assert!(text.contains("project.json"), "{text}");
        assert!(text.contains("manifest"), "{text}");
    }

    #[test]
    fn missing_module_json_returns_io_error() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        let root_path = TreePath::parse("/p.show").expect("path");
        let services = EngineServices::new(root_path);
        let err = match ProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(
                err,
                ProjectLoadError::Io { .. } | ProjectLoadError::ProjectParse { .. }
            ),
            "expected Io/parse, got {err:?}"
        );
    }

    #[test]
    fn unknown_child_kind_projects_error_node() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
      "source": "node:../missing#output"
    },
    "output": {
      "target": "bus:control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "PointList",
        "first_channel": 0,
        "points": {
          "0": [
            0.5,
            0.5
          ]
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
                    if reason.contains("unknown binding source node ref `../missing`")
            ),
            "expected missing binding source ref, got {err:?}"
        );
    }

    #[test]
    fn schemeless_node_ref_projects_error_node() {
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
      "target": "bus:control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "PointList",
        "first_channel": 0,
        "points": {
          "0": [
            0.5,
            0.5
          ]
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

        assert_node_for_def_error(&rt, "/fixture.json", "must start with `bus:` or `node:`");
    }

    #[test]
    fn project_loader_attaches_compute_shader_node() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let node = node_for_def_path(&rt, "/compute.json").expect("compute node");

        // First resolve requests a compile window (deferral); the second
        // compiles under the at-most-once progress guarantee.
        rt.resolve_with_engine_host(
            QueryKey::ProducedSlot {
                node,
                slot: SlotPath::parse("phase").expect("phase"),
            },
            ResolveLogLevel::Off,
        )
        .expect("warm-up resolve");
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

    /// The defaulting rule, end to end: `examples/fluid` predates the `power`
    /// slot and authors none, yet its fixture must still come up limited. The
    /// budget the node publishes is the one actually enforced, so asserting on
    /// it also pins that the UI cannot report a percentage against a budget
    /// nothing is applying.
    #[test]
    fn fixture_without_an_authored_power_slot_is_limited_at_the_default_budget() {
        let fs = examples_fluid_fs();
        let fs: &dyn LpFs = &fs;
        let services = EngineServices::new(TreePath::parse("/fluid.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(fs, services).expect("load fluid example");
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
        let root = rt.tree().root();
        let fixture = rt
            .tree()
            .lookup_sibling(root, NodeName::parse("fixture").unwrap())
            .expect("fixture node");

        let budget = rt
            .resolve_with_engine_host(
                QueryKey::ProducedSlot {
                    node: fixture,
                    slot: SlotPath::parse("power_budget_ma").expect("power_budget_ma"),
                },
                ResolveLogLevel::Off,
            )
            .expect("resolve power budget")
            .0;

        assert_eq!(
            *budget.value_leaf().expect("value").value(),
            LpValue::U32(lpc_model::nodes::fixture::FixturePower::DEFAULT_BUDGET_MA),
            "an unstated budget must fall back to the default guard, not to unlimited"
        );
    }

    #[test]
    fn fluid_example_loads_compute_fluid_fixture_flow() {
        let fs = examples_fluid_fs();
        let fs: &dyn LpFs = &fs;
        let services = EngineServices::new(TreePath::parse("/fluid.show").expect("path"));
        let mut rt = ProjectLoader::load_from_root(fs, services).expect("load fluid example");
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
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

        // First resolve requests a compile window (deferral); the second
        // compiles under the at-most-once progress guarantee.
        rt.resolve_with_engine_host(
            QueryKey::ProducedSlot {
                node: compute,
                slot: SlotPath::parse("emitters").expect("emitters"),
            },
            ResolveLogLevel::Off,
        )
        .expect("warm-up resolve");
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
                    space: VisualSpace::TwoD,
                    policy: ConsumerPolicy::default(),
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
                        space: None,
                        policy: None,
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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));

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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));

        rt.tick(16).expect("tick fyeah-sign without radio cycle");
    }

    /// The palette examples (M5) compile and render.
    ///
    /// The `ticks_without_radio_trigger_cycle` tests above prove they LOAD,
    /// which is deliberately not enough here: a shader whose compile fails
    /// still ticks, it just renders the black fallback
    /// (`docs/defects/2026-07-…events render flake` class). These examples
    /// took an authored `sampler2D palette` slot in place of hand-rolled
    /// cosine palettes, so "the frame after the compile window carries
    /// color" is what pins the whole chain end to end: the authored
    /// `GradientConfig` parses, the strip bakes, the sampler binds, and the
    /// body samples it.
    #[test]
    fn palette_examples_render_after_the_compile_window() {
        // `plasma-duo` names two outputs deliberately: one palette channel
        // feeding two fixtures is the thing it proves that `plasma` cannot.
        for (label, fs, outputs) in [
            (
                "fyeah-sign",
                examples_fyeah_sign_fs(),
                &["/output.json"][..],
            ),
            (
                "fyeah-button",
                examples_fyeah_button_fs(),
                &["/output.json"][..],
            ),
            (
                "button-sign",
                examples_button_sign_fs(),
                &["/output.json"][..],
            ),
            (
                "button-playlist",
                examples_button_playlist_fs(),
                &["/output.json"][..],
            ),
            ("plasma", examples_plasma_fs(), &["/output.json"][..]),
            (
                "plasma-duo",
                examples_plasma_duo_fs(),
                &["/disc_out.json", "/grid_out.json"][..],
            ),
        ] {
            let fs: &dyn LpFs = &fs;
            let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
            let hardware = Rc::new(HardwareSystem::with_virtual_drivers(registry));
            let button_service: Rc<dyn ButtonService> = hardware.clone();
            let radio_service: Rc<dyn RadioService> = hardware.clone();
            let mut services = EngineServices::new(TreePath::parse("/palette.show").expect("path"));
            services.set_button_service(Some(button_service));
            services.set_radio_service(Some(radio_service));

            let mut rt = ProjectLoader::load_from_root(fs, services)
                .unwrap_or_else(|e| panic!("load {label}: {e:?}"));
            rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
                lp_shader::ShaderFrontend::LpsGlsl,
            ))));

            // Frame 1 defers the shader compile and requests a window;
            // frame 2 opens it, compiles, and renders for real.
            rt.tick(40)
                .unwrap_or_else(|e| panic!("{label} tick 1: {e:?}"));
            rt.tick(40)
                .unwrap_or_else(|e| panic!("{label} tick 2: {e:?}"));

            for output in outputs {
                assert!(
                    output_buffer_bytes(&rt, output)
                        .iter()
                        .any(|byte| *byte != 0),
                    "{label} rendered black at {output} — its palette shader did \
                     not compile or the strip did not reach the sampler"
                );
            }
        }
    }

    #[test]
    fn fyeah_sign_binding_graph_reports_full_topology() {
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
        let fixture = rt
            .tree()
            .lookup_sibling(rt.tree().root(), NodeName::parse("fixture").unwrap())
            .expect("fixture");
        let clock = rt
            .tree()
            .lookup_sibling(rt.tree().root(), NodeName::parse("clock").unwrap())
            .expect("clock");

        let (engine, project_registry) = rt.read_parts();
        let result = engine.read_project_binding_graph_probe(
            project_registry,
            lpc_wire::BindingGraphProbeRequest {
                include_values: false,
            },
        );

        let lpc_wire::BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected binding graph");
        };
        let channel = |name: &str| {
            graph
                .channels
                .iter()
                .find(|channel| channel.name == name)
                .unwrap_or_else(|| panic!("channel {name} missing"))
        };

        // Two writers on trigger (button + radio), two readers (playlist +
        // radio bridge).
        assert!(channel("trigger").providers.len() >= 2);
        assert!(channel("trigger").consumers.len() >= 2);

        // The fixture consumes visual.out through its implicit runtime
        // `input` slot — no def field exists, the binding index still knows.
        let visual_consumers = &channel("visual.out").consumers;
        assert!(visual_consumers.iter().any(|index| {
            let binding = &graph.bindings[*index as usize];
            binding.node == fixture
                && binding.slot == Some(SlotPath::parse("input").expect("path"))
                && binding.direction == lpc_wire::WireBindingDirection::Consumes
                && binding.origin == lpc_wire::WireBindingOrigin::Authored
        }));

        // Clock publishes time.seconds via the default (loader helper)
        // binding — visible and tagged as default origin.
        let time_providers = &channel("time").providers;
        assert!(time_providers.iter().any(|index| {
            let binding = &graph.bindings[*index as usize];
            binding.node == clock && binding.origin == lpc_wire::WireBindingOrigin::Default
        }));
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
        rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));

        rt.tick(16).expect("tick fyeah-button without radio cycle");
    }

    #[test]
    fn button_node_publishes_held_and_up_from_virtual_d9() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
  "endpoint": "button:local:D9",
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
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
  "endpoint": "button:local:D9",
  "stable_ms": 1,
  "bindings": {
    "down": {
      "target": "bus:trigger"
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
  "endpoint": "radio:local:0",
  "channel": 1,
  "repeat_count": 2,
  "bindings": {
    "input": {
      "source": "bus:trigger"
    },
    "output": {
      "target": "bus:trigger"
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
                space: VisualSpace::TwoD,
                policy: ConsumerPolicy::default(),
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

    fn load_button_playlist(
        fs: &LpFsMemory,
    ) -> (LoadedProjectRuntime, NodeId, VirtualButtonDriver) {
        let registry = Rc::new(HwRegistry::new(default_esp32c6_hardware_manifest()));
        let driver = VirtualButtonDriver::new(Rc::clone(&registry));
        let control = driver.clone();
        let mut hardware = HardwareSystem::new(registry);
        hardware.add_button_driver(Box::new(driver));
        let hardware = Rc::new(hardware);
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let mut services = EngineServices::new(TreePath::parse("/button_playlist.show").unwrap());
        services.set_button_service(Some(button_service));
        let rt = ProjectLoader::load_from_root(fs, services).expect("load playlist");
        let playlist = rt
            .tree()
            .lookup_sibling(rt.tree().root(), NodeName::parse("playlist").unwrap())
            .expect("playlist");
        (rt, playlist, control)
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
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
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
      "target": "bus:visual.out"
    }
  }
}
"#,
        )
        .expect("shader.json");
        fs.write_file(
            "/shader.glsl".as_path(),
            b"vec4 render_2d(vec2 pos) { return vec4(pos, 0.0, 1.0); }",
        )
        .expect("shader.glsl");
        fs.write_file(
            "/output.json".as_path(),
            br#"
{
  "kind": "Output",
  "channels": {
    "0": {
      "endpoint": "ws281x:local:D10"
    }
  },
  "bindings": {
    "input": {
      "source": "bus:control.out"
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
      "source": "bus:visual.out"
    },
    "output": {
      "target": "bus:control.out"
    }
  },
  "mapping": {
    "kind": "PathPoints",
    "sample_diameter": 2.0,
    "paths": {
      "0": {
        "kind": "PointList",
        "first_channel": 0,
        "points": {
          "0": [
            0.5,
            0.5
          ]
        }
      }
    }
  }
}
"#,
        )
        .expect("fixture.json");
    }
    // --- Default-binding characterization (M5, ADR 2026-07-09) -------------
    //
    // These tests pin the CURRENT loader-helper behavior before the swap to
    // declarative slot-declared defaults. The generic materialization pass
    // must keep every green assertion here green — except where the ADR
    // deliberately changes behavior, each called out inline.

    fn default_publishes(
        rt: &LoadedProjectRuntime,
        node: NodeId,
        slot: &str,
        channel: &str,
    ) -> bool {
        rt.tree().bindings().any(|binding| {
            binding.priority == BindingPriority::default_fallback()
                && matches!(
                    (&binding.source, &binding.target),
                    (
                        BindingSource::ProducedSlot { node: n, slot: s },
                        BindingTarget::BusChannel(c),
                    ) if *n == node && s == &SlotPath::parse(slot).expect("slot") && c.0 == channel
                )
        })
    }

    fn default_sources(rt: &LoadedProjectRuntime, node: NodeId, slot: &str, channel: &str) -> bool {
        rt.tree().bindings().any(|binding| {
            binding.priority == BindingPriority::default_fallback()
                && matches!(
                    (&binding.source, &binding.target),
                    (
                        BindingSource::BusChannel(c),
                        BindingTarget::ConsumedSlot { node: n, slot: s },
                    ) if *n == node && s == &SlotPath::parse(slot).expect("slot") && c.0 == channel
                )
        })
    }

    fn sibling(rt: &LoadedProjectRuntime, name: &str) -> NodeId {
        let root = rt.tree().root();
        rt.tree()
            .lookup_sibling(root, NodeName::parse(name).unwrap())
            .unwrap_or_else(|| panic!("{name} node"))
    }

    fn load_project(fs: &LpFsMemory) -> LoadedProjectRuntime {
        let services = EngineServices::new(TreePath::parse("/char.show").expect("path"));
        ProjectLoader::load_from_root(fs, services).expect("load characterization project")
    }

    fn char_project(nodes: &[(&str, &str)]) -> LpFsMemory {
        let fs = LpFsMemory::new();
        let mut entries = String::new();
        for (index, (name, _)) in nodes.iter().enumerate() {
            if index > 0 {
                entries.push_str(",\n");
            }
            entries.push_str(&format!("    \"{name}\": {{ \"ref\": \"./{name}.json\" }}"));
        }
        let module = format!("{{\n  \"kind\": \"Module\",\n  \"nodes\": {{\n{entries}\n  }}\n}}\n");
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file("/module.json".as_path(), module.as_bytes())
            .expect("module.json");
        for (name, json) in nodes {
            fs.write_file(format!("/{name}.json").as_str().as_path(), json.as_bytes())
                .unwrap_or_else(|_| panic!("{name}.json"));
        }
        fs
    }

    /// Fix for
    /// `docs/defects/2026-08-02-authored-source-bindings-silently-dropped.md`:
    /// registration is driven by the declared slot shape, so an authored
    /// source binding on ANY declared slot registers. Fixture `brightness`
    /// is the defect's own example — and its target lands on
    /// `brightness.some`, the option-interior accessor path the runtime
    /// actually resolves (binding lookup is exact-path).
    #[test]
    fn authored_fixture_brightness_binding_registers_on_the_accessor_path() {
        let fs = char_project(&[(
            "fixture",
            r#"{ "kind": "Fixture",
                 "bindings": { "brightness": { "source": "bus:brightness" } } }"#,
        )]);
        let rt = load_project(&fs);
        let fixture = sibling(&rt, "fixture");
        assert!(rt.tree().bindings().any(|binding| matches!(
            (&binding.source, &binding.target),
            (BindingSource::BusChannel(channel), BindingTarget::ConsumedSlot { node, slot })
                if channel.0 == "brightness"
                    && *node == fixture
                    && slot == &SlotPath::parse("brightness.some").expect("path")
        )));
    }

    /// The defect doc's clock example: a nested declared value slot
    /// (`transport.rate`) takes an authored source binding.
    #[test]
    fn authored_clock_rate_binding_registers() {
        let fs = char_project(&[(
            "clock",
            r#"{ "kind": "Clock",
                 "bindings": { "transport.rate": { "source": "bus:rate" } } }"#,
        )]);
        let rt = load_project(&fs);
        let clock = sibling(&rt, "clock");
        assert!(rt.tree().bindings().any(|binding| matches!(
            (&binding.source, &binding.target),
            (BindingSource::BusChannel(channel), BindingTarget::ConsumedSlot { node, slot })
                if channel.0 == "rate"
                    && *node == clock
                    && slot == &SlotPath::parse("transport.rate").expect("path")
        )));
    }

    /// P6(a): a default clock materializes THREE fallback bindings, one per
    /// transport leaf, each sourcing from its own `clock.*` channel. The
    /// declarations live on the leaves (nested inside the `transport`
    /// record), so this is what the loader's record recursion buys.
    #[test]
    fn a_default_clock_materializes_three_transport_channels() {
        let fs = char_project(&[("clock", r#"{ "kind": "Clock" }"#)]);
        let rt = load_project(&fs);
        let clock = sibling(&rt, "clock");

        for (slot, channel) in TRANSPORT_LEAVES {
            assert!(
                default_sources(&rt, clock, slot, channel),
                "{slot} must source from {channel} at fallback priority"
            );
        }
        // Exactly three — the promoted RECORD itself declares no endpoint,
        // so nothing wires `transport` as a whole.
        let transport_bindings = rt
            .tree()
            .bindings()
            .filter(|binding| {
                matches!(&binding.target, BindingTarget::ConsumedSlot { node, slot }
                    if *node == clock && slot.to_string().starts_with("transport"))
            })
            .count();
        assert_eq!(transport_bindings, 3, "one wire per leaf, no record wire");
    }

    /// P6(b): an authored binding on ONE leaf suppresses only that leaf's
    /// declared default — its siblings keep their fallback wiring, and the
    /// authored source wins on the leaf it names.
    #[test]
    fn an_authored_transport_leaf_suppresses_only_its_own_default() {
        let fs = char_project(&[(
            "clock",
            r#"{ "kind": "Clock",
                 "bindings": { "transport.rate": { "source": "bus:speed" } } }"#,
        )]);
        let rt = load_project(&fs);
        let clock = sibling(&rt, "clock");

        assert!(
            !default_sources(&rt, clock, "transport.rate", "clock.rate"),
            "the authored binding suppresses the declared default on `rate`"
        );
        assert!(rt.tree().bindings().any(|binding| matches!(
            (&binding.source, &binding.target),
            (BindingSource::BusChannel(channel), BindingTarget::ConsumedSlot { node, slot })
                if channel.0 == "speed"
                    && *node == clock
                    && slot == &SlotPath::parse("transport.rate").expect("path")
        )));
        assert!(default_sources(
            &rt,
            clock,
            "transport.play_state",
            "clock.play_state"
        ));
        assert!(default_sources(
            &rt,
            clock,
            "transport.scrub_offset_seconds",
            "clock.scrub"
        ));
    }

    /// P6(c): the three channels the transport names are the ones the
    /// well-known registry teaches, with the kinds it declares — nearest-fit
    /// legacy kinds whose true ranges live in the registry's doc strings.
    #[test]
    fn the_transport_channels_land_with_their_registry_kinds() {
        use lpc_model::Kind;
        use lpc_model::bus::well_known::well_known_channel;

        for (channel, kind) in [
            ("clock.play_state", Kind::Choice),
            ("clock.rate", Kind::Ratio),
            ("clock.scrub", Kind::Duration),
        ] {
            let entry = well_known_channel(channel)
                .unwrap_or_else(|| panic!("{channel} must be a well-known channel"));
            assert_eq!(entry.kind, kind, "{channel}");
            assert!(!entry.carries_product, "{channel} is a plain scalar");
            assert!(!entry.doc.is_empty(), "{channel} needs a picker doc");
        }
    }

    /// P6 probe: the RECORD-level `panel = "show"` on `ClockDef::transport`
    /// reaches every leaf's binding endpoint. Nothing in the engine changed
    /// for this — `authored_def_slot_panel_hint` already reads the hint off
    /// the TOP-LEVEL field of a binding's path, which is the whole point of
    /// declaring grouping on the record.
    #[test]
    fn the_record_panel_hint_reaches_each_transport_leaf_endpoint() {
        let fs = char_project(&[("clock", r#"{ "kind": "Clock" }"#)]);
        let mut rt = load_project(&fs);
        let clock = sibling(&rt, "clock");

        let (engine, project_registry) = rt.read_parts();
        let result = engine.read_project_binding_graph_probe(
            project_registry,
            lpc_wire::BindingGraphProbeRequest {
                include_values: false,
            },
        );
        let lpc_wire::BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected binding graph");
        };

        for (slot, channel) in TRANSPORT_LEAVES {
            let binding = graph
                .bindings
                .iter()
                .find(|binding| {
                    binding.node == clock
                        && binding
                            .slot
                            .as_ref()
                            .is_some_and(|path| path == &SlotPath::parse(slot).expect("slot path"))
                })
                .unwrap_or_else(|| panic!("no graph binding at {slot}"));
            assert!(
                binding.panel_show,
                "{slot} must inherit the record-level panel hint"
            );
            assert!(matches!(
                &binding.endpoint,
                lpc_wire::WireBindingEndpoint::Bus { channel: name, .. } if name == channel
            ));
        }
    }

    /// The whole P6 chain, end to end: a panel write on each `clock.*`
    /// channel reaches the clock's own per-field read. This is what three
    /// leaf channels buy over one record channel — each dimension is
    /// patched on its own, with no read-modify-write anywhere.
    #[test]
    fn writing_the_clock_channels_drives_the_clock() {
        use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};

        let fs = char_project(&[("clock", r#"{ "kind": "Clock" }"#)]);
        let mut rt = load_project(&fs);
        let clock = sibling(&rt, "clock");
        let scope = rt.tree().node_scope(clock).expect("clock scope");
        let read = |rt: &mut LoadedProjectRuntime, slot: &str| {
            let key = QueryKey::ConsumedSlot {
                node: clock,
                slot: SlotPath::parse(slot).expect("path"),
            };
            rt.resolve_with_engine_host(key, ResolveLogLevel::Off)
                .expect("resolve")
                .0
                .value_leaf()
                .map(|leaf| leaf.value().clone())
        };

        // No writer: the shape defaults read through the fallback binding's
        // writerless dead-end, exactly as fixture brightness does.
        assert_eq!(
            read(&mut rt, "transport.play_state"),
            Some(LpValue::String(String::from("playing")))
        );
        assert_eq!(read(&mut rt, "transport.rate"), Some(LpValue::F32(1.0)));

        for (channel, value) in [
            (
                "clock.play_state",
                LpValue::String(String::from(lpc_model::PlayState::Paused.as_str())),
            ),
            ("clock.rate", LpValue::F32(2.0)),
            ("clock.scrub", LpValue::F32(-3.5)),
        ] {
            rt.engine_mut()
                .panel_write(scope, ChannelName(String::from(channel)), value, None);
        }

        assert_eq!(
            read(&mut rt, "transport.play_state"),
            Some(LpValue::String(String::from("paused"))),
            "a write on clock.play_state pauses the transport"
        );
        assert_eq!(
            read(&mut rt, "transport.rate"),
            Some(LpValue::F32(2.0)),
            "…and the rate leaf took its own channel's value, untouched by it"
        );
        assert_eq!(
            read(&mut rt, "transport.scrub_offset_seconds"),
            Some(LpValue::F32(-3.5)),
            "…and so did scrub — three independent wires, no record RMW"
        );
    }

    /// The clock transport's declared leaf wiring, in declaration order.
    const TRANSPORT_LEAVES: [(&str, &str); 3] = [
        ("transport.play_state", "clock.play_state"),
        ("transport.rate", "clock.rate"),
        ("transport.scrub_offset_seconds", "clock.scrub"),
    ];

    /// A binding key nobody declares fails the load with a reason naming
    /// the slot — the loud replacement for the silent drop.
    #[test]
    fn a_binding_naming_no_declared_slot_fails_the_load() {
        let fs = char_project(&[(
            "fixture",
            r#"{ "kind": "Fixture",
                 "bindings": { "brightnes": { "source": "bus:brightness" } } }"#,
        )]);
        let services = EngineServices::new(TreePath::parse("/char.show").expect("path"));
        let Err(err) = ProjectLoader::load_from_root(&fs, services) else {
            panic!("typo must not vanish");
        };
        let reason = format!("{err}");
        assert!(
            reason.contains("brightnes") && reason.contains("names no declared slot"),
            "got: {reason}"
        );
    }

    /// The scarf path at the engine (panel.md P10): every fixture's
    /// brightness is default-bound to `bus:brightness` (with
    /// `panel = "show"`), so a panel writer on the fixture's scope dims the
    /// value the render path reads — and with no writer, the authored
    /// default reads through the binding's writerless dead-end (R6 via the
    /// resolver's fallback).
    #[test]
    fn a_panel_brightness_write_reaches_the_fixture_read() {
        use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};

        let fs = char_project(&[("fixture", r#"{ "kind": "Fixture", "brightness": 0.5 }"#)]);
        let mut rt = load_project(&fs);
        let fixture = sibling(&rt, "fixture");
        let scope = rt.tree().node_scope(fixture).expect("fixture scope");
        let key = || QueryKey::ConsumedSlot {
            node: fixture,
            slot: SlotPath::parse("brightness.some").expect("path"),
        };

        let (authored, _) = rt
            .resolve_with_engine_host(key(), ResolveLogLevel::Off)
            .expect("resolve authored");
        assert_eq!(
            authored.value_leaf().map(|leaf| leaf.value().clone()),
            Some(LpValue::F32(0.5)),
            "no writer anywhere: the authored default reads through the R6 fallback"
        );

        rt.engine_mut().panel_write(
            scope,
            ChannelName(String::from("brightness")),
            LpValue::F32(0.1),
            None,
        );
        let (held, _) = rt
            .resolve_with_engine_host(key(), ResolveLogLevel::Off)
            .expect("resolve held");
        assert_eq!(
            held.value_leaf().map(|leaf| leaf.value().clone()),
            Some(LpValue::F32(0.1)),
            "the engaged writer dims what the fixture's render path reads"
        );
    }

    // The time default is declared on the slot-def (ADR 2026-07-09) — the
    // old "any unbound f32 time input" global convention is retired.
    const CHAR_SHADER_WITH_TIME: &str = r#"
{
  "kind": "Shader",
  "source": { "path": "shader.glsl" },
  "render_order": 0,
  "consumed": {
    "time": { "kind": "value", "value": "f32", "default": 0.0,
              "default_bind": "bus:time" }
  }
}
"#;

    const CHAR_SHADER_GLSL: &[u8] = b"vec4 render_2d(vec2 pos) { return vec4(pos, 0.0, 1.0); }";

    // A compute shader declares the same default-bound `time` slot the
    // starter does (`starter_time_consumed_slots`), plus one produced slot
    // so the def is representative of an authored compute node.
    const CHAR_COMPUTE_WITH_TIME: &str = r#"
{
  "kind": "ComputeShader",
  "source": { "path": "compute.glsl" },
  "consumed": {
    "time": { "kind": "value", "value": "f32", "default": 0.0,
              "default_bind": "bus:time" }
  },
  "produced": {
    "phase": { "kind": "value", "value": "f32", "default": 0.0 }
  }
}
"#;

    const CHAR_COMPUTE_GLSL: &[u8] = b"void compute() { phase = fract(time); }";

    #[test]
    fn char_minimal_clock_publishes_time_default_only_for_the_product() {
        // `bus:time` carries the clock's TIME PRODUCT, never raw seconds:
        // `seconds`/`delta_seconds` stay produced-but-unbound for the card
        // face and probes. Two fallback producers on one channel would be an
        // `AmbiguousBusBinding`, so this is a replacement, not an addition.
        let fs = char_project(&[("clock", "{ \"kind\": \"Clock\" }")]);
        let rt = load_project(&fs);
        let clock = sibling(&rt, "clock");
        assert!(default_publishes(&rt, clock, "product", "time"));
        for unbound in ["seconds", "delta_seconds"] {
            assert!(
                !rt.tree().bindings().any(|binding| matches!(
                    &binding.source,
                    BindingSource::ProducedSlot { node, slot }
                        if *node == clock && slot == &SlotPath::parse(unbound).expect("slot")
                )),
                "{unbound} has no default channel"
            );
        }
    }

    #[test]
    fn char_authored_clock_target_suppresses_the_default() {
        let fs = char_project(&[(
            "clock",
            r#"{ "kind": "Clock", "bindings": { "product": { "target": "bus:custom" } } }"#,
        )]);
        let rt = load_project(&fs);
        let clock = sibling(&rt, "clock");
        assert!(!default_publishes(&rt, clock, "product", "time"));
        assert!(rt.tree().bindings().any(|binding| {
            binding.priority != BindingPriority::default_fallback()
                && matches!(
                    (&binding.source, &binding.target),
                    (
                        BindingSource::ProducedSlot { node, .. },
                        BindingTarget::BusChannel(c),
                    ) if *node == clock && c.0 == "custom"
                )
        }));
    }

    #[test]
    fn char_shader_gets_time_and_visual_out_defaults() {
        let fs = char_project(&[
            ("clock", "{ \"kind\": \"Clock\" }"),
            ("shader", CHAR_SHADER_WITH_TIME),
        ]);
        fs.write_file("/shader.glsl".as_path(), CHAR_SHADER_GLSL)
            .expect("shader.glsl");
        let rt = load_project(&fs);
        let shader = sibling(&rt, "shader");
        assert!(default_sources(&rt, shader, "time", "time"));
        assert!(default_publishes(&rt, shader, "output", "visual.out"));
    }

    #[test]
    fn char_shader_time_default_registers_even_without_a_clock() {
        // Shaders do NOT gate their time default on a clock existing (unlike
        // fluid, below). The channel simply has readers and no writer.
        let fs = char_project(&[("shader", CHAR_SHADER_WITH_TIME)]);
        fs.write_file("/shader.glsl".as_path(), CHAR_SHADER_GLSL)
            .expect("shader.glsl");
        let rt = load_project(&fs);
        let shader = sibling(&rt, "shader");
        assert!(default_sources(&rt, shader, "time", "time"));
    }

    #[test]
    fn char_authored_shader_time_suppresses_the_default() {
        let fs = char_project(&[
            ("clock", "{ \"kind\": \"Clock\" }"),
            (
                "shader",
                r#"
{
  "kind": "Shader",
  "source": { "path": "shader.glsl" },
  "render_order": 0,
  "consumed": {
    "time": { "kind": "value", "value": "f32", "default": 0.0 }
  },
  "bindings": {
    "time": { "source": "bus:custom" }
  }
}
"#,
            ),
        ]);
        fs.write_file("/shader.glsl".as_path(), CHAR_SHADER_GLSL)
            .expect("shader.glsl");
        let rt = load_project(&fs);
        let shader = sibling(&rt, "shader");
        assert!(!default_sources(&rt, shader, "time", "time"));
    }

    /// The authored `"time": { "source": "bus:time" }` the five checked-in
    /// `playlist.json` files carry keeps registering after the slot is
    /// retyped from `f32` to a time product: registration is driven by the
    /// DECLARED SLOT (the 2026-08-02 silent-drop fix), and the channel name
    /// did not change. Only those files' authored `"time": 0` *value* is
    /// stale — a product is not a number — which is P5's sweep.
    #[test]
    fn char_authored_playlist_time_binding_survives_the_product_retype() {
        let fs = char_project(&[(
            "playlist",
            r#"{ "kind": "Playlist",
                 "bindings": { "time": { "source": "bus:time" } },
                 "idle_entry": 1 }"#,
        )]);
        let rt = load_project(&fs);
        let playlist = sibling(&rt, "playlist");
        assert!(rt.tree().bindings().any(|binding| matches!(
            (&binding.source, &binding.target),
            (BindingSource::BusChannel(channel), BindingTarget::ConsumedSlot { node, slot })
                if channel.0 == "time"
                    && *node == playlist
                    && slot == &SlotPath::parse("time").expect("path")
        )));
    }

    /// Fix for
    /// `docs/defects/2026-08-04-compute-shader-default-bind-ignored.md`: the
    /// loader registered slot-declared `default_bind` for render shaders
    /// only, so `starter_compute_shader_def`'s own default-bound `time` slot
    /// came up unwired and every compute author had to restate it under
    /// `bindings`.
    #[test]
    fn char_compute_shader_slot_default_bind_registers() {
        let fs = char_project(&[
            ("clock", "{ \"kind\": \"Clock\" }"),
            ("compute", CHAR_COMPUTE_WITH_TIME),
        ]);
        fs.write_file("/compute.glsl".as_path(), CHAR_COMPUTE_GLSL)
            .expect("compute.glsl");
        let rt = load_project(&fs);
        let compute = sibling(&rt, "compute");
        assert!(default_sources(&rt, compute, "time", "time"));
    }

    /// The suppression rule is the shader arm's: an authored source on the
    /// same slot outranks the slot-declared default rather than doubling it.
    #[test]
    fn char_authored_compute_time_suppresses_the_default() {
        let fs = char_project(&[
            ("clock", "{ \"kind\": \"Clock\" }"),
            (
                "compute",
                r#"
{
  "kind": "ComputeShader",
  "source": { "path": "compute.glsl" },
  "consumed": {
    "time": { "kind": "value", "value": "f32", "default": 0.0,
              "default_bind": "bus:time" }
  },
  "bindings": {
    "time": { "source": "bus:custom" }
  }
}
"#,
            ),
        ]);
        fs.write_file("/compute.glsl".as_path(), CHAR_COMPUTE_GLSL)
            .expect("compute.glsl");
        let rt = load_project(&fs);
        let compute = sibling(&rt, "compute");
        assert!(!default_sources(&rt, compute, "time", "time"));
        assert!(
            rt.tree().bindings().any(|binding| {
                binding.priority != BindingPriority::default_fallback()
                    && matches!(
                        (&binding.source, &binding.target),
                        (
                            BindingSource::BusChannel(channel),
                            BindingTarget::ConsumedSlot { node, slot },
                        ) if channel.0 == "custom"
                            && *node == compute
                            && slot == &SlotPath::parse("time").expect("slot")
                    )
            }),
            "the authored source binding is what wires the slot instead"
        );
    }

    #[test]
    fn char_fluid_time_default_registers_unconditionally() {
        // Pre-ADR, fluid's time default was gated on a clock providing
        // bus:time (`has_default_time_bus`). The declarative swap removed
        // the gate: defaults register unconditionally and an unfilled
        // channel (readers, no writer) is surfaced on the bus, where the UI
        // can warn — fluid cannot work without time, and hiding the missing
        // wiring helped nobody.
        let fluid_json = r#"
{
  "kind": "Fluid",
  "size": { "width": 8, "height": 8 },
  "solver_iterations": 1,
  "step_hz": 25,
  "fade_speed": 0.08,
  "viscosity": 0.00003
}
"#;
        let with_clock =
            char_project(&[("clock", "{ \"kind\": \"Clock\" }"), ("fluid", fluid_json)]);
        let rt = load_project(&with_clock);
        let fluid = sibling(&rt, "fluid");
        assert!(default_sources(&rt, fluid, "time", "time"));

        let without_clock = char_project(&[("fluid", fluid_json)]);
        let rt = load_project(&without_clock);
        let fluid = sibling(&rt, "fluid");
        assert!(
            default_sources(&rt, fluid, "time", "time"),
            "fluid time default registers even without a clock (unfilled channel)"
        );
    }

    #[test]
    fn char_playlist_entry_children_do_not_compete_for_visual_out() {
        let fs = examples_fyeah_sign_fs();
        let fs: &dyn LpFs = &fs;
        let services = EngineServices::new(TreePath::parse("/fyeah.show").expect("path"));
        let rt = ProjectLoader::load_from_root(fs, services).expect("load fyeah sign");
        let default_publishers = rt
            .tree()
            .bindings()
            .filter(|binding| {
                binding.priority == BindingPriority::default_fallback()
                    && matches!(
                        (&binding.source, &binding.target),
                        (
                            BindingSource::ProducedSlot { .. },
                            BindingTarget::BusChannel(c),
                        ) if c.0 == "visual.out"
                    )
            })
            .count();
        assert_eq!(
            default_publishers, 3,
            "the playlist plus each entry child default-publishes visual.out \
             (entries into their own sink scopes)"
        );
    }

    /// Regression guard: a new `NodeKind` variant must not silently arrive
    /// ungated (mirrors the ISA-backend exhaustiveness guard from M3a).
    ///
    /// `classify` matches every `NodeKind` with **no wildcard arm** — adding
    /// a variant to `lpc_model::NodeKind` without extending this match is a
    /// *compile* error here, not a runtime failure someone has to notice.
    /// The per-kind string is the feature gate that must own an attach loop
    /// for that kind in `attach_projected_nodes_filtered` (or `"always-on"`
    /// for `Project`/`Output`, which `lpc-engine/Cargo.toml` documents as
    /// never gated).
    #[test]
    fn every_node_kind_is_explicitly_gated_or_always_on() {
        fn classify(kind: NodeKind) -> &'static str {
            match kind {
                NodeKind::Module => "always-on",
                NodeKind::Output => "always-on",
                NodeKind::Button => "node-button",
                NodeKind::Clock => "node-clock",
                NodeKind::Texture => "node-texture",
                NodeKind::Shader => "node-shader",
                NodeKind::ComputeShader => "node-shader",
                NodeKind::Fluid => "node-fluid",
                NodeKind::Playlist => "node-playlist",
                NodeKind::ControlRadio => "node-radio",
                NodeKind::Fixture => "node-fixture",
            }
        }
        for kind in [
            NodeKind::Module,
            NodeKind::Output,
            NodeKind::Button,
            NodeKind::Clock,
            NodeKind::Texture,
            NodeKind::Shader,
            NodeKind::ComputeShader,
            NodeKind::Fluid,
            NodeKind::Playlist,
            NodeKind::ControlRadio,
            NodeKind::Fixture,
        ] {
            assert!(!classify(kind).is_empty());
        }
    }

    /// A project referencing a gated-off node kind must still **load**. The
    /// missing-node contract is deliberately minimal (M2 scope decision,
    /// `docs/debt/firmware-capability-reporting.md`): the attach loop for a
    /// disabled kind falls back to `CorePlaceholderNode` and the load
    /// succeeds silently. This asserts only that — never anything about
    /// status/reporting, which is deliberately absent by design.
    ///
    /// Gated to `node-button` off, so it only compiles when that feature is
    /// disabled; under the crate's own `default` (all eight node gates on)
    /// this cfg compiles the test out entirely, same as the disabled-path
    /// arm it exercises in `attach_projected_nodes_filtered` above. It does
    /// **not** run under `just test` — nothing there tests lpc-engine with a
    /// non-default feature set. Its home is `just check-lpc-engine-gates`
    /// (part of `check-lint`, so CI's Lint job runs it), whose final step
    /// invokes it with the gate off, mirroring the P4 compile matrix but for
    /// `test` instead of `check`:
    ///
    /// ```sh
    /// cargo test -p lpc-engine --no-default-features --features \
    ///   "std,node-radio,node-fluid,node-fixture,node-texture,node-playlist,node-clock,node-shader" \
    ///   disabled_node_kind_still_loads_project
    /// ```
    #[test]
    #[cfg(not(feature = "node-button"))]
    fn disabled_node_kind_still_loads_project() {
        let fs = LpFsMemory::new();
        fs.write_file("/project.json".as_path(), b"{\n  \"format\": 8\n}\n")
            .expect("container manifest");
        fs.write_file(
            "/module.json".as_path(),
            br#"
{
  "kind": "Module",
  "nodes": {
    "button": {
      "ref": "./button.json"
    }
  }
}
"#,
        )
        .expect("project.json");
        fs.write_file(
            "/button.json".as_path(),
            br#"
{
  "kind": "Button",
  "endpoint": "button:local:D9",
  "stable_ms": 1,
  "bindings": {
    "down": {
      "target": "bus:trigger"
    }
  }
}
"#,
        )
        .expect("button.json");

        let services = EngineServices::new(TreePath::parse("/disabled_node.show").expect("path"));
        let result = ProjectLoader::load_from_root(&fs, services);
        assert!(
            result.is_ok(),
            "a project referencing a disabled node kind must still load: {:?}",
            result.err()
        );

        // ...and it does not masquerade as a healthy quiet node: the
        // placeholder standing in for the gated-out kind reports the build
        // gap by name, so the studio can say "not on this device" instead
        // of inventing an authoring error.
        let engine = result.expect("loads");
        let statuses: alloc::vec::Vec<lpc_model::NodeRuntimeStatus> = engine
            .tree()
            .entries()
            .map(|entry| entry.status.get().clone())
            .collect();
        let unsupported = statuses
            .iter()
            .find(|status| matches!(status, lpc_model::NodeRuntimeStatus::Unsupported(_)))
            .unwrap_or_else(|| panic!("expected an Unsupported entry, got {statuses:?}"));
        let lpc_model::NodeRuntimeStatus::Unsupported(message) = unsupported else {
            unreachable!()
        };
        assert!(
            message.contains("Button"),
            "the status must name the missing kind: {message}"
        );
    }
}
