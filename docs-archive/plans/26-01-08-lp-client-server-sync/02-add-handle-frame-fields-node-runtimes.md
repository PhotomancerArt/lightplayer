# Phase 2: Add Handle and Frame Fields to Node Runtimes

## Goal

Add handle, path, and frame tracking fields to all node runtime structures.

## Tasks

1. Create `NodeRuntimeBase` struct in `lp-engine/src/runtime/mod.rs`:
   ```rust
   pub struct NodeRuntimeBase {
       pub handle: NodeHandle,
       pub path: String,
       pub created_frame: FrameId,
       pub last_config_frame: FrameId,
       pub last_state_frame: FrameId,
   }
   ```

2. Update each node runtime struct to include `base: NodeRuntimeBase`:
   - `TextureNodeRuntime`
   - `ShaderNodeRuntime`
   - `FixtureNodeRuntime`
   - `OutputNodeRuntime`

3. Update node runtime `new()` methods:
   - Take `handle: NodeHandle` and `path: String` parameters
   - Initialize `base` with handle, path, and current frame

4. Update node runtime `init()` methods:
   - Accept `current_frame: FrameId` parameter
   - Set `base.created_frame = current_frame`
   - Set `base.last_config_frame = current_frame`
   - Set `base.last_state_frame = current_frame`
   - Update `last_config_frame` when config changes

5. Update node runtime `update()` methods:
   - Accept `current_frame: FrameId` parameter
   - Update `base.last_state_frame = current_frame` when state actually changes
   - Only update when state changes (not every frame if state is unchanged)

6. Update `ProjectRuntime::init()`:
   - Pass `current_frame` and assign handles to each node runtime
   - Get path from node config/ID

7. Update `ProjectRuntime::update()`:
   - Pass `current_frame` to node update methods

## Success Criteria

- All node runtimes have `base: NodeRuntimeBase` field
- Node runtimes store handle and path
- Frame tracking fields are initialized correctly
- `last_state_frame` updates when state changes
- All code compiles without warnings
- Existing tests still pass
