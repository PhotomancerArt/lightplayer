# Phase 4: Regenerate Builtin Registry

## Goal

Run the builtin generator to auto-generate registry entries, mod.rs exports, and builtin_refs.rs for
the new add and sub builtins.

## Tasks

### 4.1 Run Builtin Generator

Execute `scripts/build-builtins.sh`:

- This will scan `lp-glsl-builtins/src/builtins/q32/` for new builtins
- Auto-generate `mod.rs` with exports for add and sub
- Auto-generate `registry.rs` with `Q32Add` and `Q32Sub` enum variants
- Auto-generate `builtin_refs.rs` with function references

### 4.2 Verify Generated Files

Check that:

- `mod.rs` includes `pub use add::__lp_q32_add;` and `pub use sub::__lp_q32_sub;`
- `registry.rs` includes `Q32Add` and `Q32Sub` in the enum
- `builtin_refs.rs` includes references to the new builtins
- All generated files compile without errors

## Success Criteria

- Builtin generator runs successfully
- All generated files updated with new builtins
- Code compiles without errors
- Registry includes `Q32Add` and `Q32Sub`

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
