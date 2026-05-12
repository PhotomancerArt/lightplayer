# Design: Dev UI Sync Fix

## Overview

Fix three critical issues preventing the debug UI from functioning:
1. Transport sharing between project loader and UI
2. Sync mechanism implementation
3. Fixture texture reference resolution

## File Structure

```
lp-app/apps/lp-cli/src/commands/dev/
├── async_client.rs              # UPDATE: Refactor AsyncLpClient to use Arc<Mutex<Transport>>
└── handler.rs                   # UPDATE: Wrap transport in Arc<Mutex<>> before creating client

lp-app/apps/lp-cli/src/debug_ui/
├── ui.rs                        # UPDATE: Implement sync mechanism (restructured to avoid holding lock)
└── panels.rs                    # UPDATE: Use texture_handle from FixtureState

lp-app/crates/lp-model/src/nodes/fixture/
└── state.rs                     # UPDATE: Add texture_handle and output_handle fields

lp-app/crates/lp-engine/src/
├── nodes/fixture/runtime.rs     # UPDATE: Add getter methods for texture_handle and output_handle
└── project/runtime.rs           # UPDATE: Extract handles when creating FixtureState
```

## Type and Function Summary

```
AsyncLpClient                    # UPDATE: Change transport field to Arc<Mutex<dyn ClientTransport + Send>>
├── new(transport: Arc<Mutex<...>>)  # UPDATE: Accept shared transport
└── project_sync(...)            # UPDATE: Restructure to not hold view lock across await

DebugUiState                     # UPDATE: Implement sync mechanism
└── handle_sync()                # UPDATE: Actually call project_sync with proper lock management

FixtureState                     # UPDATE: Add resolved handle fields
├── texture_handle: Option<NodeHandle>  # NEW: Resolved texture handle
└── output_handle: Option<NodeHandle>   # NEW: Resolved output handle

FixtureRuntime                   # UPDATE: Add getter methods
├── get_texture_handle()         # NEW: Get resolved texture handle
└── get_output_handle()          # NEW: Get resolved output handle

ProjectRuntime::get_changes()    # UPDATE: Extract handles when creating FixtureState
```

## Design Decisions

### 1. Transport Sharing
**Decision**: Refactor `AsyncLpClient` to accept `Arc<Mutex<dyn ClientTransport + Send>>` instead of `Box<dyn ClientTransport + Send>`.

**Rationale**: 
- Allows both project loader and UI to share the same transport
- Necessary for future file watching feature
- Simple refactor with minimal impact

**Implementation**:
- Change `AsyncLpClient::new()` signature
- Update all call sites to wrap transport in `Arc<Mutex<...>>`
- Update internal methods to lock transport before use

### 2. Sync Mechanism
**Decision**: Restructure `project_sync()` to avoid holding `ClientProjectView` lock across await points.

**Rationale**:
- `ClientProjectView` contains non-Send types (`Box<dyn NodeConfig>`)
- Simpler than using `LocalSet` and `spawn_local()`
- Avoids complexity of managing LocalSet tasks

**Implementation**:
1. Lock view, read `since_frame` and `detail_specifier`, unlock
2. Do async `project_get_changes()` call (no lock held)
3. Lock view, call `apply_changes()` with response, unlock

### 3. Fixture State Handles
**Decision**: Add `texture_handle` and `output_handle` to `FixtureState`.

**Rationale**:
- Runtime already resolves these handles during `init()`
- UI needs resolved handles, not specifiers
- Avoids need to resolve specifiers in UI

**Implementation**:
- Add fields to `FixtureState`
- Add getter methods to `FixtureRuntime`
- Extract handles when creating `FixtureState` in `get_changes()`
- Update UI panel to use handles directly

## Implementation Notes

### Transport Locking
- Use `blocking_lock()` for sync operations (in UI thread)
- Use `lock().await` for async operations
- Ensure transport is `Send` for cross-thread use

### Sync Timing
- Sync happens in UI update loop (`handle_sync()`)
- Only one sync in flight at a time (enforced by `sync_in_progress` flag)
- Sync as soon as previous sync completes

### Handle Extraction
- Extract handles from `FixtureRuntime` only if runtime exists and handles are Some
- Handle case where fixture hasn't been initialized yet (handles will be None)
- UI should handle None case gracefully

## Error Handling

- Transport errors: Log and continue, UI shows last known state
- Sync errors: Log and continue, UI shows last known state
- Missing handles: UI shows "No texture available" message
- Lock errors: Should not happen, but handle gracefully

## Testing Strategy

### Manual Testing
- Verify UI displays project state after sync
- Verify sync happens automatically
- Verify fixture panel shows correct texture
- Verify transport sharing works (no connection issues)

### Unit Testing
- Test `AsyncLpClient` with shared transport
- Test sync mechanism with mock responses
- Test handle extraction from runtime

## Success Criteria

- UI displays project state from server
- Sync happens automatically in UI update loop
- Fixture panel shows correct texture (not just any texture)
- Transport is shared between loader and UI
- Code compiles without warnings
- No deadlocks or race conditions
