# Streaming Memory Optimization

## Context

After direct-Q32 (Plans A–D), the streaming path peak dropped from 228,258
to 219,125 bytes (−9,133). The transform overhead is gone, but several
optimization opportunities remain from the deferred streaming-improvements2
work and new findings from the trace.

## Trace comparison (baseline → direct-Q32 streaming)

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| Peak used | 228,258 | 219,125 | −9,133 (−4.0%) |
| Allocs at peak | 1,750 | 2,154 | +404 |

## Top consumers at peak (current)

| Bytes | Allocs | Source |
|------:|-------:|--------|
| 26,313 | 296 | `glsl_jit_streaming` (overhead: vecs, hashmaps, strings) |
| 22,128 | 90 | `ChunkedVec<T>::push` (cranelift internal) |
| 20,408 | 52 | `constructor_emit_side_effect` (cranelift codegen) |
| 19,404 | 132 | `fastalloc::run` (register allocator) |
| 16,790 | 398 | `JITModule::declare_function` |
| 14,512 | 8 | `Riscv32Backend::compile_function` |
| 12,082 | 363 | `T::clone_one` (AST data alive at peak) |
| 7,248 | 28 | `Lower<I>::finish_ir_inst` (cranelift lowering) |
| 6,984 | 10 | `AllocJitMemoryProvider::allocate_readexec` |

Cranelift internals (~84 KB) are outside our control. The actionable items
are `glsl_jit_streaming`, `JITModule::declare_function`, and `T::clone_one`.

## Deferred items status (from streaming-improvements2)

- **Phase 1 (Lightweight Float Declarations)**: **Moot** — direct-Q32
  eliminated the float module entirely.
- **Phase 3 (Drop AST Before Peak)**: **Still relevant** — 12 KB of AST
  heap data alive during `define_function`.

## Plan

| Phase | Target | Expected savings |
|-------|--------|-----------------|
| 1. Format-aware builtin declaration | `JITModule::declare_function` | ~4 KB |
| 2. Release AST borrow at peak | `T::clone_one` | ~5–12 KB |
| 3. Reduce streaming overhead | `glsl_jit_streaming` | ~3–5 KB |
