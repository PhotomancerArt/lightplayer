# Phase 3: lp-engine - Project Loading (Minimal)

## Goal

Load project from filesystem and discover nodes. Create skeleton `ProjectRuntime` that can hold nodes but doesn't initialize them yet.

## Dependencies

- `lp-model` (Phase 1 & 2)
- `lp-shared` (for `LpFs` trait)

## Implementation

### 1. Error Type

**File**: `lp-engine/src/error.rs`

```rust
use alloc::string::String;

/// Engine error type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// I/O error
    Io {
        /// Path that failed
        path: String,
        /// Error details
        details: String,
    },
    /// Parse error (JSON, etc.)
    Parse {
        /// File being parsed
        file: String,
        /// Parse error details
        error: String,
    },
    /// Not found
    NotFound {
        /// Path that was not found
        path: String,
    },
    /// Invalid configuration
    InvalidConfig {
        /// Node path
        node_path: String,
        /// Reason for invalidity
        reason: String,
    },
    /// Other error
    Other {
        /// Error message
        message: String,
    },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Io { path, details } => {
                write!(f, "I/O error: {} ({})", details, path)
            }
            Error::Parse { file, error } => {
                write!(f, "Parse error in {}: {}", file, error)
            }
            Error::NotFound { path } => {
                write!(f, "Not found: {}", path)
            }
            Error::InvalidConfig { node_path, reason } => {
                write!(f, "Invalid config for {}: {}", node_path, reason)
            }
            Error::Other { message } => {
                write!(f, "Error: {}", message)
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
```

### 2. Project Loader

**File**: `lp-engine/src/project/loader.rs`

```rust
use crate::error::Error;
use lp_model::{LpPath, NodeConfig, NodeKind, ProjectConfig};
use lp_shared::fs::LpFs;
use alloc::string::String;
use alloc::vec::Vec;
use serde_json;

/// Determine node kind from path suffix
pub(crate) fn node_kind_from_path(path: &LpPath) -> Result<NodeKind, Error> {
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
            reason: format!("Unknown node kind: {}", suffix),
        }),
    }
}

/// Check if a path is a node directory
pub(crate) fn is_node_directory(path: &str) -> bool {
    path.ends_with(".texture") || path.ends_with(".shader") ||
    path.ends_with(".output") || path.ends_with(".fixture")
}

/// Load project config from filesystem
pub fn load_from_filesystem(fs: &dyn LpFs) -> Result<ProjectConfig, Error> {
    let path = "/project.json";
    let data = fs.read_file(path)
        .map_err(|e| Error::Io {
            path: path.to_string(),
            details: format!("Failed to read: {:?}", e),
        })?;

    let config: ProjectConfig = serde_json::from_slice(&data)
        .map_err(|e| Error::Parse {
            file: path.to_string(),
            error: format!("{}", e),
        })?;

    Ok(config)
}

/// Discover all node directories in /src/
pub fn discover_nodes(fs: &dyn LpFs) -> Result<Vec<LpPath>, Error> {
    let path = "/src";
    let entries = fs.list_dir(path)
        .map_err(|e| Error::Io {
            path: path.to_string(),
            details: format!("Failed to list directory: {:?}", e),
        })?;

    let mut nodes = Vec::new();
    for entry in entries {
        if is_node_directory(&entry) {
            nodes.push(LpPath::from(entry));
        }
    }

    Ok(nodes)
}

/// Load a node's config from filesystem
pub fn load_node(fs: &dyn LpFs, path: &LpPath) -> Result<(LpPath, Box<dyn NodeConfig>), Error> {
    let node_json_path = format!("{}/node.json", path.as_str());

    let data = fs.read_file(&node_json_path)
        .map_err(|e| Error::Io {
            path: node_json_path.clone(),
            details: format!("Failed to read: {:?}", e),
        })?;

    // Determine node kind from path suffix
    let kind = node_kind_from_path(path)?;

    // Parse config based on kind
    let config: Box<dyn NodeConfig> = match kind {
        NodeKind::Texture => {
            let cfg: lp_model::nodes::texture::TextureConfig = serde_json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.clone(),
                    error: format!("Failed to parse texture config: {}", e),
                })?;
            Box::new(cfg)
        }
        NodeKind::Shader => {
            let cfg: lp_model::nodes::shader::ShaderConfig = serde_json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.clone(),
                    error: format!("Failed to parse shader config: {}", e),
                })?;
            Box::new(cfg)
        }
        NodeKind::Output => {
            let cfg: lp_model::nodes::output::OutputConfig = serde_json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.clone(),
                    error: format!("Failed to parse output config: {}", e),
                })?;
            Box::new(cfg)
        }
        NodeKind::Fixture => {
            let cfg: lp_model::nodes::fixture::FixtureConfig = serde_json::from_slice(&data)
                .map_err(|e| Error::Parse {
                    file: node_json_path.clone(),
                    error: format!("Failed to parse fixture config: {}", e),
                })?;
            Box::new(cfg)
        }
    };

    Ok((path.clone(), config))
}
```

### 3. Project Runtime (Skeleton)

**File**: `lp-engine/src/project/runtime.rs`

