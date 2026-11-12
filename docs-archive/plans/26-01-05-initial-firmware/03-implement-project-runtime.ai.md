# Phase 3: Implement ProjectRuntime data structures

## Goal

Implement the `ProjectRuntime` structure that tracks runtime status for each node.

## Tasks

1. Create `src/project/runtime.rs` with:
   - `ProjectRuntime` struct with:
     - `uid: String`
     - `nodes: RuntimeNodes` (see below)
   - `RuntimeNodes` struct with:
     - `outputs: HashMap<u32, NodeStatus>`
     - `textures: HashMap<u32, NodeStatus>`
     - `shaders: HashMap<u32, NodeStatus>`
     - `fixtures: HashMap<u32, NodeStatus>`
   - `NodeStatus` enum:
     - `Ok`
     - `Warn { message: String }`
     - `Error { message: String }`
2. Implement `serde::Serialize` and `serde::Deserialize` for all types
3. Implement helper methods:
   - `new(uid: String)` - create empty runtime
   - `set_status(node_type, node_id, status)` - update status
   - `get_status(node_type, node_id)` - get status
4. Export from `src/project/mod.rs`

## Success Criteria

- `ProjectRuntime` can be serialized to/from JSON
- Status tracking works correctly
- Helper methods function as expected
- All code compiles without warnings

