# Milestone 6: Migrate Engine

## Goal

Make `lp-engine` backend-agnostic by using LPVM traits instead of direct
`lpvm-cranelift` APIs. Firmware crates select the backend at the top level.

## Suggested plan name

`lpvm2-m6`

## Scope

### In scope

- Make `ShaderRuntime` generic over `LpvmEngine` (or use a type alias
  selected by the firmware crate)
- Replace `lpvm_cranelift::jit()` + `JitModule` with
  `LpvmEngine::compile()` + `LpvmModule`
- Replace `DirectCall` usage with backend-specific hot-path API (downcast
  or cfg-gated)
- Update `ProjectRuntime` to be generic over the backend
- Update `lp-engine/Cargo.toml` to depend on `lpvm` traits only (remove
  direct `lpvm-cranelift` dependency from `lp-engine`)
- Firmware crates (`fw-esp32`, `fw-emu`) wire in `lpvm-cranelift` as the
  concrete backend
- Update `lp-server` to propagate the generic backend type
- End-to-end validation: firmware builds, fw-tests, web-demo

### Out of scope

- fw-wasm implementation (future milestone)
- Abstract texture rendering at LPVM level (future)
- Removing `DirectCall` from `lpvm-cranelift` (it stays as a backend-specific
  optimization)

## Key Decisions

1. **Backend-specific hot path**: The render loop uses `DirectCall` on
   Cranelift JIT. This is accessed via backend-specific API, not through the
   trait. The trait's `call()` method is too slow for per-pixel rendering
   due to value marshaling overhead.

2. **Generics over trait objects**: `lp-engine` is generic over
   `E: LpvmEngine`. Monomorphization gives zero-cost abstraction. Each
   firmware crate instantiates the generic with its chosen backend.

3. **Engine lifetime**: The engine is created once at startup and lives for
   the lifetime of the runtime. Modules and instances are created/destroyed
   as shaders are loaded/unloaded.

## Deliverables

- Updated `lp-core/lp-engine/src/nodes/shader/runtime.rs`
- Updated `lp-core/lp-engine/src/project/runtime.rs`
- Updated `lp-core/lp-engine/Cargo.toml`
- Updated `lp-core/lp-server/` to propagate generics
- Updated firmware crates to wire in the backend
- Passing fw-tests and web-demo

## Dependencies

- Milestone 5 (filetests) — validates the LPVM trait interface works
  end-to-end before touching production code

## Estimated scope

~800–1200 lines changed. The largest milestone. `ShaderRuntime` and
`ProjectRuntime` are the main files. The generic propagation through
`lp-server` and firmware crates adds breadth.
