# Phase 8: Refactor handler.rs

## Description

Refactor `handle_dev()` and `handle_dev_local()` to use the new architecture. Simplify the handler to use `client_connect()`, `AsyncClientTransport`, and the new helper functions.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/handler.rs`:
   - Simplify `handle_dev()`:
     - Validate local project
     - Parse host specifier from args
     - Call `client_connect(spec)` to get transport
     - Create `AsyncClientTransport` (takes ownership of transport)
     - Wrap in `Arc` for sharing
     - Create `AsyncLpClient` with shared transport
     - Run initial tasks: `push_project_async()` or `pull_project_async()` if needed
     - Load project: `load_project_async()`
     - Spawn `fs_loop()` task
     - If not headless, run UI (or spawn `ui_loop()` if created)
     - Wait for shutdown signal
     - Close transport explicitly
   - Remove `handle_dev_local()` and `handle_dev_websocket()` - use unified `handle_dev()`
   - Remove old sync code and transport pair creation
   - Remove `run_client_loop()` and `run_client_loop_async()` if no longer needed

2. Update imports:
   - Import `client_connect` from `crate::client`
   - Import `AsyncClientTransport` and `AsyncLpClient` from `crate::client`
   - Import `push_project_async`, `pull_project_async` from local modules
   - Import `fs_loop` from local modules
   - Remove unused imports

3. Update `load_project_async()` if needed:
   - Should use `AsyncLpClient::project_load()`
   - Move to appropriate module if not already there

## Success Criteria

- `handle_dev()` is simplified and uses new architecture
- `handle_dev_local()` and `handle_dev_websocket()` removed or unified
- Code uses `client_connect()` for transport creation
- Code uses `AsyncClientTransport` and `AsyncLpClient`
- Initial tasks (push/pull) work correctly
- File watching works correctly
- UI works correctly (if not headless)
- Code compiles without errors

## Implementation Notes

- Keep `validate_local_project()` function
- Use `tokio::select!` to wait for shutdown signal and background tasks
- Use `tokio::signal::ctrl_c()` for shutdown signal
- Ensure transport is closed explicitly before exit
- Consider using `tokio::spawn()` for `fs_loop()` task
- May need to keep some sync code temporarily for websocket path (if not refactored yet)
