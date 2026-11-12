# Phase 7: Integrate version-based change tracking into server tick loop

## Description

Update `LpServer::tick()` to query filesystem changes from `base_fs` and notify projects. Filter changes by project path prefix and translate to project-relative paths.

## Implementation

1. Update `lp-server/src/server.rs`:
   - After processing incoming messages, query changes from `base_fs`
   - For each loaded project:
     - Get `last_fs_version` from project
     - Query `base_fs.get_changes_since(last_fs_version)`
     - Filter changes by project path prefix (e.g., `projects/test-project/`)
     - Translate parent paths to project-relative paths (strip prefix, ensure leading `/`)
     - Call `project.runtime_mut().handle_fs_changes(&project_changes)`
     - Update project's `last_fs_version` to `base_fs.current_version()`

2. Handle errors gracefully (log and continue with other projects)

3. Import `FsVersion` and `FsChange` from `lp_shared::fs`

## Success Criteria

- Server queries changes from `base_fs` in `tick()`
- Changes are filtered per project correctly
- Path translation works correctly
- Projects receive changes via `handle_fs_changes()`
- Project versions are updated after processing
- Code compiles without errors
- File changes propagate to projects correctly
