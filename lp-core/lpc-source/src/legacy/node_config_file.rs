//! Legacy per-node config file name and directory/kind policy (`node.toml`).

use crate::legacy::nodes::NodeKind;
use alloc::string::{String, ToString};
use lpc_model::lp_path::{LpPath, LpPathBuf};

/// On-disk sentinel for legacy node instance config (TOML).
pub const LEGACY_NODE_CONFIG_FILE: &str = "node.toml";

/// Path to `<node-dir>/node.toml`.
pub fn legacy_node_config_path(path: &LpPath) -> LpPathBuf {
    path.to_path_buf().join(LEGACY_NODE_CONFIG_FILE)
}

/// Errors from deriving [`NodeKind`] from a legacy node directory path.
#[derive(Debug, PartialEq, Eq)]
pub enum LegacyNodePathError {
    /// No `.suffix` after the last `/` in the path.
    NoTypeSuffix { path: LpPathBuf },
    /// Suffix is not one of `texture`, `shader`, `output`, `fixture`.
    UnknownKind { path: LpPathBuf, suffix: String },
}

/// Determine node kind from the directory name suffix (after the last `/` and last `.`).
pub fn legacy_node_kind_from_path(path: &LpPathBuf) -> Result<NodeKind, LegacyNodePathError> {
    let path_str = path.as_str();

    let last_slash = path_str.rfind('/').unwrap_or(0);
    let after_slash = &path_str[last_slash..];

    let suffix = if let Some(dot_pos) = after_slash.rfind('.') {
        &after_slash[dot_pos + 1..]
    } else {
        return Err(LegacyNodePathError::NoTypeSuffix { path: path.clone() });
    };

    match suffix {
        "texture" => Ok(NodeKind::Texture),
        "shader" => Ok(NodeKind::Shader),
        "output" => Ok(NodeKind::Output),
        "fixture" => Ok(NodeKind::Fixture),
        _ => Err(LegacyNodePathError::UnknownKind {
            path: path.clone(),
            suffix: suffix.to_string(),
        }),
    }
}

/// `true` when `path` ends with `.texture`, `.shader`, `.output`, or `.fixture`.
pub fn legacy_is_node_directory(path: &LpPathBuf) -> bool {
    let path_str = path.as_str();
    path_str.ends_with(".texture")
        || path_str.ends_with(".shader")
        || path_str.ends_with(".output")
        || path_str.ends_with(".fixture")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::lp_path::LpPath;

    #[test]
    fn legacy_node_kind_accepts_known_suffixes() {
        assert_eq!(
            legacy_node_kind_from_path(&LpPath::new("/src/x.texture").to_path_buf()),
            Ok(NodeKind::Texture)
        );
        assert_eq!(
            legacy_node_kind_from_path(&LpPath::new("/src/y.shader").to_path_buf()),
            Ok(NodeKind::Shader)
        );
        assert_eq!(
            legacy_node_kind_from_path(&LpPath::new("/src/z.output").to_path_buf()),
            Ok(NodeKind::Output)
        );
        assert_eq!(
            legacy_node_kind_from_path(&LpPath::new("/src/w.fixture").to_path_buf()),
            Ok(NodeKind::Fixture)
        );
    }

    #[test]
    fn legacy_node_kind_rejects_unknown_suffix() {
        let p = LpPath::new("/src/bad.unknown").to_path_buf();
        match legacy_node_kind_from_path(&p) {
            Err(LegacyNodePathError::UnknownKind { suffix, .. }) => {
                assert_eq!(suffix, "unknown");
            }
            other => panic!("expected UnknownKind, got {other:?}"),
        }
    }

    #[test]
    fn legacy_node_kind_rejects_missing_suffix() {
        let p = LpPath::new("/src/no_suffix").to_path_buf();
        assert!(matches!(
            legacy_node_kind_from_path(&p),
            Err(LegacyNodePathError::NoTypeSuffix { .. })
        ));
    }

    #[test]
    fn legacy_node_config_path_appends_node_toml() {
        let p = legacy_node_config_path(LpPath::new("/src/foo.shader"));
        assert_eq!(p.as_str(), "/src/foo.shader/node.toml");
    }
}
