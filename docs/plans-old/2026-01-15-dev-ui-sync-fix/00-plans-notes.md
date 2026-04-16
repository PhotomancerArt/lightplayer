# Planning Notes: Dev UI Sync Fix

## Questions

### Q1: Transport Sharing Strategy

**Context:**
Currently, `AsyncLpClient` owns a `Box<dyn ClientTransport>` which is consumed when creating the client. After loading the project, we try to pass the `AsyncLpClient` to the UI, but the transport is already moved/consumed. The UI needs to sync project state, which requires access to the same transport connection.

**Suggested Answer:**
Refactor `AsyncLpClient` to accept a shared transport (`Arc<Mutex<dyn ClientTransport>>` or similar) instead of owning it. This allows both the project loader and the UI to share the same transport. Alternatively, we could wrap the transport in `Arc<Mutex<...>>` before creating `AsyncLpClient`.

**Options:**
- Option A: Refactor `AsyncLpClient::new()` to accept `Arc<Mutex<dyn ClientTransport>>` instead of `Box<dyn ClientTransport>`
- Option B: Create a wrapper that shares the transport between loader and UI
- Option C: Use a channel-based approach where a background task handles all sync requests

**Question:** Which approach do you prefer? Should we refactor `AsyncLpClient` to use shared transport, or use a different approach?

**Answer:** Option A - Refactor `AsyncLpClient` to use shared transport (`Arc<Mutex<...>>`). This makes sense because the client will also need to watch for file changes and send them to the server (future feature), so shared transport is the right approach.

---

### Q2: Sync Mechanism Implementation

**Context:**
The `handle_sync()` method in `DebugUiState` currently only updates `detail_tracking` but doesn't actually call `project_sync()`. The challenge is that `ClientProjectView` contains `Box<dyn NodeConfig>` which is not `Send`, so we can't spawn a regular async task that holds a `MutexGuard` across `.await` points.

**Suggested Answer:**
Use `tokio::task::LocalSet` with `spawn_local()` to allow non-Send futures. The sync task will run in a LocalSet, which allows holding the `MutexGuard` across await points. We'll spawn a background task that processes sync requests via a channel.

**Options:**
- Option A: Use `LocalSet` and `spawn_local()` for sync task (allows non-Send)
- Option B: Restructure `project_sync()` to not require holding lock across await (clone data, do async work, update view)
- Option C: Use a blocking sync approach (not ideal for UI responsiveness)

**Question:** Should we use `LocalSet` with `spawn_local()`, or restructure to avoid holding the lock across await?

**Answer:** Option B - Restructure to avoid holding the lock across await. The approach is:
1. Lock view, read `since_frame` and `detail_specifier`, unlock
2. Do async `project_get_changes()` call (no lock held)
3. Lock view, call `apply_changes()` with response, unlock

This is simpler than LocalSet and avoids the complexity of managing a LocalSet task.

---

### Q3: Fixture Texture Reference Resolution

**Context:**
The fixture panel currently finds any texture node instead of the one referenced by the fixture. `FixtureRuntime` already resolves `texture_handle` and `output_handle` during `init()`, but `FixtureState` doesn't include these resolved handles. The UI needs the resolved handles to display the correct texture.

**Suggested Answer:**
Add `texture_handle: Option<NodeHandle>` and `output_handle: Option<NodeHandle>` fields to `FixtureState`. Extract these from `FixtureRuntime` when creating the state in `get_changes()`. Then the UI can directly use these handles instead of trying to resolve specifiers.

**Question:** Should we add `texture_handle` and `output_handle` to `FixtureState`?

**Answer:** Yes - add resolved handles to `FixtureState`. The runtime already has them resolved, so we should include them in the state for the UI to use directly.

---

## Notes

- The sync mechanism is critical - without it, the UI won't display any data
- Transport sharing is needed for sync to work
- Fixture texture reference is a nice-to-have improvement but not blocking
