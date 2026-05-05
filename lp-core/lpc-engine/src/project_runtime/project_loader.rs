//! Load authored `project.toml` node-artifact trees into [`super::CoreProjectRuntime`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::lp_path::{LpPath, LpPathBuf};
use lpc_model::prop::value_path::parse_path;
use lpc_model::{FrameId, Kind, ModelValue, NodeId, NodeName};
use lpc_source::ArtifactReadRoot;
use lpc_source::node::node_def::NodeDef;
use lpc_source::node::{
    NodeKind, fixture::FixtureDef, output::OutputDef, shader::ShaderDef, texture::TextureDef,
};
use lpc_source::{ArtifactLocator, NodeInvocation, ProjectDef, SrcValueSpec};
use lpc_wire::{WireChildKind, WireSlotIndex};

use crate::artifact::ArtifactLocation;
use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::engine::Engine;
use crate::nodes::{CorePlaceholderNode, FixtureNode, OutputNode, ShaderNode, TextureNode};
use crate::runtime_buffer::RuntimeBufferId;
use crate::tree::TreeError;

use super::{CoreProjectRuntime, RuntimeServices};

/// Errors loading an authored project into [`CoreProjectRuntime`].
#[derive(Debug)]
pub enum CoreProjectLoadError {
    Io { path: String, details: String },
    ProjectToml { file: String, error: String },
    UnknownKind { path: String, suffix: String },
    InvalidSourcePath { path: String, reason: String },
    TomlParse { path: String, error: String },
    InvalidNodeName { path: String, reason: String },
    Tree(TreeError),
}

impl core::fmt::Display for CoreProjectLoadError {
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

impl core::error::Error for CoreProjectLoadError {}

struct LoadedNode {
    name: NodeName,
    artifact_path: LpPathBuf,
    id: NodeId,
    config: LoadedNodeConfig,
}

#[derive(Clone)]
pub(super) enum LoadedNodeConfig {
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}

impl LoadedNodeConfig {
    pub(super) fn clone_as_node_config_box(&self) -> Box<dyn NodeDef> {
        match self {
            LoadedNodeConfig::Texture(c) => Box::new(c.clone()),
            LoadedNodeConfig::Shader(c) => Box::new(c.clone()),
            LoadedNodeConfig::Output(c) => Box::new(c.clone()),
            LoadedNodeConfig::Fixture(c) => Box::new(c.clone()),
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            LoadedNodeConfig::Texture(_) => "texture",
            LoadedNodeConfig::Shader(_) => "shader",
            LoadedNodeConfig::Output(_) => "output",
            LoadedNodeConfig::Fixture(_) => "fixture",
        }
    }
}

/// Loads the authored project artifact tree into a core engine-backed runtime.
pub struct CoreProjectLoader;

impl CoreProjectLoader {
    pub fn load_from_root<R>(
        root: &R,
        services: RuntimeServices,
    ) -> Result<CoreProjectRuntime, CoreProjectLoadError>
    where
        R: ArtifactReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        Self::load_project_artifact(root, services, ArtifactLocator::path("/project.toml"))
    }

    pub fn load_project_artifact<R>(
        root: &R,
        services: RuntimeServices,
        project_locator: ArtifactLocator,
    ) -> Result<CoreProjectRuntime, CoreProjectLoadError>
    where
        R: ArtifactReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        let project_path = resolve_project_locator(&project_locator)?;
        let project_def = load_project_def(root, &project_path)?;

        let project_root = services.project_root().clone();
        let mut runtime = CoreProjectRuntime::new(project_root.clone(), services);
        let frame = FrameId::new(1);
        let root_id = runtime.engine().tree().root();
        let project_artifact = runtime
            .engine_mut()
            .artifacts_mut()
            .acquire_location(ArtifactLocation::file(project_path.clone()), frame);
        let project_invocation = NodeInvocation::new(project_locator);

