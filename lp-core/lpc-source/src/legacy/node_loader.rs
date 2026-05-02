//! Generic discovery and TOML load for legacy `node.toml` configs.

use crate::legacy::node_config_file::{
    legacy_is_node_directory, legacy_node_config_path, legacy_node_kind_from_path,
};
use crate::legacy::nodes::{
    NodeConfig, NodeKind, fixture::FixtureConfig, output::OutputConfig, shader::ShaderConfig,
    texture::TextureConfig,
};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use lpc_model::lp_path::{LpPath, LpPathBuf};

/// Errors while reading or parsing a legacy `node.toml`.
#[derive(Debug)]
pub enum LegacyNodeLoadError<E> {
    /// Underlying filesystem read or list failed.
    Io { path: LpPathBuf, error: E },
    /// Path or file contents could not be interpreted for load.
    InvalidPath {
        path: LpPathBuf,
        reason: &'static str,
    },
    /// Directory suffix is not a known [`NodeKind`].
    UnknownKind { path: LpPathBuf, suffix: String },
    /// TOML did not match the config type for this node kind.
    Parse {
        path: LpPathBuf,
        error: toml::de::Error,
    },
}

/// Narrow filesystem surface for legacy node load (no `lpfs` dependency).
pub trait LegacyNodeReadRoot {
    /// Low-level error returned when reading bytes or listing directories fails.
    type Err;

    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, Self::Err>;

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, Self::Err>;
}

fn map_path_err<E>(e: super::node_config_file::LegacyNodePathError) -> LegacyNodeLoadError<E> {
    use super::node_config_file::LegacyNodePathError;
    match e {
        LegacyNodePathError::NoTypeSuffix { path } => LegacyNodeLoadError::InvalidPath {
            path,
            reason: "No type suffix on node path",
        },
        LegacyNodePathError::UnknownKind { path, suffix } => {
            LegacyNodeLoadError::UnknownKind { path, suffix }
        }
    }
}

/// Lists immediate children of `src_path` that look like legacy node directories.
pub fn discover_legacy_node_dirs<R>(
    fs: &R,
    src_path: &LpPath,
) -> Result<Vec<LpPathBuf>, LegacyNodeLoadError<R::Err>>
where
    R: LegacyNodeReadRoot + ?Sized,
{
    let entries = fs
        .list_dir(src_path, false)
        .map_err(|e| LegacyNodeLoadError::Io {
            path: src_path.to_path_buf(),
            error: e,
        })?;

    let mut out = Vec::new();
    for entry in entries {
        if legacy_is_node_directory(&entry) {
            out.push(entry);
        }
    }
    Ok(out)
}

