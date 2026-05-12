# Phase 5: Update client to track status_ver and log status updates

## Description

Add `status_ver: FrameId` field to `ClientNodeEntry` and update `ClientProjectView` to track status versions and log all status updates when they change.

## Success Criteria

- `ClientNodeEntry` has `status_ver: FrameId` field
- `status_ver` is updated when processing `StatusChanged` events
- Status updates are logged when `status_ver` changes
- Code compiles without errors

## Implementation Notes

- Add `status_ver: FrameId` field to `ClientNodeEntry` struct
- Initialize `status_ver` appropriately when nodes are created
- Update `status_ver` when processing `StatusChanged` events in `apply_changes()`
- Add logging when `status_ver` changes (compare old vs new `status_ver`)
- Use appropriate logging mechanism (check existing codebase for logging patterns)

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