        {
            let root_entry = runtime
                .engine_mut()
                .tree_mut()
                .get_mut(root_id)
                .ok_or(CoreProjectLoadError::Tree(TreeError::UnknownNode(root_id)))?;
            root_entry.config = project_invocation;
            root_entry.artifact = project_artifact;
        }
        runtime
            .engine_mut()
            .attach_runtime_node(
                root_id,
                Box::new(CorePlaceholderNode::new_leaf(NodeKind::Project)),
                frame,
            )
            .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                path: project_path.as_str().to_string(),
                reason: format!("attach project runtime: {e}"),
            })?;

        let mut loaded_nodes = Vec::new();
        for (name, invocation) in project_def.nodes {
            let artifact_path =
                resolve_child_artifact_locator(&project_path, &invocation.artifact)?;
            let config = load_node_def(root, artifact_path.as_path())?;
            let artifact_id = runtime
                .engine_mut()
                .artifacts_mut()
                .acquire_location(ArtifactLocation::file(artifact_path.clone()), frame);
            let ty = node_kind_name(&config, artifact_path.as_path())?;
            let leaf_id = runtime
                .engine_mut()
                .tree_mut()
                .add_child(
                    root_id,
                    name.clone(),
                    ty,
                    WireChildKind::Input {
                        source: WireSlotIndex(0),
                    },
                    invocation,
                    artifact_id,
                    frame,
                )
                .map_err(CoreProjectLoadError::Tree)?;

            runtime.insert_artifact_node(artifact_path.clone(), leaf_id);
            loaded_nodes.push(LoadedNode {
                name,
                artifact_path,
                id: leaf_id,
                config,
            });
        }

        Self::attach_loaded_nodes(root, &mut runtime, &loaded_nodes, frame)?;

        for node in &loaded_nodes {
            runtime.compatibility_mut().record_authoring_snapshot(
                node.id,
                node.artifact_path.clone(),
                node.config.clone(),
            );
        }

        Ok(runtime)
    }

    fn attach_loaded_nodes<R>(
        root: &R,
        runtime: &mut CoreProjectRuntime,
        loaded_nodes: &[LoadedNode],
        frame: FrameId,
    ) -> Result<(), CoreProjectLoadError>
    where
        R: ArtifactReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        for node in loaded_nodes {
            if let LoadedNodeConfig::Texture(config) = &node.config {
                runtime
                    .engine_mut()
                    .attach_runtime_node(
                        node.id,
                        Box::new(TextureNode::new(node.id, config.clone())),
                        frame,
                    )
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach texture runtime: {e}"),
                    })?;
            }
        }

        for node in loaded_nodes {
            if let LoadedNodeConfig::Output(config) = &node.config {
                runtime
                    .engine_mut()
                    .attach_runtime_node(node.id, Box::new(OutputNode::new()), frame)
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach output runtime: {e}"),
                    })?;
                let sink_id = runtime
                    .engine()
                    .runtime_output_sink_buffer_id(node.id)
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: String::from("output runtime node produced no sink buffer"),
                    })?;
                runtime.services_mut().register_output_sink(sink_id, config);
            }
        }

        for node in loaded_nodes {
            if let LoadedNodeConfig::Shader(config) = &node.config {
                let texture_node =
                    resolve_node_loc(loaded_nodes, node, &config.texture_loc, "texture")?;
                let shader_path =
                    resolve_path_relative_to_file(&node.artifact_path, &config.glsl_path)?;
                let glsl_source = read_utf8_file(root, shader_path.as_path())?;
                let placeholder_dims = placeholder_texture_dimensions_for_shader(texture_node)?;
                runtime
                    .engine_mut()
                    .attach_runtime_node(
                        node.id,
                        Box::new(ShaderNode::new(
                            node.id,
                            texture_node.id,
                            config.clone(),
                            glsl_source,
                            placeholder_dims.0,
                            placeholder_dims.1,
                        )),
                        frame,
                    )
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach shader runtime: {e}"),
                    })?;
            }
        }

        for node in loaded_nodes {
            if let LoadedNodeConfig::Fixture(config) = &node.config {
                let texture_node =
                    resolve_node_loc(loaded_nodes, node, &config.texture_loc, "texture")?;
                let shader_node = find_shader_for_texture(loaded_nodes, texture_node.id)
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!(
                            "no shader targets texture node ref `{}`",
                            config.texture_loc
                        ),
                    })?;
                let output_node =
                    resolve_node_loc(loaded_nodes, node, &config.output_loc, "output")?;
                let sink_id = output_sink_for(
                    runtime.engine(),
                    output_node.id,
                    output_node.artifact_path.as_path(),
                )?;

                runtime
                    .engine_mut()
                    .attach_runtime_node(
                        node.id,
                        Box::new(FixtureNode::new(
                            node.id,
                            texture_node.id,
                            shader_node.id,
                            config.mapping.clone(),
                            frame,
                            sink_id,
                            config.color_order,
                            config.brightness.unwrap_or(64),
                            config.gamma_correction.unwrap_or(true),
                        )),
                        frame,
                    )
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("attach fixture runtime: {e}"),
                    })?;
                runtime
                    .engine_mut()
                    .bindings_mut()
                    .register(
                        BindingDraft {
                            source: BindingSource::Literal(SrcValueSpec::Literal(ModelValue::F32(
                                0.0,
                            ))),
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
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.artifact_path.as_str().to_string(),
                        reason: format!("bind fixture demand slot: {e}"),
                    })?;
                runtime.engine_mut().add_demand_root(node.id);
            }
        }

        Ok(())
    }
}

