# Stage VII: Cleanup

## Goal

Remove old emitter code, finalize crate structure, and ensure the codebase
is clean.

## Suggested plan name

`lpir-stage-vii`

## Scope

**In scope:**
- Delete dead code from `lps-wasm` (old emit.rs, emit_vec.rs, locals.rs
  should already be gone from Stage V)
- Remove any `#[allow(dead_code)]` or feature gates added during migration
- Update README.md files to reflect new architecture
- Verify all crates compile cleanly with no warnings
- Final CI validation

**Out of scope:**
- Vector support (separate roadmap)
- Cranelift backend migration (separate roadmap)
- LPIR optimizations (future work)

## Deliverables

- Clean codebase with no dead code from old emitter
- Updated documentation
- All tests and CI green

## Dependencies

- Stage VI must be complete (all filetests passing).

## Estimated scope

~100 lines of deletions + documentation updates.
