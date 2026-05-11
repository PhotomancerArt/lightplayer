# Phase 5: Cleanup and validation

## Scope

Final validation pass. Grep for temporary code, fix warnings, run full test
matrix, format.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Grep the diff

```bash
git diff --stat
git diff | grep -E 'TODO|FIXME|HACK|dbg!|println!|eprintln!'
```

Remove any temporary code, debug prints, or leftover TODOs that aren't
intentional follow-up markers.

### 2. Fix warnings

```bash
cargo check -p lpvm-cranelift 2>&1 | grep warning
cargo check -p lpvm-cranelift --no-default-features --target riscv32imac-unknown-none-elf 2>&1 | grep warning
```

Fix all warnings. Pay attention to:

- Unused imports behind cfg gates
- Dead code warnings from new types not yet fully wired (Q32Options modes)
  — suppress with `#[allow(dead_code)]` only if intentional (document why)

### 3. Format

```bash
cargo +nightly fmt
```

### 4. Full test matrix

```bash
# Host with default features (std)
cargo test -p lpvm-cranelift

# Host with riscv32-emu
cargo test -p lpvm-cranelift --features riscv32-emu

# Cross-compile check (no std)
cargo check --target riscv32imac-unknown-none-elf -p lpvm-cranelift --no-default-features

# Filetests
just glsl-filetests
```

### 5. Document findings

If the `LowMemory` investigation in Phase 4 found anything notable about
`clear_context` vs `ctx.clear()`, or about per-function finalize limitations,
ensure it's captured in `00-notes.md` or `00-design.md`.

## Plan cleanup

### Summary

Add a summary to `docs/plans/2026-03-25-lpvm-cranelift-stage-vi-a/summary.md`:

- What was done
- Feature layout
- Any notable findings or deferred items

### Move to plans-done

```bash
mv docs/plans/2026-03-25-lpvm-cranelift-stage-vi-a docs/plans-done/
```

## Commit

```
feat(lpvm-cranelift): no_std support and CompileOptions expansion (Stage VI-A)

- Add default `std` Cargo feature; `--no-default-features` builds for `no_std` + `alloc`
- Add `cranelift-optimizer` and `cranelift-verifier` as opt-in features
- `riscv32-emu` now implies `std`
- ISA selection: `cranelift-native` auto-detect with `std`, explicit `riscv32imac` without
- Gate `process_sync` mutex behind `std`; no-op guard on `no_std`
- Gate `std::error::Error` impls behind `std` feature
- Add `Q32Options` (AddSubMode, MulMode, DivMode) to `CompileOptions`
- Add `MemoryStrategy` (Default / LowMemory) to `CompileOptions`
- Add `max_errors: Option<usize>` to `CompileOptions`
- LowMemory: sort functions by size (biggest first), aggressive CLIF clear after define
- Cross-compile validated: `cargo check --target riscv32imac-unknown-none-elf --no-default-features`
```

## Validate

```bash
cargo test -p lpvm-cranelift
cargo test -p lpvm-cranelift --features riscv32-emu
cargo check --target riscv32imac-unknown-none-elf -p lpvm-cranelift --no-default-features
just glsl-filetests
cargo +nightly fmt -- --check
```
