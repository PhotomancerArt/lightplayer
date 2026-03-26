# Phase 4: Cleanup & Validation

## Scope

Final validation, cleanup of temporary code, fix all warnings, format, commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Cleanup & validation

### Grep for remnants

Search the git diff and codebase for any remaining old-convention references:

```
rg '__lp_q32_' lp-glsl/     # should only appear in stale lp-glsl-cranelift
rg '__lpfx_' lp-glsl/       # same — only stale old cranelift
rg 'std\.math' lp-glsl/     # should be zero outside old cranelift
rg 'StdMathHandler' lp-glsl/  # should be zero (renamed)
rg 'LpQ32' lp-glsl/         # old variant names, should be zero outside old cranelift
rg 'Lpfx[A-Z]' lp-glsl/     # old lpfx variant names (LpfxFbm not LpLpfxFbm)
```

Any hits outside `lp-glsl-cranelift/` need fixing.

### Check for TODOs

```
rg 'TODO' lp-glsl/ --glob '!lp-glsl-cranelift/**'
```

Remove any temporary TODOs added during this plan.

### Fix warnings

```
cargo clippy -p lp-glsl-builtin-ids -p lp-glsl-builtins -p lp-glsl-builtins-gen-app \
  -p lp-glsl-naga -p lp-glsl-wasm -p lpir -- --no-deps -D warnings
```

Fix any warnings introduced by the rename (unused imports, dead code, etc.).
Warnings in `lp-glsl-cranelift` are expected and ignored.

### Format

```
cargo +nightly fmt --all
```

### Full test suite

```
cargo test -p lp-glsl-builtin-ids -p lp-glsl-builtins -p lp-glsl-builtins-gen-app \
  -p lp-glsl-naga -p lp-glsl-wasm -p lpir
just test-glsl-filetests
```

All tests should pass except those in `lp-glsl-cranelift` (accepted breakage).

### Verify the web demo still works

```
just web-demo
```

Quick manual check that the demo compiles and renders.

## Plan cleanup

Add a summary of completed work to
`docs/plans/2026-03-24-lpir-cranelift-stage-i/summary.md`.

Move plan files to `docs/plans-done/2026-03-24-lpir-cranelift-stage-i/`.

## Commit

```
refactor(builtins): rename all builtins to __lp_<module>_<fn>_<mode> convention

- Rename 29 Q32 math builtins: 6 to lpir module (fadd, fsub, fmul, fdiv,
  fsqrt, fnearest), 23 to glsl module (sin, cos, pow, etc.)
- Rename 67 LPFX builtins: __lpfx_ prefix → __lp_lpfx_
- Make BuiltinId self-describing: add module(), fn_name(), mode() methods
- Add Module and Mode enums to lp-glsl-builtin-ids
- Split LPIR import module "std.math" into "glsl" and "lpir"
- Update generator: remove old cranelift outputs, add module/mode parsing
- Update WASM emitter import resolution for new module names
- Rename StdMathHandler → BuiltinImportHandler
- Update all test references
```
