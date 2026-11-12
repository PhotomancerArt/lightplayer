# Phase 1: Runtime Debug Gating

## Scope of phase

Make the default native JIT runtime path debug-light so firmware compilation does not unconditionally build or retain rich debug metadata.

In scope:
- make `CompiledFunction` debug payload optional or otherwise runtime-gated
- stop unconditional `FunctionDebugInfo` / `ModuleDebugInfo` construction on the runtime JIT path
- preserve explicit host/debug opt-in behavior for emulator, debug asm, and similar tools

Out of scope:
- trimming retained runtime module metadata
- link/buffer copy restructuring
- frontend latency work

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
- `lp-shader/lpvm-native/src/native_options.rs`
- `lp-shader/lpvm-native/src/compile.rs`
- `lp-shader/lpvm-native/src/rt_jit/compiler.rs`
- `lp-shader/lpvm-native/src/rt_jit/engine.rs`
- `lp-shader/lpvm-native/src/rt_jit/compile_job.rs`
- `lp-shader/lpvm-native/src/rt_emu/engine.rs`
- `lp-shader/lpvm-native/src/debug_asm.rs`

Expected changes:
- Ensure the default runtime JIT path does not build structured debug sections unless explicitly requested.
- Ensure `link_compiled_module_jit(...)` does not build/clone module debug info in the runtime-default case.
- Keep emulator/debug tools able to opt into debug data explicitly.
- Update tests to cover both debug-light and debug-enabled compilation where useful.

Constraints and edge cases:
- Do not break host/emulator filetest diagnostics.
- Do not remove line-table/debug-section support from `debug_asm`.
- Preserve the on-device native JIT product path.

## Validate

Run:

```bash
cargo test -p lpvm-native
cargo check -p lp-shader
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features test_shader_compile_incremental,esp32c6
```
