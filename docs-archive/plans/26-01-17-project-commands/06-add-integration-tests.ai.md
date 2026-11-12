# Phase 6: Add Integration Tests

## Goal

Create comprehensive integration tests for project commands with helper functions for clean, elegant tests.

## Tasks

1. Create `lp-client/tests/project_sync.rs`:
   - Helper: `create_test_project_on_client(fs: &mut dyn LpFs) -> String` - Creates project using `ProjectBuilder`, returns name
   - Helper: `sync_project_to_server(...)` - Writes all project files to server via FS operations
   - Helper: `load_project_on_server(...) -> ProjectHandle` - Sends LoadProject request, processes messages, returns handle
   - Helper: `verify_project_loaded(server: &LpServer, handle: ProjectHandle) -> bool` - Checks if project is loaded
   - Helper: `verify_project_running(server: &mut LpServer, handle: ProjectHandle) -> Result<(), Error>` - Checks if project can tick

2. Test cases:
   - `test_project_load_unload()` - Load project, verify loaded, unload, verify unloaded
   - `test_project_list_operations()` - List available, load, list loaded
   - `test_project_lifecycle()` - Create on client, sync to server, load, verify running, unload
   - `test_project_get_changes()` - Load project, send GetChanges request, verify response

3. Reuse `process_messages()` helper from `fs_sync.rs` if possible

## Success Criteria

- [ ] All helper functions exist and work correctly
- [ ] All test cases pass
- [ ] Tests are clean and elegant (similar to `scene_render.rs`)
- [ ] Code compiles without warnings
