# Phase 1: Add Handle and Frame Tracking to ProjectRuntime

## Goal

Add handle assignment and frame tracking infrastructure to `ProjectRuntime`.

## Tasks

1. Add fields to `ProjectRuntime`:
   - `current_frame: FrameId` - current frame counter
   - `next_handle: i32` - next handle ID to assign (starts at 0)

2. Update `ProjectRuntime::new()`:
   - Initialize `current_frame` to `FrameId(0)`
   - Initialize `next_handle` to 0

3. Add new methods to `ProjectRuntime`:
   - `get_current_frame(&self) -> FrameId` - returns current frame ID
   - `get_all_node_handles(&self) -> Vec<NodeHandle>` - returns all node handles (for all node types)
   - `get_engine_stats(&self) -> EngineStats` - returns engine statistics (frame timing, memory usage)
   - `get_changed_nodes_since(&self, since_frame: FrameId) -> Vec<NodeHandle>` - returns handles of nodes where `min(last_config_frame, last_state_frame) > since_frame` OR `created_frame > since_frame`
   - `get_node_detail(&self, handle: NodeHandle) -> Option<NodeDetail>` - returns full node detail by handle

4. Update `ProjectRuntime::update()`:
   - Increment `current_frame` at the start of each update cycle

5. Add helper method `assign_next_handle(&mut self) -> NodeHandle`:
   - Returns current `next_handle` and increments it

## Success Criteria

- `ProjectRuntime` has `current_frame` and `next_handle` fields
- `get_current_frame()` returns the current frame ID
- `update()` increments frame ID each cycle
- `assign_next_handle()` returns sequential handles
- All code compiles without warnings
- Existing tests still pass
