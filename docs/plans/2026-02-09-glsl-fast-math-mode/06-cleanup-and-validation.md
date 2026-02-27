# Phase 6: Cleanup and Validation

## Scope of phase

Final cleanup, fix warnings, run full validation, add plan summary, and prepare for commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Cleanup

1. Grep the git diff for:
   - `TODO` comments
   - `dbg!`, `println!` debug statements
   - Unused imports or variables

2. Remove any temporary code or fix TODOs that were left for this phase.

### Validation

Run full check and test suite:

```bash
cargo fmt
just check
just test
```

Fix any clippy warnings, formatting issues, or test failures.

### Update other GlslOptions construction sites

Ensure all places that construct `GlslOptions` include `fast_math`:
- `lp-glsl-compiler` examples, tests
- `lp-glsl-filetests` if it constructs GlslOptions
- `esp32-glsl-jit` if it uses GlslOptions directly (it may use a different path)

### Plan cleanup

Add summary to `docs/plans/2026-02-09-glsl-fast-math-mode/summary.md`:

```markdown
# GLSL Fast Math Mode - Completed

## Summary

Implemented fast math mode for q32 fixed-point add/sub: when enabled, emits inline `iadd`/`isub` (wrapping) instead of saturating builtin calls. Reduces call overhead for shaders that can tolerate overflow.

## Changes

- lp-model: GlslOpts struct, ShaderConfig.glsl_opts
- lp-glsl-compiler: GlslOptions.fast_math, Q32Transform.fast_math, conditional iadd/isub in converters
- lp-engine: ShaderRuntime uses glsl_opts for GlslOptions
- fw-esp32: Demo project rainbow.shader uses fast_math
```

Move plan files to `docs/plans-done/` (per plan process - do this when commit is done).

## Commit

Once complete, commit with:

```
feat(glsl): add fast math mode for q32 add/sub

- Add GlslOpts with fast_math to ShaderConfig (lp-model)
- Add fast_math to GlslOptions and Q32Transform
- Emit iadd/isub inline when fast_math, else saturating builtins
- ShaderRuntime reads glsl_opts for compilation
- Enable fast_math in esp32 demo project
```
