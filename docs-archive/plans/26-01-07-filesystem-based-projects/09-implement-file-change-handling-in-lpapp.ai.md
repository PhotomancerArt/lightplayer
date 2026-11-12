# Phase 9: Implement File Change Handling in LpApp

## Goal

Implement file change handling in `LpApp` to reload nodes when files change.

## Tasks

1. Implement `LpApp::handle_file_changes()` in `lp-core/src/app/lp_app.rs`:
   - Process each file change
   - Determine which node is affected (parse path to find node directory)
   - Handle each change type:
     - **Create**: Load new node from filesystem, add to runtime
     - **Modify**: Reload node (destroy old, create new)
     - **Delete**: Remove node from runtime (destroy)
   - Log file changes

2. Update `LpApp::tick()`:
   - Call `handle_file_changes()` before processing messages
   - Handle errors gracefully (log but continue)

3. Add helper functions:
   - `get_node_path_from_file_path(file_path: &str) -> Option<String>` - extract node path from file path
   - `reload_node(&mut self, node_id: &str) -> Result<(), Error>` - reload a single node

4. Handle edge cases:
   - Multiple files in same node changed (deduplicate)
   - Node directory deleted (remove node)
   - `project.json` changed (reload entire project)

5. Add tests:
   - Test file change handling with in-memory filesystem
   - Test create/modify/delete scenarios
   - Test nested file changes

## Success Criteria

- File changes are processed correctly
- Nodes are reloaded when files change
- Errors are handled gracefully
- Tests pass
- Code compiles without warnings

