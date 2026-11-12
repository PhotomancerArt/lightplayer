# Phase 5: Implement Server-Side GetChanges Handler

## Goal

Implement the server-side handler for `GetChanges` requests.

## Tasks

1. Implement `get_node_detail()` in `ProjectRuntime`:
   - Search all node type HashMaps for the handle
   - Build `NodeDetail` with path and state
   - Convert node runtime state to `NodeState` enum

2. Implement `get_changed_nodes_since()` in `ProjectRuntime`:
   - Iterate all node type HashMaps
   - Check if `min(last_config_frame, last_state_frame) > since_frame` OR `created_frame > since_frame`
   - Collect handles into Vec

3. Implement `get_engine_stats()` in `ProjectRuntime`:
   - Extract frame timing from `frame_time`
   - Calculate memory usage (placeholder for now, actual tracking can be added later)
   - Return `EngineStats`

4. Create `handle_project_request()` in `lp-server/src/handlers.rs`:
   - Match on `ProjectRequest::GetChanges`
   - Get runtime from `ProjectManager`
   - Call `get_current_frame()`, `get_changed_nodes_since()`, etc.
   - Build `ProjectResponse::GetChanges` with:
     - Current frame
     - Changed nodes
     - Nodes matching `detail_specifier`
     - All current node handles
     - Engine stats

5. Wire up handler in `ProjectManager`:
   - Add method to handle `ProjectRequest`
   - Call handler and return `ProjectResponse`

6. Update `ProjectRuntime` to implement state conversion:
   - Add methods to convert node runtime state to `NodeState` enum
   - Handle texture, shader, output state conversion

## Success Criteria

- `get_node_detail()` returns correct node details
- `get_changed_nodes_since()` correctly identifies changed nodes
- `get_engine_stats()` returns engine statistics
- `GetChanges` handler builds correct response
- Handler correctly includes changed nodes and detail-specified nodes
- All code compiles without warnings
- Tests verify GetChanges response is correct
