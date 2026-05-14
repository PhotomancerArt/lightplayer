# Phase 3: Link Buffer Measure and Trim

## Scope of phase

Measure the remaining transient link/JIT-image overhead after debug and metadata trimming, then remove obvious duplicate copies if the measurements justify it.

In scope:
- inspect the `compile -> link -> JitBuffer` handoff
- instrument or reason carefully about transient code-buffer duplication
- trim a concrete remaining copy or retained temporary when it produces measurable savings

Out of scope:
- speculative redesign without a measured target
- deep linker architecture changes unless a small local change is clearly justified

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
- `lp-shader/lpvm-native/src/rt_jit/compiler.rs`
- `lp-shader/lpvm-native/src/rt_jit/buffer.rs`
- `lp-shader/lpvm-native/src/link.rs`
- `lp-shader/lpvm-native/src/compile/module_job.rs`

Expected changes:
- Verify whether the remaining peak is materially affected by code/image duplication.
- Only make a buffer-flow change if there is a clear, bounded memory win.

Constraints and edge cases:
- Do not compromise correctness of reloc resolution or profiler symbol emission.
- Keep the incremental compile flow and one-shot wrapper behavior aligned.

## Validate

Run:

```bash
cargo test -p lpvm-native
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features test_shader_compile_incremental,esp32c6
```
