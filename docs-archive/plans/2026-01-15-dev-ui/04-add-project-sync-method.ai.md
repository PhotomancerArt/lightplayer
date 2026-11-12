# Phase 4: Add project_sync method to AsyncLpClient

## Goal

Add `project_sync` method to `AsyncLpClient` that syncs a `ClientProjectView` with the server.

## Tasks

1. Update `lp-app/apps/lp-cli/src/commands/dev/async_client.rs`:
   - Add `project_sync` method:
     ```rust
     pub async fn project_sync(
         &mut self,
         handle: ProjectHandle,
         view: &mut ClientProjectView,
     ) -> Result<()>
     ```
   - Implementation:
     - Get `since_frame` from view (view.frame_id)
     - Get `detail_specifier` from view (view.detail_specifier())
     - Call `project_get_changes` with handle, since_frame, detail_specifier
     - Wait for response
     - Call `view.apply_changes()` with response
     - Update view.frame_id to current_frame from response

2. Add dependency on `lp-engine-client` if not already present:
   - Add to `Cargo.toml` dependencies
   - Import `ClientProjectView` from `lp_engine_client::project::view`

3. Handle errors appropriately:
   - Return error if sync fails
   - Log errors for debugging

## Success Criteria

- `project_sync` method exists and works correctly
- Sync updates `ClientProjectView` with server state
- Detail specifier is generated from view's detail_tracking
- Frame ID is updated after sync
- Code compiles without warnings
- All existing tests pass

## Implementation Notes

- Use `view.detail_specifier()` to generate the specifier for the request
- After `apply_changes()`, update `view.frame_id` to the `current_frame` from response
- This method will be called from the UI sync loop