fn load_project_def<R>(root: &R, path: &LpPathBuf) -> Result<ProjectDef, CoreProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let text = read_utf8_file(root, path.as_path())?;
    let def: ProjectDef = toml::from_str(&text).map_err(|e| CoreProjectLoadError::ProjectToml {
        file: path.as_str().to_string(),
        error: format!("{e}"),
    })?;
    if !def.is_project_kind() {
        return Err(CoreProjectLoadError::UnknownKind {
            path: path.as_str().to_string(),
            suffix: def.kind,
        });
    }
    Ok(def)
}

fn load_node_def<R>(root: &R, path: &LpPath) -> Result<LoadedNodeConfig, CoreProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let text = read_utf8_file(root, path)?;
    let probe: NodeKindProbe =
        toml::from_str(&text).map_err(|e| CoreProjectLoadError::TomlParse {
            path: path.as_str().to_string(),
            error: format!("{e}"),
        })?;

    match probe.kind.as_str() {
        "texture" => parse_node_def(path, &text).map(LoadedNodeConfig::Texture),
        "shader" => parse_node_def(path, &text).map(LoadedNodeConfig::Shader),
        "output" => parse_node_def(path, &text).map(LoadedNodeConfig::Output),
        "fixture" => parse_node_def(path, &text).map(LoadedNodeConfig::Fixture),
        other => Err(CoreProjectLoadError::UnknownKind {
            path: path.as_str().to_string(),
            suffix: other.to_string(),
        }),
    }
}

fn parse_node_def<T>(path: &LpPath, text: &str) -> Result<T, CoreProjectLoadError>
where
    T: serde::de::DeserializeOwned,
{
    toml::from_str(text).map_err(|e| CoreProjectLoadError::TomlParse {
        path: path.as_str().to_string(),
        error: format!("{e}"),
    })
}

#[derive(serde::Deserialize)]
struct NodeKindProbe {
    kind: String,
}

fn resolve_project_locator(locator: &ArtifactLocator) -> Result<LpPathBuf, CoreProjectLoadError> {
    resolve_path_locator_from_dir(LpPath::new("/"), locator)
}

fn resolve_child_artifact_locator(
    containing_file: &LpPathBuf,
    locator: &ArtifactLocator,
) -> Result<LpPathBuf, CoreProjectLoadError> {
    let parent = containing_file
        .as_path()
        .parent()
        .unwrap_or(LpPath::new("/"));
    resolve_path_locator_from_dir(parent, locator)
}

fn resolve_path_locator_from_dir(
    base_dir: &LpPath,
    locator: &ArtifactLocator,
) -> Result<LpPathBuf, CoreProjectLoadError> {
    match locator {
        ArtifactLocator::Path(path) => {
            if path.is_absolute() {
                Ok(path.clone())
            } else {
                base_dir
                    .to_path_buf()
                    .join_relative(path.as_str())
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: path.as_str().to_string(),
                        reason: format!("path cannot be resolved relative to {base_dir:?}"),
                    })
            }
        }
        ArtifactLocator::Lib(lib) => Err(CoreProjectLoadError::InvalidSourcePath {
            path: lib.to_string(),
            reason: String::from("library artifact locators are not supported for nodes yet"),
        }),
    }
}

