# Design: Filesystem Updates Integration

## File Structure

```
lp-engine/src/
├── nodes/
│   └── mod.rs                        # MODIFY: Add update_config() and handle_fs_change() to NodeRuntime trait
├── nodes/shader/
│   └── runtime.rs                    # MODIFY: Implement update_config() and handle_fs_change()
├── nodes/texture/
│   └── runtime.rs                    # MODIFY: Implement update_config() and handle_fs_change()
├── nodes/fixture/
│   └── runtime.rs                    # MODIFY: Implement update_config() and handle_fs_change()
├── nodes/output/
│   └── runtime.rs                    # MODIFY: Implement update_config() and handle_fs_change()
└── project/
    ├── loader.rs                     # MODIFY: Refactor load_node() to be callable with path (already exists)
    └── runtime.rs                    # MODIFY: Add handle_fs_changes() method

lp-engine/tests/
└── scene_update.rs                   # NEW: Test filesystem change handling
```

## New Types and Functions

### NodeRuntime Trait Extensions

```rust
// nodes/mod.rs
pub trait NodeRuntime: Send + Sync {
    // ... existing methods ...
    
    /// Update the node's configuration
    ///
    /// Called when node.json changes. Nodes can choose to reinit or update in place.
    fn update_config(
        &mut self,
        new_config: Box<dyn NodeConfig>,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error>;
    
    /// Handle filesystem changes to non-config files
    ///
    /// Called when files other than node.json change (e.g., main.glsl for shaders).
    fn handle_fs_change(
        &mut self,
        change: &FsChange,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error>;
}
```

### ProjectRuntime Methods

```rust
// project/runtime.rs
impl ProjectRuntime {
    /// Handle filesystem changes
    ///
    /// Processes filesystem change events and updates affected nodes.
    /// Should be called before tick() when filesystem changes occur.
    pub fn handle_fs_changes(&mut self, changes: &[FsChange]) -> Result<(), Error> {
        // 1. Process deletions first (remove nodes)
        // 2. Process creates (load new nodes)
        // 3. Process modifies (update existing nodes)
        //    - If node.json changed: call update_config()
        //    - If other files changed: call handle_fs_change()
    }
    
    /// Load a single node by path
    ///
    /// Refactored from load_nodes() to allow loading individual nodes.
    fn load_node_by_path(&mut self, path: &LpPath) -> Result<NodeHandle, Error> {
        // Load node config
        // Create node entry
        // Return handle
    }
    
    /// Check if a file path belongs to a node directory
    fn file_belongs_to_node(file_path: &str, node_path: &LpPath) -> bool {
        // Check if file_path starts with node_path
    }
}
```

## Implementation Details

### handle_fs_changes() Logic

1. **Process deletions first**:
   - Iterate through changes with `ChangeType::Delete`
   - For each change, iterate through nodes to find matching node directory
   - If `node.json` is deleted, remove the node from runtime
   - If node directory is deleted (detected by checking if path ends with `.shader`, `.texture`, etc.), remove the node

2. **Process creates**:
   - Iterate through changes with `ChangeType::Create`
   - Detect if it's a new node directory (path ends with `.shader`, `.texture`, etc.)
   - Call `load_node_by_path()` to load the new node
   - Initialize the node if needed

3. **Process modifies**:
   - Iterate through changes with `ChangeType::Modify`
   - For each change, iterate through nodes to find matching node directory
   - If `node.json` changed: reload config and call `update_config()`
   - If other files changed (e.g., `main.glsl`): call `handle_fs_change()`

### Node Implementation Examples

**ShaderRuntime**:
- `update_config()`: Update config, may need to reinitialize if texture reference changed
- `handle_fs_change()`: If `main.glsl` changed, reload and recompile shader

**TextureRuntime**:
- `update_config()`: Update config (e.g., resize texture if dimensions changed)
- `handle_fs_change()`: Handle texture file changes if we support loading from files

**FixtureRuntime**:
- `update_config()`: Update config, may need to re-resolve texture/output handles
- `handle_fs_change()`: Handle mapping file changes if we support external mapping files

**OutputRuntime**:
- `update_config()`: Update config (e.g., change pin number)
- `handle_fs_change()`: Likely no-op for now

## Test Design

### scene_update.rs

Test cases:
1. **node.json modification**: Change shader config, verify it's applied
2. **main.glsl modification**: Change shader source, verify it's recompiled and applied
3. **Node deletion**: Delete node.json, verify node is removed
4. **Node creation**: Create new node directory, verify it's loaded
5. **Multiple changes**: Change both node.json and main.glsl, verify both are applied

Test structure similar to `scene_render.rs`:
- Create project with `ProjectBuilder`
- Initialize runtime
- Make filesystem changes using `LpFsMemory::write_file_mut()`
- Call `handle_fs_changes()` with `FsChange` events
- Verify changes are applied (check output, node status, etc.)
