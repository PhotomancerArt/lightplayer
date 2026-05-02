//! Legacy authored node configuration and source-side types; see `lpc_source::legacy`.

pub mod glsl_opts;
pub mod node_config_file;
pub mod node_loader;
pub mod nodes;

pub use node_config_file::{
    LEGACY_NODE_CONFIG_FILE, LegacyNodePathError, legacy_is_node_directory,
    legacy_node_config_path, legacy_node_kind_from_path,
};
pub use node_loader::{
    LegacyNodeLoadError, LegacyNodeReadRoot, discover_legacy_node_dirs, load_legacy_node_config,
};
