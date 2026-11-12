# Phase 6: Add client-side status change logging

## Description

Add status change logging in `ClientProjectView::apply_changes()` to log console messages when node status transitions from `Ok` to `Error` or `Error` to `Ok`. This provides immediate feedback in the CLI when editing GLSL files.

## Changes

### File: `lp-engine-client/src/project/view.rs`

1. **Add status tracking**:
   - Add a field to track previous status for each node: `previous_status: BTreeMap<NodeHandle, NodeStatus>`
   - Initialize this map when creating the view

2. **Modify `apply_changes()` method**:
   - When applying `StatusChanged` change, compare with previous status
   - If transition from `Ok` to `Error` or `Error` to `Ok`, log to console
   - Format: `[{path}] Status changed: {old} -> {new}`
   - Include error message if transitioning to `Error`
   - Update `previous_status` map with new status

3. **Logging format**:
   - Use `println!` or `eprintln!` for console output
   - Format: `[{node_path}] Status changed: Ok -> Error("GLSL compilation failed: ...")`
   - Or: `[{node_path}] Status changed: Error(...) -> Ok`

## Success Criteria

- Status changes are logged to console when syncing
- Logs appear when status transitions from `Ok` to `Error` or `Error` to `Ok`
- Log messages are clear and include error details
- Code compiles without errors
- Tests pass

## Notes

- This provides immediate feedback in the CLI
- Users can see status changes without looking at the UI
- Logging happens client-side, so it works even without the UI
