# Phase 6: Create push_project_async and pull_project_async

## Description

Create async versions of project sync operations that use `AsyncLpClient` instead of sync `LpClient`. These functions handle reading/writing project files to/from the server.

## Tasks

1. Create `lp-app/apps/lp-cli/src/commands/dev/push_project.rs`:
   - Define `pub async fn push_project_async(
       client: &mut AsyncLpClient,
       local_fs: &dyn LpFs,
       project_uid: &str,
   ) -> Result<()>`
   - Implementation:
     - List all files in project directory recursively
     - For each file:
       - Read file from local filesystem
       - Call `client.fs_write()` to write to server
     - Handle errors appropriately
     - Return success or error

2. Create `lp-app/apps/lp-cli/src/commands/dev/pull_project.rs`:
   - Define `pub async fn pull_project_async(
       client: &mut AsyncLpClient,
       local_fs: &dyn LpFs,
       project_uid: &str,
   ) -> Result<()>`
   - Implementation:
     - List all files in project on server (need `fs_list_dir` method)
     - For each file:
       - Call `client.fs_read()` to read from server
       - Write file to local filesystem
     - Handle errors appropriately
     - Return success or error

3. Update `lp-app/apps/lp-cli/src/commands/dev/mod.rs`:
   - Add `pub mod push_project;`
   - Add `pub mod pull_project;`
   - Re-export functions

4. Add tests:
   - Test `push_project_async` with mock `AsyncLpClient` and `LpFs`
   - Test `pull_project_async` with mock `AsyncLpClient` and `LpFs`
   - Test error handling (file read errors, transport errors)
   - Test with multiple files

## Success Criteria

- `push_project_async()` function exists and compiles
- `pull_project_async()` function exists and compiles
- Both functions work correctly with async client
- Error handling works correctly
- Tests pass
- Code compiles without errors

## Implementation Notes

- May need to add `fs_list_dir` method to `AsyncLpClient` if not already present
- Use `LpFs::list_dir()` for local filesystem operations
- Handle recursive directory traversal
- Consider adding progress reporting (optional)
- Handle file paths correctly (relative to project root)