fn resolve_path_relative_to_file(
    containing_file: &LpPathBuf,
    path: &LpPathBuf,
) -> Result<LpPathBuf, CoreProjectLoadError> {
    let parent = containing_file
        .as_path()
        .parent()
        .unwrap_or(LpPath::new("/"));
    parent
        .to_path_buf()
        .join_relative(path.as_str())
        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
            path: path.as_str().to_string(),
            reason: format!(
                "path cannot be resolved relative to {}",
                containing_file.as_str()
            ),
        })
}

fn node_kind_name(
    config: &LoadedNodeConfig,
    path: &LpPath,
) -> Result<NodeName, CoreProjectLoadError> {
    NodeName::parse(config.kind_name()).map_err(|e| CoreProjectLoadError::InvalidNodeName {
        path: path.as_str().to_string(),
        reason: format!("{e}"),
    })
}

fn find_node_by_loc<'a>(
    loaded_nodes: &'a [LoadedNode],
    current: &'a LoadedNode,
    loc: &lpc_model::RelativeNodeRef,
) -> Option<&'a LoadedNode> {
    resolve_relative_node_ref(loaded_nodes, current, loc)
}

fn resolve_node_loc<'a>(
    loaded_nodes: &'a [LoadedNode],
    current: &'a LoadedNode,
    loc: &lpc_model::RelativeNodeRef,
    expected: &str,
) -> Result<&'a LoadedNode, CoreProjectLoadError> {
    resolve_relative_node_ref(loaded_nodes, current, loc).ok_or_else(|| {
        CoreProjectLoadError::InvalidSourcePath {
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

fn demand_input_path() -> lpc_model::ValuePath {
    parse_path("in").expect("valid demand input path")
}

fn find_shader_for_texture<'a>(
    loaded_nodes: &'a [LoadedNode],
    texture_id: NodeId,
) -> Option<&'a LoadedNode> {
    loaded_nodes
        .iter()
        .filter(|node| {
            let LoadedNodeConfig::Shader(config) = &node.config else {
                return false;
            };
            find_node_by_loc(loaded_nodes, node, &config.texture_loc)
                .map(|candidate| candidate.id == texture_id)
                .unwrap_or(false)
        })
        .max_by(|a, b| {
            let ar = match &a.config {
                LoadedNodeConfig::Shader(config) => config.render_order,
                _ => 0,
            };
            let br = match &b.config {
                LoadedNodeConfig::Shader(config) => config.render_order,
                _ => 0,
            };
            ar.cmp(&br)
                .then_with(|| a.artifact_path.as_str().cmp(b.artifact_path.as_str()))
        })
}

fn placeholder_texture_dimensions_for_shader(
    texture_node: &LoadedNode,
) -> Result<(u32, u32), CoreProjectLoadError> {
    let LoadedNodeConfig::Texture(config) = &texture_node.config else {
        return Err(CoreProjectLoadError::InvalidSourcePath {
            path: texture_node.artifact_path.as_str().to_string(),
            reason: String::from("shader texture loc did not reference a texture node"),
        });
    };
    Ok((config.width, config.height))
}

fn output_sink_for(
    engine: &Engine,
    output_node_id: NodeId,
    output_dir: &LpPath,
) -> Result<RuntimeBufferId, CoreProjectLoadError> {
    engine
        .runtime_output_sink_buffer_id(output_node_id)
        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
            path: output_dir.as_str().to_string(),
            reason: String::from("output node has no sink buffer"),
        })
}

