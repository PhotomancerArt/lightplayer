# Phase 6: Cleanup & Validation

## Cleanup

1. **Grep the git diff for temporary code**: Search for TODO comments, debug
   `println!`/`log::debug!` statements, and any temporary implementations added
   during earlier phases.

   ```bash
   git diff --name-only | xargs grep -n "TODO\|FIXME\|HACK\|println!\|dbg!" 2>/dev/null
   ```

   Remove any that won't be addressed in later work. TODOs that reference
   `future-work.md` items can stay if they're documented there.

2. **Check for unused imports and dead code**: Fix all warnings.

   ```bash
   cd lp-shader/lp-glsl-compiler && cargo check --features std 2>&1 | grep warning
   cd lp-core/lp-engine && cargo check --features std 2>&1 | grep warning
   ```

3. **Format all changed files**:

   ```bash
   cargo +nightly fmt
   ```

4. **Verify the streaming path is used by default on no_std**: Check that the
   ESP32 callsite in `runtime.rs` calls `glsl_jit_streaming`.

## Validation

Run the full test suite:

```bash
# Compiler tests
cd lp-shader/lp-glsl-compiler && cargo test --features std

# Engine tests
cd lp-core/lp-engine && cargo test --features std

# no_std compilation check
cd lp-shader/lp-glsl-compiler && cargo check --no-default-features --features core

# Workspace-wide check
cargo check --workspace
```

## Plan Cleanup

1. Write `summary.md` in the plan directory with a brief description of what
   was implemented.

2. Move `docs/plans/2026-03-10-streaming-compilation/` to
   `docs/plans-done/2026-03-10-streaming-compilation/`.

## Commit

Commit with:

```
feat(glsl): add streaming per-function compilation pipeline

- Add glsl_jit_streaming() that compiles functions one at a time
- Each function's AST, CLIF IR, and codegen context freed before next
- Functions compiled smallest-first (by AST node count) to minimize peak
- Q32 transform applied per-function using two-module approach
- GlModule metadata (source_text, source_map, etc.) not carried into compilation
- ESP32 callsite switched to streaming path
```
