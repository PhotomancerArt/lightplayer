//! Load authored `/project.json` + `/src/*.kind` layout into [`super::CoreProjectRuntime`].

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::lp_path::{LpPath, LpPathBuf};
use lpc_model::prop::prop_path::parse_path;
use lpc_model::{
    FrameId, Kind, ModelValue, NodeId, NodeName, NodeNameError, NodePathSegment, ProjectConfig,
    TreePath, Versioned,
};
use lpc_source::legacy::nodes::{
    NodeConfig, NodeKind, fixture::FixtureConfig, output::OutputConfig, shader::ShaderConfig,
    texture::TextureConfig,
};
use lpc_source::legacy::{
    LegacyNodeLoadError, LegacyNodeReadRoot, discover_legacy_node_dirs, load_legacy_node_config,
};
use lpc_source::{SrcArtifactSpec, SrcNodeConfig, SrcValueSpec};
use lpc_wire::{WireChildKind, WireSlotIndex};

use crate::artifact::ArtifactId;
use crate::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::engine::Engine;
use crate::nodes::{CorePlaceholderNode, FixtureNode, OutputNode, ShaderNode, TextureNode};
use crate::render_product::TextureRenderProduct;
use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferId};
use crate::tree::TreeError;

use super::{CoreProjectRuntime, RuntimeServices};

/// Errors loading an authored project into [`CoreProjectRuntime`].
#[derive(Debug)]
pub enum CoreProjectLoadError {
    Io { path: String, details: String },
    ProjectJson { file: String, error: String },
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
            Self::ProjectJson { file, error } => write!(f, "parse {file}: {error}"),
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

fn map_legacy_err<E: core::fmt::Debug>(err: LegacyNodeLoadError<E>) -> CoreProjectLoadError {
    match err {
        LegacyNodeLoadError::Io { path, error } => CoreProjectLoadError::Io {
            path: path.as_str().to_string(),
            details: format!("{error:?}"),
        },
        LegacyNodeLoadError::InvalidPath { path, reason } => {
            CoreProjectLoadError::InvalidSourcePath {
                path: path.as_str().to_string(),
                reason: reason.to_string(),
            }
        }
        LegacyNodeLoadError::UnknownKind { path, suffix } => CoreProjectLoadError::UnknownKind {
            path: path.as_str().to_string(),
            suffix,
        },
        LegacyNodeLoadError::Parse { path, error } => CoreProjectLoadError::TomlParse {
            path: path.as_str().to_string(),
            error: format!("{error}"),
        },
    }
}

struct LoadedNode {
    dir: LpPathBuf,
    id: NodeId,
    config: LoadedNodeConfig,
}

enum LoadedNodeConfig {
    Texture(TextureConfig),
    Shader(ShaderConfig),
    Output(OutputConfig),
    Fixture(FixtureConfig),
}

impl LoadedNodeConfig {
    fn from_boxed(path: &LpPath, cfg: &dyn NodeConfig) -> Result<Self, CoreProjectLoadError> {
        match cfg.kind() {
            NodeKind::Texture => cfg
                .as_any()
                .downcast_ref::<TextureConfig>()
                .cloned()
                .map(Self::Texture),
            NodeKind::Shader => cfg
                .as_any()
                .downcast_ref::<ShaderConfig>()
                .cloned()
                .map(Self::Shader),
            NodeKind::Output => cfg
                .as_any()
                .downcast_ref::<OutputConfig>()
                .cloned()
                .map(Self::Output),
            NodeKind::Fixture => cfg
                .as_any()
                .downcast_ref::<FixtureConfig>()
                .cloned()
                .map(Self::Fixture),
        }
        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
            path: path.as_str().to_string(),
            reason: String::from("node config kind did not match concrete config type"),
        })
    }
}

/// Loads the current authored legacy layout into a core engine-backed runtime.
pub struct CoreProjectLoader;

