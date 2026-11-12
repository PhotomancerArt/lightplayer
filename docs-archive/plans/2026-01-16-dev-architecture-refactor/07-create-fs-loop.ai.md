# Phase 7: Create fs_loop

## Description

Create `fs_loop()` function that handles file watching and syncing changes to the server. This runs as a background task and monitors both file changes and transport errors.

## Tasks

1. Create `lp-app/apps/lp-cli/src/commands/dev/fs_loop.rs`:
   - Define `pub async fn fs_loop(
       client: Arc<AsyncClientTransport>,
       project_dir: PathBuf,
       project_uid: String,
   ) -> Result<()>`
   - Implementation:
     - Create `FileWatcher` for project directory
     - Create `AsyncLpClient` with shared transport
     - Create debouncing state (pending changes, last change time)
     - Loop:
       - Collect file changes from watcher (non-blocking)
       - Add to pending changes list (deduplicate by path)
       - Update last change time
       - Check if debounce period has passed
       - If yes, sync changes via `sync_changes()` function
       - Monitor error channel from transport
       - Exit on error or shutdown signal
       - Sleep briefly to avoid busy-waiting
     - Return success or error

2. Update `lp-app/apps/lp-cli/src/commands/dev/mod.rs`:
   - Add `pub mod fs_loop;`
   - Re-export `fs_loop` function

3. Check if `sync_changes()` exists in `sync.rs`:
   - If exists, use it
   - If not, create it (takes changes, client, project_uid, project_dir)

4. Add tests:
   - Test file watching and change collection
   - Test debouncing logic
   - Test syncing changes via `sync_changes()`
   - Test error channel monitoring
   - Test graceful shutdown

## Success Criteria

- `fs_loop()` function exists and compiles
- File watching works correctly
- Debouncing works correctly
- Changes are synced to server
- Error channel monitoring works
- Graceful shutdown works
- Tests pass
- Code compiles without errors

## Implementation Notes

- Use existing `FileWatcher` from `watcher.rs`
- Debounce duration: 500ms (as in current code)
- Use `tokio::select!` to monitor both file changes and error channel
- Use `tokio::time::sleep()` for debouncing and loop delays
- Consider using `tokio::signal::ctrl_c()` for shutdown signal
- Handle `TransportError::ConnectionLost` gracefully
- May need to create `sync_changes()` function if it doesn't exist
