# Phase 2: lp-model - API Types (Minimal)

## Goal

Create types for client sync protocol. Keep it minimal - just the types, no serialization yet.

## Implementation

### Project API Types

**File**: `lp-model/src/project/api.rs`

```rust
use crate::nodes::{NodeHandle, NodeKind, NodeSpecifier, NodeConfig};
use crate::path::LpPath;
use crate::project::FrameId;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Node specifier for API requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiNodeSpecifier {
    /// No nodes
    None,
    /// All nodes
    All,
    /// Specific handles
    ByHandles(Vec<NodeHandle>),
}

/// Project request from client
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRequest {
    /// Get changes since a frame
    GetChanges {
        /// Last frame client synced
        since_frame: FrameId,
        /// Which nodes need full state
        detail_specifier: ApiNodeSpecifier,
    },
}

/// Project response from server
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResponse {
    /// Changes response
    GetChanges {
        /// Current frame ID
        current_frame: FrameId,
        /// All current node handles (for pruning removed nodes)
        node_handles: Vec<NodeHandle>,
        /// Changed nodes since since_frame
        node_changes: Vec<NodeChange>,
        /// Full detail for requested nodes
        node_details: BTreeMap<NodeHandle, NodeDetail>,
    },
}

/// Node change notification
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeChange {
    /// New node created
    Created {
        handle: NodeHandle,
        path: LpPath,
        kind: NodeKind,
    },
    /// Config updated
    ConfigUpdated {
        handle: NodeHandle,
        config_ver: FrameId,
    },
    /// State updated
    StateUpdated {
        handle: NodeHandle,
        state_ver: FrameId,
    },
    /// Status changed
    StatusChanged {
        handle: NodeHandle,
        status: NodeStatus,
    },
    /// Node removed
    Removed {
        handle: NodeHandle,
    },
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

/// Node detail - full config + state + status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDetail {
    pub path: LpPath,
    pub config: Box<dyn NodeConfig>, // todo!() - will need serialization later
    pub state: NodeState, // External state only
    pub status: NodeStatus,
}

/// Node state - external state (shared with clients)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeState {
    Texture(crate::nodes::texture::TextureState),
    Shader(crate::nodes::shader::ShaderState),
    Output(crate::nodes::output::OutputState),
    Fixture(crate::nodes::fixture::FixtureState),
}
```

**File**: Update `lp-model/src/project/mod.rs`

```rust
pub mod api;
pub mod config;
pub mod frame_id;

pub use config::ProjectConfig;
pub use frame_id::FrameId;
pub use api::{
    ApiNodeSpecifier, ProjectRequest, ProjectResponse,
    NodeChange, NodeDetail, NodeState, NodeStatus,
};
```

## Success Criteria

- All code compiles
- Can construct all API types
- Types match design document

## Notes

- `NodeDetail::config` uses `Box<dyn NodeConfig>` - serialization will be handled later with `todo!()`
- `NodeState` enum wraps each node state type - this is the external state shared with clients
- `NodeStatus` matches the design - tracks node lifecycle and errors
