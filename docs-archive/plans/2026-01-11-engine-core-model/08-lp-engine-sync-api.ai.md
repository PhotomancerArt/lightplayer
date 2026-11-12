# Phase 8: lp-engine - Sync API (Minimal)

## Goal

Implement `get_changes()` method on `ProjectRuntime` that returns `ProjectResponse` for client sync. Track changes via `config_ver` and `state_ver` in `NodeEntry`.

## Dependencies

- `lp-model` Phase 2 (API types)
- `lp-engine` Phase 7

## Implementation

### Update ProjectRuntime

**File**: Update `lp-engine/src/project/runtime.rs`
```rust
use lp_model::project::api::{
    ApiNodeSpecifier, NodeChange, NodeDetail, NodeState, NodeStatus as ApiNodeStatus,
    ProjectRequest, ProjectResponse,
};

impl ProjectRuntime {
    // ... existing methods ...
    
    /// Get changes since a frame (for client sync)
    pub fn get_changes(
        &self,
        since_frame: FrameId,
        detail_specifier: &ApiNodeSpecifier,
    ) -> Result<ProjectResponse, Error> {
        let mut node_handles = Vec::new();
        let mut node_changes = Vec::new();
        let mut node_details = alloc::collections::BTreeMap::new();
        
        // Collect all current handles
        for handle in self.nodes.keys() {
            node_handles.push(*handle);
        }
        
        // Determine which handles need detail
        let detail_handles: alloc::collections::BTreeSet<NodeHandle> = match detail_specifier {
            ApiNodeSpecifier::None => alloc::collections::BTreeSet::new(),
            ApiNodeSpecifier::All => self.nodes.keys().copied().collect(),
            ApiNodeSpecifier::ByHandles(handles) => handles.iter().copied().collect(),
            ApiNodeSpecifier::ByPaths(paths) => {
                // Resolve paths to handles
                let mut handles = alloc::collections::BTreeSet::new();
                for path in paths {
                    if let Some((handle, _)) = self.nodes.iter().find(|(_, entry)| entry.path == *path) {
                        handles.insert(*handle);
                    }
                }
                handles
            }
        };
        
        // Collect changes and details
        for (handle, entry) in &self.nodes {
            // Check for changes since since_frame
            if entry.config_ver.as_i64() > since_frame.as_i64() {
                node_changes.push(NodeChange::ConfigUpdated {
                    handle: *handle,
                    config_ver: entry.config_ver,
                });
            }
            
            if entry.state_ver.as_i64() > since_frame.as_i64() {
                node_changes.push(NodeChange::StateUpdated {
                    handle: *handle,
                    state_ver: entry.state_ver,
                });
            }
            
            // Check if node was created after since_frame
            if entry.config_ver.as_i64() > since_frame.as_i64() && entry.config_ver == entry.state_ver {
                node_changes.push(NodeChange::Created {
                    handle: *handle,
                    path: entry.path.clone(),
                    kind: entry.kind,
                });
            }
            
            // Add detail if requested
            if detail_handles.contains(handle) {
                let state = match entry.kind {
                    NodeKind::Texture => {
                        // todo!("Get actual texture state from runtime")
                        NodeState::Texture(lp_model::nodes::texture::TextureState {
                            texture_data: Vec::new(),
                        })
                    }
                    NodeKind::Shader => {
                        // todo!("Get actual shader state from runtime")
                        NodeState::Shader(lp_model::nodes::shader::ShaderState {
                            glsl_code: String::new(),
                            error: None,
                        })
                    }
                    NodeKind::Output => {
                        // todo!("Get actual output state from runtime")
                        NodeState::Output(lp_model::nodes::output::OutputState {
                            channel_data: Vec::new(),
                        })
                    }
                    NodeKind::Fixture => {
                        // todo!("Get actual fixture state from runtime")
                        NodeState::Fixture(lp_model::nodes::fixture::FixtureState {
                            lamp_colors: Vec::new(),
                        })
                    }
                };
                
                let api_status = match &entry.status {
                    NodeStatus::Created => ApiNodeStatus::Created,
                    NodeStatus::InitError(msg) => ApiNodeStatus::InitError(msg.clone()),
                    NodeStatus::Ok => ApiNodeStatus::Ok,
                    NodeStatus::Warn(msg) => ApiNodeStatus::Warn(msg.clone()),
                    NodeStatus::Error(msg) => ApiNodeStatus::Error(msg.clone()),
                };
                
                node_details.insert(*handle, NodeDetail {
                    path: entry.path.clone(),
                    config: entry.config.as_ref() as &dyn NodeConfig, // todo!("Proper config cloning/serialization")
                    state,
                    status: api_status,
                });
            }
        }
        
        Ok(ProjectResponse::GetChanges {
            current_frame: self.frame_id,
            node_handles,
            node_changes,
            node_details,
        })
    }
}
```

**File**: Update `lp-engine/src/project/mod.rs` to export API types:
```rust
pub mod loader;
pub mod runtime;

pub use loader::{discover_nodes, load_from_filesystem, load_node};
pub use runtime::{NodeEntry, NodeStatus, ProjectRuntime};

// Re-export API types for convenience
pub use lp_model::project::api::{
    ApiNodeSpecifier, ProjectRequest, ProjectResponse,
    NodeChange, NodeDetail, NodeState,
};
```

## Success Criteria

- All code compiles
- Can call `get_changes()` on `ProjectRuntime`
- Returns `ProjectResponse` with current frame, handles, changes, and details
- Change tracking works (config_ver, state_ver)
- Detail specifier filtering works

## Tests

- Create project and initialize nodes
- Call `get_changes()` with `since_frame = 0` and `All` specifier
- Verify response includes all nodes
- Call `get_changes()` with `since_frame = current_frame` and verify no changes
- Update a node's config_ver and verify change is detected
- Test `ByHandles` and `ByPaths` specifiers

## Notes

- State extraction uses `todo!()` - will need to access runtime state later
- Config cloning uses trait object cast - will need proper serialization later
- Change detection is basic - may need more sophisticated tracking later
