# Phase 2: JIT Runtime Metadata Trim

## Scope of phase

Reduce retained post-compile memory in `NativeJitModule` by trimming large metadata structures from the default runtime path.

In scope:
- identify why `NativeJitModuleInner` retains full IR and signature metadata
- replace that retention with smaller per-entry runtime summaries where feasible
- preserve required module APIs and instantiation behavior

Out of scope:
- changing public product semantics
- broad API churn outside the native JIT module path
- link/buffer-copy work unless it becomes inseparable from the metadata trim

## Code organization reminders

- Prefer granular files with one main concept per file
- Keep related functionality grouped together
- Put helpers lower in the file when that improves readability
- Mark any temporary code with a clear `TODO`

## Sub-agent reminders

- Do not commit
- Do not expand scope
- Do not suppress warnings or weaken tests to get green builds
- If blocked, stop and report instead of improvising
- Report what changed, what was validated, and any deviations

## Implementation details

Relevant files and symbols:
- `lp-shader/lpvm-native/src/rt_jit/module.rs`
- `lp-shader/lpvm-native/src/rt_jit/engine.rs`
- `lp-shader/lpvm-native/src/rt_jit/compile_job.rs`
- `lp-shader/lpvm-native/src/abi/func_abi.rs`
- `lp-shader/lp-shader/src/px_shader.rs` (only if needed for integration context)

Expected changes:
- Introduce a compact retained entry summary for runtime direct calls.
- Remove unnecessary dependence on full retained IR in `direct_call(...)` if possible.
- Keep `signatures()` working correctly.

Constraints and edge cases:
- Avoid broad trait/API churn if a contained runtime-summary approach works.
- Preserve entry offset lookup and ABI correctness.

## Validate

Run:

```bash
cargo test -p lpvm-native
cargo test -p lp-shader
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```