impl CoreProjectLoader {
    /// Tree path for a legacy node directory under [`RuntimeServices::project_root`].
    pub fn tree_path_for_legacy_src_dir(
        project_root: &TreePath,
        node_dir: &LpPathBuf,
    ) -> Result<TreePath, CoreProjectLoadError> {
        let rel = Self::src_dir_relative_parts(node_dir.as_path())?;
        let segments = Self::segments_from_src_parts(node_dir.as_str(), &rel)?;
        Ok(Self::extend_path(project_root, &segments))
    }

    pub fn load_from_root<R>(
        root: &R,
        services: RuntimeServices,
    ) -> Result<CoreProjectRuntime, CoreProjectLoadError>
    where
        R: LegacyNodeReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        let project_path = "/project.json";
        let data =
            root.read_file(LpPath::new(project_path))
                .map_err(|e| CoreProjectLoadError::Io {
                    path: project_path.to_string(),
                    details: format!("{e:?}"),
                })?;

        let _config: ProjectConfig =
            lpc_wire::json::from_slice(&data).map_err(|e| CoreProjectLoadError::ProjectJson {
                file: project_path.to_string(),
                error: format!("{e}"),
            })?;

        let mut node_dirs =
            discover_legacy_node_dirs(root, LpPath::new("/src")).map_err(map_legacy_err)?;

        node_dirs.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        let project_root = services.project_root().clone();
        let mut runtime = CoreProjectRuntime::new(project_root.clone(), services);
        let frame = FrameId::new(1);
        let spine_cfg = SrcNodeConfig::new(SrcArtifactSpec::path("/"));
        let artifact = ArtifactId::from_raw(0);
        let root_id = runtime.engine().tree().root();
        let mut loaded_nodes = Vec::new();

