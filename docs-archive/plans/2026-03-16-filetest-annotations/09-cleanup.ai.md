# Phase 9: Cleanup & Validation

## Scope

Final cleanup, grep for leftovers, fix warnings, validate everything
compiles and passes.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Grep for leftovers

Search the diff and codebase for:

```bash
# Old format remnants in test files
grep -r '// target' lp-shader/lps-filetests/filetests/ --include='*.glsl'
grep -r '\[expect-fail\]' lp-shader/lps-filetests/filetests/ --include='*.glsl'

# Old type references in code
grep -r 'FiletestTarget' lp-shader/lps-filetests/src/
grep -r 'expect_fail' lp-shader/lps-filetests/src/
grep -r 'DecimalFormat' lp-shader/lps-filetests/src/

# TODOs introduced during this work
grep -r 'TODO' lp-shader/lps-filetests/src/ --include='*.rs'

# Debug prints
grep -r 'println!' lp-shader/lps-filetests/src/ --include='*.rs' | grep -v 'eprintln\|format_'
grep -r 'dbg!' lp-shader/lps-filetests/src/ --include='*.rs'
```

Remove any temporary code, unused imports, dead code.

### Fix warnings

```bash
cargo build -p lps-filetests 2>&1 | grep warning
cargo build -p lps-filetests-app 2>&1 | grep warning
cargo build -p lps-filetests-gen-app 2>&1 | grep warning
```

Fix all warnings.

### Format

```bash
cargo +nightly fmt
```

### Full validation

```bash
# Build everything
cargo build

# Run all Rust tests
cargo test

# Run all filetests (both targets)
scripts/filetests.sh

# Run cranelift only
scripts/filetests.sh --target cranelift.q32

# Run wasm only
scripts/filetests.sh --target wasm.q32

# Format check
cargo +nightly fmt -- --check

# Clippy
cargo clippy -p lps-filetests -p lps-filetests-app -p lps-filetests-gen-app

# Verify ESP32 firmware still builds
just build-fw-esp32
```

### Verify the wasm/int-add.glsl test was moved/removed

The `filetests/wasm/` directory should no longer exist (or be empty).

## Plan Cleanup

### Summary

Add a summary of the completed work to
`docs/plans/2026-03-16-filetest-annotations/summary.md`.

### Move plan files

Move the plan directory to `docs/plans-done/`.

## Commit

Commit with:

```
refactor(filetests): replace target/expect-fail with annotation system

- Add Target, TargetFilter, Annotation types with axis enums
  (Backend, Isa, ExecMode, FloatMode)
- Add @unimplemented/@broken/@ignore annotation parser
- Update runner for multi-target dispatch (cranelift.q32 + wasm.q32)
- Add --target CLI flag for target filtering
- Migrate all test files: remove // target, convert [expect-fail]
- Update gen-app templates and regenerate .gen.glsl files
- Update file_update.rs for annotation-based fix/bless mode
```
