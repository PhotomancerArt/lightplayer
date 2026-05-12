# Milestone 1: LPVM Trait Redesign

## Goal

Add shared memory support to the LPVM trait definitions. Introduce `ShaderPtr`
and an allocator interface on `LpvmEngine`. Update `LpvmModule::instantiate`
to accept an engine reference for memory access.

## Suggested plan name

`lpvm2-m1`

## Scope

### In scope

- Add `alloc()`, `realloc()`, `free()` methods to `LpvmEngine` trait
- Define `ShaderPtr` type in `lpvm` crate with `native_ptr()` and
  `guest_value()` methods
- Update `LpvmModule::instantiate()` signature to accept engine/memory
  reference
- Update `LpvmInstance` if needed (e.g., if instances need engine access)
- Unit tests for `ShaderPtr` construction and conversion
- Update `VmContext` documentation to clarify it stays per-instance

### Out of scope

- Backend implementations (M2–M4)
- Consumer migration (M5–M6)
- Actual texture allocation or uniform passing
- VMContext changes (no shared memory pointer in VMContext)

## Key Decisions

1. **Engine-owned memory**: `LpvmEngine` owns the shared memory and provides
   the allocator. This is because cross-module sharing (shader A's texture
   read by shader B) requires memory to outlive any single module.

2. **ShaderPtr dual-pointer model**: `ShaderPtr` wraps both a native host
   pointer and a guest-visible value. The native pointer allows Rust code to
   read/write data directly. The guest value is what gets stored in uniforms
   and used by shader `Load`/`Store` operations. These are the same value for
   Cranelift JIT (same address space), different for WASM (offset vs base+offset)
   and emulator (RAM address vs host pointer).

3. **`instantiate()` takes engine reference**: Modules need access to the
   engine's shared memory when creating instances. The engine reference
   provides this without coupling instances directly to the allocator.

4. **No VMContext changes**: Shared memory is accessed through uniforms, not
   through VMContext. VMContext stays per-instance (fuel, trap handler).

## Deliverables

- Updated `lp-shader/lpvm/src/engine.rs` — new trait methods
- Updated `lp-shader/lpvm/src/module.rs` — new `instantiate` signature
- New `lp-shader/lpvm/src/shader_ptr.rs` — `ShaderPtr` type
- Updated `lp-shader/lpvm/src/lib.rs` — re-exports
- Tests for `ShaderPtr` and trait coherence

## Dependencies

- M1–M3 of original LPVM roadmap (done)
- No external dependencies

## Estimated scope

~200–300 lines of new/changed code. Small milestone — the trait definitions
are concise. The complexity is in getting the design right, not in the
implementation.