        for dir in node_dirs {
            let (_path, cfg) =
                load_legacy_node_config(root, dir.as_path()).map_err(map_legacy_err)?;
            let config = LoadedNodeConfig::from_boxed(dir.as_path(), cfg.as_ref())?;

            let tree_path = Self::tree_path_for_legacy_src_dir(&project_root, &dir)?;
            let leaf_parent = Self::ensure_interior_path(
                runtime.engine_mut(),
                root_id,
                &project_root,
                &tree_path,
                frame,
                &spine_cfg,
                artifact,
            )?;

            let child_seg =
                tree_path
                    .0
                    .last()
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: dir.as_str().to_string(),
                        reason: String::from("empty tree path"),
                    })?;

            let leaf_ty = child_seg.ty.clone();
            let leaf_name = child_seg.name.clone();

            let leaf_id = runtime
                .engine_mut()
                .tree_mut()
                .add_child(
                    leaf_parent,
                    leaf_name,
                    leaf_ty,
                    WireChildKind::Input {
                        source: WireSlotIndex(0),
                    },
                    spine_cfg.clone(),
                    artifact,
                    frame,
                )
                .map_err(CoreProjectLoadError::Tree)?;

            runtime.insert_legacy_src_dir(dir.clone(), leaf_id);
            loaded_nodes.push(LoadedNode {
                dir,
                id: leaf_id,
                config,
            });
        }

        Self::attach_loaded_nodes(root, &mut runtime, &loaded_nodes, frame)?;

        Ok(runtime)
    }

    fn attach_loaded_nodes<R>(
        root: &R,
        runtime: &mut CoreProjectRuntime,
        loaded_nodes: &[LoadedNode],
        frame: FrameId,
    ) -> Result<(), CoreProjectLoadError>
    where
        R: LegacyNodeReadRoot + ?Sized,
        R::Err: core::fmt::Debug,
    {
        let mut output_sinks = Vec::new();

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
                        path: node.dir.as_str().to_string(),
                        reason: format!("attach texture runtime: {e}"),
                    })?;
            }
        }

        for node in loaded_nodes {
            if let LoadedNodeConfig::Output(config) = &node.config {
                let sink_id = runtime
                    .engine_mut()
                    .runtime_buffers_mut()
                    .insert(Versioned::new(
                        FrameId::default(),
                        RuntimeBuffer::raw(Vec::new()),
                    ));
                runtime.services_mut().register_output_sink(sink_id, config);
                output_sinks.push((node.id, sink_id));
                runtime
                    .engine_mut()
                    .attach_runtime_node(node.id, Box::new(OutputNode::new(sink_id)), frame)
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
                        reason: format!("attach output runtime: {e}"),
                    })?;
            }
        }

        for node in loaded_nodes {
            if let LoadedNodeConfig::Shader(config) = &node.config {
                let texture_node = find_node_by_spec(loaded_nodes, config.texture_spec.as_str())
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
                        reason: format!("unknown texture spec `{}`", config.texture_spec.as_str()),
                    })?;
                let shader_path = node.dir.join(config.glsl_path.as_str());
                let glsl_source = read_utf8_file(root, shader_path.as_path())?;
                let placeholder = empty_texture_product_for(texture_node, loaded_nodes)?;
                let product_id = runtime
                    .engine_mut()
                    .render_products_mut()
                    .insert(placeholder);

                runtime
                    .engine_mut()
                    .attach_runtime_node(
                        node.id,
                        Box::new(ShaderNode::new(
                            node.id,
                            texture_node.id,
                            config.clone(),
                            glsl_source,
                            product_id,
                        )),
                        frame,
                    )
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
                        reason: format!("attach shader runtime: {e}"),
                    })?;
            }
        }

        for node in loaded_nodes {
            if let LoadedNodeConfig::Fixture(config) = &node.config {
                let texture_node = find_node_by_spec(loaded_nodes, config.texture_spec.as_str())
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
                        reason: format!("unknown texture spec `{}`", config.texture_spec.as_str()),
                    })?;
                let shader_node =
                    find_shader_for_texture(loaded_nodes, config.texture_spec.as_str())
                        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                            path: node.dir.as_str().to_string(),
                            reason: format!(
                                "no shader targets texture `{}`",
                                config.texture_spec.as_str()
                            ),
                        })?;
                let output_node = find_node_by_spec(loaded_nodes, config.output_spec.as_str())
                    .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
                        reason: format!("unknown output spec `{}`", config.output_spec.as_str()),
                    })?;
                let sink_id = output_sink_for(&output_sinks, output_node.id, &output_node.dir)?;

                runtime
                    .engine_mut()
                    .attach_runtime_node(
                        node.id,
                        Box::new(FixtureNode::new(
                            node.id,
                            texture_node.id,
                            shader_node.id,
                            config.mapping.clone(),
                            sink_id,
                            config.color_order,
                            config.brightness.unwrap_or(64),
                            config.gamma_correction.unwrap_or(true),
                        )),
                        frame,
                    )
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
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
                            target: BindingTarget::NodeInput {
                                node: node.id,
                                input: demand_input_path(),
                            },
                            priority: BindingPriority::new(0),
                            kind: Kind::Color,
                            owner: node.id,
                        },
                        frame,
                    )
                    .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                        path: node.dir.as_str().to_string(),
                        reason: format!("bind fixture demand input: {e}"),
                    })?;
                runtime.engine_mut().add_demand_root(node.id);
            }
        }

        Ok(())
    }

    fn extend_path(root: &TreePath, segments: &[NodePathSegment]) -> TreePath {
        let mut v = root.0.clone();
        v.extend_from_slice(segments);
        TreePath(v)
    }

    fn src_dir_relative_parts(dir: &LpPath) -> Result<Vec<String>, CoreProjectLoadError> {
        let s = dir.as_str();
        let rest =
            s.strip_prefix("/src")
                .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
                    path: s.to_string(),
                    reason: String::from("expected path to start with /src"),
                })?;
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if rest.is_empty() {
            return Err(CoreProjectLoadError::InvalidSourcePath {
                path: s.to_string(),
                reason: String::from("missing path under /src"),
            });
        }
        Ok(rest
            .split('/')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect())
    }

    fn segments_from_src_parts(
        display_path: &str,
        parts: &[String],
    ) -> Result<Vec<NodePathSegment>, CoreProjectLoadError> {
        if parts.is_empty() {
            return Err(CoreProjectLoadError::InvalidSourcePath {
                path: display_path.to_string(),
                reason: String::from("no /src components"),
            });
        }
        let mut out = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            if last {
                let (base, kind) = part.rsplit_once('.').ok_or_else(|| {
                    CoreProjectLoadError::InvalidSourcePath {
                        path: display_path.to_string(),
                        reason: format!("final segment `{part}` has no .kind suffix"),
                    }
                })?;
                let name = Self::sanitize_component(display_path, base)?;
                let ty = NodeName::parse(kind).map_err(|e| Self::map_name_err(display_path, e))?;
                out.push(NodePathSegment { name, ty });
            } else if part.contains('.') {
                return Err(CoreProjectLoadError::InvalidSourcePath {
                    path: display_path.to_string(),
                    reason: format!("intermediate segment `{part}` must not contain '.'"),
                });
            } else {
                let name = Self::sanitize_component(display_path, part)?;
                let ty =
                    NodeName::parse("folder").map_err(|e| Self::map_name_err(display_path, e))?;
                out.push(NodePathSegment { name, ty });
            }
        }
        Ok(out)
    }

    fn sanitize_component(path: &str, raw: &str) -> Result<NodeName, CoreProjectLoadError> {
        let mut s = String::new();
        for c in raw.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => s.push(c),
                '-' => s.push('_'),
                _ => {
                    return Err(CoreProjectLoadError::InvalidNodeName {
                        path: path.to_string(),
                        reason: format!("unsupported character in `{raw}`"),
                    });
                }
            }
        }
        if s.is_empty() {
            return Err(CoreProjectLoadError::InvalidNodeName {
                path: path.to_string(),
                reason: format!("empty name after sanitizing `{raw}`"),
            });
        }
        NodeName::parse(&s).map_err(|e| Self::map_name_err(path, e))
    }

    fn map_name_err(path: &str, e: NodeNameError) -> CoreProjectLoadError {
        CoreProjectLoadError::InvalidNodeName {
            path: path.to_string(),
            reason: format!("{e}"),
        }
    }

    fn ensure_interior_path(
        engine: &mut Engine,
        tree_root: NodeId,
        project_root: &TreePath,
        full_path: &TreePath,
        frame: FrameId,
        spine_cfg: &SrcNodeConfig,
        artifact: ArtifactId,
    ) -> Result<NodeId, CoreProjectLoadError> {
        if full_path.0.len() <= project_root.0.len() {
            return Err(CoreProjectLoadError::InvalidSourcePath {
                path: full_path.to_string(),
                reason: String::from("tree path must extend project root"),
            });
        }
        if full_path.0.len() < project_root.0.len() + 1 {
            return Err(CoreProjectLoadError::InvalidSourcePath {
                path: full_path.to_string(),
                reason: String::from("expected at least one segment under project root"),
            });
        }
        for (i, seg) in project_root.0.iter().enumerate() {
            if full_path.0.get(i) != Some(seg) {
                return Err(CoreProjectLoadError::InvalidSourcePath {
                    path: full_path.to_string(),
                    reason: format!("path prefix must match project root {}", project_root),
                });
            }
        }

        let mut parent = tree_root;
        let interior_end = full_path.0.len() - 1;
        for seg in &full_path.0[project_root.0.len()..interior_end] {
            parent = Self::find_or_create_child(engine, parent, seg, frame, spine_cfg, artifact)?;
        }
        Ok(parent)
    }

    fn find_or_create_child(
        engine: &mut Engine,
        parent: NodeId,
        seg: &NodePathSegment,
        frame: FrameId,
        spine_cfg: &SrcNodeConfig,
        artifact: ArtifactId,
    ) -> Result<NodeId, CoreProjectLoadError> {
        let children = engine
            .tree()
            .get(parent)
            .ok_or(TreeError::UnknownNode(parent))
            .map_err(CoreProjectLoadError::Tree)?
            .children
            .clone();
        for cid in children {
            let entry = engine.tree().get(cid).expect("indexed child");
            if entry.path.0.last() == Some(seg) {
                return Ok(cid);
            }
        }

        let id = engine
            .tree_mut()
            .add_child(
                parent,
                seg.name.clone(),
                seg.ty.clone(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                spine_cfg.clone(),
                artifact,
                frame,
            )
            .map_err(CoreProjectLoadError::Tree)?;

        let folder = Box::new(CorePlaceholderNode::new_folder());
        engine.attach_runtime_node(id, folder, frame).map_err(|e| {
            CoreProjectLoadError::InvalidSourcePath {
                path: format!("{} (folder spine)", seg.name),
                reason: format!("attach runtime: {e}"),
            }
        })?;
        Ok(id)
    }
}

