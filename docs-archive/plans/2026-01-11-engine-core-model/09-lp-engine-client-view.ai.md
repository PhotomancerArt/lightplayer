# Phase 9: lp-engine-client - Client View (Minimal)

## Goal

Create `lp-engine-client` crate with client view that can sync with engine. Keep sync logic minimal - just enough to update view from `ProjectResponse`.

## Dependencies

- `lp-model` (all phases)
- `lp-engine` Phase 8

## Implementation

### 1. Create lp-engine-client crate

**File**: `lp-app/crates/lp-engine-client/Cargo.toml`

```toml
[package]
name = "lp-engine-client"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["std"]
std = []

[dependencies]
lp-model = { path = "../lp-model", default-features = false }
hashbrown = { workspace = true }
alloc = { package = "alloc", version = "1.0", features = ["alloc"] }
```

### 2. Client Project View

**File**: `lp-engine-client/src/project/view.rs`

```rust
use lp_model::{
    FrameId, NodeConfig, NodeHandle, NodeKind, NodeState,
    project::api::{ApiNodeSpecifier, NodeChange, NodeDetail, NodeStatus},
};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

/// Client view of project
pub struct ClientProjectView {
    /// Current frame ID (last synced)
    pub frame_id: FrameId,
    /// Node entries
    pub nodes: BTreeMap<NodeHandle, ClientNodeEntry>,
    /// Which nodes we're tracking detail for
    pub detail_tracking: BTreeSet<NodeHandle>,
}

/// Client node entry
pub struct ClientNodeEntry {
    pub path: lp_model::LpPath,
    pub kind: NodeKind,
    pub config: Box<dyn NodeConfig>, // todo!("Proper config storage/cloning")
    pub config_ver: FrameId,
    pub state: Option<NodeState>, // Only present if in detail_tracking
    pub state_ver: FrameId,
    pub status: NodeStatus,
}

impl ClientProjectView {
    /// Create new client view
    pub fn new() -> Self {
        Self {
            frame_id: FrameId::default(),
            nodes: BTreeMap::new(),
            detail_tracking: BTreeSet::new(),
        }
    }

    /// Request detail tracking for nodes
    pub fn request_detail(&mut self, handles: Vec<NodeHandle>) {
        for handle in handles {
            self.detail_tracking.insert(handle);
        }
    }

    /// Stop detail tracking for nodes
    pub fn stop_detail(&mut self, handles: Vec<NodeHandle>) {
        for handle in handles {
            self.detail_tracking.remove(&handle);
            // Clear state when stopping detail
            if let Some(entry) = self.nodes.get_mut(&handle) {
                entry.state = None;
            }
        }
    }

    /// Generate detail specifier for sync
    pub fn detail_specifier(&self) -> ApiNodeSpecifier {
        if self.detail_tracking.is_empty() {
            ApiNodeSpecifier::None
        } else {
            ApiNodeSpecifier::ByHandles(self.detail_tracking.iter().copied().collect())
        }
    }

    /// Sync with server (update view from response)
    pub fn sync(&mut self, response: &lp_model::project::api::ProjectResponse) -> Result<(), String> {
        match response {
            lp_model::project::api::ProjectResponse::GetChanges {
                current_frame,
                node_handles,
                node_changes,
                node_details,
            } => {
                // Update frame ID
                self.frame_id = *current_frame;

                // Prune removed nodes
                let handles_set: BTreeSet<NodeHandle> = node_handles.iter().copied().collect();
                self.nodes.retain(|handle, _| handles_set.contains(handle));

                // Apply changes
                for change in node_changes {
                    match change {
                        NodeChange::Created { handle, path, kind } => {
                            // Create new entry
                            self.nodes.insert(*handle, ClientNodeEntry {
                                path: path.clone(),
                                kind: *kind,
                                config: todo!(), // Will need to get from details
                                config_ver: FrameId::default(),
                                state: None,
                                state_ver: FrameId::default(),
                                status: NodeStatus::Created,
                            });
                        }
                        NodeChange::ConfigUpdated { handle, config_ver } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.config_ver = *config_ver;
                                // todo!("Update config from details if available")
                            }
                        }
                        NodeChange::StateUpdated { handle, state_ver } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.state_ver = *state_ver;
                                // todo!("Update state from details if tracking")
                            }
                        }
                        NodeChange::StatusChanged { handle, status } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.status = status.clone();
                            }
                        }
                        NodeChange::Removed { handle } => {
                            self.nodes.remove(handle);
                            self.detail_tracking.remove(handle);
                        }
                    }
                }

                // Update details
                for (handle, detail) in node_details {
                    if let Some(entry) = self.nodes.get_mut(handle) {
                        entry.config = detail.config.as_ref() as &dyn NodeConfig; // todo!("Proper cloning")
                        entry.state = Some(detail.state.clone());
                        entry.status = detail.status.clone();
                    }
                }

                Ok(())
            }
        }
    }
}
```

### 3. Client API Trait

**File**: `lp-engine-client/src/api/client.rs`

```rust
use lp_model::project::api::{ProjectRequest, ProjectResponse};

/// Client API trait - implemented by server connection
pub trait ClientApi {
    /// Get changes from server
    fn get_changes(&self, request: ProjectRequest) -> Result<ProjectResponse, String>;
}
```

**File**: `lp-engine-client/src/api/mod.rs`

```rust
pub mod client;

pub use client::ClientApi;
```

### 4. Module Structure

**File**: `lp-engine-client/src/project/mod.rs`

```rust
pub mod view;

pub use view::{ClientNodeEntry, ClientProjectView};
```

**File**: `lp-engine-client/src/lib.rs`

```rust
#![no_std]

extern crate alloc;

pub mod api;
pub mod project;

pub use api::ClientApi;
pub use project::{ClientNodeEntry, ClientProjectView};
```

## Success Criteria

- All code compiles
- Can create `ClientProjectView`
- Can call `request_detail()`, `stop_detail()`, `detail_specifier()`
- Can call `sync()` with `ProjectResponse` and update view
- View tracks nodes, configs, states, and status

## Tests

- Create `ClientProjectView`
- Request detail for a node handle
- Call `sync()` with `GetChanges` response
- Verify view is updated
- Stop detail and verify state is cleared

## Notes

- Config cloning uses trait object cast - will need proper serialization later
- Sync logic is minimal - handles basic cases
- Detail tracking works but state extraction from details needs work
