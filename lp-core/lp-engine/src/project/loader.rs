use crate::error::Error;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;
use lp_model::{AsLpPath, LpPath, LpPathBuf, NodeConfig, NodeKind, ProjectConfig};
use lp_shared::fs::LpFs;

/// Determine node kind from path suffix
pub(crate) fn node_kind_from_path(path: &LpPathBuf) -> Result<NodeKind, Error> {
    let path_str = path.as_str();

    // Find the last dot after the last slash
    let last_slash = path_str.rfind('/').unwrap_or(0);
    let after_slash = &path_str[last_slash..];

    // Extract suffix (part after last dot)
    let suffix = if let Some(dot_pos) = after_slash.rfind('.') {
        &after_slash[dot_pos + 1..]
    } else {
        // No type suffix found
        return Err(Error::InvalidConfig {
            node_path: path_str.to_string(),
            reason: "No type suffix on node path".to_string(),
        });
    };

    // Match suffix to node kind
    match suffix {
        "texture" => Ok(NodeKind::Texture),
        "shader" => Ok(NodeKind::Shader),
        "output" => Ok(NodeKind::Output),
        "fixture" => Ok(NodeKind::Fixture),
        _ => Err(Error::InvalidConfig {
            node_path: path_str.to_string(),
            reason: format!("Unknown node kind: {suffix}"),
        }),
    }
}

/// Check if a path is a node directory
pub(crate) fn is_node_directory(path: &LpPathBuf) -> bool {
    let path_str = path.as_str();
    path_str.ends_with(".texture")
        || path_str.ends_with(".shader")
        || path_str.ends_with(".output")
        || path_str.ends_with(".fixture")
}

/// Load project config from filesystem
pub fn load_from_filesystem(fs: &dyn LpFs) -> Result<ProjectConfig, Error> {
    let path = "/project.json";
    let data = fs.read_file(path.as_path()).map_err(|e| Error::Io {
        path: path.to_string(),
        details: format!("Failed to read: {e:?}"),
    })?;

    // Try to get a string representation of the data for error messages
    let data_str = core::str::from_utf8(&data)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| format!("<invalid UTF-8, {} bytes>", data.len()));

    // Show hex dump of first 100 bytes for debugging
    let hex_preview = if data.len() > 100 {
        format!("{:02x?}", &data[..100])
    } else {
        format!("{data:02x?}")
    };

    let config: ProjectConfig = lp_model::json::from_slice(&data).map_err(|e| Error::Parse {
        file: path.to_string(),
        error: format!(
            "{e}\n\nActual project.json content ({} bytes):\n{}\n\nHex dump (first 100 bytes):\n{}",
            data.len(),
            data_str,
            hex_preview
        ),
    })?;

    Ok(config)
}

/// Discover all node directories in /src/
pub fn discover_nodes(fs: &dyn LpFs) -> Result<Vec<LpPathBuf>, Error> {
    let path = "/src";
    let entries = fs.list_dir(path.as_path(), false).map_err(|e| Error::Io {
        path: path.to_string(),
        details: format!("Failed to list directory: {e:?}"),
    })?;

    let mut nodes = Vec::new();
    for entry in entries {
        if is_node_directory(&entry) {
            nodes.push(entry);
        }
    }

    Ok(nodes)
}

/// Load a node's config from filesystem
pub fn load_node(fs: &dyn LpFs, path: &LpPath) -> Result<(LpPathBuf, Box<dyn NodeConfig>), Error> {
    let node_json_path = path.to_path_buf().join("node.json");

    let data = fs
        .read_file(node_json_path.as_path())
        .map_err(|e| Error::Io {
            path: node_json_path.as_str().to_string(),
            details: format!("Failed to read: {e:?}"),
        })?;

    // Determine node kind from path suffix
    let kind = node_kind_from_path(&path.to_path_buf())?;

    // Parse config based on kind
    let config: Box<dyn NodeConfig> = match kind {
        NodeKind::Texture => {
            let cfg: lp_model::nodes::texture::TextureConfig = lp_model::json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.as_str().to_string(),
                    error: format!("Failed to parse texture config: {e}"),
                })?;
            Box::new(cfg)
        }
        NodeKind::Shader => {
            let cfg: lp_model::nodes::shader::ShaderConfig = lp_model::json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.as_str().to_string(),
                    error: format!("Failed to parse shader config: {e}"),
                })?;
            Box::new(cfg)
        }
        NodeKind::Output => {
            let cfg: lp_model::nodes::output::OutputConfig = lp_model::json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.as_str().to_string(),
                    error: format!("Failed to parse output config: {e}"),
                })?;
            Box::new(cfg)
        }
        NodeKind::Fixture => {
            let cfg: lp_model::nodes::fixture::FixtureConfig = lp_model::json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.as_str().to_string(),
                    error: format!("Failed to parse fixture config: {e}"),
                })?;
            Box::new(cfg)
        }
    };

    Ok((path.to_path_buf(), config))
}
