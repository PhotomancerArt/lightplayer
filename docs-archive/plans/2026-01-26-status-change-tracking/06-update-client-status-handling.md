# Phase 6: Update client to handle status only from node_changes

## Description

Update `ClientProjectView::apply_changes()` to handle status only from `node_changes` (via `StatusChanged` events) and remove any code that reads status from `node_details`.

## Success Criteria

- Status is only updated from `StatusChanged` events in `node_changes`
- No code reads status from `node_details`
- Status initialization from `Created` events works correctly
- Code compiles without errors

## Implementation Notes

- Remove any code that reads `detail.status` from `node_details` in `apply_changes()`
- Ensure `StatusChanged` events properly update `ClientNodeEntry.status` and `status_ver`
- Ensure `Created` events initialize status to `Created` (no `StatusChanged` needed for initial status)
- Update any code that relied on status from `node_details`

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
