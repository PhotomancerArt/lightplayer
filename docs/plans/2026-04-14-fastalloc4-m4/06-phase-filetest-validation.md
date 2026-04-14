# Phase 6: Filetest Validation + Fixes

## Scope

Run the full GLSL filetest suite, triage failures, and fix any remaining bugs
found through real shader compilation. Target: all filetests that pass on the
old backend also pass on FA.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Process

### 1. Baseline measurement

```bash
cargo test -p lps-filetests -- rv32fa 2>&1 | tail -5
```

Record: total, pass, fail, compile-fail counts.

### 2. Triage compile-fails

Compile-fails should now be near zero (all region types handled). For any
remaining compile-fails:

1. Run with `--nocapture` to get the error message.
2. Classify: is it a missing VInst, an allocator bug, or an emitter bug?
3. Fix and re-run.

### 3. Triage wrong-output failures

For shaders that compile but produce wrong output:

1. Compare FA output with old-backend output.
2. Use `shader-rv32fa --dump-alloc` to inspect allocations.
3. Use `shader-rv32fa --dump-asm` to inspect emitted assembly.
4. Common causes:
   - Missing boundary spill (value not in stack at merge point).
   - Wrong edit anchor (spill/reload in wrong position).
   - Branch offset miscalculation.
   - Missing reload after boundary (value expected in register but still in stack).

### 4. Regression check

After each fix, re-run the full suite to ensure no regressions. Track
pass-count monotonically increasing.

### 5. Add targeted filetests

For any bug found, add a minimal GLSL filetest that reproduces it, so we
don't regress.

## Validate

```bash
# Full suite
cargo test -p lps-filetests -- rv32fa
# Also run the unit filetest suite
cargo test -p lpvm-native-fa
# Check firmware builds still work
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Definition of Done

- All GLSL filetests that pass on old backend also pass on FA.
- No new compile-fails (all region types handled).
- Pass-count matches or exceeds old-backend pass-count for shared tests.
- Firmware check still passes.
