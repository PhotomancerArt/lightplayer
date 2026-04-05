# Phase 5: Cleanup and validation

## Scope

Final pass: grep for leftovers, warnings, format, broad test matrix, plan
summary, move to `plans-done`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Grep

```bash
git diff | grep -E 'TODO|FIXME|dbg!|println!|lps_cranelift|glsl_jit_streaming'
```

Remove stray references. Ensure no `lp-engine` path still imports the old crate.

### 2. Warnings

```bash
cargo clippy -p lp-engine -p lp-server -p lpir-cranelift --all-features -- -D warnings
cargo check -p lpir-cranelift --no-default-features --target riscv32imac-unknown-none-elf
```

### 3. Format

```bash
cargo +nightly fmt
```

### 4. Tests

```bash
cargo test -p lp-engine
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --features riscv32-emu
just glsl-filetests   # if available; ensures no accidental regressions
```

### 5. Plan summary

Write `docs/plans/2026-03-25-lpir-cranelift-stage-vi-b/summary.md` with:

- What shipped (DirectCall buf API, engine swap, deps removed)
- Known follow-ups (Q32 mode wiring in emitter, `max_errors` enforcement)

### 6. Move plan to `plans-done`

```bash
mv docs/plans/2026-03-25-lpir-cranelift-stage-vi-b docs/plans-done/
```

## Commit

Conventional commit, e.g.:

```
feat(lp-engine): use lpir-cranelift for shader JIT (Stage VI-B)

- Replace lps-cranelift with lpir-cranelift; drop cranelift-codegen and lps-jit-util
- ShaderRuntime stores JitModule + DirectCall; render uses call_i32_buf
- Forward optimizer/verifier features via lp-server
- Add DirectCall::call_i32_buf and invoke buffer path in lpir-cranelift
- unsafe impl Send + Sync for JitModule
```

## Validate

Full matrix from Phase 5 steps above; `fw-emu` build from Phase 4.