fn find_node_by_spec<'a>(loaded_nodes: &'a [LoadedNode], spec: &str) -> Option<&'a LoadedNode> {
    loaded_nodes.iter().find(|node| node.dir.as_str() == spec)
}

fn demand_input_path() -> lpc_model::PropPath {
    parse_path("in").expect("valid demand input path")
}

fn find_shader_for_texture<'a>(
    loaded_nodes: &'a [LoadedNode],
    texture_spec: &str,
) -> Option<&'a LoadedNode> {
    loaded_nodes
        .iter()
        .filter(|node| {
            matches!(
                &node.config,
                LoadedNodeConfig::Shader(config) if config.texture_spec.as_str() == texture_spec
            )
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
            ar.cmp(&br).then_with(|| a.dir.as_str().cmp(b.dir.as_str()))
        })
}

fn output_sink_for(
    output_sinks: &[(NodeId, RuntimeBufferId)],
    output_node_id: NodeId,
    output_dir: &LpPath,
) -> Result<RuntimeBufferId, CoreProjectLoadError> {
    output_sinks
        .iter()
        .find_map(|(node_id, sink_id)| (*node_id == output_node_id).then_some(*sink_id))
        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
            path: output_dir.as_str().to_string(),
            reason: String::from("output node has no registered sink"),
        })
}

