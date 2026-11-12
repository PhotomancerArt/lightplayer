# Phase 3: Convert ProjectRuntime HashMaps to Use Handles

## Goal

Convert `ProjectRuntime` HashMaps from path-based IDs to `NodeHandle` keys.

## Tasks

1. Update `ProjectRuntime` HashMap types:
   - Change `textures: HashMap<TextureId, TextureNodeRuntime>` to `HashMap<NodeHandle, TextureNodeRuntime>`
   - Change `shaders: HashMap<ShaderId, ShaderNodeRuntime>` to `HashMap<NodeHandle, ShaderNodeRuntime>`
   - Change `fixtures: HashMap<FixtureId, FixtureNodeRuntime>` to `HashMap<NodeHandle, FixtureNodeRuntime>`
   - Change `outputs: HashMap<OutputId, OutputNodeRuntime>` to `HashMap<NodeHandle, OutputNodeRuntime>`

2. Update `ProjectRuntime::init()`:
   - Assign handles using `assign_next_handle()`
   - Insert nodes using handles as keys
   - Get path from node ID (TextureId, ShaderId, etc.) and pass to node runtime

3. Update all methods that access nodes by ID:
   - Change to use handles instead of path-based IDs
   - Update `get_node_detail()` to search all node type HashMaps
   - Update `get_changed_nodes_since()` to iterate all node types
   - Update `get_all_node_handles()` to collect from all HashMaps

4. Update node runtime references:
   - When nodes reference other nodes (e.g., fixture references output), change to use `NodeHandle` instead of path
   - Update `InitContext` and render contexts if needed

5. Update `ProjectRuntime::update()`:
   - Iterate using handles instead of IDs

6. Remove path-based ID lookups:
   - Replace any remaining `get_node_by_path()` style methods with handle-based lookups
   - For file change handling, iterate HashMaps to find nodes by path (acceptable since rare)

## Success Criteria

- All HashMaps use `NodeHandle` as keys
- Node assignment and lookup works correctly
- Node references use handles instead of paths
- All code compiles without warnings
- Existing tests still pass (may need updates for handle-based access)
