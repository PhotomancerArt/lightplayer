# Phase 10: Remove LpfxFnId Enum and Cleanup

## Description

Remove the `LpfxFnId` enum and all its methods since everything now uses the registry.

## Implementation

### File: `frontend/semantic/lpfx/lpfx_fn_registry.rs`

Remove:
- Any `LpfxFnId` enum definition (if it exists here)
- Any remaining references to `LpfxFnId`

### File: `frontend/semantic/lpfx/mod.rs`

Remove:
- Any `LpfxFnId` re-exports

### Search entire codebase

Remove:
- All `LpfxFnId` enum definitions
- All `impl LpfxFnId` blocks
- All imports of `LpfxFnId` (should be none after previous phases)

## Success Criteria

- `LpfxFnId` enum completely removed
- No references to `LpfxFnId` anywhere
- Code compiles
- Code formatted with `cargo +nightly fmt`

## Style Notes

### Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

### Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

### Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
