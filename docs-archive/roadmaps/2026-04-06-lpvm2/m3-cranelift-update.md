# Milestone 3: Cranelift JIT Update

## Goal

Update the Cranelift JIT backend's LPVM trait implementations to match the
new trait signatures from M1. Validate the trait design works for the
native JIT backend.

## Suggested plan name

`lpvm2-m3`

## Scope

### In scope

- Update `CraneliftEngine` to implement new `alloc()`/`free()`/`realloc()`
methods
- Update `CraneliftModule::instantiate()` to accept engine reference
- Update `CraneliftInstance` if needed
- Implement `ShaderPtr` for Cranelift JIT: `native_ptr()` == `guest_value()`
(same address space, host memory)
- Cranelift's `alloc()` is a thin wrapper around host allocation
- Unit tests validating the new trait interface works with JIT compilation

### Out of scope

- Engine integration (M6)
- DirectCall changes (stays backend-specific)
- Any changes to the old `JitModule` API (coexists until M7)

## Key Decisions

1. **Host allocation for shared memory**: For Cranelift JIT, `alloc()` is
  essentially `alloc::alloc::alloc()` or `Vec::with_capacity()`. The native
   pointer IS the guest pointer. `ShaderPtr` for JIT is trivially a single
   pointer.
2. **Coexistence with old API**: The `JitModule` / `jit()` / `DirectCall`
  API continues to work alongside the new trait implementations. No
   breaking changes to existing consumers.

## Deliverables

- Updated `lp-shader/lpvm-cranelift/src/lpvm_engine.rs`
- Updated `lp-shader/lpvm-cranelift/src/lpvm_module.rs`
- Updated `lp-shader/lpvm-cranelift/src/lpvm_instance.rs`
- Unit tests for alloc/free and new instantiate path

## Dependencies

- Milestone 1 (trait redesign)
- Milestone 2 (WASM) is NOT a dependency — M2 and M3 could run in parallel

## Estimated scope

~100–200 lines changed. Small milestone — Cranelift JIT's memory model is
trivial (host memory). The main work is updating the signatures and adding
the thin allocator wrapper.