```rust
use crate::error::Error;
use crate::nodes::NodeRuntime;
use lp_model::{
    FrameId, LpPath, NodeConfig, NodeHandle, NodeKind, ProjectConfig,
};
use lp_shared::fs::LpFs;
use alloc::collections::BTreeMap;
use alloc::boxed::Box;

/// Project runtime - manages nodes and rendering
pub struct ProjectRuntime {
    /// Current frame ID
    pub frame_id: FrameId,
    /// Filesystem (owned for now)
    pub fs: Box<dyn LpFs>,
    /// Node entries
    pub nodes: BTreeMap<NodeHandle, NodeEntry>,
    /// Next handle to assign
    pub next_handle: i32,
}

/// Node entry in runtime
pub struct NodeEntry {
    /// Node path
    pub path: LpPath,
    /// Node kind
    pub kind: NodeKind,
    /// Node config
    pub config: Box<dyn NodeConfig>,
    /// Frame when config was last updated
    pub config_ver: FrameId,
    /// Node status
    pub status: NodeStatus,
    /// Node runtime (None until initialized)
    pub runtime: Option<Box<dyn NodeRuntime>>,
    /// Last frame state updates occurred
    pub state_ver: FrameId,
}

/// Node status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStatus {
    /// Created but not yet initialized
    Created,
    /// Error initializing the node
    InitError(String),
    /// Node is running normally
    Ok,
    /// Node is running, but something is wrong
    Warn(String),
    /// Node cannot run
    Error(String),
}

impl ProjectRuntime {
    /// Create new project runtime
    pub fn new(fs: Box<dyn LpFs>) -> Result<Self, Error> {
        let config = crate::project::loader::load_from_filesystem(fs.as_ref())?;

        Ok(Self {
            frame_id: FrameId::default(),
            fs,
            nodes: BTreeMap::new(),
            next_handle: 1,
        })
    }

    /// Load nodes from filesystem (doesn't initialize them)
    pub fn load_nodes(&mut self) -> Result<(), Error> {
        let node_paths = crate::project::loader::discover_nodes(self.fs.as_ref())?;

        for path in node_paths {
            match crate::project::loader::load_node(self.fs.as_ref(), &path) {
                Ok((path, config)) => {
                    let handle = NodeHandle::new(self.next_handle);
                    self.next_handle += 1;

                    let kind = config.kind();
                    let entry = NodeEntry {
                        path,
                        kind,
                        config,
                        config_ver: self.frame_id,
                        status: NodeStatus::Created,
                        runtime: None,
                        state_ver: FrameId::default(),
                    };

                    self.nodes.insert(handle, entry);
                }
                Err(e) => {
                    // Create entry with error status
                    let handle = NodeHandle::new(self.next_handle);
                    self.next_handle += 1;

                    // Try to determine kind from path
                    let kind = match crate::project::loader::node_kind_from_path(&path) {
                        Ok(k) => k,
                        Err(_) => continue, // Skip unknown types
                    };

                    let entry = NodeEntry {
                        path,
                        kind,
                        config: todo!(), // Need to create a dummy config - will handle later
                        config_ver: self.frame_id,
                        status: NodeStatus::InitError(format!("Failed to load: {}", e)),
                        runtime: None,
                        state_ver: FrameId::default(),
                    };

                    self.nodes.insert(handle, entry);
                }
            }
        }

        Ok(())
    }
}
```

### 4. Module Structure

**File**: `lp-engine/src/project/mod.rs`

```rust
pub mod loader;
pub mod runtime;

pub use loader::{discover_nodes, load_from_filesystem, load_node};
pub use runtime::{NodeEntry, NodeStatus, ProjectRuntime};
```

**File**: `lp-engine/src/lib.rs`

```rust
#![no_std]

extern crate alloc;

pub mod error;
pub mod project;

pub use error::Error;
pub use project::ProjectRuntime;
```

**File**: Update `lp-engine/Cargo.toml`

```toml
[dependencies]
lp-model = { path = "../lp-model", default-features = false }
lp-shared = { path = "../lp-shared", default-features = false }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
hashbrown = { workspace = true }
alloc = { package = "alloc", version = "1.0", features = ["alloc"] }
```

## Success Criteria

- All code compiles
- Can load project from `LpFsMemory`
- Can discover nodes
- Can create `ProjectRuntime` and call `load_nodes()`
- Test: Load a project with nodes and verify they're discovered

## Tests

Create test in `lp-engine/tests/`:

- Create `LpFsMemory` with project.json and node directories
- Call `load_from_filesystem()` and verify config
- Call `discover_nodes()` and verify paths
- Create `ProjectRuntime` and call `load_nodes()`, verify entries created

## Notes

- Error handling for failed node loads creates entries with `InitError` status
- `NodeEntry::config` for failed loads uses `todo!()` - we'll need a way to create dummy configs later
- `ProjectRuntime` owns `LpFs` - may need to change to `&mut` later if needed
- **Error types**: Errors use semantic variants with structured fields (path, file, node, etc.) similar to `FsError` pattern
- Error messages include variant name in Display output for easy searching
