# Phase 2: Update status changes to track status_ver

## Description

Update all places in `ProjectRuntime` that modify `entry.status` to also update `status_ver` to the current frame ID. This includes initialization, rendering errors, and filesystem change handling.

## Success Criteria

- All places that set `entry.status` also update `status_ver`
- `status_ver` is set to `self.frame_id` when status changes
- Error message changes (e.g., `Error("msg1")` -> `Error("msg2")`) update `status_ver`
- Code compiles without errors

## Implementation Notes

- Find all places that set `entry.status` using grep
- Update each location to also set `entry.status_ver = self.frame_id`
- Ensure status changes are detected even when the variant stays the same (e.g., different error messages)

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
