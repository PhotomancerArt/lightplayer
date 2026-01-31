# Phase 3: Update get_changes to include status changes for all nodes

## Description

Update `ProjectRuntime::get_changes()` to check for status changes for all nodes and include `StatusChanged` events in `node_changes` when `status_ver > since_frame`. Also remove status from `NodeDetail` construction.

## Success Criteria

- `get_changes()` checks `status_ver > since_frame` for all nodes
- `StatusChanged` events are added to `node_changes` for all nodes with status changes
- Status is removed from `NodeDetail` construction in `get_changes()`
- Code compiles without errors

## Implementation Notes

- In `get_changes()`, add a check: `if entry.status_ver.as_i64() > since_frame.as_i64()`
- Add `NodeChange::StatusChanged` to `node_changes` for nodes with status changes
- Remove `status: api_status` from `NodeDetail` construction
- Ensure status changes are included regardless of `detail_handles`

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
