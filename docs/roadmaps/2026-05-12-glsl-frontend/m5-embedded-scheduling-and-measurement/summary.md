# M5a Summary - Compile Job Budget Semantics

## What was built

- `CompileBudget::default()` now means unbounded compile work for synchronous callers.
- `CompileBudget::single_step()` still runs one coarse compiler stage.
- `CompileBudget::steps(n)` runs up to N coarse compiler stages, with `0` treated as one step.
- `CompileJob::stage()` exposes the current `CompileStage`.
- `CompileStage` is public and exported from `lps-glsl`.

## Validation

Passed:

```bash
cargo test -p lps-glsl
cargo check -p lps-glsl --target riscv32imac-unknown-none-elf
```

## Next

The next M5 slice should add timing and allocation snapshots around these coarse stages, then thread
the job through a non-production firmware/emulator scheduling path.
