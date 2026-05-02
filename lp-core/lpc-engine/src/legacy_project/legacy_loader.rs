use crate::error::Error;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lpc_model::{AsLpPath, LpPath, LpPathBuf, ProjectConfig};
use lpc_source::legacy::nodes::{NodeConfig, NodeKind};
use lpc_source::legacy::{
    LegacyNodeLoadError, LegacyNodePathError, discover_legacy_node_dirs,
    legacy_node_kind_from_path as source_legacy_node_kind_from_path, load_legacy_node_config,
};
use lpfs::LpFs;

fn map_legacy_load_error<E: core::fmt::Debug>(err: LegacyNodeLoadError<E>) -> Error {
    match err {
        LegacyNodeLoadError::Io { path, error } => Error::Io {
            path: path.as_str().to_string(),
            details: format!("Failed to read or list: {error:?}"),
        },
        LegacyNodeLoadError::InvalidPath { path, reason } => Error::InvalidConfig {
            node_path: path.as_str().to_string(),
            reason: reason.to_string(),
        },
        LegacyNodeLoadError::UnknownKind { path, suffix } => Error::InvalidConfig {
            node_path: path.as_str().to_string(),
            reason: format!("Unknown node kind: {suffix}"),
        },
        LegacyNodeLoadError::Parse { path, error } => Error::Parse {
            file: path.as_str().to_string(),
            error: format!("Failed to parse node config: {error}"),
        },
    }
}

/// Determine node kind from path suffix
pub(crate) fn legacy_node_kind_from_path(path: &LpPathBuf) -> Result<NodeKind, Error> {
    source_legacy_node_kind_from_path(path).map_err(|e| match e {
        LegacyNodePathError::NoTypeSuffix { path } => Error::InvalidConfig {
            node_path: path.as_str().to_string(),
            reason: String::from("No type suffix on node path"),
        },
        LegacyNodePathError::UnknownKind { path, suffix } => Error::InvalidConfig {
            node_path: path.as_str().to_string(),
            reason: format!("Unknown node kind: {suffix}"),
        },
    })
}

/// Load project config from filesystem
pub fn legacy_load_from_filesystem(fs: &dyn LpFs) -> Result<ProjectConfig, Error> {
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

    let config: ProjectConfig = lpc_wire::json::from_slice(&data).map_err(|e| Error::Parse {
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
    discover_legacy_node_dirs(&fs, path.as_path()).map_err(map_legacy_load_error)
}

/// Load a node's config from filesystem (`node.toml`)
pub fn legacy_load_node(
    fs: &dyn LpFs,
    path: &LpPath,
) -> Result<(LpPathBuf, Box<dyn NodeConfig>), Error> {
    load_legacy_node_config(&fs, path).map_err(map_legacy_load_error)
}
