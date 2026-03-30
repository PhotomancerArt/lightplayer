# Phase 8: Filetest harness investigation

## Scope of phase

Reproduce and fix (or document) cases where a **`.glsl` file passes when run alone** but **fails
in the full suite** with a different pass/fail count (e.g. `uvec2/fn-equal.glsl`). Suspects:
shared mutable state, working directory, compile cache, or accounting bugs — **not** necessarily
threading (single-threaded runs may still fail).

## Code organization reminders

- Prefer deterministic, isolated runs (reset state per file if needed).
- Any global cache must be keyed by file path + target + relevant options.

## Implementation details

1. **Reproduce**

```bash
./scripts/glsl-filetests.sh uvec2/fn-equal.glsl
LP_FILETESTS_THREADS=1 ./scripts/glsl-filetests.sh 2>&1 | grep fn-equal
```

2. **Instrument** — temporary logging (remove before final commit of this phase) around compile
   cache, module reuse, and per-file stats aggregation.

3. **Fix** — e.g. clear JIT module per file, fix off-by-one in test case counting, ensure parallel
   workers do not share process-global state incorrectly.

4. If the fix is **large** or **risky**, stop after a **short write-up** in this plan folder
   (`08-harness-findings.md`) and open a follow-up plan.

## Validate

```bash
cargo test -p lp-glsl-filetests
LP_FILETESTS_THREADS=1 ./scripts/glsl-filetests.sh
./scripts/glsl-filetests.sh uvec2/fn-equal.glsl vec/uvec2/fn-equal.gen.glsl
```

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
