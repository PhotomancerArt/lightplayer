# Native Runtime Memory Savings Design

## Scope of work

Reduce memory use in the native runtime shader path by trimming debug-only allocations and retained metadata from the default JIT pipeline, then measure the effect with the existing ESP32 compile stress harness.

In scope:
- make runtime JIT debug retention opt-in instead of unconditional
- shrink retained `NativeJitModule` state where possible
- inspect and trim remaining transient link/JIT buffer overhead when justified by measurements
- validate with targeted on-device stress measurements

Out of scope:
- broad frontend latency slicing work
- changing shader semantics or backend architecture
- removing host/emulator/filetest debug capabilities outright

## File structure

```text
lp-shader/
  lpvm-native/
    src/
      compile.rs
      native_options.rs
      compile/
        function_job.rs
        module_job.rs
      rt_jit/
        compiler.rs
        compile_job.rs
        engine.rs
        module.rs
      rt_emu/
        engine.rs
        module.rs
      debug/
        sections.rs
      debug_asm.rs
docs/
  plans/2026-05-14-native-runtime-memory-savings/
    00-notes.md
    00-design.md
    01-runtime-debug-gating.md
    02-jit-runtime-metadata-trim.md
    03-link-buffer-measure-and-trim.md
    04-hardware-validation-and-cleanup.md
```

## Architecture summary

The native backend should support two memory profiles with the same core compiler:

1. Runtime JIT profile
   - default firmware path
   - minimizes transient and retained heap
   - does not build or keep rich debug metadata unless explicitly requested

2. Debug/host profile
   - emulator, filetests, assembly/disasm tools, deep diagnostics
   - allowed to retain line tables and structured debug sections
   - enabled explicitly through compile options or dedicated entry points

The implementation should preserve the current staged compile flow while separating runtime-essential data from debug-only payloads.

## Main components and interactions

### `NativeCompileOptions`

`NativeCompileOptions` is the control surface for memory-sensitive behavior.

It should clearly express whether the compile is:
- runtime-oriented and debug-light
- debug-oriented and allowed to retain extra metadata

This allows the same backend stages to serve both firmware and host tooling without copying the compiler.

### `CompiledFunction` / `CompiledModule`

Compiled outputs should no longer assume that debug payload is always present.

The design target is:
- runtime-essential data always present: code, relocations, symbol/link information
- debug-only data present only when explicitly requested

That separation should let function compilation finish, drop intermediates, and hand a small result to JIT link in the common runtime case.

### `rt_jit` runtime module

The JIT runtime path should retain only what runtime execution needs.

Today it keeps full `LpirModule` and `LpsModuleSig` to answer `direct_call(...)` lazily. The intended direction is to replace that with a smaller runtime summary, such as per-entry call metadata, while preserving any required trait-facing signatures behavior.

### `rt_emu` and host debug paths

The emulator and host-oriented tools remain the home for rich debug retention.

That means:
- `ModuleDebugInfo` stays meaningful there
- `debug_asm` still opts into debug info
- filetests and debug rendering do not lose observability just because firmware gets leaner

### Link/JIT buffer finalization

After debug retention and metadata trimming, we measure whether the remaining transient memory peak is meaningfully driven by duplicated code/image buffers during link and final `JitBuffer` creation.

If so, we reduce those copies in a contained follow-up rather than pre-optimizing blindly.