fn empty_texture_product_for(
    texture_node: &LoadedNode,
    loaded_nodes: &[LoadedNode],
) -> Result<Box<dyn crate::render_product::RenderProduct>, CoreProjectLoadError> {
    let config = loaded_nodes
        .iter()
        .find(|node| node.id == texture_node.id)
        .and_then(|node| match &node.config {
            LoadedNodeConfig::Texture(config) => Some(config),
            _ => None,
        })
        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
            path: texture_node.dir.as_str().to_string(),
            reason: String::from("shader texture spec did not reference a texture node"),
        })?;
    let len = rgba16_byte_len(config.width, config.height)?;
    let product =
        TextureRenderProduct::rgba16_unorm(config.width, config.height, alloc::vec![0u8; len])
            .map_err(|e| CoreProjectLoadError::InvalidSourcePath {
                path: texture_node.dir.as_str().to_string(),
                reason: format!("create placeholder texture product: {e}"),
            })?;
    Ok(Box::new(product))
}

fn rgba16_byte_len(width: u32, height: u32) -> Result<usize, CoreProjectLoadError> {
    usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|px| px.checked_mul(8))
        .ok_or_else(|| CoreProjectLoadError::InvalidSourcePath {
            path: String::from("<texture>"),
            reason: format!("texture dimensions {width}x{height} overflow host usize"),
        })
}