fn read_utf8_file<R>(root: &R, path: &LpPath) -> Result<String, CoreProjectLoadError>
where
    R: ArtifactReadRoot + ?Sized,
    R::Err: core::fmt::Debug,
{
    let data = root.read_file(path).map_err(|e| CoreProjectLoadError::Io {
        path: path.as_str().to_string(),
        details: format!("{e:?}"),
    })?;
    String::from_utf8(data).map_err(|e| CoreProjectLoadError::InvalidSourcePath {
        path: path.as_str().to_string(),
        reason: format!("shader source is not UTF-8: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use lpc_model::NodeName;
    use lpc_model::TreePath;
    use lpc_model::lp_path::AsLpPath;
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
        let services = RuntimeServices::new(root_path.clone());
        let rt = CoreProjectLoader::load_from_root(&fs, services).expect("load");
        let root = rt.engine().tree().root();

        let tex_id = rt
            .engine()
            .tree()
            .lookup_sibling(root, NodeName::parse("texture").unwrap())
            .expect("texture id");
        let sh_id = rt
            .engine()
            .tree()
            .lookup_sibling(root, NodeName::parse("shader").unwrap())
            .expect("shader id");
        let out_id = rt
            .engine()
            .tree()
            .lookup_sibling(root, NodeName::parse("output").unwrap())
            .expect("output id");
        let fix_id = rt
            .engine()
            .tree()
            .lookup_sibling(root, NodeName::parse("fixture").unwrap())
            .expect("fixture id");

        assert_eq!(
            rt.artifact_node_id(LpPath::new("/texture.toml")),
            Some(tex_id)
        );

        for id in [tex_id, sh_id, out_id, fix_id] {
            let entry = rt.engine().tree().get(id).expect("entry");
            assert!(entry.state.is_alive(), "node {id:?} should be alive",);
        }

        let root_entry = rt.engine().tree().get(root).expect("root entry");
        assert!(root_entry.state.is_alive(), "project root should be alive");
        assert_eq!(
            rt.engine()
                .tree()
                .get(fix_id)
                .and_then(|entry| entry.path.0.last())
                .map(|s| s.ty.to_string())
                .as_deref(),
            Some("fixture")
        );

        assert!(
            rt.engine().demand_roots().contains(&fix_id),
            "fixture must be demand root"
        );
        assert!(
            !rt.engine().demand_roots().contains(&tex_id),
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
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, CoreProjectLoadError::TomlParse { .. }),
            "expected TomlParse, got {err:?}"
        );
    }

    #[test]
    fn missing_project_toml_returns_io_error() {
        let fs = LpFsMemory::new();
        let root_path = TreePath::parse("/p.show").expect("path");
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, CoreProjectLoadError::Io { .. }),
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
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, CoreProjectLoadError::UnknownKind { .. }),
            "expected UnknownKind, got {err:?}"
        );
    }

    #[test]
    fn missing_sibling_node_loc_names_missing_ref() {
        let fs = flat_project();
        fs.write_file(
            "/shader.toml".as_path(),
            br#"
kind = "shader"
glsl_path = "shader.glsl"
texture_loc = "..missing"
render_order = 0
"#,
        )
        .expect("shader.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(
                err,
                CoreProjectLoadError::InvalidSourcePath { ref reason, .. }
                    if reason.contains("unknown texture node ref `..missing`")
            ),
            "expected missing texture ref, got {err:?}"
        );
    }

    #[test]
    fn slash_node_ref_is_rejected_during_parse() {
        let fs = flat_project();
        fs.write_file(
            "/shader.toml".as_path(),
            br#"
kind = "shader"
glsl_path = "shader.glsl"
texture_loc = "/texture"
render_order = 0
"#,
        )
        .expect("shader.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&fs, services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(
                err,
                CoreProjectLoadError::TomlParse { ref error, .. }
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
width = 16
height = 16
"#,
        )
        .expect("texture.toml");
        fs.write_file(
            "/shader.toml".as_path(),
            br#"
kind = "shader"
glsl_path = "shader.glsl"
texture_loc = "..texture"
render_order = 0
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
"#,
        )
        .expect("output.toml");
        fs.write_file(
            "/fixture.toml".as_path(),
            br#"
kind = "fixture"
output_loc = "..output"
texture_loc = "..texture"
color_order = "Rgb"
transform = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
]
brightness = 255
gamma_correction = false

[mapping.PathPoints]
sample_diameter = 2.0

[[mapping.PathPoints.paths]]

[mapping.PathPoints.paths.RingArray]
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 1
ring_lamp_counts = [1]
offset_angle = 0.0
order = "InnerFirst"
"#,
        )
        .expect("fixture.toml");
    }
}
