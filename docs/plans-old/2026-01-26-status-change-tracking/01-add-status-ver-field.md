# Phase 1: Add status_ver field to NodeEntry

## Description

Add `status_ver: FrameId` field to `NodeEntry` in `ProjectRuntime` to track when status last changed. Initialize it appropriately when nodes are created.

## Success Criteria

- `NodeEntry` has `status_ver: FrameId` field
- `status_ver` is initialized when nodes are created (set to creation frame)
- Code compiles without errors

## Implementation Notes

- Add `status_ver: FrameId` field to `NodeEntry` struct
- Initialize `status_ver` to `self.frame_id` when creating new `NodeEntry` instances
- Update all places that create `NodeEntry` to set `status_ver`

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