fn read_utf8_file<R>(root: &R, path: &LpPath) -> Result<String, CoreProjectLoadError>
where
    R: LegacyNodeReadRoot + ?Sized,
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
    use alloc::rc::Rc;
    use core::cell::RefCell;

    use lpc_model::lp_path::AsLpPath;
    use lpc_shared::project::ProjectBuilder;
    use lpfs::{LpFs, LpFsMemory};

    use super::*;

    fn demo_project() -> Rc<RefCell<dyn lpfs::LpFs>> {
        let fs = Rc::new(RefCell::new(LpFsMemory::new()));
        let mut pb = ProjectBuilder::new(fs.clone());
        let tex = pb.texture_basic();
        pb.shader_basic(&tex);
        let out = pb.output_basic();
        let _fix = pb.fixture_basic(&out, &tex);
        pb.build();
        fs
    }

    #[test]
    fn project_builder_loads_into_runtime_with_expected_nodes() {
        let fs = demo_project();
        let root_path = TreePath::parse("/demo.show").expect("path");
        let services = RuntimeServices::new(root_path.clone());
        let fs_ref = fs.borrow();
        let rt = CoreProjectLoader::load_from_root(&*fs_ref, services).expect("load");

        let tex_path = CoreProjectLoader::tree_path_for_legacy_src_dir(
            &root_path,
            &LpPathBuf::from("/src/texture-1.texture"),
        )
        .expect("tree path texture");
        let sh_path = CoreProjectLoader::tree_path_for_legacy_src_dir(
            &root_path,
            &LpPathBuf::from("/src/shader-1.shader"),
        )
        .expect("tree path shader");
        let out_path = CoreProjectLoader::tree_path_for_legacy_src_dir(
            &root_path,
            &LpPathBuf::from("/src/output-1.output"),
        )
        .expect("tree path output");
        let fix_path = CoreProjectLoader::tree_path_for_legacy_src_dir(
            &root_path,
            &LpPathBuf::from("/src/fixture-1.fixture"),
        )
        .expect("tree path fixture");

        let tex_id = rt
            .engine()
            .tree()
            .lookup_path(&tex_path)
            .expect("texture id");
        let sh_id = rt.engine().tree().lookup_path(&sh_path).expect("shader id");
        let out_id = rt
            .engine()
            .tree()
            .lookup_path(&out_path)
            .expect("output id");
        let fix_id = rt
            .engine()
            .tree()
            .lookup_path(&fix_path)
            .expect("fixture id");

        assert_eq!(
            rt.legacy_src_node_id(LpPath::new("/src/texture-1.texture")),
            Some(tex_id)
        );

        for (id, path) in [
            (tex_id, &tex_path),
            (sh_id, &sh_path),
            (out_id, &out_path),
            (fix_id, &fix_path),
        ] {
            let entry = rt.engine().tree().get(id).expect("entry");
            assert_eq!(entry.path, *path);
            assert!(entry.state.is_alive(), "node {id:?} should be alive",);
        }

        assert_eq!(
            fix_path.0.last().map(|s| s.ty.to_string()).as_deref(),
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
        let fs = Rc::new(RefCell::new(LpFsMemory::new()));
        fs.borrow_mut()
            .write_file("/project.json".as_path(), br#"{"uid":"u","name":"n"}"#)
            .expect("project.json");
        fs.borrow_mut()
            .write_file(
                "/src/broken.shader/node.toml".as_path(),
                b"not valid toml {{{",
            )
            .expect("node.toml");

        let root_path = TreePath::parse("/p.show").expect("path");
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&*fs.borrow(), services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, CoreProjectLoadError::TomlParse { .. }),
            "expected TomlParse, got {err:?}"
        );
    }

    #[test]
    fn missing_project_json_returns_io_or_parse_flavored_error() {
        let fs = Rc::new(RefCell::new(LpFsMemory::new()));
        let root_path = TreePath::parse("/p.show").expect("path");
        let services = RuntimeServices::new(root_path);
        let err = match CoreProjectLoader::load_from_root(&*fs.borrow(), services) {
            Err(e) => e,
            Ok(_) => panic!("expected load error"),
        };
        assert!(
            matches!(err, CoreProjectLoadError::Io { .. }),
            "expected Io, got {err:?}"
        );
    }
}