/// Reads `<path>/node.toml`, parses by directory suffix, returns boxed [`NodeConfig`].
pub fn load_legacy_node_config<R>(
    fs: &R,
    path: &LpPath,
) -> Result<(LpPathBuf, Box<dyn NodeConfig>), LegacyNodeLoadError<R::Err>>
where
    R: LegacyNodeReadRoot + ?Sized,
{
    let node_dir = path.to_path_buf();
    let config_path = legacy_node_config_path(path);
    let data = fs
        .read_file(config_path.as_path())
        .map_err(|e| LegacyNodeLoadError::Io {
            path: config_path.clone(),
            error: e,
        })?;

    let text = core::str::from_utf8(&data).map_err(|_| LegacyNodeLoadError::InvalidPath {
        path: config_path.clone(),
        reason: "file is not valid UTF-8",
    })?;

    let kind = legacy_node_kind_from_path(&node_dir).map_err(map_path_err)?;

    let config: Box<dyn NodeConfig> = match kind {
        NodeKind::Texture => {
            let cfg: TextureConfig =
                toml::from_str(text).map_err(|e| LegacyNodeLoadError::Parse {
                    path: config_path.clone(),
                    error: e,
                })?;
            Box::new(cfg)
        }
        NodeKind::Shader => {
            let cfg: ShaderConfig =
                toml::from_str(text).map_err(|e| LegacyNodeLoadError::Parse {
                    path: config_path.clone(),
                    error: e,
                })?;
            Box::new(cfg)
        }
        NodeKind::Output => {
            let cfg: OutputConfig =
                toml::from_str(text).map_err(|e| LegacyNodeLoadError::Parse {
                    path: config_path.clone(),
                    error: e,
                })?;
            Box::new(cfg)
        }
        NodeKind::Fixture => {
            let cfg: FixtureConfig =
                toml::from_str(text).map_err(|e| LegacyNodeLoadError::Parse {
                    path: config_path.clone(),
                    error: e,
                })?;
            Box::new(cfg)
        }
    };

    Ok((node_dir, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy::node_config_file::LEGACY_NODE_CONFIG_FILE;
    use crate::legacy::nodes::shader::ShaderConfig;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_model::NodeSpec;

    #[derive(Debug)]
    struct MockErr;

    struct MockLegacyFs {
        files: BTreeMap<String, Vec<u8>>,
        dir_children: BTreeMap<String, Vec<LpPathBuf>>,
    }

    impl MockLegacyFs {
        fn new() -> Self {
            Self {
                files: BTreeMap::new(),
                dir_children: BTreeMap::new(),
            }
        }

        fn with_shader_node(mut self, node_dir: &str, toml_body: &str) -> Self {
            let path = LpPathBuf::from(node_dir)
                .join(LEGACY_NODE_CONFIG_FILE)
                .as_str()
                .to_string();
            self.files.insert(path, toml_body.as_bytes().to_vec());
            self
        }

        fn with_src_listing(mut self, children: Vec<&str>) -> Self {
            let paths: Vec<LpPathBuf> = children.iter().map(|s| LpPathBuf::from(*s)).collect();
            self.dir_children.insert("/src".to_string(), paths);
            self
        }
    }

    impl LegacyNodeReadRoot for MockLegacyFs {
        type Err = MockErr;

        fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, MockErr> {
            self.files.get(path.as_str()).cloned().ok_or(MockErr)
        }

        fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, MockErr> {
            assert!(!recursive, "test only uses non-recursive listing");
            self.dir_children.get(path.as_str()).cloned().ok_or(MockErr)
        }
    }

    #[test]
    fn load_legacy_node_config_parses_shader_toml() {
        let fs = MockLegacyFs::new().with_shader_node(
            "/src/rainbow.shader",
            r#"
glsl_path = "main.glsl"
texture_spec = "/src/main.texture"
render_order = 2
"#,
        );
        let (dir, cfg) = load_legacy_node_config(&fs, LpPath::new("/src/rainbow.shader")).unwrap();
        assert_eq!(dir.as_str(), "/src/rainbow.shader");
        assert_eq!(cfg.kind(), NodeKind::Shader);
        let any = cfg.as_any().downcast_ref::<ShaderConfig>().expect("shader");
        assert_eq!(any.glsl_path.as_str(), "main.glsl");
        assert_eq!(any.texture_spec, NodeSpec::from("/src/main.texture"));
        assert_eq!(any.render_order, 2);
    }

    #[test]
    fn discover_legacy_node_dirs_returns_only_legacy_directories() {
        let fs = MockLegacyFs::new().with_src_listing(vec![
            "/src/a.shader",
            "/src/b.texture",
            "/src/readme.txt",
            "/src/nested/other.shader",
        ]);
        let found = discover_legacy_node_dirs(&fs, LpPath::new("/src")).unwrap();
        let mut sorted = found
            .into_iter()
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>();
        sorted.sort();
        assert_eq!(
            sorted,
            vec![
                "/src/a.shader".to_string(),
                "/src/b.texture".to_string(),
                "/src/nested/other.shader".to_string(),
            ]
        );
    }
}
