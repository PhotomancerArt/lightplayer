# Milestone 2: WASM Shared Memory

## Goal

Update both WASM backends (wasmtime and browser) to use engine-owned shared
memory. Instances import the engine's `Memory` instead of creating their own.

## Suggested plan name

`lpvm2-m2`

## Scope

### In scope

- Update `WasmLpvmEngine` to create and own a `wasmtime::Memory`
- Update `BrowserLpvmEngine` to create and own a `WebAssembly.Memory`
- Update `instantiate_wasm_module()` (wasmtime linker) to accept an existing
  `Memory` instead of calling `Memory::new()`
- Update `instantiate_shader()` (browser linker) to accept an existing
  `WebAssembly.Memory`
- Implement `alloc()`/`free()` as a bump allocator or free-list within the
  linear memory
- Implement `ShaderPtr` for WASM: `guest_value()` is offset into linear
  memory, `native_ptr()` is `memory_base + offset`
- Update `WasmLpvmModule::instantiate()` and `BrowserLpvmModule::instantiate()`
  to match new trait signature
- Unit tests: multiple instances sharing memory, alloc/free, ShaderPtr
  round-trip

### Out of scope

- fw-wasm shared memory model (firmware imports shader memory) — future
- Shadow stack partitioning for shared memory — future
- Memory growth strategy optimization — start simple

## Key Decisions

1. **Memory created at engine construction**: The engine creates linear memory
   with a reasonable initial size. Memory can grow via WASM's `memory.grow()`
   when `alloc()` runs out of space.

2. **Memory sizing**: The compiled module knows its memory requirements via
   `EnvMemorySpec` / `env_memory`. The engine uses these as a starting point.
   Growth happens on demand.

3. **Allocator simplicity**: Start with a bump allocator. Free-list or more
   sophisticated allocation can be added later if needed.

## Deliverables

- Updated `lp-shader/lpvm-wasm/src/rt_wasmtime/engine.rs`
- Updated `lp-shader/lpvm-wasm/src/rt_wasmtime/link.rs`
- Updated `lp-shader/lpvm-wasm/src/rt_wasmtime/instance.rs`
- Updated `lp-shader/lpvm-wasm/src/rt_browser/engine.rs`
- Updated `lp-shader/lpvm-wasm/src/rt_browser/link.rs`
- Updated `lp-shader/lpvm-wasm/src/rt_browser/instance.rs`
- New allocator module in `lpvm-wasm`
- Unit tests for shared memory across instances

## Dependencies

- Milestone 1 (trait redesign) — new trait signatures must be defined first

## Estimated scope

~400–600 lines changed. The structural change is the same in both backends
(accept memory instead of creating it), plus the allocator implementation.
