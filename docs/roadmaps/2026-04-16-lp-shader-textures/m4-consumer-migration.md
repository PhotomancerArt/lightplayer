# M4 — Consumer Migration

## Goal

Migrate lpfx-cpu and lp-engine to use `lp-shader`'s `compile_frag` /
`FragInstance::render_frame` instead of their own pixel loops and compile
pipelines, and **switch host execution from the in-process Cranelift JIT
(`lpvm-cranelift`) to Wasmtime** (`lpvm-wasm` / `WasmLpvmEngine`, surfaced
through `lp-shader`). Today both consumers compile and run shaders via
Cranelift JIT paths; M4 is not only an API migration but a **host-backend
migration** for anything that executed GLSL-on-CPU through those stacks.

Wasmtime is a heavier dependency than the old JIT-only stack; that trade
is acceptable for **deterministic per-instance isolation** (avoiding
long-standing multi-`JITModule` state issues in the same process) and for
**32-bit guest pointers on the host**, matching RV32, the emulator, and
browser targets (no 64-bit-host-pointer special case in the render path).

## Deliverables

### lpfx-cpu migration

Replace:
- `lpfx-cpu/src/compile.rs` (ad-hoc compile_glsl + validate_inputs)
- `lpfx-cpu/src/render_cranelift.rs` (hand-rolled pixel loop)

With:
- `LpsShaderEngine::compile_frag` for compilation
- `FragInstance::render_frame` (or `render_frame_fast`) for rendering
- lpfx generates the bootstrap wrapper for `render()` -> `void main()`
- **Host execution via Wasmtime** (through `lp-shader`), not `lpvm-cranelift`

`CpuFxInstance` holds a `FragInstance` (Wasmtime-backed) instead of raw
`CraneliftState`.

### lp-engine migration

Replace:
- `lp-engine/src/gfx/cranelift.rs` (`render_direct_call` pixel loop)
- `lp-engine/src/gfx/native_jit.rs` (equivalent pixel loop)

With:
- `LpShader::render` delegates to `FragInstance::render_frame`
- Shader compilation and host CPU execution routed through **`lp-shader`**
  with **`WasmLpvmEngine`** (or an equivalent thin wrapper), not
  `CraneliftGraphics` + `lpvm-cranelift`

**Retire:** `lp-engine/src/gfx/cranelift.rs` and
`lp-engine/src/gfx/native_jit.rs` as the live host paths once migration
is complete. Replacement is Wasmtime-backed via `lp-shader` (exact module
layout TBD during implementation).

The `LpShader` trait may need adjustment or `FragInstance` implements it
directly.

### Retire duplicated texture types

- `lpfx::CpuTexture` -> use `lps-shared::CpuTextureBuffer`
- `lp-shared::Texture` (lp-core) -> use `lps-shared::CpuTextureBuffer`
  (or implement `TextureBuffer` on it as an adapter)

### Update noise.fx

The noise.fx example shader switches from `render(fragCoord, outputSize,
time)` to the standard fragment contract:

```glsl
uniform float time;
out vec4 fragColor;
void main() {
    // ... existing noise code using gl_FragCoord.xy, outputSize, time ...
    fragColor = vec4(col * tv.y, 1.0);
}
```

Or lpfx auto-generates the bootstrap wrapper and noise.fx stays as-is.

## Validation

```bash
cargo test -p lpfx -p lpfx-cpu
cargo test -p lp-engine
cargo check
# Verify: noise.fx still renders correctly
# Verify: lp-engine shaders still render correctly
```

## Dependencies

- M2 (render_frame exists and works)
- M1 (fragment shader contract)
