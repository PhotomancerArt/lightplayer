# Phase 4: Update push.rs to Async

## Description

Replace synchronous `push_project` and `load_project` functions with async versions that use `AsyncLpClient`.

## Tasks

1. Update `push_project()` to `push_project_async()`:
   - Change signature to `async fn push_project_async(client: &AsyncLpClient, local_fs: &dyn LpFs, project_uid: &str) -> Result<()>`
   - Use `client.fs_write()` for each file (await each call)
   - Remove old sync message processing logic

2. Update `load_project()` to `load_project_async()`:
   - Change signature to `async fn load_project_async(client: &AsyncLpClient, project_uid: &str) -> Result<ProjectHandle>`
   - Use `client.project_load()` (await the call)
   - Remove old sync message processing logic

3. Remove or deprecate old sync helper functions (`send_and_process`, `process_messages`) if no longer needed

## Success Criteria

- `push_project_async()` compiles and pushes files correctly
- `load_project_async()` compiles and loads project correctly
- Both functions use async client methods properly
- Code compiles without errors

## Implementation Notes

- Keep function signatures similar to sync versions for easier migration
- Use `await` for all async client method calls
- Ensure proper error propagation
